//! Native capture consent belongs to a bundled window, never to instance HTML.
use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tokio::sync::oneshot;

#[derive(Clone)]
enum Source {
    Monitor(u32),
    Window { id: u32, pid: u32 },
}
impl Source {
    fn image(&self) -> Result<image::RgbaImage, String> {
        match self {
            Self::Monitor(id) => xcap::Monitor::all()
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|m| m.id().ok() == Some(*id))
                .ok_or("Display disconnected")?
                .capture_image(),
            Self::Window { id, pid } => xcap::Window::all()
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|w| {
                    w.id().ok() == Some(*id)
                        && w.pid().ok() == Some(*pid)
                        && !w.is_minimized().unwrap_or(true)
                })
                .ok_or("Window closed or minimized")?
                .capture_image(),
        }
        .map_err(|e| e.to_string())
    }
}
#[derive(Clone, Serialize)]
pub struct Selection {
    session: u64,
    name: String,
    audio: bool,
}
#[derive(Serialize)]
pub struct SourceInfo {
    id: usize,
    name: String,
    kind: &'static str,
}
#[derive(Default)]
struct CaptureState {
    generation: u64,
    sources: Vec<(SourceInfo, Source)>,
    selected: Option<Source>,
    origin: String,
    touched: Option<Instant>,
    pending: Option<oneshot::Sender<Result<Selection, String>>>,
    audio: Option<crate::capture_audio::AudioFeed>,
}
fn state() -> &'static Mutex<CaptureState> {
    static STATE: OnceLock<Mutex<CaptureState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(CaptureState::default()))
}
fn clear(state: &mut CaptureState) {
    state.generation += 1;
    state.selected = None;
    if let Some(audio) = state.audio.take() {
        audio.stop();
    }
    state.sources.clear();
    if let Some(pending) = state.pending.take() {
        let _ = pending.send(Err("Capture cancelled".into()));
    }
}
pub fn cancel() {
    if let Ok(mut state) = state().lock() {
        clear(&mut state);
    }
}
fn cancel_generation(generation: u64) -> bool {
    if let Ok(mut state) = state().lock() {
        if state.generation == generation {
            clear(&mut state);
            return true;
        }
    }
    false
}
fn picker_location_allowed(label: &str, url: &tauri::Url) -> bool {
    label == "share-picker"
        && ((url.scheme() == "tauri" && url.host_str() == Some("localhost"))
            || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost")))
        && url.path() == "/share-picker.html"
}
fn require_picker(window: &WebviewWindow) -> Result<(), String> {
    let url = window.url().map_err(|e| e.to_string())?;
    if picker_location_allowed(window.label(), &url) {
        Ok(())
    } else {
        Err("Only the native picker can choose a source.".into())
    }
}
fn require_instance(window: &WebviewWindow, app: &AppHandle) -> Result<String, String> {
    let origin = crate::load_settings(app)
        .instance_url
        .as_deref()
        .and_then(crate::instance_origin)
        .ok_or("No connected instance")?;
    let url = window.url().map_err(|e| e.to_string())?;
    if window.label() != "main" || !crate::origin_matches(url.as_str(), &origin) {
        return Err("Capture is only available to the connected instance.".into());
    }
    Ok(origin)
}

#[tauri::command]
pub async fn choose_capture(window: WebviewWindow, app: AppHandle) -> Result<Selection, String> {
    let origin = require_instance(&window, &app)?;
    let (sender, receiver) = oneshot::channel();
    let generation = {
        let mut state = state().lock().map_err(|e| e.to_string())?;
        if state.pending.is_some() || state.selected.is_some() {
            return Err("A screen share is already open.".into());
        }
        state.generation += 1;
        state.origin = origin;
        state.pending = Some(sender);
        state.generation
    };
    let built = WebviewWindowBuilder::new(
        &app,
        "share-picker",
        WebviewUrl::App("share-picker.html".into()),
    )
    .title("Share your screen · MiniChat")
    .inner_size(820.0, 620.0)
    .min_inner_size(420.0, 350.0)
    .center()
    .decorations(false)
    .always_on_top(true)
    .build();
    let picker = match built {
        Ok(picker) => picker,
        Err(error) => {
            cancel_generation(generation);
            return Err(error.to_string());
        }
    };
    picker.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            cancel_generation(generation);
        }
    });
    if state().lock().map_err(|e| e.to_string())?.generation != generation {
        let _ = picker.destroy();
        return Err("Capture cancelled".into());
    }
    receiver
        .await
        .map_err(|_| "Capture cancelled".to_string())?
}

