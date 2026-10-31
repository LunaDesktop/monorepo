//! `Dialog` i warianty — okna modalne (nakładka): zwykłe, alert, o aplikacji, skróty, preferencje.
//!
//! Bazowy [`Dialog`] rysuje przygaszenie, ramkę i nagłówek z zamknięciem, i wyznacza obszar
//! zawartości. [`AlertDialog`], [`AboutDialog`], [`ShortcutsDialog`] i [`PreferencesDialog`]
//! to warianty o różnej zawartości (budują geometrię na bazie [`Dialog`]).

use libluna_core::style::weight;
use libluna_core::{Color, Insets, Point, Rect, Size, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;
use crate::icon;
use crate::shortcut_label::ShortcutLabel;

/// Bok obszaru przycisku zamknięcia.
const CLOSE: f64 = 24.0;

fn centered_panel(size: Size, width: f64, height: f64) -> Rect {
    Rect::from_xywh(
        (size.width - width) / 2.0,
        (size.height - height) / 2.0,
        width,
        height,
    )
}

/// Prostokąty okna modalnego.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DialogRects {
    pub panel: Rect,
    pub header: Rect,
    pub content: Rect,
    pub close: Rect,
}

/// Okno modalne (nakładka).
#[derive(Debug, Clone)]
pub struct Dialog {
    title: String,
    width: f64,
    height: f64,
}

impl Dialog {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            width: 400.0,
            height: 280.0,
        }
    }

    pub fn size(mut self, width: f64, height: f64) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn set_height(&mut self, height: f64) {
        self.height = height;
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Prostokąty okna w danym rozmiarze (panel wyśrodkowany).
    pub fn rects(&self, size: Size, theme: &Theme) -> DialogRects {
        let panel = centered_panel(size, self.width, self.height);
        let header_h = theme.metrics.sheet.header;
        let md = theme.spacing.md;
        let header = Rect::from_xywh(panel.x(), panel.y(), panel.width(), header_h);
        let content = Rect::from_xywh(
            panel.x(),
            panel.y() + header_h,
            panel.width(),
            (panel.height() - header_h).max(0.0),
        );
        let cy = panel.y() + header_h / 2.0;
        let close = Rect::from_xywh(panel.right() - md - CLOSE, cy - CLOSE / 2.0, CLOSE, CLOSE);
        DialogRects {
            panel,
            header,
            content,
            close,
        }
    }

    /// Całe tło przygaszające.
    pub fn scrim_rect(size: Size) -> Rect {
        Rect::from_size(size)
    }

    /// Czy punkt trafia w tło poza panelem (klik zamykający).
    pub fn scrim_at(&self, size: Size, theme: &Theme, p: Point) -> bool {
        !self.rects(size, theme).panel.contains(p)
    }

    /// Czy punkt trafia w przycisk zamknięcia.
    pub fn close_at(&self, size: Size, theme: &Theme, p: Point) -> bool {
        self.rects(size, theme).close.contains(p)
    }

    /// Rysuje przygaszenie i panel z nagłówkiem; zwraca prostokąty.
    pub fn paint(&self, ctx: &mut PaintCtx, size: Size) -> DialogRects {
        let theme = *ctx.theme;
        let r = self.rects(size, &theme);
        ctx.scene.fill_rect(
            Self::scrim_rect(size),
            Color::rgba(0.0, 0.0, 0.0, theme.metrics.sheet.scrim as f32),
        );
        ctx.scene
            .fill_rounded_rect(r.panel, theme.radius.large, theme.colors.card_bg);
        ctx.scene
            .stroke_rounded_rect(r.panel, theme.radius.large, 1.0, theme.colors.border);
        ctx.label_center(
            r.header.center().x,
            r.header.center().y,
            &self.title,
            theme.typography.body.size,
            weight::BOLD,
            theme.colors.text,
        );
        icon::close(ctx, theme.colors.dim_text, r.close, 5.0);
        r
    }
}

/// Okno alertu: tytuł, treść i przyciski.
#[derive(Debug, Clone)]
pub struct AlertDialog {
    dialog: Dialog,
    body: String,
    buttons: Vec<String>,
}

