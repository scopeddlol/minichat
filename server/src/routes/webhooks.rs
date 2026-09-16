//! Incoming webhooks: the integration surface. External services POST to a
//! per-webhook URL and the message appears in a channel.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::access;
use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::ids;
use crate::models::{MessageRow, Webhook};
use crate::perms;
use crate::routes::messages::MAX_MESSAGE_LEN;
use crate::state::AppState;
use crate::validate;

pub async fn list_webhooks(State(state): State<AppState>, auth: Auth) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_WEBHOOKS)?;
    let hooks: Vec<Webhook> = sqlx::query_as("SELECT * FROM webhooks ORDER BY created_at DESC")
        .fetch_all(&state.db)
        .await?;

    let public_url = state.config.public_url.clone();
    Ok(Json(json!(hooks
        .into_iter()
        .map(|h| {
            let url = format!("{public_url}/webhooks/{}/{}", h.id, h.token);
            let mut value = serde_json::to_value(&h).unwrap_or(Value::Null);
            if let Some(obj) = value.as_object_mut() {
                obj.insert("url".into(), json!(url));
            }
            value
        })
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
pub struct CreateWebhookInput {
    pub channel_id: String,
    pub name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

pub async fn create_webhook(
    State(state): State<AppState>,
    auth: Auth,
    Json(input): Json<CreateWebhookInput>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_WEBHOOKS)?;
    let channel = access::get_channel(&state, &input.channel_id).await?;
    if channel.kind == "voice" {
        return Err(AppError::bad("Webhooks can't post to voice channels."));
    }

    let name = validate::text(&input.name, "Webhook name", 1, 48)?;
    let avatar_url = match &input.avatar_url {
        Some(url) => validate::safe_url(url, "Avatar")?,
        None => None,
    };

    let id = ids::new_id();
    let token = ids::token(32);
    sqlx::query(
        "INSERT INTO webhooks (id, channel_id, name, token, avatar_url, created_by)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.channel_id)
    .bind(&name)
    .bind(&token)
    .bind(&avatar_url)
    .bind(auth.id())
    .execute(&state.db)
    .await?;

    access::audit(
        &state,
        Some(auth.id()),
        "webhook.create",
        "webhook",
        &id,
        &name,
    )
    .await;
    Ok(Json(json!({
        "id": id,
        "channel_id": input.channel_id,
        "name": name,
        "token": token,
        "avatar_url": avatar_url,
        "url": format!("{}/webhooks/{}/{}", state.config.public_url, id, token),
    })))
}

pub async fn delete_webhook(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_WEBHOOKS)?;
    sqlx::query("DELETE FROM webhooks WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;
    access::audit(
        &state,
        Some(auth.id()),
        "webhook.delete",
        "webhook",
        &id,
        "",
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ExecuteInput {
    #[serde(default)]
    pub content: String,
    /// Accepted as an alias so Slack-style senders work unchanged.
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

/// Public endpoint authenticated by the webhook's own token.
pub async fn execute(
    State(state): State<AppState>,
    Path((id, token)): Path<(String, String)>,
    Json(input): Json<ExecuteInput>,
) -> AppResult<Json<Value>> {
    let hook: Option<Webhook> = sqlx::query_as("SELECT * FROM webhooks WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?;
    let Some(hook) = hook else {
        return Err(AppError::not_found("Unknown webhook."));
    };
    if hook.token != token {
        return Err(AppError::not_found("Unknown webhook."));
    }

    let content = input
        .text
        .as_deref()
        .unwrap_or(&input.content)
        .trim()
        .chars()
        .take(MAX_MESSAGE_LEN)
        .collect::<String>();
    if content.is_empty() {
        return Err(AppError::bad("Webhook message was empty."));
    }

    // The sender may override the display name per message, which is how most
    // CI and alerting integrations expect to work.
    let display_name = input
        .username
        .as_deref()
        .map(|n| n.trim().chars().take(48).collect::<String>())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| hook.name.clone());

    let message_id = ids::new_id();
    sqlx::query(
        "INSERT INTO messages (id, channel_id, author_id, content, webhook_name)
         VALUES (?, ?, NULL, ?, ?)",
    )
    .bind(&message_id)
    .bind(&hook.channel_id)
    .bind(&content)
    .bind(&display_name)
    .execute(&state.db)
    .await?;

    let row: MessageRow = sqlx::query_as("SELECT * FROM messages WHERE id = ?")
        .bind(&message_id)
        .fetch_one(&state.db)
        .await?;
    let channel_id = row.channel_id.clone();
    let message = access::hydrate_message(&state, row, "").await?;
    state.emit_channel(
        &channel_id,
        "MESSAGE_CREATE",
        serde_json::to_value(&message).unwrap_or(Value::Null),
    );

    Ok(Json(json!({ "ok": true, "id": message_id })))
}
