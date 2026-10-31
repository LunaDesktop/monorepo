//! Tapeta pulpitu: dekodowanie PNG i skalowanie „cover".
//!
//! Trzymamy to w `libluna-render`, bo tylko ta warstwa zna `tiny-skia` (a więc i PNG).
//! Kompozytor wczytuje grafikę raz i trzyma już przeskalowaną wersję, żeby co klatkę
//! szedł tani blit 1:1, a nie skalowanie w pętli renderowania.

use tiny_skia::{BlendMode, FilterQuality, Pixmap, PixmapPaint, PixmapRef, Transform};

use std::path::{Path, PathBuf};

use crate::scene::ImageData;

/// Plik konfiguracji tapety: `$XDG_CONFIG_HOME/luna/wallpaper.conf`.
pub fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("luna").join("wallpaper.conf"))
}

/// Wyciąga `path=...` z zawartości pliku konfiguracji tapety.
pub fn parse_config(text: &str) -> Option<PathBuf> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            if key.trim() == "path" {
                let value = value.trim().trim_matches('"');
                if !value.is_empty() {
                    return Some(PathBuf::from(value));
                }
            }
        }
    }
    None
}

/// Zapisuje wybraną tapetę pulpitu (PNG) do `wallpaper.conf`.
pub fn write_config(path: &Path) -> std::io::Result<()> {
    let Some(config) = config_path() else {
        return Err(std::io::Error::other("brak katalogu konfiguracji"));
    };
    if let Some(dir) = config.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(config, format_config(path))
}

/// Usuwa wybór tapety — kompozytor wróci do wbudowanej grafiki/gradientu.
pub fn clear_config() -> std::io::Result<()> {
    let Some(config) = config_path() else {
        return Ok(());
    };
    match std::fs::remove_file(config) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// Zawartość `wallpaper.conf` (osobno, żeby dało się testować bez systemu plików).
pub fn format_config(path: &Path) -> String {
    format!(
        "# Tapeta pulpitu Luny — zapisuje centrum ustawień\npath={}\n",
        path.display()
    )
}

/// Tapeta ustawiona przez użytkownika.
///
/// Kolejność: `LUNA_WALLPAPER`, `SELENE_WALLPAPER`, potem
/// `$XDG_CONFIG_HOME/luna/wallpaper.conf` (`path=/ścieżka/do.png`).
/// `None` znaczy „brak tapety” — wywołujący decyduje o zapasie (gradient albo
/// wbudowana grafika).
pub fn configured_path() -> Option<PathBuf> {
    for var in ["LUNA_WALLPAPER", "SELENE_WALLPAPER"] {
        if let Some(value) = std::env::var_os(var).filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(value));
        }
    }
    let text = std::fs::read_to_string(config_path()?).ok()?;
    parse_config(&text)
}

/// Wczytuje i dekoduje tapetę (PNG) z podanej ścieżki.
pub fn load_path(path: &Path) -> Option<ImageData> {
    let bytes = std::fs::read(path).ok()?;
    decode_png(&bytes)
}

/// Dekoduje PNG do [`ImageData`] (premultiplied RGBA8 — spójnie z resztą obrazów sceny).
///
/// `None`, gdy bajty nie są poprawnym PNG albo brakuje pamięci.
pub fn decode_png(bytes: &[u8]) -> Option<ImageData> {
    let pixmap = Pixmap::decode_png(bytes).ok()?;
    Some(ImageData::new(
        pixmap.width(),
        pixmap.height(),
        pixmap.data().to_vec(),
    ))
}

/// Skaluje obraz tak, by **pokrył** `w`×`h`: zachowuje proporcje, przycina nadmiar
/// i centruje. Wynik ma dokładnie `w`×`h`, więc nadaje się do blitu 1:1.
pub fn cover(src: &ImageData, w: u32, h: u32) -> ImageData {
    let (w, h) = (w.max(1), h.max(1));
    let Some(sp) = PixmapRef::from_bytes(&src.rgba, src.width.max(1), src.height.max(1)) else {
        return ImageData::empty(w, h);
    };
    let Some(mut dst) = Pixmap::new(w, h) else {
        return ImageData::empty(w, h);
    };
    // Współczynnik „cover”: większy z dwóch, żeby nic nie zostało puste.
    let scale = (w as f32 / sp.width() as f32).max(h as f32 / sp.height() as f32);
    let dw = sp.width() as f32 * scale;
    let dh = sp.height() as f32 * scale;
    let tx = (w as f32 - dw) / 2.0;
    let ty = (h as f32 - dh) / 2.0;
    let paint = PixmapPaint {
        blend_mode: BlendMode::Source,
        quality: FilterQuality::Bilinear,
        opacity: 1.0,
    };
    let transform = Transform::from_scale(scale, scale).post_translate(tx, ty);
    dst.draw_pixmap(0, 0, sp, &paint, transform, None);
    // Tło pulpitu musi być nieprzezroczyste — renderer kopiuje je wprost, a przezroczyste
    // piksele dałyby czarne dziury.
    for px in dst.data_mut().chunks_exact_mut(4) {
        px[3] = 0xff;
    }
    ImageData::new(w, h, dst.data().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_fills_requested_size() {
        let src = ImageData::new(4, 2, vec![128; 4 * 2 * 4]);
        let out = cover(&src, 10, 10);
        assert_eq!((out.width, out.height), (10, 10));
        assert_eq!(out.rgba.len(), 10 * 10 * 4);
    }

    #[test]
    fn cover_keeps_aspect_by_cropping() {
        // Szeroki obraz (4×2) na kwadrat: skala wg szerszego boku, więc wychodzi kwadrat.
        let src = ImageData::new(4, 2, vec![0; 4 * 2 * 4]);
        let out = cover(&src, 8, 8);
        assert_eq!((out.width, out.height), (8, 8));
    }

    #[test]
    fn decode_png_roundtrip() {
        // Zakoduj prosty obraz i sprawdź, że dekodowanie zwraca ten sam rozmiar.
        let mut pm = Pixmap::new(3, 5).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(10, 20, 30, 255));
        let png = pm.encode_png().unwrap();
        let img = decode_png(&png).unwrap();
        assert_eq!((img.width, img.height), (3, 5));
        // Niepoprawne dane → `None` (nie panikujemy).
        assert!(decode_png(b"to nie jest png").is_none());
    }

    #[test]
    fn wallpaper_config_reads_path() {
        assert_eq!(
            parse_config("# tapeta\npath=/tmp/tlo.png\n"),
            Some(PathBuf::from("/tmp/tlo.png"))
        );
        assert_eq!(
            parse_config("  path = \"/tmp/z cudzysłowem.png\"  "),
            Some(PathBuf::from("/tmp/z cudzysłowem.png"))
        );
        assert_eq!(parse_config("path=\n"), None);
        assert_eq!(parse_config("inny=1\n"), None);
    }

    #[test]
    fn formatted_config_round_trips() {
        let path = Path::new("/usr/share/wallpapers/pagan.png");
        let text = format_config(path);
        assert_eq!(parse_config(&text), Some(path.to_path_buf()));
    }
}
