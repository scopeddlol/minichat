use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::push;
use crate::state::AppState;

/// The VAPID public key the browser needs to create a subscription. Public by
/// design — it's the half that identifies the server, not the half that signs.
pub async fn public_key(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "enabled": state.config.push_ready(),
        "public_key": state.config.vapid_public_key,
    }))
}

#[derive(Deserialize)]
pub struct SubscribeInput {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
    #[serde(default)]
    pub user_agent: String,
}

pub async fn subscribe(
    State(state): State<AppState>,
    auth: Auth,
    Json(input): Json<SubscribeInput>,
) -> AppResult<Json<Value>> {
    if !state.config.push_ready() {
        return Err(AppError::bad(
            "Push notifications aren't configured on this instance.",
        ));
    }
    if input.endpoint.is_empty() || input.p256dh.is_empty() || input.auth.is_empty() {
        return Err(AppError::bad("That subscription is incomplete."));
    }
    push::subscribe(
        &state,
        auth.id(),
        &input.endpoint,
        &input.p256dh,
        &input.auth,
        &input.user_agent,
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct UnsubscribeInput {
    pub endpoint: String,
}

pub async fn unsubscribe(
    State(state): State<AppState>,
    _auth: Auth,
    Json(input): Json<UnsubscribeInput>,
) -> AppResult<Json<Value>> {
    push::unsubscribe(&state, &input.endpoint).await?;
    Ok(Json(json!({ "ok": true })))
}

/// Send a notification to the caller's own devices, so someone can confirm
/// notifications work without waiting for a message.
pub async fn test(State(state): State<AppState>, auth: Auth) -> AppResult<Json<Value>> {
    if !state.config.push_ready() {
        return Err(AppError::bad(
            "Push notifications aren't configured on this instance.",
        ));
    }
    let instance = crate::access::instance(&state).await?;
    push::dispatch(
        &state,
        vec![auth.user.id.clone()],
        push::PushPayload {
            title: instance.name,
            body: "Notifications are working.".into(),
            icon: instance.icon_url,
            tag: "minichat-test".into(),
            channel_id: String::new(),
            message_id: String::new(),
        },
    );
    Ok(Json(json!({ "ok": true })))
}

fn valid_mode(mode: &str) -> bool {
    matches!(mode, "all" | "mentions" | "none")
}

#[derive(Deserialize)]
pub struct PreferencesInput {
    /// Instance-wide default.
    pub mode: Option<String>,
    /// Per-channel override; `None` for mode removes the override.
    pub channel_id: Option<String>,
    pub channel_mode: Option<Option<String>>,
}

pub async fn update_preferences(
    State(state): State<AppState>,
    auth: Auth,
    Json(input): Json<PreferencesInput>,
) -> AppResult<Json<Value>> {
    if let Some(mode) = &input.mode {
        if !valid_mode(mode) {
            return Err(AppError::bad("Unknown notification setting."));
        }
        sqlx::query("UPDATE users SET notification_mode = ? WHERE id = ?")
            .bind(mode)
            .bind(auth.id())
            .execute(&state.db)
            .await?;
    }

    if let (Some(channel_id), Some(channel_mode)) = (&input.channel_id, &input.channel_mode) {
        match channel_mode {
            Some(mode) => {
                if !valid_mode(mode) {
                    return Err(AppError::bad("Unknown notification setting."));
                }
                sqlx::query(
                    "INSERT INTO channel_notifications (user_id, channel_id, mode) VALUES (?, ?, ?)
                     ON CONFLICT(user_id, channel_id) DO UPDATE SET mode = excluded.mode",
                )
                .bind(auth.id())
                .bind(channel_id)
                .bind(mode)
                .execute(&state.db)
                .await?;
            }
            None => {
                sqlx::query(
                    "DELETE FROM channel_notifications WHERE user_id = ? AND channel_id = ?",
                )
                .bind(auth.id())
                .bind(channel_id)
                .execute(&state.db)
                .await?;
            }
        }
    }

    Ok(Json(json!(preferences_payload(&state, auth.id()).await?)))
}

pub async fn get_preferences(State(state): State<AppState>, auth: Auth) -> AppResult<Json<Value>> {
    Ok(Json(preferences_payload(&state, auth.id()).await?))
}

pub async fn preferences_payload(state: &AppState, user_id: &str) -> AppResult<Value> {
    let mode: String = sqlx::query_scalar("SELECT notification_mode FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
    let overrides: Vec<(String, String)> =
        sqlx::query_as("SELECT channel_id, mode FROM channel_notifications WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(&state.db)
            .await?;

    Ok(json!({
        "mode": mode,
        "channels": overrides
            .into_iter()
            .map(|(channel_id, mode)| json!({ "channel_id": channel_id, "mode": mode }))
            .collect::<Vec<_>>(),
    }))
}
