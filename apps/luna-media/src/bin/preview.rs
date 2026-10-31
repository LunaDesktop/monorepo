//! Headless-podgląd przeglądarki do dokumentacji (bez kompozytora).
//!
//! Uruchom: `cargo run --bin preview`
//! → `docs/images/luna-media.png` (galeria) i `docs/images/luna-media-viewer.png` (podgląd).

use std::error::Error;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use libluna_core::{Color, Point, Rect, Size, Theme};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use luna_media::app::App;
use luna_media::model::MediaKind;
use luna_media::prefs::Prefs;
use luna_media::ui;

const W: u32 = 1120;
const H: u32 = 720;

fn main() -> Result<(), Box<dyn Error>> {
    let dir = std::env::temp_dir().join("luna-media-preview");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;

    // Kolorowe plansze zamiast prawdziwych zdjęć — podgląd nie zależy od dysku użytkownika.
    let hues = [
        (0x35, 0x84, 0xe4),
        (0xe4, 0x6a, 0x35),
        (0x35, 0xb4, 0xa0),
        (0xb0, 0x35, 0xe4),
        (0xe4, 0xc3, 0x35),
        (0xe4, 0x35, 0x6a),
        (0x55, 0x9e, 0x35),
        (0x7e, 0x7e, 0x7e),
    ];
    for (index, (r, g, b)) in hues.iter().enumerate() {
        let color = Color::from_rgba8(*r, *g, *b, 0xff);
        let path = dir.join(format!("plan-{:02}.png", index + 1));
        std::fs::write(path, sample_png(color, index))?;
    }
    // Kilka „filmów”: ffmpeg generuje krótkie plansze testowe (jeśli jest dostępny).
    for name in ["klip-1.mp4", "klip-2.mp4"] {
        let target = dir.join(name);
        let _ = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=640x360:rate=25,trim=duration=2,setpts=PTS-STARTPTS",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&target)
            .status();
    }

    // Podkatalogi — w galerii pokazuje je pasek folderów pod nawigacją.
    std::fs::create_dir_all(dir.join("Album 2"))?;
    std::fs::create_dir_all(dir.join("Album 10"))?;
    std::fs::copy(dir.join("plan-01.png"), dir.join("Album 2/wycieczka.png"))?;

    // PDF (poppler) i RAW z osadzonym JPEG — jeśli narzędzia są dostępne.
    let pdf = dir.join("folder.pdf");
    let _ = Command::new("gs")
        .args(["-q", "-dBATCH", "-dNOPAUSE", "-sDEVICE=pdfwrite"])
        .arg(format!("-sOutputFile={}", pdf.display()))
        .args([
            "-c",
            "<</PageSize [420 594]>> setpagedevice 0.92 0.95 1 setrgbcolor 0 0 420 594 rectfill 0.2 0.4 0.8 setrgbcolor 40 470 340 80 rectfill 0 0 0 setrgbcolor /Helvetica findfont 22 scalefont setfont 50 510 moveto (Luna Media PDF) show 50 480 moveto (Strona pierwsza) show showpage 0.95 0.92 1 setrgbcolor 0 0 420 594 rectfill 0.5 0.3 0.7 setrgbcolor 40 120 340 350 rectfill 0 0 0 setrgbcolor /Helvetica findfont 24 scalefont setfont 60 500 moveto (Tekst do zaznaczenia) show 60 465 moveto (Shift i przeciagniecie) show 60 430 moveto (Ctrl+C kopiuje) show showpage",
        ])
        .status();
    if let Ok(ffmpeg) = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=800x600:rate=1",
            "-frames:v",
            "1",
        ])
        .arg(dir.join("raw-preview.jpg"))
        .status()
    {
        if ffmpeg.success() {
            if let Ok(jpeg) = std::fs::read(dir.join("raw-preview.jpg")) {
                let mut raw = vec![0x2a; 4096];
                raw.extend_from_slice(&jpeg);
                raw.extend_from_slice(&[0x55; 2048]);
                std::fs::write(dir.join("foto.cr2"), raw)?;
            }
            let _ = std::fs::remove_file(dir.join("raw-preview.jpg"));
        }
    }

    let mut app = App::new(dir.clone(), None, Prefs::default());
    app.details = true;
    for item in &app.items {
        app.thumbs.ensure(&item.path, item.kind);
    }
    let theme = Theme::dark();
    let text = TextEngine::system();
    let size = Size::new(W as f64, H as f64);

    wait_for_thumbs(&mut app, Duration::from_secs(8));

    let mut scene = Scene::new();
    ui::paint(&app, &mut scene, size, &theme, &text);
    save(&scene, W, H, &theme, "luna-media.png")?;

    // Podgląd pierwszego obrazu (a nie filmu) z panelem szczegółów.
    if let Some(index) = app
        .items
        .iter()
        .position(|item| item.kind == MediaKind::Image)
    {
        app.open_viewer(index);
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.viewer.image.is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
            app.update(0.0);
        }
        let mut scene = Scene::new();
        ui::paint(&app, &mut scene, size, &theme, &text);
        save(&scene, W, H, &theme, "luna-media-viewer.png")?;
    }

    // Podgląd odtwarzacza filmów (bez dźwięku — to tylko zrzut do dokumentacji).
    if let Some(index) = app
        .items
        .iter()
        .position(|item| item.kind == MediaKind::Video)
    {
        app.prefs.muted = true;
        app.open_viewer(index);
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let ready = app
                .viewer
                .video
                .as_ref()
                .and_then(|video| video.frame())
                .is_some();
            if ready || Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
            app.update(0.0);
        }
        let mut scene = Scene::new();
        ui::paint(&app, &mut scene, size, &theme, &text);
        save(&scene, W, H, &theme, "luna-media-video.png")?;
    }

    // Podgląd PDF (druga strona), jeśli poppler wyrenderował plik.
    if let Some(index) = app
        .items
        .iter()
        .position(|item| item.kind == MediaKind::Pdf)
    {
        app.open_viewer(index);
        wait_for_image(&mut app, Duration::from_secs(8));
        app.pdf_step(1);
        wait_for_image(&mut app, Duration::from_secs(8));
        if let Some(words) = app.viewer.pdf_text.as_ref().map(|text| text.words.len()) {
            if words > 0 {
                app.viewer.text_sel = Some((0, (words - 1).min(5)));
            }
        }
        let mut scene = Scene::new();
        ui::paint(&app, &mut scene, size, &theme, &text);
        save(&scene, W, H, &theme, "luna-media-pdf.png")?;
    }

    drop(app);
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

