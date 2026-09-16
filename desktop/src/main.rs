// MiniChat desktop shell.
//
// MiniChat is self-hosted, so there is no URL to bake in at build time. The
// app asks which instance to connect to on first launch, remembers it, and
// from then on opens straight into that instance.

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Serialize, Deserialize)]
struct Settings {
    instance_url: Option<String>,
    /// Global voice hotkeys, in Tauri accelerator syntax (e.g. "F8",
    /// "Control+Shift+M"). Unlike the in-app bindings these keep working while
    /// MiniChat is in the background, which is the point of push-to-talk.
    #[serde(default = "default_hotkeys")]
    hotkeys: Hotkeys,
}

#[derive(Serialize, Deserialize, Clone)]
struct Hotkeys {
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default = "default_ptt")]
    push_to_talk: String,
    #[serde(default = "default_mute")]
    mute: String,
    #[serde(default = "default_deafen")]
    deafen: String,
}

fn default_enabled() -> bool {
    true
}
fn default_ptt() -> String {
    "F8".into()
}
fn default_mute() -> String {
    "F9".into()
}
fn default_deafen() -> String {
    "F10".into()
}
fn default_hotkeys() -> Hotkeys {
    Hotkeys {
        enabled: default_enabled(),
        push_to_talk: default_ptt(),
        mute: default_mute(),
        deafen: default_deafen(),
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            instance_url: None,
            hotkeys: default_hotkeys(),
        }
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("could not resolve the config directory: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create the config directory: {e}"))?;
    Ok(dir.join("settings.json"))
}

fn load_settings(app: &AppHandle) -> Settings {
    settings_path(app)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn store_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app)?;
    let body = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, body).map_err(|e| format!("could not save settings: {e}"))
}

/// Only http(s) URLs are ever accepted, so a stored value can't turn into a
/// `file://` or custom-scheme navigation.
fn normalise(url: &str) -> Result<String, String> {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("Enter your instance address.".into());
    }
    let full = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    if full.starts_with("http://") || full.starts_with("https://") {
        Ok(full)
    } else {
        Err("The address must start with https://".into())
    }
}

#[tauri::command]
fn saved_instance(app: AppHandle) -> Option<String> {
    load_settings(&app).instance_url
}

/// Forward a hotkey into the web app.
///
/// The instance page is a remote origin with no IPC access, by design. Pushing
/// a DOM event in from Rust keeps that boundary intact while still letting the
/// app react to an OS-level key.
fn forward_hotkey(app: &AppHandle, action: &str, active: bool) {
    if let Some(window) = app.get_webview_window("main") {
        let script = format!(
            "window.dispatchEvent(new CustomEvent('minichat:hotkey',{{detail:{{action:'{action}',active:{active}}}}}))"
        );
        let _ = window.eval(&script);
    }
}

/// Register the configured global shortcuts.
///
/// Every failure here is non-fatal: a shortcut another application already
/// owns should cost you that one key, not the whole app.
fn register_hotkeys(app: &AppHandle) {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
    use std::str::FromStr;

    let settings = load_settings(app);
    let hotkeys = settings.hotkeys;
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();

    if !hotkeys.enabled {
        return;
    }

    for (accelerator, action) in [
        (hotkeys.push_to_talk.as_str(), "ptt"),
        (hotkeys.mute.as_str(), "mute"),
        (hotkeys.deafen.as_str(), "deafen"),
    ] {
        if accelerator.is_empty() {
            continue;
        }
        let Ok(shortcut) = Shortcut::from_str(accelerator) else {
            eprintln!("minichat: could not parse the hotkey '{accelerator}'");
            continue;
        };
        let action = action.to_string();
        let handle = app.clone();
        let result = shortcuts.on_shortcut(shortcut, move |_app, _shortcut, event| {
            match (action.as_str(), event.state()) {
                // Push-to-talk is a hold: open on press, close on release.
                ("ptt", ShortcutState::Pressed) => forward_hotkey(&handle, "ptt", true),
                ("ptt", ShortcutState::Released) => forward_hotkey(&handle, "ptt", false),
                // The toggles only act once, on press.
                (other, ShortcutState::Pressed) => forward_hotkey(&handle, other, true),
                _ => {}
            }
        });
        if let Err(e) = result {
            eprintln!("minichat: could not register '{accelerator}': {e}");
        }
    }
}

