use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::access;
use crate::error::AppResult;
use crate::state::AppState;

/// Unauthenticated bootstrap info: enough to render the login, invite and
/// setup screens with the instance's own branding.
pub async fn meta(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let instance = access::instance(&state).await?;
    let member_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await?;

    Ok(Json(json!({
        "name": instance.name,
        "tagline": instance.tagline,
        "description": instance.description,
        "icon_url": instance.icon_url,
        "banner_url": instance.banner_url,
        "accent_color": instance.accent_color,
        "rules": instance.rules,
        "welcome_message": instance.welcome_message,
        "setup_complete": instance.setup_complete,
        "registration_mode": instance.registration_mode,
        "require_rules_accept": instance.require_rules_accept,
        "member_count": member_count,
        "voice_enabled": state.config.livekit_ready(),
        "setup_token_required": !state.config.setup_token.is_empty(),
        "version": env!("CARGO_PKG_VERSION"),
    })))
}
