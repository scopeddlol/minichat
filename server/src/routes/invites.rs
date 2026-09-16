use axum::extract::{Path, State};
use axum::Json;
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::access;
use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::ids;
use crate::models::{Invite, InviteStatus};
use crate::perms;
use crate::state::AppState;
use crate::validate;

/// Public: what an invite link shows before someone signs up.
pub async fn preview(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> AppResult<Json<Value>> {
    let instance = access::instance(&state).await?;
    let invite: Option<InviteStatus> = sqlx::query_as(
        "SELECT role_id, created_by, max_uses, uses, expires_at, revoked
         FROM invites WHERE code = ?",
    )
    .bind(&code)
    .fetch_optional(&state.db)
    .await?;

    let Some(invite) = invite else {
        return Err(AppError::not_found("That invite link isn't valid."));
    };
    let rejection = invite.rejection();

    let inviter: Option<String> = match &invite.created_by {
        Some(id) => {
            sqlx::query_scalar("SELECT display_name FROM users WHERE id = ?")
                .bind(id)
                .fetch_optional(&state.db)
                .await?
        }
        None => None,
    };
    let member_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await?;

    Ok(Json(json!({
        "valid": rejection.is_none(),
        "reason": rejection.unwrap_or(""),
        "inviter": inviter,
        "instance_name": instance.name,
        "tagline": instance.tagline,
        "description": instance.description,
        "icon_url": instance.icon_url,
        "banner_url": instance.banner_url,
        "accent_color": instance.accent_color,
        "rules": instance.rules,
        "require_rules_accept": instance.require_rules_accept,
        "member_count": member_count,
    })))
}

pub async fn list_invites(State(state): State<AppState>, auth: Auth) -> AppResult<Json<Value>> {
    auth.require(perms::CREATE_INVITES)?;
    // Members who can only create invites see their own; moderators see all.
    let invites: Vec<Invite> = if auth.can(perms::MANAGE_INSTANCE) {
        sqlx::query_as("SELECT * FROM invites ORDER BY created_at DESC LIMIT 200")
            .fetch_all(&state.db)
            .await?
    } else {
        sqlx::query_as(
            "SELECT * FROM invites WHERE created_by = ? ORDER BY created_at DESC LIMIT 200",
        )
        .bind(auth.id())
        .fetch_all(&state.db)
        .await?
    };

    let public_url = state.config.public_url.clone();
    Ok(Json(json!(invites
        .into_iter()
        .map(|i| {
            let url = format!("{public_url}/invite/{}", i.code);
            let mut value = serde_json::to_value(&i).unwrap_or(Value::Null);
            if let Some(obj) = value.as_object_mut() {
                obj.insert("url".into(), json!(url));
            }
            value
        })
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
pub struct CreateInviteInput {
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub role_id: Option<String>,
    #[serde(default)]
    pub max_uses: i64,
    /// Hours until expiry; 0 means never.
    #[serde(default)]
    pub expires_in_hours: i64,
}

pub async fn create_invite(
    State(state): State<AppState>,
    auth: Auth,
    Json(input): Json<CreateInviteInput>,
) -> AppResult<Json<Value>> {
    auth.require(perms::CREATE_INVITES)?;

    // Only role managers may pin a role to an invite, or someone with plain
    // invite rights could hand out Administrator.
    let role_id = match &input.role_id {
        Some(role_id) if !role_id.is_empty() => {
            auth.require(perms::MANAGE_ROLES)?;
            let exists: Option<String> = sqlx::query_scalar("SELECT id FROM roles WHERE id = ?")
                .bind(role_id)
                .fetch_optional(&state.db)
                .await?;
            if exists.is_none() {
                return Err(AppError::bad("That role doesn't exist."));
            }
            Some(role_id.clone())
        }
        _ => None,
    };

    let expires_at = if input.expires_in_hours > 0 {
        Some(
            (Utc::now() + Duration::hours(input.expires_in_hours.min(24 * 365)))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        )
    } else {
        None
    };

    let code = ids::token(10);
    sqlx::query(
        "INSERT INTO invites (code, created_by, role_id, note, max_uses, expires_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&code)
    .bind(auth.id())
    .bind(&role_id)
    .bind(validate::optional_text(&input.note, "Note", 120)?)
    .bind(input.max_uses.clamp(0, 10_000))
    .bind(&expires_at)
    .execute(&state.db)
    .await?;

    access::audit(
        &state,
        Some(auth.id()),
        "invite.create",
        "invite",
        &code,
        &input.note,
    )
    .await;

    let invite: Invite = sqlx::query_as("SELECT * FROM invites WHERE code = ?")
        .bind(&code)
        .fetch_one(&state.db)
        .await?;
    let mut value = serde_json::to_value(&invite).unwrap_or(Value::Null);
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "url".into(),
            json!(format!("{}/invite/{}", state.config.public_url, code)),
        );
    }
    Ok(Json(value))
}

pub async fn revoke_invite(
    State(state): State<AppState>,
    auth: Auth,
    Path(code): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require(perms::CREATE_INVITES)?;
    let owner: Option<Option<String>> =
        sqlx::query_scalar("SELECT created_by FROM invites WHERE code = ?")
            .bind(&code)
            .fetch_optional(&state.db)
            .await?;
    let Some(owner) = owner else {
        return Err(AppError::not_found("That invite doesn't exist."));
    };
    if owner.as_deref() != Some(auth.id()) && !auth.can(perms::MANAGE_INSTANCE) {
        return Err(AppError::forbidden("You can only revoke your own invites."));
    }

    sqlx::query("UPDATE invites SET revoked = 1 WHERE code = ?")
        .bind(&code)
        .execute(&state.db)
        .await?;
    access::audit(
        &state,
        Some(auth.id()),
        "invite.revoke",
        "invite",
        &code,
        "",
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}
