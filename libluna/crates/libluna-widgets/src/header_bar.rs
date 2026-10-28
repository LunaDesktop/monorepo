//! `HeaderBar` — pasek tytułu okna (CSD): tło, tytuł, kontrolki okna i obszar na treść.
//!
//! Widget immediate: liczy rozkład ([`HeaderBar::controls_layout`]), rysuje
//! ([`HeaderBar::paint`]) i podpowiada trafienia ([`HeaderBar::control_at`]). Aplikacja
//! wypełnia zwrócony `content` własnymi widgetami (np. przełącznik paska bocznego, menu).

use libluna_core::style::weight;
use libluna_core::{Color, Insets, Point, Rect, Theme};

use crate::ctx::PaintCtx;
use crate::icon;

/// Kontrolka okna.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowControl {
    Minimize,
    Maximize,
    Close,
}

/// Po której stronie rysować kontrolki okna.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ControlsSide {
    Left,
    #[default]
    Right,
    None,
}

/// Wyrównanie tytułu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TitleAlign {
    #[default]
    Center,
    Start,
}

/// Rozkład paska: prostokąty kontrolek i wolny obszar na treść.
#[derive(Debug, Clone, Copy)]
pub struct ControlsLayout {
    pub minimize: Option<Rect>,
    pub maximize: Option<Rect>,
    pub close: Option<Rect>,
    /// Wolny obszar (między krawędzią a kontrolkami) na własne widgety aplikacji.
    pub content: Rect,
}

/// Pasek tytułu (HeaderBar).
#[derive(Debug, Clone, Default)]
pub struct HeaderBar {
    title: Option<String>,
    title_align: TitleAlign,
    controls: ControlsSide,
    /// Opcjonalne barwy dobrane do okna: `(tło, tekst/ikony)`.
    tint: Option<(Color, Color)>,
}

impl HeaderBar {
    pub fn new() -> Self {
        Self {
            title: None,
            title_align: TitleAlign::Center,
            controls: ControlsSide::Right,
            tint: None,
        }
    }

    /// Nadpisuje barwy paska (np. kolorem dobranym do treści okna).
    pub fn tinted(mut self, bg: Color, fg: Color) -> Self {
        self.tint = Some((bg, fg));
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn title_align(mut self, align: TitleAlign) -> Self {
        self.title_align = align;
        self
    }

    pub fn controls(mut self, side: ControlsSide) -> Self {
        self.controls = side;
        self
    }

    /// Wysokość paska (z tokenów motywu — component metrics).
    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.headerbar.height
    }

    /// Rozkłada kontrolki i wolny obszar w prostokącie paska.
    pub fn controls_layout(&self, rect: Rect, theme: &Theme) -> ControlsLayout {
        let metrics = theme.metrics.headerbar;
        let pad = theme.spacing.sm;
        let control = metrics.control;
        let gap = metrics.control_gap;
        let y = rect.center().y - control / 2.0;
        let control = |x: f64| Rect::from_xywh(x, y, control, control);

        match self.controls {
            ControlsSide::None => ControlsLayout {
                minimize: None,
                maximize: None,
                close: None,
                content: rect.deflate(Insets::symmetric(0.0, pad)),
            },
            ControlsSide::Right => {
                let close_x = rect.right() - pad - metrics.control;
                let max_x = close_x - gap - metrics.control;
                let min_x = max_x - gap - metrics.control;
                let content = Rect::from_xywh(
                    rect.x() + pad,
                    rect.y(),
                    (min_x - pad - (rect.x() + pad)).max(0.0),
                    rect.height(),
                );
                ControlsLayout {
                    minimize: Some(control(min_x)),
                    maximize: Some(control(max_x)),
                    close: Some(control(close_x)),
                    content,
                }
            }
            ControlsSide::Left => {
                let min_x = rect.x() + pad;
                let max_x = min_x + metrics.control + gap;
                let close_x = max_x + metrics.control + gap;
                let left = close_x + metrics.control + pad;
                let content = Rect::from_xywh(
                    left,
                    rect.y(),
                    (rect.right() - pad - left).max(0.0),
                    rect.height(),
                );
                ControlsLayout {
                    minimize: Some(control(min_x)),
                    maximize: Some(control(max_x)),
                    close: Some(control(close_x)),
                    content,
                }
            }
        }
    }

    /// Która kontrolka okna jest pod punktem `p` (albo `None`).
    pub fn control_at(&self, rect: Rect, theme: &Theme, p: Point) -> Option<WindowControl> {
        let layout = self.controls_layout(rect, theme);
        for (control, hit) in [
            (WindowControl::Minimize, layout.minimize),
            (WindowControl::Maximize, layout.maximize),
            (WindowControl::Close, layout.close),
        ] {
            if hit.is_some_and(|r| r.contains(p)) {
                return Some(control);
            }
        }
        None
    }

