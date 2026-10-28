//! `Slider` — suwak (zakres 0..1). Przeciąganie obsługuje aplikacja, wołając
//! [`Slider::value_at`] na zdarzeniach ruchu; widget odpowiada za wygląd i geometrię.

use libluna_core::{Color, Insets, Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Suwak; `rect` oznacza tor (cienki pasek), a uchwyt rysowany jest ponad niego.
#[derive(Debug, Clone, Copy, Default)]
pub struct Slider {
    value: f64,
}

impl Slider {
    pub fn new(value: f64) -> Self {
        Self {
            value: value.clamp(0.0, 1.0),
        }
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn set_value(&mut self, value: f64) {
        self.value = value.clamp(0.0, 1.0);
    }

    /// Wartość dla pozycji wskaźnika (0..1) względem toru.
    pub fn value_at(&self, track: Rect, p: Point) -> f64 {
        if track.width() <= 0.0 {
            return 0.0;
        }
        ((p.x - track.x()) / track.width()).clamp(0.0, 1.0)
    }

    /// Czy punkt trafia w obszar suwaka (z tolerancją z metryk motywu).
    pub fn hit(&self, track: Rect, p: Point, theme: &Theme) -> bool {
        let m = theme.metrics.slider;
        let area = track.inflate(Insets::new(m.hit_pad, m.knob_r, m.hit_pad, m.knob_r));
        area.contains(p)
    }

    pub fn paint(&self, ctx: &mut PaintCtx, track: Rect) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let knob_r = theme.metrics.slider.knob_r;
        ctx.scene
            .fill_rounded_rect(track, track.height() / 2.0, colors.shade);

        let filled_w = track.width() * self.value;
        ctx.scene.fill_rounded_rect(
            Rect::from_xywh(track.x(), track.y(), filled_w, track.height()),
            track.height() / 2.0,
            colors.accent,
        );

        let cy = track.center().y;
        let knob_cx = track.x() + filled_w;
        let knob = Rect::from_xywh(knob_cx - knob_r, cy - knob_r, knob_r * 2.0, knob_r * 2.0);
        ctx.scene.fill_rounded_rect(knob, knob_r, Color::WHITE);
        ctx.scene
            .stroke_rounded_rect(knob, knob_r, 1.0, colors.border);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Rect {
        Rect::from_xywh(100.0, 50.0, 200.0, 6.0)
    }

    #[test]
    fn value_at_maps_and_clamps() {
        let s = Slider::new(0.0);
        assert!((s.value_at(track(), Point::new(100.0, 53.0)) - 0.0).abs() < 1e-9);
        assert!((s.value_at(track(), Point::new(200.0, 53.0)) - 0.5).abs() < 1e-9);
        assert!((s.value_at(track(), Point::new(500.0, 53.0)) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn hit_has_vertical_tolerance() {
        let theme = Theme::light();
        let s = Slider::new(0.5);
        assert!(s.hit(track(), Point::new(150.0, 53.0), &theme));
        assert!(s.hit(track(), Point::new(150.0, 40.0), &theme)); // nad torem, w tolerancji
        assert!(!s.hit(track(), Point::new(50.0, 53.0), &theme)); // poza torem w poziomie
    }
}
