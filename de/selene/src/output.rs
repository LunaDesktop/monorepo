//! Wyjście nested: Selene otwiera okno na nadrzędnym kompozytorze i rysuje scenę okien.
//!
//! Wykorzystujemy `libluna-platform` (strona klienta Wayland) jako backend wyświetlania —
//! dzięki temu nie duplikujemy obsługi `wl_shm`/xdg-shell po stronie klienta. Wątek wyjścia
//! odświeża scenę w pętli (przez `Frame::request_frame`) i komponuje okna z modelu.

use libluna_core::{Color, Point, Rect, ThemeSettings};
use libluna_platform::{run, AppConfig, Frame};
use libluna_render::TextRun;

use crate::state::WindowModel;

/// Uruchamia wątek wyjścia (nested Wayland) o zadanym rozmiarze.
pub fn spawn(model: WindowModel, width: u32, height: u32) {
    std::thread::spawn(move || {
        let config = AppConfig::new("Selene")
            .app_id("eu.paganlinux.Luna.Selene")
            .initial_size(width, height)
            .settings(ThemeSettings::default());
        let _ = run(config, move |frame: &mut Frame| {
            draw(frame, &model);
            // Utrzymuj klatki — model okien zmienia się poza tą pętlą.
            frame.request_frame();
        });
    });
}

fn draw(frame: &mut Frame, model: &WindowModel) {
    let size = frame.size;
    frame.scene.clear();
    frame.scene.fill_rect(
        Rect::from_size(size),
        Color::from_rgba8(0x1e, 0x20, 0x28, 0xff),
    );

    {
        let windows = model.lock().unwrap();
        for w in windows.iter() {
            frame.scene.image(
                Rect::from_xywh(w.x, w.y, w.width as f64, w.height as f64),
                w.data.clone(),
            );
        }
    }

    frame.scene.text(
        TextRun::new(Point::new(16.0, 28.0), "Selene — kompozytor Luna", 14.0)
            .with_color(Color::from_rgba8(0xdd, 0xdd, 0xdd, 0xff)),
    );
}
