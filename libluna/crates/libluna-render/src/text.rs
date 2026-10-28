//! Minimalny silnik tekstu oparty na `fontdue` (bez harfbuzz/pango).
//!
//! Obsługuje pojedyncze pismo bez kerningu i bez kształtowania złożonego — to
//! świadome uproszczenie M0. Docelowo miejsce to zajmie `cosmic-text` (shaping,
//! fallback fontów, pisma złożone), ale kontrakt (`measure`/`draw`) zostaje ten sam.
//!
//! Ładuje font systemowy (najpierw Adwaita Sans, potem DejaVu), a jeśli żaden nie
//! jest dostępny, działa w trybie „bez fontu” (przybliżone metryki, brak rysowania).

use libluna_core::style::weight;
use libluna_core::{Color, Size};

use fontdue::{Font, FontSettings};

/// Ścieżki fontu podstawowego, w kolejności preferencji (Adwaita Sans → DejaVu).
const REGULAR_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf",
    "/usr/share/fonts/adwaita/AdwaitaSans-Regular.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
];

/// Ścieżki fontu pogrubionego.
const BOLD_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/Adwaita/AdwaitaSans-Bold.ttf",
    "/usr/share/fonts/adwaita/AdwaitaSans-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
];

/// Ścieżki fontu o **stałej szerokości** (terminal, edytory kodu).
const MONO_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/Adwaita/AdwaitaMono-Regular.ttf",
    "/usr/share/fonts/adwaita/AdwaitaMono-Regular.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
];

/// Ścieżki pogrubionego fontu o stałej szerokości.
const MONO_BOLD_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/Adwaita/AdwaitaMono-Bold.ttf",
    "/usr/share/fonts/adwaita/AdwaitaMono-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono-Bold.ttf",
    "/usr/share/fonts/noto/NotoSansMono-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
];

/// Pojedynczy glif gotowy do wgrania do atlasu.
pub struct GlyphRaster {
    pub width: usize,
    pub height: usize,
    /// Krycie glifu, `width * height` bajtów (0 = przezroczysty).
    pub coverage: Vec<u8>,
    /// Pozycja bitmapy względem punktu wstawienia (px).
    pub xmin: i32,
    pub ymin: i32,
    /// Szerokość przesunięcia pióra (px), liczona z **dokładnego** rozmiaru.
    pub advance: f32,
}

/// Silnik tekstu: trzyma fonty i potrafi mierzyć oraz rysować napisy.
pub struct TextEngine {
    regular: Option<Font>,
    bold: Option<Font>,
}

impl TextEngine {
    /// Tworzy silnik z fontów systemowych (może działać bez nich).
    pub fn system() -> Self {
        let regular = load_first(REGULAR_CANDIDATES).or_else(|| load_from_env("LUNA_FONT"));
        let bold = load_first(BOLD_CANDIDATES).or_else(|| load_from_env("LUNA_FONT_BOLD"));
        Self { regular, bold }
    }

    /// Silnik z fontem o **stałej szerokości** (terminal, edytor kodu).
    ///
    /// `LUNA_FONT_MONO` / `LUNA_FONT_MONO_BOLD` pozwalają wskazać własne pliki.
    pub fn mono() -> Self {
        let regular = load_first(MONO_CANDIDATES).or_else(|| load_from_env("LUNA_FONT_MONO"));
        let bold =
            load_first(MONO_BOLD_CANDIDATES).or_else(|| load_from_env("LUNA_FONT_MONO_BOLD"));
        Self { regular, bold }
    }

    /// Silnik bez fontów (przybliżone metryki, brak rysowania) — do pomiaru drzew układu,
    /// które nie zawierają etykiet. Tani w tworzeniu.
    pub fn none() -> Self {
        Self {
            regular: None,
            bold: None,
        }
    }

    /// Czy jakikolwiek font został wczytany.
    pub fn has_font(&self) -> bool {
        self.regular.is_some()
    }

