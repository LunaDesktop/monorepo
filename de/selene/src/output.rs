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
use crate::state::{CursorModel, Window, WindowModel};
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
            build_scene(frame.scene, size, 0.0, 0.0, &model, None);
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

/// Otwiera backend DRM, czekając chwilę, gdy mastera trzyma jeszcze greeter LightDM.
///
/// LightDM potrafi wystartować sesję Wayland, **zanim ubije greeter** (Xorg), który
/// trzyma master DRM. Bez ponowień Selene od razu widzi `MasterBusy` i zostawia czarny
/// ekran. Czekamy do ~5 s, aż greeter zwolni mastera.
fn open_drm_waiting() -> Result<DrmBackend, DrmError> {
    match DrmBackend::open() {
        Err(DrmError::MasterBusy) => {
            for _ in 0..50 {
                std::thread::sleep(Duration::from_millis(100));
                match DrmBackend::open() {
                    Err(DrmError::MasterBusy) => continue,
                    other => return other,
                }
            }
            Err(DrmError::MasterBusy)
        }
        other => other,
    }
}

/// Uruchamia wątek wyjścia DRM/KMS — renderuje scenę do buforów i skanuje na monitorach.
///
/// Wymaga bycia DRM masterem; jeśli inną sesję trzyma master, kończy bez efektu
/// (wywołujący powinien wtedy użyć wyjścia nested).
pub fn spawn_drm(model: WindowModel, cursor: CursorModel) {
    std::thread::spawn(move || {
        let mut backend = match open_drm_waiting() {
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

        // Diagnostyka (SELENE_STATS=1): fps i rozbicie czasu na render/present/wait_flip.
        let stats = std::env::var("SELENE_STATS").is_ok();
        let mut flip_error_logged = false;
        let mut frames = 0u64;
        let (mut ms_render, mut ms_present, mut ms_wait) = (0.0f64, 0.0f64, 0.0f64);
        let mut report = std::time::Instant::now();
        loop {
            for i in 0..sizes.len() {
                let (w, h) = sizes[i];
                let (ox, oy) = origins[i];
                let cur = *cursor.lock_or_recover();

                let t = std::time::Instant::now();
                let mut scene = Scene::new();
                build_scene(
                    &mut scene,
                    Size::new(w as f64, h as f64),
                    ox,
                    oy,
                    &model,
                    cur,
                );
                renderers[i].draw(&scene, 1.0, bg);
                ms_render += t.elapsed().as_secs_f64() * 1000.0;

                let t = std::time::Instant::now();
                if !backend.present(i, renderers[i].data()) && !flip_error_logged {
                    eprintln!(
                        "{}",
                        libluna_i18n::tr("DRM page-flip failed (screen may stay blank)")
                    );
                    flip_error_logged = true;
                }
                ms_present += t.elapsed().as_secs_f64() * 1000.0;

                let t = std::time::Instant::now();
                backend.wait_flip();
                ms_wait += t.elapsed().as_secs_f64() * 1000.0;
            }
            // `wait_flip` i tak synchronizuje z vblankiem, więc nie dosypujemy snu
            // (wcześniejsze 16 ms samo ograniczało do ~20 fps).
            frames += 1;

            if stats && report.elapsed().as_secs_f64() >= 2.0 {
                let secs = report.elapsed().as_secs_f64();
                let per_frame = (frames * sizes.len() as u64).max(1) as f64;
                eprintln!(
                    "selene/drm: {:.1} fps | render {:.2} ms, present {:.2} ms, wait_flip {:.2} ms",
                    frames as f64 / secs,
                    ms_render / per_frame,
                    ms_present / per_frame,
                    ms_wait / per_frame
                );
                frames = 0;
                ms_render = 0.0;
                ms_present = 0.0;
                ms_wait = 0.0;
                report = std::time::Instant::now();
            }
        }
    });
}

/// Buduje scenę okien w rozmiarze wyjścia, przesuniętą o `(ox, oy)` (origin monitora).
fn build_scene(
    scene: &mut Scene,
    size: Size,
    ox: f64,
    oy: f64,
    model: &WindowModel,
    cursor: Option<Point>,
) {
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

    // Kursor rysuje ten monitor, na którym leży (tryb DRM — nikt go za nas nie narysuje).
    if let Some(c) = cursor {
        let (cx, cy) = (c.x - ox, c.y - oy);
        if cx >= 0.0 && cy >= 0.0 && cx < size.width && cy < size.height {
            draw_cursor(scene, Point::new(cx, cy));
        }
    }
}

/// Klasyczna strzałka kursora (biała z ciemnym obrysem).
fn draw_cursor(scene: &mut Scene, p: Point) {
    let arrow = [
        Point::new(p.x, p.y),
        Point::new(p.x, p.y + 17.0),
        Point::new(p.x + 4.2, p.y + 12.6),
        Point::new(p.x + 7.0, p.y + 18.5),
        Point::new(p.x + 9.7, p.y + 17.2),
        Point::new(p.x + 6.9, p.y + 11.4),
        Point::new(p.x + 11.5, p.y + 11.4),
    ];
    let white = Color::from_rgba8(0xf5, 0xf5, 0xf5, 0xff);
    let outline = Color::from_rgba8(0x10, 0x10, 0x10, 0xff);
    scene.fill_polygon(&arrow, white);
    let mut closed = arrow.to_vec();
    closed.push(arrow[0]);
    scene.stroke_polyline(&closed, 1.5, outline);
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_render::SoftwareRenderer;

    #[test]
    fn draw_cursor_paints_pixels() {
        let mut scene = Scene::new();
        draw_cursor(&mut scene, Point::new(8.0, 8.0));
        let mut renderer = SoftwareRenderer::new(48, 48);
        renderer.draw(&scene, 1.0, Color::from_rgba8(0, 0, 0, 255));
        let painted = renderer
            .data()
            .chunks_exact(4)
            .any(|px| px[0] > 0 || px[1] > 0 || px[2] > 0);
        assert!(painted, "kursor powinien zostawić niezerowe piksele");
    }

    #[test]
    #[ignore = "benchmark reczny"]
    fn bench_1080p_frame() {
        use std::sync::{Arc, Mutex};
        use std::time::Instant;
        let model: WindowModel = Arc::new(Mutex::new(Vec::new()));
        let bg = Color::from_rgba8(0x1e, 0x20, 0x28, 0xff);
        let mut renderer = SoftwareRenderer::new(1920, 1080);
        let n = 30;
        let t = Instant::now();
        for _ in 0..n {
            let mut scene = Scene::new();
            build_scene(
                &mut scene,
                Size::new(1920.0, 1080.0),
                0.0,
                0.0,
                &model,
                Some(Point::new(100.0, 100.0)),
            );
            renderer.draw(&scene, 1.0, bg);
        }
        println!(
            "render 1920x1080: {:.2} ms/klatke",
            t.elapsed().as_secs_f64() * 1000.0 / n as f64
        );
    }
}
