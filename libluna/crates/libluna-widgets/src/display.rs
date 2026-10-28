//! Widgety prezentacji (Display): `Label`, `Separator`, `ProgressBar`, `LevelBar`, `Frame`,
//! `Revealer`, `Overlay`, `AspectFrame`.

use libluna_core::style::weight;
use libluna_core::{Animation, Easing, Rect, Size, Theme};
use libluna_render::TextEngine;

use crate::bin::BinAlign;
use crate::ctx::PaintCtx;

/// Orientacja (pozioma/pionowa).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}

/// Wyrównanie napisu w poziomie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelAlign {
    Start,
    #[default]
    Center,
    End,
}

/// Etykieta tekstowa (z ucinaniem w wielokropek).
#[derive(Debug, Clone, Default)]
pub struct Label {
    text: String,
    align: LabelAlign,
    dim: bool,
    bold: bool,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn align(mut self, align: LabelAlign) -> Self {
        self.align = align;
        self
    }

    pub fn dim(mut self, dim: bool) -> Self {
        self.dim = dim;
        self
    }

    pub fn bold(mut self, bold: bool) -> Self {
        self.bold = bold;
        self
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let size = theme.typography.body.size;
        let w = if self.bold {
            weight::BOLD
        } else {
            weight::NORMAL
        };
        let color = if self.dim {
            theme.colors.dim_text
        } else {
            theme.colors.text
        };
        let cy = rect.center().y;
        ctx.scene.push_clip(rect);
        match self.align {
            LabelAlign::Start => ctx.label_left(rect.x(), cy, &self.text, size, w, color),
            LabelAlign::Center => ctx.label_center(rect.center().x, cy, &self.text, size, w, color),
            LabelAlign::End => ctx.label_right(rect.right(), cy, &self.text, size, w, color),
        }
        ctx.scene.pop_clip();
    }
}

/// Separator (linia).
#[derive(Debug, Clone, Copy)]
pub struct Separator {
    orientation: Orientation,
}

impl Separator {
    pub fn new(orientation: Orientation) -> Self {
        Self { orientation }
    }

    pub fn horizontal() -> Self {
        Self::new(Orientation::Horizontal)
    }

    pub fn vertical() -> Self {
        Self::new(Orientation::Vertical)
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let line = match self.orientation {
            Orientation::Horizontal => {
                Rect::from_xywh(rect.x(), rect.center().y - 0.5, rect.width(), 1.0)
            }
            Orientation::Vertical => {
                Rect::from_xywh(rect.center().x - 0.5, rect.y(), 1.0, rect.height())
            }
        };
        ctx.scene.fill_rect(line, theme.colors.border);
    }
}

/// Pasek postępu (0..1) z opcjonalnym napisem.
#[derive(Debug, Clone, Default)]
pub struct ProgressBar {
    fraction: f64,
    text: Option<String>,
    show_text: bool,
}

impl ProgressBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fraction(mut self, fraction: f64) -> Self {
        self.fraction = fraction.clamp(0.0, 1.0);
        self
    }

    pub fn set_fraction(&mut self, fraction: f64) {
        self.fraction = fraction.clamp(0.0, 1.0);
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self.show_text = true;
        self
    }

    pub fn show_text(mut self, show: bool) -> Self {
        self.show_text = show;
        self
    }

    pub fn fraction_value(&self) -> f64 {
        self.fraction
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let m = theme.metrics.bar;
        let h = rect.height().min(m.thickness);
        let track = Rect::from_xywh(rect.x(), rect.center().y - h / 2.0, rect.width(), h);
        ctx.scene
            .fill_rounded_rect(track, m.radius, theme.colors.shade);
        let fill = Rect::from_xywh(
            track.x(),
            track.y(),
            track.width() * self.fraction,
            track.height(),
        );
        if self.fraction > 0.0 {
            ctx.scene
                .fill_rounded_rect(fill, m.radius, theme.colors.accent);
        }
        if self.show_text {
            if let Some(text) = &self.text {
                ctx.label_center(
                    rect.center().x,
                    rect.center().y,
                    text,
                    theme.typography.caption.size,
                    weight::NORMAL,
                    theme.colors.text,
                );
            } else {
                let label = format!("{}%", (self.fraction * 100.0).round() as i32);
                ctx.label_center(
                    rect.center().x,
                    rect.center().y,
                    &label,
                    theme.typography.caption.size,
                    weight::NORMAL,
                    theme.colors.text,
                );
            }
        }
    }
}

/// Wskaźnik poziomu (segmented).
#[derive(Debug, Clone, Copy)]
pub struct LevelBar {
    value: f64,
    segments: usize,
}

