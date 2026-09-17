use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::access;
use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::ids;
use crate::models::{AuditEntry, Ban, Role, UserRow};
use crate::perms;
use crate::routes::snapshot;
use crate::state::AppState;
use crate::validate;

pub async fn get_instance(State(state): State<AppState>, auth: Auth) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_INSTANCE)?;
    let instance = access::instance(&state).await?;
    let mut value = serde_json::to_value(&instance).unwrap_or(Value::Null);
    if let Some(obj) = value.as_object_mut() {
        obj.insert("public_url".into(), json!(state.config.public_url));
        obj.insert("voice_enabled".into(), json!(state.config.livekit_ready()));
        obj.insert("livekit_url".into(), json!(state.config.livekit_url));
    }
    Ok(Json(value))
}

#[derive(Deserialize)]
pub struct UpdateInstanceInput {
    pub name: Option<String>,
    pub tagline: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub banner_url: Option<String>,
    pub accent_color: Option<String>,
    pub rules: Option<String>,
    pub welcome_message: Option<String>,
    pub registration_mode: Option<String>,
    pub require_rules_accept: Option<bool>,
    pub default_role_id: Option<String>,
    pub system_channel_id: Option<Option<String>>,
    pub max_upload_mb: Option<i64>,
    // --- theming ---
    pub theme_mode: Option<String>,
    pub surface_tint: Option<String>,
    pub corner_radius: Option<i64>,
    pub font_family: Option<String>,
    pub custom_css: Option<String>,
    // --- the pages outsiders see ---
    pub login_headline: Option<String>,
    pub login_body: Option<String>,
    pub login_image_url: Option<String>,
}

pub async fn update_instance(
    State(state): State<AppState>,
    auth: Auth,
    Json(input): Json<UpdateInstanceInput>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_INSTANCE)?;
    let mut instance = access::instance(&state).await?;

    if let Some(v) = &input.name {
        instance.name = validate::text(v, "Instance name", 1, 64)?;
    }
    if let Some(v) = &input.tagline {
        instance.tagline = validate::optional_text(v, "Tagline", 120)?;
    }
    if let Some(v) = &input.description {
        instance.description = validate::optional_text(v, "Description", 1000)?;
    }
    if let Some(v) = &input.icon_url {
        instance.icon_url = validate::safe_url(v, "Icon")?;
    }
    if let Some(v) = &input.banner_url {
        instance.banner_url = validate::safe_url(v, "Banner")?;
    }
    if let Some(v) = &input.accent_color {
        instance.accent_color = validate::color(v)?;
    }
    if let Some(v) = &input.rules {
        instance.rules = validate::optional_text(v, "Rules", 8000)?;
    }
    if let Some(v) = &input.welcome_message {
        instance.welcome_message = validate::optional_text(v, "Welcome message", 1000)?;
    }
    if let Some(v) = &input.registration_mode {
        if !matches!(v.as_str(), "invite" | "open" | "closed") {
            return Err(AppError::bad("Unknown registration mode."));
        }
        instance.registration_mode = v.clone();
    }
    if let Some(v) = input.require_rules_accept {
        instance.require_rules_accept = v;
    }
    if let Some(v) = &input.default_role_id {
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM roles WHERE id = ?")
            .bind(v)
            .fetch_optional(&state.db)
            .await?;
        if exists.is_none() {
            return Err(AppError::bad("That role doesn't exist."));
        }
        // Exactly one role is the default, so move the flag with the pointer.
        sqlx::query("UPDATE roles SET is_default = CASE WHEN id = ? THEN 1 ELSE 0 END")
            .bind(v)
            .execute(&state.db)
            .await?;
        instance.default_role_id = Some(v.clone());
    }
    if let Some(v) = &input.system_channel_id {
        instance.system_channel_id = v.clone().filter(|s| !s.is_empty());
    }
    if let Some(v) = input.max_upload_mb {
        instance.max_upload_mb = v.clamp(1, 500);
    }

    if let Some(v) = &input.theme_mode {
        if !matches!(v.as_str(), "dark" | "light" | "system") {
            return Err(AppError::bad("Unknown theme."));
        }
        instance.theme_mode = v.clone();
    }
    if let Some(v) = &input.surface_tint {
        instance.surface_tint = if v.trim().is_empty() {
            None
        } else {
            Some(validate::color(v)?)
        };
    }
    if let Some(v) = input.corner_radius {
        instance.corner_radius = v.clamp(0, 28);
    }
    if let Some(v) = &input.font_family {
        // A font stack, not arbitrary CSS: it is interpolated into a custom
        // property, so quotes and braces have no business here.
        let font = validate::optional_text(v, "Font", 120)?;
        if font.contains([';', '{', '}', '<', '>']) {
            return Err(AppError::bad("That font name contains invalid characters."));
        }
        instance.font_family = font;
    }
    if let Some(v) = &input.custom_css {
        instance.custom_css = validate::optional_text(v, "Custom CSS", 20_000)?;
    }
    if let Some(v) = &input.login_headline {
        instance.login_headline = validate::optional_text(v, "Headline", 120)?;
    }
    if let Some(v) = &input.login_body {
        instance.login_body = validate::optional_text(v, "Body text", 600)?;
    }
    if let Some(v) = &input.login_image_url {
        instance.login_image_url = validate::safe_url(v, "Login image")?;
    }

    sqlx::query(
        "UPDATE instance SET name = ?, tagline = ?, description = ?, icon_url = ?, banner_url = ?,
                accent_color = ?, rules = ?, welcome_message = ?, registration_mode = ?,
                require_rules_accept = ?, default_role_id = ?, system_channel_id = ?,
                max_upload_mb = ?, theme_mode = ?, surface_tint = ?, corner_radius = ?,
                font_family = ?, custom_css = ?, login_headline = ?, login_body = ?,
                login_image_url = ?
         WHERE id = 1",
    )
    .bind(&instance.name)
    .bind(&instance.tagline)
    .bind(&instance.description)
    .bind(&instance.icon_url)
    .bind(&instance.banner_url)
    .bind(&instance.accent_color)
    .bind(&instance.rules)
    .bind(&instance.welcome_message)
    .bind(&instance.registration_mode)
    .bind(instance.require_rules_accept)
    .bind(&instance.default_role_id)
    .bind(&instance.system_channel_id)
    .bind(instance.max_upload_mb)
    .execute(&state.db)
    .await?;

    access::audit(
        &state,
        Some(auth.id()),
        "instance.update",
        "instance",
        "1",
        &instance.name,
    )
    .await;
    state.emit_all(
        "INSTANCE_UPDATE",
        serde_json::to_value(&instance).unwrap_or(Value::Null),
    );
    Ok(Json(json!(instance)))
}

