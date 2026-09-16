use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::access;
use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::livekit::{mint_token, Grant};
use crate::perms;
use crate::state::AppState;

/// Mint a LiveKit token scoped to one voice channel. Publish rights mirror the
/// caller's channel permissions, so a muted role can listen but not speak.
pub async fn token(
    State(state): State<AppState>,
    auth: Auth,
    Path(channel_id): Path<String>,
) -> AppResult<Json<Value>> {
    if !state.config.livekit_ready() {
        return Err(AppError::bad(
            "Voice and video aren't configured on this instance yet.",
        ));
    }

    let bits = access::require_channel_perm(&state, &auth, &channel_id, perms::CONNECT).await?;
    let channel = access::get_channel(&state, &channel_id).await?;
    if channel.kind != "voice" {
        return Err(AppError::bad("That isn't a voice channel."));
    }

    if channel.user_limit > 0 && !perms::has(bits, perms::MOVE_MEMBERS) {
        let occupants = state
            .voice
            .read()
            .await
            .values()
            .filter(|v| v.channel_id == channel_id && v.user_id != auth.user.id)
            .count() as i64;
        if occupants >= channel.user_limit {
            return Err(AppError::bad("That voice channel is full."));
        }
    }

    let metadata = json!({
        "display_name": auth.user.display_name,
        "avatar_url": auth.user.avatar_url,
        "accent_color": auth.user.accent_color,
    })
    .to_string();

    let token = mint_token(
        &state.config.livekit_api_key,
        &state.config.livekit_api_secret,
        Grant {
            room: &format!("channel-{channel_id}"),
            identity: &auth.user.id,
            display_name: &auth.user.display_name,
            metadata,
            can_speak: perms::has(bits, perms::SPEAK),
            can_video: perms::has(bits, perms::VIDEO),
            can_screen_share: perms::has(bits, perms::SCREEN_SHARE),
        },
    )?;

    Ok(Json(json!({
        "token": token,
        "url": state.config.livekit_url,
        "room": format!("channel-{channel_id}"),
        "can_speak": perms::has(bits, perms::SPEAK),
        "can_video": perms::has(bits, perms::VIDEO),
        "can_screen_share": perms::has(bits, perms::SCREEN_SHARE),
    })))
}

pub async fn states(State(state): State<AppState>, _auth: Auth) -> AppResult<Json<Value>> {
    Ok(Json(json!(state.voice_states().await)))
}

/// Moderator action: drop someone from voice. The client watching for this
/// event disconnects itself from the room.
pub async fn force_disconnect(
    State(state): State<AppState>,
    auth: Auth,
    Path(user_id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MOVE_MEMBERS)?;
    state.voice.write().await.remove(&user_id);
    state.emit_all("VOICE_STATE_LEAVE", json!({ "user_id": user_id }));
    state.emit_user(&user_id, "VOICE_FORCE_DISCONNECT", json!({}));
    access::audit(
        &state,
        Some(auth.id()),
        "voice.disconnect",
        "user",
        &user_id,
        "Disconnected from voice",
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}
