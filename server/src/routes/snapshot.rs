//! The READY payload: everything a freshly connected client needs to render
//! the app without a waterfall of follow-up requests.

use serde_json::{json, Value};

use crate::access;
use crate::auth::Auth;
use crate::error::AppResult;
use crate::models::*;
use crate::perms;
use crate::state::AppState;

pub async fn build(state: &AppState, auth: &Auth) -> AppResult<Value> {
    let instance = access::instance(state).await?;
    let roles: Vec<Role> = sqlx::query_as("SELECT * FROM roles ORDER BY position DESC, name")
        .fetch_all(&state.db)
        .await?;
    let categories: Vec<Category> =
        sqlx::query_as("SELECT * FROM categories ORDER BY position, name")
            .fetch_all(&state.db)
            .await?;

    let is_admin = perms::has(auth.permissions, perms::ADMINISTRATOR);
    let visible = access::visible_channel_ids(state, &auth.role_ids, is_admin).await?;
    let all_channels: Vec<Channel> =
        sqlx::query_as("SELECT * FROM channels ORDER BY position, name")
            .fetch_all(&state.db)
            .await?;
    let channels: Vec<Channel> = all_channels
        .into_iter()
        .filter(|c| visible.iter().any(|id| id == &c.id))
        .collect();

    let members = members_payload(state).await?;

    let read_state: Vec<(String, String)> =
        sqlx::query_as("SELECT channel_id, last_read_id FROM read_state WHERE user_id = ?")
            .bind(&auth.user.id)
            .fetch_all(&state.db)
            .await?;

    // Unread counts are cheap enough to compute up front and save the client
    // a request per channel.
    let mut unread = serde_json::Map::new();
    for channel in &channels {
        let last_read = read_state
            .iter()
            .find(|(cid, _)| cid == &channel.id)
            .map(|(_, id)| id.clone())
            .unwrap_or_default();
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE channel_id = ? AND id > ? AND author_id IS NOT ?",
        )
        .bind(&channel.id)
        .bind(&last_read)
        .bind(&auth.user.id)
        .fetch_one(&state.db)
        .await?;
        unread.insert(channel.id.clone(), json!(count));
    }

    let voice_states = state.voice_states().await;

    // Mentions get their own counter: a channel with 40 unread messages and
    // one mention should read very differently from one with 40 and none.
    let mention_counts: serde_json::Map<String, Value> =
        crate::mentions::unread_counts(state, &auth.user.id)
            .await?
            .into_iter()
            .map(|(channel_id, count)| (channel_id, json!(count)))
            .collect();

    let emojis = crate::routes::emojis::all(state).await?;
    let notifications =
        crate::routes::notifications::preferences_payload(state, &auth.user.id).await?;

    let channel_permissions: serde_json::Map<String, Value> =
        access::channel_permission_map(state, auth)
            .await?
            .into_iter()
            // Serialised as strings: these are 64-bit flags and JSON numbers aren't.
            .map(|(id, bits)| (id, json!(bits.to_string())))
            .collect();

    Ok(json!({
        "me": me_payload(state, auth).await?,
        "instance": instance,
        "roles": roles,
        "categories": categories,
        "channels": channels,
        "members": members,
        "voice_states": voice_states,
        "read_state": read_state.iter().map(|(c, m)| json!({"channel_id": c, "last_read_id": m})).collect::<Vec<_>>(),
        "unread": unread,
        "permissions": auth.permissions.to_string(),
        "channel_permissions": channel_permissions,
        "mentions": mention_counts,
        "emojis": emojis,
        "notifications": notifications,
        "push_enabled": state.config.push_ready(),
        "voice_enabled": state.config.livekit_ready(),
        "livekit_url": state.config.livekit_url,
    }))
}

pub async fn me_payload(state: &AppState, auth: &Auth) -> AppResult<Value> {
    let user = auth.user.clone();
    let email = user.email.clone();
    let accepted_rules = user.accepted_rules;
    let public = user.public(auth.role_ids.clone());
    let online = state.is_online(&public.id).await;
    let mut value = serde_json::to_value(public).unwrap_or(Value::Null);
    if let Some(obj) = value.as_object_mut() {
        obj.insert("email".into(), json!(email));
        obj.insert("accepted_rules".into(), json!(accepted_rules));
        obj.insert("permissions".into(), json!(auth.permissions.to_string()));
        if !online {
            obj.insert("presence".into(), json!("online"));
        }
    }
    Ok(value)
}

pub async fn members_payload(state: &AppState) -> AppResult<Vec<Value>> {
    let users: Vec<UserRow> = sqlx::query_as("SELECT * FROM users ORDER BY lower(display_name)")
        .fetch_all(&state.db)
        .await?;
    let pairs: Vec<(String, String)> = sqlx::query_as("SELECT user_id, role_id FROM user_roles")
        .fetch_all(&state.db)
        .await?;
    let connections = state.connections.read().await;

    Ok(users
        .into_iter()
        .map(|u| {
            let roles = pairs
                .iter()
                .filter(|(uid, _)| uid == &u.id)
                .map(|(_, rid)| rid.clone())
                .collect();
            let online = connections.get(&u.id).is_some_and(|n| *n > 0);
            let stored_presence = u.presence.clone();
            let mut public = u.public(roles);
            // Connection count is the source of truth; a stale "online" row
            // after a crash shouldn't outlive the process.
            public.presence = if online {
                if stored_presence == "offline" {
                    "online".to_string()
                } else {
                    stored_presence
                }
            } else {
                "offline".to_string()
            };
            serde_json::to_value(public).unwrap_or(Value::Null)
        })
        .collect())
}
