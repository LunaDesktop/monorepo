//! Podgląd stylów — renderuje ten sam (realny) interfejs w kilku motywach i składa
//! montaż. Headlessowy odpowiednik „okna podglądu” przy wyborze stylu.
//!
//! Uruchom: `cargo run -p luna-demo --bin styles`
//! Efekt:   `docs/images/styles.png`

use std::error::Error;
use std::path::Path;

use libluna_core::{Accent, Size, ThemePreference};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use luna_demo::ui::{self, Demo};
use tiny_skia::{FilterQuality, IntSize, Pixmap, PixmapPaint, Transform};

const W: u32 = 900;
const H: u32 = 1028;
const COLS: u32 = 2;

/// (etykieta, preferencja, ścieżka stylu) — kolejność = kolejność kafli.
fn specs<'a>() -> [(&'a str, ThemePreference, Option<&'a str>); 4] {
    [
        ("Luna Light", ThemePreference::Light, None),
        ("Luna Dark", ThemePreference::Dark, None),
        ("Nord", ThemePreference::Dark, Some("nord.theme")),
        ("Paper", ThemePreference::Light, Some("paper.theme")),
    ]
}

fn themes_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/luna/themes")
}

fn main() -> Result<(), Box<dyn Error>> {
    let text = TextEngine::system();
    let size = Size::new(W as f64, H as f64);
    let dir = themes_dir();

    // Kafle: render interfejsu w danym motywie.
    let mut tiles: Vec<Vec<u8>> = Vec::new();
    for (label, pref, style) in specs() {
        let mut theme = pref.resolve(false, Accent::Blue);
        if let Some(file) = style {
            match libluna_core::theme_file::load_spec(&dir.join(file).to_string_lossy()) {
                Some(patch) => theme = patch.build(theme),
                None => eprintln!("{label}: style not found: {file}"),
            }
        }
        let mut scene = Scene::new();
        ui::build_scene(
            &mut scene,
            &theme,
            &text,
            size,
            &Demo::default(),
            pref,
            Accent::Blue,
        );
        let mut renderer = SoftwareRenderer::new(W, H);
        renderer.draw(&scene, 1.0, theme.colors.window_bg);
        tiles.push(renderer.data().to_vec());
    }

    // Montaż `COLS` x `rows` (skalowanie + przesunięcie w jednej transformacji).
    let rows = (tiles.len() as u32).div_ceil(COLS);
    let cell_w = W / COLS;
    let cell_h = (H as f64 * cell_w as f64 / W as f64).round() as u32;
    let mut montage = Pixmap::new(cell_w * COLS, cell_h * rows).unwrap();
    let paint = PixmapPaint {
        quality: FilterQuality::Bilinear,
        ..Default::default()
    };
    for (i, pixels) in tiles.iter().enumerate() {
        let src = Pixmap::from_vec(pixels.clone(), IntSize::from_wh(W, H).unwrap()).unwrap();
        let t = Transform::from_scale(cell_w as f32 / W as f32, cell_h as f32 / H as f32)
            .post_translate(
                (i as u32 % COLS) as f32 * cell_w as f32,
                (i as u32 / COLS) as f32 * cell_h as f32,
            );
        montage.draw_pixmap(0, 0, src.as_ref(), &paint, t, None);
    }
    montage.save_png("docs/images/styles.png")?;
    println!("saved docs/images/styles.png ({} styles)", tiles.len());
    Ok(())
}
