//! `Pager` — karuzela stron (przewijanie/„swipe” w poziomie z kropkami-wskaźnikami).
//!
//! Widget immediate: trzyma liczbę stron i bieżącą pozycję (ułamkową, w jednostkach stron).
//! Zawartość stron rysuje aplikacja, wyliczając prostokąty przez [`Pager::page_rect`];
//! sam widget rysuje tylko kropki-wskaźniki ([`Pager::paint`]), obsługuje ich klik
//! ([`Pager::indicator_at`]) oraz przeciąganie ([`Pager::begin_drag`] →
//! [`Pager::update_drag`] → [`Pager::end_drag`], które „przyciąga” do najbliższej strony).

use libluna_core::{Animation, Easing, Insets, Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Karuzela stron.
#[derive(Debug, Clone, Default)]
pub struct Pager {
    count: usize,
    /// Bieżąca pozycja: 0..count-1 (ułamkowa podczas przeciągania).
    position: f64,
    /// Pozycja wskaźnika (x) na początku przeciągania.
    drag_from: Option<f64>,
    /// Pozycja karuzeli na początku przeciągania.
    drag_pos: f64,
    /// Animowane „przyciąganie” do strony (po puszczeniu / skoku z kropek).
    snap: Animation,
}

impl Pager {
    pub fn new(count: usize) -> Self {
        Self {
            count,
            ..Self::default()
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn position(&self) -> f64 {
        self.position
    }

    /// Ustawia pozycję (przycinaną do zakresu stron).
    pub fn set_position(&mut self, position: f64) {
        self.position = position.clamp(0.0, self.max_position());
    }

    /// Aktywna strona (najbliższa pozycji).
    pub fn active(&self) -> usize {
        self.position.round().clamp(0.0, self.max_position()) as usize
    }

    /// Ustawia aktywną stronę.
    pub fn set_active(&mut self, index: usize) {
        let max = self.max_position();
        self.position = (index as f64).clamp(0.0, max);
    }

    pub fn next(&mut self) {
        self.set_active(self.active() + 1);
    }

    pub fn prev(&mut self) {
        self.set_active(self.active().saturating_sub(1));
    }

    /// Prostokąt strony `i` przy bieżącej pozycji (strony sąsiadujące są poza obszarem).
    pub fn page_rect(&self, rect: Rect, i: usize) -> Rect {
        let x = rect.x() + (i as f64 - self.position) * rect.width();
        Rect::from_xywh(x, rect.y(), rect.width(), rect.height())
    }

    /// Prostokąty kropek-wskaźników (wyśrodkowane przy dolnej krawędzi).
    pub fn indicator_rects(&self, rect: Rect, theme: &Theme) -> Vec<Rect> {
        let m = theme.metrics.pager;
        let n = self.count;
        if n == 0 {
            return Vec::new();
        }
        let total = n as f64 * m.dot + (n as f64 - 1.0) * m.gap;
        let mut x = rect.center().x - total / 2.0;
        let y = rect.bottom() - m.pad - m.dot;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(Rect::from_xywh(x, y, m.dot, m.dot));
            x += m.dot + m.gap;
        }
        out
    }

    /// Indeks kropki pod punktem `p` (z tolerancją trafienia).
    pub fn indicator_at(&self, rect: Rect, theme: &Theme, p: Point) -> Option<usize> {
        self.indicator_rects(rect, theme).into_iter().position(|r| {
            r.inflate(Insets::splat(theme.metrics.pager.gap / 2.0))
                .contains(p)
        })
    }

    /// Klik w kropkę: ustawia stronę. Zwraca `true`, jeśli trafiono.
    pub fn click_indicator(&mut self, rect: Rect, theme: &Theme, p: Point) -> bool {
        match self.indicator_at(rect, theme, p) {
            Some(i) => {
                self.set_active(i);
                true
            }
            None => false,
        }
    }

    /// Rozpoczyna przeciąganie od pozycji wskaźnika `x`.
    pub fn begin_drag(&mut self, x: f64) {
        self.drag_from = Some(x);
        self.drag_pos = self.position;
    }

    /// Aktualizuje przeciągnięcie w `rect`.
    pub fn update_drag(&mut self, x: f64, rect: Rect) {
        if let Some(from) = self.drag_from {
            if rect.width() > 0.0 {
                self.set_position(self.drag_pos - (x - from) / rect.width());
            }
        }
    }

    /// Kończy przeciąganie, „przyciągając” do najbliższej strony. Zwraca aktywny indeks.
    pub fn end_drag(&mut self) -> usize {
        self.drag_from = None;
        let active = self.active();
        self.set_active(active);
        active
    }

    /// Kończy przeciąganie z **animowanym** przyciąganiem do najbliższej strony.
    pub fn end_drag_animated(&mut self) -> usize {
        self.drag_from = None;
        let active = self.active();
        self.snap.jump_to(self.position);
        self.snap.animate_to(active as f64);
        active
    }

    /// Skok do strony z animacją (np. klik w kropkę).
    pub fn set_active_animated(&mut self, index: usize) {
        let target = (index as f64).clamp(0.0, self.max_position());
        self.snap.jump_to(self.position);
        self.snap.animate_to(target);
    }

    pub fn is_animating(&self) -> bool {
        self.snap.is_animating()
    }

    /// Ustawia krzywą wygładzania przyciągania.
    pub fn set_snap_easing(&mut self, easing: Easing) {
        self.snap.set_easing(easing);
    }

    /// Krok animacji przyciągania; zwraca `true`, gdy trwa dalej.
    pub fn advance(&mut self, dt: f64, duration: f64) -> bool {
        if !self.snap.is_animating() {
            return false;
        }
        let animating = self.snap.advance(dt, duration);
        self.position = self.snap.value().clamp(0.0, self.max_position());
        animating
    }

    /// Rysuje kropki-wskaźniki.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let active = self.active();
        for (i, r) in self.indicator_rects(rect, &theme).iter().enumerate() {
            let color = if i == active {
                ctx.theme.colors.accent
            } else {
                ctx.theme.colors.dim_text.scale_alpha(0.6)
            };
            ctx.scene.fill_rounded_rect(*r, r.width() / 2.0, color);
        }
    }

    fn max_position(&self) -> f64 {
        self.count.saturating_sub(1) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 300.0, 200.0)
    }

    #[test]
    fn pages_are_offset_by_position() {
        let p = Pager::new(3);
        assert_eq!(p.page_rect(rect(), 0), rect());
        let second = p.page_rect(rect(), 1);
        assert!((second.x() - 300.0).abs() < 1e-9);
        let mut p = Pager::new(3);
        p.set_active(1);
        let first = p.page_rect(rect(), 0);
        assert!((first.x() + 300.0).abs() < 1e-9);
    }

    #[test]
    fn position_and_active_clamp() {
        let mut p = Pager::new(3);
        p.set_position(9.0);
        assert_eq!(p.active(), 2);
        p.set_active(0);
        assert_eq!(p.active(), 0);
        p.prev();
        assert_eq!(p.active(), 0); // nie schodzi poniżej zera
        p.next();
        assert_eq!(p.active(), 1);
    }

    #[test]
    fn indicator_hit_selects_page() {
        let theme = Theme::light();
        let mut p = Pager::new(3);
        let dots = p.indicator_rects(rect(), &theme);
        assert_eq!(dots.len(), 3);
        assert!(p.click_indicator(rect(), &theme, dots[2].center()));
        assert_eq!(p.active(), 2);
    }

    #[test]
    fn drag_snaps_to_nearest_page() {
        let mut p = Pager::new(3);
        p.begin_drag(0.0);
        // Przeciągnięcie w lewo o 1.4 szerokości → pozycja 1.4 → snap do 1.
        p.update_drag(-420.0, rect());
        assert!((p.position() - 1.4).abs() < 1e-9);
        assert_eq!(p.end_drag(), 1);
        assert!((p.position() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn animated_snap_settles_on_target() {
        let mut p = Pager::new(3);
        p.begin_drag(0.0);
        p.update_drag(-420.0, rect());
        assert_eq!(p.end_drag_animated(), 1);
        assert!(p.is_animating());
        assert!(p.advance(0.5, 1.0));
        assert!(!p.advance(0.5, 1.0));
        assert!((p.position() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn dot_jump_animates() {
        let mut p = Pager::new(4);
        p.set_active_animated(3);
        assert!(p.is_animating());
        while p.is_animating() {
            p.advance(0.1, 1.0);
        }
        assert_eq!(p.active(), 3);
    }
}
