mod api;
mod demo;
mod fonts;
mod format;
mod gateway;
mod images;
mod perms;
mod screenshot;
mod text;
mod theme;
mod view;

/// The types Slint generated from `ui/*.slint`.
pub mod ui {
    slint::include_modules!();
}
pub use ui::App;

use slint::ComponentHandle;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    format::init_local_offset();

    // `--screenshot <file> [width height]` paints one frame headlessly and
    // exits. See src/screenshot.rs for why that exists.
    let args: Vec<String> = std::env::args().collect();
    if let Some(index) = args.iter().position(|a| a == "--screenshot") {
        let path = args.get(index + 1).map(String::as_str).unwrap_or("shot.png");
        let width = args.get(index + 2).and_then(|v| v.parse().ok()).unwrap_or(1180);
        let height = args.get(index + 3).and_then(|v| v.parse().ok()).unwrap_or(820);
        let palette = theme::Palette::resolve(&theme::Branding::default());
        return screenshot::capture(path, width, height, move || {
            // After the headless platform is installed, never before:
            // registering a font initialises the backend, and the default one
            // needs a display this process does not have.
            fonts::register();
            let app = App::new()?;
            theme::apply(&app, &palette);
            demo::populate(&app, &palette);
            Ok(app)
        });
    }

    fonts::register();
    App::new()?.run()?;
    Ok(())
}
