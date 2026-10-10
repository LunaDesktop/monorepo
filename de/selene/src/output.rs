//! Wyjście nested: Selene otwiera okno na nadrzędnym kompozytorze i rysuje scenę okien.
//!
//! Wątek wyjścia pełni dwie role: (1) rysuje skomponowaną scenę przez `libluna-render`
//! i (2) odbiera wejście od nadrzędnego kompozytora, przekazując je do magistrali
//! (`crate::input`), skąd wątek protokołu trasuje je do klientów.

use std::os::unix::net::UnixStream;

use libluna_core::{Color, Point, Rect, ThemeSettings};
use libluna_platform::{run, AppConfig, Frame};
use libluna_render::TextRun;

use crate::input::{self, Events};
use crate::monitors::Outputs;
use crate::state::{Window, WindowModel};

/// Uruchamia wątek wyjścia (nested Wayland) o zadanym rozmiarze.
pub fn spawn(
    model: WindowModel,
    outputs: Outputs,
    events: Events,
    wake: UnixStream,
    width: u32,
    height: u32,
) {
    std::thread::spawn(move || {
        let mut wake = wake;
        let config = AppConfig::new("Selene")
            .app_id("eu.paganlinux.Luna.Selene")
            .initial_size(width, height)
            .settings(ThemeSettings::default());
        let _ = run(config, move |frame: &mut Frame| {
            // Zgłoś bieżący rozmiar monitora (okno może być zmieniane przez rodzica).
            {
                let mut outs = outputs.lock().unwrap();
                if let Some(out) = outs.first_mut() {
                    out.width = frame.size.width as i32;
                    out.height = frame.size.height as i32;
                }
            }
            // Przekaż wejście do magistrali.
            input::push(&events, &mut wake, frame.events);
            // Narysuj scenę.
            draw(frame, &model);
            // Utrzymuj klatki — model okien zmienia się poza tą pętlą.
            frame.request_frame();
        });
    });
}

fn draw(frame: &mut Frame, model: &WindowModel) {
    frame.scene.clear();
    frame.scene.fill_rect(
        Rect::from_size(frame.size),
        Color::from_rgba8(0x1e, 0x20, 0x28, 0xff),
    );

    {
        let windows = model.lock().unwrap();
        let mut order: Vec<&Window> = windows.iter().collect();
        order.sort_by_key(|w| w.z);
        for w in order {
            // Tło treści (na wypadek przezroczystości bufora klienta).
            frame
                .scene
                .fill_rect(w.content(), Color::from_rgba8(0x00, 0x00, 0x00, 0xff));
            // Treść okna (bufor klienta).
            frame.scene.image(w.content(), w.data.clone());
            // Pasek tytułu (dekoracja po stronie serwera).
            frame
                .scene
                .fill_rect(w.titlebar(), Color::from_rgba8(0x2c, 0x2f, 0x38, 0xff));
            frame.scene.text(
                TextRun::new(Point::new(w.x + 10.0, w.y + 20.0), &w.title, 13.0)
                    .with_color(Color::from_rgba8(0xee, 0xee, 0xee, 0xff)),
            );
            // Przycisk zamknięcia (krzyżyk).
            let c = w.close_rect().center();
            let glyph = Color::from_rgba8(0xd0, 0xd0, 0xd0, 0xff);
            frame.scene.stroke_line(
                Point::new(c.x - 4.0, c.y - 4.0),
                Point::new(c.x + 4.0, c.y + 4.0),
                1.6,
                glyph,
            );
            frame.scene.stroke_line(
                Point::new(c.x + 4.0, c.y - 4.0),
                Point::new(c.x - 4.0, c.y + 4.0),
                1.6,
                glyph,
            );
            // Uchwyt przeskalowania (3 kropki w narożniku).
            let g = w.grip_rect();
            for i in 0..3 {
                let x = g.right() - 3.0 - i as f64 * 4.0;
                let y = g.bottom() - 3.0 - i as f64 * 4.0;
                frame.scene.fill_rect(
                    Rect::from_xywh(x, y, 2.0, 2.0),
                    Color::from_rgba8(0x9a, 0x9e, 0xa8, 0xff),
                );
            }
            // Obramowanie całego okna.
            frame.scene.stroke_rounded_rect(
                w.outer(),
                6.0,
                1.0,
                Color::from_rgba8(0x50, 0x54, 0x60, 0xff),
            );
        }
    }

    frame.scene.text(
        TextRun::new(Point::new(16.0, 28.0), "Selene — kompozytor Luna", 14.0)
            .with_color(Color::from_rgba8(0xdd, 0xdd, 0xdd, 0xff)),
    );
}
