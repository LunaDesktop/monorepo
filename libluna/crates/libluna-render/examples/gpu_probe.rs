//! Benchmark backendu GPU vs CPU dla scen pulpitu.
//!
//! Uruchomienie (release obowiązkowo):
//!
//! ```sh
//! cargo run --release --manifest-path libluna/Cargo.toml \
//!     -p libluna-render --example gpu_probe
//!
//! # JSON do CI / porównań
//! cargo run --release ... --example gpu_probe -- --json
//! ```
//!
//! Mierzy **ten sam** [`Renderer`] co aplikacje: [`GpuRenderer`] (EGL surfaceless +
//! readback) i [`SoftwareRenderer`]. Sceny:
//!
//! - `windows` — 30 buforów okien 640×480 (czyste mieszanie pikseli),
//! - `desktop` — tło 1920×1080, 6 okien, paski tytułu i kursor (realny miks
//!   GPU + tekst z atlasu glifów + warstwa CPU na wektor).
//!
//! Wypisuje też czas samego `glReadPixels` — przy wyjściu bez zero-copy to zwykle
//! największa część budżetu klatki (patrz `docs/gpu.md`, etap 4).
//!
//! Kody wyjścia: `0` = OK, `2` = brak EGL, `3` = brak kontekstu.

use std::sync::Arc;

use libluna_core::{Color, Point, Rect};
use libluna_render::gpu::{self, GpuRenderer};
use libluna_render::{ImageData, Renderer, Scene, SoftwareRenderer, TextRun};

const W: u32 = 1920;
const H: u32 = 1080;

fn main() {
    let json = std::env::args().skip(1).any(|a| a == "--json");
    let Some(info) = gpu::info() else {
        let msg = "brak EGL/GLES (GPU niedostępne)";
        if json {
            println!("{{\"ok\":false,\"error\":\"{msg}\"}}");
        } else {
            eprintln!("GPU: {msg}");
        }
        std::process::exit(2);
    };
    if !json {
        println!("EGL {} ({})", info.egl_version, info.egl_vendor);
        println!("GL  {} ({})", info.version, info.vendor);
        println!("GPU {}", info.renderer);
        for (name, ok) in [
            ("EGL_EXT_image_dma_buf_import", info.has_dma_buf_import()),
            (
                "EGL_KHR_fence_sync",
                info.extensions.contains("EGL_KHR_fence_sync"),
            ),
            (
                "EGL_KHR_surfaceless_context",
                info.extensions.contains("EGL_KHR_surfaceless_context"),
            ),
        ] {
            println!("  {name:<42} {}", if ok { "tak" } else { "nie" });
        }
        let formats = gpu::dmabuf_formats();
        let pairs: usize = formats.iter().map(|(_, m)| m.len()).sum();
        println!(
            "  dmabuf: {} formatów, {} par format/modyfikator",
            formats.len(),
            pairs
        );
        let interesting: Vec<String> = formats
            .iter()
            .take(8)
            .map(|(f, _)| format!("0x{f:08x}"))
            .collect();
        if !interesting.is_empty() {
            println!("  formaty: {}", interesting.join(", "));
        }
        println!();
    }

    let mut gpu = match GpuRenderer::new(W, H) {
        Ok(r) => r,
        Err(e) => {
            if json {
                println!("{{\"ok\":false,\"error\":\"{}\"}}", json_str(&e));
            } else {
                eprintln!("GPU: {e}");
            }
            std::process::exit(3);
        }
    };
    let mut cpu = SoftwareRenderer::new(W, H);
    let bg = Color::from_rgba8(0x10, 0x14, 0x1c, 0xff);

    let windows = scene_windows();
    let desktop = scene_desktop();

    // Rozgrzewka: pierwsza klatka zawiera upload tekstur i kompilację shaderów.
    gpu.draw(&windows, 1.0, bg);
    gpu.draw(&desktop, 1.0, bg);
    cpu.draw(&windows, 1.0, bg);
    cpu.draw(&desktop, 1.0, bg);

    let iters = 20;
    let run = |scene: &Scene, renderer: &mut dyn Renderer| {
        let t = std::time::Instant::now();
        for _ in 0..iters {
            renderer.draw(scene, 1.0, bg);
        }
        t.elapsed().as_secs_f64() * 1000.0 / f64::from(iters)
    };

    let gpu_windows = run(&windows, &mut gpu);
    let cpu_windows = run(&windows, &mut cpu);
    let gpu_desktop = run(&desktop, &mut gpu);
    let readback = f64::from(gpu.readback_ms());
    let cpu_desktop = run(&desktop, &mut cpu);

    // Ten sam pulpit, ale odczyt z GPU ograniczony do małego prostokąta — tak
    // wygląda typowa zmiana (zegar w panelu, kursor tekstowy, podświetlenie).
    let small = Rect::from_xywh(240.0, 160.0, 320.0, 160.0);
    let gpu_damage = {
        let t = std::time::Instant::now();
        for _ in 0..iters {
            gpu.draw_damage(&desktop, 1.0, bg, Some(small));
        }
        t.elapsed().as_secs_f64() * 1000.0 / f64::from(iters)
    };
    let damage_readback = f64::from(gpu.readback_ms());

    if json {
        println!(
            "{{\"ok\":true,\"gpu\":{},\"gl\":{},\"egl\":{},\"dmabuf_import\":{},\
             \"windows_gpu_ms\":{gpu_windows:.2},\"windows_cpu_ms\":{cpu_windows:.2},\
             \"desktop_gpu_ms\":{gpu_desktop:.2},\"desktop_cpu_ms\":{cpu_desktop:.2},\
             \"desktop_damage_gpu_ms\":{gpu_damage:.2}}}",
            json_str(&info.renderer),
            json_str(&info.version),
            json_str(&info.egl_version),
            info.has_dma_buf_import()
        );
        return;
    }

    println!("1920×1080, {iters} klatek po rozgrzewce (GPU zawiera readback):");
    println!();
    println!("  scena                GPU        CPU      GPU/CPU");
    println!(
        "  windows (30 okien)  {:6.2} ms  {:6.2} ms   {:5.1}×",
        gpu_windows,
        cpu_windows,
        cpu_windows / gpu_windows.max(0.01)
    );
    println!(
        "  desktop (tło+6 okien+SSD+kursor)  {:6.2} ms  {:6.2} ms   {:5.1}×",
        gpu_desktop,
        cpu_desktop,
        cpu_desktop / gpu_desktop.max(0.01)
    );
    println!();
    println!(
        "  z tego readback (glReadPixels, bez zero-copy): {:.2} ms, sam render ≈ {:.2} ms",
        readback,
        (gpu_desktop - readback).max(0.0)
    );
    println!(
        "  desktop + damage 320×160 (tylko ten prostokąt z GPU): {:6.2} ms (readback {:.2} ms)",
        gpu_damage, damage_readback
    );
    println!(
        "  GPU desktop → {:.1} fps, CPU desktop → {:.1} fps",
        1000.0 / gpu_desktop,
        1000.0 / cpu_desktop
    );
}

