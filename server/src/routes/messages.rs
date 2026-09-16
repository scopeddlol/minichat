use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::access;
use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::ids;
use crate::mentions;
use crate::models::MessageRow;
use crate::perms;
use crate::push;
use crate::state::AppState;
use crate::validate;

pub const MAX_MESSAGE_LEN: usize = 4000;

#[derive(Deserialize)]
pub struct ListQuery {
    /// Fetch messages older than this ID (cursor pagination).
    pub before: Option<String>,
    /// Fetch messages newer than this ID.
    pub after: Option<String>,
    /// Fetch a window centred on this ID — used when jumping to a message
    /// that isn't in the currently loaded page.
    pub around: Option<String>,
    pub limit: Option<i64>,
}

pub async fn list_messages(
    State(state): State<AppState>,
    auth: Auth,
    Path(channel_id): Path<String>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<Value>> {
    access::require_channel_perm(&state, &auth, &channel_id, perms::VIEW_CHANNELS).await?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);

    // A window centred on one message: half before, half after, plus the
    // target itself.
    if let Some(around) = &query.around {
        let half = (limit / 2).max(1);
        let mut rows: Vec<MessageRow> = sqlx::query_as(
            "SELECT * FROM messages WHERE channel_id = ? AND id <= ? ORDER BY id DESC LIMIT ?",
        )
        .bind(&channel_id)
        .bind(around)
        .bind(half + 1)
        .fetch_all(&state.db)
        .await?;
        rows.reverse();

        let newer: Vec<MessageRow> = sqlx::query_as(
            "SELECT * FROM messages WHERE channel_id = ? AND id > ? ORDER BY id ASC LIMIT ?",
        )
        .bind(&channel_id)
        .bind(around)
        .bind(half)
        .fetch_all(&state.db)
        .await?;
        rows.extend(newer);

        return Ok(Json(json!(
            access::hydrate_messages(&state, rows, auth.id()).await?
        )));
    }

    let rows: Vec<MessageRow> =
        match (&query.before, &query.after) {
            (Some(before), _) => sqlx::query_as(
                "SELECT * FROM messages WHERE channel_id = ? AND id < ? ORDER BY id DESC LIMIT ?",
            )
            .bind(&channel_id)
            .bind(before)
            .bind(limit)
            .fetch_all(&state.db)
            .await?,
            (None, Some(after)) => sqlx::query_as(
                "SELECT * FROM messages WHERE channel_id = ? AND id > ? ORDER BY id ASC LIMIT ?",
            )
            .bind(&channel_id)
            .bind(after)
            .bind(limit)
            .fetch_all(&state.db)
            .await?,
            _ => {
                sqlx::query_as(
                    "SELECT * FROM messages WHERE channel_id = ? ORDER BY id DESC LIMIT ?",
                )
                .bind(&channel_id)
                .bind(limit)
                .fetch_all(&state.db)
                .await?
            }
        };

    // Always hand the client oldest-first so it can append without sorting.
    let mut rows = rows;
    if query.after.is_none() {
        rows.reverse();
    }
    let messages = access::hydrate_messages(&state, rows, auth.id()).await?;
    Ok(Json(json!(messages)))
}

#[derive(Deserialize)]
pub struct AttachmentInput {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub size: i64,
    #[serde(default)]
    pub width: Option<i64>,
    #[serde(default)]
    pub height: Option<i64>,
    pub url: String,
}

#[derive(Deserialize)]
pub struct CreateMessageInput {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub reply_to_id: Option<String>,
    #[serde(default)]
    pub attachments: Vec<AttachmentInput>,
}

