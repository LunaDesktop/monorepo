//! Dekodowanie obrazów i klatek wideo.
//!
//! Natywnie obsługujemy PNG (`tiny-skia`), JPEG (`jpeg-decoder`) i SVG (`resvg`).
//! Pozostałe formaty oraz filmy dekoduje `ffmpeg` — tak samo jak menedżer plików
//! korzysta z `bsdtar`/`7z`, zamiast wciągać ciężkie biblioteki.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, OnceLock};

use libluna_render::ImageData;

/// Czy w `PATH` jest program o tej nazwie.
pub fn in_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(name).is_file())
}

/// Czy `ffmpeg` jest dostępny (wynik jest zapamiętywany).
pub fn have_ffmpeg() -> bool {
    static HAVE: OnceLock<bool> = OnceLock::new();
    *HAVE.get_or_init(|| in_path("ffmpeg"))
}

/// Pojedyncze słowo warstwy tekstowej PDF (w punktach, origin u góry).
#[derive(Debug, Clone, PartialEq)]
pub struct PdfWord {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Tekst jednej strony PDF z prostokątami słów.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfText {
    pub width: f32,
    pub height: f32,
    pub words: Vec<PdfWord>,
}

/// Wyciąga warstwę tekstową strony (`pdftotext -bbox`).
///
/// Prostokąty są w punktach PDF; UI skaluje je do prostokąta wyrenderowanej
/// strony, dzięki czemu zaznaczanie działa też po zoomie.
pub fn pdf_text(path: &Path, page: usize) -> Option<PdfText> {
    if !in_path("pdftotext") {
        return None;
    }
    let number = page + 1;
    let output = Command::new("pdftotext")
        .args(["-bbox", "-f"])
        .arg(number.to_string())
        .arg("-l")
        .arg(number.to_string())
        .arg(path)
        .arg("-")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_pdf_text(&String::from_utf8_lossy(&output.stdout))
}

/// Parser XHTML z `pdftotext -bbox` (czysty, testowalny).
pub fn parse_pdf_text(html: &str) -> Option<PdfText> {
    let page_start = html.find("<page ")?;
    let page_end = html[page_start..].find('>')? + page_start;
    let tag = &html[page_start..page_end];
    let width = xml_attr(tag, "width")?.parse().ok()?;
    let height = xml_attr(tag, "height")?.parse().ok()?;
    let mut words = Vec::new();
    let mut rest = &html[page_end..];
    while let Some(start) = rest.find("<word ") {
        let after = &rest[start..];
        let Some(open_end) = after.find('>') else {
            break;
        };
        let tag = &after[..open_end];
        let Some(close) = after[open_end..].find("</word>") else {
            break;
        };
        let text = xml_unescape(&after[open_end + 1..open_end + close]);
        let (Some(x), Some(y), Some(x2), Some(y2)) = (
            xml_attr(tag, "xMin").and_then(|v| v.parse::<f32>().ok()),
            xml_attr(tag, "yMin").and_then(|v| v.parse::<f32>().ok()),
            xml_attr(tag, "xMax").and_then(|v| v.parse::<f32>().ok()),
            xml_attr(tag, "yMax").and_then(|v| v.parse::<f32>().ok()),
        ) else {
            rest = &after[open_end + close + 7..];
            continue;
        };
        if !text.is_empty() {
            words.push(PdfWord {
                text,
                x,
                y,
                width: (x2 - x).max(0.5),
                height: (y2 - y).max(0.5),
            });
        }
        rest = &after[open_end + close + 7..];
    }
    Some(PdfText {
        width,
        height,
        words,
    })
}

fn xml_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("{name}=\"");
    let start = tag.find(&marker)? + marker.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

fn xml_unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        let Some(end) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        let entity = &tail[1..end];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                if let Ok(code) = u32::from_str_radix(&entity[2..], 16) {
                    if let Some(ch) = char::from_u32(code) {
                        out.push(ch);
                    }
                }
            }
            _ if entity.starts_with('#') => {
                if let Ok(code) = entity[1..].parse::<u32>() {
                    if let Some(ch) = char::from_u32(code) {
                        out.push(ch);
                    }
                }
            }
            _ => out.push_str(&tail[..end + 1]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Wczytuje obraz i zmniejsza go tak, aby dłuższy bok nie przekraczał `max_side`.
///
/// `None` oznacza, że plik nie jest obrazem albo nie ma czym go zdekodować.
pub fn load(path: &Path, max_side: u32) -> Option<Arc<ImageData>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let max_side = max_side.max(32);

    let image = if crate::model::is_raw_ext(&ext) {
        decode_raw(path, max_side)
    } else {
        match ext.as_str() {
            "png" => std::fs::read(path)
                .ok()
                .and_then(|bytes| libluna_render::wallpaper::decode_png(&bytes))
                .or_else(|| ffmpeg_image(path, max_side)),
            "jpg" | "jpeg" => std::fs::read(path)
                .ok()
                .and_then(|bytes| decode_jpeg(&bytes, max_side))
                .or_else(|| ffmpeg_image(path, max_side)),
            "svg" => std::fs::read(path)
                .ok()
                .and_then(|bytes| decode_svg(&bytes, max_side)),
            _ => ffmpeg_image(path, max_side),
        }
    }?;
    Some(Arc::new(fit(image, max_side)))
}

