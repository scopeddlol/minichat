//! Global voice hotkeys.
//!
//! Push-to-talk is only useful if it works while MiniChat is in the
//! background — that is what distinguishes it from a key handler in the
//! window. So these are registered with the desktop rather than with Slint.
//!
//! Only on the platforms that ship them, for the same reason as the tray:
//! the Linux backend wants X11 or a compositor protocol the rest of this
//! client does not depend on. `available()` reports which build this is, and
//! the settings screen says so rather than offering a switch that does
//! nothing.

use crate::settings::HotkeyBindings;

/// What a global key press means.
///
/// Only the platform backend constructs these, and on Linux that backend is
/// the no-op one, so on a Linux build every variant is unconstructed. The
/// set is what a binding can mean either way.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Held: the microphone opens on press and closes on release.
    PushToTalk(bool),
    ToggleMute,
    ToggleDeafen,
}

/// Parse an accelerator like "F8", "Ctrl+Shift+M" or "Alt+Space".
///
/// The same syntax the Tauri shell stores, so a member moving between the
/// two clients keeps their bindings. An unparseable one is skipped rather
/// than failing the lot: a key another application already owns should cost
/// you that key, not every key.
///
/// Called by the platform backend, which a Linux build does not compile;
/// the tests below cover it on every platform regardless.
#[allow(dead_code)]
pub fn parse(accelerator: &str) -> Option<(Modifiers, Key)> {
    let mut modifiers = Modifiers::default();
    let mut key = None;

    for part in accelerator
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.control = true,
            "alt" | "option" => modifiers.alt = true,
            "shift" => modifiers.shift = true,
            "super" | "meta" | "cmd" | "command" | "win" => modifiers.meta = true,
            _ => {
                // Only one non-modifier key; a second means the accelerator
                // is malformed rather than a chord we should guess at.
                if key.is_some() {
                    return None;
                }
                key = Some(Key(part.to_string()));
            }
        }
    }

    key.map(|key| (modifiers, key))
}

/// See `parse` on why this is dead on a Linux build.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

