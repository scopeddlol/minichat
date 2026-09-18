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

// ---- category permission inheritance ----

/// Replace a channel's overwrites with its category's.
///
/// Inheritance is copied on write, not resolved on read. Four separate places
/// resolve channel overwrites — `channel_permissions`, `channel_permission_map`,
/// `visible_channel_ids` and `channel_viewers` — and teaching all four about
/// categories means four chances for them to disagree about who can see what.
/// Copying keeps `channel_overwrites` the single answer to that question.
async fn sync_channel_from_category(
    db: &sqlx::SqlitePool,
    channel_id: &str,
    category_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM channel_overwrites WHERE channel_id = ?")
        .bind(channel_id)
        .execute(db)
        .await?;
    sqlx::query(
        "INSERT INTO channel_overwrites (channel_id, role_id, allow, deny)
         SELECT ?, role_id, allow, deny FROM category_overwrites WHERE category_id = ?",
    )
    .bind(channel_id)
    .bind(category_id)
    .execute(db)
    .await?;
    Ok(())
}

/// Push a category's permissions down to every channel still synced to it.
///
/// Returns the channels that changed so the caller can tell clients: a member
/// who just lost access needs their sidebar to stop showing the channel.
async fn resync_category(state: &AppState, category_id: &str) -> AppResult<Vec<Channel>> {
    let channel_ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM channels WHERE category_id = ? AND sync_category = 1")
            .bind(category_id)
            .fetch_all(&state.db)
            .await?;

    for channel_id in &channel_ids {
        sync_channel_from_category(&state.db, channel_id, category_id).await?;
    }

    let mut changed = Vec::new();
    for channel_id in &channel_ids {
        changed.push(access::get_channel(state, channel_id).await?);
    }
    Ok(changed)
}

/// Which table `write_private_overwrites` writes to.
///
/// An enum rather than a table name, so the statements below stay literal
/// strings and no part of a query is ever assembled at runtime.
#[derive(Copy, Clone)]
enum Scope {
    Channel,
    Category,
}

