//! Permission resolution and row hydration shared by every route.

use std::collections::HashMap;

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::ids;
use crate::models::*;
use crate::perms;
use crate::state::AppState;

/// Effective permissions for a user inside one channel: their role permissions,
/// then channel overwrites applied deny-before-allow.
pub async fn channel_permissions(
    state: &AppState,
    auth: &Auth,
    channel_id: &str,
) -> AppResult<i64> {
    if perms::has(auth.permissions, perms::ADMINISTRATOR) {
        return Ok(perms::ALL);
    }
    let mut bits = auth.permissions;

    let overwrites: Vec<(String, i64, i64)> =
        sqlx::query_as("SELECT role_id, allow, deny FROM channel_overwrites WHERE channel_id = ?")
            .bind(channel_id)
            .fetch_all(&state.db)
            .await?;

    let mut allow = 0i64;
    let mut deny = 0i64;
    for (role_id, a, d) in overwrites {
        if auth.role_ids.iter().any(|r| r == &role_id) {
            allow |= a;
            deny |= d;
        }
    }
    bits &= !deny;
    bits |= allow;
    Ok(bits)
}

pub async fn require_channel_perm(
    state: &AppState,
    auth: &Auth,
    channel_id: &str,
    perm: i64,
) -> AppResult<i64> {
    let bits = channel_permissions(state, auth, channel_id).await?;
    if !perms::has(bits, perms::VIEW_CHANNELS) {
        // Hide the channel's existence rather than confirming it.
        return Err(AppError::not_found("Channel not found."));
    }
    if !perms::has(bits, perm) {
        return Err(AppError::forbidden(
            "You don't have permission to do that in this channel.",
        ));
    }
    Ok(bits)
}

/// The viewer's effective permission bits for every channel they can see.
/// The client uses this to decide what to render; the server still enforces.
pub async fn channel_permission_map(
    state: &AppState,
    auth: &Auth,
) -> AppResult<HashMap<String, i64>> {
    let channel_ids: Vec<String> = sqlx::query_scalar("SELECT id FROM channels")
        .fetch_all(&state.db)
        .await?;

    if perms::has(auth.permissions, perms::ADMINISTRATOR) {
        return Ok(channel_ids.into_iter().map(|id| (id, perms::ALL)).collect());
    }

    let overwrites: Vec<(String, String, i64, i64)> =
        sqlx::query_as("SELECT channel_id, role_id, allow, deny FROM channel_overwrites")
            .fetch_all(&state.db)
            .await?;

    let mut by_channel: HashMap<String, (i64, i64)> = HashMap::new();
    for (channel_id, role_id, allow, deny) in overwrites {
        if auth.role_ids.iter().any(|r| r == &role_id) {
            let entry = by_channel.entry(channel_id).or_insert((0, 0));
            entry.0 |= allow;
            entry.1 |= deny;
        }
    }

    let mut map = HashMap::new();
    for id in channel_ids {
        let (allow, deny) = by_channel.get(&id).copied().unwrap_or((0, 0));
        let bits = (auth.permissions & !deny) | allow;
        if perms::has(bits, perms::VIEW_CHANNELS) {
            map.insert(id, bits);
        }
    }
    Ok(map)
}

pub async fn get_channel(state: &AppState, channel_id: &str) -> AppResult<Channel> {
    sqlx::query_as::<_, Channel>("SELECT * FROM channels WHERE id = ?")
        .bind(channel_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("Channel not found."))
}

pub async fn visible_categories(state: &AppState, auth: &Auth) -> AppResult<Vec<Category>> {
    let categories: Vec<Category> =
        sqlx::query_as("SELECT * FROM categories ORDER BY position, name")
            .fetch_all(&state.db)
            .await?;
    if auth.can(perms::ADMINISTRATOR) {
        return Ok(categories);
    }
    let overwrites: Vec<(String, String, i64, i64)> =
        sqlx::query_as("SELECT category_id,role_id,allow,deny FROM category_overwrites")
            .fetch_all(&state.db)
            .await?;
    Ok(categories
        .into_iter()
        .filter(|category| {
            if !category.is_private {
                return true;
            }
            let (allow, deny) = overwrites
                .iter()
                .filter(|(id, role, _, _)| id == &category.id && auth.role_ids.contains(role))
                .fold((0, 0), |(a, d), (_, _, next_a, next_d)| {
                    (a | next_a, d | next_d)
                });
            perms::has((auth.permissions & !deny) | allow, perms::VIEW_CHANNELS)
        })
        .collect())
}