/// The non-modifier half of an accelerator, as written.
///
/// See `parse` on why this is dead on a Linux build.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key(pub String);

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod platform {
    use super::{Action, HotkeyBindings};

    use global_hotkey::hotkey::{Code, HotKey, Modifiers as GlobalModifiers};
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

    pub struct Hotkeys {
        manager: GlobalHotKeyManager,
        registered: Vec<(u32, Action)>,
    }

    impl Hotkeys {
        pub fn new() -> Option<Self> {
            Some(Self {
                manager: GlobalHotKeyManager::new().ok()?,
                registered: Vec::new(),
            })
        }

        /// Replace every registration with the given bindings.
        ///
        /// Every failure is non-fatal: a key another application already
        /// owns should cost you that key, not the whole set.
        pub fn apply(&mut self, bindings: &HotkeyBindings) {
            for (id, _) in self.registered.drain(..) {
                let _ = self.manager.unregister_by_id(id);
            }
            if !bindings.enabled {
                return;
            }

            for (accelerator, action) in [
                (&bindings.push_to_talk, Action::PushToTalk(true)),
                (&bindings.mute, Action::ToggleMute),
                (&bindings.deafen, Action::ToggleDeafen),
            ] {
                let Some(key) = to_hotkey(accelerator) else {
                    if !accelerator.is_empty() {
                        eprintln!("minichat: could not parse the hotkey '{accelerator}'");
                    }
                    continue;
                };
                match self.manager.register(key) {
                    Ok(()) => self.registered.push((key.id(), action)),
                    Err(error) => {
                        eprintln!("minichat: could not register '{accelerator}': {error}")
                    }
                }
            }
        }

        /// Drain what the desktop has queued. Polled from the UI timer, so
        /// no event loop integration is needed.
        pub fn poll(&self) -> Vec<Action> {
            let mut out = Vec::new();
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                let Some((_, action)) = self.registered.iter().find(|(id, _)| *id == event.id)
                else {
                    continue;
                };
                match (action, event.state) {
                    // Push-to-talk is a hold: open on press, close on release.
                    (Action::PushToTalk(_), HotKeyState::Pressed) => {
                        out.push(Action::PushToTalk(true))
                    }
                    (Action::PushToTalk(_), HotKeyState::Released) => {
                        out.push(Action::PushToTalk(false))
                    }
                    // The toggles act once, on press.
                    (other, HotKeyState::Pressed) => out.push(*other),
                    _ => {}
                }
            }
            out
        }
    }

    fn to_hotkey(accelerator: &str) -> Option<HotKey> {
        let (modifiers, key) = super::parse(accelerator)?;
        let mut flags = GlobalModifiers::empty();
        if modifiers.control {
            flags |= GlobalModifiers::CONTROL;
        }
        if modifiers.alt {
            flags |= GlobalModifiers::ALT;
        }
        if modifiers.shift {
            flags |= GlobalModifiers::SHIFT;
        }
        if modifiers.meta {
            flags |= GlobalModifiers::META;
        }
        Some(HotKey::new(Some(flags), to_code(&key.0)?))
    }

    /// Map the written key to a physical code.
    fn to_code(key: &str) -> Option<Code> {
        let upper = key.to_ascii_uppercase();
        Some(match upper.as_str() {
            "F1" => Code::F1,
            "F2" => Code::F2,
            "F3" => Code::F3,
            "F4" => Code::F4,
            "F5" => Code::F5,
            "F6" => Code::F6,
            "F7" => Code::F7,
            "F8" => Code::F8,
            "F9" => Code::F9,
            "F10" => Code::F10,
            "F11" => Code::F11,
            "F12" => Code::F12,
            "SPACE" => Code::Space,
            "TAB" => Code::Tab,
            "BACKQUOTE" | "`" => Code::Backquote,
            _ if upper.len() == 1 && upper.as_bytes()[0].is_ascii_alphabetic() => {
                // KeyA..KeyZ are contiguous, so the letter indexes into them.
                const LETTERS: [Code; 26] = [
                    Code::KeyA,
                    Code::KeyB,
                    Code::KeyC,
                    Code::KeyD,
                    Code::KeyE,
                    Code::KeyF,
                    Code::KeyG,
                    Code::KeyH,
                    Code::KeyI,
                    Code::KeyJ,
                    Code::KeyK,
                    Code::KeyL,
                    Code::KeyM,
                    Code::KeyN,
                    Code::KeyO,
                    Code::KeyP,
                    Code::KeyQ,
                    Code::KeyR,
                    Code::KeyS,
                    Code::KeyT,
                    Code::KeyU,
                    Code::KeyV,
                    Code::KeyW,
                    Code::KeyX,
                    Code::KeyY,
                    Code::KeyZ,
                ];
                LETTERS[(upper.as_bytes()[0] - b'A') as usize]
            }
            _ => return None,
        })
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform {
    use super::{Action, HotkeyBindings};

    /// The no-hotkey build. Every call is a no-op, so the caller needs no
    /// `cfg` of its own.
    pub struct Hotkeys;

    impl Hotkeys {
        pub fn new() -> Option<Self> {
            None
        }
        pub fn apply(&mut self, _bindings: &HotkeyBindings) {}
        pub fn poll(&self) -> Vec<Action> {
            Vec::new()
        }
    }
}

pub use platform::Hotkeys;

/// Whether this build can register global hotkeys.
pub fn available() -> bool {
    cfg!(any(target_os = "windows", target_os = "macos"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_key_parses_with_no_modifiers() {
        let (modifiers, key) = parse("F8").unwrap();
        assert_eq!(modifiers, Modifiers::default());
        assert_eq!(key.0, "F8");
    }

    #[test]
    fn modifiers_parse_in_any_order_and_any_case() {
        let (a, key) = parse("Ctrl+Shift+M").unwrap();
        let (b, _) = parse("shift+CONTROL+m").unwrap();
        assert_eq!(a, b);
        assert!(a.control && a.shift && !a.alt && !a.meta);
        assert_eq!(key.0, "M");
    }

    #[test]
    fn the_platform_names_for_the_command_key_all_work() {
        // A member moving between clients should not have to know which
        // spelling the settings file used.
        for spelling in ["Super+K", "Meta+K", "Cmd+K", "Win+K"] {
            let (modifiers, _) = parse(spelling).unwrap();
            assert!(modifiers.meta, "{spelling} did not set the meta modifier");
        }
    }

    #[test]
    fn an_accelerator_with_no_key_or_two_keys_is_refused() {
        // "Ctrl" alone is not a binding, and "F8+F9" is not a chord we
        // should guess at.
        assert!(parse("Ctrl").is_none());
        assert!(parse("Ctrl+Shift").is_none());
        assert!(parse("F8+F9").is_none());
        assert!(parse("").is_none());
    }

    #[test]
    fn stray_separators_and_spaces_are_tolerated() {
        let (modifiers, key) = parse(" Ctrl + F9 ").unwrap();
        assert!(modifiers.control);
        assert_eq!(key.0, "F9");
    }

    #[test]
    fn a_build_without_hotkeys_says_so_and_is_harmless() {
        assert_eq!(
            available(),
            cfg!(any(target_os = "windows", target_os = "macos"))
        );
        if !available() {
            assert!(Hotkeys::new().is_none());
        }
    }
}