impl Default for LevelBar {
    fn default() -> Self {
        Self {
            value: 0.0,
            segments: 8,
        }
    }
}

impl LevelBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn value(mut self, value: f64) -> Self {
        self.value = value.clamp(0.0, 1.0);
        self
    }

    pub fn segments(mut self, segments: usize) -> Self {
        self.segments = segments.max(1);
        self
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let n = self.segments;
        let gap = 2.0;
        let seg_w = (rect.width() - gap * (n as f64 - 1.0)).max(0.0) / n as f64;
        let filled = (self.value * n as f64).round() as usize;
        for i in 0..n {
            let x = rect.x() + i as f64 * (seg_w + gap);
            let seg = Rect::from_xywh(x, rect.y(), seg_w, rect.height());
            let color = if i < filled {
                theme.colors.accent
            } else {
                theme.colors.shade
            };
            ctx.scene.fill_rounded_rect(seg, 2.0, color);
        }
    }
}

/// Ramka dekoracyjna z opcjonalną etykietą.
#[derive(Debug, Clone, Default)]
pub struct Frame {
    label: Option<String>,
}

impl Frame {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Prostokąt zawartości (wewnątrz ramki).
    pub fn content_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        rect.deflate(libluna_core::Insets::splat(theme.spacing.md))
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) -> Rect {
        let theme = *ctx.theme;
        ctx.scene
            .stroke_rounded_rect(rect, theme.radius.medium, 1.0, theme.colors.border);
        if let Some(label) = &self.label {
            ctx.label_left(
                rect.x() + theme.spacing.md,
                rect.y(),
                label,
                theme.typography.caption.size,
                weight::BOLD,
                theme.colors.dim_text,
            );
        }
        self.content_rect(rect, &theme)
    }
}

/// Odkrywanie/ukrywanie zawartości; ułamek można zadać wprost albo animować [`Revealer::advance`].
#[derive(Debug, Clone, Copy)]
pub struct Revealer {
    open: bool,
    orientation: Orientation,
    animation: Animation,
}

impl Default for Revealer {
    fn default() -> Self {
        Self {
            open: false,
            orientation: Orientation::Vertical,
            animation: Animation::default(),
        }
    }
}

impl Revealer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// Ustawia stan natychmiast (bez animacji).
    pub fn set_open(&mut self, open: bool) {
        self.open = open;
        self.animation.jump_to(if open { 1.0 } else { 0.0 });
    }

    /// Ustawia ułamek odsłonięcia natychmiast.
    pub fn set_fraction(&mut self, fraction: f64) {
        let fraction = fraction.clamp(0.0, 1.0);
        self.animation.jump_to(fraction);
        self.open = fraction > 0.0;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Bieżący ułamek odsłonięcia (0..1).
    pub fn fraction(&self) -> f64 {
        self.animation.value()
    }

    /// Rozpoczyna płynne odsłanianie/ukrywanie z zadaną krzywą.
    pub fn animate_open(&mut self, open: bool, easing: Easing) {
        self.open = open;
        self.animation.set_easing(easing);
        self.animation.animate_to(if open { 1.0 } else { 0.0 });
    }

    /// Krok animacji (`dt` i czas `duration` w sekundach); zwraca `true`, gdy trwa dalej.
    pub fn advance(&mut self, dt: f64, duration: f64) -> bool {
        self.animation.advance(dt, duration)
    }

    pub fn is_animating(&self) -> bool {
        self.animation.is_animating()
    }

    /// Widoczny prostokąt zawartości (przycięty wg ułamka).
    pub fn reveal_rect(&self, rect: Rect) -> Rect {
        let f = self.fraction();
        match self.orientation {
            Orientation::Vertical => {
                Rect::from_xywh(rect.x(), rect.y(), rect.width(), rect.height() * f)
            }
            Orientation::Horizontal => {
                Rect::from_xywh(rect.x(), rect.y(), rect.width() * f, rect.height())
            }
        }
    }
}

/// Nakładka: prostokąt główny + pozycjonowanie dzieci „overlay”.
#[derive(Debug, Clone, Copy, Default)]
pub struct Overlay;

impl Overlay {
    pub fn new() -> Self {
        Self
    }