#[tauri::command]
pub async fn capture_sources(window: WebviewWindow) -> Result<Vec<SourceInfo>, String> {
    require_picker(&window)?;
    let generation = state().lock().map_err(|e| e.to_string())?.generation;
    let sources = tauri::async_runtime::spawn_blocking(|| {
        let mut found = Vec::new();
        for monitor in xcap::Monitor::all().map_err(|e| e.to_string())? {
            let name = monitor.friendly_name().unwrap_or_else(|_| "Display".into());
            found.push((
                name,
                "screen",
                Source::Monitor(monitor.id().map_err(|e| e.to_string())?),
            ));
        }
        for window in xcap::Window::all().map_err(|e| e.to_string())? {
            if window.pid().ok() == Some(std::process::id())
                || window.is_minimized().unwrap_or(true)
            {
                continue;
            }
            let name = window.title().unwrap_or_default();
            if !name.trim().is_empty() {
                found.push((
                    name,
                    "window",
                    Source::Window {
                        id: window.id().map_err(|e| e.to_string())?,
                        pid: window.pid().map_err(|e| e.to_string())?,
                    },
                ));
            }
        }
        Ok::<_, String>(found)
    })
    .await
    .map_err(|e| e.to_string())??;
    let mut state = state().lock().map_err(|e| e.to_string())?;
    if state.generation != generation || state.pending.is_none() {
        return Err("Picker closed".into());
    }
    state.sources = sources
        .into_iter()
        .enumerate()
        .map(|(id, (name, kind, source))| (SourceInfo { id, name, kind }, source))
        .collect();
    Ok(state
        .sources
        .iter()
        .map(|(s, _)| SourceInfo {
            id: s.id,
            name: s.name.clone(),
            kind: s.kind,
        })
        .collect())
}

fn jpeg(source: Source, height: u32) -> Result<Vec<u8>, String> {
    let image = image::DynamicImage::ImageRgba8(source.image()?);
    let height = height.clamp(180, 2160);
    let image = image.thumbnail(3840, height).to_rgb8();
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 85)
        .encode_image(&image)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

#[tauri::command]
pub async fn capture_thumbnail(
    window: WebviewWindow,
    id: usize,
) -> Result<tauri::ipc::Response, String> {
    require_picker(&window)?;
    let source = {
        let state = state().lock().map_err(|e| e.to_string())?;
        if state.pending.is_none() {
            return Err("Picker closed".into());
        }
        state
            .sources
            .get(id)
            .map(|(_, s)| s.clone())
            .ok_or("Source no longer available")?
    };
    let bytes = tauri::async_runtime::spawn_blocking(move || jpeg(source, 180))
        .await
        .map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub async fn capture_select(window: WebviewWindow, id: usize, audio: bool) -> Result<(), String> {
    require_picker(&window)?;
    let (generation, source, name) = {
        let state = state().lock().map_err(|e| e.to_string())?;
        if state.pending.is_none() {
            return Err("Picker closed".into());
        }
        let (info, source) = state.sources.get(id).ok_or("Source no longer available")?;
        (state.generation, source.clone(), info.name.clone())
    };
    let feed = if audio {
        // Screen audio excludes MiniChat's process tree to prevent call echoes;
        // window audio includes only the chosen application's process tree.
        let (pid, include) = match &source {
            Source::Monitor(_) => (std::process::id(), false),
            Source::Window { pid, .. } => (*pid, true),
        };
        Some(
            tauri::async_runtime::spawn_blocking(move || crate::capture_audio::start(pid, include))
                .await
                .map_err(|e| e.to_string())??,
        )
    } else {
        None
    };
    let mut state = state().lock().map_err(|e| e.to_string())?;
    if state.generation != generation {
        if let Some(feed) = feed {
            feed.stop();
        }
        return Err("Picker closed".into());
    }
    let selection = Selection {
        session: state.generation,
        name,
        audio,
    };
    let pending = state.pending.take().ok_or("Picker already closed")?;
    state.selected = Some(source);
    state.audio = feed;
    state.touched = Some(Instant::now());
    state.sources.clear();
    let _ = pending.send(Ok(selection));
    drop(state);
    let _ = window.set_min_size(Some(tauri::LogicalSize::new(420.0, 250.0)));
    let _ = window.set_size(tauri::LogicalSize::new(460.0, 290.0));
    if let Ok(Some(monitor)) = window.current_monitor() {
        let at = monitor.position();
        let height = (310.0 * monitor.scale_factor()) as i32;
        let _ = window.set_position(tauri::PhysicalPosition::new(
            at.x + 20,
            at.y + monitor.size().height as i32 - height,
        ));
    }
    if let Ok(hwnd) = window.hwnd() {
        // SAFETY: this is our live picker window. Excluding its indicator avoids
        // covering the content the user chose to share on a whole display.
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowDisplayAffinity(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::WDA_EXCLUDEFROMCAPTURE,
            );
        }
    }
    let indicator = window.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        let expired = {
            let Ok(state) = crate::capture::state().lock() else {
                return;
            };
            if state.generation != generation {
                return;
            }
            state
                .touched
                .is_none_or(|t| t.elapsed() > Duration::from_secs(15))
        };
        if expired {
            if cancel_generation(generation) {
                let _ = indicator.destroy();
            }
            return;
        }
    });
    Ok(())
}

