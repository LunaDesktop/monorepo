//! `layout` — dwuprzebiegowy układ (`measure` → `allocate`) w modelu retencyjnym.
//!
//! Zamiast ręcznie liczyć prostokąty (jak w `luna-demo`), aplikacja buduje **drzewo**
//! [`Node`] (stosy, wcięcia, liście), a silnik:
//!
//! 1. **mierzy** je ([`Node::measure`]) względem [`Constraints`] (min/max) — ile miejsca
//!    chce poddrzewo (tekst mierzy się realną czcionką przez [`LayoutCtx::text`]),
//! 2. **rozmieszcza** ([`Node::allocate`]) w zadanym prostokącie — dzieci dzielą przestrzeń,
//!    [`Node::Expanded`]/[`Node::Spacer`] pochłaniają nadmiar wzdłuż osi stosu.
//!
//! [`Node::allocate`] zapisuje prostokąty **liści** w kolejności DFS; aplikacja mapuje je na
//! konkretne widgety (te same, co dotąd: `paint` przyjmuje prostokąt).
//!
//! ```
//! use libluna_core::{Rect, Size, Insets, Theme};
//! use libluna_render::TextEngine;
//! use libluna_widgets::layout::{Layout, LayoutCtx, Node};
//!
//! let theme = Theme::light();
//! let text = TextEngine::system();
//! let ctx = LayoutCtx { theme: &theme, text: &text };
//! let tree = Node::padding(
//!     Insets::splat(12.0),
//!     Node::vbox(vec![
//!         Node::fixed(0.0, 40.0),
//!         Node::spacer(),
//!         Node::fixed(0.0, 32.0),
//!     ])
//!     .gap(8.0),
//! );
//! let rects = Layout::new(tree).fit(Rect::from_size(Size::new(320.0, 240.0)), &ctx);
//! assert_eq!(rects.len(), 3);
//! ```

use libluna_core::style::FontStyle;
use libluna_core::{Insets, Rect, Size, Theme};
use libluna_render::TextEngine;

/// Ograniczenia pomiaru: dopuszczalny zakres rozmiaru (min/max).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Constraints {
    pub min: Size,
    pub max: Size,
}

impl Constraints {
    /// Luźne: minimum zerowe, maksimum podane (typowe dla dziecka stosu).
    pub const fn loose(max: Size) -> Self {
        Self {
            min: Size::ZERO,
            max,
        }
    }

    /// Ciasne: wymiar wymuszony (`min == max`).
    pub const fn tight(size: Size) -> Self {
        Self {
            min: size,
            max: size,
        }
    }

    /// Bez ograniczeń (maksimum = nieskończoność).
    pub fn unconstrained() -> Self {
        Self {
            min: Size::ZERO,
            max: Size::new(f64::INFINITY, f64::INFINITY),
        }
    }

    /// Zmniejsza granice o marginesy (np. dla dziecka [`Node::Padding`]).
    pub fn deflate(&self, insets: Insets) -> Constraints {
        Constraints {
            min: Size::new(
                (self.min.width - insets.horizontal()).max(0.0),
                (self.min.height - insets.vertical()).max(0.0),
            ),
            max: Size::new(
                (self.max.width - insets.horizontal()).max(0.0),
                (self.max.height - insets.vertical()).max(0.0),
            ),
        }
    }

    /// Przycina rozmiar do dopuszczalnego zakresu.
    pub fn constrain(&self, size: Size) -> Size {
        Size::new(
            size.width.clamp(self.min.width, self.max.width),
            size.height.clamp(self.min.height, self.max.height),
        )
    }
}

/// Kontekst układu: motyw (odstępy) i silnik tekstu (pomiar napisów).
pub struct LayoutCtx<'a> {
    pub theme: &'a Theme,
    pub text: &'a TextEngine,
}

/// Stos: lista dzieci wzdłuż osi + odstęp między nimi.
#[derive(Debug, Clone)]
pub struct Stack {
    pub children: Vec<Node>,
    pub gap: f64,
}

