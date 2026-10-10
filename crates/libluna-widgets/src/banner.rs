//! `Banner` — pasek komunikatu (info/ostrzeżenie/błąd) z tytułem, opisem i akcjami.
//!
//! Widget immediate: [`Banner::rects`] liczy rozmieszczenie (ikona, tytuł, opis, przyciski),
//! [`Banner::action_at`] obsługuje klik w akcje, a [`Banner::paint`] rysuje pasek.

use libluna_core::style::weight;
use libluna_core::{Color, Insets, Point, Rect, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;

/// Rodzaj komunikatu (wpływa na kolor akcentowy paska).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BannerKind {
    #[default]
    Info,
    Warning,
    Error,
}

/// Pasek komunikatu.
#[derive(Debug, Clone, Default)]
pub struct Banner {
    kind: BannerKind,
    title: String,
    description: Option<String>,
    actions: Vec<String>,
}

/// Rozmieszczenie elementów paska.
#[derive(Debug, Clone, PartialEq)]
pub struct BannerRects {
    pub icon: Rect,
    pub title: Rect,
    pub description: Option<Rect>,
    pub actions: Vec<Rect>,
}

impl Banner {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            kind: BannerKind::Info,
            title: title.into(),
            description: None,
            actions: Vec::new(),
        }
    }

    pub fn kind(mut self, kind: BannerKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Dodaje przycisk akcji (od lewej do prawej).
    pub fn action(mut self, label: impl Into<String>) -> Self {
        self.actions.push(label.into());
        self
    }

    /// Wysokość paska.
    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.banner.min_height
    }

    /// Kolor akcentowy paska (wg rodzaju).
    pub fn color(&self, theme: &Theme) -> Color {
        match self.kind {
            BannerKind::Info => theme.colors.accent,
            BannerKind::Warning => theme.colors.warning,
            BannerKind::Error => theme.colors.error,
        }
    }

    pub fn rects(&self, area: Rect, theme: &Theme, text: &TextEngine) -> BannerRects {
        let m = theme.metrics.banner;
        let body = theme.typography.body;
        let inner = area.deflate(Insets::splat(m.pad));
        let cy = inner.center().y;

        let icon = Rect::from_xywh(inner.x(), cy - m.icon / 2.0, m.icon, m.icon);

        // Akcje wyrównane do prawej.
        let mut actions: Vec<Rect> = Vec::new();
        let mut x = inner.right();
        for label in self.actions.iter().rev() {
            let w = 2.0 * theme.spacing.md + text.measure(label, body.size, weight::MEDIUM).width;
            actions.push(Rect::from_xywh(x - w, cy - m.button_h / 2.0, w, m.button_h));
            x -= w + m.gap;
        }
        actions.reverse();
        let actions_left = actions.first().map(|r| r.x()).unwrap_or(inner.right());

        let text_left = icon.right() + m.gap;
        let text_right = (actions_left - m.gap).max(text_left);
        let width = (text_right - text_left).max(0.0);

        let (title, description) = match self.description.is_some() {
            true => {
                let block = body.line_height * 2.0;
                let top = cy - block / 2.0;
                (
                    Rect::from_xywh(text_left, top, width, body.line_height),
                    Some(Rect::from_xywh(
                        text_left,
                        top + body.line_height,
                        width,
                        body.line_height,
                    )),
                )
            }
            false => (
                Rect::from_xywh(
                    text_left,
                    cy - body.line_height / 2.0,
                    width,
                    body.line_height,
                ),
                None,
            ),
        };

        BannerRects {
            icon,
            title,
            description,
            actions,
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

    pub fn paint(&self, ctx: &mut PaintCtx, area: Rect) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let accent = self.color(&theme);
        let r = self.rects(area, &theme, ctx.text);

        // Tło (delikatny odcień) + pasek po lewej w kolorze rodzaju.
        ctx.scene
            .fill_rounded_rect(area, theme.radius.medium, accent.scale_alpha(0.16));
        let bar = Rect::from_xywh(area.x(), area.y(), 4.0, area.height());
        ctx.scene
            .fill_rounded_rect(bar, theme.radius.medium, accent);

        // Ikona (kropka w kolorze rodzaju).
        ctx.scene
            .fill_rounded_rect(r.icon, r.icon.width() / 2.0, accent);

        ctx.label_left(
            r.title.x(),
            r.title.center().y,
            &self.title,
            theme.typography.body.size,
            weight::BOLD,
            colors.text,
        );

        if let (Some(description), Some(rect)) = (&self.description, r.description) {
            ctx.label_left(
                rect.x(),
                rect.center().y,
                description,
                theme.typography.body.size,
                weight::NORMAL,
                colors.dim_text,
            );
        }

        for (label, rect) in self.actions.iter().zip(&r.actions) {
            ctx.scene
                .fill_rounded_rect(*rect, theme.radius.small, colors.shade);
            ctx.label_center(
                rect.center().x,
                rect.center().y,
                label,
                theme.typography.body.size,
                weight::MEDIUM,
                colors.text,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::from_xywh(0.0, 0.0, 600.0, 56.0)
    }

    #[test]
    fn layout_places_icon_left_and_actions_right() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let banner = Banner::new("Błąd")
            .kind(BannerKind::Error)
            .description("Nie udało się zapisać")
            .action("Ponów");
        let rects = banner.rects(area(), &theme, &text);
        assert!(rects.icon.x() < rects.title.x());
        assert!(rects.description.is_some());
        assert_eq!(rects.actions.len(), 1);
        assert!(rects.actions[0].right() <= area().right());
        assert!(rects.title.right() <= rects.actions[0].x());
    }

    #[test]
    fn action_hit_test() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let banner = Banner::new("Info").action("OK");
        let r = banner.rects(area(), &theme, &text);
        assert_eq!(
            banner.action_at(area(), &theme, &text, r.actions[0].center()),
            Some(0)
        );
        assert_eq!(
            banner.action_at(area(), &theme, &text, Point::new(2.0, 2.0)),
            None
        );
    }

    #[test]
    fn colors_per_kind() {
        let theme = Theme::light();
        assert_eq!(Banner::new("x").color(&theme), theme.colors.accent);
        assert_eq!(
            Banner::new("x").kind(BannerKind::Error).color(&theme),
            theme.colors.error
        );
    }
}