#[tauri::command]
pub fn capture_audio(
    window: WebviewWindow,
    app: AppHandle,
    session: u64,
) -> Result<tauri::ipc::Response, String> {
    let origin = require_instance(&window, &app)?;
    let state = state().lock().map_err(|e| e.to_string())?;
    if state.generation != session || state.origin != origin || state.selected.is_none() {
        return Err("Capture ended".into());
    }
    Ok(tauri::ipc::Response::new(
        state
            .audio
            .as_ref()
            .ok_or("Audio was not selected")?
            .read()?,
    ))
}

#[tauri::command]
pub async fn capture_frame(
    window: WebviewWindow,
    app: AppHandle,
    session: u64,
    height: u32,
) -> Result<tauri::ipc::Response, String> {
    let origin = require_instance(&window, &app)?;
    let source = {
        let mut state = state().lock().map_err(|e| e.to_string())?;
        if state.generation != session
            || origin != state.origin
            || state
                .touched
                .is_none_or(|t| t.elapsed() > Duration::from_secs(15))
        {
            return Err("Capture ended".into());
        }
        state.touched = Some(Instant::now());
        state.selected.clone().ok_or("Capture ended")?
    };
    let bytes = tauri::async_runtime::spawn_blocking(move || jpeg(source, height))
        .await
        .map_err(|e| e.to_string())??;
    if state().lock().map_err(|e| e.to_string())?.generation != session {
        return Err("Capture ended".into());
    }
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub fn stop_capture(
    window: WebviewWindow,
    app: AppHandle,
    session: Option<u64>,
) -> Result<(), String> {
    if window.label() == "share-picker" {
        require_picker(&window)?;
    } else {
        require_instance(&window, &app)?;
        let state = state().lock().map_err(|e| e.to_string())?;
        if let Some(session) = session {
            if state.generation != session {
                return Ok(());
            }
        } else if state.pending.is_none() {
            return Ok(());
        }
    }
    let picker = app.get_webview_window("share-picker");
    cancel();
    if let Some(picker) = picker {
        let _ = picker.destroy();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_bundled_picker_can_select_sources() {
        for url in [
            "http://tauri.localhost/share-picker.html",
            "tauri://localhost/share-picker.html",
        ] {
            assert!(picker_location_allowed(
                "share-picker",
                &tauri::Url::parse(url).unwrap()
            ));
            assert!(!picker_location_allowed(
                "main",
                &tauri::Url::parse(url).unwrap()
            ));
        }
        for url in [
            "https://chat.example/share-picker.html",
            "http://localhost/share-picker.html",
            "http://tauri.localhost.evil/share-picker.html",
            "http://tauri.localhost/index.html",
        ] {
            assert!(!picker_location_allowed(
                "share-picker",
                &tauri::Url::parse(url).unwrap()
            ));
        }
    }
    #[test]
    fn cancelling_revokes_pending_consent_and_source() {
        let (sender, mut receiver) = oneshot::channel();
        let before;
        {
            let mut state = state().lock().unwrap();
            before = state.generation;
            state.pending = Some(sender);
            state.selected = Some(Source::Monitor(7));
        }
        cancel();
        assert!(receiver.try_recv().unwrap().is_err());
        let state = state().lock().unwrap();
        assert!(state.generation > before);
        assert!(state.selected.is_none());
        assert!(state.sources.is_empty());
    }
}
