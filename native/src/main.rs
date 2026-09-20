mod api;
mod app;
mod demo;
mod emoji;
mod fonts;
mod format;
mod gateway;
mod images;
mod perms;
mod screenshot;
mod settings;
mod store;
mod text;
mod theme;
mod view;

/// The types Slint generated from `ui/*.slint`.
pub mod ui {
    slint::include_modules!();
}
pub use ui::App;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    format::init_local_offset();

    // `--screenshot <file> [width height]` paints one frame headlessly and
    // exits. See src/screenshot.rs for why that exists.
    let args: Vec<String> = std::env::args().collect();
    if let Some(index) = args.iter().position(|a| a == "--screenshot") {
        let path = args
            .get(index + 1)
            .map(String::as_str)
            .unwrap_or("shot.png");
        let width = args
            .get(index + 2)
            .and_then(|v| v.parse().ok())
            .unwrap_or(1180);
        let height = args
            .get(index + 3)
            .and_then(|v| v.parse().ok())
            .unwrap_or(820);
        // `--screenshot out.png [w h] [light|dark] [#accent] [#tint]` so a
        // palette change can be reviewed in both themes without a display.
        let light = args.iter().any(|a| a == "light");
        let hex = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .and_then(|i| args.get(i + 1))
                .and_then(|v| theme::Rgb::parse(v))
        };
        let branding = theme::Branding {
            light,
            accent: hex("--accent").unwrap_or(theme::Branding::default().accent),
            tint: hex("--tint"),
            ..theme::Branding::default()
        };
        let palette = theme::Palette::resolve(&branding);
        let overlay = args
            .iter()
            .position(|a| a == "--overlay")
            .and_then(|i| args.get(i + 1))
            .cloned()
            .unwrap_or_default();
        return screenshot::capture(path, width, height, move || {
            // After the headless platform is installed, never before:
            // registering a font initialises the backend, and the default one
            // needs a display this process does not have.
            fonts::register();
            let app = App::new()?;
            theme::apply(&app, &palette);
            demo::populate(&app, &palette);
            // `--overlay settings|profile|emoji|menu|search` renders one of
            // the dialogs over the demo, so they can be reviewed too.
            demo::show_overlay(&app, &overlay);
            Ok(app)
        });
    }

    fonts::register();
    app::run(settings::Settings::load())
}
