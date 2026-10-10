//! `Bin` — kontener jednego dziecka z wcięciem i wyrównaniem.
//!
//! Pomocnik układu (nie rysuje nic): [`Bin::content_rect`] wylicza prostokąt dziecka o zadanym
//! naturalnym rozmiarze wewnątrz prostokąta, zgodnie z wcięciem i wyrównaniem.

use libluna_core::{Insets, Rect, Size};

/// Wyrównanie wzdłuż osi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BinAlign {
    Start,
    #[default]
    Center,
    End,
}

/// Kontener jednego dziecka.
#[derive(Debug, Clone, Copy, Default)]
pub struct Bin {
    padding: Insets,
    halign: BinAlign,
    valign: BinAlign,
}

impl Bin {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn padding(mut self, padding: Insets) -> Self {
        self.padding = padding;
        self
    }

    pub fn halign(mut self, align: BinAlign) -> Self {
        self.halign = align;
        self
    }

    pub fn valign(mut self, align: BinAlign) -> Self {
        self.valign = align;
        self
    }

    pub fn align(mut self, halign: BinAlign, valign: BinAlign) -> Self {
        self.halign = halign;
        self.valign = valign;
        self
    }

    /// Prostokąt dziecka o rozmiarze `child` wewnątrz `rect`.
    pub fn content_rect(&self, rect: Rect, child: Size) -> Rect {
        let inner = rect.deflate(self.padding);
        let w = child.width.min(inner.width());
        let h = child.height.min(inner.height());
        let x = match self.halign {
            BinAlign::Start => inner.x(),
            BinAlign::Center => inner.x() + (inner.width() - w) / 2.0,
            BinAlign::End => inner.right() - w,
        };
        let y = match self.valign {
            BinAlign::Start => inner.y(),
            BinAlign::Center => inner.y() + (inner.height() - h) / 2.0,
            BinAlign::End => inner.bottom() - h,
        };
        Rect::from_xywh(x, y, w, h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centers_child() {
        let bin = Bin::new();
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 60.0);
        let r = bin.content_rect(rect, Size::new(40.0, 20.0));
        assert_eq!(r, Rect::from_xywh(30.0, 20.0, 40.0, 20.0));
    }

    #[test]
    fn end_align_and_padding() {
        let bin = Bin::new()
            .align(BinAlign::End, BinAlign::Start)
            .padding(Insets::splat(10.0));
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 60.0);
        let r = bin.content_rect(rect, Size::new(20.0, 20.0));
        assert_eq!(r.right(), 90.0);
        assert_eq!(r.y(), 10.0);
    }
}
