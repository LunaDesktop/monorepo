//! `Scrollbar` + `Adjustment` — pasek przewijania i model zakresu (GTK4, w duchu libLuny).
//!
//! [`Adjustment`] to mały model zakresu (`lower`..`upper` z oknem `page`), a [`Scrollbar`]
//! go prezentuje: liczy geometrię „kciuka”, obsługuje przeciąganie i rolkę. Działa w obu
//! orientacjach, więc uzupełnia pionowy [`ScrollView`](crate::ScrollView) o wariant poziomy.

use libluna_core::{Insets, Point, Rect, Theme};

use crate::ctx::PaintCtx;
use crate::display::Orientation;

/// Model zakresu przewijania (`lower`..`upper`, widoczne okno `page`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Adjustment {
    value: f64,
    lower: f64,
    upper: f64,
    page: f64,
}

impl Adjustment {
    /// Zakres `lower..upper` z oknem `page`, ustawiony na `lower`.
    pub fn new(lower: f64, upper: f64, page: f64) -> Self {
        Self {
            value: lower,
            lower,
            upper,
            page: page.max(0.0),
        }
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn lower(&self) -> f64 {
        self.lower
    }

    pub fn upper(&self) -> f64 {
        self.upper
    }

    pub fn page_size(&self) -> f64 {
        self.page
    }

    /// Maksymalna wartość (`upper - page`, nie mniej niż `lower`).
    pub fn max_value(&self) -> f64 {
        (self.upper - self.page).max(self.lower)
    }

    /// Czy jest co przewijać.
    pub fn is_scrollable(&self) -> bool {
        self.max_value() > self.lower + 1e-9
    }

    /// Ustawia wartość (przyciętą do zakresu).
    pub fn set_value(&mut self, value: f64) {
        self.value = value.clamp(self.lower, self.max_value());
    }

    /// Zmienia zakres (wartość przycinana do nowego zakresu).
    pub fn set_bounds(&mut self, lower: f64, upper: f64, page: f64) {
        self.lower = lower;
        self.upper = upper;
        self.page = page.max(0.0);
        self.set_value(self.value);
    }

    /// Postęp przewinięcia w zakresie 0..1.
    pub fn fraction(&self) -> f64 {
        let span = self.max_value() - self.lower;
        if span > 0.0 {
            (self.value - self.lower) / span
        } else {
            0.0
        }
    }

    /// Ustawia przewinięcie z postępu 0..1.
    pub fn set_fraction(&mut self, fraction: f64) {
        let span = self.max_value() - self.lower;
        self.set_value(self.lower + span * fraction.clamp(0.0, 1.0));
    }

    /// Przewija o `delta` jednostek (przycinane).
    pub fn scroll_by(&mut self, delta: f64) {
        self.set_value(self.value + delta);
    }
}

/// Pasek przewijania (pionowy lub poziomy).
#[derive(Debug, Clone, Copy, Default)]
pub struct Scrollbar {
    orientation: Orientation,
    adjustment: Adjustment,
    hovered: bool,
    dragging: bool,
    drag_grab: Option<f64>,
}

impl Scrollbar {
    pub fn new(orientation: Orientation) -> Self {
        Self {
            orientation,
            ..Self::default()
        }
    }

    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    pub fn adjustment(&self) -> &Adjustment {
        &self.adjustment
    }

    pub fn adjustment_mut(&mut self) -> &mut Adjustment {
        &mut self.adjustment
    }

