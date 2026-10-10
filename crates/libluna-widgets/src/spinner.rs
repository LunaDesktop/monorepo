//! `Spinner` — wskaźnik zajętości: obracający się łuk (bez animacji własnej — fazę
//! wyznacza aplikacja przez [`Spinner::set_progress`], np. z licznika klatek/czasu).
//!
//! Widget immediate: [`Spinner::paint`] rysuje ślad i łuk w zadanym prostokącie.

use std::f64::consts::TAU;

use libluna_core::{Color, Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Liczba segmentów aproksymujących łuk.
const SEGMENTS: usize = 32;

/// Wskaźnik zajętości.
#[derive(Debug, Clone, Copy, Default)]
pub struct Spinner {
    /// Faza obrotu w zakresie 0..1.
    progress: f64,
}

impl Spinner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ustawia fazę obrotu (zawijana do 0..1).
    pub fn set_progress(&mut self, progress: f64) {
        self.progress = progress.rem_euclid(1.0);
    }

    pub fn progress(&self) -> f64 {
        self.progress
    }

    /// Domyślny bok wskaźnika (z metryk).
    pub fn size(&self, theme: &Theme) -> f64 {
        theme.metrics.spinner.size
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let m = theme.metrics.spinner;
        let radius = (rect.width().min(rect.height()) - m.thickness).max(0.0) / 2.0;
        let center = rect.center();
        // Ślad (pełny, wyciszony) + obracający się łuk.
        draw_arc(
            ctx,
            center,
            radius,
            0.0,
            1.0,
            m.thickness,
            theme.colors.dim_text.scale_alpha(0.3),
        );
        draw_arc(
            ctx,
            center,
            radius,
            self.progress * TAU,
            m.sweep,
            m.thickness,
            theme.colors.accent,
        );
    }
}

fn draw_arc(
    ctx: &mut PaintCtx,
    center: Point,
    radius: f64,
    start: f64,
    sweep: f64,
    thickness: f64,
    color: Color,
) {
    let total = sweep.clamp(0.0, 1.0) * TAU;
    let mut prev = point_on(center, radius, start);
    for i in 1..=SEGMENTS {
        let angle = start + total * (i as f64 / SEGMENTS as f64);
        let p = point_on(center, radius, angle);
        ctx.scene.stroke_line(prev, p, thickness, color);
        prev = p;
    }
}

fn point_on(center: Point, radius: f64, angle: f64) -> Point {
    Point::new(
        center.x + radius * angle.cos(),
        center.y + radius * angle.sin(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_wraps() {
        let mut s = Spinner::new();
        s.set_progress(1.25);
        assert!((s.progress() - 0.25).abs() < 1e-9);
        s.set_progress(-0.25);
        assert!((s.progress() - 0.75).abs() < 1e-9);
    }

    #[test]
    fn size_from_metrics() {
        let theme = Theme::light();
        assert_eq!(Spinner::new().size(&theme), theme.metrics.spinner.size);
    }
}
