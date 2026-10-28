//! Kontenery GTK4: `CenterBox`, `Grid`, `Paned`, `Stack`, `ActionBar`, `PopoverMenuBar`.
//!
//! `Box` w modelu libLuny to stos drzewa układu ([`crate::layout::Node::hbox`] /
//! [`crate::layout::Node::vbox`]); tutaj żyją pozostałe kontenery. Te, które są
//! wyłącznie geometrią (CenterBox, Grid), mają statyczne funkcje liczące prostokąty;
//! te ze stanem (Paned, Stack, PopoverMenuBar) rysują się i obsługują trafienia.

use libluna_core::style::weight;
use libluna_core::{Animation, Easing, Point, Rect, Size, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;
use crate::display::Orientation;

/// Trzyczęściowy układ: początek (lewo), środek (na środku), koniec (prawo).
#[derive(Debug, Clone, Copy, Default)]
pub struct CenterBox;

impl CenterBox {
    /// Dzieli `rect` na trzy części; środek jest wyśrodkowany względem całego prostokąta.
    pub fn parts(rect: Rect, start_w: f64, center_w: f64, end_w: f64) -> [Rect; 3] {
        let max = rect.width().max(0.0);
        let start_w = start_w.clamp(0.0, max);
        let end_w = end_w.clamp(0.0, max);
        let center_w = center_w.clamp(0.0, max);
        let start = Rect::from_xywh(rect.x(), rect.y(), start_w, rect.height());
        let end = Rect::from_xywh(rect.right() - end_w, rect.y(), end_w, rect.height());
        let center = Rect::from_xywh(
            rect.center().x - center_w / 2.0,
            rect.y(),
            center_w,
            rect.height(),
        );
        [start, center, end]
    }
}

/// Kontener o stałym położeniu dzieci (`GtkFixed`): dziecko umieszczane jest
/// we współrzędnych względem kontenera, niezależnie od reszty układu.
#[derive(Debug, Clone, Copy, Default)]
pub struct Fixed;

impl Fixed {
    /// Prostokąt dziecka o rozmiarze `w × h` w punkcie `(x, y)` względem `rect`.
    pub fn child(rect: Rect, x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect::from_xywh(rect.x() + x, rect.y() + y, w, h)
    }

    /// Przesuwa gotowy prostokąt dziecka o przesunięcie względne w `rect`.
    pub fn place(rect: Rect, child: Rect) -> Rect {
        Rect::from_xywh(
            rect.x() + child.x(),
            rect.y() + child.y(),
            child.width(),
            child.height(),
        )
    }
}

/// Okno na większą treść (`GtkViewport`): mapuje przesuniętą treść na obszar widoku.
#[derive(Debug, Clone, Copy, Default)]
pub struct Viewport;

impl Viewport {
    /// Prostokąt treści o rozmiarze `content`, przesunięty o `offset`, we współrzędnych `view`.
    pub fn content_rect(view: Rect, content: Size, offset: Point) -> Rect {
        Rect::from_xywh(
            view.x() - offset.x,
            view.y() - offset.y,
            content.width,
            content.height,
        )
    }

    /// Czy treść nie mieści się w widoku (po którejkolwiek osi).
    pub fn overflows(view: Rect, content: Size) -> bool {
        content.width > view.width() + 1e-9 || content.height > view.height() + 1e-9
    }
}

/// Siatka `rows × cols` z równymi komórkami i odstępem z motywu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    rows: usize,
    cols: usize,
}

