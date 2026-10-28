//! Konwersja SVG w obie strony.
//!
//! Systemowe zestawy ikon (Papirus, Adwaita, `hicolor`) często dostarczają ikony
//! aplikacji wyłącznie jako SVG. `resvg` (czysty Rust, ten sam `tiny-skia`) pozwala
//! pokazać oryginalną grafikę zamiast ogólnej ikony wektorowej Luny.
//!
//! W drugą stronę [`scene_to_svg`] zapisuje [`Scene`] jako samodzielny plik SVG —
//! dzięki temu ten sam kod, który rysuje ikony w interfejsie, generuje też
//! skalowalny pakiet dla pasków aplikacji i innych toolkitów.

use crate::scene::{DrawCommand, ImageData, Scene};
use libluna_core::{Color, Rect};

/// Rasteryzuje SVG do obrazu o dokładnie `width`×`height`.
///
/// Zachowuje proporcje i centruje grafikę; tło pozostaje przezroczyste.
/// `None`, gdy SVG jest niepoprawne albo nie da się utworzyć bufora.
pub fn decode_svg(bytes: &[u8], width: u32, height: u32) -> Option<ImageData> {
    let (width, height) = (width.max(1), height.max(1));
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    if size.width() <= 0.0 || size.height() <= 0.0 {
        return None;
    }

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    let scale = (width as f32 / size.width()).min(height as f32 / size.height());
    let dx = (width as f32 - size.width() * scale) / 2.0;
    let dy = (height as f32 - size.height() * scale) / 2.0;
    let transform = resvg::tiny_skia::Transform::from_translate(dx, dy).pre_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    Some(ImageData::new(width, height, pixmap.data().to_vec()))
}

/// Zapisuje scenę jako samodzielny plik SVG.
///
/// `width` i `height` trafiają do `viewBox`, więc grafika skaluje się bez utraty
/// jakości (np. w paskach aplikacji). Eksportowane są wszystkie prymitywy
/// wektorowe; tekst, obrazy i tło są pomijane, bo wymagałyby osadzenia danych —
/// ikony Luny ich nie używają.
///
/// Kolory są zapisywane jako `#rrggbb` (z `fill-opacity`/`stroke-opacity`, gdy
/// kanał alfa jest mniejszy od 255), a linie mają zaokrąglone końce i zgięcia —
/// dokładnie tak, jak rysuje je [`crate::SoftwareRenderer`].
pub fn scene_to_svg(scene: &Scene, width: f64, height: f64) -> String {
    let (width, height) = (width.max(1.0), height.max(1.0));
    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
        num(width),
        num(height),
        num(width),
        num(height)
    ));

    let mut defs = String::new();
    let mut clip_id = 0usize;
    let mut open_clips = 0usize;

    for command in scene.commands() {
        match command {
            DrawCommand::FillRect { rect, color } => {
                svg.push_str(&format!(
                    "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}/>\n",
                    num(rect.x()),
                    num(rect.y()),
                    num(rect.width()),
                    num(rect.height()),
                    paint_attrs(*color, "fill")
                ));
            }
            DrawCommand::FillRoundedRect {
                rect,
                radius,
                color,
            } => {
                svg.push_str(&format!(
                    "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"{}/>\n",
                    num(rect.x()),
                    num(rect.y()),
                    num(rect.width()),
                    num(rect.height()),
                    num(clamp_radius(*rect, *radius)),
                    paint_attrs(*color, "fill")
                ));
            }
            DrawCommand::StrokeRoundedRect {
                rect,
                radius,
                width,
                color,
            } => {
                svg.push_str(&format!(
                    "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"none\" stroke-width=\"{}\"{}/>\n",
                    num(rect.x()),
                    num(rect.y()),
                    num(rect.width()),
                    num(rect.height()),
                    num(clamp_radius(*rect, *radius)),
                    num(*width),
                    paint_attrs(*color, "stroke")
                ));
            }
            DrawCommand::StrokeLine {
                from,
                to,
                width,
                color,
            } => {
                svg.push_str(&format!(
                    "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\"{}/>\n",
                    num(from.x),
                    num(from.y),
                    num(to.x),
                    num(to.y),
                    num(*width),
                    paint_attrs(*color, "stroke")
                ));
            }
            DrawCommand::FillCircle {
                center,
                radius,
                color,
            } => {
                svg.push_str(&format!(
                    "  <circle cx=\"{}\" cy=\"{}\" r=\"{}\"{}/>\n",
                    num(center.x),
                    num(center.y),
                    num(*radius),
                    paint_attrs(*color, "fill")
                ));
            }
            DrawCommand::StrokeCircle {
                center,
                radius,
                width,
                color,
            } => {
                svg.push_str(&format!(
                    "  <circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"none\" stroke-width=\"{}\"{}/>\n",
                    num(center.x),
                    num(center.y),
                    num(*radius),
                    num(*width),
                    paint_attrs(*color, "stroke")
                ));
            }
            DrawCommand::StrokePolyline {
                points,
                width,
                color,
            } => {
                svg.push_str(&format!(
                    "  <polyline points=\"{}\" fill=\"none\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"{}/>\n",
                    points_attr(points),
                    num(*width),
                    paint_attrs(*color, "stroke")
                ));
            }
            DrawCommand::FillPolygon { points, color } => {
                svg.push_str(&format!(
                    "  <polygon points=\"{}\"{}/>\n",
                    points_attr(points),
                    paint_attrs(*color, "fill")
                ));
            }
            DrawCommand::PushClip { rect } => {
                open_clip(
                    &mut svg,
                    &mut defs,
                    &mut clip_id,
                    &mut open_clips,
                    *rect,
                    None,
                );
            }
            DrawCommand::PushClipRounded { rect, radius } => {
                open_clip(
                    &mut svg,
                    &mut defs,
                    &mut clip_id,
                    &mut open_clips,
                    *rect,
                    Some(*radius),
                );
            }
            DrawCommand::PopClip => {
                if open_clips > 0 {
                    open_clips -= 1;
                    svg.push_str("  </g>\n");
                }
            }
            DrawCommand::Text(_)
            | DrawCommand::Image { .. }
            | DrawCommand::ImageDmabuf { .. }
            | DrawCommand::Background { .. } => {
                svg.push_str("  <!-- pominięto: tekst/obraz nie jest eksportowany -->\n");
            }
        }
    }

    // Scena z niezamkniętym clipem nadal musi dać poprawny dokument.
    for _ in 0..open_clips {
        svg.push_str("  </g>\n");
    }
    if !defs.is_empty() {
        svg.push_str("  <defs>\n");
        svg.push_str(&defs);
        svg.push_str("  </defs>\n");
    }
    svg.push_str("</svg>\n");
    svg
}

