//! Headless-podglądy `luna-calculator` do dokumentacji (bez kompozytora).
//!
//! Uruchom: `cargo run --bin preview`
//! → `docs/images/calculator-{standard,scientific,menu,settings}.png`.

use std::error::Error;
use std::path::PathBuf;

use libluna_core::{Size, Theme};
use libluna_i18n::{set_locale, Locale};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use luna_calculator::app::{App, Mode};
use luna_calculator::eval::AngleMode;
use luna_calculator::prefs::Prefs;
use luna_calculator::ui;

const W: u32 = 900;
const H: u32 = 680;

fn main() -> Result<(), Box<dyn Error>> {
    set_locale(Locale::new("pl"));

    let size = Size::new(W as f64, H as f64);
    let text = TextEngine::system();
    let dark = Theme::dark();
    let light = Theme::light();

    let mut standard = App::new();
    standard.expression = "2+3×4".to_owned();
    save(&mut standard, size, &dark, &text, "calculator-standard.png")?;

    standard.expression = "2+3×4".to_owned();
    standard.open_menu(ui::menu_anchor(size, &dark, &text));
    save(&mut standard, size, &dark, &text, "calculator-menu.png")?;

    let mut scientific = App::new();
    scientific.mode = Mode::Scientific;
    scientific.expression = "sin(30)+√(16)".to_owned();
    save(
        &mut scientific,
        size,
        &light,
        &text,
        "calculator-scientific.png",
    )?;

    let mut settings = App::with_prefs(Prefs {
        angle: AngleMode::Radians,
        start_mode: Mode::Scientific,
    });
    settings.settings_open = true;
    save(
        &mut settings,
        size,
        &light,
        &text,
        "calculator-settings.png",
    )?;

    let mut about = App::new();
    about.show_about();
    save(&mut about, size, &light, &text, "calculator-about.png")?;

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

    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, theme.colors.window_bg);
    let png = renderer.encode_png().ok_or("nie udało się zakodować PNG")?;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/images")
        .join(name);
    std::fs::write(&path, png)?;
    println!("saved {}", path.display());
    Ok(())
}
