//! Benchmark scen zbliżonych do **realnego pulpitu Selene**.
//!
//! `render_bench` mierzy syntetyczne kształty. Ten przykład odtwarza układ z sesji:
//! tapeta pełnoekranowa, panel (rzadki obraz) i duże okna klientów — z zaokrąglonym
//! clipem i bez. Pozwala ocenić, ile kosztuje blit obrazu, maska clipu i panel,
//! zanim zmieni się cokolwiek w kompozytorze.
//!
//! ```sh
//! cargo run --release --manifest-path libluna/Cargo.toml \
//!     -p libluna-render --example scene_bench
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use libluna_core::{Color, Rect};
use libluna_render::{ImageData, Scene, SoftwareRenderer};

fn opaque_image(w: u32, h: u32, tint: u8) -> Arc<ImageData> {
    let mut rgba = vec![0u8; w as usize * h as usize * 4];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = (y * w as usize + x) * 4;
            rgba[i] = (x as u8).wrapping_add(tint);
            rgba[i + 1] = (y as u8).wrapping_add(tint);
            rgba[i + 2] = tint;
            rgba[i + 3] = 255;
        }
    }
    Arc::new(ImageData::new(w, h, rgba).with_opaque(true))
}

/// Panel: obraz z przezroczystym dołem (tak wygląda bufor `top_chrome`).
fn sparse_panel(w: u32, h: u32) -> Arc<ImageData> {
    let mut rgba = vec![0u8; w as usize * h as usize * 4];
    for y in 0..(h as usize / 2) {
        for x in 0..w as usize {
            let i = (y * w as usize + x) * 4;
            rgba[i] = 0x28;
            rgba[i + 1] = 0x2c;
            rgba[i + 2] = 0x38;
            rgba[i + 3] = 255;
        }
    }
    Arc::new(ImageData::new(w, h, rgba).with_sparse(true))
}

fn bench(name: &str, width: u32, height: u32, iters: u32, build: impl Fn() -> Scene) -> Duration {
    let mut r = SoftwareRenderer::new(width, height);
    let scene = build();
    // Rozgrzewka: cache ścieżek i alokacje.
    r.draw(&scene, 1.0, Color::BLACK);
    let start = Instant::now();
    for _ in 0..iters {
        r.draw(&scene, 1.0, Color::BLACK);
    }
    let avg = start.elapsed() / iters;
    println!(
        "  {:<22} {:>8.3} ms  {:>6.1} FPS  ({} cmd)",
        name,
        avg.as_secs_f64() * 1000.0,
        1.0 / avg.as_secs_f64().max(f64::EPSILON),
        scene.len()
    );
    avg
}

fn main() {
    let iters = std::env::args()
        .nth(1)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(20)
        .max(1);

    for (w, h) in [(1920u32, 1080u32), (1600, 900)] {
        println!("\n== {w}×{h} (realistyczny pulpit) ==");
        let bg = opaque_image(w, h, 10);
        let panel = sparse_panel(w, 48);
        let win = opaque_image(w, (h as f64 * 0.928) as u32, 60);
        let win_h = win.height;

        bench("tło", w, h, iters, || {
            let mut s = Scene::new();
            s.background(Arc::clone(&bg));
            s
        });

        bench("tło+panel", w, h, iters, || {
            let mut s = Scene::new();
            s.background(Arc::clone(&bg));
            s.image(Rect::from_xywh(0.0, 0.0, w as f64, 48.0), panel.clone());
            s
        });

        bench("tło+okno", w, h, iters, || {
            let mut s = Scene::new();
            s.background(Arc::clone(&bg));
            s.image(
                Rect::from_xywh(0.0, 40.0, w as f64, win_h as f64),
                win.clone(),
            );
            s
        });

        bench("tło+okno+clip", w, h, iters, || {
            let mut s = Scene::new();
            s.background(Arc::clone(&bg));
            s.push_clip_rounded(Rect::from_xywh(0.0, 40.0, w as f64, win_h as f64), 12.0);
            s.image(
                Rect::from_xywh(0.0, 40.0, w as f64, win_h as f64),
                win.clone(),
            );
            s.pop_clip();
            s
        });

        let term = opaque_image(900, 600, 120);
        bench("900x600+clip", w, h, iters, || {
            let mut s = Scene::new();
            s.background(Arc::clone(&bg));
            s.push_clip_rounded(Rect::from_xywh(300.0, 121.0, 900.0, 600.0), 12.0);
            s.image(Rect::from_xywh(300.0, 121.0, 900.0, 600.0), term.clone());
            s.pop_clip();
            s
        });

        bench("desktop (2 okna)", w, h, iters, || {
            let mut s = Scene::new();
            s.background(Arc::clone(&bg));
            s.image(Rect::from_xywh(0.0, 0.0, w as f64, 48.0), panel.clone());
            // Okno na całą szerokość — bez clipu, jak w `build_scene`.
            s.image(
                Rect::from_xywh(0.0, 40.0, w as f64, win_h as f64),
                win.clone(),
            );
            s.push_clip_rounded(Rect::from_xywh(300.0, 121.0, 900.0, 600.0), 12.0);
            s.image(Rect::from_xywh(300.0, 121.0, 900.0, 600.0), term.clone());
            s.pop_clip();
            s
        });

        bench("desktop+dim", w, h, iters, || {
            let mut s = Scene::new();
            s.background(Arc::clone(&bg));
            s.image(Rect::from_xywh(0.0, 0.0, w as f64, 48.0), panel.clone());
            s.image(
                Rect::from_xywh(0.0, 40.0, w as f64, win_h as f64),
                win.clone(),
            );
            // Nieaktywne okno — pełny prostokąt przygaszony (jak w `build_scene`).
            s.fill_rect(
                Rect::from_xywh(0.0, 40.0, w as f64, win_h as f64),
                Color::from_rgba8(0, 0, 0, 0x40),
            );
            s.push_clip_rounded(Rect::from_xywh(300.0, 121.0, 900.0, 600.0), 12.0);
            s.image(Rect::from_xywh(300.0, 121.0, 900.0, 600.0), term.clone());
            s.fill_rect(
                Rect::from_xywh(300.0, 121.0, 900.0, 600.0),
                Color::from_rgba8(0, 0, 0, 0x40),
            );
            s.pop_clip();
            s
        });

        // Okno z innego monitora: w scenie jest, ale poza buforem.
        bench("desktop+offscreen", w, h, iters, || {
            let mut s = Scene::new();
            s.background(Arc::clone(&bg));
            s.image(Rect::from_xywh(0.0, 0.0, w as f64, 48.0), panel.clone());
            s.push_clip_rounded(Rect::from_xywh(w as f64, 0.0, w as f64, win_h as f64), 12.0);
            s.image(
                Rect::from_xywh(w as f64, 0.0, w as f64, win_h as f64),
                win.clone(),
            );
            s.pop_clip();
            s
        });
    }
}
