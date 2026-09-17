// Windows-specific native behaviour.
//
// WebView2 is Edge, and Edge asks before handing a page the camera or the
// microphone. In a browser that prompt is right: the page is untrusted. In the
// desktop app it is noise — the user installed MiniChat and picked the instance
// it connects to, and they already grant the same permission in the web app.
// So the app answers the prompt itself, and only for the instance it is
// connected to.

use tauri::WebviewWindow;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2PermissionRequestedEventArgs, COREWEBVIEW2_PERMISSION_KIND,
    COREWEBVIEW2_PERMISSION_KIND_CAMERA, COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ,
    COREWEBVIEW2_PERMISSION_KIND_MICROPHONE, COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS,
    COREWEBVIEW2_PERMISSION_KIND_UNKNOWN_PERMISSION, COREWEBVIEW2_PERMISSION_STATE_ALLOW,
};
use webview2_com::{take_pwstr, PermissionRequestedEventHandler};
use windows::core::PWSTR;

/// The prompts MiniChat answers for itself. Everything else (geolocation,
/// sensors, arbitrary file access) is left alone, so WebView2 still asks —
/// MiniChat never requests them, and a page that does should have to explain
/// itself.
fn is_granted(kind: COREWEBVIEW2_PERMISSION_KIND) -> bool {
    kind == COREWEBVIEW2_PERMISSION_KIND_MICROPHONE
        || kind == COREWEBVIEW2_PERMISSION_KIND_CAMERA
        || kind == COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS
        || kind == COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ
}

/// Answer camera/microphone prompts for `origin` instead of showing them.
///
/// Registered once per webview, right after it is created. Failure is
/// non-fatal: the worst case is the prompt the user would have seen anyway.
pub fn auto_grant_permissions(window: &WebviewWindow, origin: String) {
    let result = window.with_webview(move |webview| {
        // SAFETY: `with_webview` runs on the thread that owns the webview, so
        // the controller is live for the duration of the call.
        let core = match unsafe { webview.controller().CoreWebView2() } {
            Ok(core) => core,
            Err(e) => {
                eprintln!("minichat: could not reach WebView2 ({e}); permission prompts stay");
                return;
            }
        };

        let handler = PermissionRequestedEventHandler::create(Box::new(
            move |_sender: Option<ICoreWebView2>,
                  args: Option<ICoreWebView2PermissionRequestedEventArgs>| {
                let Some(args) = args else { return Ok(()) };

                // SAFETY: WebView2 hands us a live args object and owns it for
                // the duration of the callback. `Uri` allocates with
                // `CoTaskMemAlloc`; `take_pwstr` takes ownership and frees it.
                unsafe {
                    let mut kind = COREWEBVIEW2_PERMISSION_KIND_UNKNOWN_PERMISSION;
                    args.PermissionKind(&mut kind)?;
                    if !is_granted(kind) {
                        return Ok(());
                    }

                    let mut uri = PWSTR::null();
                    args.Uri(&mut uri)?;
                    let uri = take_pwstr(uri);
                    if !crate::origin_matches(&uri, &origin) {
                        return Ok(());
                    }

                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
                }
                Ok(())
            },
        ));

        let mut token = 0i64;
        // SAFETY: same thread, and the handler is a COM object WebView2 keeps
        // alive by reference count for as long as it is subscribed.
        if let Err(e) = unsafe { core.add_PermissionRequested(&handler, &mut token) } {
            eprintln!("minichat: could not hook WebView2 permissions ({e}); prompts stay");
        }
    });

    if let Err(e) = result {
        eprintln!("minichat: could not reach the webview ({e}); permission prompts stay");
    }
}