/// Channel IDs a set of roles can view. Used by the gateway to filter events
/// without re-running permission logic per message.
pub async fn visible_channel_ids(
    state: &AppState,
    role_ids: &[String],
    is_admin: bool,
) -> AppResult<Vec<String>> {
    let channels: Vec<(String, bool)> = sqlx::query_as("SELECT id, is_private FROM channels")
        .fetch_all(&state.db)
        .await?;
    if is_admin {
        return Ok(channels.into_iter().map(|(id, _)| id).collect());
    }

    let overwrites: Vec<(String, String, i64, i64)> =
        sqlx::query_as("SELECT channel_id, role_id, allow, deny FROM channel_overwrites")
            .fetch_all(&state.db)
            .await?;

    let mut by_channel: HashMap<String, (i64, i64)> = HashMap::new();
    for (channel_id, role_id, allow, deny) in overwrites {
        if role_ids.iter().any(|r| r == &role_id) {
            let entry = by_channel.entry(channel_id).or_insert((0, 0));
            entry.0 |= allow;
            entry.1 |= deny;
        }
    }

    // Bitwise OR of every role the user holds (SUM would double-count
    // overlapping bits, so fold in Rust rather than in SQL).
    let base_bits = if role_ids.is_empty() {
        0
    } else {
        let placeholders = vec!["?"; role_ids.len()].join(",");
        let sql = format!("SELECT permissions FROM roles WHERE id IN ({placeholders})");
        let mut q = sqlx::query_scalar::<_, i64>(&sql);
        for r in role_ids {
            q = q.bind(r);
        }
        q.fetch_all(&state.db)
            .await?
            .into_iter()
            .fold(0i64, |a, b| a | b)
    };

    let mut visible = Vec::new();
    for (id, _private) in channels {
        let (allow, deny) = by_channel.get(&id).copied().unwrap_or((0, 0));
        let bits = (base_bits & !deny) | allow;
        if perms::has(bits, perms::VIEW_CHANNELS) {
            visible.push(id);
        }
    }
    Ok(visible)
}

/// Turn message rows into full API messages, batching the author, attachment
/// and reaction lookups so a 50-message page stays at a handful of queries.
pub async fn hydrate_messages(
    state: &AppState,
    rows: Vec<MessageRow>,
    viewer_id: &str,
) -> AppResult<Vec<Message>> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let placeholders = vec!["?"; ids.len()].join(",");

    // Attachments
    let mut attachments: HashMap<String, Vec<Attachment>> = HashMap::new();
    {
        let sql = format!(
            "SELECT message_id, id, filename, content_type, size, width, height, url
             FROM attachments WHERE message_id IN ({placeholders})"
        );
        let mut q = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                i64,
                Option<i64>,
                Option<i64>,
                String,
            ),
        >(&sql);
        for id in &ids {
            q = q.bind(id);
        }
        for (mid, id, filename, content_type, size, width, height, url) in
            q.fetch_all(&state.db).await?
        {
            attachments.entry(mid).or_default().push(Attachment {
                id,
                filename,
                content_type,
                size,
                width,
                height,
                url,
            });
        }
    }

    // Reactions, grouped by emoji with a "did I react" flag for the viewer.
    let mut reactions: HashMap<String, Vec<ReactionGroup>> = HashMap::new();
    {
        let sql = format!(
            "SELECT message_id, emoji, COUNT(*) as count,
                    SUM(CASE WHEN user_id = ? THEN 1 ELSE 0 END) as mine
             FROM reactions WHERE message_id IN ({placeholders})
             GROUP BY message_id, emoji ORDER BY emoji"
        );
        let mut q = sqlx::query_as::<_, (String, String, i64, i64)>(&sql).bind(viewer_id);
        for id in &ids {
            q = q.bind(id);
        }
        for (mid, emoji, count, mine) in q.fetch_all(&state.db).await? {
            reactions.entry(mid).or_default().push(ReactionGroup {
                emoji,
                count,
                me: mine > 0,
            });
        }
    }

    // Authors
    let author_ids: Vec<String> = rows.iter().filter_map(|r| r.author_id.clone()).collect();
    let mut authors: HashMap<String, MessageAuthor> = HashMap::new();
    if !author_ids.is_empty() {
        let ph = vec!["?"; author_ids.len()].join(",");
        let sql = format!(
            "SELECT id, username, display_name, avatar_url, accent_color, is_operator
             FROM users WHERE id IN ({ph})"
        );
        let mut q =
            sqlx::query_as::<_, (String, String, String, Option<String>, String, bool)>(&sql);
        for id in &author_ids {
            q = q.bind(id);
        }
        for (id, username, display_name, avatar_url, accent_color, is_operator) in
            q.fetch_all(&state.db).await?
        {
            authors.insert(
                id.clone(),
                MessageAuthor {
                    id,
                    username,
                    display_name,
                    avatar_url,
                    accent_color,
                    is_operator,
                    roles: Vec::new(),
                },
            );
        }
        let sql = format!("SELECT user_id, role_id FROM user_roles WHERE user_id IN ({ph})");
        let mut q = sqlx::query_as::<_, (String, String)>(&sql);
        for id in &author_ids {
            q = q.bind(id);
        }
        for (user_id, role_id) in q.fetch_all(&state.db).await? {
            if let Some(a) = authors.get_mut(&user_id) {
                a.roles.push(role_id);
            }
        }
    }

    Ok(rows
        .into_iter()
        .map(|r| Message {
            author: r.author_id.as_ref().and_then(|id| authors.get(id).cloned()),
            attachments: attachments.remove(&r.id).unwrap_or_default(),
            reactions: reactions.remove(&r.id).unwrap_or_default(),
            id: r.id,
            channel_id: r.channel_id,
            content: r.content,
            reply_to_id: r.reply_to_id,
            system_kind: r.system_kind,
            webhook_name: r.webhook_name,
            pinned: r.pinned,
            created_at: r.created_at,
            edited_at: r.edited_at,
        })
        .collect())
}