/// Węzeł drzewa układu.
#[derive(Debug, Clone)]
pub enum Node {
    /// Liść o zadanym rozmiarze (wskazówka pomiaru; w stosie oś poprzeczna jest rozciągana).
    Fixed(Size),
    /// Liść tekstowy (mierzy się realną czcionką).
    Label { text: String, style: FontStyle },
    /// Rozciągliwa pustka (pochłania nadmiar wzdłuż osi stosu).
    Spacer,
    /// Stos pionowy (dzieci jedna pod drugą).
    VBox(Stack),
    /// Stos poziomy (dzieci jedna za drugą).
    HBox(Stack),
    /// Wcięcie wokół dziecka.
    Padding { insets: Insets, child: Box<Node> },
    /// Dziecko zajmujące nadmiar wzdłuż osi stosu.
    Expanded(Box<Node>),
    /// Dziecko o naturalnym rozmiarze, wyśrodkowane w przydzielonym prostokącie.
    Center(Box<Node>),
}

impl Node {
    pub fn fixed(width: f64, height: f64) -> Self {
        Node::Fixed(Size::new(width, height))
    }

    pub fn label(text: impl Into<String>, style: FontStyle) -> Self {
        Node::Label {
            text: text.into(),
            style,
        }
    }

    pub fn spacer() -> Self {
        Node::Spacer
    }

    pub fn vbox(children: Vec<Node>) -> Self {
        Node::VBox(Stack { children, gap: 0.0 })
    }

    pub fn hbox(children: Vec<Node>) -> Self {
        Node::HBox(Stack { children, gap: 0.0 })
    }

    pub fn padding(insets: Insets, child: Node) -> Self {
        Node::Padding {
            insets,
            child: Box::new(child),
        }
    }

    pub fn expanded(child: Node) -> Self {
        Node::Expanded(Box::new(child))
    }

    pub fn center(child: Node) -> Self {
        Node::Center(Box::new(child))
    }

    /// Ustawia odstęp stosu (dla [`Node::VBox`]/[`Node::HBox`]; inaczej bez efektu).
    pub fn gap(mut self, gap: f64) -> Self {
        match &mut self {
            Node::VBox(stack) | Node::HBox(stack) => stack.gap = gap,
            _ => {}
        }
        self
    }

    /// Opakowuje węzeł we wcięcie.
    pub fn padded(self, insets: Insets) -> Self {
        Node::padding(insets, self)
    }

    fn is_flexible(&self) -> bool {
        matches!(self, Node::Spacer | Node::Expanded(_))
    }

    /// Naturalny rozmiar poddrzewa dla zadanych ograniczeń.
    pub fn measure(&self, c: Constraints, ctx: &LayoutCtx) -> Size {
        let size = match self {
            Node::Fixed(s) => *s,
            Node::Label { text, style } => {
                let w = ctx.text.measure(text, style.size, style.weight).width;
                Size::new(w, style.line_height)
            }
            Node::Spacer => Size::ZERO,
            Node::VBox(stack) => stack_size(stack, c, ctx, true),
            Node::HBox(stack) => stack_size(stack, c, ctx, false),
            Node::Padding { insets, child } => {
                let inner = child.measure(c.deflate(*insets), ctx);
                Size::new(
                    inner.width + insets.horizontal(),
                    inner.height + insets.vertical(),
                )
            }
            Node::Expanded(child) | Node::Center(child) => child.measure(c, ctx),
        };
        c.constrain(size)
    }

    /// Odwiedza liście poddrzewa z ich prostokątami (kolejność DFS).
    ///
    /// Wygodne do rysowania: dla każdego liścia wiadomo, *jaki* to węzeł i *gdzie* leży.
    pub fn visit(&self, rect: Rect, ctx: &LayoutCtx, f: &mut impl FnMut(&Node, Rect)) {
        match self {
            Node::Fixed(_) | Node::Label { .. } | Node::Spacer => f(self, rect),
            Node::Padding { insets, child } => child.visit(rect.deflate(*insets), ctx, f),
            Node::Expanded(child) => child.visit(rect, ctx, f),
            Node::Center(child) => {
                let natural = child.measure(Constraints::loose(rect.size), ctx);
                let w = natural.width.min(rect.width());
                let h = natural.height.min(rect.height());
                let child_rect = Rect::from_xywh(
                    rect.x() + (rect.width() - w) / 2.0,
                    rect.y() + (rect.height() - h) / 2.0,
                    w,
                    h,
                );
                child.visit(child_rect, ctx, f);
            }
            Node::VBox(stack) => visit_stack(stack, rect, ctx, f, true),
            Node::HBox(stack) => visit_stack(stack, rect, ctx, f, false),
        }
    }

