//! The MiniChat HTTP API, mirroring `web/src/lib/api.ts`.
//!
//! The endpoints are covered as a set rather than one at a time as screens
//! need them, so the client is a complete description of the API and adding
//! a screen is not also an exercise in adding a request.
#![allow(dead_code)]

pub mod types;

use std::sync::Arc;
use std::time::Duration;

use reqwest::{header, Method, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::json;

use types::*;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The request never reached the server.
    #[error("Could not reach the server. Check your connection.")]
    Unreachable,
    /// The session is gone; the UI signs out rather than retrying.
    #[error("Sign in again. Your session was rejected.")]
    Unauthorised,
    #[error("{0}")]
    Server(String),
    #[error("{0}")]
    Invalid(String),
}

impl ApiError {
    pub fn is_unauthorised(&self) -> bool {
        matches!(self, Self::Unauthorised)
    }
}

pub type Result<T> = std::result::Result<T, ApiError>;

/// Only `https`, plus `http` on the loopback so a developer can run against a
/// local server. A stored value can therefore never become a `file://` or a
/// custom-scheme navigation — the same rule the Tauri shell applies.
pub fn normalise_origin(input: &str) -> Result<String> {
    let trimmed = input.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(ApiError::Invalid("Enter your instance address.".into()));
    }
    // A bare host gets `https://` put in front of it, which is the whole
    // point of accepting "chat.example.com". But that must not happen to
    // input that already declares a scheme: prefixing `file:///etc/passwd`
    // yields `https://file:///etc/passwd`, which parses cleanly with `file`
    // as the host and would be accepted. So any declared scheme other than
    // http(s) is refused before the prefix is considered.
    let declared_scheme = trimmed.split_once(':').and_then(|(head, _)| {
        let mut chars = head.chars();
        let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c));
        valid.then(|| head.to_ascii_lowercase())
    });

    let candidate = match declared_scheme.as_deref() {
        Some("http") | Some("https") => trimmed.to_string(),
        Some(other) => {
            return Err(ApiError::Invalid(format!(
                "The address must start with https:// — {other}:// is not an instance."
            )))
        }
        None => format!("https://{trimmed}"),
    };

    let url = url::Url::parse(&candidate)
        .map_err(|_| ApiError::Invalid("That doesn't look like a valid address.".into()))?;

    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    match url.scheme() {
        "https" => {}
        "http" if loopback => {}
        "http" => {
            return Err(ApiError::Invalid(
                "The address must start with https:// — voice and uploads need it.".into(),
            ))
        }
        _ => {
            return Err(ApiError::Invalid(
                "The address must start with https://".into(),
            ))
        }
    }
    if url.host_str().is_none() {
        return Err(ApiError::Invalid(
            "That doesn't look like a valid address.".into(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ApiError::Invalid(
            "Remove the username and password from the address.".into(),
        ));
    }

    let mut origin = format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default());
    if let Some(port) = url.port() {
        origin.push_str(&format!(":{port}"));
    }
    Ok(origin)
}

/// The `wss://` address of the gateway for an origin.
pub fn gateway_url(origin: &str) -> String {
    let scheme = if origin.starts_with("https://") {
        "wss"
    } else {
        "ws"
    };
    let host = origin
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(origin);
    format!("{scheme}://{host}/api/gateway")
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    origin: Arc<str>,
    token: Arc<std::sync::RwLock<Option<String>>>,
}

