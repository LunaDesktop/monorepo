//! `Row` — wiersz listy („boxed list”): tytuł/podtytuł z lewej, kontrolka z prawej.
//!
//! Jeden elastyczny widget pokrywa rodzinę wierszy znaną z libadwaita — konstruktory
//! [`Row::switch`], [`Row::combo`], [`Row::expander`], [`Row::entry`], [`Row::password`],
//! [`Row::spin`], [`Row::button`], [`Row::nav`] i [`Row::action`] różnią się tylko kontrolką
//! (suffixem). Wiersz rysuje tytuł/podtytuł oraz **proste** suffixy (przełącznik, wartość,
//! chevron, przycisk); dla suffixu [`RowSuffix::Custom`] aplikacja rysuje własną kontrolkę
//! we [`Row::suffix_rect`].

use libluna_core::style::weight;
use libluna_core::{Insets, Point, Rect, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;
use crate::icon;
use crate::switch::Switch;

/// Kontrolka po prawej stronie wiersza.
#[derive(Debug, Clone, PartialEq)]
pub enum RowSuffix {
    /// Bez kontrolki.
    None,
    /// Przełącznik wł./wył. (rysowany przez wiersz).
    Switch { on: bool },
    /// Wartość + chevron (jak wybór z listy).
    Value(String),
    /// Strzałka nawigacji (chevron w prawo).
    Chevron,
    /// Chevron rozwijania (w prawo gdy zwinięte, w dół gdy rozwinięte).
    Expander { expanded: bool },
    /// Płaski przycisk z etykietą.
    Button(String),
    /// Kontrolka rysowana przez aplikację (Entry/Slider/Stepper…) o zadanej szerokości.
    Custom(f64),
}

/// Prostokąty wiersza.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowRects {
    pub title: Rect,
    pub subtitle: Option<Rect>,
    pub suffix: Rect,
}

/// Wiersz listy.
#[derive(Debug, Clone)]
pub struct Row {
    title: String,
    subtitle: Option<String>,
    suffix: RowSuffix,
}

/// Bok obszaru prostych suffixów (chevron).
const ICON_AREA: f64 = 20.0;

impl Row {
    fn with_suffix(title: String, subtitle: Option<String>, suffix: RowSuffix) -> Self {
        Self {
            title,
            subtitle,
            suffix,
        }
    }

    /// Wiersz akcji (bez kontrolki) — jak [`ActionRow`](crate::ActionRow).
    pub fn action(title: impl Into<String>) -> Self {
        Self::with_suffix(title.into(), None, RowSuffix::None)
    }

    /// Wiersz z przełącznikiem.
    pub fn switch(title: impl Into<String>, on: bool) -> Self {
        Self::with_suffix(title.into(), None, RowSuffix::Switch { on })
    }

    /// Wiersz z wyborem (wartość + chevron).
    pub fn combo(title: impl Into<String>, value: impl Into<String>) -> Self {
        Self::with_suffix(title.into(), None, RowSuffix::Value(value.into()))
    }

    /// Wiersz nawigacyjny (chevron w prawo).
    pub fn nav(title: impl Into<String>) -> Self {
        Self::with_suffix(title.into(), None, RowSuffix::Chevron)
    }

    /// Wiersz rozwijany.
    pub fn expander(title: impl Into<String>, expanded: bool) -> Self {
        Self::with_suffix(title.into(), None, RowSuffix::Expander { expanded })
    }

    /// Wiersz z płaskim przyciskiem.
    pub fn button(title: impl Into<String>, label: impl Into<String>) -> Self {
        Self::with_suffix(title.into(), None, RowSuffix::Button(label.into()))
    }

    /// Wiersz z polem tekstowym (aplikacja rysuje `Entry` w obszarze suffixu).
    pub fn entry(title: impl Into<String>) -> Self {
        Self::with_suffix(title.into(), None, RowSuffix::Custom(200.0))
    }

