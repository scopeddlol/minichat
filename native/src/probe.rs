//! Driving the real client against a real server, without a display.
//!
//! `--live` exercises the API client: it calls `login` and reads what comes
//! back. That is not what a member does. A member types into the sign-in
//! screen and presses a button, and between that button and the API client
//! there is a callback, a task queue, a network thread and an event coming
//! back the other way — none of which `--live` touches. The first bug found
//! in that gap was a sign-in failure reported as "your session was
//! rejected", which is an answer to a question nobody had asked.
//!
//! So this runs the whole client, headlessly, and drives the screen: fill in
//! the fields, press the button, wait for the app to appear or for the error
//! to say why not.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType, TargetPixel,
};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, PhysicalSize};

use crate::ui;

/// A pixel the software renderer can write, thrown away afterwards.
///
/// The probe never looks at the picture — `--screenshot` is what does that —
/// but the renderer has to have somewhere to draw or the window never
/// settles and the properties it is being asked about never update.
#[derive(Clone, Copy, Default)]
struct Discard(u32);

impl TargetPixel for Discard {
    fn blend(&mut self, colour: PremultipliedRgbaColor) {
        self.0 =
            u32::from(colour.red) << 16 | u32::from(colour.green) << 8 | u32::from(colour.blue);
    }
    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self(u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue))
    }
}

/// Slint's platform, with an event loop that spins instead of waiting on a
/// compositor.
///
/// The one in `screenshot.rs` renders a single frame and never runs a loop,
/// which is all a screenshot needs. This one has to keep timers firing,
/// because the client's whole event pump is a Slint timer.
struct Headless {
    window: Rc<MinimalSoftwareWindow>,
    quit: Arc<AtomicBool>,
}

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }

    fn run_event_loop(&self) -> Result<(), slint::PlatformError> {
        let size = self.window.size();
        let mut line = vec![Discard::default(); size.width.max(1) as usize];
        while !self.quit.load(Ordering::Relaxed) {
            slint::platform::update_timers_and_animations();
            self.window.draw_if_needed(|renderer| {
                renderer.render_by_line(LineByLine(&mut line));
            });
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}

/// Hands the renderer one scratch line at a time, so a full-window buffer is
/// never allocated for output nobody reads.
struct LineByLine<'a>(&'a mut Vec<Discard>);

impl slint::platform::software_renderer::LineBufferProvider for LineByLine<'_> {
    type TargetPixel = Discard;

    fn process_line(
        &mut self,
        _line: usize,
        range: core::ops::Range<usize>,
        render: impl FnOnce(&mut [Self::TargetPixel]),
    ) {
        if self.0.len() < range.end {
            self.0.resize(range.end, Discard::default());
        }
        render(&mut self.0[range]);
    }
}

/// Install the headless platform. The handle stops the event loop.
pub fn headless(width: u32, height: u32) -> Result<Arc<AtomicBool>, Box<dyn std::error::Error>> {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    window.set_size(PhysicalSize::new(width, height));
    let quit = Arc::new(AtomicBool::new(false));
    slint::platform::set_platform(Box::new(Headless {
        window,
        quit: quit.clone(),
    }))
    .map_err(|error| format!("could not install the headless platform: {error}"))?;
    Ok(quit)
}

/// What the probe saw.
pub type Outcome = Result<String, String>;

/// Fills in the sign-in screen, presses the button, and watches what happens.
pub struct SignIn {
    username: String,
    password: String,
    timeout: Duration,
    quit: Arc<AtomicBool>,
    outcome: Rc<RefCell<Option<Outcome>>>,
    /// Held for as long as the probe runs: a dropped `Timer` stops.
    timer: RefCell<Option<slint::Timer>>,
}

impl SignIn {
    pub fn new(username: &str, password: &str, timeout: Duration, quit: Arc<AtomicBool>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
            timeout,
            quit,
            outcome: Rc::new(RefCell::new(None)),
            timer: RefCell::new(None),
        }
    }

    pub fn outcome(&self) -> Outcome {
        self.outcome
            .borrow()
            .clone()
            .unwrap_or_else(|| Err("the probe never reached a verdict".into()))
    }

    /// Start driving. Called once the window exists and the client has been
    /// pointed at an instance.
    pub fn attach(&self, app: &ui::App) {
        let timer = slint::Timer::default();
        let weak = app.as_weak();
        let quit = self.quit.clone();
        let outcome = self.outcome.clone();
        let username = self.username.clone();
        let password = self.password.clone();
        let deadline = Instant::now() + self.timeout;
        let mut pressed = false;

        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(50),
            move || {
                let Some(app) = weak.upgrade() else { return };
                let finish = |result: Outcome| {
                    *outcome.borrow_mut() = Some(result);
                    quit.store(true, Ordering::Relaxed);
                };

                if !pressed {
                    // Wait for the instance to be known: until `Meta` comes
                    // back the screen is still settling, and typing into it
                    // before then tests the wrong thing.
                    if app.get_screen() != ui::Screen::Signin {
                        if Instant::now() > deadline {
                            finish(Err("the sign-in screen never appeared".into()));
                        }
                        return;
                    }
                    app.set_username(username.as_str().into());
                    app.set_password(password.as_str().into());
                    app.invoke_sign_in();
                    pressed = true;
                    return;
                }

                let error = app.get_auth_error();
                if !error.is_empty() {
                    finish(Err(error.to_string()));
                } else if app.get_screen() == ui::Screen::Chat {
                    finish(Ok(format!(
                        "signed in to {} as {}",
                        app.get_instance_name(),
                        app.get_my_name()
                    )));
                } else if Instant::now() > deadline {
                    finish(Err("timed out waiting for the client to sign in".into()));
                }
            },
        );

        *self.timer.borrow_mut() = Some(timer);
    }
}
