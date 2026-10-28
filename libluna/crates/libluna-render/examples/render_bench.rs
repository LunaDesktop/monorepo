//! Benchmark rasteryzacji — baseline dla decyzji o backendzie GPU.
//!
//! Uruchomienie (release jest obowiązkowy, inaczej liczby nie mają sensu):
//!
//! ```sh
//! cargo run --release --manifest-path libluna/Cargo.toml \
//!     -p libluna-render --example render_bench
//! ```
//!
//! Opcje:
//! - `--json` — wynik w JSON (do CI/porównań),
//! - `--iters N` — liczba klatek na pomiar (domyślnie 30),
//! - `--width W --height H` — zamiast standardowego zestawu rozdzielczości.
//!
//! Mierzymy **samą rasteryzację** (`Renderer::draw`), bez `wl_shm` i kompozytora.
//! Czasy obejmują wypełnienie tła, kształty, clip, tekst i obrazy.

use std::sync::Arc;
use std::time::{Duration, Instant};

use libluna_core::{Color, Point, Rect};
use libluna_render::{ImageData, Renderer, Scene, SoftwareRenderer, TextRun};

const RESOLUTIONS: [(u32, u32); 3] = [(1280, 800), (1920, 1080), (2560, 1440)];
const SCENES: [&str; 6] = ["rects", "rounded", "text", "clip", "icons", "mixed"];

#[derive(Clone, Copy)]
struct Args {
    json: bool,
    iters: u32,
    only: Option<(u32, u32)>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            json: false,
            iters: 30,
            only: None,
        }
    }
}

fn parse_args() -> Args {
    let mut args = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--json" => args.json = true,
            "--iters" => {
                if let Some(v) = it.next().and_then(|v| v.parse::<u32>().ok()) {
                    args.iters = v.max(1);
                }
            }
            "--width" => {
                let w = it.next().and_then(|v| v.parse::<u32>().ok());
                let h = it.next().and_then(|v| v.parse::<u32>().ok());
                if let (Some(w), Some(h)) = (w, h) {
                    args.only = Some((w, h));
                }
            }
            _ => {}
        }
    }
    args
}

struct Measurement {
    scene: &'static str,
    width: u32,
    height: u32,
    commands: usize,
    avg: Duration,
}

impl Measurement {
    fn fps(&self) -> f64 {
        1.0 / self.avg.as_secs_f64().max(f64::EPSILON)
    }

    fn mpix_per_s(&self) -> f64 {
        (self.width as f64 * self.height as f64 * self.fps()) / 1_000_000.0
    }

    fn gb_per_s(&self) -> f64 {
        (self.width as f64 * self.height as f64 * 4.0 * self.fps()) / 1_000_000_000.0
    }
}

fn main() {
    let args = parse_args();
    let resolutions: Vec<(u32, u32)> = match args.only {
        Some(size) => vec![size],
        None => RESOLUTIONS.to_vec(),
    };

    let mut all = Vec::new();
    let mut renderer: Box<dyn Renderer> =
        Box::new(SoftwareRenderer::new(resolutions[0].0, resolutions[0].1));

    for (w, h) in resolutions {
        renderer.resize(w, h);
        println!(
            "\n== {}×{} ({:.2} Mpx) — backend: {} ==",
            w,
            h,
            (w as f64 * h as f64) / 1_000_000.0,
            renderer.name()
        );
        println!(
            "{:<8} {:>8} {:>10} {:>10} {:>10} {:>10}",
            "scena", "komendy", "śr. ms", "FPS", "Mpx/s", "GB/s"
        );
        for scene_name in SCENES {
            let scene = build_scene(scene_name, w, h);
            let measurement = measure(&mut *renderer, scene_name, &scene, w, h, args.iters);
            println!(
                "{:<8} {:>8} {:>10.3} {:>10.1} {:>10.1} {:>10.2}",
                measurement.scene,
                measurement.commands,
                measurement.avg.as_secs_f64() * 1000.0,
                measurement.fps(),
                measurement.mpix_per_s(),
                measurement.gb_per_s()
            );
            all.push(measurement);
        }
    }

    // PNG to osobna ścieżka (kompresja), nie wchodzi do średniej klatki.
    let scene = build_scene("mixed", 1920, 1080);
    renderer.resize(1920, 1080);
    renderer.draw(&scene, 1.0, background());
    let t = Instant::now();
    let png = renderer.encode_png();
    let encode = t.elapsed();
    if let Some(bytes) = &png {
        println!(
            "\nPNG 1920×1080: {:.1} ms, {:.1} MiB",
            encode.as_secs_f64() * 1000.0,
            bytes.len() as f64 / (1024.0 * 1024.0)
        );
    }

    println!("\nPamięć (VmHWM): {}", peak_memory_mib());

    if args.json {
        println!("\n{}", to_json(&all));
    }
}

