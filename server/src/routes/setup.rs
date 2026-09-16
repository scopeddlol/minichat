//! First-run wizard. Runs exactly once: it creates the operator account,
//! brands the instance, defines roles and seeds the starting channels.

use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::access;
use crate::auth::{hash_password, issue_token};
use crate::error::{AppError, AppResult};
use crate::ids;
use crate::perms;
use crate::state::AppState;
use crate::validate;

#[derive(Deserialize)]
pub struct SetupRoleInput {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub permissions: i64,
    #[serde(default)]
    pub hoist: bool,
}

#[derive(Deserialize)]
pub struct SetupChannelInput {
    pub name: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub category: String,
}

fn default_kind() -> String {
    "text".to_string()
}

#[derive(Deserialize)]
pub struct SetupInput {
    #[serde(default)]
    pub setup_token: String,
    // Operator account
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub email: Option<String>,
    // Branding
    pub instance_name: String,
    #[serde(default)]
    pub tagline: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub banner_url: Option<String>,
    #[serde(default = "default_accent")]
    pub accent_color: String,
    // Community
    #[serde(default)]
    pub rules: String,
    #[serde(default)]
    pub welcome_message: String,
    #[serde(default = "default_registration")]
    pub registration_mode: String,
    #[serde(default = "yes")]
    pub require_rules_accept: bool,
    // Roles & channels
    #[serde(default)]
    pub default_role_name: String,
    #[serde(default)]
    pub default_permissions: Option<i64>,
    #[serde(default)]
    pub roles: Vec<SetupRoleInput>,
    #[serde(default)]
    pub channels: Vec<SetupChannelInput>,
}

fn default_accent() -> String {
    "#5b6ee8".to_string()
}
fn default_registration() -> String {
    "invite".to_string()
}
fn yes() -> bool {
    true
}