impl Grid {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows: rows.max(1),
            cols: cols.max(1),
        }
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Wszystkie komórki w kolejności wierszami (row-major).
    pub fn cells(&self, rect: Rect, theme: &Theme) -> Vec<Rect> {
        let mut out = Vec::with_capacity(self.rows * self.cols);
        for r in 0..self.rows {
            for c in 0..self.cols {
                out.push(self.cell(rect, theme, r, c));
            }
        }
        out
    }

    /// Prostokąt komórki `(row, col)`.
    pub fn cell(&self, rect: Rect, theme: &Theme, row: usize, col: usize) -> Rect {
        let gap = theme.metrics.container.gap;
        let cw = ((rect.width() - gap * (self.cols as f64 - 1.0)) / self.cols as f64).max(0.0);
        let ch = ((rect.height() - gap * (self.rows as f64 - 1.0)) / self.rows as f64).max(0.0);
        Rect::from_xywh(
            rect.x() + col as f64 * (cw + gap),
            rect.y() + row as f64 * (ch + gap),
            cw,
            ch,
        )
    }

    /// Komórka pod punktem `p` (lub `None`).
    pub fn cell_at(&self, rect: Rect, theme: &Theme, p: Point) -> Option<(usize, usize)> {
        if !rect.contains(p) {
            return None;
        }
        let gap = theme.metrics.container.gap;
        let cw = ((rect.width() - gap * (self.cols as f64 - 1.0)) / self.cols as f64).max(0.0);
        let ch = ((rect.height() - gap * (self.rows as f64 - 1.0)) / self.rows as f64).max(0.0);
        let col = ((p.x - rect.x()) / (cw + gap)) as usize;
        let row = ((p.y - rect.y()) / (ch + gap)) as usize;
        Some((row.min(self.rows - 1), col.min(self.cols - 1)))
    }
}

/// Panel dzielony (dwa dzieci + przesuwany separator).
#[derive(Debug, Clone, Copy)]
pub struct Paned {
    orientation: Orientation,
    position: f64,
    min_first: f64,
    min_second: f64,
    hovered: bool,
    dragging: bool,
}

impl Default for Paned {
    fn default() -> Self {
        Self {
            orientation: Orientation::Horizontal,
            position: 0.5,
            min_first: 60.0,
            min_second: 60.0,
            hovered: false,
            dragging: false,
        }
    }
}

impl Paned {
    pub fn new(orientation: Orientation) -> Self {
        Self {
            orientation,
            ..Self::default()
        }
    }

    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    /// Położenie separatora jako ułamek długości głównej osi (0..1).
    pub fn position(&self) -> f64 {
        self.position
    }

    pub fn set_position(&mut self, fraction: f64) {
        self.position = fraction.clamp(0.0, 1.0);
    }

    pub fn min_sizes(mut self, first: f64, second: f64) -> Self {
        self.min_first = first.max(0.0);
        self.min_second = second.max(0.0);
        self
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    fn clamped_fraction(&self, avail: f64) -> f64 {
        if avail <= 0.0 {
            return 0.0;
        }
        let min_f = (self.min_first / avail).clamp(0.0, 1.0);
        let max_f = (1.0 - self.min_second / avail).clamp(0.0, 1.0);
        self.position.clamp(min_f.min(max_f), max_f.max(min_f))
    }

    /// Trzy prostokąty: pierwszy panel, separator, drugi panel.
    pub fn split(&self, rect: Rect, theme: &Theme) -> (Rect, Rect, Rect) {
        let thick = theme.metrics.container.separator;
        match self.orientation {
            Orientation::Horizontal => {
                let avail = (rect.width() - thick).max(0.0);
                let fw = avail * self.clamped_fraction(avail);
                let first = Rect::from_xywh(rect.x(), rect.y(), fw, rect.height());
                let sep = Rect::from_xywh(rect.x() + fw, rect.y(), thick, rect.height());
                let second = Rect::from_xywh(
                    rect.x() + fw + thick,
                    rect.y(),
                    (avail - fw).max(0.0),
                    rect.height(),
                );
                (first, sep, second)
            }
            Orientation::Vertical => {
                let avail = (rect.height() - thick).max(0.0);
                let fh = avail * self.clamped_fraction(avail);
                let first = Rect::from_xywh(rect.x(), rect.y(), rect.width(), fh);
                let sep = Rect::from_xywh(rect.x(), rect.y() + fh, rect.width(), thick);
                let second = Rect::from_xywh(
                    rect.x(),
                    rect.y() + fh + thick,
                    rect.width(),
                    (avail - fh).max(0.0),
                );
                (first, sep, second)
            }
        }
    }

    pub fn panes(&self, rect: Rect, theme: &Theme) -> (Rect, Rect) {
        let (a, _, b) = self.split(rect, theme);
        (a, b)
    }

    pub fn separator_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        self.split(rect, theme).1
    }