fn wait_for_image(app: &mut App, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while (app.viewer.image.is_none() || app.viewer.loading) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        app.update(0.0);
    }
}

fn wait_for_thumbs(app: &mut App, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while app.thumbs.pending() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        app.update(0.0);
    }
}

fn sample_png(color: Color, index: usize) -> Vec<u8> {
    let (w, h) = (640u32, 420u32);
    let mut scene = Scene::new();
    scene.fill_rect(Rect::from_xywh(0.0, 0.0, w as f64, h as f64), color);
    let accent = Color::rgba(1.0, 1.0, 1.0, 0.25);
    let dark = Color::rgba(0.0, 0.0, 0.0, 0.20);
    let x = 90.0 + (index as f64 * 37.0) % 220.0;
    scene.fill_circle(Point::new(x, 150.0), 74.0, accent);
    scene.fill_rounded_rect(Rect::from_xywh(120.0, 270.0, 330.0, 22.0), 11.0, dark);
    scene.fill_rounded_rect(Rect::from_xywh(120.0, 306.0, 210.0, 18.0), 9.0, dark);
    scene.stroke_polyline(
        &[
            Point::new(40.0, 380.0),
            Point::new(200.0, 330.0),
            Point::new(360.0, 360.0),
            Point::new(520.0, 300.0),
            Point::new(600.0, 320.0),
        ],
        6.0,
        accent,
    );

    let mut renderer = SoftwareRenderer::new(w, h);
    renderer.draw(&scene, 1.0, color);
    renderer.encode_png().unwrap_or_default()
}

fn save(scene: &Scene, w: u32, h: u32, theme: &Theme, name: &str) -> Result<(), Box<dyn Error>> {
    let mut renderer = SoftwareRenderer::new(w, h);
    renderer.draw(scene, 1.0, theme.colors.window_bg);
    let png = renderer.encode_png().ok_or("nie udało się zakodować PNG")?;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/images")
        .join(name);
    std::fs::write(&path, png)?;
    println!("saved {}", path.display());
    Ok(())
}