/// Czy jest czym renderować PDF-y (`pdfinfo` + `pdftocairo`/`pdftoppm`).
pub fn have_pdf() -> bool {
    in_path("pdfinfo") && (in_path("pdftocairo") || in_path("pdftoppm"))
}

/// Liczba stron PDF (z `pdfinfo`).
pub fn pdf_page_count(path: &Path) -> Option<usize> {
    if !in_path("pdfinfo") {
        return None;
    }
    let output = Command::new("pdfinfo")
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_pdfinfo(&String::from_utf8_lossy(&output.stdout))
}

/// Parser `Pages: N` z wyjścia `pdfinfo`.
pub fn parse_pdfinfo(text: &str) -> Option<usize> {
    text.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if !key.trim().eq_ignore_ascii_case("pages") {
            return None;
        }
        value
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|pages| *pages > 0)
    })
}

/// Renderuje pojedynczą stronę PDF (indeks od 0) do obrazu.
///
/// `max_side` ogranicza dłuższy bok — poppler skaluje stronę, więc 2200 px
/// wystarcza do ostrego zoomu bez trzymania w pamięci ogromnego bufora.
pub fn pdf_page(path: &Path, page: usize, max_side: u32) -> Option<ImageData> {
    let number = page + 1;
    let max_side = max_side.max(64);
    let image = if in_path("pdftocairo") {
        let output = Command::new("pdftocairo")
            .args(["-png", "-singlefile", "-f"])
            .arg(number.to_string())
            .arg("-l")
            .arg(number.to_string())
            .args(["-scale-to", &max_side.to_string()])
            .arg(path)
            .arg("-")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        decode_png(&output.stdout, max_side)
    } else if in_path("pdftoppm") {
        // Starsze poppler nie pisze PNG do stdout — użyj pliku tymczasowego.
        let prefix =
            std::env::temp_dir().join(format!("luna-media-pdf-{}-{}", std::process::id(), number));
        let file = prefix.with_extension("png");
        let _ = std::fs::remove_file(&file);
        let status = Command::new("pdftoppm")
            .args(["-png", "-singlefile", "-f"])
            .arg(number.to_string())
            .arg("-l")
            .arg(number.to_string())
            .args(["-scale-to", &max_side.to_string()])
            .arg(path)
            .arg(&prefix)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok()?;
        let image = status
            .success()
            .then(|| std::fs::read(&file).ok())
            .flatten();
        let _ = std::fs::remove_file(&file);
        image.and_then(|bytes| decode_png(&bytes, max_side))
    } else {
        None
    }?;
    Some(flatten_on_white(image))
}

/// Scala obraz z białym tłem (strony PDF bywają półprzezroczyste).
fn flatten_on_white(image: ImageData) -> ImageData {
    let mut rgba = image.rgba;
    for px in rgba.chunks_exact_mut(4) {
        let alpha = px[3] as u16;
        if alpha < 255 {
            let add = 255u16 - alpha;
            px[0] = (px[0] as u16 + add).min(255) as u8;
            px[1] = (px[1] as u16 + add).min(255) as u8;
            px[2] = (px[2] as u16 + add).min(255) as u8;
            px[3] = 255;
        }
    }
    ImageData::new(image.width, image.height, rgba).with_opaque(true)
}