    /// Rysuje tło, tytuł i kontrolki okna. `hovered` podświetla wskazaną kontrolkę.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect, hovered: Option<WindowControl>) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let layout = self.controls_layout(rect, &theme);
        let (bg, fg_default) = self.tint.unwrap_or((colors.headerbar_bg, colors.text));

        ctx.scene.fill_rect(rect, bg);
        let border = if self.tint.is_some() {
            fg_default.scale_alpha(0.18)
        } else {
            colors.border
        };
        ctx.scene.fill_rect(
            Rect::from_xywh(rect.x(), rect.bottom() - 1.0, rect.width(), 1.0),
            border,
        );

        for (control, hit) in [
            (WindowControl::Minimize, layout.minimize),
            (WindowControl::Maximize, layout.maximize),
            (WindowControl::Close, layout.close),
        ] {
            let Some(box_) = hit else { continue };
            let active = hovered == Some(control);
            if active {
                let bg = if control == WindowControl::Close {
                    colors.error
                } else {
                    fg_default.scale_alpha(0.15)
                };
                ctx.scene.fill_rounded_rect(box_, theme.radius.small, bg);
            }
            let fg = if active && control == WindowControl::Close {
                colors.accent_fg
            } else {
                fg_default
            };
            match control {
                WindowControl::Minimize => icon::minimize(ctx, fg, box_, 5.0),
                WindowControl::Maximize => icon::maximize(ctx, fg, box_, 5.0),
                WindowControl::Close => icon::close(ctx, fg, box_, 5.0),
            }
        }

        if let Some(title) = &self.title {
            let size = theme.typography.body.size;
            match self.title_align {
                TitleAlign::Center => ctx.label_center(
                    rect.center().x,
                    rect.center().y,
                    title,
                    size,
                    weight::BOLD,
                    fg_default,
                ),
                TitleAlign::Start => {
                    let x = layout.content.x();
                    ctx.label_left(x, rect.center().y, title, size, weight::BOLD, fg_default)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar() -> Rect {
        Rect::from_xywh(0.0, 0.0, 900.0, Theme::light().metrics.headerbar.height)
    }

    #[test]
    fn right_side_controls_are_at_right_edge() {
        let theme = Theme::light();
        let hb = HeaderBar::new().controls(ControlsSide::Right);
        let layout = hb.controls_layout(bar(), &theme);
        let close = layout.close.unwrap();
        let minimize = layout.minimize.unwrap();
        assert!(close.right() <= bar().right());
        assert!(close.x() > minimize.x());
        // Treść nie nachodzi na kontrolki.
        assert!(layout.content.right() <= minimize.x());
    }

    #[test]
    fn left_side_controls_are_at_left_edge() {
        let theme = Theme::light();
        let hb = HeaderBar::new().controls(ControlsSide::Left);
        let layout = hb.controls_layout(bar(), &theme);
        let minimize = layout.minimize.unwrap();
        let close = layout.close.unwrap();
        assert!(minimize.x() >= bar().x());
        assert!(close.x() > minimize.x());
        assert!(layout.content.x() >= close.right());
    }

    #[test]
    fn control_at_hits_close() {
        let theme = Theme::light();
        let hb = HeaderBar::new().controls(ControlsSide::Right);
        let close = hb.controls_layout(bar(), &theme).close.unwrap();
        assert_eq!(
            hb.control_at(bar(), &theme, close.center()),
            Some(WindowControl::Close)
        );
        assert_eq!(hb.control_at(bar(), &theme, Point::new(400.0, 23.0)), None);
    }

    #[test]
    fn no_controls_leaves_full_content() {
        let theme = Theme::light();
        let hb = HeaderBar::new().controls(ControlsSide::None);
        let layout = hb.controls_layout(bar(), &theme);
        assert!(layout.close.is_none());
        assert!(layout.content.width() > 800.0);
    }

    #[test]
    fn metrics_drive_bar_and_control_geometry() {
        let mut theme = Theme::light();
        theme.metrics.headerbar.control = 40.0;
        theme.metrics.headerbar.height = 60.0;
        let hb = HeaderBar::new();
        assert_eq!(hb.height(&theme), 60.0);
        let rect = Rect::from_xywh(0.0, 0.0, 900.0, hb.height(&theme));
        let close = hb.controls_layout(rect, &theme).close.unwrap();
        assert_eq!(close.width(), 40.0);
        assert_eq!(close.height(), 40.0);
    }
}