    /// Wiersz z polem hasła (jak [`Row::entry`], maskowane przez aplikację).
    pub fn password(title: impl Into<String>) -> Self {
        Self::with_suffix(title.into(), None, RowSuffix::Custom(200.0))
    }

    /// Wiersz z krokrowcem (aplikacja rysuje `Stepper` w obszarze suffixu).
    pub fn spin(title: impl Into<String>) -> Self {
        Self::with_suffix(title.into(), None, RowSuffix::Custom(120.0))
    }

    /// Wiersz z suwakiem (aplikacja rysuje `Slider` w obszarze suffixu).
    pub fn slider(title: impl Into<String>) -> Self {
        Self::with_suffix(title.into(), None, RowSuffix::Custom(180.0))
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn suffix(mut self, suffix: RowSuffix) -> Self {
        self.suffix = suffix;
        self
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn subtitle_text(&self) -> Option<&str> {
        self.subtitle.as_deref()
    }

    pub fn suffix_kind(&self) -> &RowSuffix {
        &self.suffix
    }

    pub fn is_switch_on(&self) -> bool {
        matches!(self.suffix, RowSuffix::Switch { on: true })
    }

    pub fn is_expanded(&self) -> bool {
        matches!(self.suffix, RowSuffix::Expander { expanded: true })
    }

    /// Prostokąt kontrolki (suffixu), wyrównany do prawej i wyśrodkowany pionowo.
    pub fn suffix_rect(&self, rect: Rect, theme: &Theme, text: &TextEngine) -> Rect {
        let pad = theme.metrics.row.pad_x;
        let cy = rect.center().y;
        let body = theme.typography.body.size;
        match &self.suffix {
            RowSuffix::None => Rect::from_xywh(rect.right() - pad, cy, 0.0, 0.0),
            RowSuffix::Switch { .. } => {
                let m = theme.metrics.switch;
                Rect::from_xywh(
                    rect.right() - pad - m.width,
                    cy - m.height / 2.0,
                    m.width,
                    m.height,
                )
            }
            RowSuffix::Value(value) => {
                let bm = theme.metrics.button;
                let w =
                    text.measure(value, body, weight::NORMAL).width + bm.chevron_gap + bm.chevron;
                Rect::from_xywh(rect.right() - pad - w, cy - 12.0, w, 24.0)
            }
            RowSuffix::Chevron | RowSuffix::Expander { .. } => Rect::from_xywh(
                rect.right() - pad - ICON_AREA,
                cy - ICON_AREA / 2.0,
                ICON_AREA,
                ICON_AREA,
            ),
            RowSuffix::Button(label) => {
                let bm = theme.metrics.button;
                let w = 2.0 * bm.pad_x + text.measure(label, body, weight::MEDIUM).width;
                Rect::from_xywh(rect.right() - pad - w, cy - bm.height / 2.0, w, bm.height)
            }
            RowSuffix::Custom(w) => {
                Rect::from_xywh(rect.right() - pad - w, rect.y(), *w, rect.height())
            }
        }
    }

    /// Prostokąty: tytuł, ewentualny podtytuł i kontrolka.
    pub fn rects(&self, rect: Rect, theme: &Theme, text: &TextEngine) -> RowRects {
        let pad = theme.metrics.row.pad_x;
        let suffix = self.suffix_rect(rect, theme, text);
        let left = rect.x() + pad;
        let width = (suffix.x() - left - theme.metrics.row.gap).max(0.0);
        let pad_y = 8.0;
        let (title, subtitle) = match self.subtitle.as_deref() {
            Some(_) => (
                Rect::from_xywh(left, rect.y() + pad_y, width, rect.height() / 2.0 - pad_y),
                Some(Rect::from_xywh(
                    left,
                    rect.center().y,
                    width,
                    rect.height() / 2.0 - pad_y,
                )),
            ),
            None => (Rect::from_xywh(left, rect.y(), width, rect.height()), None),
        };
        RowRects {
            title,
            subtitle,
            suffix,
        }
    }

    /// Rysuje tytuł/podtytuł i prosty suffix; zwraca prostokąty.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) -> RowRects {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let r = self.rects(rect, &theme, ctx.text);

        match &self.subtitle {
            Some(subtitle) => {
                ctx.label_left(
                    r.title.x(),
                    r.title.center().y,
                    &self.title,
                    theme.typography.body.size,
                    weight::NORMAL,
                    colors.text,
                );
                if let Some(sr) = r.subtitle {
                    ctx.label_left(
                        sr.x(),
                        sr.center().y,
                        subtitle,
                        theme.typography.caption.size,
                        weight::NORMAL,
                        colors.dim_text,
                    );
                }
            }
            None => ctx.label_left(
                r.title.x(),
                r.title.center().y,
                &self.title,
                theme.typography.body.size,
                weight::NORMAL,
                colors.text,
            ),
        }

        let body = theme.typography.body.size;
        match &self.suffix {
            RowSuffix::None => {}
            RowSuffix::Switch { on } => {
                Switch::new(*on).paint(ctx, r.suffix);
            }
            RowSuffix::Value(value) => {
                let bm = theme.metrics.button;
                let text_right = r.suffix.right() - bm.chevron - bm.chevron_gap;
                ctx.label_right(
                    text_right,
                    r.suffix.center().y,
                    value,
                    body,
                    weight::NORMAL,
                    colors.dim_text,
                );
                icon::chevron_down(
                    ctx,
                    colors.dim_text,
                    Point::new(r.suffix.right() - bm.chevron / 2.0, r.suffix.center().y),
                    4.0,
                    2.0,
                );
            }
            RowSuffix::Chevron => {
                icon::chevron_right(ctx, colors.dim_text, r.suffix.center(), 4.0, 2.0);
            }
            RowSuffix::Expander { expanded } => {
                if *expanded {
                    icon::chevron_down(ctx, colors.dim_text, r.suffix.center(), 4.0, 2.0);
                } else {
                    icon::chevron_right(ctx, colors.dim_text, r.suffix.center(), 4.0, 2.0);
                }
            }
            RowSuffix::Button(label) => {
                ctx.scene
                    .fill_rounded_rect(r.suffix, theme.radius.small, colors.shade);
                ctx.label_center(
                    r.suffix.center().x,
                    r.suffix.center().y,
                    label,
                    body,
                    weight::MEDIUM,
                    colors.text,
                );
            }
            RowSuffix::Custom(_) => {}
        }
        r
    }

    /// Czy punkt trafia w obszar kontrolki (z tolerancją).
    pub fn suffix_at(&self, rect: Rect, theme: &Theme, text: &TextEngine, p: Point) -> bool {
        if matches!(self.suffix, RowSuffix::None) {
            return false;
        }
        self.suffix_rect(rect, theme, text)
            .inflate(Insets::splat(4.0))
            .contains(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 360.0, 56.0)
    }

    #[test]
    fn switch_row_suffix_hit() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let row = Row::switch("Motyw", true);
        assert!(row.is_switch_on());
        let r = row.rects(rect(), &theme, &text);
        assert!(row.suffix_at(rect(), &theme, &text, r.suffix.center()));
        assert!(!row.suffix_at(rect(), &theme, &text, Point::new(5.0, 5.0)));
        assert!(r.suffix.right() <= rect().right());
        assert!(r.title.right() <= r.suffix.x());
    }

    #[test]
    fn constructors_map_to_suffix_kinds() {
        assert_eq!(
            Row::combo("Język", "Polski").suffix_kind(),
            &RowSuffix::Value("Polski".into())
        );
        assert!(Row::expander("Więcej", true).is_expanded());
        assert!(matches!(
            Row::entry("Nazwa").suffix_kind(),
            RowSuffix::Custom(_)
        ));
        assert!(matches!(
            Row::nav("O nas").suffix_kind(),
            RowSuffix::Chevron
        ));
    }

    #[test]
    fn action_row_has_no_suffix_hit() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let row = Row::action("Tylko tekst");
        assert!(!row.suffix_at(rect(), &theme, &text, rect().center()));
    }
}
