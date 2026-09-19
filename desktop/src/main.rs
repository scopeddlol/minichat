// MiniChat desktop shell.
//
// MiniChat is self-hosted, so there is no URL to bake in at build time. The
// app asks which instance to connect to on first launch, remembers it, and
// from then on opens straight into that instance.
//
// The window is frameless: the titlebar, the minimise/maximise/close buttons
// and the menus all live in the app itself, so the desktop app looks like
// MiniChat rather than like a browser in a box. Driving a frameless window
// needs IPC from the page, which the instance origin is granted explicitly
// (see `grant_instance_ipc`) — a tight, named set of window commands and
// nothing else.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::window::Color;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

#[cfg(windows)]
mod capture;
#[cfg(windows)]
mod capture_audio;
#[cfg(windows)]
mod win;

/// Window commands the connected instance is allowed to call.
///
/// This is the whole IPC surface a remote page gets: move the window it is
/// already inside, and nothing else. No filesystem, no shell, no process, no
/// arbitrary events. `internal-toggle-maximize` and `start-dragging` are what
/// Tauri's own `data-tauri-drag-region` handler invokes, so the titlebar can
/// be dragged and double-clicked like a real one.
const INSTANCE_WINDOW_PERMISSIONS: &[&str] = &[
    "core:window:allow-minimize",
    "core:window:allow-toggle-maximize",
    "core:window:allow-internal-toggle-maximize",
    "core:window:allow-is-maximized",
    "core:window:allow-start-dragging",
    "core:window:allow-close",
];

#[derive(Serialize, Deserialize)]
struct Settings {
    instance_url: Option<String>,
    /// Global voice hotkeys, in Tauri accelerator syntax (e.g. "F8",
    /// "Control+Shift+M"). Unlike the in-app bindings these keep working while
    /// MiniChat is in the background, which is the point of push-to-talk.
    #[serde(default = "default_hotkeys")]
    hotkeys: Hotkeys,
    /// Whether closing the window hides it to the tray instead of quitting.
    #[serde(default = "default_close_to_tray")]
    close_to_tray: bool,
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
fn default_close_to_tray() -> bool {
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
            close_to_tray: default_close_to_tray(),
        }
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("could not resolve the config directory: {e}"))?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("could not create the config directory: {e}"))?;
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

/// The scheme/host/port of an instance URL, with any path, query or fragment
/// dropped.
///
/// Everything that is granted — IPC, camera, microphone — is keyed on this, so
/// it has to be the origin and not the URL the user happened to type. It is
/// also what gets handed to Tauri as a URL pattern, and Tauri panics on a
/// malformed one, so it is built by the URL parser rather than by string
/// surgery.
fn instance_origin(url: &str) -> Option<String> {
    let parsed = tauri::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let origin = parsed.origin();
    if !origin.is_tuple() {
        return None;
    }
    Some(origin.ascii_serialization())
}