    fn font_for(&self, weight: u16) -> Option<&Font> {
        if weight >= weight::BOLD {
            self.bold.as_ref().or(self.regular.as_ref())
        } else {
            self.regular.as_ref()
        }
    }

    /// Szerokość i wysokość tekstu w pikselach logicznych (bez kerningu).
    pub fn measure(&self, text: &str, size: f64, weight: u16) -> Size {
        let Some(font) = self.font_for(weight) else {
            // Tryb bez fontu: przybliżenie, żeby układ miał sens.
            return Size::new(text.chars().count() as f64 * size * 0.5, size * 1.2);
        };

        let px = size as f32;
        let mut advance = 0.0f32;
        let mut max_ascent = 0.0f32;
        let mut max_descent = 0.0f32;
        for ch in text.chars() {
            let m = font.metrics(ch, px);
            advance += m.advance_width;
            max_ascent = max_ascent.max(m.height as f32 + m.ymin as f32);
            max_descent = max_descent.max(-(m.ymin as f32));
        }
        Size::new(advance as f64, (max_ascent + max_descent) as f64)
    }

    /// Rasteryzuje jeden glif na potrzeby atlasu GPU.
    ///
    /// Bitmapa jest liczona w rozmiarze zaokrąglonym do 0,5 px (mniej wariantów
    /// w atlasie), ale `advance` wraca z **dokładnego** `px` — inaczej długie
    /// napisy rozjeżdżałyby się względem `measure`.
    pub fn glyph(&self, ch: char, px: f32, weight: u16) -> Option<GlyphRaster> {
        let font = self.font_for(weight)?;
        let exact = px.max(1.0);
        let quantized = ((exact * 2.0).round() / 2.0).max(1.0);
        let (metrics, coverage) = font.rasterize(ch, quantized);
        let advance = font.metrics(ch, exact).advance_width;
        Some(GlyphRaster {
            width: metrics.width,
            height: metrics.height,
            coverage,
            xmin: metrics.xmin,
            ymin: metrics.ymin,
            advance,
        })
    }

    /// Rysuje tekst do bufora RGBA8888 **premnożonego** (format tiny-skia).
    ///
    /// `origin` w pikselach fizycznych, `size` skalowana przez `scale`.
    /// `clip` ogranicza rysowanie do prostokąta (piksele fizyczne) — używane
    /// przy aktywnym `Scene::push_clip`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        buf: &mut [u8],
        buf_w: usize,
        buf_h: usize,
        origin_x: f64,
        origin_y: f64,
        text: &str,
        size: f64,
        weight: u16,
        color: Color,
        scale: f64,
        clip: Option<ClipRect>,
    ) {
        let Some(font) = self.font_for(weight) else {
            return;
        };

        let px = (size * scale) as f32;
        let (r, g, b) = (color.r * color.a, color.g * color.a, color.b * color.a);
        let alpha = color.a;

        let mut pen_x = origin_x;
        let baseline_y = origin_y;
        for ch in text.chars() {
            let (metrics, coverage) = font.rasterize(ch, px);
            if metrics.width > 0 && metrics.height > 0 {
                let gx = (pen_x + metrics.xmin as f64).round() as i64;
                let gy =
                    (baseline_y - (metrics.height as f64 + metrics.ymin as f64)).round() as i64;
                blend_glyph(
                    buf,
                    buf_w,
                    buf_h,
                    gx,
                    gy,
                    metrics.width,
                    metrics.height,
                    &coverage,
                    r,
                    g,
                    b,
                    alpha,
                    clip,
                );
            }
            pen_x += metrics.advance_width as f64;
        }
    }
}

fn load_first(paths: &[&str]) -> Option<Font> {
    paths.iter().find_map(|p| load_font(p))
}

fn load_from_env(var: &str) -> Option<Font> {
    std::env::var(var).ok().and_then(|p| load_font(&p))
}

fn load_font(path: &str) -> Option<Font> {
    let bytes = std::fs::read(path).ok()?;
    Font::from_bytes(bytes, FontSettings::default()).ok()
}

