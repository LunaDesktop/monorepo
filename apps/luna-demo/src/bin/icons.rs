//! Galeria pakietu ikon libLuny — „packshot” całego zestawu `IconName::ALL`.
//!
//! Uruchom: `cargo run -p luna-demo --bin icons`
//! Efekt:   `docs/images/icons.png` (górna sekcja: motyw jasny, dolna: ciemny).

use std::error::Error;

use libluna_core::style::weight;
use libluna_core::{Rect, Size, Theme};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use libluna_widgets::{paint_icon, IconName, PaintCtx};

const COLS: usize = 7;
const CELL_W: f64 = 118.0;
const CELL_H: f64 = 112.0;
const MARGIN: f64 = 24.0;
const TITLE_H: f64 = 40.0;

/// Szerokość obrazu dobrana do liczby kolumn.
fn width() -> u32 {
    (MARGIN * 2.0 + CELL_W * COLS as f64) as u32
}

/// Wysokość jednej sekcji motywu.
fn section_h() -> f64 {
    let rows = IconName::ALL.len().div_ceil(COLS) as f64;
    TITLE_H + rows * CELL_H + MARGIN
}

fn main() -> Result<(), Box<dyn Error>> {
    let text = TextEngine::system();
    let w = width();
    let h = (section_h() * 2.0) as u32;
    let size = Size::new(w as f64, h as f64);

    let mut scene = Scene::new();
    scene.fill_rect(Rect::from_size(size), Theme::light().colors.window_bg);

    let mut y = 0.0;
    for (label, theme) in [
        ("Motyw jasny", Theme::light()),
        ("Motyw ciemny", Theme::dark()),
    ] {
        let mut paint = PaintCtx::new(&mut scene, &theme, &text);
        // Tło sekcji.
        let section = Rect::from_xywh(0.0, y, size.width, section_h());
        paint.scene.fill_rect(section, theme.colors.window_bg);
        paint.label_left(
            MARGIN,
            y + TITLE_H / 2.0,
            label,
            theme.typography.heading.size,
            weight::BOLD,
            theme.colors.text,
        );
        paint_icon_grid(&mut paint, &theme, y + TITLE_H);
        y += section_h();
    }

    let mut renderer = SoftwareRenderer::new(w, h);
    renderer.draw(&scene, 1.0, Theme::light().colors.window_bg);
    let png = renderer.encode_png().ok_or("nie udało się zakodować PNG")?;
    std::fs::write("docs/images/icons.png", png)?;
    println!("saved docs/images/icons.png ({w}x{h})");
    Ok(())
}

/// Rysuje siatkę wszystkich ikon z podpisami, zaczynając w `top`.
fn paint_icon_grid(paint: &mut PaintCtx, theme: &Theme, top: f64) {
    for (i, icon) in IconName::ALL.iter().enumerate() {
        let col = (i % COLS) as f64;
        let row = (i / COLS) as f64;
        let cell = Rect::from_xywh(
            MARGIN + col * CELL_W,
            top + row * CELL_H,
            CELL_W - 8.0,
            CELL_H - 8.0,
        );

        // Karta z ikoną.
        paint
            .scene
            .fill_rounded_rect(cell, theme.radius.medium, theme.colors.card_bg);
        paint
            .scene
            .stroke_rounded_rect(cell, theme.radius.medium, 1.0, theme.colors.border);

        let icon_rect = Rect::from_xywh(cell.center().x - 24.0, cell.y() + 22.0, 48.0, 48.0);
        paint_icon(paint, *icon, icon_rect, theme.colors.accent);

        paint.label_center(
            cell.center().x,
            icon_rect.bottom() + 16.0,
            icon.name(),
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.text,
        );
    }
}