    /// Czy punkt trafia w separator (z tolerancją `hit_pad`).
    pub fn separator_hit(&self, rect: Rect, theme: &Theme, p: Point) -> bool {
        let sep = self.separator_rect(rect, theme);
        let pad = theme.metrics.container.hit_pad;
        match self.orientation {
            Orientation::Horizontal => Rect::from_xywh(
                sep.x() - pad,
                sep.y(),
                sep.width() + 2.0 * pad,
                sep.height(),
            )
            .contains(p),
            Orientation::Vertical => Rect::from_xywh(
                sep.x(),
                sep.y() - pad,
                sep.width(),
                sep.height() + 2.0 * pad,
            )
            .contains(p),
        }
    }

    /// Rozpoczyna przeciąganie separatora (jeśli punkt w niego trafia).
    pub fn begin_drag(&mut self, rect: Rect, theme: &Theme, p: Point) -> bool {
        if self.separator_hit(rect, theme, p) {
            self.dragging = true;
            true
        } else {
            false
        }
    }

    /// Aktualizuje położenie separatora w trakcie przeciągania.
    pub fn drag_to(&mut self, rect: Rect, theme: &Theme, p: Point) {
        if !self.dragging {
            return;
        }
        let thick = theme.metrics.container.separator;
        match self.orientation {
            Orientation::Horizontal => {
                let avail = (rect.width() - thick).max(0.0);
                if avail > 0.0 {
                    self.set_position((p.x - rect.x()) / avail);
                }
            }
            Orientation::Vertical => {
                let avail = (rect.height() - thick).max(0.0);
                if avail > 0.0 {
                    self.set_position((p.y - rect.y()) / avail);
                }
            }
        }
    }

    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let sep = self.separator_rect(rect, &theme);
        let active = self.dragging || self.hovered;
        let color = if active {
            theme.colors.accent
        } else {
            theme.colors.border
        };
        ctx.scene.fill_rect(sep, color);
        if active {
            // Uchwyt: trzy kropki na środku separatora.
            let c = sep.center();
            for i in -1..=1 {
                let dot = Rect::from_xywh(c.x - 1.5, c.y + i as f64 * 6.0 - 1.5, 3.0, 3.0);
                ctx.scene
                    .fill_rounded_rect(dot, 1.5, theme.colors.accent_fg);
            }
        }
    }
}

/// Sposób przejścia między stronami [`Stack`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StackTransition {
    /// Bez animacji (natychmiastowa zmiana).
    #[default]
    None,
    /// Wsuwanie w poziomie.
    SlideHorizontal,
    /// Wsuwanie w pionie.
    SlideVertical,
    /// Przenikanie (crossfade).
    Fade,
}

/// Pojedyncza klatka strony w trakcie przejścia: prostokąt i krycie (0..1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackFrame {
    pub rect: Rect,
    pub alpha: f64,
}

/// Stos stron: wiele dzieci, jedno widoczne (bez zakładek).
///
/// W odróżnieniu od [`TabView`](crate::TabView) nie ma paska zakładek — przełączaniem
/// steruje aplikacja (np. przez [`ViewSwitcher`](crate::ViewSwitcher)). Opcjonalnie zmiana
/// strony może być animowana ([`StackTransition`]); postęp animacji sterowany jest
/// z aplikacji przez [`Stack::advance`], a klatki wychodzącej/przychodzącej daje
/// [`Stack::frames`].
#[derive(Debug, Clone, Default)]
pub struct Stack {
    count: usize,
    visible: usize,
    previous: Option<usize>,
    transition: StackTransition,
    progress: Animation,
}

