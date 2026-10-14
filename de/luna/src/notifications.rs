//! Powiadomienia (toasty) w powłoce Luny — kolejka, animacja i rysowanie.
//!
//! To warstwa prezentacji: przyjmuje gotowe powiadomienie, animuje wejście, po
//! upływie czasu samoistnie je zamyka. Podłączenie usługi D-Bus
//! `org.freedesktop.Notifications` to kolejny krok (patrz roadmapa).

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};
use libluna_render::TextEngine;
use libluna_widgets::{paint_icon, IconName, PaintCtx};

/// Czas animacji wejścia/wyjścia (s).
const SLIDE: f64 = 0.22;
/// Szerokość pojedynczego powiadomienia.
pub const WIDTH: f64 = 340.0;
/// Wysokość pojedynczego powiadomienia.
pub const HEIGHT: f64 = 96.0;
/// Odstęp pionowy między powiadomieniami.
const GAP: f64 = 10.0;
/// Wysokość przycisku akcji.
const ACTION_H: f64 = 24.0;
/// Margines przycisków akcji od krawędzi.
const ACTION_PAD: f64 = 12.0;
/// Odstęp między przyciskami akcji.
const ACTION_GAP: f64 = 8.0;

/// Pojedyncze powiadomienie.
#[derive(Debug, Clone)]
pub struct Notification {
    /// Nazwa aplikacji (np. „Pliki”).
    pub app: String,
    pub title: String,
    pub body: String,
    /// Ikona z pakietu Luny.
    pub icon: IconName,
    /// Czas życia (s); 0 = nie zamykaj automatycznie.
    pub timeout: f64,
    /// Identyfikator z magistrali D‑Bus (0 = powiadomienie lokalne).
    pub id: u32,
    /// Klucze i etykiety akcji (`ActionInvoked`); puste, gdy brak akcji.
    pub actions: Vec<(String, String)>,
}

impl Notification {
    pub fn new(app: impl Into<String>, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            app: app.into(),
            title: title.into(),
            body: body.into(),
            icon: IconName::Info,
            timeout: 5.0,
            id: 0,
            actions: Vec::new(),
        }
    }

    pub fn with_icon(mut self, icon: IconName) -> Self {
        self.icon = icon;
        self
    }

    /// Ustawia identyfikator z magistrali (do wysłania `NotificationClosed`).
    pub fn with_id(mut self, id: u32) -> Self {
        self.id = id;
        self
    }

    /// Ustawia akcje jako pary `(klucz, etykieta)` (do wysłania `ActionInvoked`).
    pub fn with_actions(mut self, actions: Vec<(String, String)>) -> Self {
        self.actions = actions;
        self
    }
}

/// Kolejka powiadomień z animacją.
#[derive(Default)]
pub struct Notifications {
    items: Vec<(Notification, f64)>,
}

impl Notifications {
    pub fn new() -> Self {
        Self::default()
    }

    /// Czy stos powiadomień nie jest pusty (potrzebna animacja/przerysowanie).
    pub fn visible(&self) -> bool {
        !self.items.is_empty()
    }

    /// Dodaje powiadomienie na wierzch stosu.
    pub fn push(&mut self, n: Notification) {
        self.items.push((n, 0.0));
    }