    /// Prostokąt dziecka nakładki o rozmiarze `size`, wyrównanego w `rect`.
    pub fn child_rect(
        &self,
        rect: Rect,
        size: Size,
        halign: BinAlign,
        valign: BinAlign,
        margin: f64,
    ) -> Rect {
        let inner = rect.deflate(libluna_core::Insets::splat(margin));
        let w = size.width.min(inner.width());
        let h = size.height.min(inner.height());
        let x = match halign {
            BinAlign::Start => inner.x(),
            BinAlign::Center => inner.x() + (inner.width() - w) / 2.0,
            BinAlign::End => inner.right() - w,
        };
        let y = match valign {
            BinAlign::Start => inner.y(),
            BinAlign::Center => inner.y() + (inner.height() - h) / 2.0,
            BinAlign::End => inner.bottom() - h,
        };
        Rect::from_xywh(x, y, w, h)
    }
}

/// Ramka utrzymująca proporcje dziecka (szerokość/wysokość).
#[derive(Debug, Clone, Copy)]
pub struct AspectFrame {
    ratio: f64,
}

impl AspectFrame {
    pub fn new(ratio: f64) -> Self {
        Self { ratio }
    }

    /// Prostokąt dziecka o zadanych proporcjach, wpisany w `rect` (wyśrodkowany).
    pub fn content_rect(&self, rect: Rect) -> Rect {
        if self.ratio <= 0.0 || rect.width() <= 0.0 || rect.height() <= 0.0 {
            return rect;
        }
        let mut w = rect.width();
        let mut h = w / self.ratio;
        if h > rect.height() {
            h = rect.height();
            w = h * self.ratio;
        }
        Rect::from_xywh(
            rect.x() + (rect.width() - w) / 2.0,
            rect.y() + (rect.height() - h) / 2.0,
            w,
            h,
        )
    }
}

/// Pomocnik: ucinanie napisu z wielokropkiem do zadanej szerokości.
pub fn ellipsize(text: &str, max_width: f64, size: f64, text_engine: &TextEngine) -> String {
    if text_engine.measure(text, size, weight::NORMAL).width <= max_width {
        return text.to_owned();
    }
    let mut out = String::new();
    for ch in text.chars() {
        let mut candidate = out.clone();
        candidate.push(ch);
        candidate.push('…');
        if text_engine.measure(&candidate, size, weight::NORMAL).width > max_width {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 200.0, 40.0)
    }

    #[test]
    fn progress_fraction_clamps() {
        let mut p = ProgressBar::new().fraction(1.5);
        assert_eq!(p.fraction_value(), 1.0);
        p.set_fraction(-1.0);
        assert_eq!(p.fraction_value(), 0.0);
    }

    #[test]
    fn separator_line_is_centered() {
        let s = Separator::horizontal();
        assert_eq!(s.orientation, Orientation::Horizontal);
    }

    #[test]
    fn revealer_clips_by_fraction() {
        let mut r = Revealer::new();
        r.set_fraction(0.5);
        let rr = r.reveal_rect(rect());
        assert_eq!(rr.height(), 20.0);
        r.set_open(true);
        assert_eq!(r.reveal_rect(rect()).height(), 40.0);
        r.set_open(false);
        assert_eq!(r.reveal_rect(rect()).height(), 0.0);
    }

    #[test]
    fn revealer_animates_open() {
        let mut r = Revealer::new();
        r.animate_open(true, Easing::Linear);
        assert!(r.is_animating());
        assert!(r.advance(0.5, 1.0));
        assert!((r.fraction() - 0.5).abs() < 1e-9);
        assert!(!r.advance(0.5, 1.0));
        assert_eq!(r.fraction(), 1.0);
        assert!(!r.is_animating());
        assert_eq!(r.reveal_rect(rect()).height(), 40.0);
    }

    #[test]
    fn aspect_frame_fits_ratio() {
        let a = AspectFrame::new(2.0); // szersze niż wyższe
        let r = a.content_rect(Rect::from_xywh(0.0, 0.0, 100.0, 100.0));
        assert_eq!(r.width(), 100.0);
        assert_eq!(r.height(), 50.0);
        assert_eq!(r.y(), 25.0);
    }

    #[test]
    fn overlay_anchors_child() {
        let o = Overlay::new();
        let r = o.child_rect(
            rect(),
            Size::new(20.0, 10.0),
            BinAlign::End,
            BinAlign::Start,
            4.0,
        );
        assert_eq!(r.right(), 196.0);
        assert_eq!(r.y(), 4.0);
    }

    #[test]
    fn ellipsize_shortens() {
        let engine = TextEngine::system();
        let out = ellipsize("bardzo długi napis do ucięcia", 20.0, 14.0, &engine);
        assert!(out.ends_with('…'));
        assert!(out.chars().count() < "bardzo długi napis do ucięcia".chars().count());
    }
}
