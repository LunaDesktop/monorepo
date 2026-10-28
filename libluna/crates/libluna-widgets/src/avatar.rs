//! `Avatar` — okrągły awatar z inicjałami (zastępstwo obrazka).
//!
//! Widget immediate: [`Avatar::paint`] rysuje wypełniony okrąg z inicjałami. Rozmiar z tokenów
//! ([`AvatarMetrics`](libluna_core::AvatarMetrics)).

use libluna_core::style::weight;
use libluna_core::{Rect, Theme};

use crate::ctx::PaintCtx;

/// Awatar z inicjałami.
#[derive(Debug, Clone, Default)]
pub struct Avatar {
    initials: String,
}

impl Avatar {
    pub fn new(initials: impl Into<String>) -> Self {
        Self {
            initials: initials.into(),
        }
    }

    pub fn initials(&self) -> &str {
        &self.initials
    }

    /// Domyślna średnica awatara (z metryk).
    pub fn size(&self, theme: &Theme) -> f64 {
        theme.metrics.avatar.size
    }

    /// Rysuje awatar w (kwadratowym) prostokącie `rect`.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let radius = rect.width().min(rect.height()) / 2.0;
        ctx.scene
            .fill_rounded_rect(rect, radius, theme.colors.accent);
        // Maksymalnie dwa znaki inicjałów (np. „Ada Lovelace” → „AL”).
        let initials: String = self
            .initials
            .chars()
            .take(2)
            .collect::<String>()
            .to_uppercase();
        // Inicjały skalują się z awatarem: 40 px → ~14 px, 72 px → ~26 px.
        let initials_size = (rect.width() * 0.36).clamp(
            theme.typography.caption.size,
            theme.typography.title.size * 1.25,
        );
        ctx.label_center(
            rect.center().x,
            rect.center().y,
            &initials,
            initials_size,
            weight::BOLD,
            theme.colors.accent_fg,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_from_metrics() {
        let theme = Theme::light();
        assert_eq!(Avatar::new("AL").size(&theme), theme.metrics.avatar.size);
        assert_eq!(Avatar::new("AL").initials(), "AL");
    }
}