/// Whether `url` is a page served by `origin`.
///
/// The check is on the origin, not a prefix of the URL:
/// `https://chat.example.com/channel/x` is the instance,
/// `https://chat.example.com.evil.test/` is not. Used to decide which pages
/// get the camera and the microphone without being asked.
// Only WebView2 has a prompt to answer, but the predicate is the security-
// relevant part, so it lives here where every platform's tests can run it.
#[cfg_attr(not(windows), allow(dead_code))]
fn origin_matches(url: &str, origin: &str) -> bool {
    url == origin
        || url
            .strip_prefix(origin)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn granted_origins() -> &'static Mutex<HashSet<String>> {
    static GRANTED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    GRANTED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Let the connected instance drive its own window.
///
/// A capability is added at runtime rather than declared in
/// `capabilities/*.json` because the instance URL is only known once the user
/// has typed it — a self-hosted app has no domain to ship. The grant is scoped
/// to one origin and to the commands in `INSTANCE_WINDOW_PERMISSIONS`; a
/// different instance gets its own grant, and a page from anywhere else gets
/// nothing.
fn grant_instance_ipc(app: &AppHandle, origin: &str) {
    use tauri::ipc::CapabilityBuilder;

    let mut granted = match granted_origins().lock() {
        Ok(granted) => granted,
        Err(poisoned) => poisoned.into_inner(),
    };
    if !granted.insert(origin.to_string()) {
        return;
    }

    let mut capability = CapabilityBuilder::new(format!("instance-{origin}"))
        .remote(origin.to_string())
        // The bundled connect screen has its own capability file; this one is
        // only for the remote instance.
        .local(false)
        .window("main");
    for permission in INSTANCE_WINDOW_PERMISSIONS {
        capability = capability.permission(*permission);
    }

    if let Err(e) = app.add_capability(capability) {
        eprintln!("minichat: could not grant window controls to {origin}: {e}");
    }
}

/// Whether the window is currently showing the app's own bundled page rather
/// than a remote instance.
///
/// `withGlobalTauri` has to be on for the bundler-less connect screen to reach
/// `invoke`, and that injects the API into every page the webview loads — the
/// remote instance page included. Capabilities are scoped per origin, but
/// rather than depend on that, the commands below check for themselves. Cheap,
/// and it holds regardless of how the ACL treats application commands.
///
/// Deliberately permissive when the URL can't be read: failing closed here
/// would brick the connect screen, which is worse than what this guards
/// against (a hostile instance page changing which instance opens next).
fn is_app_page(window: &WebviewWindow) -> bool {
    match window.url() {
        Ok(url) => {
            // tauri://localhost on macOS and Linux, http://tauri.localhost on
            // Windows, and a localhost dev server if one is ever used.
            url.scheme() == "tauri"
                || matches!(
                    url.host_str(),
                    Some("tauri.localhost") | Some("localhost") | Some("127.0.0.1") | None
                )
        }
        Err(_) => true,
    }
}

fn require_app_page(app: &AppHandle) -> Result<WebviewWindow, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window is missing".to_string())?;
    if !is_app_page(&window) {
        return Err("That action is only available on the connect screen.".into());
    }
    Ok(window)
}

#[tauri::command]
fn saved_instance(app: AppHandle) -> Option<String> {
    require_app_page(&app).ok()?;
    load_settings(&app).instance_url
}

/// Forward a hotkey into the web app.
///
/// Pushing a DOM event in from Rust rather than emitting a Tauri event keeps
/// the IPC grant to window controls alone: the page never needs permission to
/// listen for app events.
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
    use std::str::FromStr;
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

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
    // Checked before anything is written, so a rejected call leaves no trace.
    require_app_page(&app)?;

    let url = normalise(&url)?;
    let origin = instance_origin(&url)
        .ok_or_else(|| "That doesn't look like a valid address.".to_string())?;

    let mut settings = load_settings(&app);
    settings.instance_url = Some(url.clone());
    store_settings(&app, &settings)?;

    let target = tauri::Url::parse(&url)
        .map(WebviewUrl::External)
        .map_err(|_| "That doesn't look like a valid address.".to_string())?;

    // Rebuilt rather than navigated: the window controls and the permission
    // handler are both scoped to one origin, and they are wired up when the
    // webview is created.
    open_main_window(&app, target, Some(origin))
        .map_err(|e| format!("could not open that instance: {e}"))?;
    register_hotkeys(&app);
    Ok(url)
}

