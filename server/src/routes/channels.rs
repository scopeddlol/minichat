use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::access;
use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::ids;
use crate::models::{Category, Channel};
use crate::perms;
use crate::state::AppState;
use crate::validate;

pub async fn list_channels(State(state): State<AppState>, auth: Auth) -> AppResult<Json<Value>> {
    let is_admin = auth.can(perms::ADMINISTRATOR);
    let visible = access::visible_channel_ids(&state, &auth.role_ids, is_admin).await?;
    let channels: Vec<Channel> = sqlx::query_as("SELECT * FROM channels ORDER BY position, name")
        .fetch_all(&state.db)
        .await?;
    let channels: Vec<Channel> = channels
        .into_iter()
        .filter(|c| visible.iter().any(|id| id == &c.id))
        .collect();
    Ok(Json(json!(channels)))
}

/// Refreshes the caller's per-channel permission map after roles or channel
/// overrides change.
pub async fn my_channel_permissions(
    State(state): State<AppState>,
    auth: Auth,
) -> AppResult<Json<Value>> {
    let map: serde_json::Map<String, Value> = access::channel_permission_map(&state, &auth)
        .await?
        .into_iter()
        .map(|(id, bits)| (id, json!(bits.to_string())))
        .collect();
    Ok(Json(Value::Object(map)))
}

#[derive(Deserialize)]
pub struct CreateChannelInput {
    pub name: String,
    #[serde(default = "text_kind")]
    pub kind: String,
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub user_limit: i64,
    #[serde(default)]
    pub slowmode: i64,
}

fn text_kind() -> String {
    "text".into()
}

pub async fn create_channel(
    State(state): State<AppState>,
    auth: Auth,
    Json(input): Json<CreateChannelInput>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_CHANNELS)?;
    if !matches!(input.kind.as_str(), "text" | "voice" | "announcement") {
        return Err(AppError::bad("Unknown channel type."));
    }

    // Text-style channels get the familiar lowercase-hyphenated treatment;
    // voice channels keep whatever the admin typed.
    let name = if input.kind == "voice" {
        validate::channel_name(&input.name)?
    } else {
        let slug = validate::slugify_channel(&input.name);
        validate::channel_name(&slug)?
    };

    let position: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(position), 0) + 1 FROM channels")
        .fetch_one(&state.db)
        .await?;
    let id = ids::new_id();

    sqlx::query(
        "INSERT INTO channels (id, category_id, kind, name, topic, position, slowmode, is_private, user_limit)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.category_id)
    .bind(&input.kind)
    .bind(&name)
    .bind(validate::optional_text(&input.topic, "Topic", 256)?)
    .bind(position)
    .bind(input.slowmode.clamp(0, 21600))
    .bind(input.is_private)
    .bind(input.user_limit.clamp(0, 99))
    .execute(&state.db)
    .await?;

    // A private channel that nobody can see would be a dead end, so deny the
    // default role and grant the creator's roles explicitly.
    if input.is_private {
        let instance = access::instance(&state).await?;
        if let Some(default_role) = instance.default_role_id {
            sqlx::query(
                "INSERT INTO channel_overwrites (channel_id, role_id, allow, deny) VALUES (?, ?, 0, ?)",
            )
            .bind(&id)
            .bind(&default_role)
            .bind(perms::VIEW_CHANNELS)
            .execute(&state.db)
            .await?;
        }
    }

    if input.kind == "announcement" {
        let instance = access::instance(&state).await?;
        if let Some(default_role) = instance.default_role_id {
            sqlx::query(
                "INSERT INTO channel_overwrites (channel_id, role_id, allow, deny)
                 VALUES (?, ?, 0, ?)
                 ON CONFLICT(channel_id, role_id) DO UPDATE SET deny = deny | excluded.deny",
            )
            .bind(&id)
            .bind(&default_role)
            .bind(perms::SEND_MESSAGES)
            .execute(&state.db)
            .await?;
        }
    }

    let channel = access::get_channel(&state, &id).await?;
    access::audit(
        &state,
        Some(auth.id()),
        "channel.create",
        "channel",
        &id,
        &name,
    )
    .await;
    state.emit_all(
        "CHANNEL_CREATE",
        serde_json::to_value(&channel).unwrap_or(Value::Null),
    );
    Ok(Json(json!(channel)))
}

