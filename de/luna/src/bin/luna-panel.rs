//! Podgląd powłoki Luny (headless): panel, OSD i powiadomienia — jasny i ciemny motyw.
//!
//! Uruchom: `cargo run -p luna --bin luna-panel`
//! Efekt:   `docs/images/luna-shell-light.png`, `docs/images/luna-shell-dark.png`

use std::error::Error;

use libluna_core::{Rect, Size, Theme};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use libluna_widgets::PaintCtx;
use luna::notifications::{Notification, Notifications};
use luna::osd::Osd;
use luna::panel::{self, App, PanelState};

const W: u32 = 1280;
const H: u32 = 720;

fn main() -> Result<(), Box<dyn Error>> {
    let text = TextEngine::system();
    let size = Size::new(W as f64, H as f64);
    let pstate = PanelState::sample();
    let apps = App::sample();

    for (name, theme) in [("light", Theme::light()), ("dark", Theme::dark())] {
        let mut scene = Scene::new();
        let mut paint = PaintCtx::new(&mut scene, &theme, &text);

        // Pulpit + panel + launcher.
        panel::paint_desktop(&pstate, &apps, &mut paint, size);

        // OSD wyśrodkowany pod paskiem (po animacji wejścia).
        let mut osd = Osd::new("◆", "Głośność", 0.65);
        osd.advance(0.3);
        osd.paint(
            &mut paint,
            Rect::from_xywh((W as f64 - 340.0) / 2.0, 64.0, 340.0, 72.0),
        );

        // Powiadomienia w prawym-górnym narożniku.
        let mut notes = Notifications::new();
        notes.push(
            Notification::new("Pliki", "Kopiowanie zakończone", "12 plików → Dokumenty")
                .with_glyph("▦"),
        );
        notes.push(Notification::new("System", "Bateria", "Pozostało 18%").with_glyph("■"));
        notes.advance(0.3);
        notes.paint(&mut paint, Rect::from_xywh(0.0, 56.0, W as f64, 240.0));

        let mut renderer = SoftwareRenderer::new(W, H);
        renderer.draw(&scene, 1.0, theme.colors.window_bg);
        let png = renderer.encode_png().ok_or("failed to encode PNG")?;
        let path = format!("docs/images/luna-shell-{name}.png");
        std::fs::write(&path, png)?;
        println!("saved {path} ({W}x{H})");
    }
    Ok(())
}