/// 30 buforów okien — czysta kompozycja obrazów.
fn scene_windows() -> Scene {
    let mut scene = Scene::new();
    for i in 0..30 {
        let img = patterned(640, 480, i);
        scene.image(
            Rect::from_xywh(
                (i as f64 * 41.0) % f64::from(W - 640),
                (i as f64 * 29.0) % f64::from(H - 480),
                640.0,
                480.0,
            ),
            Arc::new(img),
        );
    }
    scene
}

/// Realistyczny pulpit: tło, okna z paskami SSD, kursor.
fn scene_desktop() -> Scene {
    let mut scene = Scene::new();
    scene.background(Arc::new(patterned(W, H, 0)));
    for i in 0..6 {
        let (w, h) = (520.0 + i as f64 * 40.0, 360.0 + i as f64 * 30.0);
        let x = 80.0 + i as f64 * 70.0;
        let y = 120.0 + i as f64 * 55.0;
        let outer = Rect::from_xywh(x, y, w, h + 38.0);
        scene.push_clip_rounded(outer, 14.0);
        scene.image(
            Rect::from_xywh(x, y + 38.0, w, h),
            Arc::new(patterned(w as u32, h as u32, i + 1)),
        );
        scene.fill_rounded_rect(
            Rect::from_xywh(x, y, w, 38.0),
            14.0,
            Color::from_rgba8(0x2c, 0x2f, 0x38, 0xff),
        );
        scene.text(
            TextRun::new(
                Point::new(x + 12.0, y + 25.0),
                format!("Okno {i}: Terminal — projekt"),
                13.0,
            )
            .with_color(Color::from_rgba8(0xee, 0xee, 0xee, 0xff)),
        );
        scene.pop_clip();
    }
    // Kursor: wypełniony wielokąt + obrys (typowa ścieżka CPU).
    let c = Point::new(1200.0, 700.0);
    scene.fill_polygon(
        &[
            Point::new(c.x, c.y),
            Point::new(c.x, c.y + 18.0),
            Point::new(c.x + 5.0, c.y + 13.0),
            Point::new(c.x + 12.0, c.y + 12.0),
        ],
        Color::from_rgba8(0x20, 0x20, 0x20, 0xff),
    );
    scene
}

fn patterned(w: u32, h: u32, seed: usize) -> ImageData {
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let n = ((x / 16 + y / 16 + seed as u32) % 2) as u8;
            px[i] = 60 + n * 40;
            px[i + 1] = 80 + n * 30;
            px[i + 2] = 120 + (seed % 3) as u8 * 30;
            px[i + 3] = 255;
        }
    }
    ImageData::new(w, h, px)
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
