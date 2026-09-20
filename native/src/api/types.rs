//! Wire types, mirroring `web/src/lib/types.ts`.
//!
//! Fields are carried whether or not the client reads them yet: this is the
//! shape of the API, and a partial mirror is the kind of thing that quietly
//! drops a field when the feature that needs it lands. Hence the allow.
#![allow(dead_code)]
//!
//! The server is the same one the web client talks to and its JSON is not
//! versioned, so every field is either optional or carries a default: a server
//! newer than this client must not be able to break deserialisation of the
//! fields that *are* understood. `deny_unknown_fields` is deliberately absent
//! for the same reason.

use serde::{Deserialize, Serialize};

fn default_zoom() -> f64 {
    1.0
}

fn centre() -> f64 {
    50.0
}

/// How a member framed one of their images: the focal point as a percentage
/// and a zoom factor, applied at render time so re-cropping needs no upload.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ImageFrame {
    #[serde(default = "centre")]
    pub x: f64,
    #[serde(default = "centre")]
    pub y: f64,
    #[serde(default = "default_zoom")]
    pub zoom: f64,
}

impl Default for ImageFrame {
    fn default() -> Self {
        Self {
            x: 50.0,
            y: 50.0,
            zoom: 1.0,
        }
    }
}

impl ImageFrame {
    /// Whether the frame is the default one, in which case the renderer can
    /// take the plain cover path.
    pub fn is_centred(&self) -> bool {
        (self.x - 50.0).abs() < f64::EPSILON
            && (self.y - 50.0).abs() < f64::EPSILON
            && (self.zoom - 1.0).abs() < f64::EPSILON
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Presence {
    Online,
    Idle,
    Dnd,
    #[default]
    Offline,
}

impl Presence {
    pub fn label(self) -> &'static str {
        match self {
            Self::Online => "Online",
            Self::Idle => "Idle",
            Self::Dnd => "Do not disturb",
            Self::Offline => "Offline",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelKind {
    #[default]
    Text,
    Voice,
    Announcement,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Member {
    pub id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub banner_url: Option<String>,
    #[serde(default)]
    pub bio: String,
    #[serde(default)]
    pub pronouns: String,
    #[serde(default)]
    pub favorite_game: String,
    #[serde(default)]
    pub accent_color: String,
    #[serde(default)]
    pub custom_status: String,
    #[serde(default)]
    pub presence: Presence,
    #[serde(default)]
    pub is_operator: bool,
    #[serde(default)]
    pub is_suspended: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub last_seen_at: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub avatar_frame: ImageFrame,
    #[serde(default)]
    pub banner_frame: ImageFrame,
}

impl Member {
    /// The name to show. Falls back through the username so a member with an
    /// empty display name never renders as a blank row.
    pub fn name(&self) -> &str {
        if !self.display_name.is_empty() {
            &self.display_name
        } else if !self.username.is_empty() {
            &self.username
        } else {
            "Unknown member"
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Me {
    #[serde(flatten)]
    pub member: Member,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub accepted_rules: bool,
    /// A decimal string: the flags run past 2^53 once ADMINISTRATOR is set,
    /// so the server sends them as text and they are parsed, never floated.
    #[serde(default)]
    pub permissions: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Role {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub permissions: i64,
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub hoist: bool,
    #[serde(default)]
    pub mentionable: bool,
    #[serde(default)]
    pub icon_url: Option<String>,
    /// A short text badge shown beside the name, e.g. "MOD".
    #[serde(default)]
    pub badge: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Category {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub position: i64,
    /// Hides the category and every channel synced to it.
    #[serde(default)]
    pub is_private: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Channel {
    pub id: String,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub kind: ChannelKind,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub slowmode: i64,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub user_limit: i64,
    #[serde(default)]
    pub created_at: String,
    /// Shown in place of the # / speaker glyph.
    #[serde(default)]
    pub emoji: String,
    /// A short line under the channel name in the sidebar.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub sync_category: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Attachment {
    pub id: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub width: Option<i64>,
    #[serde(default)]
    pub height: Option<i64>,
    #[serde(default)]
    pub url: String,
}

impl Attachment {
    pub fn is_image(&self) -> bool {
        self.content_type.starts_with("image/")
    }
    pub fn is_video(&self) -> bool {
        self.content_type.starts_with("video/")
    }
    pub fn is_audio(&self) -> bool {
        self.content_type.starts_with("audio/")
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReactionGroup {
    #[serde(default)]
    pub emoji: String,
    #[serde(default)]
    pub count: i64,
    /// Whether the signed-in member is among them.
    #[serde(default)]
    pub me: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MessageAuthor {
    pub id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub accent_color: String,
    #[serde(default)]
    pub is_operator: bool,
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Message {
    pub id: String,
    #[serde(default)]
    pub channel_id: String,
    #[serde(default)]
    pub author: Option<MessageAuthor>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub reply_to_id: Option<String>,
    #[serde(default)]
    pub system_kind: Option<String>,
    #[serde(default)]
    pub webhook_name: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub edited_at: Option<String>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub reactions: Vec<ReactionGroup>,

    // Local-only, set while a message is in flight. Never sent by the server.
    #[serde(skip)]
    pub pending: bool,
    #[serde(skip)]
    pub failed: bool,
}

impl Message {
    pub fn author_name(&self) -> &str {
        if let Some(name) = self.webhook_name.as_deref() {
            return name;
        }
        match self.author.as_ref() {
            Some(author) if !author.display_name.is_empty() => &author.display_name,
            Some(author) if !author.username.is_empty() => &author.username,
            _ => "Deleted member",
        }
    }

    pub fn author_id(&self) -> Option<&str> {
        self.author.as_ref().map(|a| a.id.as_str())
    }

    pub fn is_system(&self) -> bool {
        self.system_kind.as_deref().is_some_and(|k| !k.is_empty())
    }
}

/// Whether two consecutive messages render as one group.
/// Mirrors `shouldGroup` in `web/src/lib/format.ts`.
pub fn should_group(previous: Option<&Message>, current: &Message) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    if previous.is_system() || current.is_system() {
        return false;
    }
    if previous.webhook_name != current.webhook_name {
        return false;
    }
    if previous.author_id() != current.author_id() {
        return false;
    }
    if current.reply_to_id.is_some() {
        return false;
    }
    let gap = crate::format::parse_timestamp(&current.created_at)
        - crate::format::parse_timestamp(&previous.created_at);
    // Five minutes, the same window the web client uses.
    (0..5 * 60).contains(&gap)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RegistrationMode {
    #[default]
    Invite,
    Open,
    Closed,
}

fn default_radius() -> f64 {
    10.0
}

/// The branding fields, shared by `/api/meta` and the gateway's READY.
#[derive(Clone, Debug, Deserialize)]
pub struct Instance {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub tagline: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub banner_url: Option<String>,
    #[serde(default)]
    pub accent_color: String,
    #[serde(default)]
    pub rules: String,
    #[serde(default)]
    pub welcome_message: String,
    #[serde(default)]
    pub setup_complete: bool,
    #[serde(default)]
    pub registration_mode: RegistrationMode,
    #[serde(default)]
    pub require_rules_accept: bool,
    #[serde(default)]
    pub theme_mode: ThemeMode,
    #[serde(default)]
    pub surface_tint: Option<String>,
    #[serde(default = "default_radius")]
    pub corner_radius: f64,
    #[serde(default)]
    pub font_family: String,
    #[serde(default)]
    pub custom_css: String,
    #[serde(default)]
    pub login_headline: String,
    #[serde(default)]
    pub login_body: String,
    #[serde(default)]
    pub login_image_url: Option<String>,
    #[serde(default)]
    pub max_upload_mb: i64,
    // Present on /api/meta only.
    #[serde(default)]
    pub member_count: i64,
    #[serde(default)]
    pub voice_enabled: bool,
    #[serde(default)]
    pub setup_token_required: bool,
    #[serde(default)]
    pub version: String,
    /// Only `/api/admin/instance` carries this, so it is empty until an
    /// administrator has fetched that. The forward link uses it.
    #[serde(default)]
    pub public_url: String,
}

impl Default for Instance {
    fn default() -> Self {
        Self {
            name: "MiniChat".into(),
            tagline: String::new(),
            description: String::new(),
            icon_url: None,
            banner_url: None,
            accent_color: "#5b6ee8".into(),
            rules: String::new(),
            welcome_message: String::new(),
            setup_complete: true,
            registration_mode: RegistrationMode::Invite,
            require_rules_accept: false,
            theme_mode: ThemeMode::Dark,
            surface_tint: None,
            corner_radius: 10.0,
            font_family: String::new(),
            custom_css: String::new(),
            login_headline: String::new(),
            login_body: String::new(),
            login_image_url: None,
            max_upload_mb: 0,
            member_count: 0,
            voice_enabled: false,
            setup_token_required: false,
            version: String::new(),
            public_url: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct VoiceState {
    pub user_id: String,
    #[serde(default)]
    pub channel_id: String,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub deafened: bool,
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub video: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Emoji {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationMode {
    #[default]
    All,
    Mentions,
    None,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ChannelNotification {
    pub channel_id: String,
    #[serde(default)]
    pub mode: NotificationMode,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct NotificationPreferences {
    #[serde(default)]
    pub mode: NotificationMode,
    #[serde(default)]
    pub channels: Vec<ChannelNotification>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelationshipKind {
    Friend,
    Outgoing,
    Incoming,
    Blocked,
    #[default]
    None,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Relationship {
    pub user_id: String,
    #[serde(default)]
    pub kind: RelationshipKind,
    /// Private to me; the other member is never told.
    #[serde(default)]
    pub favourite: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReadStateEntry {
    pub channel_id: String,
    #[serde(default)]
    pub last_read_id: String,
}

/// The gateway's opening frame: everything needed to paint the app.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReadyPayload {
    #[serde(default)]
    pub me: Me,
    #[serde(default)]
    pub instance: Instance,
    #[serde(default)]
    pub roles: Vec<Role>,
    #[serde(default)]
    pub categories: Vec<Category>,
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub members: Vec<Member>,
    #[serde(default)]
    pub voice_states: Vec<VoiceState>,
    #[serde(default)]
    pub read_state: Vec<ReadStateEntry>,
    #[serde(default)]
    pub unread: std::collections::HashMap<String, i64>,
    #[serde(default)]
    pub mentions: std::collections::HashMap<String, i64>,
    #[serde(default)]
    pub emojis: Vec<Emoji>,
    #[serde(default)]
    pub notifications: NotificationPreferences,
    #[serde(default)]
    pub permissions: String,
    #[serde(default)]
    pub channel_permissions: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub voice_enabled: bool,
    #[serde(default)]
    pub livekit_url: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    #[serde(default)]
    pub user_id: String,
}

// --- direct messages ----------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Conversation {
    pub id: String,
    #[serde(default)]
    pub peer_id: String,
    #[serde(default)]
    pub last_content: Option<String>,
    #[serde(default)]
    pub unread: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct DirectMessage {
    pub id: String,
    #[serde(default)]
    pub conversation_id: String,
    #[serde(default)]
    pub author_id: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub created_at: String,

    /// Local-only, while a message is in flight.
    #[serde(skip)]
    pub pending: bool,
    #[serde(skip)]
    pub failed: bool,
}

impl DirectMessage {
    /// A direct message as an ordinary one, so the message list can draw it
    /// without a second renderer. The conversation is the channel, and the
    /// author is resolved from the member list by the view builder.
    pub fn as_message(&self, author: Option<&Member>) -> Message {
        Message {
            id: self.id.clone(),
            channel_id: self.conversation_id.clone(),
            author: author.map(|member| MessageAuthor {
                id: member.id.clone(),
                username: member.username.clone(),
                display_name: member.display_name.clone(),
                avatar_url: member.avatar_url.clone(),
                accent_color: member.accent_color.clone(),
                is_operator: member.is_operator,
                roles: member.roles.clone(),
            }),
            content: self.content.clone(),
            created_at: self.created_at.clone(),
            pending: self.pending,
            failed: self.failed,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_member_survives_a_response_missing_everything_optional() {
        let member: Member = serde_json::from_str(r#"{"id":"1"}"#).unwrap();
        assert_eq!(member.id, "1");
        assert_eq!(member.presence, Presence::Offline);
        assert!(member.avatar_frame.is_centred());
    }

    #[test]
    fn unknown_fields_from_a_newer_server_are_ignored() {
        let channel: Channel =
            serde_json::from_str(r#"{"id":"c","name":"general","invented_later":42}"#).unwrap();
        assert_eq!(channel.name, "general");
        assert_eq!(channel.kind, ChannelKind::Text);
    }

    #[test]
    fn the_display_name_falls_back_to_the_username() {
        let member = Member {
            username: "ada".into(),
            ..Default::default()
        };
        assert_eq!(member.name(), "ada");
    }

    #[test]
    fn a_webhook_name_wins_over_a_missing_author() {
        let message = Message {
            webhook_name: Some("CI".into()),
            ..Default::default()
        };
        assert_eq!(message.author_name(), "CI");
        let orphan = Message::default();
        assert_eq!(orphan.author_name(), "Deleted member");
    }

    #[test]
    fn me_flattens_the_member_fields() {
        let me: Me =
            serde_json::from_str(r#"{"id":"1","username":"ada","permissions":"1073741824"}"#)
                .unwrap();
        assert_eq!(me.member.username, "ada");
        assert_eq!(me.permissions, "1073741824");
    }
}

// --- direct calls -------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallStatus {
    Ringing,
    Accepted,
    #[default]
    Ended,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct DirectCall {
    pub id: String,
    #[serde(default)]
    pub conversation_id: String,
    #[serde(default)]
    pub caller_id: String,
    #[serde(default)]
    pub status: CallStatus,
    #[serde(default)]
    pub created_at: String,
}

impl DirectCall {
    pub fn is_live(&self) -> bool {
        matches!(self.status, CallStatus::Ringing | CallStatus::Accepted)
    }
}

// --- administration ------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize)]
pub struct BanEntry {
    pub user_id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub banned_by: Option<String>,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Invite {
    pub code: String,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub role_id: Option<String>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub max_uses: i64,
    #[serde(default)]
    pub uses: i64,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub revoked: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    #[serde(default)]
    pub actor_id: Option<String>,
    #[serde(default)]
    pub actor_name: Option<String>,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub target_type: String,
    #[serde(default)]
    pub target_id: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ActivityDay {
    #[serde(default)]
    pub day: String,
    #[serde(default)]
    pub count: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct TopChannel {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub count: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Stats {
    #[serde(default)]
    pub members: i64,
    #[serde(default)]
    pub online: i64,
    #[serde(default)]
    pub in_voice: i64,
    #[serde(default)]
    pub messages: i64,
    #[serde(default)]
    pub channels: i64,
    #[serde(default)]
    pub roles: i64,
    #[serde(default)]
    pub invites: i64,
    #[serde(default)]
    pub bans: i64,
    #[serde(default)]
    pub storage_bytes: i64,
    #[serde(default)]
    pub joined_last_week: i64,
    #[serde(default)]
    pub messages_last_week: i64,
    #[serde(default)]
    pub activity: Vec<ActivityDay>,
    #[serde(default)]
    pub top_channels: Vec<TopChannel>,
    #[serde(default)]
    pub voice_enabled: bool,
    #[serde(default)]
    pub public_url: String,
    #[serde(default)]
    pub version: String,
}