#[derive(Deserialize)]
pub struct UpdateChannelInput {
    pub name: Option<String>,
    pub topic: Option<String>,
    /// Shown in place of the # / speaker glyph.
    pub emoji: Option<String>,
    /// A short line under the channel name in the sidebar.
    pub description: Option<String>,
    pub category_id: Option<Option<String>>,
    pub position: Option<i64>,
    pub slowmode: Option<i64>,
    pub is_private: Option<bool>,
    pub user_limit: Option<i64>,
}

pub async fn update_channel(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(input): Json<UpdateChannelInput>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_CHANNELS)?;
    let mut channel = access::get_channel(&state, &id).await?;

    if let Some(name) = &input.name {
        channel.name = if channel.kind == "voice" {
            validate::channel_name(name)?
        } else {
            validate::channel_name(&validate::slugify_channel(name))?
        };
    }
    if let Some(topic) = &input.topic {
        channel.topic = validate::optional_text(topic, "Topic", 256)?;
    }
    if let Some(emoji) = &input.emoji {
        channel.emoji = validate::optional_text(emoji, "Emoji", 8)?;
    }
    if let Some(description) = &input.description {
        channel.description = validate::optional_text(description, "Description", 100)?;
    }
    if let Some(category_id) = &input.category_id {
        channel.category_id = category_id.clone();
    }
    if let Some(position) = input.position {
        channel.position = position;
    }
    if let Some(slowmode) = input.slowmode {
        channel.slowmode = slowmode.clamp(0, 21600);
    }
    if let Some(is_private) = input.is_private {
        channel.is_private = is_private;
    }
    if let Some(user_limit) = input.user_limit {
        channel.user_limit = user_limit.clamp(0, 99);
    }

    sqlx::query(
        "UPDATE channels SET name = ?, topic = ?, emoji = ?, description = ?, category_id = ?,
                position = ?, slowmode = ?, is_private = ?, user_limit = ? WHERE id = ?",
    )
    .bind(&channel.name)
    .bind(&channel.topic)
    .bind(&channel.emoji)
    .bind(&channel.description)
    .bind(&channel.category_id)
    .bind(channel.position)
    .bind(channel.slowmode)
    .bind(channel.is_private)
    .bind(channel.user_limit)
    .bind(&id)
    .execute(&state.db)
    .await?;

    access::audit(
        &state,
        Some(auth.id()),
        "channel.update",
        "channel",
        &id,
        &channel.name,
    )
    .await;
    state.emit_all(
        "CHANNEL_UPDATE",
        serde_json::to_value(&channel).unwrap_or(Value::Null),
    );
    Ok(Json(json!(channel)))
}

