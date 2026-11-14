//! Headless-podglądy `luna-calculator` do dokumentacji (bez kompozytora).
//!
//! Uruchom: `cargo run --bin preview`
//! → `docs/images/calculator-{standard,scientific,menu,history,settings,about}.png`.

use std::error::Error;
use std::path::PathBuf;

use libluna_core::{Size, Theme};
use libluna_i18n::{set_locale, Locale};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use luna_calculator::app::{App, Mode};
use luna_calculator::eval::AngleMode;
use luna_calculator::prefs::Prefs;
use luna_calculator::ui;

const STANDARD: Size = Size {
    width: 640.0,
    height: 680.0,
};
const WIDE: Size = Size {
    width: 900.0,
    height: 680.0,
};

fn main() -> Result<(), Box<dyn Error>> {
    set_locale(Locale::new("pl"));

    let text = TextEngine::system();
    let dark = Theme::dark();
    let light = Theme::light();

    let mut standard = App::new();
    standard.expression = "2+3×4".to_owned();
    save(
        &mut standard,
        STANDARD,
        &dark,
        &text,
        "calculator-standard.png",
    )?;

    standard.open_menu(ui::menu_anchor(STANDARD, &dark, &text));
    save(&mut standard, STANDARD, &dark, &text, "calculator-menu.png")?;

    standard.history.add("12×8", "96");
    standard.history.add("sin(30)", "0.5");
    standard.history.add("2^10", "1024");
    standard.open_history();
    save(
        &mut standard,
        STANDARD,
        &dark,
        &text,
        "calculator-history.png",
    )?;

    let mut scientific = App::new();
    scientific.mode = Mode::Scientific;
    scientific.expression = "sin(30)+√(16)".to_owned();
    save(
        &mut scientific,
        WIDE,
        &light,
        &text,
        "calculator-scientific.png",
    )?;

    let mut narrow = App::new();
    narrow.mode = Mode::Scientific;
    narrow.expression = "sin(30)+√(16)".to_owned();
    save(
        &mut narrow,
        STANDARD,
        &light,
        &text,
        "calculator-scientific-narrow.png",
    )?;

    let mut settings = App::with_prefs(Prefs {
        angle: AngleMode::Radians,
        start_mode: Mode::Scientific,
    });
    settings.settings_open = true;
    save(
        &mut settings,
        WIDE,
        &light,
        &text,
        "calculator-settings.png",
    )?;

    let mut about = App::new();
    about.show_about();
    save(&mut about, WIDE, &light, &text, "calculator-about.png")?;

    Ok(())
}

fn save(
    app: &mut App,
    size: Size,
    theme: &Theme,
    text: &TextEngine,
    name: &str,
) -> Result<(), Box<dyn Error>> {
    let mut scene = Scene::new();
    ui::paint(app, &mut scene, size, theme, text);

    let width = size.width.round() as u32;
    let height = size.height.round() as u32;
    let mut renderer = SoftwareRenderer::new(width, height);
    renderer.draw(&scene, 1.0, theme.colors.window_bg);
    let png = renderer.encode_png().ok_or("nie udało się zakodować PNG")?;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/images")
        .join(name);
    std::fs::write(&path, png)?;
    println!("saved {}", path.display());
    Ok(())
}