impl Client {
    pub fn new(origin: &str) -> Result<Self> {
        let origin = normalise_origin(origin)?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(25))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("MiniChat/", env!("CARGO_PKG_VERSION"), " Native"))
            // The API is same-origin by design. A redirect would mean the
            // instance is sending the bearer token somewhere else, which is
            // never something to follow silently.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| ApiError::Invalid("Could not start the HTTP client.".into()))?;

        Ok(Self {
            http,
            origin: origin.into(),
            token: Arc::new(std::sync::RwLock::new(None)),
        })
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn token(&self) -> Option<String> {
        self.token.read().ok().and_then(|t| t.clone())
    }

    pub fn set_token(&self, token: Option<String>) {
        if let Ok(mut slot) = self.token.write() {
            *slot = token;
        }
    }

    /// Absolute URL for a path the server handed back.
    ///
    /// Upload and emoji URLs come through as instance-relative, so they are
    /// resolved against the origin; anything already absolute is left alone.
    pub fn absolute(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else if let Some(rest) = path.strip_prefix('/') {
            format!("{}/{rest}", self.origin)
        } else {
            format!("{}/{path}", self.origin)
        }
    }

    async fn send<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<T> {
        let mut request = self
            .http
            .request(method, format!("{}/api{path}", self.origin));

        if let Some(token) = self.token() {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        if let Some(body) = body {
            request = request.json(&body);
        }

        let response = request.send().await.map_err(|_| ApiError::Unreachable)?;
        let status = response.status();

        if status == StatusCode::NO_CONTENT {
            // `()` and other unit-like shapes deserialise from null.
            return serde_json::from_value(serde_json::Value::Null)
                .map_err(|e| ApiError::Server(e.to_string()));
        }

        let text = response.text().await.unwrap_or_default();

        if !status.is_success() {
            if status == StatusCode::UNAUTHORIZED {
                return Err(ApiError::Unauthorised);
            }
            let detail = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_owned))
                .unwrap_or_else(|| format!("Request failed ({})", status.as_u16()));
            return Err(ApiError::Server(detail));
        }

        serde_json::from_str(&text)
            .map_err(|e| ApiError::Server(format!("The instance sent something unexpected: {e}")))
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.send(Method::GET, path, None).await
    }
    async fn post<T: DeserializeOwned>(&self, path: &str, body: serde_json::Value) -> Result<T> {
        self.send(Method::POST, path, Some(body)).await
    }
    async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.send(Method::POST, path, None).await
    }
    async fn patch<T: DeserializeOwned>(&self, path: &str, body: serde_json::Value) -> Result<T> {
        self.send(Method::PATCH, path, Some(body)).await
    }
    async fn put<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.send(Method::PUT, path, None).await
    }
    async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.send(Method::DELETE, path, None).await
    }

    // --- instance ------------------------------------------------------

    pub async fn meta(&self) -> Result<Instance> {
        self.get("/meta").await
    }

    // --- auth ----------------------------------------------------------

    pub async fn login(&self, username: &str, password: &str) -> Result<AuthResponse> {
        self.post(
            "/auth/login",
            json!({ "username": username, "password": password }),
        )
        .await
    }

    pub async fn register(
        &self,
        username: &str,
        password: &str,
        display_name: &str,
        invite: Option<&str>,
        accept_rules: bool,
    ) -> Result<AuthResponse> {
        self.post(
            "/auth/register",
            json!({
                "username": username,
                "password": password,
                "display_name": display_name,
                "invite": invite,
                "accept_rules": accept_rules,
            }),
        )
        .await
    }

    pub async fn me(&self) -> Result<Me> {
        self.get("/auth/me").await
    }

    pub async fn change_password(&self, current: &str, new: &str) -> Result<AuthResponse> {
        self.post(
            "/auth/password",
            json!({ "current_password": current, "new_password": new }),
        )
        .await
    }

    // --- members and channels ------------------------------------------

    pub async fn members(&self) -> Result<Vec<Member>> {
        self.get("/members").await
    }

    pub async fn channels(&self) -> Result<Vec<Channel>> {
        self.get("/channels").await
    }

    pub async fn categories(&self) -> Result<Vec<Category>> {
        self.get("/categories").await
    }

    pub async fn update_me(&self, payload: serde_json::Value) -> Result<Me> {
        self.patch("/users/@me", payload).await
    }

    pub async fn user(&self, id: &str) -> Result<Member> {
        self.get(&format!("/users/{}", encode(id))).await
    }

    // --- messages ------------------------------------------------------

    pub async fn messages(
        &self,
        channel: &str,
        before: Option<&str>,
        around: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Message>> {
        let mut query = format!("?limit={limit}");
        if let Some(before) = before {
            query.push_str(&format!("&before={}", encode(before)));
        }
        if let Some(around) = around {
            query.push_str(&format!("&around={}", encode(around)));
        }
        self.get(&format!("/channels/{}/messages{query}", encode(channel)))
            .await
    }

    pub async fn send_message(
        &self,
        channel: &str,
        content: &str,
        reply_to: Option<&str>,
    ) -> Result<Message> {
        self.post(
            &format!("/channels/{}/messages", encode(channel)),
            json!({ "content": content, "reply_to_id": reply_to }),
        )
        .await
    }

    pub async fn edit_message(&self, id: &str, content: &str) -> Result<Message> {
        self.patch(
            &format!("/messages/{}", encode(id)),
            json!({ "content": content }),
        )
        .await
    }

    pub async fn delete_message(&self, id: &str) -> Result<serde_json::Value> {
        self.delete(&format!("/messages/{}", encode(id))).await
    }

    pub async fn pin_message(&self, id: &str, pinned: bool) -> Result<Message> {
        let path = format!("/messages/{}/pin", encode(id));
        if pinned {
            self.put(&path).await
        } else {
            self.delete(&path).await
        }
    }

    pub async fn pins(&self, channel: &str) -> Result<Vec<Message>> {
        self.get(&format!("/channels/{}/pins", encode(channel)))
            .await
    }

    pub async fn react(&self, id: &str, emoji: &str, on: bool) -> Result<serde_json::Value> {
        let path = format!("/messages/{}/reactions/{}", encode(id), encode(emoji));
        if on {
            self.put(&path).await
        } else {
            self.delete(&path).await
        }
    }

    pub async fn ack(&self, channel: &str, message: &str) -> Result<serde_json::Value> {
        self.post(
            &format!("/channels/{}/ack", encode(channel)),
            json!({ "message_id": message }),
        )
        .await
    }

    pub async fn search(&self, query: &str, channel: Option<&str>) -> Result<Vec<Message>> {
        let mut path = format!("/search?q={}", encode(query));
        if let Some(channel) = channel {
            path.push_str(&format!("&channel_id={}", encode(channel)));
        }
        self.get(&path).await
    }

    // --- emoji, relationships, notifications ---------------------------

    pub async fn emojis(&self) -> Result<Vec<Emoji>> {
        self.get("/emojis").await
    }

    pub async fn relationships(&self) -> Result<Vec<Relationship>> {
        self.get("/relationships").await
    }

    pub async fn notification_preferences(&self) -> Result<NotificationPreferences> {
        self.get("/notifications").await
    }

    pub async fn update_notifications(
        &self,
        payload: serde_json::Value,
    ) -> Result<NotificationPreferences> {
        self.patch("/notifications", payload).await
    }

    // --- voice ---------------------------------------------------------

    pub async fn voice_states(&self) -> Result<Vec<VoiceState>> {
        self.get("/voice/states").await
    }

    pub async fn channel_permissions(&self) -> Result<std::collections::HashMap<String, String>> {
        self.get("/channels/permissions").await
    }

    /// Fetch bytes for an avatar, emoji or attachment.
    ///
    /// Capped rather than unbounded: an instance is trusted to serve the app,
    /// not to decide how much memory the client spends on one image.
    pub async fn fetch_bytes(&self, url: &str, max: usize) -> Result<Vec<u8>> {
        let response = self
            .http
            .get(self.absolute(url))
            .send()
            .await
            .map_err(|_| ApiError::Unreachable)?;
        if !response.status().is_success() {
            return Err(ApiError::Server(format!(
                "Could not load {} ({})",
                url,
                response.status().as_u16()
            )));
        }
        if let Some(length) = response.content_length() {
            if length as usize > max {
                return Err(ApiError::Invalid(
                    "That file is too large to display.".into(),
                ));
            }
        }
        let bytes = response.bytes().await.map_err(|_| ApiError::Unreachable)?;
        if bytes.len() > max {
            return Err(ApiError::Invalid(
                "That file is too large to display.".into(),
            ));
        }
        Ok(bytes.to_vec())
    }
}