/// Open the main window, replacing it if one already exists.
///
/// Going through `WebviewUrl::App` rather than a literal URL matters for the
/// bundled page: the scheme Tauri serves it from differs by platform
/// (`tauri://localhost` on macOS and Linux, `http://tauri.localhost` on
/// Windows), so any hardcoded address is wrong somewhere.
fn open_main_window(
    app: &AppHandle,
    target: WebviewUrl,
    origin: Option<String>,
) -> tauri::Result<()> {
    if let Some(origin) = &origin {
        grant_instance_ipc(app, origin);
    }

    if let Some(existing) = app.get_webview_window("main") {
        #[cfg(windows)]
        capture::cancel();
        existing.destroy()?;
    }

    let builder = WebviewWindowBuilder::new(app, "main", target);
    #[cfg(windows)]
    let builder = builder
        .initialization_script("window.__MINICHAT_NATIVE_CAPTURE__ = true;")
        .on_navigation(|_| {
            capture::cancel();
            true
        });
    let window = builder
        .title("MiniChat")
        .inner_size(1180.0, 820.0)
        .min_inner_size(420.0, 520.0)
        .center()
        // Frameless: MiniChat draws its own titlebar. The window keeps its
        // shadow and its resize edges, it just has no Windows chrome.
        .decorations(false)
        .shadow(true)
        // Without this the window flashes white before the app paints, which
        // on a dark theme is the most obvious "this is a webview" tell there
        // is. Matches `--bg` in the dark theme.
        .background_color(Color(14, 16, 22, 255))
        // The web app looks for this to offer desktop-specific help.
        .user_agent(&format!(
            "Mozilla/5.0 MiniChat/{} Desktop",
            env!("CARGO_PKG_VERSION")
        ))
        .build()?;

    // Only WebView2 has a permission prompt to answer. The other platforms
    // build (developers run this on Linux and macOS) but only Windows ships.
    let _ = (&window, &origin);
    #[cfg(windows)]
    if let Some(origin) = origin {
        win::auto_grant_permissions(&window, origin);
    }

    Ok(())
}

fn show_connect_screen(app: &AppHandle) -> Result<(), String> {
    open_main_window(app, WebviewUrl::App("index.html".into()), None).map_err(|e| e.to_string())
}

/// Bring the window back from the tray (or from being minimised).
fn reveal(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        let _ = show_connect_screen(app);
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// Forget the instance so the next launch asks again, keeping everything else.
fn forget_instance(app: &AppHandle) {
    let mut settings = load_settings(app);
    settings.instance_url = None;
    let _ = store_settings(app, &settings);
    let _ = show_connect_screen(app);
}

/// The tray icon, which is also the app's only menu.
///
/// There is no window menu bar on purpose: a Win32 menu strip on a frameless
/// window is exactly the "default Windows stuff" this shell exists to avoid.
/// Everything that used to live there is here, plus the two things only the
/// tray can do — reopen a hidden window, and quit for real.
fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let settings = load_settings(app);

    let open = MenuItem::with_id(app, "open", "Open MiniChat", true, None::<&str>)?;
    let switch = MenuItem::with_id(app, "switch", "Switch instance…", true, None::<&str>)?;
    let hotkeys = CheckMenuItem::with_id(
        app,
        "hotkeys",
        "Global voice hotkeys",
        true,
        settings.hotkeys.enabled,
        None::<&str>,
    )?;
    let close_to_tray = CheckMenuItem::with_id(
        app,
        "close_to_tray",
        "Close to tray",
        true,
        settings.close_to_tray,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit MiniChat", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &open,
            &PredefinedMenuItem::separator(app)?,
            &switch,
            &hotkeys,
            &close_to_tray,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut tray = TrayIconBuilder::with_id("main");
    // Bundled at compile time from `icons/`, so this is only ever `None` in a
    // build with no icon configured — which would be a packaging mistake, not
    // a reason to start without a tray.
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.tooltip("MiniChat")
        .menu(&menu)
        // The menu belongs on right-click, the way every other tray icon on
        // Windows behaves; left-click reopens the window.
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            let left_click = matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            );
            if left_click {
                reveal(tray.app_handle());
            }
        })
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => reveal(app),
            "switch" => forget_instance(app),
            "hotkeys" => {
                let mut settings = load_settings(app);
                settings.hotkeys.enabled = !settings.hotkeys.enabled;
                let enabled = settings.hotkeys.enabled;
                let _ = store_settings(app, &settings);
                register_hotkeys(app);
                let _ = hotkeys.set_checked(enabled);
            }
            "close_to_tray" => {
                let mut settings = load_settings(app);
                settings.close_to_tray = !settings.close_to_tray;
                let enabled = settings.close_to_tray;
                let _ = store_settings(app, &settings);
                let _ = close_to_tray.set_checked(enabled);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

/// Hide instead of quitting when the window is closed.
///
/// Only once an instance is open: hiding the connect screen to the tray would
/// leave a first-run user with an app that appears not to have started.
fn on_window_event(window: &WebviewWindow, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        let app = window.app_handle();
        if load_settings(app).close_to_tray && !is_app_page(window) {
            api.prevent_close();
            let _ = window.hide();
        }
    }
}

