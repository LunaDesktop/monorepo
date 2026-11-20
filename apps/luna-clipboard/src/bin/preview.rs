//! Headless-podglądy `luna-clipboard` do dokumentacji (bez kompozytora).
//!
//! Uruchom: `cargo run --bin preview`
//! → `docs/images/clipboard-list.png`, `clipboard-empty.png`, `clipboard-about.png`.

use std::error::Error;
use std::path::PathBuf;

use libluna_core::{Size, Theme};
use libluna_i18n::{set_locale, Locale};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use luna_clipboard::app::App;
use luna_clipboard::history::History;
use luna_clipboard::ui;

const SIZE: Size = Size {
    width: 460.0,
    height: 560.0,
};

fn main() -> Result<(), Box<dyn Error>> {
    set_locale(Locale::new("pl"));

    let text = TextEngine::system();
    let dark = Theme::dark();

    let mut list = demo(true);
    list.selected = 1;
    save(&mut list, &dark, &text, "clipboard-list.png")?;

    let mut empty = demo(false);
    save(&mut empty, &dark, &text, "clipboard-empty.png")?;

    let mut about = demo(true);
    about.about = true;
    save(&mut about, &dark, &text, "clipboard-about.png")?;
    Ok(())
}

fn demo(with_entries: bool) -> App {
    let mut app = App::launch();
    app.history = History::default();
    if with_entries {
        let now = History::now();
        let samples = [
            ("sudo pacman -Syu", true, 0u64),
            (
                "git commit -m \"clipboard: historia, przypinanie i filtr\"",
                false,
                95,
            ),
            ("https://paganlinux.org/docs/tasks", false, 640),
            ("Zażółć gęślą jaźń — test polskich znaków", false, 7_400),
            (
                "cargo test --manifest-path apps/luna-clipboard/Cargo.toml",
                false,
                95_000,
            ),
        ];
        for (index, (content, pinned, ago)) in samples.iter().enumerate() {
            app.history.push_at(content, now.saturating_sub(*ago));
            if *pinned {
                let position = app.history.entries.len() - 1;
                app.history.entries[position].pinned = true;
            }
            let _ = index;
        }
    }
    app.recompute();
    app
}

fn save(app: &mut App, theme: &Theme, text: &TextEngine, name: &str) -> Result<(), Box<dyn Error>> {
    let mut scene = Scene::new();
    ui::paint(app, &mut scene, SIZE, theme, text);

    let width = SIZE.width.round() as u32;
    let height = SIZE.height.round() as u32;
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
