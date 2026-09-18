//! Friends and favourites.
//!
//! Two different ideas, deliberately stored differently:
//!
//! - **Friendship** needs consent, so it is a pair of directed rows. A request
//!   is `pending` from the sender; accepting writes the mirror row and turns
//!   both into `friend`. That means "did they accept?" is a fact about a row
//!   rather than a flag someone could set on another person's behalf.
//! - **Favouriting** needs none — it is a bookmark, one-sided and private, so
//!   it is its own table and never tells the other person.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::models::UserRow;
use crate::state::AppState;

/// One entry in the friends list, from the asking member's point of view.
#[derive(serde::Serialize)]
pub struct RelationshipView {
    pub user_id: String,
    /// `friend`, `outgoing`, `incoming` or `blocked`.
    pub kind: String,
    pub favourite: bool,
}

/// Everything the client needs to render the friends list in one request.
pub async fn list(State(state): State<AppState>, auth: Auth) -> AppResult<Json<Value>> {
    let mine: Vec<(String, String)> =
        sqlx::query_as("SELECT other_id, kind FROM relationships WHERE user_id = ?")
            .bind(auth.id())
            .fetch_all(&state.db)
            .await?;
    // Requests pointing at me that I haven't answered yet.
    let incoming: Vec<String> = sqlx::query_scalar(
        "SELECT user_id FROM relationships WHERE other_id = ? AND kind = 'pending'",
    )
    .bind(auth.id())
    .fetch_all(&state.db)
    .await?;
    let favourites: Vec<String> =
        sqlx::query_scalar("SELECT other_id FROM favourites WHERE user_id = ?")
            .bind(auth.id())
            .fetch_all(&state.db)
            .await?;

    let mut out: Vec<RelationshipView> = Vec::new();
    for (user_id, kind) in mine {
        let kind = match kind.as_str() {
            // "pending" is directional; name it from the reader's side so the
            // client doesn't have to work out which way it points.
            "pending" => "outgoing",
            other => other,
        };
        out.push(RelationshipView {
            favourite: favourites.contains(&user_id),
            user_id,
            kind: kind.to_string(),
        });
    }
    for user_id in incoming {
        // A row already here means they accepted and this is stale.
        if out.iter().any(|entry| entry.user_id == user_id) {
            continue;
        }
        out.push(RelationshipView {
            favourite: favourites.contains(&user_id),
            user_id,
            kind: "incoming".into(),
        });
    }
    // Favourites need not be friends, so surface them even with no relationship.
    for user_id in favourites {
        if !out.iter().any(|entry| entry.user_id == user_id) {
            out.push(RelationshipView {
                user_id,
                kind: "none".into(),
                favourite: true,
            });
        }
    }

    Ok(Json(json!(out)))
}

async fn require_other(state: &AppState, auth: &Auth, other_id: &str) -> AppResult<UserRow> {
    if other_id == auth.id() {
        return Err(AppError::bad("That's you."));
    }
    sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = ?")
        .bind(other_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("That member no longer exists."))
}

/// Tell both sides their friends list changed.
///
/// Scoped to the two people involved: a friendship is nobody else's business,
/// and broadcasting it would leak who talks to whom.
fn notify(state: &AppState, a: &str, b: &str) {
    for user in [a, b] {
        state.emit_user(user, "RELATIONSHIPS_STALE", json!({}));
    }
}

#[derive(Deserialize)]
pub struct RequestInput {
    pub user_id: String,
}

/// Send a friend request, or accept one already pointing at me.
pub async fn add_friend(
    State(state): State<AppState>,
    auth: Auth,
    Json(input): Json<RequestInput>,
) -> AppResult<Json<Value>> {
    let other = require_other(&state, &auth, &input.user_id).await?;

    // Blocks win, in both directions, and without saying which way.
    let blocked: Option<String> = sqlx::query_scalar(
        "SELECT kind FROM relationships
         WHERE kind = 'blocked' AND ((user_id = ? AND other_id = ?) OR (user_id = ? AND other_id = ?))",
    )
    .bind(auth.id())
    .bind(&other.id)
    .bind(&other.id)
    .bind(auth.id())
    .fetch_optional(&state.db)
    .await?;
    if blocked.is_some() {
        return Err(AppError::forbidden("You can't add that member."));
    }

    let theirs: Option<String> =
        sqlx::query_scalar("SELECT kind FROM relationships WHERE user_id = ? AND other_id = ?")
            .bind(&other.id)
            .bind(auth.id())
            .fetch_optional(&state.db)
            .await?;

    let mut tx = state.db.begin().await?;
    if theirs.as_deref() == Some("pending") {
        // They asked first, so this is an acceptance.
        sqlx::query("UPDATE relationships SET kind = 'friend' WHERE user_id = ? AND other_id = ?")
            .bind(&other.id)
            .bind(auth.id())
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO relationships (user_id, other_id, kind) VALUES (?, ?, 'friend')
             ON CONFLICT(user_id, other_id) DO UPDATE SET kind = 'friend'",
        )
        .bind(auth.id())
        .bind(&other.id)
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO relationships (user_id, other_id, kind) VALUES (?, ?, 'pending')
             ON CONFLICT(user_id, other_id) DO NOTHING",
        )
        .bind(auth.id())
        .bind(&other.id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    notify(&state, auth.id(), &other.id);
    Ok(Json(json!({ "ok": true })))
}