fn main() {
    let builder =
        tauri::Builder::default().plugin(tauri_plugin_global_shortcut::Builder::new().build());
    #[cfg(windows)]
    let builder = builder.invoke_handler(tauri::generate_handler![
        saved_instance,
        connect,
        capture::choose_capture,
        capture::capture_sources,
        capture::capture_thumbnail,
        capture::capture_select,
        capture::capture_frame,
        capture::capture_audio,
        capture::stop_capture
    ]);
    #[cfg(not(windows))]
    let builder = builder.invoke_handler(tauri::generate_handler![saved_instance, connect]);
    builder
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let Some(webview) = window.get_webview_window("main") {
                    on_window_event(&webview, event);
                }
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let saved = load_settings(&handle).instance_url;

            // Open straight into the saved instance; otherwise show the local
            // connect screen bundled with the app.
            let saved = saved.as_deref().and_then(|url| normalise(url).ok());
            let origin = saved.as_deref().and_then(instance_origin);
            let target = match (&saved, &origin) {
                (Some(url), Some(_)) => tauri::Url::parse(url)
                    .map(WebviewUrl::External)
                    .unwrap_or_else(|_| WebviewUrl::App("index.html".into())),
                _ => WebviewUrl::App("index.html".into()),
            };

            open_main_window(&handle, target, origin)?;

            build_tray(&handle)?;
            register_hotkeys(&handle);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to start MiniChat");
}

#[cfg(test)]
mod tests {
    use super::{instance_origin, normalise, origin_matches};

    #[test]
    fn normalise_defaults_to_https_and_trims() {
        assert_eq!(
            normalise(" chat.example.com/ ").unwrap(),
            "https://chat.example.com"
        );
        assert_eq!(
            normalise("http://localhost:8080").unwrap(),
            "http://localhost:8080"
        );
        assert!(normalise("   ").is_err());
    }

    #[test]
    fn origin_drops_everything_after_the_host() {
        assert_eq!(
            instance_origin("https://chat.example.com/invite/abc?x=1#y").as_deref(),
            Some("https://chat.example.com")
        );
        assert_eq!(
            instance_origin("http://localhost:8080/").as_deref(),
            Some("http://localhost:8080")
        );
    }

    #[test]
    fn origin_matches_the_instance_and_its_pages() {
        let origin = "https://chat.example.com";
        assert!(origin_matches("https://chat.example.com", origin));
        assert!(origin_matches("https://chat.example.com/", origin));
        assert!(origin_matches(
            "https://chat.example.com/invite/abc",
            origin
        ));
    }

    #[test]
    fn origin_rejects_lookalike_hosts() {
        let origin = "https://chat.example.com";
        assert!(!origin_matches(
            "https://chat.example.com.evil.test/",
            origin
        ));
        assert!(!origin_matches("https://chat.example.community/", origin));
        assert!(!origin_matches("http://chat.example.com/", origin));
        assert!(!origin_matches("https://evil.test/", origin));
        assert!(!origin_matches("", origin));
    }

    #[test]
    fn origin_rejects_non_http_schemes() {
        assert!(instance_origin("file:///etc/passwd").is_none());
        assert!(instance_origin("javascript:alert(1)").is_none());
        assert!(instance_origin("not a url").is_none());
    }
}