/// Percent-encode a path segment, so an ID or an emoji with a slash in it
/// cannot walk out of the route it belongs to.
fn encode(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_is_assumed_and_trailing_slashes_go() {
        assert_eq!(
            normalise_origin(" chat.example.com/ ").unwrap(),
            "https://chat.example.com"
        );
        assert_eq!(
            normalise_origin("https://chat.example.com/invite/x").unwrap(),
            "https://chat.example.com"
        );
    }

    #[test]
    fn plain_http_is_only_allowed_on_the_loopback() {
        assert_eq!(
            normalise_origin("http://localhost:8080").unwrap(),
            "http://localhost:8080"
        );
        assert_eq!(
            normalise_origin("http://127.0.0.1:8080").unwrap(),
            "http://127.0.0.1:8080"
        );
        assert!(normalise_origin("http://chat.example.com").is_err());
    }

    #[test]
    fn other_schemes_and_empty_input_are_refused() {
        assert!(normalise_origin("file:///etc/passwd").is_err());
        assert!(normalise_origin("javascript:alert(1)").is_err());
        assert!(normalise_origin("   ").is_err());
    }

    #[test]
    fn credentials_in_the_address_are_refused() {
        // An address like https://user:pass@host is a phishing shape, and
        // would put the credentials in a stored setting.
        assert!(normalise_origin("https://someone:secret@chat.example.com").is_err());
    }

    #[test]
    fn the_gateway_follows_the_origins_scheme() {
        assert_eq!(
            gateway_url("https://chat.example.com"),
            "wss://chat.example.com/api/gateway"
        );
        assert_eq!(
            gateway_url("http://localhost:8080"),
            "ws://localhost:8080/api/gateway"
        );
    }

    #[test]
    fn relative_urls_resolve_against_the_instance() {
        let client = Client::new("https://chat.example.com").unwrap();
        assert_eq!(
            client.absolute("/uploads/a.png"),
            "https://chat.example.com/uploads/a.png"
        );
        assert_eq!(
            client.absolute("https://cdn.example.com/a.png"),
            "https://cdn.example.com/a.png"
        );
    }

    #[test]
    fn path_segments_are_encoded() {
        // An emoji reaction is a path segment and can contain anything.
        assert_eq!(encode("a/b"), "a%2Fb");
        assert_eq!(encode("👍"), "%F0%9F%91%8D");
    }
}
