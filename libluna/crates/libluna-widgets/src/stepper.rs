//! `Stepper` — krokowy wybór liczby: `[ − ]  wartość  [ + ]`.
//!
//! Widget immediate: trzyma wartość w zakresie `[min, max]` i krok. Aplikacja woła
//! [`Stepper::paint`] oraz [`Stepper::click_at`] (albo [`Stepper::inc`] / [`Stepper::dec`]).

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Która strona krokowca.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepperSide {
    Minus,
    Plus,
}

/// Krokowiec.
#[derive(Debug, Clone, Copy)]
pub struct Stepper {
    value: i64,
    min: i64,
    max: i64,
    step: i64,
    hovered: Option<StepperSide>,
}

impl Stepper {
    pub fn new(value: i64, min: i64, max: i64) -> Self {
        Self {
            value: value.clamp(min, max),
            min,
            max,
            step: 1,
            hovered: None,
        }
    }

    pub fn step(mut self, step: i64) -> Self {
        self.step = step.max(1);
        self
    }

    pub fn value(&self) -> i64 {
        self.value
    }

    pub fn min(&self) -> i64 {
        self.min
    }

    pub fn max(&self) -> i64 {
        self.max
    }

    pub fn set_value(&mut self, value: i64) {
        self.value = value.clamp(self.min, self.max);
    }

    pub fn inc(&mut self) {
        self.set_value(self.value + self.step);
    }

    pub fn dec(&mut self) {
        self.set_value(self.value - self.step);
    }

    pub fn set_hovered(&mut self, side: Option<StepperSide>) {
        self.hovered = side;
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.stepper.height
    }

    pub fn width(&self, theme: &Theme) -> f64 {
        theme.metrics.stepper.width
    }

    /// Prostokąt przycisku `−`/`+`.
    pub fn side_rect(&self, rect: Rect, theme: &Theme, side: StepperSide) -> Rect {
        let b = theme.metrics.stepper.button.clamp(0.0, rect.height());
        let y = rect.center().y - b / 2.0;
        match side {
            StepperSide::Minus => Rect::from_xywh(rect.x(), y, b, b),
            StepperSide::Plus => Rect::from_xywh(rect.right() - b, y, b, b),
        }
    }

    /// Która strona jest pod punktem `p`.
    pub fn side_at(&self, rect: Rect, theme: &Theme, p: Point) -> Option<StepperSide> {
        [StepperSide::Minus, StepperSide::Plus]
            .into_iter()
            .find(|&side| self.side_rect(rect, theme, side).contains(p))
    }

    /// Obsługuje klik: zmienia wartość. Zwraca `true`, jeśli trafiono w przycisk.
    pub fn click_at(&mut self, rect: Rect, theme: &Theme, p: Point) -> bool {
        match self.side_at(rect, theme, p) {
            Some(StepperSide::Minus) => {
                self.dec();
                true
            }
            Some(StepperSide::Plus) => {
                self.inc();
                true
            }
            None => false,
        }
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let radius = theme.radius.small;
        ctx.scene.fill_rounded_rect(rect, radius, colors.shade);

        let b = theme.metrics.stepper.button.clamp(0.0, rect.height());
        let arm = b * 0.16;
        for side in [StepperSide::Minus, StepperSide::Plus] {
            let sr = self.side_rect(rect, &theme, side);
            if self.hovered == Some(side) {
                ctx.scene
                    .fill_rounded_rect(sr, radius, colors.shade.scale_alpha(1.8));
            }
            let c = sr.center();
            match side {
                StepperSide::Minus => {
                    ctx.scene.stroke_line(
                        Point::new(c.x - arm, c.y),
                        Point::new(c.x + arm, c.y),
                        2.0,
                        colors.text,
                    );
                }
                StepperSide::Plus => {
                    ctx.scene.stroke_line(
                        Point::new(c.x - arm, c.y),
                        Point::new(c.x + arm, c.y),
                        2.0,
                        colors.text,
                    );
                    ctx.scene.stroke_line(
                        Point::new(c.x, c.y - arm),
                        Point::new(c.x, c.y + arm),
                        2.0,
                        colors.text,
                    );
                }
            }
        }

        ctx.label_center(
            rect.center().x,
            rect.center().y,
            &self.value.to_string(),
            theme.typography.body.size,
            weight::MEDIUM,
            colors.text,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 120.0, 34.0)
    }

    #[test]
    fn value_is_clamped() {
        let mut s = Stepper::new(5, 0, 10);
        s.set_value(99);
        assert_eq!(s.value(), 10);
        s.set_value(-5);
        assert_eq!(s.value(), 0);
    }

    #[test]
    fn click_on_sides_changes_value() {
        let theme = Theme::light();
        let mut s = Stepper::new(5, 0, 10);
        let plus = s.side_rect(rect(), &theme, StepperSide::Plus).center();
        let minus = s.side_rect(rect(), &theme, StepperSide::Minus).center();
        assert!(s.click_at(rect(), &theme, plus));
        assert_eq!(s.value(), 6);
        assert!(s.click_at(rect(), &theme, minus));
        assert_eq!(s.value(), 5);
        // Środek nie zmienia wartości.
        assert!(!s.click_at(rect(), &theme, rect().center()));
    }

    #[test]
    fn step_and_bounds() {
        let mut s = Stepper::new(32, 16, 64).step(8);
        s.inc();
        assert_eq!(s.value(), 40);
        s.set_value(64);
        s.inc();
        assert_eq!(s.value(), 64);
    }
}
