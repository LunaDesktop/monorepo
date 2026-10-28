//! Geometria w **pikselach logicznych** (niezależnych od gęstości ekranu).
//!
//! Konwersja do pikseli fizycznych odbywa się na granicy rysowania, przez
//! [`crate::scale::Scale`]. Wszystkie typy są `Copy` i tanie — można je swobodnie
//! przekazywać w silniku układu.

use std::ops::{Add, Sub};

/// Punkt w przestrzeni 2D.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl Add for Point {
    type Output = Point;
    fn add(self, rhs: Point) -> Point {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Point;
    fn sub(self, rhs: Point) -> Point {
        Point::new(self.x - rhs.x, self.y - rhs.y)
    }
}

/// Rozmiar (szerokość × wysokość).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub const ZERO: Size = Size {
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    /// Pusty, jeśli którykolwiek wymiar jest niedodatni.
    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

/// Prostokąt wyrównany do osi: pozycja + rozmiar.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const ZERO: Rect = Rect {
        origin: Point::ZERO,
        size: Size::ZERO,
    };

    pub const fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    pub const fn from_xywh(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(width, height),
        }
    }

    pub const fn from_size(size: Size) -> Self {
        Self {
            origin: Point::ZERO,
            size,
        }
    }

    pub fn x(&self) -> f64 {
        self.origin.x
    }
    pub fn y(&self) -> f64 {
        self.origin.y
    }
    pub fn width(&self) -> f64 {
        self.size.width
    }
    pub fn height(&self) -> f64 {
        self.size.height
    }
    pub fn left(&self) -> f64 {
        self.origin.x
    }
    pub fn top(&self) -> f64 {
        self.origin.y
    }
    pub fn right(&self) -> f64 {
        self.origin.x + self.size.width
    }
    pub fn bottom(&self) -> f64 {
        self.origin.y + self.size.height
    }

    pub fn center(&self) -> Point {
        Point::new(
            self.origin.x + self.size.width / 2.0,
            self.origin.y + self.size.height / 2.0,
        )
    }

    pub fn is_empty(&self) -> bool {
        self.size.is_empty()
    }

    /// Czy punkt leży wewnątrz prostokąta (krawędź prawa/dolna jest wyłączna).
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.left() && p.x < self.right() && p.y >= self.top() && p.y < self.bottom()
    }

    /// Czy dwa prostokąty mają wspólne pole o niezerowej powierzchni.
    pub fn intersects(&self, other: &Rect) -> bool {
        self.left() < other.right()
            && other.left() < self.right()
            && self.top() < other.bottom()
            && other.top() < self.bottom()
    }

    /// Część wspólna lub `None`, gdy brak przecięcia.
    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let left = self.left().max(other.left());
        let top = self.top().max(other.top());
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right > left && bottom > top {
            Some(Rect::from_xywh(left, top, right - left, bottom - top))
        } else {
            None
        }
    }

    /// Najmniejszy prostokąt zawierający oba (pusty operand jest ignorowany).
    pub fn union(&self, other: &Rect) -> Rect {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        let left = self.left().min(other.left());
        let top = self.top().min(other.top());
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Rect::from_xywh(left, top, right - left, bottom - top)
    }

    /// Powiększenie o marginesy (wartości dodatnie rosną na zewnątrz).
    pub fn inflate(&self, insets: Insets) -> Rect {
        Rect::from_xywh(
            self.left() - insets.left,
            self.top() - insets.top,
            self.width() + insets.left + insets.right,
            self.height() + insets.top + insets.bottom,
        )
    }

    /// Zmniejszenie o marginesy; wymiary nigdy nie schodzą poniżej zera.
    pub fn deflate(&self, insets: Insets) -> Rect {
        Rect::from_xywh(
            self.left() + insets.left,
            self.top() + insets.top,
            (self.width() - insets.left - insets.right).max(0.0),
            (self.height() - insets.top - insets.bottom).max(0.0),
        )
    }

    /// Przesunięcie o wektor.
    pub fn offset(&self, dx: f64, dy: f64) -> Rect {
        Rect::new(Point::new(self.x() + dx, self.y() + dy), self.size)
    }
}

/// Marginesy (góra/prawo/dół/lewo) — używane na padding, border i spacing.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Insets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Insets {
    pub const ZERO: Insets = Insets {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    pub const fn new(top: f64, right: f64, bottom: f64, left: f64) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    pub const fn splat(value: f64) -> Self {
        Self::new(value, value, value, value)
    }

    pub const fn symmetric(vertical: f64, horizontal: f64) -> Self {
        Self::new(vertical, horizontal, vertical, horizontal)
    }

    pub const fn horizontal(&self) -> f64 {
        self.left + self.right
    }

    pub const fn vertical(&self) -> f64 {
        self.top + self.bottom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_arithmetic() {
        let a = Point::new(1.0, 2.0);
        let b = Point::new(3.0, 5.0);
        assert_eq!(a + b, Point::new(4.0, 7.0));
        assert_eq!(b - a, Point::new(2.0, 3.0));
    }

    #[test]
    fn rect_contains_uses_half_open_bounds() {
        let r = Rect::from_xywh(10.0, 10.0, 100.0, 50.0);
        assert!(r.contains(Point::new(10.0, 10.0)));
        assert!(r.contains(Point::new(109.9, 59.9)));
        assert!(!r.contains(Point::new(110.0, 30.0)));
        assert!(!r.contains(Point::new(50.0, 60.0)));
    }

    #[test]
    fn rect_intersection_and_none() {
        let a = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
        let b = Rect::from_xywh(5.0, 5.0, 10.0, 10.0);
        assert_eq!(
            a.intersection(&b),
            Some(Rect::from_xywh(5.0, 5.0, 5.0, 5.0))
        );

        let c = Rect::from_xywh(20.0, 20.0, 1.0, 1.0);
        assert_eq!(a.intersection(&c), None);
        assert!(!a.intersects(&c));
    }

    #[test]
    fn rect_union_ignores_empty() {
        let a = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
        assert_eq!(a.union(&Rect::ZERO), a);
        let b = Rect::from_xywh(5.0, 5.0, 10.0, 10.0);
        assert_eq!(a.union(&b), Rect::from_xywh(0.0, 0.0, 15.0, 15.0));
    }

    #[test]
    fn inflate_deflate_roundtrip() {
        let r = Rect::from_xywh(10.0, 10.0, 100.0, 100.0);
        let m = Insets::splat(5.0);
        assert_eq!(r.inflate(m).deflate(m), r);
    }

    #[test]
    fn deflate_clamps_to_zero() {
        let r = Rect::from_xywh(0.0, 0.0, 3.0, 3.0);
        let m = Insets::splat(10.0);
        let d = r.deflate(m);
        assert_eq!(d.width(), 0.0);
        assert_eq!(d.height(), 0.0);
    }
}