fn open_clip(
    svg: &mut String,
    defs: &mut String,
    clip_id: &mut usize,
    open_clips: &mut usize,
    rect: Rect,
    radius: Option<f64>,
) {
    let id = *clip_id;
    *clip_id += 1;
    *open_clips += 1;
    let radius = radius.map(|r| clamp_radius(rect, r));
    match radius {
        Some(r) => defs.push_str(&format!(
            "    <clipPath id=\"clip{id}\" clipPathUnits=\"userSpaceOnUse\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"/></clipPath>\n",
            num(rect.x()),
            num(rect.y()),
            num(rect.width()),
            num(rect.height()),
            num(r)
        )),
        None => defs.push_str(&format!(
            "    <clipPath id=\"clip{id}\" clipPathUnits=\"userSpaceOnUse\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>\n",
            num(rect.x()),
            num(rect.y()),
            num(rect.width()),
            num(rect.height())
        )),
    }
    svg.push_str(&format!("  <g clip-path=\"url(#clip{id})\">\n"));
}

fn clamp_radius(rect: Rect, radius: f64) -> f64 {
    radius
        .max(0.0)
        .min(rect.width() / 2.0)
        .min(rect.height() / 2.0)
}

fn points_attr(points: &[libluna_core::Point]) -> String {
    points
        .iter()
        .map(|p| format!("{},{}", num(p.x), num(p.y)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn paint_attrs(color: Color, kind: &str) -> String {
    let [r, g, b, a] = color.to_rgba8();
    let mut attrs = format!(" {kind}=\"#{r:02x}{g:02x}{b:02x}\"");
    if a < 255 {
        attrs.push_str(&format!(" {kind}-opacity=\"{}\"", num(a as f64 / 255.0)));
    }
    attrs
}

/// Liczba zmiennoprzecinkowa bez zbędnych zer (np. `12`, `1.6`, `0.55`).
fn num(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let mut text = format!("{:.3}", value);
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text.is_empty() || text == "-0" {
        "0".to_string()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16">
        <rect x="2" y="2" width="12" height="12" rx="3" fill="#cbbee8"/>
    </svg>"##;

    #[test]
    fn renders_svg_on_transparent_canvas() {
        let image = decode_svg(ICON.as_bytes(), 32, 32).unwrap();
        assert_eq!((image.width, image.height), (32, 32));
        assert_eq!(image.rgba.len(), 32 * 32 * 4);
        // Rogi są przezroczyste, środek ma kolor.
        assert_eq!(image.rgba[3], 0);
        let center = (16 * 32 + 16) * 4;
        assert!(image.rgba[center + 3] > 0);
    }

    #[test]
    fn invalid_svg_is_rejected() {
        assert!(decode_svg(b"to nie jest svg", 16, 16).is_none());
    }

    #[test]
    fn scene_exports_to_valid_svg() {
        use crate::scene::Scene;
        use libluna_core::{Color, Point, Rect};

        let mut scene = Scene::new();
        scene.fill_rounded_rect(
            Rect::from_xywh(2.0, 2.0, 20.0, 20.0),
            4.0,
            Color::from_rgba8(0xf3, 0xd2, 0x7c, 255),
        );
        scene.stroke_polyline(
            &[
                Point::new(6.0, 12.0),
                Point::new(12.0, 18.0),
                Point::new(18.0, 8.0),
            ],
            1.6,
            Color::from_rgba8(0xa4, 0x78, 0x2c, 255),
        );

        let svg = scene_to_svg(&scene, 24.0, 24.0);
        assert!(svg.contains("viewBox=\"0 0 24 24\""));
        assert!(svg.contains("fill=\"#f3d27c\""));
        assert!(svg.contains("stroke-linecap=\"round\""));

        // Wynik musi być poprawnym SVG, który renderer potrafi z powrotem wczytać.
        let image = decode_svg(svg.as_bytes(), 24, 24).expect("poprawne SVG");
        let center = (12 * 24 + 12) * 4;
        assert!(image.rgba[center + 3] > 0);
    }

    #[test]
    fn scene_svg_skips_non_vector_commands() {
        use crate::scene::{ImageData, Scene, TextRun};
        use libluna_core::Point;
        use std::sync::Arc;

        let mut scene = Scene::new();
        scene.text(TextRun::new(Point::new(2.0, 8.0), "x", 12.0));
        scene.image(
            Rect::from_xywh(0.0, 0.0, 4.0, 4.0),
            Arc::new(ImageData::empty(4, 4)),
        );

        let svg = scene_to_svg(&scene, 24.0, 24.0);
        assert!(svg.contains("pominięto"));
        assert!(decode_svg(svg.as_bytes(), 24, 24).is_some());
    }
}
