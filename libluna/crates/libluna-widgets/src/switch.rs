//! `Switch` — przełącznik (włącz/wyłącz).

use libluna_core::{Color, Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Przełącznik. Widget immediate: trzyma stan „wł.” i podświetlenie.
#[derive(Debug, Clone, Copy, Default)]
pub struct Switch {
    on: bool,
    hovered: bool,
}

impl Switch {
    pub fn new(on: bool) -> Self {
        Self { on, hovered: false }
    }

    pub fn is_on(&self) -> bool {
        self.on
    }

    pub fn set_on(&mut self, on: bool) {
        self.on = on;
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    pub fn size(&self, theme: &Theme) -> (f64, f64) {
        (theme.metrics.switch.width, theme.metrics.switch.height)
    }

    pub fn hit(&self, rect: Rect, p: Point) -> bool {
        rect.contains(p)
    }

    /// Zwraca `true`, jeśli klik trafił w przełącznik (zmianę stanu robi aplikacja).
    pub fn click_at(&self, rect: Rect, p: Point) -> bool {
        rect.contains(p)
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let bg = if self.on {
            colors.accent
        } else if self.hovered {
            colors.shade.scale_alpha(1.8)
        } else {
            colors.shade
        };
        ctx.scene.fill_rounded_rect(rect, rect.height() / 2.0, bg);

        // Promień i wcięcie jako proporcje wysokości, więc geometria skaluje się z metrykami.
        let knob_r = rect.height() * 0.375;
        let inset = rect.height() * 0.125;
        let cy = rect.center().y;
        let knob_x = if self.on {
            rect.right() - inset - knob_r
        } else {
            rect.x() + inset + knob_r
        };
        ctx.scene.fill_rounded_rect(
            Rect::from_xywh(knob_x - knob_r, cy - knob_r, knob_r * 2.0, knob_r * 2.0),
            knob_r,
            Color::WHITE,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_test() {
        let theme = Theme::light();
        let (w, h) = Switch::new(true).size(&theme);
        let rect = Rect::from_xywh(0.0, 0.0, w, h);
        let s = Switch::new(true);
        assert!(s.click_at(rect, Point::new(10.0, 10.0)));
        assert!(!s.click_at(rect, Point::new(100.0, 10.0)));
        assert!(s.is_on());
    }

    #[test]
    fn metrics_drive_size() {
        let mut theme = Theme::light();
        theme.metrics.switch.width = 60.0;
        theme.metrics.switch.height = 30.0;
        assert_eq!(Switch::new(false).size(&theme), (60.0, 30.0));
    }
}