pub async fn create_message(
    State(state): State<AppState>,
    auth: Auth,
    Path(channel_id): Path<String>,
    Json(input): Json<CreateMessageInput>,
) -> AppResult<Json<Value>> {
    let bits =
        access::require_channel_perm(&state, &auth, &channel_id, perms::SEND_MESSAGES).await?;
    let channel = access::get_channel(&state, &channel_id).await?;
    if channel.kind == "voice" {
        return Err(AppError::bad("You can't post messages in a voice channel."));
    }

    let content = input.content.trim().to_string();
    if content.chars().count() > MAX_MESSAGE_LEN {
        return Err(AppError::bad(format!(
            "Messages are limited to {MAX_MESSAGE_LEN} characters."
        )));
    }
    if content.is_empty() && input.attachments.is_empty() {
        return Err(AppError::bad("Your message is empty."));
    }
    if !input.attachments.is_empty() && !perms::has(bits, perms::ATTACH_FILES) {
        return Err(AppError::forbidden(
            "You don't have permission to attach files here.",
        ));
    }

    // Slowmode, skipped for anyone who can moderate the channel.
    if channel.slowmode > 0 && !perms::has(bits, perms::MANAGE_MESSAGES) {
        // System messages (a join announcement, for instance) are attributed
        // to the member but aren't something they chose to post, so they must
        // not start the member's own slow-mode clock.
        let seconds: Option<i64> = sqlx::query_scalar(
            "SELECT CAST((julianday('now') - julianday(created_at)) * 86400 AS INTEGER)
             FROM messages
             WHERE channel_id = ? AND author_id = ? AND system_kind IS NULL
             ORDER BY id DESC LIMIT 1",
        )
        .bind(&channel_id)
        .bind(auth.id())
        .fetch_optional(&state.db)
        .await?
        .flatten();
        if let Some(elapsed) = seconds {
            if elapsed < channel.slowmode {
                let wait = channel.slowmode - elapsed;
                return Err(AppError::bad(format!(
                    "Slow mode is on — try again in {wait}s."
                )));
            }
        }
    }

    if let Some(reply_to) = &input.reply_to_id {
        let exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM messages WHERE id = ? AND channel_id = ?")
                .bind(reply_to)
                .bind(&channel_id)
                .fetch_optional(&state.db)
                .await?;
        if exists.is_none() {
            return Err(AppError::bad("The message you're replying to is gone."));
        }
    }

    let message_id = ids::new_id();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO messages (id, channel_id, author_id, content, reply_to_id)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&message_id)
    .bind(&channel_id)
    .bind(auth.id())
    .bind(&content)
    .bind(&input.reply_to_id)
    .execute(&mut *tx)
    .await?;

    for attachment in input.attachments.iter().take(10) {
        // Only accept URLs this server issued, so a message can't embed an
        // arbitrary remote resource as an "attachment".
        if !attachment.url.starts_with("/uploads/") {
            return Err(AppError::bad("Attachments must be uploaded first."));
        }
        sqlx::query(
            "INSERT INTO attachments (id, message_id, filename, content_type, size, width, height, url)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&attachment.id)
        .bind(&message_id)
        .bind(&attachment.filename)
        .bind(&attachment.content_type)
        .bind(attachment.size)
        .bind(attachment.width)
        .bind(attachment.height)
        .bind(&attachment.url)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    let resolved =
        mentions::resolve(&state, &content, perms::has(bits, perms::MENTION_EVERYONE)).await?;
    mentions::store(&state, &message_id, &channel_id, Some(auth.id()), &resolved).await?;

    let row: MessageRow = sqlx::query_as("SELECT * FROM messages WHERE id = ?")
        .bind(&message_id)
        .fetch_one(&state.db)
        .await?;
    let message = access::hydrate_message(&state, row, auth.id()).await?;

    state.emit_channel(
        &channel_id,
        "MESSAGE_CREATE",
        serde_json::to_value(&message).unwrap_or(Value::Null),
    );

    // Tell mentioned members even if they aren't looking at the channel.
    for user_id in &resolved.user_ids {
        if user_id == auth.id() {
            continue;
        }
        state.emit_user(
            user_id,
            "MENTION_ADD",
            json!({ "channel_id": channel_id, "message_id": message_id }),
        );
    }

    notify_new_message(
        &state,
        &channel_id,
        &channel.name,
        &auth,
        &message_id,
        &content,
        input.attachments.len(),
        &resolved,
    )
    .await;

    Ok(Json(json!(message)))
}

/// Work out who wants a push for this message and hand it to the dispatcher.
#[allow(clippy::too_many_arguments)]
async fn notify_new_message(
    state: &AppState,
    channel_id: &str,
    channel_name: &str,
    auth: &Auth,
    message_id: &str,
    content: &str,
    attachment_count: usize,
    resolved: &mentions::Resolved,
) {
    let recipients = match push::recipients_for_message(
        state,
        channel_id,
        Some(auth.id()),
        &resolved.user_ids,
        resolved.everyone,
    )
    .await
    {
        Ok(recipients) => recipients,
        Err(e) => {
            tracing::warn!("could not resolve push recipients: {e}");
            return;
        }
    };

    push::dispatch(
        state,
        recipients,
        push::PushPayload {
            title: format!("{} in #{}", auth.user.display_name, channel_name),
            body: push::payload_preview(content, attachment_count),
            icon: auth.user.avatar_url.clone(),
            tag: channel_id.to_string(),
            channel_id: channel_id.to_string(),
            message_id: message_id.to_string(),
        },
    );
}

#[derive(Deserialize)]
pub struct EditMessageInput {
    pub content: String,
}

pub async fn edit_message(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(input): Json<EditMessageInput>,
) -> AppResult<Json<Value>> {
    let row: MessageRow = sqlx::query_as("SELECT * FROM messages WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("That message no longer exists."))?;

    access::require_channel_perm(&state, &auth, &row.channel_id, perms::VIEW_CHANNELS).await?;
    // Editing is always the author's own act — moderators delete, they don't
    // rewrite what someone said.
    if row.author_id.as_deref() != Some(auth.id()) {
        return Err(AppError::forbidden("You can only edit your own messages."));
    }

    let content = input.content.trim().to_string();
    if content.is_empty() {
        return Err(AppError::bad("Message can't be empty — delete it instead."));
    }
    if content.chars().count() > MAX_MESSAGE_LEN {
        return Err(AppError::bad(format!(
            "Messages are limited to {MAX_MESSAGE_LEN} characters."
        )));
    }

    sqlx::query("UPDATE messages SET content = ?, edited_at = datetime('now') WHERE id = ?")
        .bind(&content)
        .bind(&id)
        .execute(&state.db)
        .await?;

    // Editing a mention out of a message should clear the ping too.
    let bits = access::channel_permissions(&state, &auth, &row.channel_id).await?;
    let resolved =
        mentions::resolve(&state, &content, perms::has(bits, perms::MENTION_EVERYONE)).await?;
    mentions::store(&state, &id, &row.channel_id, Some(auth.id()), &resolved).await?;

    let row: MessageRow = sqlx::query_as("SELECT * FROM messages WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    let channel_id = row.channel_id.clone();
    let message = access::hydrate_message(&state, row, auth.id()).await?;
    state.emit_channel(
        &channel_id,
        "MESSAGE_UPDATE",
        serde_json::to_value(&message).unwrap_or(Value::Null),
    );
    Ok(Json(json!(message)))
}

pub async fn delete_message(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let row: MessageRow = sqlx::query_as("SELECT * FROM messages WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("That message no longer exists."))?;

    let bits =
        access::require_channel_perm(&state, &auth, &row.channel_id, perms::VIEW_CHANNELS).await?;
    let is_author = row.author_id.as_deref() == Some(auth.id());
    if !is_author && !perms::has(bits, perms::MANAGE_MESSAGES) {
        return Err(AppError::forbidden(
            "You don't have permission to delete that message.",
        ));
    }

    sqlx::query("DELETE FROM messages WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    if !is_author {
        access::audit(
            &state,
            Some(auth.id()),
            "message.delete",
            "message",
            &id,
            "Deleted another member's message",
        )
        .await;
    }

    state.emit_channel(
        &row.channel_id,
        "MESSAGE_DELETE",
        json!({ "id": id, "channel_id": row.channel_id }),
    );
    Ok(Json(json!({ "ok": true })))
}

async fn set_pinned(
    state: &AppState,
    auth: &Auth,
    id: &str,
    pinned: bool,
) -> AppResult<Json<Value>> {
    let row: MessageRow = sqlx::query_as("SELECT * FROM messages WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("That message no longer exists."))?;

    access::require_channel_perm(state, auth, &row.channel_id, perms::PIN_MESSAGES).await?;
    sqlx::query("UPDATE messages SET pinned = ? WHERE id = ?")
        .bind(pinned)
        .bind(id)
        .execute(&state.db)
        .await?;

    let row: MessageRow = sqlx::query_as("SELECT * FROM messages WHERE id = ?")
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    let channel_id = row.channel_id.clone();
    let message = access::hydrate_message(state, row, auth.id()).await?;
    state.emit_channel(
        &channel_id,
        "MESSAGE_UPDATE",
        serde_json::to_value(&message).unwrap_or(Value::Null),
    );
    Ok(Json(json!(message)))
}

pub async fn pin(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    set_pinned(&state, &auth, &id, true).await
}

pub async fn unpin(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    set_pinned(&state, &auth, &id, false).await
}

pub async fn list_pins(
    State(state): State<AppState>,
    auth: Auth,
    Path(channel_id): Path<String>,
) -> AppResult<Json<Value>> {
    access::require_channel_perm(&state, &auth, &channel_id, perms::VIEW_CHANNELS).await?;
    let rows: Vec<MessageRow> = sqlx::query_as(
        "SELECT * FROM messages WHERE channel_id = ? AND pinned = 1 ORDER BY id DESC LIMIT 50",
    )
    .bind(&channel_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!(
        access::hydrate_messages(&state, rows, auth.id()).await?
    )))
}

pub async fn add_reaction(
    State(state): State<AppState>,
    auth: Auth,
    Path((id, emoji)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let emoji = validate::emoji(&emoji)?;
    if emoji.starts_with(':') {
        let name = emoji.trim_matches(':');
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM emojis WHERE name = ?")
            .bind(name)
            .fetch_optional(&state.db)
            .await?;
        if exists.is_none() {
            return Err(AppError::bad("That emoji no longer exists."));
        }
    }
    let channel_id: String = sqlx::query_scalar("SELECT channel_id FROM messages WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("That message no longer exists."))?;

    access::require_channel_perm(&state, &auth, &channel_id, perms::ADD_REACTIONS).await?;
    sqlx::query("INSERT OR IGNORE INTO reactions (message_id, user_id, emoji) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(auth.id())
        .bind(&emoji)
        .execute(&state.db)
        .await?;

    state.emit_channel(
        &channel_id,
        "REACTION_UPDATE",
        json!({
            "message_id": id,
            "channel_id": channel_id,
            "emoji": emoji,
            "user_id": auth.id(),
            "added": true,
        }),
    );
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove_reaction(
    State(state): State<AppState>,
    auth: Auth,
    Path((id, emoji)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let channel_id: String = sqlx::query_scalar("SELECT channel_id FROM messages WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("That message no longer exists."))?;

    sqlx::query("DELETE FROM reactions WHERE message_id = ? AND user_id = ? AND emoji = ?")
        .bind(&id)
        .bind(auth.id())
        .bind(&emoji)
        .execute(&state.db)
        .await?;

    state.emit_channel(
        &channel_id,
        "REACTION_UPDATE",
        json!({
            "message_id": id,
            "channel_id": channel_id,
            "emoji": emoji,
            "user_id": auth.id(),
            "added": false,
        }),
    );
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct AckInput {
    pub message_id: String,
}

pub async fn ack(
    State(state): State<AppState>,
    auth: Auth,
    Path(channel_id): Path<String>,
    Json(input): Json<AckInput>,
) -> AppResult<Json<Value>> {
    sqlx::query(
        "INSERT INTO read_state (user_id, channel_id, last_read_id) VALUES (?, ?, ?)
         ON CONFLICT(user_id, channel_id) DO UPDATE SET last_read_id = excluded.last_read_id",
    )
    .bind(auth.id())
    .bind(&channel_id)
    .bind(&input.message_id)
    .execute(&state.db)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub author_id: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn search(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<SearchQuery>,
) -> AppResult<Json<Value>> {
    let needle = query.q.trim();
    if needle.len() < 2 {
        return Err(AppError::bad("Search for at least 2 characters."));
    }
    let limit = query.limit.unwrap_or(40).clamp(1, 100);

    let is_admin = auth.can(perms::ADMINISTRATOR);
    let visible = access::visible_channel_ids(&state, &auth.role_ids, is_admin).await?;
    let visible: Vec<String> = match &query.channel_id {
        Some(channel_id) => visible.into_iter().filter(|c| c == channel_id).collect(),
        None => visible,
    };
    if visible.is_empty() {
        return Ok(Json(json!([])));
    }

    let placeholders = vec!["?"; visible.len()].join(",");
    let author_clause = if query.author_id.is_some() {
        " AND author_id = ?"
    } else {
        ""
    };
    let sql = format!(
        "SELECT * FROM messages
         WHERE channel_id IN ({placeholders}) AND content LIKE ? ESCAPE '\\'{author_clause}
         ORDER BY id DESC LIMIT ?"
    );

    let mut q = sqlx::query_as::<_, MessageRow>(&sql);
    for channel_id in &visible {
        q = q.bind(channel_id);
    }
    let escaped = needle
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    q = q.bind(format!("%{escaped}%"));
    if let Some(author_id) = &query.author_id {
        q = q.bind(author_id);
    }
    q = q.bind(limit);

    let rows = q.fetch_all(&state.db).await?;
    Ok(Json(json!(
        access::hydrate_messages(&state, rows, auth.id()).await?
    )))
}
