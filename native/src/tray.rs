//! The tray icon.
//!
//! The same menu the Tauri shell puts there, for the same reason: with
//! "close to tray" on, the window's close button hides it, and the tray is
//! then the only way back — and the only way to quit for real.
//!
//! Only built on the platforms that ship it. On Linux `tray-icon` needs GTK
//! or libayatana-appindicator, which is a system dependency the rest of this
//! client does not have and is not worth acquiring for one icon.

/// What the member picked from the tray.
///
/// Only the platform backend constructs these, and on Linux that backend is
/// the no-op one — so on a Linux build every variant is unconstructed. The
/// set is the tray's menu either way.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    SwitchInstance,
    ToggleCloseToTray,
    Quit,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod platform {
    use super::Action;

    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

    const ICON: &[u8] = include_bytes!("../assets/icons/32x32.png");

    pub struct Tray {
        // Held so the icon stays in the tray; dropping it removes it.
        _icon: TrayIcon,
        open: MenuItem,
        switch: MenuItem,
        close_to_tray: CheckMenuItem,
        quit: MenuItem,
    }

    impl Tray {
        pub fn new(close_to_tray: bool) -> Option<Self> {
            let image = image::load_from_memory(ICON).ok()?.to_rgba8();
            let (width, height) = image.dimensions();
            let icon = Icon::from_rgba(image.into_raw(), width, height).ok()?;

            let open = MenuItem::new("Open MiniChat", true, None);
            let switch = MenuItem::new("Switch instance…", true, None);
            let close_item = CheckMenuItem::new("Close to tray", true, close_to_tray, None);
            let quit = MenuItem::new("Quit MiniChat", true, None);

            let menu = Menu::new();
            menu.append(&open).ok()?;
            menu.append(&PredefinedMenuItem::separator()).ok()?;
            menu.append(&switch).ok()?;
            menu.append(&close_item).ok()?;
            menu.append(&PredefinedMenuItem::separator()).ok()?;
            menu.append(&quit).ok()?;

            let tray = TrayIconBuilder::new()
                .with_tooltip("MiniChat")
                .with_icon(icon)
                .with_menu(Box::new(menu))
                // The menu belongs on right-click, the way every other tray
                // icon behaves; left-click reopens the window.
                .with_menu_on_left_click(false)
                .build()
                .ok()?;

            Some(Self {
                _icon: tray,
                open,
                switch,
                close_to_tray: close_item,
                quit,
            })
        }

        /// Drain whatever the tray has queued. Polled from the UI timer, so
        /// no event loop integration is needed.
        pub fn poll(&self) -> Vec<Action> {
            let mut actions = Vec::new();

            while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                // A left click reopens; everything else the icon reports
                // (movement, enter, leave) is not a command.
                if let TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    button_state: tray_icon::MouseButtonState::Up,
                    ..
                } = event
                {
                    actions.push(Action::Open);
                }
            }

            while let Ok(event) = MenuEvent::receiver().try_recv() {
                let id = event.id();
                if id == self.open.id() {
                    actions.push(Action::Open);
                } else if id == self.switch.id() {
                    actions.push(Action::SwitchInstance);
                } else if id == self.close_to_tray.id() {
                    actions.push(Action::ToggleCloseToTray);
                } else if id == self.quit.id() {
                    actions.push(Action::Quit);
                }
            }

            actions
        }

        pub fn set_close_to_tray(&self, checked: bool) {
            self.close_to_tray.set_checked(checked);
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform {
    use super::Action;

    /// The no-tray build. Every call is a no-op, so the caller needs no
    /// `cfg` of its own.
    pub struct Tray;

    impl Tray {
        pub fn new(_close_to_tray: bool) -> Option<Self> {
            None
        }
        pub fn poll(&self) -> Vec<Action> {
            Vec::new()
        }
        pub fn set_close_to_tray(&self, _checked: bool) {}
    }
}

pub use platform::Tray;

/// Whether this build has a tray at all, which decides whether closing the
/// window may hide it: hiding to a tray that is not there leaves someone
/// with an app that appears to have vanished.
pub fn available() -> bool {
    cfg!(any(target_os = "windows", target_os = "macos"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_build_with_no_tray_says_so() {
        // The window must not hide itself where there is nothing to restore
        // it from, so this answer is load-bearing rather than cosmetic.
        assert_eq!(
            super::available(),
            cfg!(any(target_os = "windows", target_os = "macos"))
        );
        // And constructing one is harmless either way.
        let tray = super::Tray::new(true);
        if !super::available() {
            assert!(tray.is_none());
        }
    }
}
