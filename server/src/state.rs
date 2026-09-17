use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use tokio::sync::{broadcast, RwLock};

use crate::config::Config;
use crate::models::VoiceState;

/// Who should receive a gateway event. Each connection filters the broadcast
/// stream against its own viewer, so a private channel never leaks.
#[derive(Clone, Debug)]
pub enum Scope {
    /// Everyone connected.
    All,
    /// Only members who can view this channel.
    Channel(String),
    /// A single user (all of their connections).
    User(String),
    /// Members holding a permission bit, e.g. audit log entries.
    Permission(i64),
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    #[serde(rename = "t")]
    pub kind: String,
    #[serde(rename = "d")]
    pub data: Value,
    #[serde(skip)]
    pub scope: Scope,
}

impl Event {
    pub fn new(kind: &str, data: Value, scope: Scope) -> Self {
        Self {
            kind: kind.to_string(),
            data,
            scope,
        }
    }
}

pub struct Inner {
    pub db: SqlitePool,
    pub config: Config,
    pub events: broadcast::Sender<Event>,
    /// user_id -> number of live gateway connections. Presence is derived from
    /// this so a closed laptop lid doesn't leave someone stuck "online".
    pub connections: RwLock<HashMap<String, usize>>,
    /// user_id -> current voice state.
    pub voice: RwLock<HashMap<String, VoiceState>>,
    /// Cached answer from the desktop release lookup.
    pub desktop_release: RwLock<Option<crate::desktop::CachedRelease>>,
}

#[derive(Clone)]
pub struct AppState(pub Arc<Inner>);

impl Deref for AppState {
    type Target = Inner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AppState {
    pub fn new(db: SqlitePool, config: Config) -> Self {
        let (events, _) = broadcast::channel(1024);
        Self(Arc::new(Inner {
            db,
            config,
            events,
            connections: RwLock::new(HashMap::new()),
            voice: RwLock::new(HashMap::new()),
            desktop_release: RwLock::new(None),
        }))
    }

    /// Publish an event. A send error just means nobody is listening.
    pub fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }

    pub fn emit_all(&self, kind: &str, data: Value) {
        self.emit(Event::new(kind, data, Scope::All));
    }

    pub fn emit_channel(&self, channel_id: &str, kind: &str, data: Value) {
        self.emit(Event::new(
            kind,
            data,
            Scope::Channel(channel_id.to_string()),
        ));
    }

    pub fn emit_user(&self, user_id: &str, kind: &str, data: Value) {
        self.emit(Event::new(kind, data, Scope::User(user_id.to_string())));
    }

    pub async fn voice_states(&self) -> Vec<VoiceState> {
        self.voice.read().await.values().cloned().collect()
    }

    pub async fn is_online(&self, user_id: &str) -> bool {
        self.connections
            .read()
            .await
            .get(user_id)
            .is_some_and(|n| *n > 0)
    }
}
