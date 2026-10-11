//! Wyjście: nested (okno na nadrzędnym kompozytorze) oraz **realne DRM/KMS** (scanout).
//!
//! Wątek nested pełni dwie role: rysuje scenę przez `libluna-render` i odbiera wejście
//! od nadrzędnego kompozytora, przekazując je do magistrali (`crate::input`).
//! Wątek DRM renderuje scenę wprost do buforów „dumb” i skanuje je na monitorach
//! (page-flip). Wejście w trybie DRM (evdev) jest kolejnym krokiem.

use std::os::unix::net::UnixStream;
use std::time::Duration;

use libluna_core::{Color, Point, Rect, Size, ThemeSettings};
use libluna_platform::{run, AppConfig, Frame};
use libluna_render::{Scene, SoftwareRenderer, TextRun};

use crate::drm::{DrmBackend, DrmError};
use crate::input::{self, Events};
use crate::monitors::Outputs;
use crate::state::{Window, WindowModel};
use crate::sync::LockExt;

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
        let result = run(config, move |frame: &mut Frame| {
            // Zgłoś bieżący rozmiar monitora (okno może być zmieniane przez rodzica).
            {
                let mut outs = outputs.lock_or_recover();
                if let Some(out) = outs.first_mut() {
                    out.width = frame.size.width as i32;
                    out.height = frame.size.height as i32;
                }
            }
            // Przekaż wejście do magistrali.
            input::push(&events, &mut wake, frame.events);
            // Narysuj scenę.
            let size = frame.size;
            build_scene(frame.scene, size, 0.0, 0.0, &model);
            // Utrzymuj klatki — model okien zmienia się poza tą pętlą.
            frame.request_frame();
        });
        // Nested rysuje do okna na nadrzędnym kompozytorze Wayland. Gdy go nie ma
        // (np. uruchomienie na X11), nie połykamy błędu — inaczej wygląda to jak
        // „kompozytor działa, ale nic nie widać”.
        if let Err(e) = result {
            eprintln!(
                "{}: {e}",
                libluna_i18n::tr("nested output unavailable (no parent compositor)")
            );
        }
    });
}

/// Uruchamia wątek wyjścia DRM/KMS — renderuje scenę do buforów i skanuje na monitorach.
///
/// Wymaga bycia DRM masterem; jeśli inną sesję trzyma master, kończy bez efektu
/// (wywołujący powinien wtedy użyć wyjścia nested).
pub fn spawn_drm(model: WindowModel) {
    std::thread::spawn(move || {
        let mut backend = match DrmBackend::open() {
            Ok(backend) => backend,
            Err(reason) => {
                // Rozróżniamy powód — inaczej „brak mastera” maskuje błędy inicjalizacji.
                let msg = match reason {
                    DrmError::MasterBusy => {
                        libluna_i18n::tr("DRM backend unavailable (master held by another session)")
                    }
                    DrmError::Resources => {
                        libluna_i18n::tr("DRM backend unavailable (cannot read card resources)")
                    }
                    DrmError::NoOutputs => {
                        libluna_i18n::tr("DRM backend unavailable (no monitor could be set up)")
                    }
                };
                eprintln!("{msg}");
                return;
            }
        };
        let infos = backend.outputs();
        let sizes: Vec<(u32, u32)> = infos.iter().map(|(_, w, h)| (*w, *h)).collect();
        let origins: Vec<(f64, f64)> = {
            // Wyjścia ułożone poziomo (spójne z modelem monitorów).
            let mut x = 0.0;
            sizes
                .iter()
                .map(|(w, _)| {
                    let o = (x, 0.0);
                    x += *w as f64;
                    o
                })
                .collect()
        };
        for (name, w, h) in &infos {
            println!("{}: {} {}x{}", libluna_i18n::tr("DRM output"), name, w, h);
        }
        let mut renderers: Vec<SoftwareRenderer> = sizes
            .iter()
            .map(|(w, h)| SoftwareRenderer::new(*w, *h))
            .collect();
        let bg = Color::from_rgba8(0x1e, 0x20, 0x28, 0xff);

        // Gdy page-flip zawodzi (np. sterownik/karta), ekran zostaje czarny — logujemy raz.
        let mut flip_error_logged = false;
        loop {
            for i in 0..sizes.len() {
                let (w, h) = sizes[i];
                let (ox, oy) = origins[i];
                let mut scene = Scene::new();
                build_scene(&mut scene, Size::new(w as f64, h as f64), ox, oy, &model);
                renderers[i].draw(&scene, 1.0, bg);
                if !backend.present(i, renderers[i].data()) && !flip_error_logged {
                    eprintln!(
                        "{}",
                        libluna_i18n::tr("DRM page-flip failed (screen may stay blank)")
                    );
                    flip_error_logged = true;
                }
                backend.wait_flip();
            }
            // Bez wejścia w trybie DRM odświeżamy w stałym rytmie (~60 Hz).
            std::thread::sleep(Duration::from_millis(16));
        }
    });
}