pub async fn delete_channel(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_CHANNELS)?;
    let channel = access::get_channel(&state, &id).await?;

    sqlx::query("DELETE FROM channels WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    // Keep the instance pointer valid so joins don't try to post into a gone
    // channel.
    sqlx::query("UPDATE instance SET system_channel_id = NULL WHERE system_channel_id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    access::audit(
        &state,
        Some(auth.id()),
        "channel.delete",
        "channel",
        &id,
        &channel.name,
    )
    .await;
    state.emit_all("CHANNEL_DELETE", json!({ "id": id }));
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ReorderInput {
    /// Channel IDs in their new display order.
    pub channels: Vec<ReorderEntry>,
}

#[derive(Deserialize)]
pub struct ReorderEntry {
    pub id: String,
    pub position: i64,
    #[serde(default)]
    pub category_id: Option<String>,
}

pub async fn reorder(
    State(state): State<AppState>,
    auth: Auth,
    Json(input): Json<ReorderInput>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_CHANNELS)?;
    let mut tx = state.db.begin().await?;
    for entry in &input.channels {
        sqlx::query("UPDATE channels SET position = ?, category_id = ? WHERE id = ?")
            .bind(entry.position)
            .bind(&entry.category_id)
            .bind(&entry.id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    let channels: Vec<Channel> = sqlx::query_as("SELECT * FROM channels ORDER BY position, name")
        .fetch_all(&state.db)
        .await?;
    for channel in &channels {
        state.emit_all(
            "CHANNEL_UPDATE",
            serde_json::to_value(channel).unwrap_or(Value::Null),
        );
    }
    Ok(Json(json!({ "ok": true })))
}

// ---- categories ----

pub async fn list_categories(State(state): State<AppState>, _auth: Auth) -> AppResult<Json<Value>> {
    let categories: Vec<Category> =
        sqlx::query_as("SELECT * FROM categories ORDER BY position, name")
            .fetch_all(&state.db)
            .await?;
    Ok(Json(json!(categories)))
}

#[derive(Deserialize)]
pub struct CategoryInput {
    pub name: String,
    #[serde(default)]
    pub position: Option<i64>,
}

pub async fn create_category(
    State(state): State<AppState>,
    auth: Auth,
    Json(input): Json<CategoryInput>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_CHANNELS)?;
    let name = validate::text(&input.name, "Category name", 1, 48)?;
    let position = match input.position {
        Some(p) => p,
        None => {
            sqlx::query_scalar("SELECT COALESCE(MAX(position), 0) + 1 FROM categories")
                .fetch_one(&state.db)
                .await?
        }
    };
    let id = ids::new_id();
    sqlx::query("INSERT INTO categories (id, name, position) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(&name)
        .bind(position)
        .execute(&state.db)
        .await?;

    let category = Category { id, name, position };
    state.emit_all(
        "CATEGORY_CREATE",
        serde_json::to_value(&category).unwrap_or(Value::Null),
    );
    Ok(Json(json!(category)))
}

pub async fn update_category(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(input): Json<CategoryInput>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_CHANNELS)?;
    let name = validate::text(&input.name, "Category name", 1, 48)?;
    let position = input.position.unwrap_or(0);
    sqlx::query("UPDATE categories SET name = ?, position = ? WHERE id = ?")
        .bind(&name)
        .bind(position)
        .bind(&id)
        .execute(&state.db)
        .await?;
    let category = Category { id, name, position };
    state.emit_all(
        "CATEGORY_UPDATE",
        serde_json::to_value(&category).unwrap_or(Value::Null),
    );
    Ok(Json(json!(category)))
}

pub async fn delete_category(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_CHANNELS)?;
    // Channels outlive their category; they just become uncategorised.
    sqlx::query("DELETE FROM categories WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;
    state.emit_all("CATEGORY_DELETE", json!({ "id": id }));
    Ok(Json(json!({ "ok": true })))
}

// ---- permission overwrites ----

pub async fn list_overwrites(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_ROLES)?;
    let rows: Vec<(String, i64, i64)> =
        sqlx::query_as("SELECT role_id, allow, deny FROM channel_overwrites WHERE channel_id = ?")
            .bind(&id)
            .fetch_all(&state.db)
            .await?;
    Ok(Json(json!(rows
        .into_iter()
        .map(|(role_id, allow, deny)| json!({
            "role_id": role_id,
            "allow": allow.to_string(),
            "deny": deny.to_string(),
        }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
pub struct OverwriteInput {
    pub role_id: String,
    #[serde(default)]
    pub allow: i64,
    #[serde(default)]
    pub deny: i64,
}

pub async fn set_overwrite(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(input): Json<OverwriteInput>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_ROLES)?;
    access::get_channel(&state, &id).await?;

    let allow = input.allow & perms::ALL;
    let deny = input.deny & perms::ALL;
    if allow == 0 && deny == 0 {
        sqlx::query("DELETE FROM channel_overwrites WHERE channel_id = ? AND role_id = ?")
            .bind(&id)
            .bind(&input.role_id)
            .execute(&state.db)
            .await?;
    } else {
        sqlx::query(
            "INSERT INTO channel_overwrites (channel_id, role_id, allow, deny) VALUES (?, ?, ?, ?)
             ON CONFLICT(channel_id, role_id) DO UPDATE SET allow = excluded.allow, deny = excluded.deny",
        )
        .bind(&id)
        .bind(&input.role_id)
        .bind(allow)
        .bind(deny)
        .execute(&state.db)
        .await?;
    }

    access::audit(
        &state,
        Some(auth.id()),
        "channel.permissions",
        "channel",
        &id,
        &format!("allow={allow} deny={deny}"),
    )
    .await;
    let channel = access::get_channel(&state, &id).await?;
    state.emit_all(
        "CHANNEL_UPDATE",
        serde_json::to_value(&channel).unwrap_or(Value::Null),
    );
    Ok(Json(json!({ "ok": true })))
}
