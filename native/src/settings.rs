//! What survives a restart.
//!
//! Kept next to the Tauri shell's own `settings.json`, in the platform config
//! directory, so a member who moves between the two clients is not asked for
//! their instance address twice.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    Dark,
    Light,
    /// Follow the instance's own default, which is what an unset choice means
    /// in the web client too.
    #[default]
    Instance,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Settings {
    /// The instance opened on launch. `None` shows the connect screen.
    #[serde(default)]
    pub instance_url: Option<String>,

    /// The session token.
    ///
    /// Stored so the app opens signed in, which is what a desktop client is
    /// expected to do — the web client keeps the same value in localStorage,
    /// which is also plain text on disk. The file is written user-only where
    /// the platform supports it (see `store`). It is *not* in the OS
    /// credential store; that is worth doing and is noted in the README as
    /// outstanding.
    #[serde(default)]
    pub token: Option<String>,

    #[serde(default)]
    pub theme: ThemeChoice,

    /// Categories the member has collapsed, by ID.
    #[serde(default)]
    pub collapsed_categories: Vec<String>,

    #[serde(default = "default_true")]
    pub members_open: bool,

    #[serde(default)]
    pub last_channel: Option<String>,

    /// Whether closing the window hides it to the tray instead of quitting.
    /// The same setting the Tauri shell keeps, under the same name.
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
}

fn default_true() -> bool {
    true
}

// Written out rather than derived: `#[derive(Default)]` uses each field's own
// default and ignores the serde attributes, so a derived `members_open` would
// be false while a missing field in the file reads as true. The two have to
// agree, or a first run and a partial file disagree about the same setting.
impl Default for Settings {
    fn default() -> Self {
        Self {
            instance_url: None,
            token: None,
            theme: ThemeChoice::default(),
            collapsed_categories: Vec::new(),
            members_open: default_true(),
            last_channel: None,
            close_to_tray: default_true(),
        }
    }
}

fn directory() -> Option<PathBuf> {
    directories::ProjectDirs::from("chat", "mini", "MiniChat")
        .map(|dirs| dirs.config_dir().to_path_buf())
}

pub fn path() -> Option<PathBuf> {
    directory().map(|dir| dir.join("native.json"))
}

impl Settings {
    pub fn load() -> Self {
        let Some(path) = path() else {
            return Self::default();
        };
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            // A settings file from a newer version, or a corrupt one, must
            // not stop the app starting — it starts at the connect screen
            // instead, which is recoverable.
            .unwrap_or_default()
    }

    pub fn store(&self) -> Result<(), String> {
        let Some(path) = path() else {
            return Err("could not resolve the config directory".into());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("could not create the config directory: {e}"))?;
        }

        let body = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, body).map_err(|e| format!("could not save settings: {e}"))?;

        // The file holds a session token, so it is read/write for the owner
        // and nothing else. Windows has no equivalent mode; there the config
        // directory is already per-user.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    /// Forget the session but keep the instance, for signing out.
    pub fn sign_out(&mut self) {
        self.token = None;
        self.last_channel = None;
    }

    /// Forget the instance entirely, for switching to another one.
    pub fn forget_instance(&mut self) {
        self.instance_url = None;
        self.sign_out();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_struct_default_and_an_empty_file_agree() {
        // The two must not drift: one is what a first run gets, the other is
        // what a file missing every optional field gets.
        let empty: Settings = serde_json::from_str("{}").unwrap();
        let fresh = Settings::default();
        assert_eq!(empty.members_open, fresh.members_open);
        assert_eq!(empty.close_to_tray, fresh.close_to_tray);
        assert_eq!(empty.theme, fresh.theme);
        assert_eq!(empty.instance_url, fresh.instance_url);
    }

    #[test]
    fn defaults_are_a_first_run() {
        let settings = Settings::default();
        assert!(settings.instance_url.is_none());
        assert!(settings.token.is_none());
        assert_eq!(settings.theme, ThemeChoice::Instance);
        assert!(settings.members_open);
    }

    #[test]
    fn a_partial_file_keeps_the_defaults_for_what_is_missing() {
        let settings: Settings =
            serde_json::from_str(r#"{"instance_url":"https://chat.example.com"}"#).unwrap();
        assert_eq!(
            settings.instance_url.as_deref(),
            Some("https://chat.example.com")
        );
        assert!(
            settings.members_open,
            "an absent field must not read as false"
        );
        assert_eq!(settings.theme, ThemeChoice::Instance);
    }

    #[test]
    fn an_unknown_field_from_a_newer_version_is_ignored() {
        let settings: Settings =
            serde_json::from_str(r#"{"theme":"light","invented_later":true}"#).unwrap();
        assert_eq!(settings.theme, ThemeChoice::Light);
    }

    #[test]
    fn signing_out_keeps_the_instance_but_drops_the_session() {
        let mut settings = Settings {
            instance_url: Some("https://chat.example.com".into()),
            token: Some("secret".into()),
            last_channel: Some("general".into()),
            ..Default::default()
        };
        settings.sign_out();
        assert!(settings.token.is_none());
        assert!(settings.last_channel.is_none());
        assert!(settings.instance_url.is_some());

        settings.forget_instance();
        assert!(settings.instance_url.is_none());
    }

    #[test]
    fn a_round_trip_preserves_every_field() {
        let original = Settings {
            instance_url: Some("https://chat.example.com".into()),
            token: Some("t".into()),
            theme: ThemeChoice::Dark,
            collapsed_categories: vec!["c1".into()],
            members_open: false,
            last_channel: Some("general".into()),
            close_to_tray: false,
        };
        let restored: Settings =
            serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
        assert_eq!(restored.instance_url, original.instance_url);
        assert_eq!(restored.collapsed_categories, original.collapsed_categories);
        assert!(!restored.members_open);
        assert_eq!(restored.theme, ThemeChoice::Dark);
    }
}
