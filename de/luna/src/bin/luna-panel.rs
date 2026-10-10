//! Podgląd powłoki Luny (headless): panel + launcher, w motywie jasnym i ciemnym.
//!
//! Uruchom: `cargo run -p luna --bin luna-panel`
//! Efekt:   `docs/images/luna-shell-light.png`, `docs/images/luna-shell-dark.png`

use std::error::Error;

use libluna_core::{Size, Theme};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use libluna_widgets::PaintCtx;
use luna::panel::{self, App, PanelState};

const W: u32 = 1280;
const H: u32 = 720;

fn main() -> Result<(), Box<dyn Error>> {
    let text = TextEngine::system();
    let size = Size::new(W as f64, H as f64);
    let panel = PanelState::sample();
    let apps = App::sample();

    for (name, theme) in [("light", Theme::light()), ("dark", Theme::dark())] {
        let mut scene = Scene::new();
        let mut paint = PaintCtx::new(&mut scene, &theme, &text);
        panel::paint_desktop(&panel, &apps, &mut paint, size);

        let mut renderer = SoftwareRenderer::new(W, H);
        renderer.draw(&scene, 1.0, theme.colors.window_bg);
        let png = renderer.encode_png().ok_or("failed to encode PNG")?;
        let path = format!("docs/images/luna-shell-{name}.png");
        std::fs::write(&path, png)?;
        println!("saved {path} ({W}x{H})");
    }
    Ok(())
}
