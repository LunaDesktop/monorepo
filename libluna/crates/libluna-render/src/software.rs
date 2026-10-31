//! Backend rysowania na CPU: rasteryzuje [`Scene`] do bufora RGBA8888.
//!
//! Używa `tiny-skia` do kształtów (z antyaliasingiem, zaokrągleniami i clipem)
//! oraz [`TextEngine`] do tekstu. Bufor wyjściowy ma format zgodny z `wl_shm`
//! (ARGB8888 little-endian to to samo ułożenie bajtów co RGBA8888 — do potwierdzenia
//! przy podłączaniu Waylanda).

use libluna_core::{Color, Rect, Scale, Size};
use tiny_skia::{
    FillRule, FilterQuality, LineCap, LineJoin, Mask, Paint, Path, Pixmap, PixmapPaint, PixmapRef,
    Shader, Stroke, Transform,
};

use crate::path_cache::PathCache;
use crate::scene::{DrawCommand, ImageData, Scene};
use crate::text::TextEngine;

/// Renderer software'owy: trzyma własny bufor pikseli.
pub struct SoftwareRenderer {
    pixmap: Pixmap,
    text: TextEngine,
    paths: PathCache,
}

impl SoftwareRenderer {
    /// Tworzy renderer o zadanym rozmiarze **fizycznym** (piksele).
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            pixmap: Pixmap::new(width.max(1), height.max(1)).expect("buffer allocation"),
            text: TextEngine::system(),
            paths: PathCache::new(),
        }
    }

    /// Renderer z fontem o stałej szerokości (terminal, edytor kodu).
    pub fn new_mono(width: u32, height: u32) -> Self {
        Self {
            pixmap: Pixmap::new(width.max(1), height.max(1)).expect("buffer allocation"),
            text: TextEngine::mono(),
            paths: PathCache::new(),
        }
    }

    /// Zmienia rozmiar bufora (zawartość jest zerowana).
    pub fn resize(&mut self, width: u32, height: u32) {
        if self.pixmap.width() != width.max(1) || self.pixmap.height() != height.max(1) {
            self.pixmap = Pixmap::new(width.max(1), height.max(1)).expect("buffer reallocation");
        }
    }

    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// Surowy bufor RGBA8888 (premnożony) — do skopiowania w `wl_shm`.
    pub fn data(&self) -> &[u8] {
        self.pixmap.data()
    }

    /// Koduje bufor do PNG — przydatne do podglądu offline (bez kompozytora).
    pub fn encode_png(&self) -> Option<Vec<u8>> {
        self.pixmap.encode_png().ok()
    }

    pub fn text_engine(&self) -> &TextEngine {
        &self.text
    }

    /// Mierzy tekst w pikselach logicznych.
    pub fn measure_text(&self, text: &str, size: f64, weight: u16) -> Size {
        self.text.measure(text, size, weight)
    }

    /// Rasteryzuje scenę. `scale` przelicza piksele logiczne sceny na fizyczne.
    pub fn draw(&mut self, scene: &Scene, scale: f64, background: Color) {
        let SoftwareRenderer {
            pixmap,
            text,
            paths,
        } = self;
        let scale = Scale::new(scale);
        let transform = Transform::from_scale(scale.factor() as f32, scale.factor() as f32);

        // Gdy scena zaczyna się od tła pokrywającego cały bufor, wypełnianie kolorem
        // jest zbyteczne — tło i tak nadpisze wszystkie piksele.
        let bg_covers = scene.commands().iter().any(|c| {
            matches!(c, DrawCommand::Background { data }
                if data.width == pixmap.width() && data.height == pixmap.height())
        });
        if !bg_covers {
            pixmap.fill(ts_color(background));
        }

        let mut clip_stack: Vec<(Rect, f64)> = Vec::new();
        let mut mask: Option<Mask> = None;

        for cmd in scene.commands() {
            match cmd {
                DrawCommand::FillRect { rect, color } => {
                    // Ścieżka dla zwykłego prostokąta to zbędny narzut: tiny-skia
                    // ma szybkie `fill_rect` (skan linii, bez budowania `Path`).
                    if color.a > 0.0 {
                        // Jednolity, osiowo wyrównany prostokąt (tak rysujemy tło,
                        // przygaszenie okien i paski) blendujemy bezpośrednio —
                        // pomijamy generyczny pipeline tiny-skia z f32 na piksel.
                        if !fast_solid_rect(pixmap, *rect, *color, scale.factor(), mask.as_ref()) {
                            if let Some(r) = ts_rect(*rect) {
                                pixmap.fill_rect(r, &paint_rect(*color), transform, mask.as_ref());
                            }
                        }
                    }
                }
                DrawCommand::FillRoundedRect {
                    rect,
                    radius,
                    color,
                } => {
                    if *radius <= 0.0 {
                        if color.a > 0.0 {
                            if let Some(r) = ts_rect(*rect) {
                                pixmap.fill_rect(r, &paint_rect(*color), transform, mask.as_ref());
                            }
                        }
                    } else if let Some(path) = paths.rect(*rect, *radius) {
                        fill(pixmap, path, *color, transform, mask.as_ref());
                    }
                }
                DrawCommand::StrokeRoundedRect {
                    rect,
                    radius,
                    width,
                    color,
                } => {
                    if let Some(path) = paths.rect(*rect, *radius) {
                        stroke(pixmap, path, *width, *color, transform, mask.as_ref());
                    }
                }
                DrawCommand::StrokeLine {
                    from,
                    to,
                    width,
                    color,
                } => {
                    if color.a > 0.0 && *width > 0.0 {
                        if let Some(path) = paths.line(*from, *to) {
                            let stroke = Stroke {
                                width: *width as f32,
                                line_cap: LineCap::Round,
                                line_join: LineJoin::Round,
                                ..Default::default()
                            };
                            pixmap.stroke_path(
                                path,
                                &paint(*color),
                                &stroke,
                                transform,
                                mask.as_ref(),
                            );
                        }
                    }
                }
                DrawCommand::FillCircle {
                    center,
                    radius,
                    color,
                } => {
                    if let Some(path) = paths.circle(*center, *radius) {
                        fill(pixmap, path, *color, transform, mask.as_ref());
                    }
                }
                DrawCommand::StrokeCircle {
                    center,
                    radius,
                    width,
                    color,
                } => {
                    if let Some(path) = paths.circle(*center, *radius) {
                        stroke(pixmap, path, *width, *color, transform, mask.as_ref());
                    }
                }
                DrawCommand::StrokePolyline {
                    points,
                    width,
                    color,
                } => {
                    if let Some(path) = paths.polyline(points) {
                        stroke(pixmap, path, *width, *color, transform, mask.as_ref());
                    }
                }
                DrawCommand::FillPolygon { points, color } => {
                    if let Some(path) = paths.polygon(points) {
                        fill(pixmap, path, *color, transform, mask.as_ref());
                    }
                }
                DrawCommand::PushClip { rect } => {
                    let (effective, radius) = match clip_stack.last() {
                        Some((current, _)) => {
                            (current.intersection(rect).unwrap_or(Rect::ZERO), 0.0)
                        }
                        None => (*rect, 0.0),
                    };
                    clip_stack.push((effective, radius));
                    mask = paths
                        .rect(effective, radius)
                        .and_then(|path| build_mask(pixmap, path, transform));
                }
                DrawCommand::PushClipRounded { rect, radius } => {
                    let (effective, effective_radius) = match clip_stack.last() {
                        Some((current, current_radius)) => (
                            current.intersection(rect).unwrap_or(Rect::ZERO),
                            radius.min(*current_radius),
                        ),
                        None => (*rect, *radius),
                    };
                    clip_stack.push((effective, effective_radius));
                    mask = paths
                        .rect(effective, effective_radius)
                        .and_then(|path| build_mask(pixmap, path, transform));
                }
                DrawCommand::PopClip => {
                    clip_stack.pop();
                    mask = clip_stack.last().copied().and_then(|(rect, radius)| {
                        paths
                            .rect(rect, radius)
                            .and_then(|path| build_mask(pixmap, path, transform))
                    });
                }
                DrawCommand::Text(run) => {
                    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
                    // Tekst nie korzysta z maski kształtów — obcinamy wprost piksele.
                    let k = scale.factor();
                    let clip = clip_stack.last().map(|(r, _)| crate::text::ClipRect {
                        x0: (r.x() * k).floor() as i64,
                        y0: (r.y() * k).floor() as i64,
                        x1: (r.right() * k).ceil() as i64,
                        y1: (r.bottom() * k).ceil() as i64,
                    });
                    text.draw(
                        pixmap.data_mut(),
                        w,
                        h,
                        scale.to_physical(run.origin.x),
                        scale.to_physical(run.origin.y),
                        &run.text,
                        run.size,
                        run.weight,
                        run.color,
                        scale.factor(),
                        clip,
                    );
                }
                DrawCommand::Image { rect, data } => {
                    if rect.width() <= 0.0 || rect.height() <= 0.0 {
                        continue;
                    }
                    // Bez kopiowania: widok na piksele sceny.
                    let Some(src) = PixmapRef::from_bytes(&data.rgba, data.width, data.height)
                    else {
                        continue;
                    };
                    let k = scale.factor() as f32;
                    let is_1to1 = (scale.factor() - 1.0).abs() < 0.001
                        && (rect.width().round() as u32) == data.width
                        && (rect.height().round() as u32) == data.height;

                    if is_1to1 && data.opaque {
                        let (px, py) = (
                            (rect.x() * k as f64).round() as i32,
                            (rect.y() * k as f64).round() as i32,
                        );
                        match mask.as_ref() {
                            // Nieprzezroczysty obraz pod clipem: nadal można pominąć
                            // generyczny pipeline tiny-skia i blendować tylko piksele
                            // objęte maską (większość bufora to pełne krycie → kopiowanie).
                            Some(m) => blit_opaque_masked(pixmap, data, px, py, m),
                            None => blit_opaque(pixmap, data, px, py),
                        }
                        continue;
                    }

                    if is_1to1 && mask.is_none() && data.sparse {
                        // Rzadki obraz (panel, popup): większość pikseli ma `alpha == 0`.
                        // Blit z pomijaniem zer jest znacznie tańszy niż blendowanie
                        // całego prostokąta 1920×680 (~7 ms/klatkę).
                        blend_sparse(
                            pixmap,
                            data,
                            (rect.x() * k as f64).round() as i32,
                            (rect.y() * k as f64).round() as i32,
                        );
                        continue;
                    }

                    if is_1to1 && mask.is_none() {
                        // Błyskawiczna ścieżka tiny-skia: bezpośredni blit zoptymalizowany pod SIMD
                        // bez narzutu macierzy transformacji i interpolacji.
                        pixmap.draw_pixmap(
                            (rect.x() * k as f64).round() as i32,
                            (rect.y() * k as f64).round() as i32,
                            src,
                            &PixmapPaint::default(),
                            Transform::identity(),
                            None,
                        );
                    } else {
                        let sx = (rect.width() as f32 / data.width as f32) * k;
                        let sy = (rect.height() as f32 / data.height as f32) * k;
                        let t = Transform::from_scale(sx, sy)
                            .post_translate(rect.x() as f32 * k, rect.y() as f32 * k);
                        let paint = PixmapPaint {
                            quality: if data.nearest {
                                FilterQuality::Nearest
                            } else {
                                FilterQuality::Bilinear
                            },
                            ..PixmapPaint::default()
                        };
                        pixmap.draw_pixmap(0, 0, src, &paint, t, mask.as_ref());
                    }
                }
                DrawCommand::ImageDmabuf { rect, .. } => {
                    // CPU nie potrafi odczytać dmabuf — rysujemy neutralny zastępczy
                    // prostokąt. W praktyce Selene ogłasza dmabuf tylko z GPU.
                    if rect.width() > 0.0 && rect.height() > 0.0 {
                        if let Some(r) = ts_rect(*rect) {
                            pixmap.fill_rect(
                                r,
                                &paint(Color::from_rgba8(0x22, 0x24, 0x2c, 0xff)),
                                transform,
                                mask.as_ref(),
                            );
                        }
                    }
                }
                DrawCommand::Background { data } => {
                    // Tło pulpitu: pełnoekranowy obraz kopiujemy wprost (jeden `memcpy`)
                    // zamiast mieszać 2 mln pikseli. Gdy rozmiar nie zgadza się z buforem
                    // (np. podgląd), rysujemy ze skalowaniem.
                    if data.width == pixmap.width() && data.height == pixmap.height() {
                        pixmap.data_mut().copy_from_slice(&data.rgba);
                    } else if let Some(src) =
                        PixmapRef::from_bytes(&data.rgba, data.width, data.height)
                    {
                        let sx = pixmap.width() as f32 / data.width as f32;
                        let sy = pixmap.height() as f32 / data.height as f32;
                        let paint = PixmapPaint {
                            quality: FilterQuality::Bilinear,
                            ..PixmapPaint::default()
                        };
                        pixmap.draw_pixmap(0, 0, src, &paint, Transform::from_scale(sx, sy), None);
                    }
                }
            }
        }
    }
}