    /// Rozmieszcza poddrzewo w `rect`, zapisując prostokąty liści (DFS) do `out`.
    pub fn allocate(&self, rect: Rect, ctx: &LayoutCtx, out: &mut Vec<Rect>) {
        self.visit(rect, ctx, &mut |_node, r| out.push(r));
    }
}

fn stack_size(stack: &Stack, c: Constraints, ctx: &LayoutCtx, vertical: bool) -> Size {
    let child_c = Constraints::loose(c.max);
    let mut cross: f64 = 0.0;
    let mut main: f64 = 0.0;
    for (i, ch) in stack.children.iter().enumerate() {
        let s = ch.measure(child_c, ctx);
        let (m, cr) = if vertical {
            (s.height, s.width)
        } else {
            (s.width, s.height)
        };
        main += m;
        cross = cross.max(cr);
        if i + 1 < stack.children.len() {
            main += stack.gap;
        }
    }
    if vertical {
        Size::new(cross, main)
    } else {
        Size::new(main, cross)
    }
}

fn visit_stack(
    stack: &Stack,
    rect: Rect,
    ctx: &LayoutCtx,
    f: &mut impl FnMut(&Node, Rect),
    vertical: bool,
) {
    let n = stack.children.len();
    if n == 0 {
        return;
    }
    let meas_c = Constraints::loose(rect.size);
    let naturals: Vec<Size> = stack
        .children
        .iter()
        .map(|c| c.measure(meas_c, ctx))
        .collect();

    let (avail, cross) = if vertical {
        (rect.height(), rect.width())
    } else {
        (rect.width(), rect.height())
    };

    let gap_total = stack.gap * (n as f64 - 1.0);
    let used: f64 = naturals
        .iter()
        .map(|s| if vertical { s.height } else { s.width })
        .sum::<f64>()
        + gap_total;
    let extra = (avail - used).max(0.0);
    let flex_count = stack.children.iter().filter(|c| c.is_flexible()).count() as f64;
    let bonus = if flex_count > 0.0 {
        extra / flex_count
    } else {
        0.0
    };

    let mut cursor = if vertical { rect.y() } else { rect.x() };
    for (i, ch) in stack.children.iter().enumerate() {
        let natural_main = if vertical {
            naturals[i].height
        } else {
            naturals[i].width
        };
        let main = natural_main + if ch.is_flexible() { bonus } else { 0.0 };
        let child_rect = if vertical {
            Rect::from_xywh(rect.x(), cursor, cross, main)
        } else {
            Rect::from_xywh(cursor, rect.y(), main, cross)
        };
        ch.visit(child_rect, ctx, f);
        cursor += main + stack.gap;
    }
}

/// Gotowy układ (korzeń drzewa) — wygodne wejście dla aplikacji.
#[derive(Debug, Clone)]
pub struct Layout {
    root: Node,
}

impl Layout {
    pub fn new(root: Node) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Node {
        &self.root
    }

    /// Pożądany rozmiar drzewa dla danych ograniczeń (przebieg `measure`).
    pub fn desired_size(&self, c: Constraints, ctx: &LayoutCtx) -> Size {
        self.root.measure(c, ctx)
    }