/// Prostokąty okna alertu.
#[derive(Debug, Clone, PartialEq)]
pub struct AlertRects {
    pub dialog: DialogRects,
    pub body: Rect,
    pub buttons: Vec<Rect>,
}

impl AlertDialog {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            dialog: Dialog::new(title).size(380.0, 0.0),
            body: body.into(),
            buttons: Vec::new(),
        }
    }

    pub fn button(mut self, label: impl Into<String>) -> Self {
        self.buttons.push(label.into());
        self
    }

    /// Szerokość okna (np. gdy jest więcej przycisków).
    pub fn width(mut self, width: f64) -> Self {
        self.dialog.width = width;
        self
    }

    fn height(&self, theme: &Theme, text: &TextEngine) -> f64 {
        let pad = theme.spacing.lg;
        let header = theme.metrics.sheet.header;
        let lines = self.wrapped_lines(theme, text).len().max(1) as f64;
        header
            + pad
            + lines * theme.typography.body.line_height
            + pad
            + theme.metrics.button.height
            + pad
    }

    /// Zawija treść do szerokości okna (zachowuje jawne `\n`).
    fn wrapped_lines(&self, theme: &Theme, text: &TextEngine) -> Vec<String> {
        let width = (self.dialog.width - 2.0 * theme.spacing.lg).max(40.0);
        wrap_text(&self.body, width, text, theme)
    }

    pub fn rects(&self, size: Size, theme: &Theme, text: &TextEngine) -> AlertRects {
        let lines = self.wrapped_lines(theme, text);
        let dialog = Dialog {
            title: self.dialog.title.clone(),
            width: self.dialog.width,
            height: self.height(theme, text),
        };
        let d = dialog.rects(size, theme);
        let pad = theme.spacing.lg;
        let inner = d.content.deflate(Insets::new(pad, pad, 0.0, pad));
        let body_h = lines.len().max(1) as f64 * theme.typography.body.line_height;
        let body = Rect::from_xywh(inner.x(), inner.y(), inner.width(), body_h);
        let buttons = layout_buttons(
            &self.buttons,
            Rect::from_xywh(
                inner.x(),
                inner.y() + body_h + pad,
                inner.width(),
                theme.metrics.button.height,
            ),
            theme,
            text,
        );
        AlertRects {
            dialog: d,
            body,
            buttons,
        }
    }

    pub fn action_at(
        &self,
        size: Size,
        theme: &Theme,
        text: &TextEngine,
        p: Point,
    ) -> Option<usize> {
        self.rects(size, theme, text)
            .buttons
            .iter()
            .position(|r| r.contains(p))
    }

    pub fn paint(&self, ctx: &mut PaintCtx, size: Size) {
        let theme = *ctx.theme;
        let lines = self.wrapped_lines(&theme, ctx.text);
        let r = {
            let dialog = Dialog {
                title: self.dialog.title.clone(),
                width: self.dialog.width,
                height: self.height(&theme, ctx.text),
            };
            dialog.paint(ctx, size)
        };
        let pad = theme.spacing.lg;
        let inner = r.content.deflate(Insets::new(pad, pad, 0.0, pad));
        for (i, line) in lines.iter().enumerate() {
            ctx.label_left(
                inner.x(),
                inner.y() + i as f64 * theme.typography.body.line_height,
                line,
                theme.typography.body.size,
                weight::NORMAL,
                theme.colors.dim_text,
            );
        }
        let rects = self.rects(size, &theme, ctx.text);
        for (label, br) in self.buttons.iter().zip(&rects.buttons) {
            ctx.scene
                .fill_rounded_rect(*br, theme.radius.small, theme.colors.accent);
            ctx.label_center(
                br.center().x,
                br.center().y,
                label,
                theme.typography.body.size,
                weight::MEDIUM,
                theme.colors.accent_fg,
            );
        }
    }
}