impl crate::Renderer for SoftwareRenderer {
    fn name(&self) -> &'static str {
        "cpu"
    }

    fn resize(&mut self, width: u32, height: u32) {
        SoftwareRenderer::resize(self, width, height);
    }

    fn width(&self) -> u32 {
        SoftwareRenderer::width(self)
    }

    fn height(&self) -> u32 {
        SoftwareRenderer::height(self)
    }

    fn data(&self) -> &[u8] {
        SoftwareRenderer::data(self)
    }

    fn draw(&mut self, scene: &Scene, scale: f64, background: Color) {
        SoftwareRenderer::draw(self, scene, scale, background);
    }

    fn encode_png(&self) -> Option<Vec<u8>> {
        SoftwareRenderer::encode_png(self)
    }

    fn text_engine(&self) -> &TextEngine {
        SoftwareRenderer::text_engine(self)
    }
}

fn ts_color(color: Color) -> tiny_skia::Color {
    let [r, g, b, a] = color.to_rgba8();
    tiny_skia::Color::from_rgba8(r, g, b, a)
}

fn ts_rect(rect: Rect) -> Option<tiny_skia::Rect> {
    tiny_skia::Rect::from_xywh(
        rect.x() as f32,
        rect.y() as f32,
        rect.width() as f32,
        rect.height() as f32,
    )
}

