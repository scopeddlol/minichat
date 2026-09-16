use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::access;
use crate::auth::{hash_password, issue_token, verify_password, Auth};
use crate::error::{AppError, AppResult};
use crate::ids;
use crate::models::{InviteStatus, UserRow};
use crate::routes::snapshot;
use crate::state::AppState;
use crate::validate;

#[derive(Deserialize)]
pub struct RegisterInput {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub invite: Option<String>,
    #[serde(default)]
    pub accept_rules: bool,
}

pub async fn register(
    State(state): State<AppState>,
    Json(input): Json<RegisterInput>,
) -> AppResult<Json<Value>> {
    let instance = access::instance(&state).await?;
    if !instance.setup_complete {
        return Err(AppError::bad("This instance hasn't finished setup yet."));
    }

    let username = validate::username(&input.username)?;
    validate::password(&input.password)?;
    let display_name = if input.display_name.trim().is_empty() {
        username.clone()
    } else {
        validate::text(&input.display_name, "Display name", 1, 48)?
    };
    let email = match &input.email {
        Some(e) if !e.trim().is_empty() => Some(validate::email(e)?),
        _ => None,
    };

    // Resolve the invite before touching the database, so a closed instance
    // never half-creates an account.
    let invite_code = input
        .invite
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    let mut invite_role: Option<String> = None;
    let mut inviter: Option<String> = None;

    match instance.registration_mode.as_str() {
        "open" => {
            if let Some(code) = invite_code {
                let invite = load_valid_invite(&state, code).await?;
                invite_role = invite.role_id;
                inviter = invite.created_by;
            }
        }
        "invite" => {
            let code = invite_code.ok_or_else(|| {
                AppError::forbidden(
                    "This instance is invite-only. Ask an admin for an invite link.",
                )
            })?;
            let invite = load_valid_invite(&state, code).await?;
            invite_role = invite.role_id;
            inviter = invite.created_by;
        }
        _ => {
            return Err(AppError::forbidden(
                "Registration is currently closed on this instance.",
            ))
        }
    }

    if instance.require_rules_accept && !instance.rules.trim().is_empty() && !input.accept_rules {
        return Err(AppError::bad("You need to accept the rules to join."));
    }

    let taken: Option<String> =
        sqlx::query_scalar("SELECT id FROM users WHERE lower(username) = lower(?)")
            .bind(&username)
            .fetch_optional(&state.db)
            .await?;
    if taken.is_some() {
        return Err(AppError::Conflict("That username is already taken.".into()));
    }
    if let Some(email) = &email {
        let taken: Option<String> = sqlx::query_scalar("SELECT id FROM users WHERE email = ?")
            .bind(email)
            .fetch_optional(&state.db)
            .await?;
        if taken.is_some() {
            return Err(AppError::Conflict(
                "An account already uses that email address.".into(),
            ));
        }
    }

    let user_id = ids::new_id();
    let password_hash = hash_password(&input.password)?;
    let mut tx = state.db.begin().await?;

    sqlx::query(
        "INSERT INTO users (id, username, display_name, email, password_hash, accepted_rules, invited_by)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&user_id)
    .bind(&username)
    .bind(&display_name)
    .bind(&email)
    .bind(&password_hash)
    .bind(input.accept_rules)
    .bind(&inviter)
    .execute(&mut *tx)
    .await?;

    for role_id in [instance.default_role_id.clone(), invite_role.clone()]
        .into_iter()
        .flatten()
    {
        sqlx::query("INSERT OR IGNORE INTO user_roles (user_id, role_id) VALUES (?, ?)")
            .bind(&user_id)
            .bind(&role_id)
            .execute(&mut *tx)
            .await?;
    }

    if let Some(code) = invite_code {
        sqlx::query("UPDATE invites SET uses = uses + 1 WHERE code = ?")
            .bind(code)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    // Greet the new member in the system channel so joins are visible in-app.
    if let Some(channel_id) = &instance.system_channel_id {
        let message_id = ids::new_id();
        let inserted = sqlx::query(
            "INSERT INTO messages (id, channel_id, author_id, content, system_kind)
             VALUES (?, ?, ?, '', 'member_join')",
        )
        .bind(&message_id)
        .bind(channel_id)
        .bind(&user_id)
        .execute(&state.db)
        .await;
        if inserted.is_ok() {
            if let Ok(row) = sqlx::query_as::<_, crate::models::MessageRow>(
                "SELECT * FROM messages WHERE id = ?",
            )
            .bind(&message_id)
            .fetch_one(&state.db)
            .await
            {
                if let Ok(message) = access::hydrate_message(&state, row, &user_id).await {
                    state.emit_channel(
                        channel_id,
                        "MESSAGE_CREATE",
                        serde_json::to_value(message).unwrap_or(Value::Null),
                    );
                }
            }
        }
    }

    access::audit(
        &state,
        Some(&user_id),
        "member.join",
        "user",
        &user_id,
        &format!("@{username} joined"),
    )
    .await;

    if let Ok(members) = snapshot::members_payload(&state).await {
        if let Some(member) = members.iter().find(|m| m["id"] == json!(user_id)) {
            state.emit_all("MEMBER_ADD", member.clone());
        }
    }

    let token = issue_token(&state.config.jwt_secret, &user_id, 1)?;
    Ok(Json(json!({ "token": token, "user_id": user_id })))
}

/// Returns the invite when it is usable, otherwise an error explaining why.
async fn load_valid_invite(state: &AppState, code: &str) -> AppResult<InviteStatus> {
    let invite: Option<InviteStatus> = sqlx::query_as(
        "SELECT role_id, created_by, max_uses, uses, expires_at, revoked
         FROM invites WHERE code = ?",
    )
    .bind(code)
    .fetch_optional(&state.db)
    .await?;

    let Some(invite) = invite else {
        return Err(AppError::bad("That invite link isn't valid."));
    };

    match invite.rejection() {
        None => Ok(invite),
        Some("revoked") => Err(AppError::bad("That invite has been revoked.")),
        Some("used_up") => Err(AppError::bad("That invite has already been fully used.")),
        _ => Err(AppError::bad("That invite has expired.")),
    }
}

#[derive(Deserialize)]
pub struct LoginInput {
    pub username: String,
    pub password: String,
}

pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginInput>,
) -> AppResult<Json<Value>> {
    let user: Option<UserRow> =
        sqlx::query_as("SELECT * FROM users WHERE lower(username) = lower(?) OR email = lower(?)")
            .bind(input.username.trim())
            .bind(input.username.trim())
            .fetch_optional(&state.db)
            .await?;

    // Same message for unknown user and wrong password so the endpoint can't
    // be used to enumerate accounts.
    let invalid = || AppError::Unauthorized("Incorrect username or password.".into());
    let Some(user) = user else {
        return Err(invalid());
    };
    if !verify_password(&input.password, &user.password_hash) {
        return Err(invalid());
    }
    if user.is_suspended {
        return Err(AppError::forbidden(
            "Your account has been suspended on this instance.",
        ));
    }

    let token = issue_token(&state.config.jwt_secret, &user.id, user.token_version)?;
    Ok(Json(json!({ "token": token, "user_id": user.id })))
}

pub async fn me(State(state): State<AppState>, auth: Auth) -> AppResult<Json<Value>> {
    Ok(Json(snapshot::me_payload(&state, &auth).await?))
}

#[derive(Deserialize)]
pub struct PasswordInput {
    pub current_password: String,
    pub new_password: String,
}

pub async fn change_password(
    State(state): State<AppState>,
    auth: Auth,
    Json(input): Json<PasswordInput>,
) -> AppResult<Json<Value>> {
    if !verify_password(&input.current_password, &auth.user.password_hash) {
        return Err(AppError::Unauthorized(
            "Your current password is incorrect.".into(),
        ));
    }
    validate::password(&input.new_password)?;
    let hash = hash_password(&input.new_password)?;

    // Bumping token_version signs every other device out, which is what a
    // password change should do.
    sqlx::query(
        "UPDATE users SET password_hash = ?, token_version = token_version + 1 WHERE id = ?",
    )
    .bind(&hash)
    .bind(auth.id())
    .execute(&state.db)
    .await?;

    let token = issue_token(
        &state.config.jwt_secret,
        auth.id(),
        auth.user.token_version + 1,
    )?;
    Ok(Json(json!({ "token": token })))
}

pub async fn revoke_sessions(State(state): State<AppState>, auth: Auth) -> AppResult<Json<Value>> {
    sqlx::query("UPDATE users SET token_version = token_version + 1 WHERE id = ?")
        .bind(auth.id())
        .execute(&state.db)
        .await?;
    let token = issue_token(
        &state.config.jwt_secret,
        auth.id(),
        auth.user.token_version + 1,
    )?;
    Ok(Json(json!({ "token": token })))
}
