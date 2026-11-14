//! Headless-podglądy `luna-calculator` do dokumentacji (bez kompozytora).
//!
//! Uruchom: `cargo run --bin preview`
//! → `docs/images/calculator-standard.png` i `docs/images/calculator-scientific.png`.

use std::error::Error;
use std::path::PathBuf;

use libluna_core::{Size, Theme};
use libluna_i18n::{set_locale, Locale};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use luna_calculator::app::{App, Mode};
use luna_calculator::ui;

const W: u32 = 900;
const H: u32 = 680;

fn main() -> Result<(), Box<dyn Error>> {
    set_locale(Locale::new("pl"));

    let size = Size::new(W as f64, H as f64);
    let text = TextEngine::system();

    let mut standard = App::new();
    standard.expression = "2+3×4".to_owned();
    let dark = Theme::dark();
    save(&standard, size, &dark, &text, "calculator-standard.png")?;

    let mut scientific = App::new();
    scientific.mode = Mode::Scientific;
    scientific.expression = "sin(30)+√(16)".to_owned();
    let light = Theme::light();
    save(
        &scientific,
        size,
        &light,
        &text,
        "calculator-scientific.png",
    )?;

    Ok(())
}

fn save(
    app: &App,
    size: Size,
    theme: &Theme,
    text: &TextEngine,
    name: &str,
) -> Result<(), Box<dyn Error>> {
    let mut app = clone_for_preview(app);
    let mut scene = Scene::new();
    ui::paint(&mut app, &mut scene, size, theme, text);

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

/// `App` nie implementuje `Clone`; podgląd potrzebuje tylko kilku pól stanu.
fn clone_for_preview(app: &App) -> App {
    let mut copy = App::new();
    copy.mode = app.mode;
    copy.expression = app.expression.clone();
    copy.result = app.result;
    copy.last_result = app.last_result;
    copy.error = app.error;
    copy.angle = app.angle;
    copy.just_evaluated = app.just_evaluated;
    copy
}
