//! LiveKit access-token minting. LiveKit accepts a plain HS256 JWT whose
//! claims describe the grant, so no server SDK is needed.

use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;

use crate::error::{AppError, AppResult};

#[derive(Serialize)]
struct VideoGrant {
    room: String,
    #[serde(rename = "roomJoin")]
    room_join: bool,
    #[serde(rename = "canPublish")]
    can_publish: bool,
    #[serde(rename = "canSubscribe")]
    can_subscribe: bool,
    #[serde(rename = "canPublishData")]
    can_publish_data: bool,
    #[serde(rename = "canPublishSources")]
    can_publish_sources: Vec<String>,
}

#[derive(Serialize)]
struct LivekitClaims {
    exp: i64,
    nbf: i64,
    iss: String,
    sub: String,
    name: String,
    metadata: String,
    video: VideoGrant,
}

pub struct Grant<'a> {
    pub room: &'a str,
    pub identity: &'a str,
    pub display_name: &'a str,
    pub metadata: String,
    pub can_speak: bool,
    pub can_video: bool,
    pub can_screen_share: bool,
}

pub fn mint_token(api_key: &str, api_secret: &str, grant: Grant<'_>) -> AppResult<String> {
    if api_key.is_empty() || api_secret.is_empty() {
        return Err(AppError::bad(
            "Voice and video are not configured on this instance.",
        ));
    }

    let mut sources = Vec::new();
    if grant.can_speak {
        sources.push("microphone".to_string());
    }
    if grant.can_video {
        sources.push("camera".to_string());
    }
    if grant.can_screen_share {
        sources.push("screen_share".to_string());
        sources.push("screen_share_audio".to_string());
    }

    let now = Utc::now();
    let claims = LivekitClaims {
        // Tokens only need to survive the connection handshake; the session
        // itself continues after expiry.
        exp: (now + Duration::hours(6)).timestamp(),
        nbf: now.timestamp() - 10,
        iss: api_key.to_string(),
        sub: grant.identity.to_string(),
        name: grant.display_name.to_string(),
        metadata: grant.metadata,
        video: VideoGrant {
            room: grant.room.to_string(),
            room_join: true,
            can_publish: !sources.is_empty(),
            can_subscribe: true,
            can_publish_data: true,
            can_publish_sources: sources,
        },
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(api_secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("livekit token signing failed: {e}")))
}
