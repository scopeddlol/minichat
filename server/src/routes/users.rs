use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::models::UserRow;
use crate::routes::snapshot;
use crate::state::AppState;
use crate::validate;

#[derive(Deserialize)]
pub struct UpdateMeInput {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub pronouns: Option<String>,
    pub favorite_game: Option<String>,
    pub accent_color: Option<String>,
    pub custom_status: Option<String>,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub email: Option<String>,
    pub accept_rules: Option<bool>,
    /// Where the member dragged each image to, and how far they zoomed in.
    /// Sent as `{ x, y, zoom }`; clamped server-side.
    pub avatar_frame: Option<FrameInput>,
    pub banner_frame: Option<FrameInput>,
}

#[derive(Deserialize)]
pub struct FrameInput {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

pub async fn update_me(
    State(state): State<AppState>,
    auth: Auth,
    Json(input): Json<UpdateMeInput>,
) -> AppResult<Json<Value>> {
    let mut user = auth.user.clone();

    if let Some(v) = &input.display_name {
        user.display_name = validate::text(v, "Display name", 1, 48)?;
    }
    if let Some(v) = &input.bio {
        user.bio = validate::optional_text(v, "Bio", 600)?;
    }
    if let Some(v) = &input.pronouns {
        user.pronouns = validate::optional_text(v, "Pronouns", 32)?;
    }
    if let Some(v) = &input.favorite_game {
        user.favorite_game = validate::optional_text(v, "Favourite game", 64)?;
    }
    if let Some(v) = &input.custom_status {
        user.custom_status = validate::optional_text(v, "Status", 128)?;
    }
    if let Some(v) = &input.accent_color {
        user.accent_color = validate::color(v)?;
    }
    if let Some(v) = &input.avatar_url {
        user.avatar_url = validate::safe_url(v, "Avatar")?;
        // A replaced image has nothing to do with how the last one was
        // framed, so start it centred rather than inheriting a crop that was
        // chosen for a different picture.
        if input.avatar_frame.is_none() {
            user.avatar_x = 50.0;
            user.avatar_y = 50.0;
            user.avatar_zoom = 1.0;
        }
    }
    if let Some(v) = &input.banner_url {
        user.banner_url = validate::safe_url(v, "Banner")?;
        if input.banner_frame.is_none() {
            user.banner_x = 50.0;
            user.banner_y = 50.0;
            user.banner_zoom = 1.0;
        }
    }
    if let Some(v) = &input.avatar_frame {
        let frame = crate::models::ImageFrame::clamped(v.x, v.y, v.zoom);
        user.avatar_x = frame.x;
        user.avatar_y = frame.y;
        user.avatar_zoom = frame.zoom;
    }
    if let Some(v) = &input.banner_frame {
        let frame = crate::models::ImageFrame::clamped(v.x, v.y, v.zoom);
        user.banner_x = frame.x;
        user.banner_y = frame.y;
        user.banner_zoom = frame.zoom;
    }
    if let Some(v) = &input.email {
        let email = if v.trim().is_empty() {
            None
        } else {
            Some(validate::email(v)?)
        };
        if email != user.email {
            if let Some(email) = &email {
                let taken: Option<String> =
                    sqlx::query_scalar("SELECT id FROM users WHERE email = ? AND id != ?")
                        .bind(email)
                        .bind(auth.id())
                        .fetch_optional(&state.db)
                        .await?;
                if taken.is_some() {
                    return Err(AppError::Conflict(
                        "An account already uses that email address.".into(),
                    ));
                }
            }
            user.email = email;
        }
    }
    if input.accept_rules == Some(true) {
        user.accepted_rules = true;
    }

    sqlx::query(
        "UPDATE users SET display_name = ?, bio = ?, pronouns = ?, favorite_game = ?,
                accent_color = ?, custom_status = ?, avatar_url = ?, banner_url = ?,
                email = ?, accepted_rules = ?,
                avatar_x = ?, avatar_y = ?, avatar_zoom = ?,
                banner_x = ?, banner_y = ?, banner_zoom = ?
         WHERE id = ?",
    )
    .bind(&user.display_name)
    .bind(&user.bio)
    .bind(&user.pronouns)
    .bind(&user.favorite_game)
    .bind(&user.accent_color)
    .bind(&user.custom_status)
    .bind(&user.avatar_url)
    .bind(&user.banner_url)
    .bind(&user.email)
    .bind(user.accepted_rules)
    .bind(user.avatar_x)
    .bind(user.avatar_y)
    .bind(user.avatar_zoom)
    .bind(user.banner_x)
    .bind(user.banner_y)
    .bind(user.banner_zoom)
    .bind(auth.id())
    .execute(&state.db)
    .await?;

    let public = user.clone().public(auth.role_ids.clone());
    state.emit_all(
        "MEMBER_UPDATE",
        serde_json::to_value(&public).unwrap_or(Value::Null),
    );

    let refreshed = Auth {
        user,
        permissions: auth.permissions,
        role_ids: auth.role_ids.clone(),
    };
    Ok(Json(snapshot::me_payload(&state, &refreshed).await?))
}

pub async fn get_user(
    State(state): State<AppState>,
    _auth: Auth,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let user: UserRow = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("That member doesn't exist."))?;

    let roles: Vec<String> = sqlx::query_scalar("SELECT role_id FROM user_roles WHERE user_id = ?")
        .bind(&id)
        .fetch_all(&state.db)
        .await?;

    let online = state.is_online(&id).await;
    let message_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE author_id = ?")
            .bind(&id)
            .fetch_one(&state.db)
            .await?;

    let mut public = user.public(roles);
    if !online {
        public.presence = "offline".into();
    }
    let mut value = serde_json::to_value(public).unwrap_or(Value::Null);
    if let Some(obj) = value.as_object_mut() {
        obj.insert("message_count".into(), json!(message_count));
    }
    Ok(Json(value))
}

pub async fn list_members(State(state): State<AppState>, _auth: Auth) -> AppResult<Json<Value>> {
    Ok(Json(json!(snapshot::members_payload(&state).await?)))
}