pub async fn run_setup(
    State(state): State<AppState>,
    Json(input): Json<SetupInput>,
) -> AppResult<Json<Value>> {
    let instance = access::instance(&state).await?;
    if instance.setup_complete {
        return Err(AppError::Conflict(
            "This instance has already been set up.".into(),
        ));
    }
    if !state.config.setup_token.is_empty() && input.setup_token != state.config.setup_token {
        return Err(AppError::forbidden(
            "That setup token doesn't match the one configured for this instance.",
        ));
    }

    validate::username(&input.username)?;
    validate::password(&input.password)?;
    let instance_name = validate::text(&input.instance_name, "Instance name", 1, 64)?;
    let accent = validate::color(&input.accent_color)?;
    if let Some(email) = &input.email {
        if !email.trim().is_empty() {
            validate::email(email)?;
        }
    }
    if !matches!(
        input.registration_mode.as_str(),
        "invite" | "open" | "closed"
    ) {
        return Err(AppError::bad("Unknown registration mode."));
    }

    let display_name = if input.display_name.trim().is_empty() {
        input.username.clone()
    } else {
        validate::text(&input.display_name, "Display name", 1, 48)?
    };

    let mut tx = state.db.begin().await?;

    // Default role — every member gets it, and it defines the baseline.
    let default_role_id = ids::new_id();
    let default_role_name = if input.default_role_name.trim().is_empty() {
        "Member".to_string()
    } else {
        validate::text(&input.default_role_name, "Role name", 1, 32)?
    };
    let default_permissions = input
        .default_permissions
        .map(|p| p & perms::ALL & !perms::ADMINISTRATOR)
        .unwrap_or(perms::DEFAULT_MEMBER);

    sqlx::query(
        "INSERT INTO roles (id, name, color, permissions, position, is_default, hoist, mentionable)
         VALUES (?, ?, NULL, ?, 0, 1, 0, 1)",
    )
    .bind(&default_role_id)
    .bind(&default_role_name)
    .bind(default_permissions)
    .execute(&mut *tx)
    .await?;

    // Operator role, so the owner shows up with a colour and a hoisted group
    // rather than looking like any other member.
    let operator_role_id = ids::new_id();
    sqlx::query(
        "INSERT INTO roles (id, name, color, permissions, position, is_default, hoist, mentionable)
         VALUES (?, 'Operator', ?, ?, 100, 0, 1, 1)",
    )
    .bind(&operator_role_id)
    .bind(&accent)
    .bind(perms::ALL)
    .execute(&mut *tx)
    .await?;

    let mut extra_roles = Vec::new();
    for (i, role) in input.roles.iter().enumerate() {
        let name = validate::text(&role.name, "Role name", 1, 32)?;
        let color = match &role.color {
            Some(c) if !c.trim().is_empty() => Some(validate::color(c)?),
            _ => None,
        };
        let id = ids::new_id();
        sqlx::query(
            "INSERT INTO roles (id, name, color, permissions, position, is_default, hoist, mentionable)
             VALUES (?, ?, ?, ?, ?, 0, ?, 1)",
        )
        .bind(&id)
        .bind(&name)
        .bind(&color)
        .bind(role.permissions & perms::ALL)
        .bind((i as i64) + 1)
        .bind(role.hoist)
        .execute(&mut *tx)
        .await?;
        extra_roles.push(id);
    }

    // Operator account
    let user_id = ids::new_id();
    let password_hash = hash_password(&input.password)?;
    let email = input
        .email
        .as_ref()
        .map(|e| e.trim().to_lowercase())
        .filter(|e| !e.is_empty());

    sqlx::query(
        "INSERT INTO users (id, username, display_name, email, password_hash, accent_color,
                            is_operator, accepted_rules)
         VALUES (?, ?, ?, ?, ?, ?, 1, 1)",
    )
    .bind(&user_id)
    .bind(&input.username)
    .bind(&display_name)
    .bind(&email)
    .bind(&password_hash)
    .bind(&accent)
    .execute(&mut *tx)
    .await?;

    for role_id in [&default_role_id, &operator_role_id] {
        sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES (?, ?)")
            .bind(&user_id)
            .bind(role_id)
            .execute(&mut *tx)
            .await?;
    }

    // Channels. An empty list gets a sensible starter community rather than a
    // blank sidebar.
    let channel_inputs = if input.channels.is_empty() {
        vec![
            SetupChannelInput {
                name: "announcements".into(),
                kind: "announcement".into(),
                topic: "Instance news and updates".into(),
                category: "Information".into(),
            },
            SetupChannelInput {
                name: "general".into(),
                kind: "text".into(),
                topic: "Say hello".into(),
                category: "Text".into(),
            },
            SetupChannelInput {
                name: "General".into(),
                kind: "voice".into(),
                topic: String::new(),
                category: "Voice".into(),
            },
        ]
    } else {
        input.channels
    };

    let mut categories: Vec<(String, String)> = Vec::new();
    let mut first_text_channel: Option<String> = None;

    for (i, channel) in channel_inputs.iter().enumerate() {
        let name = validate::channel_name(&channel.name)?;
        if !matches!(channel.kind.as_str(), "text" | "voice" | "announcement") {
            return Err(AppError::bad("Unknown channel type."));
        }
        let category_id = if channel.category.trim().is_empty() {
            None
        } else {
            let label = channel.category.trim().to_string();
            match categories.iter().find(|(n, _)| n == &label) {
                Some((_, id)) => Some(id.clone()),
                None => {
                    let id = ids::new_id();
                    sqlx::query("INSERT INTO categories (id, name, position) VALUES (?, ?, ?)")
                        .bind(&id)
                        .bind(&label)
                        .bind(categories.len() as i64)
                        .execute(&mut *tx)
                        .await?;
                    categories.push((label, id.clone()));
                    Some(id)
                }
            }
        };

        let channel_id = ids::new_id();
        sqlx::query(
            "INSERT INTO channels (id, category_id, kind, name, topic, position)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&channel_id)
        .bind(&category_id)
        .bind(&channel.kind)
        .bind(&name)
        .bind(channel.topic.trim())
        .bind(i as i64)
        .execute(&mut *tx)
        .await?;

        // Announcement channels are read-only for the default role by default;
        // that's the whole point of the type.
        if channel.kind == "announcement" {
            sqlx::query(
                "INSERT INTO channel_overwrites (channel_id, role_id, allow, deny)
                 VALUES (?, ?, 0, ?)",
            )
            .bind(&channel_id)
            .bind(&default_role_id)
            .bind(perms::SEND_MESSAGES)
            .execute(&mut *tx)
            .await?;
        }

        if channel.kind == "text" && first_text_channel.is_none() {
            first_text_channel = Some(channel_id.clone());
        }
    }

    sqlx::query(
        "UPDATE instance SET name = ?, tagline = ?, description = ?, icon_url = ?, banner_url = ?,
                accent_color = ?, rules = ?, welcome_message = ?, registration_mode = ?,
                require_rules_accept = ?, default_role_id = ?, system_channel_id = ?,
                setup_complete = 1
         WHERE id = 1",
    )
    .bind(&instance_name)
    .bind(input.tagline.trim())
    .bind(input.description.trim())
    .bind(&input.icon_url)
    .bind(&input.banner_url)
    .bind(&accent)
    .bind(input.rules.trim())
    .bind(input.welcome_message.trim())
    .bind(&input.registration_mode)
    .bind(input.require_rules_accept)
    .bind(&default_role_id)
    .bind(&first_text_channel)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    access::audit(
        &state,
        Some(&user_id),
        "instance.setup",
        "instance",
        "1",
        &format!("Instance '{instance_name}' created"),
    )
    .await;

    let token = issue_token(&state.config.jwt_secret, &user_id, 1)?;
    Ok(Json(json!({ "token": token, "user_id": user_id })))
}
