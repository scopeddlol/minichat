use crate::{
    auth::Auth,
    error::{AppError, AppResult},
    ids,
    livekit::{mint_token, Grant},
    state::AppState,
    validate,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, models::UserRow, perms, state::Scope};
    use sqlx::sqlite::SqlitePoolOptions;

    async fn fixture() -> (AppState, Auth, Auth, Auth) {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        let config = Config {
            bind_addr: String::new(),
            data_dir: String::new(),
            database_url: String::new(),
            jwt_secret: "test-secret-not-for-production".into(),
            public_url: "http://localhost".into(),
            livekit_url: "ws://localhost:7880".into(),
            livekit_api_key: "test".into(),
            livekit_api_secret: "test-secret-not-for-production".into(),
            web_dir: String::new(),
            setup_token: String::new(),
            vapid_public_key: String::new(),
            vapid_private_key: String::new(),
            vapid_subject: String::new(),
            desktop_release_repo: String::new(),
        };
        let state = AppState::new(db, config);
        let mut users = Vec::new();
        for id in ["a", "b", "operator"] {
            sqlx::query("INSERT INTO users(id,username,display_name,password_hash,is_operator) VALUES(?,?,?,?,?)")
                .bind(id).bind(id).bind(id).bind("unused").bind(id == "operator").execute(&state.db).await.unwrap();
            let user: UserRow = sqlx::query_as("SELECT * FROM users WHERE id = ?")
                .bind(id)
                .fetch_one(&state.db)
                .await
                .unwrap();
            users.push(Auth {
                user,
                permissions: perms::ALL,
                role_ids: vec![],
            });
        }
        (state, users.remove(0), users.remove(0), users.remove(0))
    }

    #[tokio::test]
    async fn private_history_unread_and_events_never_leak_to_operator() {
        let (state, a, b, outsider) = fixture().await;
        let conversation = open(State(state.clone()), a.clone(), Path(b.id().into()))
            .await
            .unwrap()
            .0;
        let id = conversation["id"].as_str().unwrap().to_string();
        let reverse = open(State(state.clone()), b.clone(), Path(a.id().into()))
            .await
            .unwrap()
            .0;
        assert_eq!(conversation["id"], reverse["id"]);
        let mut events = state.events.subscribe();
        let message = send(
            State(state.clone()),
            a.clone(),
            Path(id.clone()),
            Json(Send {
                content: "hello".into(),
            }),
        )
        .await
        .unwrap()
        .0;
        for expected in ["a", "b"] {
            let event = events.recv().await.unwrap();
            assert!(matches!(event.scope, Scope::User(ref id) if id == expected));
            assert_eq!(event.kind, "DIRECT_MESSAGE");
        }
        assert!(events.try_recv().is_err());
        assert!(history(
            State(state.clone()),
            outsider.clone(),
            Path(id.clone()),
            Query(History { before: None })
        )
        .await
        .is_err());
        assert!(send(
            State(state.clone()),
            outsider.clone(),
            Path(id.clone()),
            Json(Send {
                content: "intrusion".into()
            })
        )
        .await
        .is_err());
        assert!(ack(
            State(state.clone()),
            outsider.clone(),
            Path(id.clone()),
            Json(Ack {
                message_id: message["id"].as_str().unwrap().into()
            })
        )
        .await
        .is_err());
        assert_eq!(
            list(State(state.clone()), outsider).await.unwrap().0,
            json!([])
        );
        assert_eq!(
            list(State(state.clone()), b.clone()).await.unwrap().0[0]["unread"],
            1
        );
        let _ = ack(
            State(state.clone()),
            b.clone(),
            Path(id.clone()),
            Json(Ack {
                message_id: message["id"].as_str().unwrap().into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            list(State(state.clone()), b.clone()).await.unwrap().0[0]["unread"],
            0
        );
        assert_eq!(
            history(
                State(state.clone()),
                b,
                Path(id.clone()),
                Query(History {
                    before: Some(message["id"].as_str().unwrap().into())
                })
            )
            .await
            .unwrap()
            .0,
            json!([])
        );
        assert!(send(
            State(state.clone()),
            a.clone(),
            Path(id.clone()),
            Json(Send {
                content: " ".into()
            })
        )
        .await
        .is_err());
        assert!(open(State(state), a.clone(), Path(a.id().into()))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn calls_require_consent_membership_and_an_active_session() {
        let (state, a, b, outsider) = fixture().await;
        let conversation = open(State(state.clone()), a.clone(), Path(b.id().into()))
            .await
            .unwrap()
            .0;
        let conversation_id = conversation["id"].as_str().unwrap().to_string();
        assert!(call(
            State(state.clone()),
            outsider.clone(),
            Path(conversation_id.clone())
        )
        .await
        .is_err());
        let started = call(
            State(state.clone()),
            a.clone(),
            Path(conversation_id.clone()),
        )
        .await
        .unwrap()
        .0;
        let id = started["id"].as_str().unwrap().to_string();
        assert_eq!(
            call(
                State(state.clone()),
                a.clone(),
                Path(conversation_id.clone())
            )
            .await
            .unwrap()
            .0["id"],
            started["id"]
        );
        assert!(token(State(state.clone()), a.clone(), Path(id.clone()))
            .await
            .is_err());
        assert!(action(
            State(state.clone()),
            a.clone(),
            Path(id.clone()),
            Json(CallAction {
                action: "accept".into()
            })
        )
        .await
        .is_err());
        assert!(action(
            State(state.clone()),
            outsider.clone(),
            Path(id.clone()),
            Json(CallAction {
                action: "accept".into()
            })
        )
        .await
        .is_err());
        let _ = action(
            State(state.clone()),
            b.clone(),
            Path(id.clone()),
            Json(CallAction {
                action: "accept".into(),
            }),
        )
        .await
        .unwrap();
        assert!(token(State(state.clone()), a.clone(), Path(id.clone()))
            .await
            .is_ok());
        assert!(token(State(state.clone()), outsider, Path(id.clone()))
            .await
            .is_err());
        let _ = action(
            State(state.clone()),
            b,
            Path(id.clone()),
            Json(CallAction {
                action: "end".into(),
            }),
        )
        .await
        .unwrap();
        assert!(token(State(state.clone()), a.clone(), Path(id.clone()))
            .await
            .is_err());
        let next = call(State(state.clone()), a.clone(), Path(conversation_id))
            .await
            .unwrap()
            .0;
        assert_ne!(started["id"], next["id"]);
        sqlx::query("UPDATE direct_calls SET created_at = datetime('now','-60 seconds')")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(calls(State(state), a).await.unwrap().0, json!([]));
    }
}
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// No operator/admin bypass: only the two participants can read or call.
async fn participants(state: &AppState, user: &str, id: &str) -> AppResult<(String, String)> {
    sqlx::query_as(
        "SELECT user_a, user_b FROM conversations WHERE id = ? AND (user_a = ? OR user_b = ?)",
    )
    .bind(id)
    .bind(user)
    .bind(user)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Conversation not found."))
}

fn emit(state: &AppState, members: &(String, String), kind: &str, data: Value) {
    state.emit_user(&members.0, kind, data.clone());
    state.emit_user(&members.1, kind, data);
}

async fn require_contact(state: &AppState, members: &(String, String)) -> AppResult<()> {
    let unavailable: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM relationships
        WHERE kind = 'blocked' AND ((user_id = ? AND other_id = ?) OR (user_id = ? AND other_id = ?)))
        OR EXISTS(SELECT 1 FROM users WHERE id IN (?, ?) AND is_suspended = 1)")
        .bind(&members.0).bind(&members.1).bind(&members.1).bind(&members.0)
        .bind(&members.0).bind(&members.1).fetch_one(&state.db).await?;
    if unavailable {
        return Err(AppError::forbidden(
            "This member is unavailable for messages and calls.",
        ));
    }
    Ok(())
}

pub async fn end_between(state: &AppState, a: &str, b: &str) -> AppResult<()> {
    let calls: Vec<Call> = sqlx::query_as("UPDATE direct_calls SET status = 'ended' WHERE status != 'ended'
        AND conversation_id IN (SELECT id FROM conversations WHERE (user_a = ? AND user_b = ?) OR (user_a = ? AND user_b = ?)) RETURNING *")
        .bind(a).bind(b).bind(b).bind(a).fetch_all(&state.db).await?;
    for call in calls {
        emit(state, &(a.into(), b.into()), "DIRECT_CALL", json!(call));
    }
    Ok(())
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Conversation {
    id: String,
    peer_id: String,
    last_content: Option<String>,
    unread: i64,
}

pub async fn list(State(state): State<AppState>, auth: Auth) -> AppResult<Json<Value>> {
    let rows: Vec<Conversation> = sqlx::query_as(
        "SELECT c.id, CASE WHEN c.user_a = ? THEN c.user_b ELSE c.user_a END AS peer_id,
         (SELECT content FROM direct_messages WHERE conversation_id = c.id ORDER BY id DESC LIMIT 1) AS last_content,
         (SELECT COUNT(*) FROM direct_messages m WHERE m.conversation_id = c.id AND m.author_id != ? AND m.id > COALESCE(r.last_read_id, '')) AS unread
         FROM conversations c LEFT JOIN direct_reads r ON r.conversation_id = c.id AND r.user_id = ?
         WHERE c.user_a = ? OR c.user_b = ? ORDER BY (SELECT MAX(id) FROM direct_messages WHERE conversation_id = c.id) DESC")
        .bind(auth.id()).bind(auth.id()).bind(auth.id()).bind(auth.id()).bind(auth.id()).fetch_all(&state.db).await?;
    Ok(Json(json!(rows)))
}

pub async fn open(
    State(state): State<AppState>,
    auth: Auth,
    Path(peer): Path<String>,
) -> AppResult<Json<Value>> {
    if peer == auth.id() {
        return Err(AppError::bad("Choose another member."));
    }
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = ? AND is_suspended = 0)")
            .bind(&peer)
            .fetch_one(&state.db)
            .await?;
    if !exists {
        return Err(AppError::not_found("Member not found."));
    }
    let (a, b) = if auth.id() < peer.as_str() {
        (auth.id(), peer.as_str())
    } else {
        (peer.as_str(), auth.id())
    };
    require_contact(&state, &(a.into(), b.into())).await?;
    sqlx::query("INSERT INTO conversations(id,user_a,user_b) VALUES(?,?,?) ON CONFLICT(user_a,user_b) DO NOTHING")
        .bind(ids::new_id()).bind(a).bind(b).execute(&state.db).await?;
    let id: String =
        sqlx::query_scalar("SELECT id FROM conversations WHERE user_a = ? AND user_b = ?")
            .bind(a)
            .bind(b)
            .fetch_one(&state.db)
            .await?;
    Ok(Json(
        json!({"id":id, "peer_id":peer, "last_content":null, "unread":0}),
    ))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct DirectMessage {
    id: String,
    conversation_id: String,
    author_id: String,
    content: String,
    created_at: String,
}
#[derive(Deserialize)]
pub struct History {
    before: Option<String>,
}
pub async fn history(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Query(query): Query<History>,
) -> AppResult<Json<Value>> {
    participants(&state, auth.id(), &id).await?;
    let rows: Vec<DirectMessage> = sqlx::query_as("SELECT * FROM direct_messages WHERE conversation_id = ? AND (? IS NULL OR id < ?) ORDER BY id DESC LIMIT 50")
        .bind(id).bind(&query.before).bind(&query.before).fetch_all(&state.db).await?;
    Ok(Json(json!(rows)))
}
#[derive(Deserialize)]
pub struct Send {
    content: String,
}
pub async fn send(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(input): Json<Send>,
) -> AppResult<Json<Value>> {
    let members = participants(&state, auth.id(), &id).await?;
    require_contact(&state, &members).await?;
    let content = validate::text(&input.content, "Message", 1, 4000)?;
    let message_id = ids::new_id();
    sqlx::query(
        "INSERT INTO direct_messages(id,conversation_id,author_id,content) VALUES(?,?,?,?)",
    )
    .bind(&message_id)
    .bind(&id)
    .bind(auth.id())
    .bind(content)
    .execute(&state.db)
    .await?;
    let message: DirectMessage = sqlx::query_as("SELECT * FROM direct_messages WHERE id = ?")
        .bind(message_id)
        .fetch_one(&state.db)
        .await?;
    let data = json!(message);
    emit(&state, &members, "DIRECT_MESSAGE", data.clone());
    Ok(Json(data))
}
#[derive(Deserialize)]
pub struct Ack {
    message_id: String,
}
pub async fn ack(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(input): Json<Ack>,
) -> AppResult<Json<Value>> {
    participants(&state, auth.id(), &id).await?;
    sqlx::query("INSERT INTO direct_reads(conversation_id,user_id,last_read_id) SELECT ?,?,id FROM direct_messages WHERE id = ? AND conversation_id = ? ON CONFLICT(conversation_id,user_id) DO UPDATE SET last_read_id = MAX(last_read_id, excluded.last_read_id)")
        .bind(&id).bind(auth.id()).bind(input.message_id).bind(&id).execute(&state.db).await?;
    state.emit_user(auth.id(), "DIRECT_READ", json!({"conversation_id":id}));
    Ok(Json(json!({"ok":true})))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Call {
    id: String,
    conversation_id: String,
    caller_id: String,
    status: String,
    created_at: String,
}
async fn expire(state: &AppState) -> AppResult<()> {
    sqlx::query("UPDATE direct_calls SET status = 'ended' WHERE (status = 'ringing' AND created_at < datetime('now','-45 seconds')) OR (status = 'accepted' AND created_at < datetime('now','-6 hours'))").execute(&state.db).await?;
    Ok(())
}
pub async fn calls(State(state): State<AppState>, auth: Auth) -> AppResult<Json<Value>> {
    expire(&state).await?;
    let rows: Vec<Call> = sqlx::query_as("SELECT d.* FROM direct_calls d JOIN conversations c ON c.id = d.conversation_id WHERE (c.user_a = ? OR c.user_b = ?) AND d.status != 'ended'")
        .bind(auth.id()).bind(auth.id()).fetch_all(&state.db).await?;
    Ok(Json(json!(rows)))
}
pub async fn call(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let members = participants(&state, auth.id(), &id).await?;
    require_contact(&state, &members).await?;
    if !state.config.livekit_ready() {
        return Err(AppError::bad("Calls are not configured on this instance."));
    }
    expire(&state).await?;
    let inserted = sqlx::query(
        "INSERT INTO direct_calls(id,conversation_id,caller_id,status)
        SELECT ?,?,?,'ringing' WHERE NOT EXISTS (
          SELECT 1 FROM direct_calls d JOIN conversations c ON c.id = d.conversation_id
          WHERE d.status != 'ended' AND (c.user_a IN (?,?) OR c.user_b IN (?,?))
        ) ON CONFLICT DO NOTHING",
    )
    .bind(ids::new_id())
    .bind(&id)
    .bind(auth.id())
    .bind(&members.0)
    .bind(&members.1)
    .bind(&members.0)
    .bind(&members.1)
    .execute(&state.db)
    .await?;
    let call: Call = sqlx::query_as(
        "SELECT * FROM direct_calls WHERE conversation_id = ? AND status != 'ended'",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::bad("One of you is already in another call."))?;
    if inserted.rows_affected() > 0 {
        emit(&state, &members, "DIRECT_CALL", json!(call));
    }
    Ok(Json(json!(call)))
}

/// The final gateway connection disappearing must not leave the peer ringing
/// or connected indefinitely. Events are still restricted to the two members.
pub async fn disconnect(state: &AppState, user: &str) -> AppResult<()> {
    let ended: Vec<Call> = sqlx::query_as(
        "UPDATE direct_calls SET status = 'ended'
        WHERE status != 'ended' AND conversation_id IN
        (SELECT id FROM conversations WHERE user_a = ? OR user_b = ?) RETURNING *",
    )
    .bind(user)
    .bind(user)
    .fetch_all(&state.db)
    .await?;
    for call in ended {
        let members = participants(state, user, &call.conversation_id).await?;
        emit(state, &members, "DIRECT_CALL", json!(call));
    }
    Ok(())
}
#[derive(Deserialize)]
pub struct CallAction {
    action: String,
}
pub async fn action(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(input): Json<CallAction>,
) -> AppResult<Json<Value>> {
    expire(&state).await?;
    let mut call: Call = sqlx::query_as("SELECT * FROM direct_calls WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("Call not found."))?;
    let members = participants(&state, auth.id(), &call.conversation_id).await?;
    if input.action == "accept" {
        require_contact(&state, &members).await?;
    }
    let status = match input.action.as_str() {
        "accept" if call.caller_id != auth.id() && call.status == "ringing" => "accepted",
        "end" => "ended",
        _ => return Err(AppError::bad("That call can no longer be accepted.")),
    };
    let changed = sqlx::query("UPDATE direct_calls SET status = ? WHERE id = ? AND status = ?")
        .bind(status)
        .bind(id)
        .bind(&call.status)
        .execute(&state.db)
        .await?;
    if changed.rows_affected() == 0 {
        return Err(AppError::bad("The call changed. Please try again."));
    }
    call.status = status.into();
    emit(&state, &members, "DIRECT_CALL", json!(call));
    Ok(Json(json!(call)))
}
pub async fn token(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    expire(&state).await?;
    let call: Call =
        sqlx::query_as("SELECT * FROM direct_calls WHERE id = ? AND status = 'accepted'")
            .bind(&id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::not_found("Active call not found."))?;
    let members = participants(&state, auth.id(), &call.conversation_id).await?;
    require_contact(&state, &members).await?;
    let room = format!("direct-{id}");
    let token = mint_token(
        &state.config.livekit_api_key,
        &state.config.livekit_api_secret,
        Grant {
            room: &room,
            identity: auth.id(),
            display_name: &auth.user.display_name,
            metadata: "{}".into(),
            can_speak: true,
            can_video: true,
            can_screen_share: true,
        },
    )?;
    Ok(Json(
        json!({"token":token,"url":state.config.livekit_url,"room":room,"can_speak":true,"can_video":true,"can_screen_share":true}),
    ))
}
