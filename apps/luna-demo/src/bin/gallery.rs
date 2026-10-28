//! Galeria deweloperska widgetów libLuny — render headless do PNG.
//!
//! To ten sam ekran, który w aplikacji otwiera pozycja „Widgety” w sidebarze.
//! Uruchom: `cargo run -p luna-demo --bin gallery`
//! Efekt:   `docs/images/gallery.png`

use std::error::Error;

use libluna_core::{Rect, Size, Theme};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use libluna_widgets::PaintCtx;
use luna_demo::gallery_page::{paint, GalleryState};

const W: u32 = 1200;
const H: u32 = 1000;

fn main() -> Result<(), Box<dyn Error>> {
    let theme = Theme::light();
    let text = TextEngine::system();
    let size = Size::new(W as f64, H as f64);
    let state = GalleryState {
        show_code: true,
        ..GalleryState::default()
    };

    let mut scene = Scene::new();
    {
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        paint(&mut ctx, Rect::from_size(size), &state);
    }

    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, theme.colors.window_bg);
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/gallery.png", png)?;
    println!("saved docs/images/gallery.png ({W}x{H})");
    Ok(())
}