fn paint(color: Color) -> Paint<'static> {
    Paint {
        shader: Shader::SolidColor(ts_color(color)),
        anti_alias: true,
        ..Default::default()
    }
}

/// Jak [`paint`], ale bez antyaliasingu — dla osiowo wyrównanych prostokątów.
///
/// tiny-skia ma bug w ścieżce AA dla prostokątów o ułamkowym położeniu i grubości
/// bliskiej 1 px (np. separatorów `1.0` na `x.5`); bez AA wynik jest poprawny.
fn paint_rect(color: Color) -> Paint<'static> {
    Paint {
        shader: Shader::SolidColor(ts_color(color)),
        anti_alias: false,
        ..Default::default()
    }
}

fn fill(pixmap: &mut Pixmap, path: &Path, color: Color, transform: Transform, mask: Option<&Mask>) {
    if color.a <= 0.0 {
        return;
    }
    pixmap.fill_path(path, &paint(color), FillRule::Winding, transform, mask);
}

fn stroke(
    pixmap: &mut Pixmap,
    path: &Path,
    width: f64,
    color: Color,
    transform: Transform,
    mask: Option<&Mask>,
) {
    if color.a <= 0.0 || width <= 0.0 {
        return;
    }
    let stroke = Stroke {
        width: width as f32,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    pixmap.stroke_path(path, &paint(color), &stroke, transform, mask);
}

fn build_mask(pixmap: &Pixmap, path: &Path, transform: Transform) -> Option<Mask> {
    let mut mask = Mask::new(pixmap.width(), pixmap.height())?;
    mask.fill_path(path, FillRule::Winding, true, transform);
    Some(mask)
}

/// Dzielenie przez 255 zaokrąglone w górę (szybkie, bez FP).
fn div255(x: u32) -> u32 {
    let x = x + 128;
    (x + (x >> 8)) >> 8
}

/// Szybki blend jednolitego, osiowo wyrównanego prostokąta.
///
/// Zwraca `false`, gdy geometria nie nadaje się do ścieżki szybkiej (skala ≠ 1,
/// niecałkowite współrzędne) — wtedy rysuje tiny-skia. `mask` jest maską clipu
/// (pełny rozmiar bufora), więc jej wartość mnożymy per piksel.
fn fast_solid_rect(
    pixmap: &mut Pixmap,
    rect: Rect,
    color: Color,
    scale: f64,
    mask: Option<&Mask>,
) -> bool {
    if (scale - 1.0).abs() > 1e-3 {
        return false;
    }
    let near_int = |v: f64| (v - v.round()).abs() < 0.01;
    if !near_int(rect.x())
        || !near_int(rect.y())
        || !near_int(rect.width())
        || !near_int(rect.height())
    {
        return false;
    }
    let (dw, dh) = (pixmap.width() as i64, pixmap.height() as i64);
    let x0 = (rect.x() as i64).max(0);
    let y0 = (rect.y() as i64).max(0);
    let x1 = ((rect.x() + rect.width()) as i64).min(dw);
    let y1 = ((rect.y() + rect.height()) as i64).min(dh);
    if x0 >= x1 || y0 >= y1 {
        return true;
    }

    let [r, g, b, a] = color.to_rgba8();
    let sa = a as u32;
    // Kolor jest niemnożony; źródło do source-over musi być premultiplied.
    let sr = (r as u32 * sa + 127) / 255;
    let sg = (g as u32 * sa + 127) / 255;
    let sb = (b as u32 * sa + 127) / 255;

    let dw_usize = pixmap.width() as usize;
    let mask_stride = mask.map(|m| m.width() as usize);
    let mask_data = mask.map(|m| m.data());
    let dst = pixmap.data_mut();

    if sa == 255 && mask_data.is_none() {
        // Pełne krycie: wiersz wypełniamy wzorcem 4-bajtowym.
        let pattern = [r, g, b, 255];
        for y in y0..y1 {
            let start = (y as usize * dw_usize + x0 as usize) * 4;
            let end = (y as usize * dw_usize + x1 as usize) * 4;
            for px in dst[start..end].chunks_exact_mut(4) {
                px.copy_from_slice(&pattern);
            }
        }
        return true;
    }

    let inv_a = 255 - sa;
    let row_bytes = (x1 - x0) as usize * 4;
    for y in y0..y1 {
        let mask_row = mask_data.map(|data| {
            let stride = mask_stride.unwrap_or(dw_usize);
            &data[y as usize * stride + x0 as usize..y as usize * stride + x1 as usize]
        });
        let start = (y as usize * dw_usize + x0 as usize) * 4;
        let row = &mut dst[start..start + row_bytes];
        match mask_row {
            None => {
                for px in row.chunks_exact_mut(4) {
                    px[0] = (sr + div255(px[0] as u32 * inv_a)) as u8;
                    px[1] = (sg + div255(px[1] as u32 * inv_a)) as u8;
                    px[2] = (sb + div255(px[2] as u32 * inv_a)) as u8;
                    px[3] = (sa + div255(px[3] as u32 * inv_a)) as u8;
                }
            }
            Some(cov_row) => {
                for (px, &cov) in row.chunks_exact_mut(4).zip(cov_row) {
                    if cov == 0 {
                        continue;
                    }
                    let cov = cov as u32;
                    if cov == 255 {
                        px[0] = (sr + div255(px[0] as u32 * inv_a)) as u8;
                        px[1] = (sg + div255(px[1] as u32 * inv_a)) as u8;
                        px[2] = (sb + div255(px[2] as u32 * inv_a)) as u8;
                        px[3] = (sa + div255(px[3] as u32 * inv_a)) as u8;
                        continue;
                    }
                    let a = div255(sa * cov);
                    let inv = 255 - a;
                    let cr = div255(sr * cov);
                    let cg = div255(sg * cov);
                    let cb = div255(sb * cov);
                    px[0] = (cr + div255(px[0] as u32 * inv)) as u8;
                    px[1] = (cg + div255(px[1] as u32 * inv)) as u8;
                    px[2] = (cb + div255(px[2] as u32 * inv)) as u8;
                    px[3] = (a + div255(px[3] as u32 * inv)) as u8;
                }
            }
        }
    }
    true
}

/// Kopiuje nieprzezroczysty obraz 1:1 (bez maski) wprost do bufora.
fn blit_opaque(pixmap: &mut Pixmap, src: &ImageData, dx: i32, dy: i32) {
    blit_opaque_impl(pixmap, src, dx, dy, None)
}

/// Wariant z maską clipu: piksele poza maską zostają nietknięte, a na krawędziach
/// mieszamy źródło z tłem według pokrycia (antyaliasing clipu).
fn blit_opaque_masked(pixmap: &mut Pixmap, src: &ImageData, dx: i32, dy: i32, mask: &Mask) {
    blit_opaque_impl(pixmap, src, dx, dy, Some(mask))
}

fn blit_opaque_impl(pixmap: &mut Pixmap, src: &ImageData, dx: i32, dy: i32, mask: Option<&Mask>) {
    let dw = pixmap.width() as i32;
    let dh = pixmap.height() as i32;
    let sw = src.width as i32;
    let sh = src.height as i32;
    let x0 = dx.max(0);
    let x1 = (dx + sw).min(dw);
    let y0 = dy.max(0);
    let y1 = (dy + sh).min(dh);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let copy_bytes = (x1 - x0) as usize * 4;
    let src_stride = sw as usize * 4;
    let dst_stride = dw as usize * 4;
    let mask_width = mask.map(|m| m.width() as usize);
    let mask_data = mask.map(|m| m.data());
    let dst = pixmap.data_mut();
    for y in y0..y1 {
        let src_start = ((y - dy) as usize * src_stride) + (x0 - dx) as usize * 4;
        let dst_start = y as usize * dst_stride + x0 as usize * 4;
        let src_row = &src.rgba[src_start..src_start + copy_bytes];
        let dst_row = &mut dst[dst_start..dst_start + copy_bytes];
        let Some(mask_data) = mask_data else {
            dst_row.copy_from_slice(src_row);
            continue;
        };
        let stride = mask_width.unwrap_or(dw as usize);
        let cov_row =
            &mask_data[y as usize * stride + x0 as usize..y as usize * stride + x1 as usize];
        for ((d, s), &cov) in dst_row
            .chunks_exact_mut(4)
            .zip(src_row.chunks_exact(4))
            .zip(cov_row)
        {
            if cov == 0 {
                continue;
            }
            if cov == 255 {
                d.copy_from_slice(s);
                continue;
            }
            let inv = 255 - cov as u32;
            for c in 0..4 {
                d[c] = (div255(s[c] as u32 * cov as u32) + div255(d[c] as u32 * inv)) as u8;
            }
        }
    }
}

/// Rysuje obraz 1:1 z pominięciem w pełni przezroczystych pikseli.
///
/// Format: premultiplied RGBA8. Dla `alpha == 255` to zwykłe kopiowanie,
/// dla pozostałych — source-over. Piksele z `alpha == 0` są pomijane, co daje
/// dużą oszczędność na panelu/popupach (przezroczyste tło).
fn blend_sparse(pixmap: &mut Pixmap, src: &ImageData, dx: i32, dy: i32) {
    let dw = pixmap.width() as i32;
    let dh = pixmap.height() as i32;
    let sw = src.width as i32;
    let sh = src.height as i32;
    let stride = dw as usize * 4;
    let dst = pixmap.data_mut();
    let x0 = dx.max(0);
    let x1 = (dx + sw).min(dw);
    let y0 = dy.max(0);
    let y1 = (dy + sh).min(dh);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    for ty in y0..y1 {
        let sy = (ty - dy) as usize;
        let srow = sy * src.width as usize * 4;
        let drow = ty as usize * stride;
        let mut tx = x0;
        while tx < x1 {
            let sx = (tx - dx) as usize;
            let si = srow + sx * 4;
            let a = src.rgba[si + 3];
            if a == 0 {
                tx += 1;
                continue;
            }
            if a == 255 {
                // Ciąg w pełni nieprzezroczystych pikseli → jeden `memcpy` na wiersz.
                let run_start = tx;
                let mut run_end = tx + 1;
                while run_end < x1 {
                    let s2 = (run_end - dx) as usize;
                    if src.rgba[srow + s2 * 4 + 3] != 255 {
                        break;
                    }
                    run_end += 1;
                }
                let ss = srow + (run_start - dx) as usize * 4;
                let se = srow + (run_end - dx) as usize * 4;
                let ds = drow + run_start as usize * 4;
                let de = drow + run_end as usize * 4;
                dst[ds..de].copy_from_slice(&src.rgba[ss..se]);
                tx = run_end;
                continue;
            }
            let inv = 255 - u32::from(a);
            let s = &src.rgba[si..si + 4];
            let d = &mut dst[drow + tx as usize * 4..][..4];
            for c in 0..3 {
                d[c] = (u32::from(s[c]) + div255(u32::from(d[c]) * inv)).min(255) as u8;
            }
            d[3] = (u32::from(a) + div255(u32::from(d[3]) * inv)).min(255) as u8;
            tx += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_core::{style::weight, Point};

    #[test]
    fn sparse_blend_skips_transparent_and_blends_alpha() {
        let mut pm = Pixmap::new(2, 2).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        let mut rgba = vec![0u8; 2 * 2 * 4];
        // (1,1): premultiplied biały 50%.
        rgba[(3) * 4..(3) * 4 + 4].copy_from_slice(&[128, 128, 128, 128]);
        let img = ImageData::new(2, 2, rgba).with_sparse(true);
        blend_sparse(&mut pm, &img, 0, 0);
        let d = pm.data();
        assert_eq!(
            &d[0..4],
            &[255, 0, 0, 255],
            "piksel alfa=0 nie może zmienić tła"
        );
        assert_eq!(
            &d[12..16],
            &[255, 128, 128, 255],
            "source-over premultiplied na czerwonym tle"
        );
    }

    fn pixel(renderer: &SoftwareRenderer, x: u32, y: u32) -> [u8; 4] {
        let w = renderer.width() as usize;
        let idx = (y as usize * w + x as usize) * 4;
        let d = renderer.data();
        [d[idx], d[idx + 1], d[idx + 2], d[idx + 3]]
    }

    #[test]
    fn opaque_image_is_copied_not_blended() {
        let mut r = SoftwareRenderer::new(6, 6);
        let mut rgba = Vec::new();
        for _ in 0..4 {
            rgba.extend_from_slice(&[0x12, 0x34, 0x56, 0xff]);
        }
        let img = ImageData::new(2, 2, rgba).with_opaque(true);
        let mut s = Scene::new();
        s.image(
            Rect::from_xywh(2.0, 2.0, 2.0, 2.0),
            std::sync::Arc::new(img),
        );
        r.draw(&s, 1.0, Color::WHITE);
        assert_eq!(pixel(&r, 2, 2), [0x12, 0x34, 0x56, 0xff]);
        assert_eq!(pixel(&r, 0, 0), [255, 255, 255, 255]);
    }

    #[test]
    fn masked_opaque_image_respects_clip() {
        let mut r = SoftwareRenderer::new(6, 6);
        let rgba = [255u8, 0, 0, 255].repeat(16);
        let img = ImageData::new(4, 4, rgba).with_opaque(true);
        let mut s = Scene::new();
        s.push_clip(Rect::from_xywh(0.0, 0.0, 2.0, 6.0));
        s.image(
            Rect::from_xywh(0.0, 0.0, 4.0, 4.0),
            std::sync::Arc::new(img),
        );
        s.pop_clip();
        r.draw(&s, 1.0, Color::WHITE);
        assert_eq!(pixel(&r, 1, 2), [255, 0, 0, 255]);
        assert_eq!(pixel(&r, 3, 2), [255, 255, 255, 255]);
    }

    #[test]
    fn fractional_hairline_rect_does_not_panic() {
        // tiny-skia ma bug w ścieżce AA dla prostokątów o ułamkowym położeniu
        // i grubości ~1 px; renderer używa wtedy ścieżki bez AA.
        let mut r = SoftwareRenderer::new(8, 8);
        let mut s = Scene::new();
        s.fill_rect(
            Rect::from_xywh(3.5, 0.0, 1.0, 8.0),
            Color::from_rgba8(0, 0, 0, 0xff),
        );
        r.draw(&s, 1.0, Color::WHITE);
        let painted = [pixel(&r, 3, 4), pixel(&r, 4, 4)]
            .iter()
            .any(|p| p[0] < 128);
        assert!(painted, "cienka linia powinna trafić w któryś piksel");
    }

    #[test]
    fn fast_solid_rect_blends_alpha() {
        let mut r = SoftwareRenderer::new(8, 8);
        let mut s = Scene::new();
        s.fill_rect(
            Rect::from_xywh(0.0, 0.0, 8.0, 8.0),
            Color::from_rgba8(0, 0, 0, 0x80),
        );
        r.draw(&s, 1.0, Color::WHITE);
        let p = pixel(&r, 4, 4);
        assert!(
            (126..=129).contains(&p[0]),
            "czarny 50% na białym powinien dać ~127, jest {}",
            p[0]
        );
        assert_eq!(p[3], 255);
    }

    #[test]
    fn fast_solid_rect_respects_clip_mask() {
        let mut r = SoftwareRenderer::new(8, 8);
        let mut s = Scene::new();
        s.push_clip(Rect::from_xywh(0.0, 0.0, 4.0, 8.0));
        s.fill_rect(
            Rect::from_xywh(0.0, 0.0, 8.0, 8.0),
            Color::from_rgba8(255, 0, 0, 255),
        );
        s.pop_clip();
        r.draw(&s, 1.0, Color::WHITE);
        assert_eq!(pixel(&r, 2, 4), [255, 0, 0, 255]);
        assert_eq!(pixel(&r, 5, 4), [255, 255, 255, 255]);
    }

    #[test]
    fn fills_background_opaque() {
        let mut r = SoftwareRenderer::new(8, 8);
        let scene = Scene::new();
        r.draw(&scene, 1.0, Color::WHITE);
        assert_eq!(pixel(&r, 0, 0), [255, 255, 255, 255]);
        assert_eq!(pixel(&r, 7, 7), [255, 255, 255, 255]);
    }

    #[test]
    fn fill_circle_paints_center_only() {
        let mut r = SoftwareRenderer::new(16, 16);
        let mut s = Scene::new();
        s.fill_circle(Point::new(8.0, 8.0), 4.0, Color::from_rgba8(255, 0, 0, 255));
        r.draw(&s, 1.0, Color::WHITE);
        assert_eq!(pixel(&r, 8, 8), [255, 0, 0, 255]);
        assert_eq!(pixel(&r, 0, 0), [255, 255, 255, 255]);
    }

    #[test]
    fn fill_polygon_covers_interior() {
        let mut r = SoftwareRenderer::new(16, 16);
        let mut s = Scene::new();
        s.fill_polygon(
            &[
                Point::new(2.0, 2.0),
                Point::new(14.0, 2.0),
                Point::new(8.0, 14.0),
            ],
            Color::from_rgba8(0, 255, 0, 255),
        );
        r.draw(&s, 1.0, Color::WHITE);
        assert_eq!(pixel(&r, 8, 6), [0, 255, 0, 255]);
        assert_eq!(pixel(&r, 0, 0), [255, 255, 255, 255]);
    }

    #[test]
    fn stroke_polyline_paints_segment() {
        let mut r = SoftwareRenderer::new(16, 16);
        let mut s = Scene::new();
        s.stroke_polyline(
            &[
                Point::new(2.0, 8.0),
                Point::new(8.0, 8.0),
                Point::new(8.0, 14.0),
            ],
            3.0,
            Color::from_rgba8(0, 0, 255, 255),
        );
        r.draw(&s, 1.0, Color::WHITE);
        assert_eq!(pixel(&r, 5, 8), [0, 0, 255, 255]);
        assert_eq!(pixel(&r, 8, 12), [0, 0, 255, 255]);
        assert_eq!(pixel(&r, 14, 14), [255, 255, 255, 255]);
    }

    #[test]
    fn draws_red_rect_center_but_not_corner() {
        let mut r = SoftwareRenderer::new(60, 60);
        let mut scene = Scene::new();
        scene.fill_rounded_rect(
            Rect::from_xywh(10.0, 10.0, 40.0, 40.0),
            10.0,
            Color::from_rgba8(255, 0, 0, 255),
        );
        r.draw(&scene, 1.0, Color::WHITE);

        // Środek prostokąta jest czerwony.
        let c = pixel(&r, 30, 30);
        assert!(c[0] > 200 && c[1] < 40 && c[2] < 40, "center: {c:?}");
        // Narożnik kwadratu (10,10) wypada poza zaokrąglenie → tło.
        let corner = pixel(&r, 10, 10);
        assert!(
            corner[0] > 200 && corner[1] > 200 && corner[2] > 200,
            "corner: {corner:?}"
        );
    }

    #[test]
    fn rounded_clip_cuts_corners() {
        let mut r = SoftwareRenderer::new(40, 40);
        let mut scene = Scene::new();
        scene
            .push_clip_rounded(Rect::from_xywh(0.0, 0.0, 40.0, 40.0), 12.0)
            .fill_rect(
                Rect::from_xywh(0.0, 0.0, 40.0, 40.0),
                Color::from_rgba8(255, 0, 0, 255),
            )
            .pop_clip();
        r.draw(&scene, 1.0, Color::WHITE);

        // Narożnik poza promieniem pozostaje tłem, środek jest wypełniony.
        assert_eq!(pixel(&r, 1, 1), [255, 255, 255, 255]);
        assert_eq!(pixel(&r, 20, 20), [255, 0, 0, 255]);
    }

    #[test]
    fn clip_limits_drawing() {
        let mut r = SoftwareRenderer::new(40, 40);
        let mut scene = Scene::new();
        scene
            .push_clip(Rect::from_xywh(0.0, 0.0, 10.0, 40.0))
            .fill_rect(
                Rect::from_xywh(0.0, 0.0, 40.0, 40.0),
                Color::from_rgba8(0, 0, 255, 255),
            )
            .pop_clip();
        r.draw(&scene, 1.0, Color::WHITE);

        let inside = pixel(&r, 5, 20);
        assert!(inside[2] > 200 && inside[0] < 40, "w clipie: {inside:?}");
        let outside = pixel(&r, 30, 20);
        assert!(
            outside[0] > 200 && outside[2] > 200,
            "poza clipem: {outside:?}"
        );
    }

    #[test]
    fn scale_2x_halves_effective_area() {
        let mut r = SoftwareRenderer::new(20, 20);
        let mut scene = Scene::new();
        // Prostokąt 5x5 w px logicznych przy scale 2 zajmuje 10x10 px fizycznych.
        scene.fill_rect(
            Rect::from_xywh(0.0, 0.0, 5.0, 5.0),
            Color::from_rgba8(0, 0, 0, 255),
        );
        r.draw(&scene, 2.0, Color::WHITE);
        assert_eq!(pixel(&r, 9, 9)[0], 0);
        assert_eq!(pixel(&r, 10, 10)[0], 255);
    }

    #[test]
    fn text_measurement_never_panics() {
        let r = SoftwareRenderer::new(16, 16);
        let s = r.measure_text("Ustawienia", 14.0, weight::NORMAL);
        assert!(s.width > 0.0);
        // Punkt nieużywany wprost — upewnia się, że typ jest w zasięgu.
        let _ = Point::ZERO;
    }

    #[test]
    fn draws_image_into_rect() {
        use crate::scene::ImageData;
        use std::sync::Arc;
        let mut r = SoftwareRenderer::new(20, 20);
        // Czerwony obraz 2×2 wpisany w prostokąt 10×10 w (5,5).
        let data = ImageData::new(
            2,
            2,
            vec![
                255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
            ],
        );
        let mut scene = Scene::new();
        scene.image(Rect::from_xywh(5.0, 5.0, 10.0, 10.0), Arc::new(data));
        r.draw(&scene, 1.0, Color::WHITE);
        assert_eq!(pixel(&r, 10, 10), [255, 0, 0, 255]);
        assert_eq!(pixel(&r, 1, 1), [255, 255, 255, 255]);
    }
}
