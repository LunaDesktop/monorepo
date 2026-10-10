//! Przykład nowych widgetów: `Banner`, `Spinner`, `Tooltip` (render headless).
//!
//! Uruchom: `cargo run -p luna-demo --bin widgets`
//! Efekt:   `docs/images/widgets.png`

use std::error::Error;

use libluna_core::style::weight;
use libluna_core::{Rect, Size, Theme};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use libluna_widgets::{Banner, BannerKind, Button, PaintCtx, Spinner, TabBar, Tooltip};

const W: u32 = 520;
const H: u32 = 364;

fn main() -> Result<(), Box<dyn Error>> {
    let theme = Theme::dark();
    let text = TextEngine::system();
    let size = Size::new(W as f64, H as f64);

    let mut scene = Scene::new();
    let mut paint = PaintCtx {
        scene: &mut scene,
        theme: &theme,
        text: &text,
    };
    paint
        .scene
        .fill_rect(Rect::from_size(size), theme.colors.window_bg);

    // Banner (ostrzeżenie) z akcją.
    let banner = Banner::new("Storage almost full")
        .kind(BannerKind::Warning)
        .description("Only 2 GB left on the system disk")
        .action("Manage");
    let banner_rect = Rect::from_xywh(16.0, 16.0, size.width - 32.0, banner.height(&theme));
    banner.paint(&mut paint, banner_rect);

    // Spinner (wskaźnik zajętości) — faza ustawiona ręcznie (brak pętli animacji).
    let mut spinner = Spinner::new();
    spinner.set_progress(0.12);
    let s = spinner.size(&theme);
    let spinner_rect = Rect::from_xywh(size.width / 2.0 - s / 2.0, 112.0, s, s);
    spinner.paint(&mut paint, spinner_rect);

    // Przycisk + dymek podpowiedzi nad nim.
    let button = Button::new("Refresh");
    let button_rect = Rect::from_xywh(16.0, 220.0, 120.0, button.height(&theme));
    button.paint(&mut paint, button_rect);

    let tooltip = Tooltip::new("Reload the list");
    let tooltip_rect = tooltip.rect(button_rect, &theme, &text);
    tooltip.paint(&mut paint, tooltip_rect);

    // Etykieta pomocnicza pod spinnerem.
    paint.label_center(
        size.width / 2.0,
        spinner_rect.bottom() + 20.0,
        "Loading…",
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );

    // Pasek zakładek z przewijaniem (overflow) — 6 zakładek w wąskim pasku.
    let tabs = TabBar::new([
        "General",
        "Advanced",
        "Appearance",
        "Network",
        "System",
        "About",
    ]);
    let tabs_rect = Rect::from_xywh(
        16.0,
        size.height - 52.0,
        size.width - 32.0,
        tabs.height(&theme),
    );
    tabs.paint(&mut paint, tabs_rect);

    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, theme.colors.window_bg);
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/widgets.png", png)?;
    println!("saved docs/images/widgets.png ({W}x{H})");
    Ok(())
}
