//! Pomocniki układu i „chrome”: `ToolbarView`, `WindowTitle`, `Clamp`, `WrapBox`,
//! `BreakpointBin`, `ButtonContent`.

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Size, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;

/// Pasek górny + treść + pasek dolny.
#[derive(Debug, Clone, Copy, Default)]
pub struct ToolbarView {
    top: f64,
    bottom: f64,
}

/// Podział na pasek górny, treść i pasek dolny.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolbarRects {
    pub top: Option<Rect>,
    pub content: Rect,
    pub bottom: Option<Rect>,
}

impl ToolbarView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn top(mut self, height: f64) -> Self {
        self.top = height;
        self
    }

    pub fn bottom(mut self, height: f64) -> Self {
        self.bottom = height;
        self
    }

    /// Prostokąty pasków (o ile mają dodatnią wysokość) i treści.
    pub fn rects(&self, rect: Rect) -> ToolbarRects {
        let top =
            (self.top > 0.0).then(|| Rect::from_xywh(rect.x(), rect.y(), rect.width(), self.top));
        let bottom = (self.bottom > 0.0).then(|| {
            Rect::from_xywh(
                rect.x(),
                rect.bottom() - self.bottom,
                rect.width(),
                self.bottom,
            )
        });
        let y = rect.y() + self.top;
        let h = (rect.height() - self.top - self.bottom).max(0.0);
        ToolbarRects {
            top,
            content: Rect::from_xywh(rect.x(), y, rect.width(), h),
            bottom,
        }
    }
}

/// Tytuł okna (z opcjonalnym podtytułem), wyśrodkowany.
#[derive(Debug, Clone, Default)]
pub struct WindowTitle {
    title: String,
    subtitle: Option<String>,
}

/// Prostokąty tytułu i podtytułu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowTitleRects {
    pub title: Rect,
    pub subtitle: Option<Rect>,
}

impl WindowTitle {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn rects(&self, rect: Rect, theme: &Theme) -> WindowTitleRects {
        let cx = rect.center().x;
        let half = rect.width().min(360.0) / 2.0;
        let body = theme.typography.body.line_height;
        let caption = theme.typography.caption.line_height;
        let (title, subtitle) = match self.subtitle.is_some() {
            true => (
                Rect::from_xywh(cx - half, rect.center().y - body, half * 2.0, body),
                Some(Rect::from_xywh(
                    cx - half,
                    rect.center().y,
                    half * 2.0,
                    caption,
                )),
            ),
            false => (
                Rect::from_xywh(cx - half, rect.center().y - body / 2.0, half * 2.0, body),
                None,
            ),
        };
        WindowTitleRects { title, subtitle }
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let r = self.rects(rect, &theme);
        ctx.label_center(
            r.title.center().x,
            r.title.center().y,
            &self.title,
            theme.typography.body.size,
            weight::BOLD,
            theme.colors.text,
        );
        if let (Some(subtitle), Some(sr)) = (&self.subtitle, r.subtitle) {
            ctx.label_center(
                sr.center().x,
                sr.center().y,
                subtitle,
                theme.typography.caption.size,
                weight::NORMAL,
                theme.colors.dim_text,
            );
        }
    }
}

/// Ograniczenie szerokości treści (wyśrodkowane).
#[derive(Debug, Clone, Copy)]
pub struct Clamp {
    max_width: f64,
}

impl Default for Clamp {
    fn default() -> Self {
        Self { max_width: 600.0 }
    }
}

impl Clamp {
    pub fn new(max_width: f64) -> Self {
        Self { max_width }
    }

    /// Prostokąt treści: pełna wysokość, szerokość ograniczona i wyśrodkowana.
    pub fn content_rect(&self, rect: Rect) -> Rect {
        let w = rect.width().min(self.max_width);
        Rect::from_xywh(
            rect.x() + (rect.width() - w) / 2.0,
            rect.y(),
            w,
            rect.height(),
        )
    }
}

/// Zawijanie elementów w kolejne linie (flow).
#[derive(Debug, Clone, Copy, Default)]
pub struct WrapBox {
    spacing: f64,
    line_spacing: f64,
}

impl WrapBox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spacing(mut self, spacing: f64) -> Self {
        self.spacing = spacing;
        self
    }

    pub fn line_spacing(mut self, spacing: f64) -> Self {
        self.line_spacing = spacing;
        self
    }

    /// Rozmieszcza elementy o zadanych naturalnych rozmiarach, zawijając do szerokości `width`.
    pub fn layout(&self, rect: Rect, sizes: &[Size]) -> Vec<Rect> {
        let mut out = Vec::with_capacity(sizes.len());
        let mut x = rect.x();
        let mut y = rect.y();
        let mut line_h = 0.0f64;
        for &size in sizes {
            if x > rect.x() && x + size.width > rect.right() {
                x = rect.x();
                y += line_h + self.line_spacing;
                line_h = 0.0;
            }
            out.push(Rect::from_xywh(x, y, size.width, size.height));
            x += size.width + self.spacing;
            line_h = line_h.max(size.height);
        }
        out
    }

    /// Wysokość potrzebna na zawinięcie elementów w szerokości `width`.
    pub fn height(&self, width: f64, sizes: &[Size]) -> f64 {
        let mut x = 0.0;
        let mut y = 0.0;
        let mut line_h = 0.0f64;
        for &size in sizes {
            if x > 0.0 && x + size.width > width {
                x = 0.0;
                y += line_h + self.line_spacing;
                line_h = 0.0;
            }
            x += size.width + self.spacing;
            line_h = line_h.max(size.height);
        }
        y + line_h
    }
}

