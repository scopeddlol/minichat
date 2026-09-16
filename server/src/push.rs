//! Web Push delivery.
//!
//! Notifications are the difference between an app people check and an app
//! people live in, so this path is deliberately forgiving: a failed send never
//! affects the request that triggered it, and a subscription the browser has
//! retired is pruned automatically.

use serde::Serialize;
use std::sync::Arc;
use web_push::{
    ContentEncoding, IsahcWebPushClient, SubscriptionInfo, SubscriptionKeys, VapidSignatureBuilder,
    WebPushClient, WebPushError, WebPushMessageBuilder,
};

use crate::ids;
use crate::state::AppState;

/// What the service worker renders. Kept small — push payloads have a hard
/// size limit once encrypted.
#[derive(Serialize)]
pub struct PushPayload {
    pub title: String,
    pub body: String,
    pub icon: Option<String>,
    /// Collapses repeat notifications for the same channel.
    pub tag: String,
    pub channel_id: String,
    pub message_id: String,
}

#[derive(Debug, sqlx::FromRow)]
struct Subscription {
    id: String,
    endpoint: String,
    p256dh: String,
    auth: String,
}

/// Store (or refresh) a browser's push subscription.
pub async fn subscribe(
    state: &AppState,
    user_id: &str,
    endpoint: &str,
    p256dh: &str,
    auth_key: &str,
    user_agent: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO push_subscriptions (id, user_id, endpoint, p256dh, auth, user_agent)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(endpoint) DO UPDATE SET
             user_id = excluded.user_id,
             p256dh = excluded.p256dh,
             auth = excluded.auth,
             user_agent = excluded.user_agent",
    )
    .bind(ids::new_id())
    .bind(user_id)
    .bind(endpoint)
    .bind(p256dh)
    .bind(auth_key)
    .bind(user_agent.chars().take(200).collect::<String>())
    .execute(&state.db)
    .await?;
    Ok(())
}

pub async fn unsubscribe(state: &AppState, endpoint: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = ?")
        .bind(endpoint)
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Fan a payload out to every device a set of members has registered.
///
/// Spawned rather than awaited by callers: sending to a dozen endpoints should
/// never hold up the HTTP response for the message that triggered it.
pub fn dispatch(state: &AppState, user_ids: Vec<String>, payload: PushPayload) {
    if !state.config.push_ready() || user_ids.is_empty() {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = send_to_users(&state, &user_ids, &payload).await {
            tracing::warn!("push dispatch failed: {e}");
        }
    });
}

