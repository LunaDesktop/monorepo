//! `InfoBar` — pasek informacyjny (odpowiednik GTK4 `GtkInfoBar`).
//!
//! W odróżnieniu od libadwaitowej karty [`Banner`](crate::Banner), `InfoBar` ma cztery
//! rodzaje (info/pytanie/ostrzeżenie/błąd), opcjonalny przycisk zamknięcia oraz dowolną
//! liczbę przycisków akcji — jak w GTK4.

use libluna_core::style::weight;
use libluna_core::{Color, Insets, Point, Rect, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;

/// Rodzaj paska (kolor ikony i paska po lewej).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InfoBarKind {
    #[default]
    Info,
    Question,
    Warning,
    Error,
}

/// Pasek informacyjny.
#[derive(Debug, Clone, Default)]
pub struct InfoBar {
    kind: InfoBarKind,
    message: String,
    actions: Vec<String>,
    show_close: bool,
}

/// Rozmieszczenie elementów paska.
#[derive(Debug, Clone, PartialEq)]
pub struct InfoBarRects {
    pub icon: Rect,
    pub message: Rect,
    pub actions: Vec<Rect>,
    pub close: Option<Rect>,
}

impl InfoBar {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            kind: InfoBarKind::Info,
            message: message.into(),
            actions: Vec::new(),
            show_close: true,
        }
    }

    pub fn kind(mut self, kind: InfoBarKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn action(mut self, label: impl Into<String>) -> Self {
        self.actions.push(label.into());
        self
    }

    pub fn show_close(mut self, show: bool) -> Self {
        self.show_close = show;
        self
    }

    /// Kolor rodzaju.
    pub fn color(&self, theme: &Theme) -> Color {
        match self.kind {
            InfoBarKind::Info => theme.colors.accent,
            InfoBarKind::Question => theme.colors.success,
            InfoBarKind::Warning => theme.colors.warning,
            InfoBarKind::Error => theme.colors.error,
        }
    }

    /// Symbol rodzaju rysowany fontem interfejsu.
    pub fn glyph(&self) -> &'static str {
        match self.kind {
            InfoBarKind::Info => "●",
            InfoBarKind::Question => "?",
            InfoBarKind::Warning => "▲",
            InfoBarKind::Error => "✗",
        }
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.banner.min_height
    }

    pub fn rects(&self, area: Rect, theme: &Theme, text: &TextEngine) -> InfoBarRects {
        let m = theme.metrics.banner;
        let body = theme.typography.body.size;
        let inner = area.deflate(Insets::splat(m.pad));
        let cy = inner.center().y;
        let icon = Rect::from_xywh(inner.x(), cy - m.icon / 2.0, m.icon, m.icon);

        // Zamknięcie po prawej.
        let mut right = inner.right();
        let close = if self.show_close {
            let r = Rect::from_xywh(
                right - m.button_h,
                cy - m.button_h / 2.0,
                m.button_h,
                m.button_h,
            );
            right = r.x() - m.gap;
            Some(r)
        } else {
            None
        };

        // Akcje wyrównane do prawej (przed zamknięciem).
        let mut actions: Vec<Rect> = Vec::new();
        for label in self.actions.iter().rev() {
            let w = 2.0 * theme.spacing.md + text.measure(label, body, weight::MEDIUM).width;
            actions.push(Rect::from_xywh(
                right - w,
                cy - m.button_h / 2.0,
                w,
                m.button_h,
            ));
            right -= w + m.gap;
        }
        actions.reverse();

        let text_left = icon.right() + m.gap;
        let width = (right - text_left).max(0.0);
        let message = Rect::from_xywh(text_left, cy - body / 2.0 - 2.0, width, body + 4.0);

        InfoBarRects {
            icon,
            message,
            actions,
            close,
        }
    }

    /// Indeks akcji pod punktem `p`.
    pub fn action_at(
        &self,
        area: Rect,
        theme: &Theme,
        text: &TextEngine,
        p: Point,
    ) -> Option<usize> {
        self.rects(area, theme, text)
            .actions
            .iter()
            .position(|r| r.contains(p))
    }

    /// Czy kliknięto przycisk zamknięcia.
    pub fn close_at(&self, area: Rect, theme: &Theme, text: &TextEngine, p: Point) -> bool {
        self.rects(area, theme, text)
            .close
            .map(|r| r.contains(p))
            .unwrap_or(false)
    }

    pub fn paint(&self, ctx: &mut PaintCtx, area: Rect) {
        let theme = *ctx.theme;
        let accent = self.color(&theme);
        let r = self.rects(area, &theme, ctx.text);

        ctx.scene
            .fill_rounded_rect(area, theme.radius.medium, accent.scale_alpha(0.14));
        ctx.scene
            .stroke_rounded_rect(area, theme.radius.medium, 1.0, accent.scale_alpha(0.35));

        // Ikona rodzaju.
        ctx.label_center(
            r.icon.center().x,
            r.icon.center().y,
            self.glyph(),
            theme.typography.body.size,
            weight::BOLD,
            accent,
        );

        ctx.label_left(
            r.message.x(),
            r.message.center().y,
            &self.message,
            theme.typography.body.size,
            weight::MEDIUM,
            theme.colors.text,
        );

        for (label, rect) in self.actions.iter().zip(&r.actions) {
            ctx.scene
                .fill_rounded_rect(*rect, theme.radius.small, accent);
            ctx.label_center(
                rect.center().x,
                rect.center().y,
                label,
                theme.typography.body.size,
                weight::MEDIUM,
                theme.colors.accent_fg,
            );
        }

        if let Some(close) = r.close {
            ctx.scene
                .fill_rounded_rect(close, theme.radius.small, theme.colors.shade);
            ctx.label_center(
                close.center().x,
                close.center().y,
                "✗",
                theme.typography.caption.size,
                weight::MEDIUM,
                theme.colors.text,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_render::{Scene, TextEngine};

    fn area() -> Rect {
        Rect::from_xywh(0.0, 0.0, 600.0, 56.0)
    }

    #[test]
    fn layout_icon_left_actions_and_close_right() {
        let theme = Theme::light();
        let text = TextEngine::none();
        let bar = InfoBar::new("Zapisano").action("Cofnij");
        let r = bar.rects(area(), &theme, &text);
        assert!(r.icon.x() < r.message.x());
        assert!(r.close.is_some());
        assert_eq!(r.actions.len(), 1);
        assert!(r.actions[0].right() <= r.close.unwrap().x());
        assert!(r.message.right() <= r.actions[0].x());
    }

    #[test]
    fn hit_tests_for_action_and_close() {
        let theme = Theme::light();
        let text = TextEngine::none();
        let bar = InfoBar::new("Pytanie?")
            .kind(InfoBarKind::Question)
            .action("Tak");
        let r = bar.rects(area(), &theme, &text);
        assert_eq!(
            bar.action_at(area(), &theme, &text, r.actions[0].center()),
            Some(0)
        );
        assert!(bar.close_at(area(), &theme, &text, r.close.unwrap().center()));
        assert!(!bar.close_at(area(), &theme, &text, Point::new(2.0, 2.0)));
    }

    #[test]
    fn close_can_be_hidden() {
        let theme = Theme::light();
        let text = TextEngine::none();
        let bar = InfoBar::new("Info").show_close(false);
        assert!(bar.rects(area(), &theme, &text).close.is_none());
    }

    #[test]
    fn colors_per_kind() {
        let theme = Theme::light();
        assert_eq!(InfoBar::new("x").color(&theme), theme.colors.accent);
        assert_eq!(
            InfoBar::new("x").kind(InfoBarKind::Error).color(&theme),
            theme.colors.error
        );
    }

    #[test]
    fn paints_without_window() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut scene = Scene::new();
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        InfoBar::new("Zapisano zmiany")
            .action("Cofnij")
            .paint(&mut ctx, area());
        assert!(scene.len() > 5);
    }
}