pub async fn hydrate_message(
    state: &AppState,
    row: MessageRow,
    viewer_id: &str,
) -> AppResult<Message> {
    let mut out = hydrate_messages(state, vec![row], viewer_id).await?;
    out.pop()
        .ok_or_else(|| AppError::Internal("message vanished during hydration".into()))
}

pub async fn audit(
    state: &AppState,
    actor_id: Option<&str>,
    action: &str,
    target_type: &str,
    target_id: &str,
    detail: &str,
) {
    let entry_id = ids::new_id();
    let result = sqlx::query(
        "INSERT INTO audit_log (id, actor_id, action, target_type, target_id, detail)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(actor_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(detail)
    .execute(&state.db)
    .await;
    match result {
        // Push the entry straight to anyone with the audit log open.
        Ok(_) => {
            let actor_name: Option<String> = match actor_id {
                Some(id) => sqlx::query_scalar("SELECT display_name FROM users WHERE id = ?")
                    .bind(id)
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten(),
                None => None,
            };
            state.emit(crate::state::Event::new(
                "AUDIT_ENTRY",
                serde_json::json!({
                    "id": entry_id,
                    "actor_id": actor_id,
                    "actor_name": actor_name,
                    "action": action,
                    "target_type": target_type,
                    "target_id": target_id,
                    "detail": detail,
                    "created_at": chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                }),
                crate::state::Scope::Permission(perms::VIEW_AUDIT_LOG),
            ));
        }
        // Never fail the caller's action because the audit write failed.
        Err(e) => tracing::error!("failed to write audit entry {action}: {e}"),
    }
}

pub async fn instance(state: &AppState) -> AppResult<Instance> {
    Ok(
        sqlx::query_as::<_, Instance>("SELECT * FROM instance WHERE id = 1")
            .fetch_one(&state.db)
            .await?,
    )
}

/// Every member who can currently view a channel. Used by push dispatch, which
/// must never notify someone about a channel they can't open.
pub async fn channel_viewers(
    state: &AppState,
    channel_id: &str,
) -> Result<std::collections::HashSet<String>, sqlx::Error> {
    let overwrites: Vec<(String, i64, i64)> =
        sqlx::query_as("SELECT role_id, allow, deny FROM channel_overwrites WHERE channel_id = ?")
            .bind(channel_id)
            .fetch_all(&state.db)
            .await?;

    let members: Vec<(String, bool)> = sqlx::query_as("SELECT id, is_operator FROM users")
        .fetch_all(&state.db)
        .await?;
    let role_pairs: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT ur.user_id, r.id, r.permissions FROM user_roles ur JOIN roles r ON r.id = ur.role_id",
    )
    .fetch_all(&state.db)
    .await?;

    let mut viewers = std::collections::HashSet::new();
    for (user_id, is_operator) in members {
        if is_operator {
            viewers.insert(user_id);
            continue;
        }
        let mut bits = 0i64;
        let mut allow = 0i64;
        let mut deny = 0i64;
        for (uid, role_id, permissions) in &role_pairs {
            if uid != &user_id {
                continue;
            }
            bits |= permissions;
            if let Some((_, a, d)) = overwrites.iter().find(|(rid, _, _)| rid == role_id) {
                allow |= a;
                deny |= d;
            }
        }
        let effective = (bits & !deny) | allow;
        if perms::has(effective, perms::VIEW_CHANNELS) {
            viewers.insert(user_id);
        }
    }
    Ok(viewers)
}
