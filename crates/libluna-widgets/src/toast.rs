//! `Toast` — krótkie powiadomienie unoszące się nad treścią (nakładka).
//!
//! Widget immediate: trzyma napis, opcjonalną etykietę akcji i stan otwarcia.
//! Aplikacja woła [`Toast::paint`] (na samym wierzchu sceny) oraz [`Toast::action_at`]
//! (klik w akcję — np. „Cofnij”). Czas ukrycia sterowany jest z aplikacji (brak pętli animacji).

use libluna_core::style::{weight, Appearance};
use libluna_core::{Point, Rect, Size, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;

/// Powiadomienie („toast”).
#[derive(Debug, Clone, Default)]
pub struct Toast {
    message: String,
    action: Option<String>,
    open: bool,
}

impl Toast {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            action: None,
            open: false,
        }
    }

    /// Ustawia etykietę akcji (np. „Cofnij”).
    pub fn action(mut self, label: impl Into<String>) -> Self {
        self.action = Some(label.into());
        self
    }

    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
    }

    pub fn set_action(&mut self, action: Option<String>) {
        self.action = action;
    }

    /// Pokazuje powiadomienie.
    pub fn show(&mut self) {
        self.open = true;
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn action_label(&self) -> Option<&str> {
        self.action.as_deref()
    }

    /// Wysokość powiadomienia (z metryk motywu).
    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.toast.height
    }

    /// Prostokąt powiadomienia (wyśrodkowany poziomo, odsunięty od dołu).
    pub fn rect(&self, size: Size, theme: &Theme, text: &TextEngine) -> Rect {
        let m = theme.metrics.toast;
        let body = theme.typography.body.size;
        let mut w = m.pad_x * 2.0 + text.measure(&self.message, body, weight::NORMAL).width;
        if let Some(action) = &self.action {
            w += m.gap + text.measure(action, body, weight::MEDIUM).width;
        }
        Rect::from_xywh(
            (size.width - w) / 2.0,
            size.height - m.margin_bottom - m.height,
            w,
            m.height,
        )
    }

    /// Prostokąt akcji (jeśli jest), w prawym końcu powiadomienia.
    pub fn action_rect(&self, size: Size, theme: &Theme, text: &TextEngine) -> Option<Rect> {
        let action = self.action.as_ref()?;
        let t = self.rect(size, theme, text);
        let body = theme.typography.body.size;
        let pad = theme.spacing.sm;
        let w = text.measure(action, body, weight::MEDIUM).width + pad * 2.0;
        Some(Rect::from_xywh(
            t.right() - theme.metrics.toast.pad_x - w,
            t.y(),
            w,
            t.height(),
        ))
    }

    /// Czy punkt trafia w akcję powiadomienia.
    pub fn action_at(&self, size: Size, theme: &Theme, text: &TextEngine, p: Point) -> bool {
        self.open
            && self
                .action_rect(size, theme, text)
                .is_some_and(|r| r.contains(p))
    }

    /// Czy punkt trafia w powiadomienie poza jego akcją (klik zamykający).
    pub fn dismiss_at(&self, size: Size, theme: &Theme, text: &TextEngine, p: Point) -> bool {
        self.open
            && self.rect(size, theme, text).contains(p)
            && !self
                .action_rect(size, theme, text)
                .is_some_and(|r| r.contains(p))
    }

    /// Rysuje powiadomienie (na wierzchu sceny).
    pub fn paint(&self, ctx: &mut PaintCtx, size: Size) {
        if !self.open {
            return;
        }
        let theme = *ctx.theme;
        let colors = theme.colors;
        let t = self.rect(size, &theme, ctx.text);

        // Powierzchnia o wysokim kontraście: w motywie jasnym ciemna, w ciemnym nieco
        // jaśniejsza od tła okna (jak „toast” w libadwaita).
        let (bg, fg) = match theme.appearance {
            Appearance::Light => (colors.text, colors.view_bg),
            Appearance::Dark => (colors.card_bg, colors.text),
        };

        ctx.scene.fill_rounded_rect(t, t.height() / 2.0, bg);

        let body = theme.typography.body.size;
        let m = theme.metrics.toast;
        ctx.label_left(
            t.x() + m.pad_x,
            t.center().y,
            &self.message,
            body,
            weight::NORMAL,
            fg,
        );

        if let (Some(action), Some(ar)) = (&self.action, self.action_rect(size, &theme, ctx.text)) {
            ctx.label_center(
                ar.center().x,
                ar.center().y,
                action,
                body,
                weight::MEDIUM,
                colors.accent,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size() -> Size {
        Size::new(900.0, 800.0)
    }

    #[test]
    fn hidden_toast_never_hits_action() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let toast = Toast::new("Zapisano").action("Cofnij");
        let ar = toast.action_rect(size(), &theme, &text).unwrap();
        assert!(!toast.action_at(size(), &theme, &text, ar.center()));
    }

    #[test]
    fn shown_toast_hits_action() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let mut toast = Toast::new("Zapisano").action("Cofnij");
        toast.show();
        let ar = toast.action_rect(size(), &theme, &text).unwrap();
        assert!(toast.action_at(size(), &theme, &text, ar.center()));
        assert!(!toast.action_at(size(), &theme, &text, Point::new(5.0, 5.0)));
    }

    #[test]
    fn body_dismisses_but_action_does_not() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let mut toast = Toast::new("Saved").action("Undo");
        toast.show();
        let r = toast.rect(size(), &theme, &text);
        // Klik w lewy skraj treści zamyka.
        assert!(toast.dismiss_at(size(), &theme, &text, Point::new(r.x() + 6.0, r.center().y)));
        // Klik w akcję nie zamyka (obsługiwany osobno).
        let ar = toast.action_rect(size(), &theme, &text).unwrap();
        assert!(!toast.dismiss_at(size(), &theme, &text, ar.center()));
        // Klik poza powiadomieniem nie zamyka.
        assert!(!toast.dismiss_at(size(), &theme, &text, Point::new(5.0, 5.0)));
    }

    #[test]
    fn toast_is_centered_and_above_bottom() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let mut toast = Toast::new("Gotowe");
        toast.show();
        let r = toast.rect(size(), &theme, &text);
        assert!((r.center().x - 450.0).abs() < 1e-9);
        assert!((r.bottom() - (800.0 - theme.metrics.toast.margin_bottom)).abs() < 1e-9);
        assert!(toast.action_rect(size(), &theme, &text).is_none());
    }
}
