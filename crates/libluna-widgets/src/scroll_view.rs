//! `ScrollView` — przewijanie zawartości (kołysko/„kciuk” + obsługa rolki i przeciągania).
//!
//! Widget immediate: trzyma przesunięcie i wysokość zawartości. Aplikacja rysuje swoją treść
//! z uwzględnieniem [`ScrollView::offset`], przycina do obszaru, a widget rysuje pasek
//! ([`ScrollView::paint_scrollbar`]) i obsługuje rolkę ([`ScrollView::on_scroll`]) oraz
//! przeciąganie „kciuka” ([`ScrollView::begin_drag`] / [`ScrollView::drag_to`] / [`ScrollView::end_drag`]).

use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Przewijanie pionowe.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScrollView {
    offset: f64,
    content_height: f64,
    /// Przesunięcie wskaźnika (y) względem początku „kciuka” na starcie przeciągania.
    drag_grab: Option<f64>,
}

impl ScrollView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn content_height(&self) -> f64 {
        self.content_height
    }

    pub fn set_content_height(&mut self, height: f64) {
        self.content_height = height.max(0.0);
    }

    pub fn offset(&self) -> f64 {
        self.offset
    }

    /// Maksymalne przesunięcie (0, gdy wszystko się mieści).
    pub fn max_offset(&self, viewport: Rect) -> f64 {
        (self.content_height - viewport.height()).max(0.0)
    }

    pub fn is_scrollable(&self, viewport: Rect) -> bool {
        self.max_offset(viewport) > 0.5
    }

    /// Ustawia przesunięcie (przycięte do zakresu).
    pub fn set_offset(&mut self, offset: f64, viewport: Rect) {
        self.offset = offset.clamp(0.0, self.max_offset(viewport));
    }

    /// Przewija o `dy`; zwraca `true`, jeśli zawartość da się przewijać.
    pub fn on_scroll(&mut self, dy: f64, viewport: Rect) -> bool {
        if !self.is_scrollable(viewport) {
            return false;
        }
        self.set_offset(self.offset + dy, viewport);
        true
    }

    /// Prostokąt „kciuka” paska przewijania (albo `None`, gdy nie ma czego przewijać).
    pub fn thumb_rect(&self, viewport: Rect, theme: &Theme) -> Option<Rect> {
        if !self.is_scrollable(viewport) {
            return None;
        }
        let m = theme.metrics.scrollview;
        let track_h = viewport.height();
        let frac = (viewport.height() / self.content_height).clamp(0.0, 1.0);
        let thumb_h = (track_h * frac).clamp(m.min_thumb, track_h);
        let max = self.max_offset(viewport);
        let t = if max > 0.0 { self.offset / max } else { 0.0 };
        let y = viewport.y() + t * (track_h - thumb_h);
        Some(Rect::from_xywh(
            viewport.right() - m.margin - m.thickness,
            y,
            m.thickness,
            thumb_h,
        ))
    }

    /// Czy punkt trafia w „kciuk” (z tolerancją grubości).
    pub fn thumb_at(&self, viewport: Rect, theme: &Theme, p: Point) -> bool {
        self.thumb_rect(viewport, theme)
            .is_some_and(|t| t.inflate(luna_insets(theme)).contains(p))
    }

    /// Rozpoczyna przeciąganie „kciuka” (jeśli punkt w niego trafia).
    pub fn begin_drag(&mut self, p: Point, viewport: Rect, theme: &Theme) -> bool {
        match self.thumb_rect(viewport, theme) {
            Some(t) if t.contains(p) => {
                self.drag_grab = Some(p.y - t.y());
                true
            }
            _ => false,
        }
    }

    /// Aktualizuje przeciąganie „kciuka”.
    pub fn drag_to(&mut self, p: Point, viewport: Rect, theme: &Theme) {
        let Some(grab) = self.drag_grab else { return };
        let Some(thumb) = self.thumb_rect(viewport, theme) else {
            return;
        };
        let range = viewport.height() - thumb.height();
        if range > 0.0 {
            let t = ((p.y - grab - viewport.y()) / range).clamp(0.0, 1.0);
            self.offset = t * self.max_offset(viewport);
        }
    }

    /// Kończy przeciąganie.
    pub fn end_drag(&mut self) {
        self.drag_grab = None;
    }

    /// Rysuje pasek przewijania (jeśli jest co przewijać).
    pub fn paint_scrollbar(&self, ctx: &mut PaintCtx, viewport: Rect) {
        let theme = *ctx.theme;
        if let Some(t) = self.thumb_rect(viewport, &theme) {
            ctx.scene.fill_rounded_rect(
                t,
                t.width() / 2.0,
                theme.colors.dim_text.scale_alpha(0.45),
            );
        }
    }
}

fn luna_insets(theme: &Theme) -> libluna_core::Insets {
    libluna_core::Insets::new(0.0, theme.metrics.scrollview.thickness, 0.0, 4.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Rect {
        Rect::from_xywh(0.0, 0.0, 200.0, 100.0)
    }

    #[test]
    fn not_scrollable_when_content_fits() {
        let mut s = ScrollView::new();
        s.set_content_height(80.0);
        assert!(!s.is_scrollable(viewport()));
        assert!(!s.on_scroll(50.0, viewport()));
        assert!(s.thumb_rect(viewport(), &Theme::light()).is_none());
    }

    #[test]
    fn offset_is_clamped() {
        let mut s = ScrollView::new();
        s.set_content_height(300.0);
        s.set_offset(999.0, viewport());
        assert_eq!(s.offset(), 200.0);
        s.set_offset(-5.0, viewport());
        assert_eq!(s.offset(), 0.0);
    }

    #[test]
    fn thumb_shrinks_with_content() {
        let theme = Theme::light();
        let mut s = ScrollView::new();
        s.set_content_height(200.0); // viewport 100 → kciuk ~ połowa toru
        let t = s.thumb_rect(viewport(), &theme).unwrap();
        assert!((t.height() - 50.0).abs() < 1e-9);
        // Na dole kciuk dojeżdża do końca toru.
        s.set_offset(100.0, viewport());
        let t2 = s.thumb_rect(viewport(), &theme).unwrap();
        assert!((t2.bottom() - viewport().bottom()).abs() < 1e-9);
    }

    #[test]
    fn drag_moves_thumb_to_bottom() {
        let theme = Theme::light();
        let mut s = ScrollView::new();
        s.set_content_height(200.0);
        let t = s.thumb_rect(viewport(), &theme).unwrap();
        assert!(s.begin_drag(t.center(), viewport(), &theme));
        s.drag_to(Point::new(t.center().x, 500.0), viewport(), &theme);
        assert_eq!(s.offset(), s.max_offset(viewport()));
        s.end_drag();
    }
}
