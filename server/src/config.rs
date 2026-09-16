use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: String,
    pub data_dir: String,
    pub database_url: String,
    pub jwt_secret: String,
    pub public_url: String,
    pub livekit_url: String,
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    pub web_dir: String,
    /// Optional shared secret required to run the first-run wizard. Set it when
    /// the instance is reachable before you get to it yourself.
    pub setup_token: String,
    /// Web Push (VAPID) keypair. Without it, push notifications stay off and
    /// the rest of the app works unchanged.
    pub vapid_public_key: String,
    pub vapid_private_key: String,
    pub vapid_subject: String,
}

fn var_or(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

impl Config {
    pub fn from_env() -> Self {
        let data_dir = var_or("MINICHAT_DATA_DIR", "./data");
        let database_url = var_or(
            "DATABASE_URL",
            &format!("sqlite://{data_dir}/minichat.db?mode=rwc"),
        );
        let jwt_secret = var_or("JWT_SECRET", "");
        if jwt_secret.len() < 16 {
            // A weak signing key silently undermines every session in the
            // instance, so refuse to boot rather than paper over it.
            panic!(
                "JWT_SECRET must be set to at least 16 characters. \
                 Generate one with: openssl rand -hex 32"
            );
        }
        Self {
            bind_addr: var_or("BIND_ADDR", "0.0.0.0:8080"),
            data_dir,
            database_url,
            jwt_secret,
            public_url: var_or("PUBLIC_URL", "http://localhost:8080")
                .trim_end_matches('/')
                .to_string(),
            livekit_url: var_or("LIVEKIT_URL", ""),
            livekit_api_key: var_or("LIVEKIT_API_KEY", ""),
            livekit_api_secret: var_or("LIVEKIT_API_SECRET", ""),
            web_dir: var_or("WEB_DIR", "./web"),
            setup_token: var_or("SETUP_TOKEN", ""),
            vapid_public_key: var_or("VAPID_PUBLIC_KEY", ""),
            vapid_private_key: var_or("VAPID_PRIVATE_KEY", ""),
            vapid_subject: var_or("VAPID_SUBJECT", "mailto:admin@example.com"),
        }
    }

    pub fn livekit_ready(&self) -> bool {
        !self.livekit_url.is_empty()
            && !self.livekit_api_key.is_empty()
            && !self.livekit_api_secret.is_empty()
    }

    pub fn push_ready(&self) -> bool {
        !self.vapid_public_key.is_empty() && !self.vapid_private_key.is_empty()
    }

    pub fn uploads_dir(&self) -> String {
        format!("{}/uploads", self.data_dir)
    }
}