/// Prostokąt obcinania w pikselach fizycznych (półotwarty: `x0..x1`, `y0..y1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipRect {
    pub x0: i64,
    pub y0: i64,
    pub x1: i64,
    pub y1: i64,
}

/// Nakłada bitmapę krycia glifu na bufor premnożony (operacja „over”).
#[allow(clippy::too_many_arguments)]
fn blend_glyph(
    buf: &mut [u8],
    buf_w: usize,
    buf_h: usize,
    gx: i64,
    gy: i64,
    gw: usize,
    gh: usize,
    coverage: &[u8],
    sr: f32,
    sg: f32,
    sb: f32,
    sa: f32,
    clip: Option<ClipRect>,
) {
    for row in 0..gh {
        let y = gy + row as i64;
        if y < 0 || y >= buf_h as i64 {
            continue;
        }
        if let Some(c) = clip {
            if y < c.y0 || y >= c.y1 {
                continue;
            }
        }
        for col in 0..gw {
            let x = gx + col as i64;
            if x < 0 || x >= buf_w as i64 {
                continue;
            }
            if let Some(c) = clip {
                if x < c.x0 || x >= c.x1 {
                    continue;
                }
            }
            let cov = coverage[row * gw + col] as f32 / 255.0;
            if cov <= 0.0 {
                continue;
            }
            // Źródło premnożone, przemnożone jeszcze przez krycie glifu.
            let a_s = sa * cov;
            let r_s = sr * cov;
            let g_s = sg * cov;
            let b_s = sb * cov;

            let idx = (y as usize * buf_w + x as usize) * 4;
            let inv = 1.0 - a_s;
            let a_d = buf[idx + 3] as f32 / 255.0;
            let r_d = buf[idx] as f32 / 255.0;
            let g_d = buf[idx + 1] as f32 / 255.0;
            let b_d = buf[idx + 2] as f32 / 255.0;

            let out_a = a_s + a_d * inv;
            let out_r = r_s + r_d * inv;
            let out_g = g_s + g_d * inv;
            let out_b = b_s + b_d * inv;

            buf[idx] = (out_r * 255.0).round().clamp(0.0, 255.0) as u8;
            buf[idx + 1] = (out_g * 255.0).round().clamp(0.0, 255.0) as u8;
            buf[idx + 2] = (out_b * 255.0).round().clamp(0.0, 255.0) as u8;
            buf[idx + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_without_font_is_positive() {
        let engine = TextEngine {
            regular: None,
            bold: None,
        };
        let s = engine.measure("Hello", 14.0, weight::NORMAL);
        assert!(s.width > 0.0);
        assert!(s.height > 0.0);
    }

    #[test]
    fn system_engine_loads_something_on_this_host() {
        // Nie jest to twardy wymóg — jeśli brak fontów, po prostu nie rysujemy.
        let engine = TextEngine::system();
        let s = engine.measure("AG", 14.0, weight::NORMAL);
        assert!(s.width > 0.0);
    }

    #[test]
    fn blend_glyph_respects_clip() {
        // Białe 4×2, pełne krycie glifu; czerwony glif obcięty do kolumn 0..2.
        let (w, h) = (4usize, 2usize);
        let mut buf = vec![255u8; w * h * 4];
        let cov = vec![255u8; w * h];
        let clip = Some(ClipRect {
            x0: 0,
            y0: 0,
            x1: 2,
            y1: 2,
        });
        blend_glyph(&mut buf, w, h, 0, 0, w, h, &cov, 1.0, 0.0, 0.0, 1.0, clip);
        let px = |x: usize, y: usize| {
            let i = (y * w + x) * 4;
            (buf[i], buf[i + 1], buf[i + 2], buf[i + 3])
        };
        // W klipie: czerwony, pełne krycie.
        assert_eq!(px(0, 0), (255, 0, 0, 255));
        assert_eq!(px(1, 1), (255, 0, 0, 255));
        // Poza klipem: nietknięte białe tło.
        assert_eq!(px(2, 0), (255, 255, 255, 255));
        assert_eq!(px(3, 1), (255, 255, 255, 255));
    }
}
