//! Backend rysowania na CPU: rasteryzuje [`Scene`] do bufora RGBA8888.
//!
//! Używa `tiny-skia` do kształtów (z antyaliasingiem, zaokrągleniami i clipem)
//! oraz [`TextEngine`] do tekstu. Bufor wyjściowy ma format zgodny z `wl_shm`
//! (ARGB8888 little-endian to to samo ułożenie bajtów co RGBA8888 — do potwierdzenia
//! przy podłączaniu Waylanda).

use libluna_core::{Color, Point, Rect, Scale, Size};
use tiny_skia::{
    FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, Shader, Stroke, Transform,
};

use crate::scene::{DrawCommand, Scene};
use crate::text::TextEngine;

/// Renderer software'owy: trzyma własny bufor pikseli.
pub struct SoftwareRenderer {
    pixmap: Pixmap,
    text: TextEngine,
}

impl SoftwareRenderer {
    /// Tworzy renderer o zadanym rozmiarze **fizycznym** (piksele).
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            pixmap: Pixmap::new(width.max(1), height.max(1)).expect("buffer allocation"),
            text: TextEngine::system(),
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
        let SoftwareRenderer { pixmap, text } = self;
        let scale = Scale::new(scale);
        let transform = Transform::from_scale(scale.factor() as f32, scale.factor() as f32);

        pixmap.fill(ts_color(background));

        let mut clip_stack: Vec<Rect> = Vec::new();
        let mut mask: Option<Mask> = None;

        for cmd in scene.commands() {
            match cmd {
                DrawCommand::FillRect { rect, color } => {
                    if let Some(path) = rect_path(*rect, 0.0) {
                        fill(pixmap, &path, *color, transform, mask.as_ref());
                    }
                }
                DrawCommand::FillRoundedRect {
                    rect,
                    radius,
                    color,
                } => {
                    if let Some(path) = rect_path(*rect, *radius) {
                        fill(pixmap, &path, *color, transform, mask.as_ref());
                    }
                }
                DrawCommand::StrokeRoundedRect {
                    rect,
                    radius,
                    width,
                    color,
                } => {
                    if let Some(path) = rect_path(*rect, *radius) {
                        stroke(pixmap, &path, *width, *color, transform, mask.as_ref());
                    }
                }
                DrawCommand::StrokeLine {
                    from,
                    to,
                    width,
                    color,
                } => {
                    if color.a > 0.0 && *width > 0.0 {
                        if let Some(path) = line_path(*from, *to) {
                            let stroke = Stroke {
                                width: *width as f32,
                                line_cap: LineCap::Round,
                                line_join: LineJoin::Round,
                                ..Default::default()
                            };
                            pixmap.stroke_path(
                                &path,
                                &paint(*color),
                                &stroke,
                                transform,
                                mask.as_ref(),
                            );
                        }
                    }
                }
                DrawCommand::PushClip { rect } => {
                    let effective = match clip_stack.last() {
                        Some(current) => current.intersection(rect).unwrap_or(Rect::ZERO),
                        None => *rect,
                    };
                    clip_stack.push(effective);
                    mask = build_mask(pixmap, effective, transform);
                }
                DrawCommand::PopClip => {
                    clip_stack.pop();
                    mask = clip_stack
                        .last()
                        .copied()
                        .and_then(|r| build_mask(pixmap, r, transform));
                }
                DrawCommand::Text(run) => {
                    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
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
                    );
                }
            }
        }
    }
}

fn ts_color(color: Color) -> tiny_skia::Color {
    let [r, g, b, a] = color.to_rgba8();
    tiny_skia::Color::from_rgba8(r, g, b, a)
}

fn line_path(from: Point, to: Point) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(from.x as f32, from.y as f32);
    pb.line_to(to.x as f32, to.y as f32);
    pb.finish()
}

fn rect_path(rect: Rect, radius: f64) -> Option<Path> {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return None;
    }
    let x = rect.x() as f32;
    let y = rect.y() as f32;
    let w = rect.width() as f32;
    let h = rect.height() as f32;
    let mut pb = PathBuilder::new();
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0) as f32;
    if r > 0.0 {
        pb.move_to(x + r, y);
        pb.line_to(x + w - r, y);
        pb.quad_to(x + w, y, x + w, y + r);
        pb.line_to(x + w, y + h - r);
        pb.quad_to(x + w, y + h, x + w - r, y + h);
        pb.line_to(x + r, y + h);
        pb.quad_to(x, y + h, x, y + h - r);
        pb.line_to(x, y + r);
        pb.quad_to(x, y, x + r, y);
        pb.close();
    } else {
        pb.push_rect(tiny_skia::Rect::from_xywh(x, y, w, h)?);
    }
    pb.finish()
}

fn paint(color: Color) -> Paint<'static> {
    Paint {
        shader: Shader::SolidColor(ts_color(color)),
        anti_alias: true,
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
        ..Default::default()
    };
    pixmap.stroke_path(path, &paint(color), &stroke, transform, mask);
}

fn build_mask(pixmap: &Pixmap, rect: Rect, transform: Transform) -> Option<Mask> {
    let mut mask = Mask::new(pixmap.width(), pixmap.height())?;
    if let Some(path) = rect_path(rect, 0.0) {
        mask.fill_path(&path, FillRule::Winding, true, transform);
    }
    Some(mask)
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_core::{style::weight, Point};

    fn pixel(renderer: &SoftwareRenderer, x: u32, y: u32) -> [u8; 4] {
        let w = renderer.width() as usize;
        let idx = (y as usize * w + x as usize) * 4;
        let d = renderer.data();
        [d[idx], d[idx + 1], d[idx + 2], d[idx + 3]]
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
}
