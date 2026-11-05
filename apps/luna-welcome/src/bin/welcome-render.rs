//! Renderuje Luna Welcome do PNG — podgląd bez kompozytora Wayland.
//!
//! Użycie:
//!   `cargo run --bin welcome-render -- [--dark] [--lang CODE] [--live|--installed] [wyjście.png]`

use std::error::Error;

use libluna_core::{Accent, Size, ThemePreference};
use libluna_i18n::{set_locale, Locale};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use luna_welcome::live::LiveEnvironment;
use luna_welcome::{news, ui, App};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut dark = false;
    let mut lang = "pl".to_owned();
    let mut live: Option<bool> = None;
    let mut out: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--dark" => {
                dark = true;
                index += 1;
            }
            "--live" => {
                live = Some(true);
                index += 1;
            }
            "--installed" => {
                live = Some(false);
                index += 1;
            }
            "--lang" => {
                if let Some(value) = args.get(index + 1) {
                    lang = value.clone();
                }
                index += 2;
            }
            other => {
                out = Some(other.to_owned());
                index += 1;
            }
        }
    }

    let out = out.unwrap_or_else(|| {
        if dark {
            "luna-welcome-dark.png".to_owned()
        } else {
            "luna-welcome.png".to_owned()
        }
    });

    set_locale(Locale::new(&lang));
    libluna_i18n::load_catalogs_from_dirs();

    let detected = LiveEnvironment::detect();
    let live = live.unwrap_or(detected.live);
    let environment = LiveEnvironment { live, ..detected };
    let notes = news::load();
    let version = news::system_version(&notes);
    let app = App::with_environment(environment, notes, version);

    let preference = if dark {
        ThemePreference::Dark
    } else {
        ThemePreference::Light
    };
    let theme = preference.resolve(false, Accent::Blue);
    let size = Size::new(980.0, 760.0);
    let text = TextEngine::system();
    let mut scene = Scene::new();
    ui::paint(&app, &mut scene, size, &theme, &text);

    let mut renderer = SoftwareRenderer::new(980, 760);
    renderer.draw(&scene, 1.0, theme.colors.window_bg);
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write(&out, png)?;
    println!("saved {out} (980x760, lang={lang}, live={live})");
    Ok(())
}
