mod api;
mod app;
mod demo;
mod emoji;
mod fonts;
mod format;
mod gateway;
mod images;
mod live;
mod notify;
mod perms;
mod screenshot;
mod settings;
mod store;
mod text;
mod theme;
mod tray;
mod view;
mod voice;

/// The types Slint generated from `ui/*.slint`.
pub mod ui {
    slint::include_modules!();
}
pub use ui::App;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    format::init_local_offset();
    let args: Vec<String> = std::env::args().collect();

    // `--live <origin> <user> <pass> [out.png]` signs in to a real instance
    // and renders what it sent. See src/live.rs.
    if let Some(index) = args.iter().position(|a| a == "--live") {
        let at = |offset: usize| args.get(index + offset).map(String::as_str).unwrap_or("");
        let (origin, user, pass) = (at(1), at(2), at(3));
        let out = args.get(index + 4).cloned();

        let runtime = tokio::runtime::Runtime::new()?;
        let (store, report) = runtime.block_on(live::fetch(origin, user, pass))?;

        println!("connected to  {}", report.instance);
        println!("signed in as  {}", report.me);
        println!(
            "permissions   {} (admin panel: {})",
            report.permissions,
            perms::can_see_admin_panel(report.permissions)
        );
        println!(
            "channels      {} in {} categories",
            report.channels, report.categories
        );
        println!("members       {}", report.members);
        println!("roles         {}", report.roles);
        println!("#{:<12} {} messages", report.channel_name, report.messages);

        if let Some(out) = out {
            let palette = theme::Palette::resolve(&theme::Branding {
                accent: theme::Rgb::parse(&store.instance.accent_color)
                    .unwrap_or(theme::Branding::default().accent),
                tint: store
                    .instance
                    .surface_tint
                    .as_deref()
                    .and_then(theme::Rgb::parse),
                corner_radius: store.instance.corner_radius as f32,
                light: store.instance.theme_mode == api::types::ThemeMode::Light,
            });
            screenshot::capture(&out, 1180, 760, move || {
                fonts::register();
                let app = App::new()?;
                theme::apply(&app, &palette);
                live::populate(&app, &store, &palette);
                Ok(app)
            })?;
            println!("rendered      {out}");
        }
        return Ok(());
    }

    // `--screenshot <file> [width height]` paints one frame headlessly and
    // exits. See src/screenshot.rs for why that exists.
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
