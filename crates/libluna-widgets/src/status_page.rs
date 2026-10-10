//! `StatusPage` — ekran stanu (pusty/powitalny): ikona, tytuł, opis i opcjonalna akcja.
//!
//! Widget immediate: układa wyśrodkowany blok w zadanym obszarze. Aplikacja woła
//! [`StatusPage::paint`] oraz [`StatusPage::action_at`] (klik w przycisk akcji).

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;
use crate::icon;

/// Ikona ekranu stanu (rysowana wektorowo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusIcon {
    /// Katalog (np. „pusty folder”).
    Folder,
    /// Plik.
    File,
    /// Wyszukiwanie (np. „brak wyników”).
    Search,
}

/// Prostokąty elementów ekranu stanu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatusRects {
    pub icon: Option<Rect>,
    pub title: Rect,
    pub description: Option<Rect>,
    pub action: Option<Rect>,
}

/// Ekran stanu.
#[derive(Debug, Clone, Default)]
pub struct StatusPage {
    icon: Option<StatusIcon>,
    title: String,
    description: Option<String>,
    action: Option<String>,
}

impl StatusPage {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            icon: None,
            title: title.into(),
            description: None,
            action: None,
        }
    }

    pub fn icon(mut self, icon: StatusIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn action(mut self, label: impl Into<String>) -> Self {
        self.action = Some(label.into());
        self
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub fn has_action(&self) -> bool {
        self.action.is_some()
    }

    /// Wyśrodkowany układ elementów w obszarze `area`.
    pub fn rects(&self, area: Rect, theme: &Theme) -> StatusRects {
        let m = theme.metrics.status;
        let title_lh = theme.typography.heading.line_height;
        let desc_lh = theme.typography.body.line_height;

        let icon_h = if self.icon.is_some() { m.icon } else { 0.0 };
        let mut total = title_lh;
        if icon_h > 0.0 {
            total += icon_h + m.gap;
        }
        if self.description.is_some() {
            total += m.gap + desc_lh;
        }
        if self.action.is_some() {
            total += m.gap + m.button_h;
        }

        let cx = area.center().x;
        let mut y = area.center().y - total / 2.0;

        let icon_rect = self.icon.map(|_| {
            let r = Rect::from_xywh(cx - m.icon / 2.0, y, m.icon, m.icon);
            y += m.icon + m.gap;
            r
        });

        let title = Rect::from_xywh(area.x(), y, area.width(), title_lh);
        y += title_lh;

        let description = self.description.as_ref().map(|_| {
            y += m.gap;
            let r = Rect::from_xywh(area.x(), y, area.width(), desc_lh);
            y += desc_lh;
            r
        });

        let action = self.action.as_ref().map(|_| {
            y += m.gap;
            Rect::from_xywh(cx - m.button_w / 2.0, y, m.button_w, m.button_h)
        });

        StatusRects {
            icon: icon_rect,
            title,
            description,
            action,
        }
    }

    /// Czy punkt trafia w przycisk akcji.
    pub fn action_at(&self, area: Rect, theme: &Theme, p: Point) -> bool {
        self.rects(area, theme)
            .action
            .is_some_and(|r| r.contains(p))
    }

    /// Rysuje ekran stanu w obszarze `area`.
    pub fn paint(&self, ctx: &mut PaintCtx, area: Rect) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let r = self.rects(area, &theme);

        ctx.scene.push_clip(area);

        if let (Some(kind), Some(rect)) = (self.icon, r.icon) {
            let accent = colors.dim_text;
            match kind {
                StatusIcon::Folder => icon::folder(ctx, accent, rect),
                StatusIcon::File => icon::file(ctx, accent, rect),
                StatusIcon::Search => icon::search(ctx, accent, rect.center(), rect.width()),
            }
        }

        ctx.label_center(
            r.title.center().x,
            r.title.center().y,
            &self.title,
            theme.typography.heading.size,
            weight::BOLD,
            colors.text,
        );

        if let (Some(description), Some(rect)) = (&self.description, r.description) {
            ctx.label_center(
                rect.center().x,
                rect.center().y,
                description,
                theme.typography.body.size,
                weight::NORMAL,
                colors.dim_text,
            );
        }

        if let (Some(label), Some(rect)) = (&self.action, r.action) {
            let radius = theme.radius.small;
            ctx.scene.fill_rounded_rect(rect, radius, colors.accent);
            ctx.label_center(
                rect.center().x,
                rect.center().y,
                label,
                theme.typography.body.size,
                weight::MEDIUM,
                colors.accent_fg,
            );
        }

        ctx.scene.pop_clip();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_is_vertically_centered() {
        let theme = Theme::light();
        let page = StatusPage::new("Pusto")
            .icon(StatusIcon::Folder)
            .description("Brak plików");
        let area = Rect::from_xywh(0.0, 0.0, 400.0, 300.0);
        let r = page.rects(area, &theme);
        // Ikona jest najwyżej, opis najniżej, a całość wyśrodkowana pionowo.
        assert!(r.icon.unwrap().y() < r.title.y());
        assert!(r.title.y() < r.description.unwrap().y());
        let top = r.icon.unwrap().y();
        let bottom = r.description.unwrap().bottom();
        assert!(((top + bottom) / 2.0 - 150.0).abs() < 1e-9);
    }

    #[test]
    fn action_hit_only_when_present() {
        let theme = Theme::light();
        let area = Rect::from_xywh(0.0, 0.0, 400.0, 300.0);
        let with = StatusPage::new("Pusto").action("Nowy");
        let ar = with.rects(area, &theme).action.unwrap();
        assert!(with.action_at(area, &theme, ar.center()));
        let without = StatusPage::new("Pusto");
        assert!(!without.action_at(area, &theme, area.center()));
    }

    #[test]
    fn title_is_horizontally_centered() {
        let theme = Theme::light();
        let page = StatusPage::new("Pusto");
        let area = Rect::from_xywh(20.0, 0.0, 400.0, 300.0);
        let r = page.rects(area, &theme);
        assert!((r.title.center().x - area.center().x).abs() < 1e-9);
    }
}