fn background() -> Color {
    Color::from_rgba8(0x10, 0x12, 0x18, 0xff)
}

fn measure(
    renderer: &mut dyn Renderer,
    scene_name: &'static str,
    scene: &Scene,
    width: u32,
    height: u32,
    iters: u32,
) -> Measurement {
    renderer.resize(width, height);
    // Rozgrzewka: alokacje, cache glifów, cache ścieżek.
    for _ in 0..3 {
        renderer.draw(scene, 1.0, background());
    }
    let started = Instant::now();
    for _ in 0..iters {
        renderer.draw(scene, 1.0, background());
    }
    let avg = started.elapsed() / iters;
    Measurement {
        scene: scene_name,
        width,
        height,
        commands: scene.commands().len(),
        avg,
    }
}

/// Deterministyczny „losowy” rozkład — bez zależności od `rand`.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f64) / (u32::MAX as f64 / 2.0)
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next() * (hi - lo)
    }
}

fn palette() -> [Color; 6] {
    [
        Color::from_rgba8(0x4c, 0x7d, 0xff, 0xff),
        Color::from_rgba8(0x34, 0xc7, 0x9b, 0xff),
        Color::from_rgba8(0xff, 0xb3, 0x4d, 0xff),
        Color::from_rgba8(0xff, 0x6b, 0x8a, 0xff),
        Color::from_rgba8(0xc7, 0xd2, 0xeb, 0xff),
        Color::from_rgba8(0x8a, 0x7d, 0xff, 0xff),
    ]
}

fn checkerboard() -> Arc<ImageData> {
    let (w, h) = (32u32, 32u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let on = ((x / 4) + (y / 4)) % 2 == 0;
            let i = ((y * w + x) * 4) as usize;
            let v = if on { 220 } else { 60 };
            rgba[i..i + 4].copy_from_slice(&[v, v / 2, 255 - v, 255]);
        }
    }
    Arc::new(ImageData::new(w, h, rgba))
}