/// Punkt przełamania (breakpoint) — responsywny układ wg szerokości.
#[derive(Debug, Clone, Default)]
pub struct BreakpointBin {
    breakpoints: Vec<f64>,
}

impl BreakpointBin {
    pub fn new(breakpoints: Vec<f64>) -> Self {
        let mut breakpoints = breakpoints;
        breakpoints.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Self { breakpoints }
    }

    pub fn breakpoints(&self) -> &[f64] {
        &self.breakpoints
    }

    /// Indeks aktywnego progu (liczba progów <= szerokości).
    pub fn active(&self, width: f64) -> usize {
        self.breakpoints.iter().filter(|b| width >= **b).count()
    }
}

/// Układ zawartości przycisku: ikona (opcjonalna) + etykieta, wyśrodkowane.
#[derive(Debug, Clone, Copy, Default)]
pub struct ButtonContent {
    icon: bool,
}

impl ButtonContent {
    pub fn label() -> Self {
        Self { icon: false }
    }

    pub fn with_icon() -> Self {
        Self { icon: true }
    }

    /// Zwraca `(opcjonalny prostokąt ikony, prostokąt etykiety)`.
    pub fn layout(
        &self,
        rect: Rect,
        theme: &Theme,
        text: &TextEngine,
        label: &str,
    ) -> (Option<Rect>, Rect) {
        let body = theme.typography.body.size;
        let label_w = text.measure(label, body, weight::NORMAL).width;
        let icon = theme.metrics.entry.icon;
        let gap = if self.icon { theme.spacing.sm } else { 0.0 };
        let icon_w = if self.icon { icon + gap } else { 0.0 };
        let total = icon_w + label_w;
        let start = rect.center().x - total / 2.0;
        let icon_rect = self
            .icon
            .then(|| Rect::from_xywh(start, rect.center().y - icon / 2.0, icon, icon));
        let label_rect = Rect::from_xywh(start + icon_w, rect.y(), label_w, rect.height());
        (icon_rect, label_rect)
    }
}

/// Obszar chwytu okna (`GtkWindowHandle`): rozstrzyga, czy punkt w pasku tytułu służy
/// do **przeciągania okna**, czy trafia w kontrolkę (wtedy nie jest uchwytem).
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowHandle;

impl WindowHandle {
    /// Czy `p` leży w `bar` i nie trafia w żadną z kontrolek.
    pub fn is_drag_region(bar: Rect, controls: &[Rect], p: Point) -> bool {
        bar.contains(p) && !controls.iter().any(|c| c.contains(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 800.0, 600.0)
    }

    #[test]
    fn toolbar_partitions() {
        let tv = ToolbarView::new().top(46.0).bottom(28.0);
        let r = tv.rects(rect());
        assert_eq!(r.top.unwrap().height(), 46.0);
        assert_eq!(r.bottom.unwrap().height(), 28.0);
        assert_eq!(r.content.y(), 46.0);
        assert_eq!(r.content.height(), 600.0 - 46.0 - 28.0);
    }

    #[test]
    fn window_handle_excludes_controls() {
        let bar = Rect::from_xywh(0.0, 0.0, 800.0, 46.0);
        let controls = [Rect::from_xywh(700.0, 8.0, 90.0, 30.0)];
        assert!(WindowHandle::is_drag_region(
            bar,
            &controls,
            Point::new(400.0, 20.0)
        ));
        assert!(!WindowHandle::is_drag_region(
            bar,
            &controls,
            Point::new(720.0, 20.0)
        ));
        assert!(!WindowHandle::is_drag_region(
            bar,
            &controls,
            Point::new(400.0, 200.0)
        ));
    }

    #[test]
    fn clamp_centers_and_limits() {
        let clamp = Clamp::new(400.0);
        let r = clamp.content_rect(rect());
        assert_eq!(r.width(), 400.0);
        assert_eq!(r.x(), 200.0);
        let small = clamp.content_rect(Rect::from_xywh(0.0, 0.0, 300.0, 100.0));
        assert_eq!(small.width(), 300.0);
    }

    #[test]
    fn wrap_box_wraps_and_measures() {
        let wrap = WrapBox::new().spacing(10.0).line_spacing(5.0);
        let sizes = [Size::new(120.0, 30.0); 5];
        let rects = wrap.layout(rect(), &sizes);
        assert_eq!(rects.len(), 5);
        // Zmieściło się 6 na linię w 800 px? (5*120 + 4*10 = 640 < 800) → jedna linia.
        assert_eq!(rects[0].y(), rects[4].y());
        let h = wrap.height(400.0, &sizes);
        // 400 px: 3 na linię (3*120+2*10=380), więc 2 linie.
        assert!((h - (30.0 + 5.0 + 30.0)).abs() < 1e-9);
    }

    #[test]
    fn breakpoint_active() {
        let bp = BreakpointBin::new(vec![400.0, 700.0]);
        assert_eq!(bp.active(300.0), 0);
        assert_eq!(bp.active(500.0), 1);
        assert_eq!(bp.active(900.0), 2);
    }
}