/// Zawija tekst do `width`, zachowując jawne podziały linii.
fn wrap_text(body: &str, width: f64, text: &TextEngine, theme: &Theme) -> Vec<String> {
    let size = theme.typography.body.size;
    let weight = weight::NORMAL;
    let mut out = Vec::new();
    for paragraph in body.split('\n') {
        if paragraph.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if text.measure(&candidate, size, weight).width <= width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            // Słowo dłuższe niż cała linia — dzielimy po znakach.
            let mut chunk = String::new();
            for ch in word.chars() {
                let probe = format!("{chunk}{ch}");
                if !chunk.is_empty() && text.measure(&probe, size, weight).width > width {
                    out.push(std::mem::take(&mut chunk));
                }
                chunk.push(ch);
            }
            line = chunk;
        }
        out.push(line);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn layout_buttons(labels: &[String], area: Rect, theme: &Theme, text: &TextEngine) -> Vec<Rect> {
    let bm = theme.metrics.button;
    let mut out = Vec::with_capacity(labels.len());
    let mut right = area.right();
    for label in labels.iter().rev() {
        let w = 2.0 * bm.pad_x
            + text
                .measure(label, theme.typography.body.size, weight::MEDIUM)
                .width;
        out.push(Rect::from_xywh(right - w, area.y(), w, area.height()));
        right -= w + theme.spacing.md;
    }
    out.reverse();
    out
}

/// Okno „O aplikacji".
#[derive(Debug, Clone)]
pub struct AboutDialog {
    dialog: Dialog,
    app_name: String,
    version: String,
    developer: Option<String>,
    website: Option<String>,
}

impl AboutDialog {
    pub fn new(app_name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            dialog: Dialog::new("About").size(380.0, 0.0),
            app_name: app_name.into(),
            version: version.into(),
            developer: None,
            website: None,
        }
    }

    pub fn developer(mut self, developer: impl Into<String>) -> Self {
        self.developer = Some(developer.into());
        self
    }

    pub fn website(mut self, website: impl Into<String>) -> Self {
        self.website = Some(website.into());
        self
    }

    fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.sheet.header + theme.spacing.lg * 2.0 + 4.0 * 24.0
    }

    pub fn paint(&self, ctx: &mut PaintCtx, size: Size) {
        let theme = *ctx.theme;
        let dialog = Dialog {
            title: self.dialog.title.clone(),
            width: self.dialog.width,
            height: self.height(&theme),
        };
        let r = dialog.paint(ctx, size);
        let cx = r.content.center().x;
        let mut y = r.content.y() + theme.spacing.lg;
        let line = 24.0;
        ctx.label_center(
            cx,
            y,
            &self.app_name,
            theme.typography.title.size,
            weight::BOLD,
            theme.colors.text,
        );
        y += line;
        ctx.label_center(
            cx,
            y,
            &self.version,
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
        y += line;
        if let Some(dev) = &self.developer {
            ctx.label_center(
                cx,
                y,
                dev,
                theme.typography.caption.size,
                weight::NORMAL,
                theme.colors.dim_text,
            );
            y += line;
        }
        if let Some(site) = &self.website {
            ctx.label_center(
                cx,
                y,
                site,
                theme.typography.caption.size,
                weight::NORMAL,
                theme.colors.accent,
            );
        }
    }
}

/// Okno ze skrótami klawiszowymi.
#[derive(Debug, Clone)]
pub struct ShortcutsDialog {
    dialog: Dialog,
    entries: Vec<(String, Vec<String>)>,
}

impl ShortcutsDialog {
    pub fn new() -> Self {
        Self {
            dialog: Dialog::new("Keyboard Shortcuts").size(420.0, 0.0),
            entries: Vec::new(),
        }
    }

    /// Dodaje pozycję: etykietę akcji + klawisze (np. `["Ctrl","N"]`).
    pub fn entry<I, S>(mut self, label: impl Into<String>, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.entries
            .push((label.into(), keys.into_iter().map(Into::into).collect()));
        self
    }

    fn row_h(theme: &Theme) -> f64 {
        theme.metrics.menu.item_h
    }

    fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.sheet.header
            + theme.spacing.lg * 2.0
            + self.entries.len() as f64 * Self::row_h(theme)
    }

    pub fn paint(&self, ctx: &mut PaintCtx, size: Size) {
        let theme = *ctx.theme;
        let dialog = Dialog {
            title: self.dialog.title.clone(),
            width: self.dialog.width,
            height: self.height(&theme),
        };
        let r = dialog.paint(ctx, size);
        let pad = theme.spacing.lg;
        let inner = r.content.deflate(Insets::splat(pad));
        let row_h = Self::row_h(&theme);
        for (i, (label, keys)) in self.entries.iter().enumerate() {
            let row = Rect::from_xywh(
                inner.x(),
                inner.y() + i as f64 * row_h,
                inner.width(),
                row_h,
            );
            ctx.label_left(
                row.x(),
                row.center().y,
                label,
                theme.typography.body.size,
                weight::NORMAL,
                theme.colors.text,
            );
            let sc = ShortcutLabel::new(keys.clone());
            let w = sc.width(&theme, ctx.text);
            let rect = Rect::from_xywh(row.right() - w, row.y(), w, row.height());
            sc.paint(ctx, rect);
        }
    }
}

impl Default for ShortcutsDialog {
    fn default() -> Self {
        Self::new()
    }
}

/// Okno preferencji: [`Dialog`] z obszarem zawartości na [`PreferencesPage`](crate::PreferencesPage).
#[derive(Debug, Clone)]
pub struct PreferencesDialog {
    dialog: Dialog,
}

impl PreferencesDialog {
    pub fn new(title: impl Into<String>, height: f64) -> Self {
        Self {
            dialog: Dialog::new(title).size(460.0, height),
        }
    }

    /// Prostokąty (m.in. `content` na [`PreferencesPage`](crate::PreferencesPage)).
    pub fn rects(&self, size: Size, theme: &Theme) -> DialogRects {
        self.dialog.rects(size, theme)
    }

    pub fn scrim_at(&self, size: Size, theme: &Theme, p: Point) -> bool {
        self.dialog.scrim_at(size, theme, p)
    }

    pub fn close_at(&self, size: Size, theme: &Theme, p: Point) -> bool {
        self.dialog.close_at(size, theme, p)
    }

    pub fn paint(&self, ctx: &mut PaintCtx, size: Size) -> DialogRects {
        self.dialog.paint(ctx, size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size() -> Size {
        Size::new(900.0, 700.0)
    }

    #[test]
    fn dialog_centers_and_close_hits() {
        let theme = Theme::light();
        let d = Dialog::new("Test").size(400.0, 300.0);
        let r = d.rects(size(), &theme);
        assert!((r.panel.center().x - size().width / 2.0).abs() < 1e-9);
        assert!(d.close_at(size(), &theme, r.close.center()));
        assert!(d.scrim_at(size(), &theme, Point::new(5.0, 5.0)));
        assert!(!d.scrim_at(size(), &theme, r.panel.center()));
    }

    #[test]
    fn alert_body_wraps_long_lines_and_keeps_explicit_breaks() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let long = "syntax highlighting ".repeat(30);
        let alert = AlertDialog::new("O aplikacji", long);
        let lines = alert.wrapped_lines(&theme, &text);
        assert!(lines.len() > 2, "długi opis musi się zawinąć");
        for line in &lines {
            let width = text
                .measure(line, theme.typography.body.size, weight::NORMAL)
                .width;
            assert!(width <= alert.dialog.width - 2.0 * theme.spacing.lg + 0.5);
        }
        let with_break = AlertDialog::new("T", "pierwsza\ndruga");
        assert_eq!(with_break.wrapped_lines(&theme, &text).len(), 2);
    }

    #[test]
    fn alert_actions_are_right_aligned() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let alert = AlertDialog::new("Błąd?", "Nie udało się zapisać.")
            .button("Anuluj")
            .button("Zapisz");
        let r = alert.rects(size(), &theme, &text);
        assert_eq!(r.buttons.len(), 2);
        assert!(r.buttons[0].right() <= r.buttons[1].x());
        assert_eq!(
            alert.action_at(size(), &theme, &text, r.buttons[1].center()),
            Some(1)
        );
        assert!(r.dialog.content.y() > r.dialog.panel.y());
    }

    #[test]
    fn preferences_dialog_content_below_header() {
        let theme = Theme::light();
        let d = PreferencesDialog::new("Preferences", 400.0);
        let r = d.rects(size(), &theme);
        assert!(r.content.y() > r.panel.y());
        assert_eq!(r.panel.height(), 400.0);
    }
}