/// Buduje scenę okien w rozmiarze wyjścia, przesuniętą o `(ox, oy)` (origin monitora).
fn build_scene(scene: &mut Scene, size: Size, ox: f64, oy: f64, model: &WindowModel) {
    let sh = |r: Rect| r.offset(-ox, -oy);
    scene.clear();
    scene.fill_rect(
        Rect::from_size(size),
        Color::from_rgba8(0x1e, 0x20, 0x28, 0xff),
    );

    {
        let windows = model.lock_or_recover();
        let mut order: Vec<&Window> = windows.iter().collect();
        order.sort_by_key(|w| w.z);
        for w in order {
            if !w.mapped || w.minimized {
                continue;
            }
            // Tło treści (na wypadek przezroczystości bufora klienta).
            scene.fill_rect(sh(w.content()), Color::from_rgba8(0x00, 0x00, 0x00, 0xff));
            // Treść okna (bufor klienta).
            scene.image(sh(w.content()), w.data.clone());
            // Dekoracja tylko dla topleveli (popupy i panele jej nie mają).
            if !w.popup && !w.layer {
                scene.fill_rect(sh(w.titlebar()), Color::from_rgba8(0x2c, 0x2f, 0x38, 0xff));
                scene.text(
                    TextRun::new(Point::new(w.x - ox + 10.0, w.y - oy + 20.0), &w.title, 13.0)
                        .with_color(Color::from_rgba8(0xee, 0xee, 0xee, 0xff)),
                );
                let glyph = Color::from_rgba8(0xd0, 0xd0, 0xd0, 0xff);
                let shift = |p: Point| Point::new(p.x - ox, p.y - oy);
                // Zamknięcie — krzyżyk.
                let c = shift(w.close_rect().center());
                scene.stroke_line(
                    Point::new(c.x - 4.0, c.y - 4.0),
                    Point::new(c.x + 4.0, c.y + 4.0),
                    1.6,
                    glyph,
                );
                scene.stroke_line(
                    Point::new(c.x + 4.0, c.y - 4.0),
                    Point::new(c.x - 4.0, c.y + 4.0),
                    1.6,
                    glyph,
                );
                // Maksymalizacja — kwadrat.
                let m = shift(w.maximize_rect().center());
                scene.stroke_rounded_rect(
                    Rect::from_xywh(m.x - 4.0, m.y - 4.0, 8.0, 8.0),
                    0.0,
                    1.4,
                    glyph,
                );
                // Minimalizacja — kreska.
                let n = shift(w.minimize_rect().center());
                scene.stroke_line(
                    Point::new(n.x - 4.0, n.y + 3.0),
                    Point::new(n.x + 4.0, n.y + 3.0),
                    1.6,
                    glyph,
                );
            }
            // Obramowanie całego okna (tylko zwykłe okna).
            if !w.layer {
                scene.stroke_rounded_rect(
                    sh(w.outer()),
                    6.0,
                    1.0,
                    Color::from_rgba8(0x50, 0x54, 0x60, 0xff),
                );
            }
        }
    }

    scene.text(
        TextRun::new(Point::new(16.0, 28.0), "Selene — kompozytor Luna", 14.0)
            .with_color(Color::from_rgba8(0xdd, 0xdd, 0xdd, 0xff)),
    );
}
