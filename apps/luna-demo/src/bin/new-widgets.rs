//! Podgląd nowych widgetów libLuny: `InfoBar`, `Picture`, `MediaControls`.
//!
//! Uruchom: `cargo run -p luna-demo --bin new-widgets`
//! Efekt:   `docs/images/new-widgets.png`

use std::error::Error;
use std::sync::Arc;

use libluna_core::style::weight;
use libluna_core::{Rect, Size, Theme};
use libluna_render::{ImageData, Scene, SoftwareRenderer, TextEngine};
use libluna_widgets::{ContentFit, InfoBar, InfoBarKind, MediaControls, PaintCtx, Picture};

const W: u32 = 900;
const H: u32 = 560;

fn main() -> Result<(), Box<dyn Error>> {
    let theme = Theme::dark();
    let text = TextEngine::system();
    let size = Size::new(W as f64, H as f64);

    let mut scene = Scene::new();
    let mut paint = PaintCtx::new(&mut scene, &theme, &text);
    paint
        .scene
        .fill_rect(Rect::from_size(size), theme.colors.window_bg);

    caption(&mut paint, 24.0, 20.0, "InfoBar (GTK4) — 4 rodzaje");

    let y = 44.0;
    let bars = [
        (InfoBarKind::Info, "Nowa wersja jest dostępna", true),
        (InfoBarKind::Question, "Zsynchronizować ustawienia?", true),
        (InfoBarKind::Warning, "Mało miejsca na dysku", true),
        (InfoBarKind::Error, "Nie udało się zapisać pliku", false),
    ];
    for (i, (kind, msg, with_action)) in bars.iter().enumerate() {
        let mut bar = InfoBar::new(*msg).kind(*kind);
        if *with_action {
            bar = bar.action("OK");
        }
        if i == 3 {
            bar = bar.action("Ponów");
        }
        let rect = Rect::from_xywh(24.0, y + i as f64 * 64.0, 520.0, 56.0);
        bar.paint(&mut paint, rect);
    }

    // ── Picture (dopasowanie) ──
    caption(&mut paint, 580.0, 20.0, "Picture — Contain / Cover / Fill");
    let image = Arc::new(gradient(160, 100));
    for (i, fit) in [ContentFit::Contain, ContentFit::Cover, ContentFit::Fill]
        .into_iter()
        .enumerate()
    {
        let mut pic = Picture::with_image(image.clone());
        pic.set_fit(fit);
        let rect = Rect::from_xywh(580.0, 44.0 + i as f64 * 92.0, 296.0, 80.0);
        paint.scene.fill_rect(rect, theme.colors.view_bg);
        pic.paint(&mut paint, rect);
        paint
            .scene
            .stroke_rounded_rect(rect, 0.0, 1.0, theme.colors.border);
    }

    // ── MediaControls ──
    caption(&mut paint, 24.0, 320.0, "MediaControls (GTK4)");
    let mut media = MediaControls::new("Aurora", "Luna Ensemble");
    media.set_times("1:42", "4:07");
    media.set_position(0.41);
    media.paint(&mut paint, Rect::from_xywh(24.0, 344.0, 520.0, 64.0));

    let mut paused = MediaControls::new("Nokturn", "Selene");
    paused.set_playing(false);
    paused.set_times("0:15", "3:20");
    paused.set_position(0.08);
    paused.paint(&mut paint, Rect::from_xywh(24.0, 420.0, 520.0, 64.0));

    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, theme.colors.window_bg);
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/new-widgets.png", png)?;
    println!("saved docs/images/new-widgets.png ({W}x{H})");
    Ok(())
}

fn caption(paint: &mut PaintCtx, x: f64, y: f64, text: &str) {
    paint.label_left(
        x,
        y + 8.0,
        text,
        paint.theme.typography.caption.size,
        weight::BOLD,
        paint.theme.colors.dim_text,
    );
}

/// Prosty obraz gradientowy (RGB) do demonstracji `Picture`.
fn gradient(w: u32, h: u32) -> ImageData {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let r = (x * 255 / w.max(1)) as u8;
            let g = (y * 255 / h.max(1)) as u8;
            rgba.extend_from_slice(&[r, g, 200, 255]);
        }
    }
    ImageData::new(w, h, rgba)
}
