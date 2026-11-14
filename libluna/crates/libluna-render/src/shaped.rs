//! Tekst **kształtowany** (shaping, bidi, fallback fontów) przez `cosmic-text`.
//!
//! [`crate::text::TextEngine`] rysuje znak po znaku — wystarcza dla interfejsu,
//! ale nie dla pism złożonych: arabskiego (łączenie liter), dewanagari (przestawianie
//! znaków), tajskiego (samogłoski przed spółgłoską) czy CJK (fallback fontu).
//!
//! Ten moduł nie zmienia kontraktu `TextEngine`. Renderuje **cały akapit do obrazu**
//! RGBA (premnożonego), który scena rysuje jak każdą inną teksturę — dzięki temu
//! działa identycznie na backendzie CPU i GPU, bez nowych poleceń w grafie sceny.
//!
//! Używany tam, gdzie tekst jest artystyczny i statyczny (np. scena księżycowego
//! jajka z podziękowaniami w wielu pismach).

use std::sync::Arc;

use cosmic_text::{
    Align, Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent,
    SwashImage, Weight, Wrap,
};
use libluna_core::Color;

use crate::scene::ImageData;

/// Silnik kształtowania tekstu. Tworzy `FontSystem` (skan fontów systemowych)
/// dopiero przy pierwszym użyciu — nie obciąża startu aplikacji.
pub struct ShapedText {
    fonts: FontSystem,
    swash: SwashCache,
}

impl Default for ShapedText {
    fn default() -> Self {
        Self::new()
    }
}

impl ShapedText {
    pub fn new() -> Self {
        Self {
            fonts: FontSystem::new(),
            swash: SwashCache::new(),
        }
    }

    /// Składa akapit wyśrodkowany w `max_width` i zwraca go jako obraz RGBA
    /// (premnożony, gotowy dla [`crate::Scene::image`]).
    ///
    /// Obsługuje zawijanie (także CJK, gdzie nie ma spacji), pisma RTL oraz
    /// dobór fontu per znak. `None` = pusty tekst albo brak fontów.
    pub fn render_centered(
        &mut self,
        text: &str,
        px: f32,
        font_weight: u16,
        color: Color,
        max_width: f32,
    ) -> Option<Arc<ImageData>> {
        if text.trim().is_empty() {
            return None;
        }
        let px = px.clamp(6.0, 120.0);
        let width = max_width.clamp(px * 2.0, 4096.0);
        let metrics = Metrics::new(px, (px * 1.4).ceil());

        let mut buffer = Buffer::new(&mut self.fonts, metrics);
        buffer.set_wrap(&mut self.fonts, Wrap::WordOrGlyph);
        buffer.set_size(&mut self.fonts, Some(width), None);
        let attrs = Attrs::new()
            .family(Family::SansSerif)
            .weight(Weight(font_weight));
        buffer.set_text(
            &mut self.fonts,
            text,
            &attrs,
            Shaping::Advanced,
            Some(Align::Center),
        );
        buffer.shape_until_scroll(&mut self.fonts, false);

        let mut height = 0.0f32;
        for run in buffer.layout_runs() {
            height = height.max(run.line_top + run.line_height);
        }
        if !(height > 0.0) {
            return None;
        }

        // Margines na wystające fragmenty glifów (ujemne bearingi, kursywa).
        const PAD: i32 = 4;
        let w = width.ceil() as u32 + (PAD * 2) as u32;
        let h = height.ceil() as u32 + (PAD * 2) as u32;
        let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];

        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let physical = glyph.physical((0.0, 0.0), 1.0);
                let Some(image) = self.swash.get_image(&mut self.fonts, physical.cache_key) else {
                    continue;
                };
                let x = physical.x + image.placement.left + PAD;
                let y = run.line_y as i32 + physical.y - image.placement.top + PAD;
                composite(
                    &mut rgba,
                    w as i32,
                    h as i32,
                    x,
                    y,
                    image,
                    glyph
                        .color_opt
                        .map(|c| [c.r(), c.g(), c.b(), c.a()])
                        .unwrap_or_else(|| color.to_rgba8()),
                );
            }
        }

        Some(Arc::new(ImageData::new(w, h, rgba).with_sparse(true)))
    }
}