/// Dekoduje PNG z pamięci (z opcjonalnym zmniejszeniem).
pub fn decode_png(bytes: &[u8], max_side: u32) -> Option<ImageData> {
    let image = libluna_render::wallpaper::decode_png(bytes)?;
    Some(fit(image, max_side))
}

/// Zmniejsza obraz metodą najbliższego sąsiada (szybko, bez dodatkowych zależności).
pub fn fit(image: ImageData, max_side: u32) -> ImageData {
    let (w, h) = (image.width.max(1), image.height.max(1));
    if w <= max_side && h <= max_side {
        return image;
    }
    let scale = max_side as f64 / w.max(h) as f64;
    let nw = (w as f64 * scale).round().max(1.0) as u32;
    let nh = (h as f64 * scale).round().max(1.0) as u32;
    let mut out = vec![0u8; nw as usize * nh as usize * 4];
    for y in 0..nh {
        let sy = (y as u64 * h as u64 / nh as u64) as usize;
        for x in 0..nw {
            let sx = (x as u64 * w as u64 / nw as u64) as usize;
            let src = (sy * w as usize + sx) * 4;
            let dst = (y as usize * nw as usize + x as usize) * 4;
            out[dst..dst + 4].copy_from_slice(&image.rgba[src..src + 4]);
        }
    }
    ImageData::new(nw, nh, out)
}

/// Dekoduje JPEG do RGBA8 (bez orientacji z EXIF — ffmpeg robi to w fallbacku).
fn decode_jpeg(bytes: &[u8], max_side: u32) -> Option<ImageData> {
    use jpeg_decoder::PixelFormat;
    let mut decoder = jpeg_decoder::Decoder::new(bytes);
    decoder.read_info().ok()?;
    let original = decoder.info()?;
    // Skalowanie DCT przed dekodowaniem — duże zdjęcia nie zajmują dziesiątek MB.
    if original.width.max(original.height) as u32 > max_side {
        let scale = max_side as f32 / original.width.max(original.height) as f32;
        let _ = decoder.scale(
            (original.width as f32 * scale).max(1.0) as u16,
            (original.height as f32 * scale).max(1.0) as u16,
        );
    }
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    let (width, height) = (info.width as usize, info.height as usize);
    let mut rgba = Vec::with_capacity(width * height * 4);
    match info.pixel_format {
        PixelFormat::RGB24 => {
            for px in pixels.chunks_exact(3) {
                rgba.extend_from_slice(&[px[0], px[1], px[2], 0xff]);
            }
        }
        PixelFormat::L8 => {
            for value in pixels {
                rgba.extend_from_slice(&[value, value, value, 0xff]);
            }
        }
        PixelFormat::CMYK32 => {
            for px in pixels.chunks_exact(4) {
                let (c, m, y, k) = (px[0] as u16, px[1] as u16, px[2] as u16, px[3] as u16);
                let r = ((255 - c) * (255 - k) / 255) as u8;
                let g = ((255 - m) * (255 - k) / 255) as u8;
                let b = ((255 - y) * (255 - k) / 255) as u8;
                rgba.extend_from_slice(&[r, g, b, 0xff]);
            }
        }
        PixelFormat::L16 => return None,
    }
    Some(ImageData::new(width as u32, height as u32, rgba))
}

/// Rasteryzuje SVG, skalując go do `max_side` (z zachowaniem proporcji).
fn decode_svg(bytes: &[u8], max_side: u32) -> Option<ImageData> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    if size.width() <= 0.0 || size.height() <= 0.0 {
        return None;
    }
    let scale = max_side as f32 / size.width().max(size.height());
    let width = (size.width() * scale).round().max(1.0) as u32;
    let height = (size.height() * scale).round().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(ImageData::new(width, height, pixmap.data().to_vec()))
}

