//! Tapeta pulpitu: dekodowanie PNG i skalowanie „cover".
//!
//! Trzymamy to w `libluna-render`, bo tylko ta warstwa zna `tiny-skia` (a więc i PNG).
//! Kompozytor wczytuje grafikę raz i trzyma już przeskalowaną wersję, żeby co klatkę
//! szedł tani blit 1:1, a nie skalowanie w pętli renderowania.

use tiny_skia::{BlendMode, FilterQuality, Pixmap, PixmapPaint, PixmapRef, Transform};

use crate::scene::ImageData;

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
}
