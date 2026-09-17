use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::access;
use crate::desktop;
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
        // Theming and the outsider-facing copy are needed before sign-in, so
        // the login and invite pages can render branded rather than default.
        "theme_mode": instance.theme_mode,
        "surface_tint": instance.surface_tint,
        "corner_radius": instance.corner_radius,
        "font_family": instance.font_family,
        "custom_css": instance.custom_css,
        "login_headline": instance.login_headline,
        "login_body": instance.login_body,
        "login_image_url": instance.login_image_url,
        "voice_enabled": state.config.livekit_ready(),
        "setup_token_required": !state.config.setup_token.is_empty(),
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

/// The PWA manifest, generated per instance.
///
/// A static manifest would install on every member's phone as "MiniChat"
/// regardless of how the operator branded their community, which rather
/// defeats the point of branding it.
pub async fn manifest(
    State(state): State<AppState>,
) -> AppResult<impl axum::response::IntoResponse> {
    let instance = access::instance(&state).await?;

    let name = if instance.name.trim().is_empty() {
        "MiniChat".to_string()
    } else {
        instance.name.clone()
    };
    let description = if instance.tagline.trim().is_empty() {
        "Voice, video and text chat for a single community.".to_string()
    } else {
        instance.tagline.clone()
    };

    // The bundled icons are always listed: they are known-good 192/512 PNGs,
    // and a browser that can't use the uploaded one still has something to
    // install with.
    let mut icons = Vec::new();
    if let Some(icon) = instance.icon_url.as_deref().filter(|u| !u.is_empty()) {
        let mime = if icon.ends_with(".png") {
            "image/png"
        } else if icon.ends_with(".webp") {
            "image/webp"
        } else if icon.ends_with(".gif") {
            "image/gif"
        } else {
            "image/jpeg"
        };
        icons.push(json!({ "src": icon, "sizes": "any", "type": mime, "purpose": "any" }));
    }
    icons.push(json!({ "src": "/icon-192.png", "sizes": "192x192", "type": "image/png" }));
    icons.push(json!({ "src": "/icon-512.png", "sizes": "512x512", "type": "image/png" }));
    icons.push(json!({
        "src": "/icon-512.png", "sizes": "512x512", "type": "image/png", "purpose": "maskable"
    }));

    let background = if instance.theme_mode == "light" {
        "#f3f4f8"
    } else {
        "#0b0d13"
    };

    let manifest = json!({
        "name": name,
        "short_name": name.chars().take(12).collect::<String>(),
        "description": description,
        "id": "/",
        "start_url": "/",
        "scope": "/",
        "display": "standalone",
        "orientation": "portrait-primary",
        "theme_color": instance.accent_color,
        "background_color": background,
        "categories": ["social", "communication"],
        "icons": icons,
    });

    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/manifest+json",
            ),
            // Short cache: rebranding should show up on the next visit, not in
            // an hour.
            (axum::http::header::CACHE_CONTROL, "public, max-age=60"),
        ],
        axum::Json(manifest),
    ))
}

/// Where to download the desktop app from, resolved from GitHub releases.
pub async fn desktop_latest(State(state): State<AppState>) -> Json<Value> {
    let release = desktop::latest(&state).await;
    Json(json!({
        "available": release.available,
        "version": release.version,
        "published_at": release.published_at,
        "release_url": release.release_url,
        "assets": release.assets,
        "repo": state.config.desktop_release_repo,
    }))
}