/// Dekoduje obraz przez ffmpeg (GIF, WebP, BMP, TIFF, AVIF, HEIC, …) do PNG.
fn ffmpeg_image(path: &Path, max_side: u32) -> Option<ImageData> {
    if !have_ffmpeg() {
        return None;
    }
    let filter = format!(
        "scale=min({max},iw):min({max},ih):force_original_aspect_ratio=decrease",
        max = max_side
    );
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args([
            "-frames:v",
            "1",
            "-vf",
            &filter,
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "-",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    decode_png(&output.stdout, max_side)
}

/// Pobiera pojedynczą klatkę filmu (albo animacji) do miniatury/podglądu.
///
/// `at` to sekunda, od której ffmpeg ma zacząć; przy bardzo krótkich klipach
/// warto podać 0.
pub fn video_frame(path: &Path, at: f64, max_side: u32) -> Option<ImageData> {
    if !have_ffmpeg() {
        return None;
    }
    let filter = format!(
        "scale={max}:{max}:force_original_aspect_ratio=decrease",
        max = max_side.max(32)
    );
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-ss"])
        .arg(format!("{:.3}", at.max(0.0)))
        .arg("-i")
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            &filter,
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "-",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    decode_png(&output.stdout, max_side)
}

/// RAW z aparatu: najpierw podgląd osadzony JPEG, a gdy jest za mały albo go
/// nie ma — pełna demozajka przez `dcraw_emu` (LibRaw).
fn decode_raw(path: &Path, max_side: u32) -> Option<ImageData> {
    let bytes = std::fs::read(path).ok();
    let preview = bytes.as_deref().and_then(best_embedded_preview);
    let preview_long = preview
        .as_ref()
        .map(|(_, w, h)| u32::from(*w.max(h)))
        .unwrap_or(0);

    // Miniatury zawsze korzystają z szybkiego podglądu. Pełny widok rozwija RAW,
    // jeśli osadzony JPEG jest zbyt mały, żeby wyglądać dobrze po powiększeniu.
    let needs_develop = can_develop_raw() && max_side > 512 && preview_long < 1600;
    if !needs_develop {
        if let Some((jpeg, _, _)) = preview {
            if let Some(image) = decode_jpeg(jpeg, max_side) {
                return Some(image);
            }
        }
    }

    if can_develop_raw() {
        let half = max_side <= 768;
        if let Some(image) = develop_raw(path, max_side, half) {
            return Some(image);
        }
    }

    // Ostatnia szansa: nawet mały podgląd jest lepszy niż brak obrazu.
    let (jpeg, _, _) = preview?;
    decode_jpeg(jpeg, max_side)
}

/// Czy jest czym wywołać pełną demozajkę RAW-a.
pub fn can_develop_raw() -> bool {
    in_path("dcraw_emu")
}

/// Pełna demozajka RAW-a (`dcraw_emu` z LibRaw) — np. z menu „Rozwiń RAW”.
///
/// Wynik jest zawsze w sRGB, z balansem bieli aparatu; `max_side` ogranicza
/// wynik po zdekodowaniu. Dla miniaturek można poprosić o połowę rozdzielczości.
pub fn develop_raw(path: &Path, max_side: u32, half: bool) -> Option<ImageData> {
    if !can_develop_raw() {
        return None;
    }
    let mut command = Command::new("dcraw_emu");
    command.args([
        "-q",
        if half { "1" } else { "3" },
        "-w",
        "-o",
        "1",
        "-Z",
        "-",
    ]);
    if half {
        command.arg("-h");
    }
    let output = command
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    decode_ppm(&output.stdout, max_side)
}

/// Największy poprawny JPEG z danych RAW (kamera trzyma tam podgląd).
/// Zwraca `(jpeg, szerokość, wysokość)`.
fn best_embedded_preview(bytes: &[u8]) -> Option<(&[u8], u16, u16)> {
    let mut best: Option<(&[u8], u16, u16, u64)> = None;
    let mut i = 0usize;
    while i + 3 < bytes.len() {
        if bytes[i] != 0xFF || bytes[i + 1] != 0xD8 || bytes[i + 2] != 0xFF {
            i += 1;
            continue;
        }
        let Some(end) = find_eoi(bytes, i + 3) else {
            i += 2;
            continue;
        };
        let candidate = &bytes[i..end];
        if candidate.len() >= 4096 {
            if let Some((w, h)) = jpeg_dimensions(candidate) {
                let area = w as u64 * h as u64;
                if best.map_or(true, |(_, _, _, best_area)| area > best_area) {
                    best = Some((candidate, w, h, area));
                }
            }
        }
        i = end;
    }
    best.map(|(jpeg, w, h, _)| (jpeg, w, h))
}

fn find_eoi(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < bytes.len() {
        if bytes[i] == 0xFF && bytes[i + 1] == 0xD9 {
            return Some(i + 2);
        }
        i += 1;
    }
    None
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u16, u16)> {
    let mut decoder = jpeg_decoder::Decoder::new(bytes);
    decoder.read_info().ok()?;
    let info = decoder.info()?;
    Some((info.width, info.height))
}

/// Minimalny dekoder PPM (P5/P6) — wyjście awaryjne `dcraw_emu`.
fn decode_ppm(bytes: &[u8], max_side: u32) -> Option<ImageData> {
    let mut tokens = Vec::new();
    let mut i = 0usize;
    while tokens.len() < 4 && i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if start == i {
            break;
        }
        tokens.push(&bytes[start..i]);
    }
    if tokens.len() < 4 {
        return None;
    }
    let magic = tokens[0];
    if magic != b"P5" && magic != b"P6" {
        return None;
    }
    let width: u32 = std::str::from_utf8(tokens[1]).ok()?.parse().ok()?;
    let height: u32 = std::str::from_utf8(tokens[2]).ok()?.parse().ok()?;
    let maxval: u32 = std::str::from_utf8(tokens[3]).ok()?.parse().ok()?;
    if width == 0 || height == 0 || maxval == 0 || maxval > 255 {
        return None;
    }
    // Po nagłówku jest dokładnie jeden biały znak.
    if i < bytes.len() {
        i += 1;
    }
    let channels = if magic == b"P6" { 3 } else { 1 };
    let expected = width as usize * height as usize * channels;
    if bytes.len() < i + expected {
        return None;
    }
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for pixel in bytes[i..i + expected].chunks_exact(channels) {
        if channels == 3 {
            rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 0xff]);
        } else {
            rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], 0xff]);
        }
    }
    Some(fit(
        ImageData::new(width, height, rgba).with_opaque(true),
        max_side,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_aspect_ratio() {
        let image = ImageData::new(400, 200, vec![255; 400 * 200 * 4]);
        let small = fit(image, 100);
        assert_eq!((small.width, small.height), (100, 50));
    }

    #[test]
    fn fit_leaves_small_images_untouched() {
        let image = ImageData::new(20, 10, vec![0; 20 * 10 * 4]);
        let same = fit(image, 100);
        assert_eq!((same.width, same.height), (20, 10));
    }

    #[test]
    fn decodes_a_small_jpeg() {
        // 2×1, czerwony — minimalny poprawny JPEG.
        const JPEG: &[u8] = &[
            255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0, 255, 219, 0,
            67, 0, 3, 2, 2, 2, 2, 2, 3, 2, 2, 2, 3, 3, 3, 3, 4, 6, 4, 4, 4, 4, 4, 8, 6, 6, 5, 6, 9,
            8, 10, 10, 9, 8, 9, 9, 10, 12, 15, 12, 10, 11, 14, 11, 9, 9, 13, 17, 13, 14, 15, 16,
            16, 17, 16, 10, 12, 18, 19, 18, 16, 19, 15, 16, 16, 16, 255, 219, 0, 67, 1, 3, 3, 3, 4,
            3, 4, 8, 4, 4, 8, 16, 11, 9, 11, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16,
            16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16,
            16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 255, 192, 0, 17, 8, 0,
            1, 0, 2, 3, 1, 17, 0, 2, 17, 1, 3, 17, 1, 255, 196, 0, 20, 0, 1, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 8, 255, 196, 0, 20, 16, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 255, 196, 0, 21, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7, 9,
            255, 196, 0, 20, 17, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 218, 0,
            12, 3, 1, 0, 2, 17, 3, 17, 0, 63, 0, 58, 3, 21, 77, 255, 217,
        ];
        let image = decode_jpeg(JPEG, 4096).expect("JPEG powinien się zdekodować");
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.rgba.len(), 2 * 4);
        assert_eq!(image.rgba[3], 255);
    }

    #[test]
    fn parses_pdf_text_layer() {
        let html = r#"<html><body><doc>
  <page width="600.0" height="800.0">
    <word xMin="10" yMin="20" xMax="60" yMax="32">Hello</word>
    <word xMin="70" yMin="20" xMax="120" yMax="32">world &amp; co</word>
  </page>
</doc></body></html>"#;
        let text = parse_pdf_text(html).expect("warstwa tekstowa");
        assert_eq!((text.width, text.height), (600.0, 800.0));
        assert_eq!(text.words.len(), 2);
        assert_eq!(text.words[0].text, "Hello");
        assert_eq!(text.words[1].text, "world & co");
        assert_eq!(text.words[1].x, 70.0);
        assert_eq!(text.words[1].width, 50.0);
    }

    #[test]
    fn parses_pdfinfo_pages() {
        let text = "Title: Test\nPages:           15\nPage size:       595 x 842 pts\n";
        assert_eq!(parse_pdfinfo(text), Some(15));
        assert_eq!(parse_pdfinfo("Pages: N/A\n"), None);
        assert_eq!(parse_pdfinfo("Pages: 0\n"), None);
    }

    #[test]
    fn decodes_a_small_ppm() {
        // 2×1: czerwony + zielony piksel.
        let ppm = b"P6\n2 1\n255\n\xff\x00\x00\x00\xff\x00";
        let image = decode_ppm(ppm, 64).expect("PPM powinien się zdekodować");
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(&image.rgba[0..4], &[0xff, 0, 0, 0xff]);
        assert_eq!(&image.rgba[4..8], &[0, 0xff, 0, 0xff]);
    }

    #[test]
    fn flattens_transparency_on_white() {
        let image = ImageData::new(1, 1, vec![0, 0, 0, 0]);
        let flat = flatten_on_white(image);
        assert_eq!(&flat.rgba, &[255, 255, 255, 255]);
        assert!(flat.opaque);
    }

    #[test]
    #[ignore = "wymaga ffmpeg — tworzy atrapę RAW z osadzonym JPEG"]
    fn reads_raw_embedded_jpeg() {
        if !in_path("ffmpeg") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("luna-media-raw-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let jpeg = dir.join("preview.jpg");
        let status = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=1",
                "-frames:v",
                "1",
            ])
            .arg(&jpeg)
            .status();
        assert!(status.map(|s| s.success()).unwrap_or(false));
        let preview = std::fs::read(&jpeg).unwrap();
        let mut raw = vec![0x33u8; 4096];
        raw.extend_from_slice(&preview);
        raw.extend_from_slice(&[0x44u8; 2048]);
        let raw_path = dir.join("foto.cr3");
        std::fs::write(&raw_path, raw).unwrap();
        let image = load(&raw_path, 512).expect("RAW z osadzonym JPEG");
        assert_eq!((image.width, image.height), (320, 240));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    #[ignore = "wymaga poppler + ghostscript — renderuje wygenerowany PDF"]
    fn renders_pdf_pages() {
        if !have_pdf() || !in_path("gs") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("luna-media-pdf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.pdf");
        let status = Command::new("gs")
            .args([
                "-q",
                "-dBATCH",
                "-dNOPAUSE",
                "-sDEVICE=pdfwrite",
            ])
            .arg(format!("-sOutputFile={}", path.display()))
            .args([
                "-c",
                "<</PageSize [200 200]>> setpagedevice 1 0 0 setrgbcolor 0 0 200 200 rectfill showpage 0 1 0 setrgbcolor 0 0 100 100 rectfill showpage",
            ])
            .status();
        assert!(status.map(|s| s.success()).unwrap_or(false));
        assert_eq!(pdf_page_count(&path), Some(2));
        let page = pdf_page(&path, 1, 256).expect("druga strona powinna się wyrenderować");
        assert!(page.width > 0 && page.height > 0);
        assert!(page.opaque);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    #[ignore = "wymaga dcraw_emu i próbki RAW w LUNA_MEDIA_RAW_SAMPLE"]
    fn develops_a_real_raw_sample() {
        let Some(path) = std::env::var_os("LUNA_MEDIA_RAW_SAMPLE").map(std::path::PathBuf::from)
        else {
            return;
        };
        if !can_develop_raw() {
            return;
        }
        let image = develop_raw(&path, 1024, false).expect("pełna demozajka RAW");
        assert!(image.width > 0 && image.height > 0);
        assert!(image.width.max(image.height) <= 1024);
        // Automatyczna ścieżka powinna też dać obraz (podgląd albo demozajka).
        let auto = load(&path, 1024).expect("load() dla RAW");
        assert!(auto.width > 0 && auto.height > 0);
    }

    #[test]
    fn renders_svg_at_requested_size() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="5"><rect width="10" height="5" fill="#ff0000"/></svg>"##;
        let image = decode_svg(svg, 64).expect("SVG powinien się wyrenderować");
        assert_eq!((image.width, image.height), (64, 32));
        assert_eq!(image.rgba.len(), 64 * 32 * 4);
    }
}