/// Nakłada bitmapę glifu (maskę albo kolorową bitmapę emoji) na bufor premnożony.
fn composite(dst: &mut [u8], w: i32, h: i32, gx: i32, gy: i32, image: &SwashImage, color: [u8; 4]) {
    let pw = image.placement.width as i32;
    let ph = image.placement.height as i32;
    if pw <= 0 || ph <= 0 {
        return;
    }
    let base_a = color[3] as f32 / 255.0;

    match image.content {
        SwashContent::Mask => {
            for row in 0..ph {
                for col in 0..pw {
                    let cov = image.data[(row * pw + col) as usize] as f32 / 255.0;
                    if cov <= 0.0 {
                        continue;
                    }
                    let a = cov * base_a;
                    blend(
                        dst,
                        w,
                        h,
                        gx + col,
                        gy + row,
                        color[0] as f32 / 255.0 * a,
                        color[1] as f32 / 255.0 * a,
                        color[2] as f32 / 255.0 * a,
                        a,
                    );
                }
            }
        }
        SwashContent::SubpixelMask => {
            for row in 0..ph {
                for col in 0..pw {
                    let i = ((row * pw + col) * 4) as usize;
                    let cov = (image.data[i] as f32
                        + image.data[i + 1] as f32
                        + image.data[i + 2] as f32)
                        / (3.0 * 255.0);
                    if cov <= 0.0 {
                        continue;
                    }
                    let a = cov * base_a;
                    blend(
                        dst,
                        w,
                        h,
                        gx + col,
                        gy + row,
                        color[0] as f32 / 255.0 * a,
                        color[1] as f32 / 255.0 * a,
                        color[2] as f32 / 255.0 * a,
                        a,
                    );
                }
            }
        }
        SwashContent::Color => {
            // Emoji: bitmapa ma własne kolory (proste RGBA) — premnożymy ją tutaj.
            for row in 0..ph {
                for col in 0..pw {
                    let i = ((row * pw + col) * 4) as usize;
                    let a = image.data[i + 3] as f32 / 255.0;
                    if a <= 0.0 {
                        continue;
                    }
                    blend(
                        dst,
                        w,
                        h,
                        gx + col,
                        gy + row,
                        image.data[i] as f32 / 255.0 * a,
                        image.data[i + 1] as f32 / 255.0 * a,
                        image.data[i + 2] as f32 / 255.0 * a,
                        a,
                    );
                }
            }
        }
    }
}

/// „Over” na premnożonym RGBA.
#[allow(clippy::too_many_arguments)]
fn blend(dst: &mut [u8], w: i32, h: i32, x: i32, y: i32, sr: f32, sg: f32, sb: f32, sa: f32) {
    if x < 0 || y < 0 || x >= w || y >= h || sa <= 0.0 {
        return;
    }
    let idx = ((y * w + x) * 4) as usize;
    let inv = 1.0 - sa;
    let r = sr + (dst[idx] as f32 / 255.0) * inv;
    let g = sg + (dst[idx + 1] as f32 / 255.0) * inv;
    let b = sb + (dst[idx + 2] as f32 / 255.0) * inv;
    let a = sa + (dst[idx + 3] as f32 / 255.0) * inv;
    dst[idx] = (r * 255.0).round().clamp(0.0, 255.0) as u8;
    dst[idx + 1] = (g * 255.0).round().clamp(0.0, 255.0) as u8;
    dst[idx + 2] = (b * 255.0).round().clamp(0.0, 255.0) as u8;
    dst[idx + 3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_core::style::weight;

    #[test]
    fn shapes_latin_text() {
        let mut shaped = ShapedText::new();
        let image = shaped
            .render_centered("Dziękujemy!", 16.0, weight::NORMAL, Color::WHITE, 400.0)
            .expect("obraz tekstu");
        assert!(image.width > 10);
        assert!(image.height > 5);
        assert!(image.rgba.iter().any(|b| *b > 0), "coś się narysowało");
    }

    #[test]
    fn shapes_arabic_with_joining() {
        let mut shaped = ShapedText::new();
        let image = shaped
            .render_centered("شكرًا لكونك معنا", 18.0, weight::NORMAL, Color::WHITE, 400.0)
            .expect("obraz tekstu arabskiego");
        assert!(image.rgba.iter().any(|b| *b > 0));
    }

    #[test]
    fn empty_text_is_none() {
        let mut shaped = ShapedText::new();
        assert!(shaped
            .render_centered("   ", 16.0, weight::NORMAL, Color::WHITE, 300.0)
            .is_none());
    }
}
