//! `Button` — przycisk z etykietą (warianty: domyślny, płaski, sugerowany, destrukcyjny).
//!
//! Widget immediate: trzyma wygląd (etykieta, wariant, stan), a aplikacja wywołuje
//! [`Button::paint`] i [`Button::click_at`].

use libluna_core::style::{weight, ColorScheme};
use libluna_core::{Color, Point, Rect, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;
use crate::icon;

/// Wariant wizualny przycisku.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    /// Tło „shade” (jak przyciski w HeaderBarze).
    #[default]
    Default,
    /// Bez tła; tło pojawia się przy najechaniu/wciśnięciu.
    Flat,
    /// Sugerowana akcja (akcent).
    Suggested,
    /// Akcja destrukcyjna (kolor błędu).
    Destructive,
}

/// Przycisk z etykietą.
#[derive(Debug, Clone)]
pub struct Button {
    label: String,
    variant: ButtonVariant,
    dropdown: bool,
    enabled: bool,
    hovered: bool,
    pressed: bool,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            variant: ButtonVariant::Default,
            dropdown: false,
            enabled: true,
            hovered: false,
            pressed: false,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Dodaje na końcu wskaźnik rozwijania (chevron) — przyciski menu.
    pub fn dropdown(mut self, dropdown: bool) -> Self {
        self.dropdown = dropdown;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    pub fn set_pressed(&mut self, pressed: bool) {
        self.pressed = pressed;
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Szerokość dopasowana do etykiety.
    pub fn width(&self, text: &TextEngine, theme: &Theme) -> f64 {
        let m = theme.metrics.button;
        let body = theme.typography.body.size;
        let mut w = m.pad_x * 2.0 + text.measure(&self.label, body, weight::NORMAL).width;
        if self.dropdown {
            w += m.chevron_gap + m.chevron;
        }
        w
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.button.height
    }

    pub fn hit(&self, rect: Rect, p: Point) -> bool {
        rect.contains(p)
    }

    /// Zwraca `true`, jeśli klik trafił w aktywny przycisk.
    pub fn click_at(&self, rect: Rect, p: Point) -> bool {
        self.enabled && rect.contains(p)
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        // Spójnie z polami tekstowymi i menu — miękkie, zaokrąglone kontrolki.
        let radius = theme.radius.medium;
        let strong = self.pressed || self.hovered;
        // Warianty kolorowe też muszą reagować na kursor — w przeciwnym razie
        // główne akcje (Zainstaluj/Otwórz) wyglądają martwo pod myszą.
        let tinted = |base: Color| {
            if self.pressed {
                base.lerp(Color::BLACK, 0.10)
            } else if self.hovered {
                base.lerp(Color::WHITE, 0.12)
            } else {
                base
            }
        };

        match self.variant {
            ButtonVariant::Suggested => {
                ctx.scene
                    .fill_rounded_rect(rect, radius, tinted(colors.accent));
            }
            ButtonVariant::Destructive => {
                ctx.scene
                    .fill_rounded_rect(rect, radius, tinted(colors.error));
            }
            ButtonVariant::Default => {
                ctx.scene
                    .fill_rounded_rect(rect, radius, shade(&colors, strong));
            }
            ButtonVariant::Flat => {
                if strong {
                    ctx.scene
                        .fill_rounded_rect(rect, radius, shade(&colors, false));
                }
            }
        }

        let fg = match self.variant {
            ButtonVariant::Suggested | ButtonVariant::Destructive => colors.accent_fg,
            _ if self.enabled => colors.text,
            _ => colors.dim_text,
        };

        let body = theme.typography.body.size;
        let m = theme.metrics.button;
        let cy = rect.center().y;
        let label_w = ctx.measure(&self.label, body, weight::NORMAL);
        let content_w = label_w
            + if self.dropdown {
                m.chevron_gap + m.chevron
            } else {
                0.0
            };
        let start_x = rect.center().x - content_w / 2.0;
        ctx.label_left(start_x, cy, &self.label, body, weight::NORMAL, fg);

        if self.dropdown {
            let cx = start_x + label_w + m.chevron_gap + m.chevron / 2.0;
            icon::chevron_down(ctx, fg, Point::new(cx, cy), 4.0, 2.0);
        }
    }
}

fn shade(colors: &ColorScheme, strong: bool) -> Color {
    if strong {
        colors.shade.scale_alpha(1.8)
    } else {
        colors.shade
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_only_when_enabled_and_inside() {
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 34.0);
        let b = Button::new("OK");
        assert!(b.click_at(rect, Point::new(50.0, 17.0)));
        assert!(!b.click_at(rect, Point::new(200.0, 17.0)));
        let disabled = Button::new("OK").enabled(false);
        assert!(!disabled.click_at(rect, Point::new(50.0, 17.0)));
    }

    #[test]
    fn width_grows_with_dropdown() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let plain = Button::new("Opcje");
        let dd = Button::new("Opcje").dropdown(true);
        assert!(dd.width(&text, &theme) > plain.width(&text, &theme));
    }
}
