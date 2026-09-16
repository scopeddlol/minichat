//! Custom emoji. Usable as `:name:` in message bodies and as reactions.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::access;
use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::ids;
use crate::models::Emoji;
use crate::perms;
use crate::state::AppState;

pub async fn list(State(state): State<AppState>, _auth: Auth) -> AppResult<Json<Value>> {
    Ok(Json(json!(all(&state).await?)))
}

pub async fn all(state: &AppState) -> AppResult<Vec<Emoji>> {
    Ok(
        sqlx::query_as::<_, Emoji>("SELECT * FROM emojis ORDER BY name")
            .fetch_all(&state.db)
            .await?,
    )
}

/// Emoji names share the `:name:` namespace, so keep them predictable.
fn normalise_name(raw: &str) -> AppResult<String> {
    let name: String = raw
        .trim()
        .trim_matches(':')
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let name = name.trim_matches('_').to_string();
    if name.len() < 2 || name.len() > 32 {
        return Err(AppError::bad(
            "Emoji names must be 2-32 characters of letters, numbers or underscores.",
        ));
    }
    Ok(name)
}

#[derive(Deserialize)]
pub struct CreateEmojiInput {
    pub name: String,
    /// Must be an already-uploaded `/uploads/...` URL.
    pub url: String,
}

pub async fn create(
    State(state): State<AppState>,
    auth: Auth,
    Json(input): Json<CreateEmojiInput>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_EMOJI)?;
    let name = normalise_name(&input.name)?;

    if !input.url.starts_with("/uploads/") {
        return Err(AppError::bad("Emoji images must be uploaded first."));
    }

    let taken: Option<String> = sqlx::query_scalar("SELECT id FROM emojis WHERE name = ?")
        .bind(&name)
        .fetch_optional(&state.db)
        .await?;
    if taken.is_some() {
        return Err(AppError::Conflict(format!(
            "An emoji called :{name}: already exists."
        )));
    }

    let id = ids::new_id();
    sqlx::query("INSERT INTO emojis (id, name, url, created_by) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(&name)
        .bind(&input.url)
        .bind(auth.id())
        .execute(&state.db)
        .await?;

    let emoji: Emoji = sqlx::query_as("SELECT * FROM emojis WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await?;

    access::audit(&state, Some(auth.id()), "emoji.create", "emoji", &id, &name).await;
    state.emit_all(
        "EMOJI_CREATE",
        serde_json::to_value(&emoji).unwrap_or(Value::Null),
    );
    Ok(Json(json!(emoji)))
}

#[derive(Deserialize)]
pub struct RenameEmojiInput {
    pub name: String,
}

pub async fn rename(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(input): Json<RenameEmojiInput>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_EMOJI)?;
    let name = normalise_name(&input.name)?;

    let result = sqlx::query("UPDATE emojis SET name = ? WHERE id = ?")
        .bind(&name)
        .bind(&id)
        .execute(&state.db)
        .await;
    if let Err(sqlx::Error::Database(e)) = &result {
        if e.message().contains("UNIQUE") {
            return Err(AppError::Conflict(format!(
                "An emoji called :{name}: already exists."
            )));
        }
    }
    result?;

    let emoji: Emoji = sqlx::query_as("SELECT * FROM emojis WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("That emoji doesn't exist."))?;

    state.emit_all(
        "EMOJI_UPDATE",
        serde_json::to_value(&emoji).unwrap_or(Value::Null),
    );
    Ok(Json(json!(emoji)))
}

pub async fn delete(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    auth.require(perms::MANAGE_EMOJI)?;
    let name: Option<String> = sqlx::query_scalar("SELECT name FROM emojis WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?;

    sqlx::query("DELETE FROM emojis WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    access::audit(
        &state,
        Some(auth.id()),
        "emoji.delete",
        "emoji",
        &id,
        name.as_deref().unwrap_or(""),
    )
    .await;
    state.emit_all("EMOJI_DELETE", json!({ "id": id }));
    Ok(Json(json!({ "ok": true })))
}