async fn send_to_users(
    state: &AppState,
    user_ids: &[String],
    payload: &PushPayload,
) -> Result<(), String> {
    let placeholders = vec!["?"; user_ids.len()].join(",");
    let sql = format!(
        "SELECT id, endpoint, p256dh, auth FROM push_subscriptions WHERE user_id IN ({placeholders})"
    );
    let mut query = sqlx::query_as::<_, Subscription>(&sql);
    for id in user_ids {
        query = query.bind(id);
    }
    let subscriptions = query
        .fetch_all(&state.db)
        .await
        .map_err(|e| e.to_string())?;
    if subscriptions.is_empty() {
        return Ok(());
    }

    let client = IsahcWebPushClient::new().map_err(|e| e.to_string())?;
    let body = serde_json::to_vec(payload).map_err(|e| e.to_string())?;
    let body = Arc::new(body);

    for subscription in subscriptions {
        let info = SubscriptionInfo {
            endpoint: subscription.endpoint.clone(),
            keys: SubscriptionKeys {
                p256dh: subscription.p256dh.clone(),
                auth: subscription.auth.clone(),
            },
        };

        let signature =
            match VapidSignatureBuilder::from_base64(&state.config.vapid_private_key, &info) {
                Ok(builder) => {
                    let mut builder = builder;
                    builder.add_claim("sub", state.config.vapid_subject.clone());
                    match builder.build() {
                        Ok(signature) => signature,
                        Err(e) => {
                            tracing::warn!("could not build VAPID signature: {e}");
                            continue;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("invalid VAPID key: {e}");
                    return Ok(());
                }
            };

        let mut builder = WebPushMessageBuilder::new(&info);
        builder.set_payload(ContentEncoding::Aes128Gcm, &body);
        builder.set_vapid_signature(signature);
        // Browsers hold a notification for a while if the device is offline.
        builder.set_ttl(60 * 60 * 12);

        let message = match builder.build() {
            Ok(message) => message,
            Err(e) => {
                tracing::warn!("could not build push message: {e}");
                continue;
            }
        };

        match client.send(message).await {
            Ok(()) => {
                let _ = sqlx::query(
                    "UPDATE push_subscriptions SET last_used_at = datetime('now') WHERE id = ?",
                )
                .bind(&subscription.id)
                .execute(&state.db)
                .await;
            }
            // The browser has retired this subscription; stop trying forever.
            Err(WebPushError::EndpointNotValid(_)) | Err(WebPushError::EndpointNotFound(_)) => {
                tracing::debug!("pruning dead push subscription {}", subscription.id);
                let _ = sqlx::query("DELETE FROM push_subscriptions WHERE id = ?")
                    .bind(&subscription.id)
                    .execute(&state.db)
                    .await;
            }
            Err(e) => tracing::warn!("push send failed: {e}"),
        }
    }
    Ok(())
}

/// Work out who should be notified about a new message.
///
/// A member is notified when they are mentioned, or when their notification
/// mode for that channel is "all". Members who muted the channel are never
/// notified, not even for a mention — muting has to mean muted.
pub async fn recipients_for_message(
    state: &AppState,
    channel_id: &str,
    author_id: Option<&str>,
    mentioned: &[String],
    is_everyone: bool,
) -> Result<Vec<String>, sqlx::Error> {
    // Only members who can actually view the channel are eligible.
    let candidates: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT u.id, u.notification_mode, cn.mode
         FROM users u
         LEFT JOIN channel_notifications cn ON cn.user_id = u.id AND cn.channel_id = ?
         WHERE u.is_suspended = 0",
    )
    .bind(channel_id)
    .fetch_all(&state.db)
    .await?;

    let viewers = crate::access::channel_viewers(state, channel_id).await?;

    let mut recipients = Vec::new();
    for (user_id, global_mode, channel_mode) in candidates {
        if Some(user_id.as_str()) == author_id {
            continue;
        }
        if !viewers.contains(&user_id) {
            continue;
        }
        let mode = channel_mode.unwrap_or(global_mode);
        if mode == "none" {
            continue;
        }
        let is_mentioned = is_everyone || mentioned.iter().any(|id| id == &user_id);
        if mode == "all" || is_mentioned {
            recipients.push(user_id);
        }
    }
    Ok(recipients)
}

/// Generate a VAPID keypair for `.env`. Exposed as a CLI subcommand so an
/// operator never has to hunt for an online key generator and paste their
/// private key into it.
pub fn generate_vapid_keypair() -> (String, String) {
    use jwt_simple::algorithms::{ECDSAP256PublicKeyLike, ES256KeyPair};
    let keypair = ES256KeyPair::generate();
    // VAPID wants the uncompressed SEC1 point (65 bytes, 0x04-prefixed).
    let public = keypair.public_key().public_key().to_bytes_uncompressed();
    (base64_url(&public), base64_url(&keypair.to_bytes()))
}

fn base64_url(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn payload_preview(content: &str, attachments: usize) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return if attachments > 0 {
            format!("Sent {} attachment(s)", attachments)
        } else {
            "Sent a message".to_string()
        };
    }
    let preview: String = trimmed.chars().take(140).collect();
    if trimmed.chars().count() > 140 {
        format!("{preview}…")
    } else {
        preview
    }
}
