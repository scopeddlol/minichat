use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Instance {
    pub name: String,
    pub tagline: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub banner_url: Option<String>,
    pub accent_color: String,
    pub rules: String,
    pub welcome_message: String,
    pub setup_complete: bool,
    pub registration_mode: String,
    pub require_rules_accept: bool,
    pub default_role_id: Option<String>,
    pub system_channel_id: Option<String>,
    pub max_upload_mb: i64,
    pub created_at: String,
    // --- theming ---
    pub theme_mode: String,
    pub surface_tint: Option<String>,
    pub corner_radius: i64,
    pub font_family: String,
    pub custom_css: String,
    // --- the pages outsiders see ---
    pub login_headline: String,
    pub login_body: String,
    pub login_image_url: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub bio: String,
    pub pronouns: String,
    pub favorite_game: String,
    pub accent_color: String,
    pub custom_status: String,
    pub presence: String,
    pub is_operator: bool,
    pub is_suspended: bool,
    pub token_version: i64,
    pub accepted_rules: bool,
    pub created_at: String,
    pub last_seen_at: String,
}

/// What every other member is allowed to see about a user.
#[derive(Debug, Clone, Serialize)]
pub struct PublicUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub bio: String,
    pub pronouns: String,
    pub favorite_game: String,
    pub accent_color: String,
    pub custom_status: String,
    pub presence: String,
    pub is_operator: bool,
    pub is_suspended: bool,
    pub created_at: String,
    pub last_seen_at: String,
    pub roles: Vec<String>,
}

impl UserRow {
    pub fn public(self, roles: Vec<String>) -> PublicUser {
        PublicUser {
            id: self.id,
            username: self.username,
            display_name: self.display_name,
            avatar_url: self.avatar_url,
            banner_url: self.banner_url,
            bio: self.bio,
            pronouns: self.pronouns,
            favorite_game: self.favorite_game,
            accent_color: self.accent_color,
            custom_status: self.custom_status,
            presence: self.presence,
            is_operator: self.is_operator,
            is_suspended: self.is_suspended,
            created_at: self.created_at,
            last_seen_at: self.last_seen_at,
            roles,
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub permissions: i64,
    pub position: i64,
    pub is_default: bool,
    pub hoist: bool,
    pub mentionable: bool,
    pub icon_url: Option<String>,
    pub badge: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub position: i64,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Channel {
    pub id: String,
    pub category_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub topic: String,
    pub position: i64,
    pub slowmode: i64,
    pub is_private: bool,
    pub user_limit: i64,
    pub created_at: String,
    pub emoji: String,
    pub description: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Attachment {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub size: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub url: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct MessageRow {
    pub id: String,
    pub channel_id: String,
    pub author_id: Option<String>,
    pub content: String,
    pub reply_to_id: Option<String>,
    pub system_kind: Option<String>,
    pub webhook_name: Option<String>,
    pub pinned: bool,
    pub created_at: String,
    pub edited_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReactionGroup {
    pub emoji: String,
    pub count: i64,
    pub me: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageAuthor {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub accent_color: String,
    pub is_operator: bool,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: String,
    pub channel_id: String,
    pub author: Option<MessageAuthor>,
    pub content: String,
    pub reply_to_id: Option<String>,
    pub system_kind: Option<String>,
    pub webhook_name: Option<String>,
    pub pinned: bool,
    pub created_at: String,
    pub edited_at: Option<String>,
    pub attachments: Vec<Attachment>,
    pub reactions: Vec<ReactionGroup>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Invite {
    pub code: String,
    pub created_by: Option<String>,
    pub role_id: Option<String>,
    pub note: String,
    pub max_uses: i64,
    pub uses: i64,
    pub expires_at: Option<String>,
    pub revoked: bool,
    pub created_at: String,
}

/// Just the columns needed to decide whether an invite is usable.
#[derive(Debug, Clone, FromRow)]
pub struct InviteStatus {
    pub role_id: Option<String>,
    pub created_by: Option<String>,
    pub max_uses: i64,
    pub uses: i64,
    pub expires_at: Option<String>,
    pub revoked: bool,
}

impl InviteStatus {
    /// `None` when usable, otherwise the reason it isn't.
    pub fn rejection(&self) -> Option<&'static str> {
        if self.revoked {
            return Some("revoked");
        }
        if self.max_uses > 0 && self.uses >= self.max_uses {
            return Some("used_up");
        }
        let expired = self
            .expires_at
            .as_deref()
            .and_then(|e| chrono::NaiveDateTime::parse_from_str(e, "%Y-%m-%d %H:%M:%S").ok())
            .is_some_and(|d| d.and_utc() < chrono::Utc::now());
        if expired {
            return Some("expired");
        }
        None
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Webhook {
    pub id: String,
    pub channel_id: String,
    pub name: String,
    pub token: String,
    pub avatar_url: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Emoji {
    pub id: String,
    pub name: String,
    pub url: String,
    pub created_by: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct AuditEntry {
    pub id: String,
    pub actor_id: Option<String>,
    pub actor_name: Option<String>,
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    pub detail: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Ban {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub reason: String,
    pub banned_by: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceState {
    pub user_id: String,
    pub channel_id: String,
    pub muted: bool,
    pub deafened: bool,
    pub streaming: bool,
    pub video: bool,
}