impl Stack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_count(count: usize) -> Self {
        Self {
            count,
            visible: 0,
            ..Self::default()
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn visible_index(&self) -> usize {
        self.visible
    }

    /// Indeks strony wychodzącej w trakcie animacji przejścia.
    pub fn previous_index(&self) -> Option<usize> {
        self.previous
    }

    /// Ustawia sposób przejścia między stronami.
    pub fn set_transition(&mut self, transition: StackTransition) {
        self.transition = transition;
    }

    pub fn transition(&self) -> StackTransition {
        self.transition
    }

    /// Ustawia krzywą wygładzania przejścia.
    pub fn set_transition_easing(&mut self, easing: Easing) {
        self.progress.set_easing(easing);
    }

    pub fn is_animating(&self) -> bool {
        self.previous.is_some() && self.progress.is_animating()
    }

    /// Ustawia widoczną stronę; przy włączonym przejściu startuje animację.
    pub fn set_visible_index(&mut self, index: usize) {
        if self.count == 0 {
            return;
        }
        let index = index.min(self.count - 1);
        if index == self.visible {
            return;
        }
        if self.transition != StackTransition::None {
            self.previous = Some(self.visible);
            self.progress.jump_to(0.0);
            self.progress.animate_to(1.0);
        }
        self.visible = index;
    }

    pub fn is_visible(&self, index: usize) -> bool {
        index == self.visible
    }

    pub fn push(&mut self) -> usize {
        self.count += 1;
        self.count - 1
    }

    /// Usuwa stronę (przycina wybór, jeśli trzeba).
    pub fn remove(&mut self, index: usize) {
        if index < self.count {
            self.count -= 1;
            if self.visible >= self.count && self.count > 0 {
                self.visible = self.count - 1;
            }
            self.previous = None;
        }
    }

    pub fn next(&mut self) {
        if self.count > 0 {
            self.set_visible_index((self.visible + 1) % self.count);
        }
    }

    pub fn prev(&mut self) {
        if self.count > 0 {
            self.set_visible_index((self.visible + self.count - 1) % self.count);
        }
    }

    /// Krok animacji przejścia; po jej zakończeniu czyści stronę wychodzącą.
    /// Zwraca `true`, gdy animacja trwa dalej.
    pub fn advance(&mut self, dt: f64, duration: f64) -> bool {
        if self.previous.is_none() {
            return false;
        }
        let animating = self.progress.advance(dt, duration);
        if !animating {
            self.previous = None;
        }
        animating
    }

    /// Klatki `(wychodząca, przychodząca)` dla bieżącego postępu przejścia.
    pub fn frames(&self, rect: Rect) -> (StackFrame, StackFrame) {
        let full = |alpha: f64| StackFrame { rect, alpha };
        if self.previous.is_none() {
            return (full(0.0), full(1.0));
        }
        let t = self.progress.value().clamp(0.0, 1.0);
        match self.transition {
            StackTransition::None => (full(0.0), full(1.0)),
            StackTransition::Fade => (full(1.0 - t), full(t)),
            StackTransition::SlideHorizontal => {
                let out = Rect::from_xywh(
                    rect.x() - rect.width() * t,
                    rect.y(),
                    rect.width(),
                    rect.height(),
                );
                let inc = Rect::from_xywh(
                    rect.x() + rect.width() * (1.0 - t),
                    rect.y(),
                    rect.width(),
                    rect.height(),
                );
                (
                    StackFrame {
                        rect: out,
                        alpha: 1.0,
                    },
                    StackFrame {
                        rect: inc,
                        alpha: 1.0,
                    },
                )
            }
            StackTransition::SlideVertical => {
                let out = Rect::from_xywh(
                    rect.x(),
                    rect.y() - rect.height() * t,
                    rect.width(),
                    rect.height(),
                );
                let inc = Rect::from_xywh(
                    rect.x(),
                    rect.y() + rect.height() * (1.0 - t),
                    rect.width(),
                    rect.height(),
                );
                (
                    StackFrame {
                        rect: out,
                        alpha: 1.0,
                    },
                    StackFrame {
                        rect: inc,
                        alpha: 1.0,
                    },
                )
            }
        }
    }
}

/// Pasek akcji (`ActionBar`): pasek u dołu/góry z akcjami po lewej i prawej.
#[derive(Debug, Clone, Copy, Default)]
pub struct ActionBar;

impl ActionBar {
    pub fn height(theme: &Theme) -> f64 {
        theme.metrics.button.height + theme.spacing.xs * 2.0
    }