/// Deny `VIEW_CHANNELS` to everyone except the roles named.
///
/// The default role is denied so nobody sees it by accident; each named role is
/// allowed back in. A named role that *is* the default role simply isn't denied,
/// which would otherwise be an overwrite that fights itself.
async fn write_private_overwrites(
    state: &AppState,
    scope: Scope,
    owner_id: &str,
    allowed_role_ids: &[String],
) -> AppResult<()> {
    const ALLOW_CHANNEL: &str =
        "INSERT INTO channel_overwrites (channel_id, role_id, allow, deny) VALUES (?, ?, ?, 0)
         ON CONFLICT(channel_id, role_id) DO UPDATE
           SET allow = excluded.allow | allow, deny = deny & ~excluded.allow";
    const ALLOW_CATEGORY: &str =
        "INSERT INTO category_overwrites (category_id, role_id, allow, deny) VALUES (?, ?, ?, 0)
         ON CONFLICT(category_id, role_id) DO UPDATE
           SET allow = excluded.allow | allow, deny = deny & ~excluded.allow";
    const DENY_CHANNEL: &str =
        "INSERT INTO channel_overwrites (channel_id, role_id, allow, deny) VALUES (?, ?, 0, ?)
         ON CONFLICT(channel_id, role_id) DO UPDATE
           SET deny = deny | excluded.deny, allow = allow & ~excluded.deny";
    const DENY_CATEGORY: &str =
        "INSERT INTO category_overwrites (category_id, role_id, allow, deny) VALUES (?, ?, 0, ?)
         ON CONFLICT(category_id, role_id) DO UPDATE
           SET deny = deny | excluded.deny, allow = allow & ~excluded.deny";

    let (allow_sql, deny_sql) = match scope {
        Scope::Channel => (ALLOW_CHANNEL, DENY_CHANNEL),
        Scope::Category => (ALLOW_CATEGORY, DENY_CATEGORY),
    };

    let default_role = access::instance(state).await?.default_role_id;

    for role_id in allowed_role_ids {
        sqlx::query(allow_sql)
            .bind(owner_id)
            .bind(role_id)
            .bind(perms::VIEW_CHANNELS)
            .execute(&state.db)
            .await?;
    }

    if let Some(default_role) = default_role {
        if !allowed_role_ids.iter().any(|r| r == &default_role) {
            sqlx::query(deny_sql)
                .bind(owner_id)
                .bind(&default_role)
                .bind(perms::VIEW_CHANNELS)
                .execute(&state.db)
                .await?;
        }
    }
    Ok(())
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
    /// Roles that keep access when `is_private` is set. Empty means only
    /// members who already hold VIEW_CHANNELS some other way — an admin, or a
    /// role granted it in a later edit.
    #[serde(default)]
    pub allowed_role_ids: Vec<String>,
    #[serde(default)]
    pub emoji: String,
    #[serde(default)]
    pub description: String,
    /// Take permissions from the category. Defaults to true when the channel
    /// is being created inside one, which is the useful default and matches
    /// what the sidebar's right-click "Create channel" does.
    pub sync_category: Option<bool>,
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

    let name = validate::channel_name(&input.name)?;

    let position: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(position), 0) + 1 FROM channels")
        .fetch_one(&state.db)
        .await?;
    let id = ids::new_id();

    // Synced by default inside a category: that is what makes a private
    // category worth having, since every channel added to it is private too
    // without anyone having to remember.
    let sync_category =
        input.sync_category.unwrap_or(input.category_id.is_some()) && input.category_id.is_some();

    sqlx::query(
        "INSERT INTO channels (id, category_id, kind, name, topic, position, slowmode,
                               is_private, user_limit, emoji, description, sync_category)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .bind(validate::optional_text(&input.emoji, "Emoji", 8)?)
    .bind(validate::optional_text(
        &input.description,
        "Description",
        100,
    )?)
    .bind(sync_category)
    .execute(&state.db)
    .await?;

    if sync_category {
        // Inherit wholesale; the category decides who gets in.
        if let Some(category_id) = &input.category_id {
            sync_channel_from_category(&state.db, &id, category_id).await?;
        }
    } else if input.is_private {
        write_private_overwrites(&state, Scope::Channel, &id, &input.allowed_role_ids).await?;
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
    /// Toggle category inheritance. Turning it on immediately replaces the
    /// channel's own overwrites with the category's.
    pub sync_category: Option<bool>,
    /// Replace the set of roles that can see a private channel. Only read when
    /// the channel is private and not synced.
    pub allowed_role_ids: Option<Vec<String>>,
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
        channel.name = validate::channel_name(name)?;
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
    if let Some(sync) = input.sync_category {
        // Inheriting needs something to inherit from.
        channel.sync_category = sync && channel.category_id.is_some();
    }

    sqlx::query(
        "UPDATE channels SET name = ?, topic = ?, emoji = ?, description = ?, category_id = ?,
                position = ?, slowmode = ?, is_private = ?, user_limit = ?, sync_category = ?
         WHERE id = ?",
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
    .bind(channel.sync_category)
    .bind(&id)
    .execute(&state.db)
    .await?;

    // Permissions follow the switch: synced channels take the category's,
    // and an explicit role list is only meaningful when they don't.
    if channel.sync_category {
        if let Some(category_id) = &channel.category_id {
            sync_channel_from_category(&state.db, &id, category_id).await?;
        }
    } else if let Some(allowed) = &input.allowed_role_ids {
        sqlx::query("DELETE FROM channel_overwrites WHERE channel_id = ?")
            .bind(&id)
            .execute(&state.db)
            .await?;
        if channel.is_private {
            write_private_overwrites(&state, Scope::Channel, &id, allowed).await?;
        }
    }

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
    #[serde(default)]
    pub is_private: bool,
    /// Roles that keep access when the category is private. Every synced
    /// channel inside it gets the same answer.
    #[serde(default)]
    pub allowed_role_ids: Vec<String>,
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
    sqlx::query("INSERT INTO categories (id, name, position, is_private) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(&name)
        .bind(position)
        .bind(input.is_private)
        .execute(&state.db)
        .await?;

    if input.is_private {
        write_private_overwrites(&state, Scope::Category, &id, &input.allowed_role_ids).await?;
    }

    let category = Category {
        id,
        name,
        position,
        is_private: input.is_private,
    };
    access::audit(
        &state,
        Some(auth.id()),
        "category.create",
        "category",
        &category.id,
        &category.name,
    )
    .await;
    state.emit_all(
        "CATEGORY_CREATE",
        serde_json::to_value(&category).unwrap_or(Value::Null),
    );
    Ok(Json(json!(category)))
}

/// PATCH input: every field optional.
///
/// A rename must not be able to un-private a category. `is_private` and
/// `allowed_role_ids` are only touched when they are actually sent, because
/// `#[serde(default)]` on a bool would quietly turn "rename this" into
/// "rename this and open it to everyone", and the overwrite replacement below
/// would then wipe the permissions of every channel synced to it.
#[derive(Deserialize)]
pub struct UpdateCategoryInput {
    pub name: Option<String>,
    pub position: Option<i64>,
    pub is_private: Option<bool>,
    pub allowed_role_ids: Option<Vec<String>>,
}

pub async fn update_category(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(input): Json<UpdateCategoryInput>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_CHANNELS)?;
    let mut category: Category = sqlx::query_as("SELECT * FROM categories WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("Category not found."))?;

    if let Some(name) = &input.name {
        category.name = validate::text(name, "Category name", 1, 48)?;
    }
    if let Some(position) = input.position {
        category.position = position;
    }
    if let Some(is_private) = input.is_private {
        category.is_private = is_private;
    }

    sqlx::query("UPDATE categories SET name = ?, position = ?, is_private = ? WHERE id = ?")
        .bind(&category.name)
        .bind(category.position)
        .bind(category.is_private)
        .bind(&id)
        .execute(&state.db)
        .await?;

    // The role list is the whole truth about a private category, so replace
    // rather than merge: a role dropped from the list must lose access. Only
    // when a list was actually sent, or when privacy was just switched off.
    let rewrite_access = input.allowed_role_ids.is_some() || input.is_private == Some(false);
    if rewrite_access {
        sqlx::query("DELETE FROM category_overwrites WHERE category_id = ?")
            .bind(&id)
            .execute(&state.db)
            .await?;
        if category.is_private {
            let allowed = input.allowed_role_ids.clone().unwrap_or_default();
            write_private_overwrites(&state, Scope::Category, &id, &allowed).await?;
        }
    }

    // Every synced channel inherits the new answer, and clients need telling:
    // a member who just lost access should stop seeing the channel without
    // having to reload. Skipped for a plain rename, which changes no access.
    if rewrite_access {
        for channel in resync_category(&state, &id).await? {
            state.emit_all(
                "CHANNEL_UPDATE",
                serde_json::to_value(&channel).unwrap_or(Value::Null),
            );
        }
    }

    access::audit(
        &state,
        Some(auth.id()),
        "category.update",
        "category",
        &id,
        &category.name,
    )
    .await;
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

/// The roles a private channel or category admits.
///
/// Derived from the overwrites rather than stored twice: a role is "allowed" if
/// it is granted VIEW_CHANNELS here, which is exactly what
/// `write_private_overwrites` writes.
pub async fn allowed_roles(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_CHANNELS)?;
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT role_id FROM channel_overwrites WHERE channel_id = ? AND allow & ? != 0
         UNION
         SELECT role_id FROM category_overwrites WHERE category_id = ? AND allow & ? != 0",
    )
    .bind(&id)
    .bind(perms::VIEW_CHANNELS)
    .bind(&id)
    .bind(perms::VIEW_CHANNELS)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "role_ids": rows })))
}

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