    /// Dwuprzebiegowe złożenie: `measure`, a następnie `allocate` w `rect`.
    /// Zwraca prostokąty liści w kolejności DFS.
    pub fn fit(&self, rect: Rect, ctx: &LayoutCtx) -> Vec<Rect> {
        let _desired = self.root.measure(Constraints::loose(rect.size), ctx);
        let mut out = Vec::new();
        self.root.allocate(rect, ctx, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::light()
    }

    #[test]
    fn constraints_clamp() {
        let c = Constraints::loose(Size::new(100.0, 50.0));
        assert_eq!(c.constrain(Size::new(200.0, 10.0)), Size::new(100.0, 10.0));
        let t = Constraints::tight(Size::new(40.0, 40.0));
        assert_eq!(t.constrain(Size::new(0.0, 0.0)), Size::new(40.0, 40.0));
    }

    #[test]
    fn vbox_stacks_with_gap_and_stretches_cross_axis() {
        let theme = theme();
        let text = TextEngine::system();
        let ctx = LayoutCtx {
            theme: &theme,
            text: &text,
        };
        let node = Node::vbox(vec![Node::fixed(10.0, 20.0), Node::fixed(10.0, 30.0)]).gap(5.0);
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 200.0);
        let mut out = Vec::new();
        node.allocate(rect, &ctx, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], Rect::from_xywh(0.0, 0.0, 100.0, 20.0));
        assert_eq!(out[1], Rect::from_xywh(0.0, 25.0, 100.0, 30.0));
    }

    #[test]
    fn spacer_absorbs_extra_in_hbox() {
        let theme = theme();
        let text = TextEngine::system();
        let ctx = LayoutCtx {
            theme: &theme,
            text: &text,
        };
        let node = Node::hbox(vec![
            Node::fixed(10.0, 10.0),
            Node::spacer(),
            Node::fixed(10.0, 10.0),
        ]);
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 10.0);
        let mut out = Vec::new();
        node.allocate(rect, &ctx, &mut out);
        assert_eq!(out[0], Rect::from_xywh(0.0, 0.0, 10.0, 10.0));
        assert_eq!(out[1], Rect::from_xywh(10.0, 0.0, 80.0, 10.0));
        assert_eq!(out[2], Rect::from_xywh(90.0, 0.0, 10.0, 10.0));
    }

    #[test]
    fn expanded_grows_along_main_axis() {
        let theme = theme();
        let text = TextEngine::system();
        let ctx = LayoutCtx {
            theme: &theme,
            text: &text,
        };
        let node = Node::vbox(vec![
            Node::fixed(0.0, 10.0),
            Node::expanded(Node::fixed(0.0, 10.0)),
        ]);
        let rect = Rect::from_xywh(0.0, 0.0, 50.0, 100.0);
        let mut out = Vec::new();
        node.allocate(rect, &ctx, &mut out);
        assert_eq!(out[0], Rect::from_xywh(0.0, 0.0, 50.0, 10.0));
        assert_eq!(out[1], Rect::from_xywh(0.0, 10.0, 50.0, 90.0));
    }

    #[test]
    fn padding_insets_child() {
        let theme = theme();
        let text = TextEngine::system();
        let ctx = LayoutCtx {
            theme: &theme,
            text: &text,
        };
        let node = Node::padding(
            Insets::splat(5.0),
            Node::vbox(vec![Node::fixed(0.0, 10.0), Node::fixed(0.0, 10.0)]),
        );
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
        let mut out = Vec::new();
        node.allocate(rect, &ctx, &mut out);
        assert_eq!(out[0], Rect::from_xywh(5.0, 5.0, 90.0, 10.0));
        assert_eq!(out[1], Rect::from_xywh(5.0, 15.0, 90.0, 10.0));
    }

    #[test]
    fn center_keeps_natural_size() {
        let theme = theme();
        let text = TextEngine::system();
        let ctx = LayoutCtx {
            theme: &theme,
            text: &text,
        };
        let node = Node::center(Node::fixed(40.0, 24.0));
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 60.0);
        let mut out = Vec::new();
        node.allocate(rect, &ctx, &mut out);
        assert_eq!(out[0], Rect::from_xywh(30.0, 18.0, 40.0, 24.0));
    }

    #[test]
    fn label_measures_text() {
        let theme = theme();
        let text = TextEngine::system();
        let ctx = LayoutCtx {
            theme: &theme,
            text: &text,
        };
        let node = Node::label("Przykładowy napis", theme.typography.body);
        let size = node.measure(Constraints::unconstrained(), &ctx);
        assert!(size.width > 0.0);
        assert_eq!(size.height, theme.typography.body.line_height);
    }

    #[test]
    fn fit_collects_leaf_rects() {
        let theme = theme();
        let text = TextEngine::system();
        let ctx = LayoutCtx {
            theme: &theme,
            text: &text,
        };
        let tree = Node::padding(
            Insets::splat(12.0),
            Node::vbox(vec![
                Node::fixed(0.0, 40.0),
                Node::spacer(),
                Node::fixed(0.0, 32.0),
            ])
            .gap(8.0),
        );
        let rects = Layout::new(tree).fit(Rect::from_size(Size::new(320.0, 240.0)), &ctx);
        assert_eq!(rects.len(), 3);
        // Ostatni liść dotyka dolnej krawędzi wewnątrz wcięcia.
        assert!((rects[2].bottom() - (240.0 - 12.0)).abs() < 1e-9);
    }
}
