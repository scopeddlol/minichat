//! The WebSocket gateway, mirroring `web/src/lib/gateway.ts`.
//!
//! The server expects an `identify` frame before it sends anything, which is
//! what keeps the session token out of the URL (and therefore out of proxy
//! logs). Everything after that is `{"t": <event>, "d": <payload>}`.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

/// How the UI is told what the connection is doing. The sidebar shows this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Connecting,
    Ready,
    Reconnecting,
    Closed,
}

/// A frame from the server, still unparsed.
#[derive(Debug, Deserialize)]
pub struct Frame {
    #[serde(rename = "t")]
    pub event: String,
    #[serde(rename = "d", default)]
    pub data: serde_json::Value,
}

/// What the gateway task sends up to the UI.
#[derive(Debug)]
pub enum Update {
    Status(Status),
    Event(Frame),
    /// The token was rejected; the app signs out rather than looping.
    SessionInvalid,
}

/// Commands the UI sends down to the gateway task.
#[derive(Debug)]
pub enum Command {
    Close,
}

const HEARTBEAT: Duration = Duration::from_secs(25);
/// Backoff between reconnection attempts, in seconds. The last value repeats,
/// so a server that is down overnight is retried every half minute rather
/// than being given up on.
const BACKOFF: &[u64] = &[1, 2, 5, 10, 15, 30];

fn backoff_for(attempt: usize) -> Duration {
    Duration::from_secs(BACKOFF[attempt.min(BACKOFF.len() - 1)])
}

/// Run the gateway until told to stop or the session is rejected.
///
/// Reconnection lives here rather than in the caller: every disconnect that
/// is not a deliberate close or an invalid session is transient, and the UI
/// should see a status change rather than have to drive a retry loop.
pub async fn run(
    url: String,
    token: String,
    updates: mpsc::UnboundedSender<Update>,
    mut commands: mpsc::UnboundedReceiver<Command>,
) {
    let mut attempt = 0usize;

    loop {
        let status = if attempt == 0 {
            Status::Connecting
        } else {
            Status::Reconnecting
        };
        if updates.send(Update::Status(status)).is_err() {
            return;
        }

        match connect_once(&url, &token, &updates, &mut commands).await {
            Outcome::Closed => {
                let _ = updates.send(Update::Status(Status::Closed));
                return;
            }
            Outcome::SessionInvalid => {
                let _ = updates.send(Update::SessionInvalid);
                let _ = updates.send(Update::Status(Status::Closed));
                return;
            }
            Outcome::Dropped => {
                // A connection that lived long enough to be useful starts the
                // backoff over, so a nightly server restart doesn't leave the
                // client waiting half a minute.
                attempt += 1;
            }
            Outcome::Connected => {
                attempt = 0;
            }
        }

        let _ = updates.send(Update::Status(Status::Reconnecting));

        tokio::select! {
            _ = tokio::time::sleep(backoff_for(attempt)) => {}
            command = commands.recv() => {
                if matches!(command, Some(Command::Close) | None) {
                    let _ = updates.send(Update::Status(Status::Closed));
                    return;
                }
            }
        }
    }
}

enum Outcome {
    /// Deliberately closed by the UI.
    Closed,
    /// The token was rejected.
    SessionInvalid,
    /// Never established.
    Dropped,
    /// Established, then lost.
    Connected,
}

async fn connect_once(
    url: &str,
    token: &str,
    updates: &mpsc::UnboundedSender<Update>,
    commands: &mut mpsc::UnboundedReceiver<Command>,
) -> Outcome {
    let Ok((stream, _)) = tokio_tungstenite::connect_async(url).await else {
        return Outcome::Dropped;
    };

    let (mut sink, mut source) = stream.split();

    let identify = serde_json::json!({ "op": "identify", "token": token }).to_string();
    if sink.send(WsMessage::Text(identify)).await.is_err() {
        return Outcome::Dropped;
    }

    let mut heartbeat = tokio::time::interval(HEARTBEAT);
    // The first tick fires immediately; the server has just been identified
    // to, so there is nothing to prove yet.
    heartbeat.tick().await;

    let mut established = false;

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                let ping = serde_json::json!({ "op": "ping" }).to_string();
                if sink.send(WsMessage::Text(ping)).await.is_err() {
                    return if established { Outcome::Connected } else { Outcome::Dropped };
                }
            }

            // Either a deliberate close, or a dropped channel meaning the
            // UI is gone. Both end the connection for good.
            _ = commands.recv() => {
                let _ = sink.send(WsMessage::Close(None)).await;
                return Outcome::Closed;
            }

            frame = source.next() => {
                match frame {
                    Some(Ok(WsMessage::Text(text))) => {
                        let Ok(frame) = serde_json::from_str::<Frame>(&text) else {
                            // A frame this client doesn't understand is not a
                            // reason to drop a working connection.
                            continue;
                        };
                        match frame.event.as_str() {
                            "PONG" => continue,
                            "INVALID_SESSION" => return Outcome::SessionInvalid,
                            "READY" => {
                                established = true;
                                if updates.send(Update::Status(Status::Ready)).is_err() {
                                    return Outcome::Closed;
                                }
                            }
                            _ => {}
                        }
                        if updates.send(Update::Event(frame)).is_err() {
                            return Outcome::Closed;
                        }
                    }
                    Some(Ok(WsMessage::Ping(payload))) => {
                        if sink.send(WsMessage::Pong(payload)).await.is_err() {
                            return if established { Outcome::Connected } else { Outcome::Dropped };
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) | None => {
                        return if established { Outcome::Connected } else { Outcome::Dropped };
                    }
                    Some(Err(_)) => {
                        return if established { Outcome::Connected } else { Outcome::Dropped };
                    }
                    Some(Ok(_)) => continue,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_climbs_then_holds() {
        assert_eq!(backoff_for(0), Duration::from_secs(1));
        assert_eq!(backoff_for(3), Duration::from_secs(10));
        // Far past the end of the table it keeps retrying rather than giving
        // up, at the longest interval.
        assert_eq!(backoff_for(99), Duration::from_secs(30));
    }

    #[test]
    fn frames_parse_and_tolerate_a_missing_payload() {
        let frame: Frame = serde_json::from_str(r#"{"t":"MESSAGE_CREATE","d":{"id":"1"}}"#).unwrap();
        assert_eq!(frame.event, "MESSAGE_CREATE");
        assert_eq!(frame.data["id"], "1");

        let bare: Frame = serde_json::from_str(r#"{"t":"PONG"}"#).unwrap();
        assert_eq!(bare.event, "PONG");
        assert!(bare.data.is_null());
    }
}
