//! Przykład: okno (karta ustawień) zbudowane **wyłącznie** z drzewa układu
//! (`measure`/`allocate`) — bez ręcznego liczenia prostokątów.
//!
//! Uruchom: `cargo run -p luna-demo --bin layout`
//! Efekt:   `docs/images/layout.png`

use std::error::Error;

use libluna_core::style::{weight, FontStyle};
use libluna_core::{Insets, Point, Rect, Size, Theme};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use libluna_widgets::icon;
use libluna_widgets::layout::{Layout, LayoutCtx, Node};
use libluna_widgets::PaintCtx;

const W: u32 = 560;
const H: u32 = 420;
const PAD: f64 = 16.0;

/// Wiersz ustawień: etykiety po lewej, kontrolka (wyśrodkowana) po prawej.
fn row(label: &str, style: FontStyle, control: Node) -> Node {
    Node::padding(
        Insets::symmetric(12.0, PAD),
        Node::hbox(vec![
            Node::label(label, style),
            Node::spacer(),
            Node::center(control),
        ])
        .gap(12.0),
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    let theme = Theme::dark();
    let text = TextEngine::system();
    let size = Size::new(W as f64, H as f64);
    let ctx = LayoutCtx {
        theme: &theme,
        text: &text,
    };

    // --- Drzewo układu (retencyjne, dwa przebiegi: measure → allocate) ---
    let tree = Node::padding(
        Insets::splat(PAD),
        Node::vbox(vec![
            Node::padding(
                Insets::symmetric(14.0, PAD),
                Node::label("Appearance", theme.typography.heading),
            ),
            Node::fixed(0.0, 1.0), // separator
            row("Theme", theme.typography.body, Node::fixed(160.0, 32.0)),
            Node::fixed(0.0, 1.0),
            row("Volume", theme.typography.body, Node::fixed(160.0, 32.0)),
            Node::fixed(0.0, 1.0),
            row("Language", theme.typography.body, Node::fixed(160.0, 32.0)),
            Node::spacer(),
            Node::padding(
                Insets::symmetric(12.0, PAD),
                Node::hbox(vec![Node::spacer(), Node::center(Node::fixed(120.0, 34.0))]).gap(12.0),
            ),
        ]),
    );

    let rects = Layout::new(tree).fit(Rect::from_size(size), &ctx);

    let mut scene = Scene::new();
    let mut paint = PaintCtx {
        scene: &mut scene,
        theme: &theme,
        text: &text,
    };
    let colors = theme.colors;

    // Tło okna i karty.
    paint
        .scene
        .fill_rect(Rect::from_size(size), colors.window_bg);
    let card = Rect::from_size(size).deflate(Insets::splat(PAD));
    paint
        .scene
        .fill_rounded_rect(card, theme.radius.large, colors.card_bg);
    paint
        .scene
        .stroke_rounded_rect(card, theme.radius.large, 1.0, colors.border);

    // Liście w kolejności DFS (zgodnie ze strukturą drzewa powyżej):
    // 0 nagłówek · 1/5/9 separatory · 2/6/10 etykiety · 4/8/12 kontrolki · 15 przycisk.
    let heading = theme.typography.heading.size;
    let body = theme.typography.body.size;
    let caption = theme.typography.caption.size;

    paint.label_left(
        rects[0].x(),
        rects[0].center().y,
        "Appearance",
        heading,
        weight::BOLD,
        colors.text,
    );

    for sep in [1, 5, 9] {
        let r = rects[sep];
        paint.scene.fill_rect(
            Rect::from_xywh(r.x(), r.center().y, r.width(), 1.0),
            colors.border,
        );
    }

    // Etykiety wierszy.
    for (i, name) in [(2, "Theme"), (6, "Volume"), (10, "Language")] {
        let r = rects[i];
        paint.label_left(r.x(), r.center().y, name, body, weight::NORMAL, colors.text);
    }

    // Kontrolka 4 — pole z chevronem (jak segmentowy przełącznik).
    let seg = rects[4];
    paint
        .scene
        .fill_rounded_rect(seg, theme.radius.small, colors.shade);
    paint.label_center(
        seg.x() + seg.width() * 0.4,
        seg.center().y,
        "Dark",
        body,
        weight::MEDIUM,
        colors.text,
    );
    icon::chevron_down(
        &mut paint,
        colors.dim_text,
        Point::new(seg.right() - 14.0, seg.center().y),
        4.0,
        2.0,
    );

    // Kontrolka 8 — suwak głośności.
    let vol = rects[8];
    let track = Rect::from_xywh(vol.x(), vol.center().y - 3.0, vol.width(), 6.0);
    paint
        .scene
        .fill_rounded_rect(track, 3.0, colors.shade.scale_alpha(1.8));
    let filled = Rect::from_xywh(track.x(), track.y(), track.width() * 0.65, track.height());
    paint.scene.fill_rounded_rect(filled, 3.0, colors.accent);
    paint.scene.fill_rounded_rect(
        Rect::from_xywh(filled.right() - 9.0, vol.center().y - 9.0, 18.0, 18.0),
        9.0,
        colors.accent_fg,
    );
    paint.label_center(
        vol.x() - 28.0,
        vol.center().y,
        "65%",
        caption,
        weight::NORMAL,
        colors.text,
    );

    // Kontrolka 12 — lista rozwijana (pole + chevron).
    let dd = rects[12];
    paint
        .scene
        .fill_rounded_rect(dd, theme.radius.small, colors.shade);
    paint.label_left(
        dd.x() + 12.0,
        dd.center().y,
        "Polski",
        body,
        weight::NORMAL,
        colors.text,
    );
    icon::chevron_down(
        &mut paint,
        colors.dim_text,
        Point::new(dd.right() - 16.0, dd.center().y),
        4.0,
        2.0,
    );

    // Przycisk 15 — akcja sugerowana.
    let btn = rects[15];
    paint
        .scene
        .fill_rounded_rect(btn, theme.radius.small, colors.accent);
    paint.label_center(
        btn.center().x,
        btn.center().y,
        "Save changes",
        body,
        weight::MEDIUM,
        colors.accent_fg,
    );

    // Render do PNG.
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, colors.window_bg);
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/layout.png", png)?;
    println!(
        "saved docs/images/layout.png ({W}x{H}, {} leaves)",
        rects.len()
    );
    Ok(())
}