    pub fn set_adjustment(&mut self, adjustment: Adjustment) {
        self.adjustment = adjustment;
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    fn track_length(&self, viewport: Rect) -> f64 {
        match self.orientation {
            Orientation::Vertical => viewport.height(),
            Orientation::Horizontal => viewport.width(),
        }
    }

    /// Tor paska wewnątrz `viewport`.
    pub fn track_rect(&self, viewport: Rect, theme: &Theme) -> Rect {
        let m = theme.metrics.scrollview;
        match self.orientation {
            Orientation::Vertical => Rect::from_xywh(
                viewport.right() - m.margin - m.thickness,
                viewport.y(),
                m.thickness,
                viewport.height(),
            ),
            Orientation::Horizontal => Rect::from_xywh(
                viewport.x(),
                viewport.bottom() - m.margin - m.thickness,
                viewport.width(),
                m.thickness,
            ),
        }
    }

    /// Prostokąt „kciuka” (albo `None`, gdy nie ma czego przewijać).
    pub fn thumb_rect(&self, viewport: Rect, theme: &Theme) -> Option<Rect> {
        if !self.adjustment.is_scrollable() {
            return None;
        }
        let m = theme.metrics.scrollview;
        let track = self.track_length(viewport);
        let content = (self.adjustment.upper() - self.adjustment.lower()).max(1e-9);
        let frac = (self.adjustment.page_size() / content).clamp(0.0, 1.0);
        let thumb_len = (track * frac).clamp(m.min_thumb.min(track), track);
        let t = self.adjustment.fraction();
        match self.orientation {
            Orientation::Vertical => {
                let y = viewport.y() + t * (track - thumb_len);
                Some(Rect::from_xywh(
                    viewport.right() - m.margin - m.thickness,
                    y,
                    m.thickness,
                    thumb_len,
                ))
            }
            Orientation::Horizontal => {
                let x = viewport.x() + t * (track - thumb_len);
                Some(Rect::from_xywh(
                    x,
                    viewport.bottom() - m.margin - m.thickness,
                    thumb_len,
                    m.thickness,
                ))
            }
        }
    }

    fn hit_pad(&self, theme: &Theme) -> Insets {
        let t = theme.metrics.scrollview.thickness;
        match self.orientation {
            Orientation::Vertical => Insets::new(0.0, t, 0.0, 4.0),
            Orientation::Horizontal => Insets::new(0.0, 4.0, 0.0, t),
        }
    }

    /// Czy punkt trafia w „kciuk”.
    pub fn thumb_at(&self, viewport: Rect, theme: &Theme, p: Point) -> bool {
        self.thumb_rect(viewport, theme)
            .is_some_and(|t| t.inflate(self.hit_pad(theme)).contains(p))
    }

    /// Przewija o `pixels` pikseli toru; zwraca `true`, gdy da się przewijać.
    pub fn on_scroll(&mut self, pixels: f64, viewport: Rect) -> bool {
        if !self.adjustment.is_scrollable() {
            return false;
        }
        let track = self.track_length(viewport);
        if track <= 0.0 {
            return false;
        }
        let frac = pixels / track;
        self.adjustment
            .set_fraction(self.adjustment.fraction() + frac);
        true
    }

    /// Rozpoczyna przeciąganie „kciuka” (albo skok w tor).
    pub fn begin_drag(&mut self, p: Point, viewport: Rect, theme: &Theme) -> bool {
        if !self.adjustment.is_scrollable() {
            return false;
        }
        if let Some(thumb) = self.thumb_rect(viewport, theme) {
            let (along, thumb_start, thumb_len) = match self.orientation {
                Orientation::Vertical => (p.y, thumb.y(), thumb.height()),
                Orientation::Horizontal => (p.x, thumb.x(), thumb.width()),
            };
            if thumb.inflate(self.hit_pad(theme)).contains(p) {
                self.drag_grab = Some(along - thumb_start);
                self.dragging = true;
                return true;
            }
            // Klik w tor poza kciukiem: wyśrodkuj kciuk i zacznij przeciąganie.
            if self.track_rect(viewport, theme).contains(p) {
                let t = (along - thumb_start - thumb_len / 2.0).max(0.0);
                let track = self.track_length(viewport);
                let denom = (track - thumb_len).max(1e-9);
                self.adjustment.set_fraction(t / denom);
                self.drag_grab = Some(thumb_len / 2.0);
                self.dragging = true;
                return true;
            }
        }
        false
    }

    /// Aktualizuje przeciąganie „kciuka”.
    pub fn drag_to(&mut self, p: Point, viewport: Rect, theme: &Theme) {
        if !self.dragging {
            return;
        }
        let Some(thumb) = self.thumb_rect(viewport, theme) else {
            return;
        };
        let track = self.track_length(viewport);
        let thumb_len = match self.orientation {
            Orientation::Vertical => thumb.height(),
            Orientation::Horizontal => thumb.width(),
        };
        let (along, start) = match self.orientation {
            Orientation::Vertical => (p.y, viewport.y()),
            Orientation::Horizontal => (p.x, viewport.x()),
        };
        let grab = self.drag_grab.unwrap_or(0.0);
        let denom = (track - thumb_len).max(1e-9);
        self.adjustment
            .set_fraction(((along - grab - start) / denom).clamp(0.0, 1.0));
    }

    /// Kończy przeciąganie.
    pub fn end_drag(&mut self) {
        self.dragging = false;
        self.drag_grab = None;
    }

    /// Rysuje „kciuk” (jeśli jest co przewijać).
    pub fn paint(&self, ctx: &mut PaintCtx, viewport: Rect) {
        let theme = *ctx.theme;
        if let Some(t) = self.thumb_rect(viewport, &theme) {
            let alpha = if self.dragging {
                0.7
            } else if self.hovered {
                0.55
            } else {
                0.45
            };
            let radius = match self.orientation {
                Orientation::Vertical => t.width() / 2.0,
                Orientation::Horizontal => t.height() / 2.0,
            };
            ctx.scene
                .fill_rounded_rect(t, radius, theme.colors.dim_text.scale_alpha(alpha));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Rect {
        Rect::from_xywh(0.0, 0.0, 200.0, 100.0)
    }

    #[test]
    fn adjustment_fraction_and_clamping() {
        let mut a = Adjustment::new(0.0, 100.0, 25.0);
        assert_eq!(a.max_value(), 75.0);
        a.set_value(999.0);
        assert_eq!(a.value(), 75.0);
        assert_eq!(a.fraction(), 1.0);
        a.set_fraction(0.5);
        assert_eq!(a.value(), 37.5);
        a.set_value(-10.0);
        assert_eq!(a.value(), 0.0);
    }

    #[test]
    fn not_scrollable_when_page_covers_range() {
        let a = Adjustment::new(0.0, 100.0, 120.0);
        assert!(!a.is_scrollable());
        let theme = Theme::light();
        let mut s = Scrollbar::new(Orientation::Vertical);
        s.set_adjustment(a);
        assert!(s.thumb_rect(viewport(), &theme).is_none());
        assert!(!s.on_scroll(50.0, viewport()));
    }

    #[test]
    fn thumb_length_tracks_page_size() {
        let theme = Theme::light();
        let mut s = Scrollbar::new(Orientation::Vertical);
        s.set_adjustment(Adjustment::new(0.0, 200.0, 100.0));
        let t = s.thumb_rect(viewport(), &theme).unwrap();
        assert!((t.height() - 50.0).abs() < 1e-9);
        // Na końcu kciuk dojeżdża do dołu toru.
        s.adjustment_mut().set_fraction(1.0);
        let t2 = s.thumb_rect(viewport(), &theme).unwrap();
        assert!((t2.bottom() - viewport().bottom()).abs() < 1e-9);
    }

    #[test]
    fn horizontal_thumb_moves_along_x() {
        let theme = Theme::light();
        let mut s = Scrollbar::new(Orientation::Horizontal);
        s.set_adjustment(Adjustment::new(0.0, 200.0, 100.0));
        let t = s.thumb_rect(viewport(), &theme).unwrap();
        assert!(t.x() >= viewport().x());
        s.adjustment_mut().set_fraction(1.0);
        let t2 = s.thumb_rect(viewport(), &theme).unwrap();
        assert!((t2.right() - viewport().right()).abs() < 1e-9);
    }

    #[test]
    fn drag_moves_to_end() {
        let theme = Theme::light();
        let mut s = Scrollbar::new(Orientation::Vertical);
        s.set_adjustment(Adjustment::new(0.0, 200.0, 100.0));
        let t = s.thumb_rect(viewport(), &theme).unwrap();
        assert!(s.begin_drag(t.center(), viewport(), &theme));
        assert!(s.is_dragging());
        s.drag_to(Point::new(t.center().x, 500.0), viewport(), &theme);
        assert_eq!(s.adjustment().fraction(), 1.0);
        s.end_drag();
        assert!(!s.is_dragging());
    }

    #[test]
    fn scroll_pixels_change_fraction() {
        let mut s = Scrollbar::new(Orientation::Vertical);
        s.set_adjustment(Adjustment::new(0.0, 200.0, 100.0));
        assert!(s.on_scroll(25.0, viewport())); // 25/100 toru → +0.25
        assert!((s.adjustment().fraction() - 0.25).abs() < 1e-9);
    }
}