/// Withdraw a request, decline one, or remove a friend.
///
/// One endpoint for all three because they are the same operation — forget
/// this pair — and splitting them would invite the client to guess which
/// state it is in.
pub async fn remove_friend(
    State(state): State<AppState>,
    auth: Auth,
    Path(user_id): Path<String>,
) -> AppResult<Json<Value>> {
    if user_id == auth.id() {
        return Err(AppError::bad("That's you."));
    }
    // A block is deliberate and outlives a friendship, so unfriending must
    // not quietly lift it.
    sqlx::query(
        "DELETE FROM relationships
         WHERE kind != 'blocked'
           AND ((user_id = ? AND other_id = ?) OR (user_id = ? AND other_id = ?))",
    )
    .bind(auth.id())
    .bind(&user_id)
    .bind(&user_id)
    .bind(auth.id())
    .execute(&state.db)
    .await?;

    notify(&state, auth.id(), &user_id);
    Ok(Json(json!({ "ok": true })))
}

/// Block a member: drops any friendship and stops further requests.
pub async fn block(
    State(state): State<AppState>,
    auth: Auth,
    Json(input): Json<RequestInput>,
) -> AppResult<Json<Value>> {
    let other = require_other(&state, &auth, &input.user_id).await?;

    let mut tx = state.db.begin().await?;
    // Their side of the friendship goes; mine becomes the block.
    sqlx::query("DELETE FROM relationships WHERE user_id = ? AND other_id = ?")
        .bind(&other.id)
        .bind(auth.id())
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO relationships (user_id, other_id, kind) VALUES (?, ?, 'blocked')
         ON CONFLICT(user_id, other_id) DO UPDATE SET kind = 'blocked'",
    )
    .bind(auth.id())
    .bind(&other.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    notify(&state, auth.id(), &other.id);
    Ok(Json(json!({ "ok": true })))
}

pub async fn unblock(
    State(state): State<AppState>,
    auth: Auth,
    Path(user_id): Path<String>,
) -> AppResult<Json<Value>> {
    sqlx::query(
        "DELETE FROM relationships WHERE user_id = ? AND other_id = ? AND kind = 'blocked'",
    )
    .bind(auth.id())
    .bind(&user_id)
    .execute(&state.db)
    .await?;
    notify(&state, auth.id(), &user_id);
    Ok(Json(json!({ "ok": true })))
}

/// Favourite a member. Private and one-sided, so only the owner is told.
pub async fn favourite(
    State(state): State<AppState>,
    auth: Auth,
    Path(user_id): Path<String>,
) -> AppResult<Json<Value>> {
    require_other(&state, &auth, &user_id).await?;
    sqlx::query(
        "INSERT INTO favourites (user_id, other_id) VALUES (?, ?)
         ON CONFLICT(user_id, other_id) DO NOTHING",
    )
    .bind(auth.id())
    .bind(&user_id)
    .execute(&state.db)
    .await?;
    state.emit_user(auth.id(), "RELATIONSHIPS_STALE", json!({}));
    Ok(Json(json!({ "ok": true })))
}

pub async fn unfavourite(
    State(state): State<AppState>,
    auth: Auth,
    Path(user_id): Path<String>,
) -> AppResult<Json<Value>> {
    sqlx::query("DELETE FROM favourites WHERE user_id = ? AND other_id = ?")
        .bind(auth.id())
        .bind(&user_id)
        .execute(&state.db)
        .await?;
    state.emit_user(auth.id(), "RELATIONSHIPS_STALE", json!({}));
    Ok(Json(json!({ "ok": true })))
}