fn build_scene(name: &str, width: u32, height: u32) -> Scene {
    let mut scene = Scene::new();
    let mut rng = Lcg(0x1234_5678_9abc_def0);
    let colors = palette();
    let w = width as f64;
    let h = height as f64;
    let image = checkerboard();

    match name {
        "rects" => {
            for i in 0..900 {
                let x = rng.range(0.0, w);
                let y = rng.range(0.0, h);
                let rw = rng.range(4.0, 160.0);
                let rh = rng.range(2.0, 40.0);
                scene.fill_rect(
                    Rect::from_xywh(x, y, rw, rh),
                    colors[i % colors.len()].scale_alpha(0.8),
                );
            }
        }
        "rounded" => {
            for i in 0..700 {
                let x = rng.range(0.0, w);
                let y = rng.range(0.0, h);
                let rw = rng.range(20.0, 220.0);
                let rh = rng.range(14.0, 80.0);
                let radius = rng.range(2.0, 16.0);
                let rect = Rect::from_xywh(x, y, rw, rh);
                scene.fill_rounded_rect(rect, radius, colors[i % colors.len()].scale_alpha(0.7));
                scene.stroke_rounded_rect(rect, radius, 1.0, Color::rgba(1.0, 1.0, 1.0, 0.25));
            }
        }
        "text" => {
            // 400 krótkich napisów — typowy panel/listy ustawień.
            for i in 0..400 {
                let x = rng.range(0.0, w * 0.8);
                let y = rng.range(12.0, h - 4.0);
                let size = 11.0 + (i % 5) as f64 * 2.0;
                scene.text(
                    TextRun::new(
                        Point::new(x, y),
                        format!("Pozycja {i}: wartość {}", i % 97),
                        size,
                    )
                    .with_color(colors[i % colors.len()]),
                );
            }
        }
        "clip" => {
            let cols = 8;
            let rows = 6;
            let cw = w / cols as f64;
            let ch = h / rows as f64;
            for row in 0..rows {
                for col in 0..cols {
                    let cell = Rect::from_xywh(
                        col as f64 * cw + 6.0,
                        row as f64 * ch + 6.0,
                        cw - 12.0,
                        ch - 12.0,
                    );
                    scene.push_clip_rounded(cell, 12.0);
                    for i in 0..8 {
                        let x = cell.x() + rng.range(-40.0, cell.width());
                        let y = cell.y() + rng.range(-20.0, cell.height());
                        scene.fill_rounded_rect(
                            Rect::from_xywh(x, y, rng.range(20.0, 120.0), rng.range(8.0, 40.0)),
                            6.0,
                            colors[(row + col + i) % colors.len()].scale_alpha(0.65),
                        );
                    }
                    scene.pop_clip();
                }
            }
        }
        "icons" => {
            // Ikony wektorowe: okręgi + łamane + wielokąty. W realnym UI te same
            // kształty wracają co klatkę, więc testują cache ścieżek.
            for i in 0..250 {
                let x = rng.range(0.0, w - 40.0);
                let y = rng.range(0.0, h - 40.0);
                let color = colors[i % colors.len()];
                let cx = x + 12.0;
                let cy = y + 12.0;
                scene.stroke_circle(Point::new(cx, cy), 7.0, 1.4, color);
                let line = [
                    Point::new(x + 2.0, y + 28.0),
                    Point::new(x + 12.0, y + 20.0),
                    Point::new(x + 22.0, y + 28.0),
                    Point::new(x + 32.0, y + 18.0),
                ];
                scene.stroke_polyline(&line, 1.6, color.scale_alpha(0.9));
                let poly = [
                    Point::new(x + 4.0, y + 6.0),
                    Point::new(x + 12.0, y + 2.0),
                    Point::new(x + 20.0, y + 9.0),
                    Point::new(x + 9.0, y + 14.0),
                ];
                scene.fill_polygon(&poly, color.scale_alpha(0.45));
            }
        }
        _ => {
            // Mieszanka: karty, ikony-obrazki, tekst i przycięte listy.
            for i in 0..180 {
                let x = rng.range(0.0, w);
                let y = rng.range(0.0, h);
                let card = Rect::from_xywh(x, y, rng.range(80.0, 320.0), rng.range(50.0, 180.0));
                scene.fill_rounded_rect(card, 12.0, colors[i % colors.len()].scale_alpha(0.35));
                scene.stroke_rounded_rect(card, 12.0, 1.0, Color::rgba(1.0, 1.0, 1.0, 0.18));
                let icon = Rect::from_xywh(card.x() + 12.0, card.y() + 12.0, 32.0, 32.0);
                scene.image(icon, Arc::clone(&image));
                scene.text(
                    TextRun::new(
                        Point::new(card.x() + 54.0, card.y() + 32.0),
                        format!("Widget {i}"),
                        14.0,
                    )
                    .with_color(Color::rgba(1.0, 1.0, 1.0, 0.9)),
                );
                scene.push_clip(card);
                for line in 0..4 {
                    let y = card.y() + 52.0 + line as f64 * 18.0;
                    scene.text(
                        TextRun::new(
                            Point::new(card.x() + 16.0, y),
                            "przykładowy wiersz listy",
                            12.0,
                        )
                        .with_color(Color::rgba(1.0, 1.0, 1.0, 0.55)),
                    );
                }
                scene.pop_clip();
            }
        }
    }
    scene
}

fn peak_memory_mib() -> String {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return "niedostępne".to_owned();
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            if let Some(kib) = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok())
            {
                return format!("{:.1} MiB", kib as f64 / 1024.0);
            }
        }
    }
    "niedostępne".to_owned()
}

fn to_json(rows: &[Measurement]) -> String {
    let items: Vec<String> = rows
        .iter()
        .map(|m| {
            format!(
                "{{\"scene\":\"{}\",\"width\":{},\"height\":{},\"commands\":{},\"avg_ms\":{:.4},\"fps\":{:.2},\"mpix_s\":{:.2},\"gb_s\":{:.3}}}",
                m.scene,
                m.width,
                m.height,
                m.commands,
                m.avg.as_secs_f64() * 1000.0,
                m.fps(),
                m.mpix_per_s(),
                m.gb_per_s()
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}
