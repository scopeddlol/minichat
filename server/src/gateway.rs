//! WebSocket gateway: one connection per open client, fed by a broadcast
//! channel and filtered per-viewer so private channels stay private.

use std::collections::HashSet;

use axum::extract::ws::{Message as Ws, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::time::{interval, Duration};

use crate::access;
use crate::auth::{authenticate, Auth};
use crate::models::VoiceState;
use crate::perms;
use crate::routes::snapshot;
use crate::state::{AppState, Event, Scope};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum ClientFrame {
    Identify {
        token: String,
    },
    Ping,
    Typing {
        channel_id: String,
    },
    Presence {
        presence: String,
    },
    VoiceState {
        channel_id: Option<String>,
        #[serde(default)]
        muted: bool,
        #[serde(default)]
        deafened: bool,
        #[serde(default)]
        video: bool,
        #[serde(default)]
        streaming: bool,
    },
    Ack {
        channel_id: String,
        message_id: String,
    },
}

pub async fn handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = run(socket, state).await {
            tracing::debug!("gateway connection closed: {e}");
        }
    })
}

async fn run(socket: WebSocket, state: AppState) -> Result<(), String> {
    let (mut sink, mut stream) = socket.split();

    // First frame must be IDENTIFY. Keeping the token out of the URL means it
    // never lands in proxy access logs.
    let (mut auth, session_token): (Auth, String) = loop {
        let Some(Ok(msg)) = stream.next().await else {
            return Err("closed before identify".into());
        };
        let Ws::Text(text) = msg else { continue };
        let Ok(ClientFrame::Identify { token }) = serde_json::from_str::<ClientFrame>(&text) else {
            let _ = sink
                .send(Ws::Text(
                    json!({"t":"INVALID_SESSION","d":{"reason":"expected identify"}})
                        .to_string()
                        .into(),
                ))
                .await;
            return Err("bad identify".into());
        };
        match authenticate(&state, &token).await {
            Ok(auth) => break (auth, token),
            Err(e) => {
                let _ = sink
                    .send(Ws::Text(
                        json!({"t":"INVALID_SESSION","d":{"reason": e.to_string()}})
                            .to_string()
                            .into(),
                    ))
                    .await;
                return Err("auth failed".into());
            }
        }
    };

    let user_id = auth.user.id.clone();
    let is_admin = perms::has(auth.permissions, perms::ADMINISTRATOR);
    let mut visible: HashSet<String> =
        access::visible_channel_ids(&state, &auth.role_ids, is_admin)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .collect();

    let mut rx = state.events.subscribe();

    // Register the connection before sending READY so the snapshot the client
    // receives already includes its own presence.
    let first_connection = {
        let mut conns = state.connections.write().await;
        let entry = conns.entry(user_id.clone()).or_insert(0);
        *entry += 1;
        *entry == 1
    };

    if first_connection {
        let _ = sqlx::query(
            "UPDATE users SET presence = 'online', last_seen_at = datetime('now') WHERE id = ?",
        )
        .bind(&user_id)
        .execute(&state.db)
        .await;
        state.emit_all(
            "PRESENCE_UPDATE",
            json!({"user_id": user_id, "presence": "online"}),
        );
    }

    let ready = snapshot::build(&state, &auth)
        .await
        .map_err(|e| e.to_string())?;
    if sink
        .send(Ws::Text(json!({"t":"READY","d":ready}).to_string().into()))
        .await
        .is_err()
    {
        cleanup(&state, &user_id).await;
        return Err("failed to send ready".into());
    }

    let mut heartbeat = interval(Duration::from_secs(30));
    heartbeat.tick().await;

    loop {
        tokio::select! {
            // Outbound: broadcast events filtered for this viewer.
            event = rx.recv() => {
                let Ok(event) = event else { break };

                // Channel/role changes can change what this viewer can see.
                if matches!(
                    event.kind.as_str(),
                    "CHANNEL_CREATE" | "CHANNEL_UPDATE" | "CHANNEL_DELETE" | "ROLE_UPDATE"
                        | "ROLE_DELETE" | "ROLE_CREATE" | "MEMBER_UPDATE" | "MEMBER_REMOVE"
                        | "CATEGORY_CREATE" | "CATEGORY_UPDATE" | "CATEGORY_DELETE"
                ) {
                    auth = match authenticate(&state, &session_token).await {
                        Ok(auth) => auth,
                        Err(_) => { let _ = sink.send(Ws::Text(json!({"t":"INVALID_SESSION","d":{}}).to_string().into())).await; break; }
                    };
                    let Ok(current) = snapshot::build(&state,&auth).await else { break };
                    visible = current["channels"].as_array().into_iter().flatten().filter_map(|c| c["id"].as_str().map(str::to_owned)).collect();
                    let navigation = json!({"t":"ACCESS_UPDATE","d":{
                        "channels":current["channels"],"categories":current["categories"],
                        "permissions":current["permissions"],"channel_permissions":current["channel_permissions"]
                    }});
                    if sink.send(Ws::Text(navigation.to_string().into())).await.is_err() { break; }
                    // The filtered replacement above also removes revoked channels.
                    // Never forward globally broadcast private channel/category metadata.
                    if event.kind.starts_with("CHANNEL_") || event.kind.starts_with("CATEGORY_") { continue; }
                }

                if !should_deliver(&event, &user_id, &visible, auth.permissions) {
                    continue;
                }
                if sink.send(Ws::Text(serde_json::to_string(&event).unwrap_or_default().into())).await.is_err() {
                    break;
                }
            }

            // Inbound: client frames.
            incoming = stream.next() => {
                let Some(Ok(msg)) = incoming else { break };
                match msg {
                    Ws::Text(text) => {
                        let Ok(frame) = serde_json::from_str::<ClientFrame>(&text) else { continue };
                        handle_frame(&state, &auth, &visible, frame, &mut sink).await;
                    }
                    Ws::Close(_) => break,
                    _ => {}
                }
            }

            _ = heartbeat.tick() => {
                if authenticate(&state, &session_token).await.is_err() {
                    let _ = sink.send(Ws::Text(json!({"t":"INVALID_SESSION","d":{}}).to_string().into())).await;
                    break;
                }
                if sink.send(Ws::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
        }
    }

    cleanup(&state, &user_id).await;
    Ok(())
}

fn should_deliver(
    event: &Event,
    user_id: &str,
    visible: &HashSet<String>,
    permissions: i64,
) -> bool {
    if event.kind == "VOICE_STATE_UPDATE"
        && !event.data["channel_id"]
            .as_str()
            .is_some_and(|id| visible.contains(id))
    {
        return false;
    }
    match &event.scope {
        Scope::All => true,
        Scope::User(target) => target == user_id,
        Scope::Channel(channel_id) => visible.contains(channel_id),
        Scope::Permission(bits) => perms::has(permissions, *bits),
    }
}

type Sink = futures::stream::SplitSink<WebSocket, Ws>;

async fn handle_frame(
    state: &AppState,
    auth: &Auth,
    visible: &HashSet<String>,
    frame: ClientFrame,
    sink: &mut Sink,
) {
    match frame {
        ClientFrame::Identify { .. } => {}
        ClientFrame::Ping => {
            let _ = sink
                .send(Ws::Text(json!({"t":"PONG","d":{}}).to_string().into()))
                .await;
        }
        ClientFrame::Typing { channel_id } => {
            if !visible.contains(&channel_id) {
                return;
            }
            state.emit_channel(
                &channel_id,
                "TYPING_START",
                json!({
                    "channel_id": channel_id,
                    "user_id": auth.user.id,
                    "display_name": auth.user.display_name,
                }),
            );
        }
        ClientFrame::Presence { presence } => {
            if !matches!(presence.as_str(), "online" | "idle" | "dnd") {
                return;
            }
            let _ = sqlx::query("UPDATE users SET presence = ? WHERE id = ?")
                .bind(&presence)
                .bind(&auth.user.id)
                .execute(&state.db)
                .await;
            state.emit_all(
                "PRESENCE_UPDATE",
                json!({"user_id": auth.user.id, "presence": presence}),
            );
        }
        ClientFrame::VoiceState {
            channel_id,
            muted,
            deafened,
            video,
            streaming,
        } => match channel_id {
            Some(channel_id) if visible.contains(&channel_id) => {
                let voice_state = VoiceState {
                    user_id: auth.user.id.clone(),
                    channel_id: channel_id.clone(),
                    muted,
                    deafened,
                    streaming,
                    video,
                };
                state
                    .voice
                    .write()
                    .await
                    .insert(auth.user.id.clone(), voice_state.clone());
                state.emit_all(
                    "VOICE_STATE_UPDATE",
                    serde_json::to_value(&voice_state).unwrap_or(Value::Null),
                );
            }
            _ => {
                state.voice.write().await.remove(&auth.user.id);
                state.emit_all("VOICE_STATE_LEAVE", json!({"user_id": auth.user.id}));
            }
        },
        ClientFrame::Ack {
            channel_id,
            message_id,
        } => {
            let _ = sqlx::query(
                "INSERT INTO read_state (user_id, channel_id, last_read_id) VALUES (?, ?, ?)
                 ON CONFLICT(user_id, channel_id) DO UPDATE SET last_read_id = excluded.last_read_id",
            )
            .bind(&auth.user.id)
            .bind(&channel_id)
            .bind(&message_id)
            .execute(&state.db)
            .await;
        }
    }
}

async fn cleanup(state: &AppState, user_id: &str) {
    let last = {
        let mut conns = state.connections.write().await;
        if let Some(entry) = conns.get_mut(user_id) {
            *entry = entry.saturating_sub(1);
            if *entry == 0 {
                conns.remove(user_id);
                true
            } else {
                false
            }
        } else {
            false
        }
    };

    if last {
        let _ = crate::routes::direct::disconnect(state, user_id).await;
        state.voice.write().await.remove(user_id);
        state.emit_all("VOICE_STATE_LEAVE", json!({"user_id": user_id}));
        let _ = sqlx::query(
            "UPDATE users SET presence = 'offline', last_seen_at = datetime('now') WHERE id = ?",
        )
        .bind(user_id)
        .execute(&state.db)
        .await;
        state.emit_all(
            "PRESENCE_UPDATE",
            json!({"user_id": user_id, "presence": "offline"}),
        );
    }
}