    /// Prostokąty: `[akcje-początku, akcje-końca]` wewnątrz `rect`.
    pub fn parts(rect: Rect, theme: &Theme, start_w: f64, end_w: f64) -> [Rect; 2] {
        let pad = theme.spacing.md;
        let h = rect.height();
        let inner_w = (rect.width() - 2.0 * pad).max(0.0);
        let start_w = start_w.clamp(0.0, inner_w);
        let end_w = end_w.clamp(0.0, inner_w);
        [
            Rect::from_xywh(rect.x() + pad, rect.y(), start_w, h),
            Rect::from_xywh(rect.right() - pad - end_w, rect.y(), end_w, h),
        ]
    }

    pub fn hit(rect: Rect, p: Point) -> bool {
        rect.contains(p)
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        ctx.scene.fill_rect(rect, theme.colors.window_bg);
        let line = Rect::from_xywh(rect.x(), rect.top(), rect.width(), 1.0);
        ctx.scene.fill_rect(line, theme.colors.border);
    }
}

/// Pasek menu (`PopoverMenuBar`): rząd przycisków otwierających popovery.
#[derive(Debug, Clone, Default)]
pub struct PopoverMenuBar {
    labels: Vec<String>,
    open: Option<usize>,
    hovered: Option<usize>,
}

impl PopoverMenuBar {
    pub fn new<I, S>(labels: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            labels: labels.into_iter().map(Into::into).collect(),
            open: None,
            hovered: None,
        }
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn open_index(&self) -> Option<usize> {
        self.open
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn set_hovered(&mut self, hovered: Option<usize>) {
        self.hovered = hovered;
    }

    pub fn close(&mut self) {
        self.open = None;
    }

    /// Prostokąty przycisków, jeden za drugim, o szerokości dopasowanej do etykiety.
    pub fn buttons(&self, rect: Rect, text: &TextEngine, theme: &Theme) -> Vec<Rect> {
        let m = theme.metrics.button;
        let size = theme.typography.body.size;
        let mut x = rect.x();
        let mut out = Vec::with_capacity(self.labels.len());
        for label in &self.labels {
            let w = text.measure(label, size, weight::NORMAL).width + m.pad_x * 2.0;
            out.push(Rect::from_xywh(x, rect.y(), w, rect.height()));
            x += w;
        }
        out
    }

    pub fn index_at(
        &self,
        rect: Rect,
        text: &TextEngine,
        theme: &Theme,
        p: Point,
    ) -> Option<usize> {
        self.buttons(rect, text, theme)
            .iter()
            .position(|b| b.contains(p))
    }

    /// Obsługuje klik: otwiera popover danego przycisku (lub zamyka, gdy ten sam).
    pub fn click_at(
        &mut self,
        rect: Rect,
        text: &TextEngine,
        theme: &Theme,
        p: Point,
    ) -> Option<usize> {
        match self.index_at(rect, text, theme, p) {
            Some(i) => {
                self.open = if self.open == Some(i) { None } else { Some(i) };
                self.open
            }
            None => None,
        }
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let size = theme.typography.body.size;
        ctx.scene.fill_rect(rect, theme.colors.headerbar_bg);
        let text = ctx.text;
        for (i, b) in self.buttons(rect, text, &theme).iter().enumerate() {
            if self.open == Some(i) || self.hovered == Some(i) {
                let bg = if self.open == Some(i) {
                    theme.colors.shade.scale_alpha(1.8)
                } else {
                    theme.colors.shade
                };
                ctx.scene.fill_rounded_rect(*b, theme.radius.small, bg);
            }
            ctx.label_center(
                b.center().x,
                b.center().y,
                &self.labels[i],
                size,
                weight::NORMAL,
                theme.colors.text,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_render::TextEngine;

    fn theme() -> Theme {
        Theme::light()
    }

    #[test]
    fn fixed_offsets_children() {
        let rect = Rect::from_xywh(10.0, 20.0, 100.0, 100.0);
        let c = Fixed::child(rect, 5.0, 7.0, 30.0, 40.0);
        assert_eq!(c, Rect::from_xywh(15.0, 27.0, 30.0, 40.0));
        let p = Fixed::place(rect, Rect::from_xywh(1.0, 2.0, 3.0, 4.0));
        assert_eq!(p, Rect::from_xywh(11.0, 22.0, 3.0, 4.0));
    }

    #[test]
    fn viewport_maps_scrolled_content() {
        let view = Rect::from_xywh(0.0, 0.0, 100.0, 80.0);
        let content = Size::new(100.0, 300.0);
        assert!(Viewport::overflows(view, content));
        assert!(!Viewport::overflows(view, Size::new(100.0, 80.0)));
        let r = Viewport::content_rect(view, content, Point::new(0.0, 50.0));
        assert_eq!(r.y(), -50.0);
        assert_eq!(r.height(), 300.0);
    }

    #[test]
    fn center_box_places_three_parts() {
        let rect = Rect::from_xywh(0.0, 0.0, 300.0, 40.0);
        let [s, c, e] = CenterBox::parts(rect, 60.0, 100.0, 60.0);
        assert_eq!(s, Rect::from_xywh(0.0, 0.0, 60.0, 40.0));
        assert_eq!(e, Rect::from_xywh(240.0, 0.0, 60.0, 40.0));
        assert_eq!(c.center().x, 150.0);
        assert_eq!(c.width(), 100.0);
    }

    #[test]
    fn grid_cells_partition_with_gap() {
        let theme = theme();
        let rect = Rect::from_xywh(0.0, 0.0, 208.0, 108.0);
        let g = Grid::new(2, 3);
        let cells = g.cells(rect, &theme);
        assert_eq!(cells.len(), 6);
        let gap = theme.metrics.container.gap; // 8
        assert!((cells[0].width() - (208.0 - 2.0 * gap) / 3.0).abs() < 1e-9);
        assert_eq!(cells[0].x(), 0.0);
        assert_eq!(cells[1].x(), cells[0].right() + gap);
        // Ostatnia komórka dotyka prawego brzegu.
        assert!((cells[2].right() - 208.0).abs() < 1e-9);
    }

    #[test]
    fn grid_cell_at_maps_points() {
        let theme = theme();
        let rect = Rect::from_xywh(0.0, 0.0, 208.0, 108.0);
        let g = Grid::new(2, 3);
        assert_eq!(g.cell_at(rect, &theme, Point::new(5.0, 5.0)), Some((0, 0)));
        let c = g.cell(rect, &theme, 1, 2);
        assert_eq!(g.cell_at(rect, &theme, c.center()), Some((1, 2)));
        assert_eq!(g.cell_at(rect, &theme, Point::new(999.0, 5.0)), None);
    }

    #[test]
    fn paned_splits_and_drags() {
        let theme = theme();
        let rect = Rect::from_xywh(0.0, 0.0, 300.0, 100.0);
        let mut p = Paned::new(Orientation::Horizontal);
        let (a, b) = p.panes(rect, &theme);
        assert!((a.width() - b.width()).abs() <= theme.metrics.container.separator);
        let sep = p.separator_rect(rect, &theme);
        assert!(p.begin_drag(rect, &theme, sep.center()));
        assert!(p.is_dragging());
        p.drag_to(rect, &theme, Point::new(60.0, 50.0));
        p.end_drag();
        assert!(!p.is_dragging());
        assert!(p.position() > 0.0 && p.position() < 1.0);
        // Minimalne rozmiary ograniczają przeciągnięcie do krawędzi.
        p.set_position(0.0);
        let (a2, _) = p.panes(rect, &theme);
        assert!(a2.width() >= theme.metrics.container.separator);
    }

    #[test]
    fn paned_vertical_hit_zone() {
        let theme = theme();
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 300.0);
        let mut p = Paned::new(Orientation::Vertical);
        p.set_position(0.5);
        let sep = p.separator_rect(rect, &theme);
        assert!(p.separator_hit(rect, &theme, sep.center()));
        assert!(!p.separator_hit(rect, &theme, Point::new(10.0, 10.0)));
    }

    #[test]
    fn stack_cycles_and_clamps() {
        let mut s = Stack::with_count(3);
        assert_eq!(s.visible_index(), 0);
        s.next();
        assert_eq!(s.visible_index(), 1);
        s.prev();
        s.prev();
        assert_eq!(s.visible_index(), 2);
        s.set_visible_index(99);
        assert_eq!(s.visible_index(), 2);
        s.remove(2);
        assert_eq!(s.count(), 2);
        assert_eq!(s.visible_index(), 1);
    }

    #[test]
    fn stack_without_transition_hides_outgoing() {
        let s = Stack::with_count(2);
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 50.0);
        let (out, inc) = s.frames(rect);
        assert_eq!(out.alpha, 0.0);
        assert_eq!(inc.alpha, 1.0);
        assert!(!s.is_animating());
    }

    #[test]
    fn stack_slide_moves_frames() {
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 50.0);
        let mut s = Stack::with_count(2);
        s.set_transition(StackTransition::SlideHorizontal);
        s.set_visible_index(1);
        assert!(s.is_animating());
        assert_eq!(s.previous_index(), Some(0));
        let (out, inc) = s.frames(rect);
        assert_eq!(out.rect.x(), 0.0);
        assert_eq!(inc.rect.x(), 100.0);
        assert!(!s.advance(1.0, 1.0));
        assert!(!s.is_animating());
        assert_eq!(s.frames(rect).1.rect.x(), 0.0);
    }

    #[test]
    fn stack_fade_crossfades() {
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 50.0);
        let mut s = Stack::with_count(2);
        s.set_transition(StackTransition::Fade);
        s.set_visible_index(1);
        let (out, inc) = s.frames(rect);
        assert_eq!(out.alpha, 1.0);
        assert_eq!(inc.alpha, 0.0);
        assert!(s.advance(0.5, 1.0)); // EaseInOut w połowie = 0.5
        let (out2, inc2) = s.frames(rect);
        assert!((out2.alpha - 0.5).abs() < 1e-9);
        assert!((inc2.alpha - 0.5).abs() < 1e-9);
    }

