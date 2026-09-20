//! Tests that drive the real interface.
//!
//! Slint's software renderer needs no display, and `Window::dispatch_event`
//! takes real input events, so a test can build the window, type into it and
//! assert on what came out. That is worth having for one class of bug in
//! particular: the kind that compiles, renders and is completely broken —
//! Enter not sending a message, a dialog that lays out as nothing.

#![cfg(test)]

use std::cell::RefCell;
use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, PhysicalSize, SharedString};

use crate::ui;

struct Headless {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }
}

/// Install the headless platform.
///
/// Slint allows one platform per process and a window belongs to the thread
/// that made it, so every scenario below runs inside a single `#[test]` on a
/// single thread. Splitting them would leave the test runner free to build
/// windows on several threads and share one platform between them.
fn platform() -> Rc<MinimalSoftwareWindow> {
    thread_local! {
        static WINDOW: RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(None) };
    }
    WINDOW.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some(window) = slot.as_ref() {
            return window.clone();
        }
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        let _ = slint::platform::set_platform(Box::new(Headless {
            window: window.clone(),
        }));
        *slot = Some(window.clone());
        window
    })
}

/// A window with the fonts registered and a size set, ready to be driven.
/// Let bindings settle and anything queued run. A single
/// `update_timers_and_animations` is not enough: `changed` handlers run as
/// part of a render pass.
fn settle(app: &ui::App) {
    let window = platform();
    // Rendering for real, not a no-op draw: Slint evaluates properties
    // lazily during a render pass, and `changed` handlers run as part of
    // that. A `draw_if_needed` whose closure ignores the renderer settles
    // nothing.
    let mut buffer = vec![crate::screenshot::Rgba8::default(); 1180 * 800];
    for _ in 0..3 {
        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut buffer, 1180);
        });
        window.request_redraw();
    }
    let _ = app;
}

fn app() -> ui::App {
    let window = platform();
    crate::fonts::register();
    let app = ui::App::new().expect("the window should build");
    app.show().expect("the window should show");
    window.set_size(PhysicalSize::new(1180, 800));
    app
}

fn press(app: &ui::App, text: &str) {
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: SharedString::from(text),
    });
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: SharedString::from(text),
    });
}

/// The character Slint reports for Return.
const RETURN: &str = "\n";

#[test]
fn the_interface_responds_to_input() {
    enter_sends_the_draft();
    escape_closes_an_open_dialog();
}

fn enter_sends_the_draft() {
    // The bug this pins: Slint only raises `accepted` on a single-line
    // input, and the composer is not one — so binding send to `accepted`
    // compiles, renders, and silently never sends anything.
    let app = app();
    let sent = Rc::new(RefCell::new(Vec::<String>::new()));

    app.on_send({
        let sent = sent.clone();
        move |text| sent.borrow_mut().push(text.to_string())
    });

    app.set_screen(ui::Screen::Chat);
    app.set_can_send(true);
    app.set_channel_name("general".into());
    app.set_draft("hello there".into());

    settle(&app);
    app.window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));

    // Put the caret in the composer, the way the app does after a channel
    // change or when a dialog closes.
    app.set_focus_composer(1);
    settle(&app);

    press(&app, RETURN);

    assert_eq!(
        sent.borrow().as_slice(),
        ["hello there"],
        "Enter did not send the draft"
    );
}

fn escape_closes_an_open_dialog() {
    let app = app();
    let dismissed = Rc::new(RefCell::new(0));
    app.on_dismiss_overlay({
        let dismissed = dismissed.clone();
        move || *dismissed.borrow_mut() += 1
    });

    app.set_screen(ui::Screen::Chat);
    app.set_overlay(ui::Overlay::Settings);
    settle(&app);
    app.window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));

    press(&app, "\u{1b}");

    assert_eq!(*dismissed.borrow(), 1, "Escape did not close the dialog");
}
