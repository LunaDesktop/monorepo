//! Galeria deweloperska widgetów libLuny — render headless do PNG.
//!
//! To ten sam ekran, który w aplikacji otwiera pozycja „Widgety” w sidebarze.
//!
//! Uruchom:
//!
//! ```sh
//! cargo run -p luna-demo --bin gallery
//! cargo run -p luna-demo --bin gallery -- Slider /tmp/slider.png
//! ```
//!
//! Efekt (bez argumentów): `docs/images/gallery.png`.

use std::error::Error;

use libluna_core::{Rect, Size, Theme};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use libluna_widgets::PaintCtx;
use luna_demo::gallery_catalog::DEMOS;
use luna_demo::gallery_page::{paint, GalleryState};

const W: u32 = 1200;
const H: u32 = 1000;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let wanted = args.next();
    let out = args
        .next()
        .unwrap_or_else(|| "docs/images/gallery.png".to_owned());

    let mut state = GalleryState::default();
    if let Some(name) = wanted {
        let index = DEMOS
            .iter()
            .position(|demo| demo.name.eq_ignore_ascii_case(&name))
            .ok_or_else(|| format!("nie ma widgetu „{name}”"))?;
        state.selected = index;
    }

    let theme = Theme::light();
    let text = TextEngine::system();
    let size = Size::new(W as f64, H as f64);

    let mut scene = Scene::new();
    {
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        paint(&mut ctx, Rect::from_size(size), &state);
    }

    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, theme.colors.window_bg);
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write(&out, png)?;
    println!("saved {out} ({W}x{H}, {})", DEMOS[state.selected].name);
    Ok(())
}