    #[test]
    fn action_bar_parts_and_height() {
        let theme = theme();
        let rect = Rect::from_xywh(0.0, 0.0, 400.0, ActionBar::height(&theme));
        let [s, e] = ActionBar::parts(rect, &theme, 80.0, 120.0);
        assert_eq!(s.x(), theme.spacing.md);
        assert!((e.right() - (400.0 - theme.spacing.md)).abs() < 1e-9);
        assert!(ActionBar::hit(rect, rect.center()));
    }

    #[test]
    fn menu_bar_opens_and_toggles() {
        let theme = theme();
        let text = TextEngine::system();
        let rect = Rect::from_xywh(0.0, 0.0, 600.0, 34.0);
        let mut bar = PopoverMenuBar::new(["Plik", "Edycja", "Widok"]);
        let buttons = bar.buttons(rect, &text, &theme);
        assert_eq!(buttons.len(), 3);
        assert_eq!(
            bar.index_at(rect, &text, &theme, buttons[1].center()),
            Some(1)
        );
        assert_eq!(
            bar.click_at(rect, &text, &theme, buttons[1].center()),
            Some(1)
        );
        assert_eq!(bar.open_index(), Some(1));
        // Klik w ten sam przycisk zamyka.
        assert_eq!(bar.click_at(rect, &text, &theme, buttons[1].center()), None);
        assert!(!bar.is_open());
    }
}