#[tauri::command]
fn connect(app: AppHandle, url: String) -> Result<String, String> {
    let url = normalise(&url)?;
    let mut settings = load_settings(&app);
    settings.instance_url = Some(url.clone());
    store_settings(&app, &settings)?;

    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window is missing".to_string())?;
    let parsed = url
        .parse()
        .map_err(|_| "That doesn't look like a valid address.".to_string())?;
    window
        .navigate(parsed)
        .map_err(|e| format!("could not open that instance: {e}"))?;
    register_hotkeys(&app);
    Ok(url)
}

fn show_connect_screen(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let url = "tauri://localhost/index.html"
            .parse()
            .map_err(|_| "invalid local url".to_string())?;
        window.navigate(url).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn build_menu(app: &AppHandle) -> tauri::Result<()> {
    let switch = MenuItem::with_id(app, "switch", "Switch instance…", true, None::<&str>)?;
    let reload = MenuItem::with_id(app, "reload", "Reload", true, Some("CmdOrCtrl+R"))?;
    let hotkeys = MenuItem::with_id(
        app,
        "hotkeys",
        "Toggle global voice hotkeys",
        true,
        None::<&str>,
    )?;
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &switch,
            &reload,
            &hotkeys,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let menu = Menu::with_items(app, &[&file, &edit])?;
    app.set_menu(menu)?;

    app.on_menu_event(move |app, event| match event.id().as_ref() {
        "switch" => {
            // Forget the instance so the next launch asks again too, but keep
            // the user's hotkey configuration.
            let hotkeys = load_settings(app).hotkeys;
            let _ = store_settings(
                app,
                &Settings {
                    instance_url: None,
                    hotkeys,
                },
            );
            let _ = show_connect_screen(app);
        }
        "hotkeys" => {
            let mut settings = load_settings(app);
            settings.hotkeys.enabled = !settings.hotkeys.enabled;
            let enabled = settings.hotkeys.enabled;
            let _ = store_settings(app, &settings);
            register_hotkeys(app);
            if let Some(window) = app.get_webview_window("main") {
                let message = if enabled {
                    "Global voice hotkeys enabled"
                } else {
                    "Global voice hotkeys disabled"
                };
                let _ = window.eval(&format!("console.info('minichat: {message}')"));
            }
        }
        "reload" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval("window.location.reload()");
            }
        }
        _ => {}
    });
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![saved_instance, connect])
        .setup(|app| {
            let handle = app.handle().clone();
            let saved = load_settings(&handle).instance_url;

            // Open straight into the saved instance; otherwise show the local
            // connect screen bundled with the app.
            let target = match saved.as_deref().map(normalise) {
                Some(Ok(url)) => url
                    .parse()
                    .map(WebviewUrl::External)
                    .unwrap_or_else(|_| WebviewUrl::App("index.html".into())),
                _ => WebviewUrl::App("index.html".into()),
            };

            WebviewWindowBuilder::new(app, "main", target)
                .title("MiniChat")
                .inner_size(1180.0, 820.0)
                .min_inner_size(420.0, 520.0)
                .center()
                // The web app looks for this to offer desktop-specific help.
                .user_agent(&format!(
                    "Mozilla/5.0 MiniChat/{} Desktop",
                    env!("CARGO_PKG_VERSION")
                ))
                .build()?;

            build_menu(&handle)?;
            register_hotkeys(&handle);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to start MiniChat");
}