    /// Przesuwa czas i usuwa wygasłe powiadomienia.
    pub fn advance(&mut self, dt: f64) {
        for (_, age) in &mut self.items {
            *age += dt;
        }
        self.items
            .retain(|(n, age)| n.timeout <= 0.0 || *age < n.timeout);
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Indeks powiadomienia pod punktem `p` (od wierzchu stosu).
    pub fn hit(&self, area: Rect, p: Point) -> Option<usize> {
        (0..self.items.len())
            .rev()
            .find(|&i| Self::rect_in(area, i).contains(p))
    }

    /// Powiadomienie o indeksie `i`.
    pub fn get(&self, i: usize) -> Option<&Notification> {
        self.items.get(i).map(|(n, _)| n)
    }

    /// Usuwa powiadomienie o indeksie `i` (np. po kliknięciu).
    pub fn remove(&mut self, i: usize) {
        if i < self.items.len() {
            self.items.remove(i);
        }
    }

    /// Prostokąt `i`-tego powiadomienia w kolumnie zakotwiczonej w prawym-górnym
    /// narożniku obszaru `area`.
    pub fn rect_in(area: Rect, i: usize) -> Rect {
        Rect::from_xywh(
            area.right() - WIDTH,
            area.y() + i as f64 * (HEIGHT + GAP),
            WIDTH,
            HEIGHT,
        )
    }

    /// Prostokąty przycisków akcji toasta `i` (od prawej do lewej, u dołu).
    pub fn action_rects(
        &self,
        area: Rect,
        i: usize,
        theme: &Theme,
        text: &TextEngine,
    ) -> Vec<Rect> {
        let Some((n, _)) = self.items.get(i) else {
            return Vec::new();
        };
        if n.actions.is_empty() {
            return Vec::new();
        }
        let rect = Self::rect_in(area, i);
        let cap = theme.typography.caption.size;
        let y = rect.bottom() - ACTION_PAD - ACTION_H;
        let mut right = rect.right() - ACTION_PAD;
        let mut out = vec![Rect::ZERO; n.actions.len()];
        for (k, (_, label)) in n.actions.iter().enumerate().rev() {
            let w = text.measure(label, cap, weight::MEDIUM).width + 2.0 * theme.spacing.sm;
            let r = Rect::from_xywh(right - w, y, w, ACTION_H);
            out[k] = r;
            right = r.x() - ACTION_GAP;
        }
        out
    }

    /// Trafienie przycisku akcji: `(indeks toasta, indeks akcji)`.
    pub fn action_at(
        &self,
        area: Rect,
        p: Point,
        theme: &Theme,
        text: &TextEngine,
    ) -> Option<(usize, usize)> {
        for i in (0..self.items.len()).rev() {
            if let Some(k) = self
                .action_rects(area, i, theme, text)
                .iter()
                .position(|r| r.contains(p))
            {
                return Some((i, k));
            }
        }
        None
    }

    /// Rysuje stos powiadomień w obszarze `area` (od góry, do prawej).
    pub fn paint(&self, ctx: &mut PaintCtx, area: Rect) {
        let t = ctx.theme;
        for (i, (n, age)) in self.items.iter().enumerate().rev() {
            let mut rect = Self::rect_in(area, i);
            // Wjazd z prawej + fade.
            let p = (*age / SLIDE).clamp(0.0, 1.0);
            let slide = (1.0 - p) * 28.0;
            rect = Rect::from_xywh(rect.x() + slide, rect.y(), rect.width(), rect.height());
            let a = p as f32;

            ctx.scene
                .fill_rounded_rect(rect, t.radius.medium, t.colors.card_bg.scale_alpha(a));
            ctx.scene.stroke_rounded_rect(
                rect,
                t.radius.medium,
                1.0,
                t.colors.border.scale_alpha(a),
            );

            let pad = t.spacing.md;
            let icon = Rect::from_xywh(rect.x() + pad, rect.y() + pad, 32.0, 32.0);
            ctx.scene.fill_rounded_rect(
                icon,
                t.radius.small,
                t.colors.accent.scale_alpha(0.18 * a),
            );
            let inner = Rect::from_xywh(icon.x() + 7.0, icon.y() + 7.0, 18.0, 18.0);
            paint_icon(ctx, n.icon, inner, t.colors.accent.scale_alpha(a));

            let tx = icon.right() + pad;
            ctx.label_left(
                tx,
                rect.y() + 26.0,
                &n.title,
                t.typography.body.size,
                weight::MEDIUM,
                t.colors.text.scale_alpha(a),
            );
            ctx.label_left(
                tx,
                rect.y() + 48.0,
                &n.body,
                t.typography.caption.size,
                weight::NORMAL,
                t.colors.dim_text.scale_alpha(a),
            );
            ctx.label_right(
                rect.right() - pad,
                rect.y() + 26.0,
                &n.app,
                t.typography.caption.size,
                weight::NORMAL,
                t.colors.dim_text.scale_alpha(a),
            );

            // Przyciski akcji (np. „Otwórz”, „Odpisz”).
            if !n.actions.is_empty() {
                let mut acts = self.action_rects(area, i, t, ctx.text);
                for ((_, label), r) in n.actions.iter().zip(acts.iter_mut()) {
                    *r = Rect::from_xywh(r.x() + slide, r.y(), r.width(), r.height());
                    ctx.scene.fill_rounded_rect(
                        *r,
                        r.height() / 2.0,
                        t.colors.accent.scale_alpha(a),
                    );
                    ctx.label_center(
                        r.center().x,
                        r.center().y,
                        label,
                        t.typography.caption.size,
                        weight::MEDIUM,
                        t.colors.accent_fg.scale_alpha(a),
                    );
                }
            }

            // Pasek czasu (kurczy się) — tylko gdy nie ma akcji.
            if n.timeout > 0.0 && n.actions.is_empty() {
                let frac = (1.0 - (*age / n.timeout)).clamp(0.0, 1.0);
                let track = Rect::from_xywh(
                    rect.x() + pad,
                    rect.bottom() - 8.0,
                    rect.width() - 2.0 * pad,
                    3.0,
                );
                ctx.scene
                    .fill_rounded_rect(track, 1.5, t.colors.shade.scale_alpha(a));
                let fill =
                    Rect::from_xywh(track.x(), track.y(), track.width() * frac, track.height());
                ctx.scene
                    .fill_rounded_rect(fill, 1.5, t.colors.accent.scale_alpha(a));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_core::Theme;
    use libluna_render::{Scene, TextEngine};

    #[test]
    fn expires_after_timeout() {
        let mut n = Notifications::new();
        n.push(Notification::new("Pliki", "Kopiowanie", "12 plików"));
        assert_eq!(n.len(), 1);
        n.advance(3.0);
        assert_eq!(n.len(), 1);
        n.advance(3.0);
        assert!(n.is_empty());
    }

    #[test]
    fn zero_timeout_is_sticky() {
        let mut n = Notifications::new();
        let mut item = Notification::new("System", "Aktualizacja", "W toku");
        item.timeout = 0.0;
        n.push(item);
        n.advance(100.0);
        assert_eq!(n.len(), 1);
    }

    #[test]
    fn rects_stack_downward_from_top_right() {
        let area = Rect::from_xywh(0.0, 40.0, 1280.0, 200.0);
        let a = Notifications::rect_in(area, 0);
        let b = Notifications::rect_in(area, 1);
        assert_eq!(a.right(), area.right());
        assert!(b.y() > a.y());
    }

    #[test]
    fn hit_and_remove_by_point() {
        let mut n = Notifications::new();
        n.push(
            Notification::new("A", "t", "b")
                .with_id(7)
                .with_actions(vec![("default".to_owned(), "Otwórz".to_owned())]),
        );
        n.advance(0.3);
        let area = Rect::from_xywh(0.0, 40.0, 1280.0, 200.0);
        let rect = Notifications::rect_in(area, 0);
        assert_eq!(n.hit(area, rect.center()), Some(0));
        assert_eq!(n.hit(area, Point::new(-10.0, -10.0)), None);
        let n0 = n.get(0).unwrap();
        assert_eq!(n0.id, 7);
        assert_eq!(
            n0.actions,
            vec![("default".to_owned(), "Otwórz".to_owned())]
        );
        n.remove(0);
        assert!(n.is_empty());
    }

    #[test]
    fn action_buttons_hit_test() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let area = Rect::from_xywh(0.0, 40.0, 1280.0, 300.0);
        let mut n = Notifications::new();
        n.push(Notification::new("A", "t", "b").with_actions(vec![
            ("open".to_owned(), "Otwórz".to_owned()),
            ("dismiss".to_owned(), "Zamknij".to_owned()),
        ]));
        n.advance(0.3);
        let rects = n.action_rects(area, 0, &theme, &text);
        assert_eq!(rects.len(), 2);
        assert_eq!(
            n.action_at(area, rects[0].center(), &theme, &text),
            Some((0, 0))
        );
        assert_eq!(
            n.action_at(area, rects[1].center(), &theme, &text),
            Some((0, 1))
        );
        // Środek toasta (poza przyciskami) nie jest trafieniem akcji.
        assert_eq!(
            n.action_at(
                area,
                Notifications::rect_in(area, 0).center(),
                &theme,
                &text
            ),
            None
        );
    }

    #[test]
    fn paints_without_window() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut scene = Scene::new();
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        let mut n = Notifications::new();
        n.push(Notification::new("Pliki", "Kopiowanie", "12 plików").with_icon(IconName::Files));
        n.advance(0.3);
        n.paint(&mut ctx, Rect::from_xywh(0.0, 40.0, 1280.0, 200.0));
        assert!(scene.len() > 5);
    }
}