/// Run the orphaned-upload sweep on demand rather than waiting for the hourly
/// timer. Handy when an operator wants the disk back now.
pub async fn sweep_uploads(State(state): State<AppState>, auth: Auth) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_INSTANCE)?;
    let removed = crate::sweeper::sweep(&state)
        .await
        .map_err(AppError::Internal)?;
    access::audit(
        &state,
        Some(auth.id()),
        "instance.sweep_uploads",
        "instance",
        "1",
        &format!("Removed {removed} orphaned upload(s)"),
    )
    .await;
    Ok(Json(json!({ "removed": removed })))
}

pub async fn stats(State(state): State<AppState>, auth: Auth) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_INSTANCE)?;

    let members: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await?;
    let messages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
        .fetch_one(&state.db)
        .await?;
    let channels: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM channels")
        .fetch_one(&state.db)
        .await?;
    let roles: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM roles")
        .fetch_one(&state.db)
        .await?;
    let invites: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invites WHERE revoked = 0")
        .fetch_one(&state.db)
        .await?;
    let bans: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bans")
        .fetch_one(&state.db)
        .await?;
    let storage: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(size), 0) FROM attachments")
        .fetch_one(&state.db)
        .await?;
    let joined_week: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE created_at > datetime('now', '-7 days')",
    )
    .fetch_one(&state.db)
    .await?;
    let messages_week: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE created_at > datetime('now', '-7 days')",
    )
    .fetch_one(&state.db)
    .await?;

    // Message volume per day, for the little activity chart in the panel.
    let activity: Vec<(String, i64)> = sqlx::query_as(
        "SELECT date(created_at) AS day, COUNT(*) FROM messages
         WHERE created_at > datetime('now', '-14 days')
         GROUP BY day ORDER BY day",
    )
    .fetch_all(&state.db)
    .await?;

    let top_channels: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT c.id, c.name, COUNT(m.id) AS total FROM channels c
         LEFT JOIN messages m ON m.channel_id = c.id
         GROUP BY c.id ORDER BY total DESC LIMIT 5",
    )
    .fetch_all(&state.db)
    .await?;

    let online = state.connections.read().await.len() as i64;
    let in_voice = state.voice.read().await.len() as i64;

    Ok(Json(json!({
        "members": members,
        "online": online,
        "in_voice": in_voice,
        "messages": messages,
        "channels": channels,
        "roles": roles,
        "invites": invites,
        "bans": bans,
        "storage_bytes": storage,
        "joined_last_week": joined_week,
        "messages_last_week": messages_week,
        "activity": activity.into_iter().map(|(day, count)| json!({"day": day, "count": count})).collect::<Vec<_>>(),
        "top_channels": top_channels.into_iter().map(|(id, name, count)| json!({"id": id, "name": name, "count": count})).collect::<Vec<_>>(),
        "voice_enabled": state.config.livekit_ready(),
        "public_url": state.config.public_url,
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

pub async fn permission_catalog(_auth: Auth) -> Json<Value> {
    Json(json!(perms::CATALOG
        .iter()
        .map(|(key, bit, label, group)| json!({
            "key": key,
            // Sent as a string: these are 64-bit flags and JSON numbers aren't.
            "bit": bit.to_string(),
            "label": label,
            "group": group,
        }))
        .collect::<Vec<_>>()))
}

// ---- roles ----

pub async fn list_roles(State(state): State<AppState>, _auth: Auth) -> AppResult<Json<Value>> {
    let roles: Vec<Role> = sqlx::query_as("SELECT * FROM roles ORDER BY position DESC, name")
        .fetch_all(&state.db)
        .await?;
    Ok(Json(json!(roles)))
}

/// The highest role position a user holds. Anything at or above it is off
/// limits, which stops an admin from editing their own superiors.
async fn highest_position(state: &AppState, auth: &Auth) -> AppResult<i64> {
    if auth.user.is_operator {
        return Ok(i64::MAX);
    }
    if auth.role_ids.is_empty() {
        return Ok(0);
    }
    let placeholders = vec!["?"; auth.role_ids.len()].join(",");
    let sql = format!("SELECT COALESCE(MAX(position), 0) FROM roles WHERE id IN ({placeholders})");
    let mut q = sqlx::query_scalar::<_, i64>(&sql);
    for id in &auth.role_ids {
        q = q.bind(id);
    }
    Ok(q.fetch_one(&state.db).await?)
}

#[derive(Deserialize)]
pub struct RoleInput {
    pub name: Option<String>,
    pub color: Option<String>,
    pub permissions: Option<i64>,
    pub position: Option<i64>,
    pub hoist: Option<bool>,
    pub mentionable: Option<bool>,
    pub icon_url: Option<String>,
    /// A short text badge shown beside the name, e.g. "MOD".
    pub badge: Option<String>,
}

pub async fn create_role(
    State(state): State<AppState>,
    auth: Auth,
    Json(input): Json<RoleInput>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_ROLES)?;
    let name = validate::text(input.name.as_deref().unwrap_or(""), "Role name", 1, 32)?;
    let color = match &input.color {
        Some(c) if !c.trim().is_empty() => Some(validate::color(c)?),
        _ => None,
    };

    // You can never grant a permission you don't hold yourself.
    let requested = input.permissions.unwrap_or(0) & perms::ALL;
    let granted = requested & auth.permissions;
    if granted != requested {
        return Err(AppError::forbidden(
            "You can't grant permissions you don't have yourself.",
        ));
    }

    let ceiling = highest_position(&state, &auth).await?;
    let position = input
        .position
        .unwrap_or(1)
        .min(ceiling.saturating_sub(1).max(0));

    let id = ids::new_id();
    sqlx::query(
        "INSERT INTO roles (id, name, color, permissions, position, is_default, hoist, mentionable)
         VALUES (?, ?, ?, ?, ?, 0, ?, ?)",
    )
    .bind(&id)
    .bind(&name)
    .bind(&color)
    .bind(granted)
    .bind(position)
    .bind(input.hoist.unwrap_or(false))
    .bind(input.mentionable.unwrap_or(true))
    .execute(&state.db)
    .await?;

    let role: Role = sqlx::query_as("SELECT * FROM roles WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    access::audit(&state, Some(auth.id()), "role.create", "role", &id, &name).await;
    state.emit_all(
        "ROLE_CREATE",
        serde_json::to_value(&role).unwrap_or(Value::Null),
    );
    Ok(Json(json!(role)))
}

pub async fn update_role(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(input): Json<RoleInput>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_ROLES)?;
    let mut role: Role = sqlx::query_as("SELECT * FROM roles WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("That role doesn't exist."))?;

    let ceiling = highest_position(&state, &auth).await?;
    if role.position >= ceiling {
        return Err(AppError::forbidden(
            "That role is ranked at or above your own, so you can't edit it.",
        ));
    }

    if let Some(name) = &input.name {
        role.name = validate::text(name, "Role name", 1, 32)?;
    }
    if let Some(color) = &input.color {
        role.color = if color.trim().is_empty() {
            None
        } else {
            Some(validate::color(color)?)
        };
    }
    if let Some(permissions) = input.permissions {
        let requested = permissions & perms::ALL;
        if requested & auth.permissions != requested {
            return Err(AppError::forbidden(
                "You can't grant permissions you don't have yourself.",
            ));
        }
        role.permissions = requested;
    }
    if let Some(position) = input.position {
        role.position = position.min(ceiling.saturating_sub(1).max(0));
    }
    if let Some(hoist) = input.hoist {
        role.hoist = hoist;
    }
    if let Some(mentionable) = input.mentionable {
        role.mentionable = mentionable;
    }
    if let Some(icon_url) = &input.icon_url {
        role.icon_url = validate::safe_url(icon_url, "Role icon")?;
    }
    if let Some(badge) = &input.badge {
        role.badge = validate::optional_text(badge, "Badge", 8)?;
    }

    sqlx::query(
        "UPDATE roles SET name = ?, color = ?, permissions = ?, position = ?, hoist = ?,
                mentionable = ?, icon_url = ?, badge = ?
         WHERE id = ?",
    )
    .bind(&role.name)
    .bind(&role.color)
    .bind(role.permissions)
    .bind(role.position)
    .bind(role.hoist)
    .bind(role.mentionable)
    .bind(&role.icon_url)
    .bind(&role.badge)
    .bind(&id)
    .execute(&state.db)
    .await?;

    access::audit(
        &state,
        Some(auth.id()),
        "role.update",
        "role",
        &id,
        &role.name,
    )
    .await;
    state.emit_all(
        "ROLE_UPDATE",
        serde_json::to_value(&role).unwrap_or(Value::Null),
    );
    Ok(Json(json!(role)))
}

pub async fn delete_role(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_ROLES)?;
    let role: Role = sqlx::query_as("SELECT * FROM roles WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("That role doesn't exist."))?;

    if role.is_default {
        return Err(AppError::bad(
            "The default role can't be deleted. Pick a different default first.",
        ));
    }
    let ceiling = highest_position(&state, &auth).await?;
    if role.position >= ceiling {
        return Err(AppError::forbidden(
            "That role is ranked at or above your own, so you can't delete it.",
        ));
    }

    sqlx::query("DELETE FROM roles WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;
    access::audit(
        &state,
        Some(auth.id()),
        "role.delete",
        "role",
        &id,
        &role.name,
    )
    .await;
    state.emit_all("ROLE_DELETE", json!({ "id": id }));
    Ok(Json(json!({ "ok": true })))
}

// ---- membership ----

async fn guard_role_assignment(state: &AppState, auth: &Auth, role_id: &str) -> AppResult<Role> {
    let role: Role = sqlx::query_as("SELECT * FROM roles WHERE id = ?")
        .bind(role_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("That role doesn't exist."))?;
    let ceiling = highest_position(state, auth).await?;
    if role.position >= ceiling {
        return Err(AppError::forbidden(
            "That role is ranked at or above your own, so you can't assign it.",
        ));
    }
    Ok(role)
}

pub async fn add_member_role(
    State(state): State<AppState>,
    auth: Auth,
    Path((user_id, role_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_ROLES)?;
    let role = guard_role_assignment(&state, &auth, &role_id).await?;

    sqlx::query("INSERT OR IGNORE INTO user_roles (user_id, role_id) VALUES (?, ?)")
        .bind(&user_id)
        .bind(&role_id)
        .execute(&state.db)
        .await?;

    access::audit(
        &state,
        Some(auth.id()),
        "member.role_add",
        "user",
        &user_id,
        &role.name,
    )
    .await;
    emit_member(&state, &user_id).await;
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove_member_role(
    State(state): State<AppState>,
    auth: Auth,
    Path((user_id, role_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_ROLES)?;
    let role = guard_role_assignment(&state, &auth, &role_id).await?;
    if role.is_default {
        return Err(AppError::bad("Every member keeps the default role."));
    }

    sqlx::query("DELETE FROM user_roles WHERE user_id = ? AND role_id = ?")
        .bind(&user_id)
        .bind(&role_id)
        .execute(&state.db)
        .await?;

    access::audit(
        &state,
        Some(auth.id()),
        "member.role_remove",
        "user",
        &user_id,
        &role.name,
    )
    .await;
    emit_member(&state, &user_id).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct UpdateMemberInput {
    pub display_name: Option<String>,
    pub is_suspended: Option<bool>,
    pub is_operator: Option<bool>,
}

pub async fn update_member(
    State(state): State<AppState>,
    auth: Auth,
    Path(user_id): Path<String>,
    Json(input): Json<UpdateMemberInput>,
) -> AppResult<Json<Value>> {
    let target: UserRow = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("That member doesn't exist."))?;

    if target.is_operator && !auth.user.is_operator {
        return Err(AppError::forbidden(
            "Only an operator can modify another operator.",
        ));
    }

    if let Some(display_name) = &input.display_name {
        auth.require(perms::MANAGE_NICKNAMES)?;
        let display_name = validate::text(display_name, "Display name", 1, 48)?;
        sqlx::query("UPDATE users SET display_name = ? WHERE id = ?")
            .bind(&display_name)
            .bind(&user_id)
            .execute(&state.db)
            .await?;
        access::audit(
            &state,
            Some(auth.id()),
            "member.rename",
            "user",
            &user_id,
            &display_name,
        )
        .await;
    }

    if let Some(is_suspended) = input.is_suspended {
        auth.require(perms::KICK_MEMBERS)?;
        if user_id == auth.user.id {
            return Err(AppError::bad("You can't suspend your own account."));
        }
        // Bumping token_version ends their live sessions immediately.
        sqlx::query(
            "UPDATE users SET is_suspended = ?, token_version = token_version + 1 WHERE id = ?",
        )
        .bind(is_suspended)
        .bind(&user_id)
        .execute(&state.db)
        .await?;
        access::audit(
            &state,
            Some(auth.id()),
            if is_suspended {
                "member.suspend"
            } else {
                "member.unsuspend"
            },
            "user",
            &user_id,
            &target.username,
        )
        .await;
    }

    if let Some(is_operator) = input.is_operator {
        // Operator is the instance owner role; only another operator grants it.
        if !auth.user.is_operator {
            return Err(AppError::forbidden(
                "Only an operator can promote or demote operators.",
            ));
        }
        if user_id == auth.user.id && !is_operator {
            let operators: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_operator = 1")
                    .fetch_one(&state.db)
                    .await?;
            if operators <= 1 {
                return Err(AppError::bad(
                    "You're the only operator — promote someone else first.",
                ));
            }
        }
        sqlx::query("UPDATE users SET is_operator = ? WHERE id = ?")
            .bind(is_operator)
            .bind(&user_id)
            .execute(&state.db)
            .await?;
        access::audit(
            &state,
            Some(auth.id()),
            if is_operator {
                "member.promote"
            } else {
                "member.demote"
            },
            "user",
            &user_id,
            &target.username,
        )
        .await;
    }

    emit_member(&state, &user_id).await;
    Ok(Json(json!({ "ok": true })))
}

pub async fn kick_member(
    State(state): State<AppState>,
    auth: Auth,
    Path(user_id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require(perms::KICK_MEMBERS)?;
    let target: UserRow = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("That member doesn't exist."))?;

    if target.is_operator {
        return Err(AppError::forbidden("Operators can't be removed."));
    }
    if user_id == auth.user.id {
        return Err(AppError::bad("You can't remove your own account here."));
    }

    // Messages survive the member leaving (author_id is set to NULL) so
    // conversations stay readable.
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(&user_id)
        .execute(&state.db)
        .await?;

    access::audit(
        &state,
        Some(auth.id()),
        "member.kick",
        "user",
        &user_id,
        &target.username,
    )
    .await;
    state.emit_all("MEMBER_REMOVE", json!({ "id": user_id }));
    state.emit_user(&user_id, "INVALID_SESSION", json!({"reason": "removed"}));
    Ok(Json(json!({ "ok": true })))
}

// ---- bans ----

pub async fn list_bans(State(state): State<AppState>, auth: Auth) -> AppResult<Json<Value>> {
    auth.require(perms::BAN_MEMBERS)?;
    let bans: Vec<Ban> = sqlx::query_as(
        "SELECT b.user_id, u.username, u.display_name, b.reason, b.banned_by, b.created_at
         FROM bans b JOIN users u ON u.id = b.user_id ORDER BY b.created_at DESC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!(bans)))
}

#[derive(Deserialize)]
pub struct BanInput {
    #[serde(default)]
    pub reason: String,
}

pub async fn ban_member(
    State(state): State<AppState>,
    auth: Auth,
    Path(user_id): Path<String>,
    Json(input): Json<BanInput>,
) -> AppResult<Json<Value>> {
    auth.require(perms::BAN_MEMBERS)?;
    let target: UserRow = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("That member doesn't exist."))?;

    if target.is_operator {
        return Err(AppError::forbidden("Operators can't be banned."));
    }
    if user_id == auth.user.id {
        return Err(AppError::bad("You can't ban yourself."));
    }

    let reason = validate::optional_text(&input.reason, "Reason", 500)?;
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO bans (user_id, reason, banned_by) VALUES (?, ?, ?)
         ON CONFLICT(user_id) DO UPDATE SET reason = excluded.reason, banned_by = excluded.banned_by",
    )
    .bind(&user_id)
    .bind(&reason)
    .bind(auth.id())
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE users SET is_suspended = 1, token_version = token_version + 1 WHERE id = ?",
    )
    .bind(&user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    access::audit(
        &state,
        Some(auth.id()),
        "member.ban",
        "user",
        &user_id,
        &reason,
    )
    .await;
    state.emit_user(&user_id, "INVALID_SESSION", json!({"reason": "banned"}));
    emit_member(&state, &user_id).await;
    Ok(Json(json!({ "ok": true })))
}

pub async fn unban_member(
    State(state): State<AppState>,
    auth: Auth,
    Path(user_id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require(perms::BAN_MEMBERS)?;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM bans WHERE user_id = ?")
        .bind(&user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE users SET is_suspended = 0 WHERE id = ?")
        .bind(&user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    access::audit(
        &state,
        Some(auth.id()),
        "member.unban",
        "user",
        &user_id,
        "",
    )
    .await;
    emit_member(&state, &user_id).await;
    Ok(Json(json!({ "ok": true })))
}

// ---- audit log ----

#[derive(Deserialize)]
pub struct AuditQuery {
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn audit_log(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<AuditQuery>,
) -> AppResult<Json<Value>> {
    auth.require(perms::VIEW_AUDIT_LOG)?;
    let limit = query.limit.unwrap_or(60).clamp(1, 200);

    let mut sql = String::from(
        "SELECT a.id, a.actor_id, u.display_name AS actor_name, a.action, a.target_type,
                a.target_id, a.detail, a.created_at
         FROM audit_log a LEFT JOIN users u ON u.id = a.actor_id WHERE 1 = 1",
    );
    if query.before.is_some() {
        sql.push_str(" AND a.id < ?");
    }
    if query.action.is_some() {
        sql.push_str(" AND a.action LIKE ?");
    }
    sql.push_str(" ORDER BY a.id DESC LIMIT ?");

    let mut q = sqlx::query_as::<_, AuditEntry>(&sql);
    if let Some(before) = &query.before {
        q = q.bind(before);
    }
    if let Some(action) = &query.action {
        q = q.bind(format!("{action}%"));
    }
    q = q.bind(limit);

    Ok(Json(json!(q.fetch_all(&state.db).await?)))
}

async fn emit_member(state: &AppState, user_id: &str) {
    if let Ok(members) = snapshot::members_payload(state).await {
        if let Some(member) = members.into_iter().find(|m| m["id"] == json!(user_id)) {
            state.emit_all("MEMBER_UPDATE", member);
        }
    }
}
