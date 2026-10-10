//! Współdzielone ikony rysowane wektorowo (bez zależności od fontu).

use libluna_core::{Color, Point, Rect};

use crate::ctx::PaintCtx;

/// Strzałka w dół (chevron); środek w `center`, rozpiętość `size`, grubość `width`.
pub fn chevron_down(paint: &mut PaintCtx, color: Color, center: Point, size: f64, width: f64) {
    paint.scene.stroke_line(
        Point::new(center.x - size, center.y - size * 0.5),
        Point::new(center.x, center.y + size * 0.5),
        width,
        color,
    );
    paint.scene.stroke_line(
        Point::new(center.x, center.y + size * 0.5),
        Point::new(center.x + size, center.y - size * 0.5),
        width,
        color,
    );
}

/// Strzałka w górę (chevron); środek w `center`, rozpiętość `size`, grubość `width`.
pub fn chevron_up(paint: &mut PaintCtx, color: Color, center: Point, size: f64, width: f64) {
    paint.scene.stroke_line(
        Point::new(center.x - size, center.y + size * 0.5),
        Point::new(center.x, center.y - size * 0.5),
        width,
        color,
    );
    paint.scene.stroke_line(
        Point::new(center.x, center.y - size * 0.5),
        Point::new(center.x + size, center.y + size * 0.5),
        width,
        color,
    );
}

/// Strzałka w prawo (chevron); środek w `center`, rozpiętość `size`, grubość `width`.
pub fn chevron_right(paint: &mut PaintCtx, color: Color, center: Point, size: f64, width: f64) {
    paint.scene.stroke_line(
        Point::new(center.x - size * 0.45, center.y - size),
        Point::new(center.x + size * 0.45, center.y),
        width,
        color,
    );
    paint.scene.stroke_line(
        Point::new(center.x + size * 0.45, center.y),
        Point::new(center.x - size * 0.45, center.y + size),
        width,
        color,
    );
}

/// Strzałka w lewo (chevron); środek w `center`, rozpiętość `size`, grubość `width`.
pub fn chevron_left(paint: &mut PaintCtx, color: Color, center: Point, size: f64, width: f64) {
    paint.scene.stroke_line(
        Point::new(center.x + size * 0.45, center.y - size),
        Point::new(center.x - size * 0.45, center.y),
        width,
        color,
    );
    paint.scene.stroke_line(
        Point::new(center.x - size * 0.45, center.y),
        Point::new(center.x + size * 0.45, center.y + size),
        width,
        color,
    );
}

/// Lupka (wyszukiwanie); środek w `center`, bok `size`.
pub fn search(paint: &mut PaintCtx, color: Color, center: Point, size: f64) {
    let r = size * 0.34;
    let c = Point::new(center.x - size * 0.08, center.y - size * 0.08);
    paint.scene.stroke_rounded_rect(
        Rect::from_xywh(c.x - r, c.y - r, r * 2.0, r * 2.0),
        r,
        1.6,
        color,
    );
    let a = Point::new(c.x + r * 0.75, c.y + r * 0.75);
    paint.scene.stroke_line(
        a,
        Point::new(a.x + size * 0.28, a.y + size * 0.28),
        1.6,
        color,
    );
}

/// Ikona katalogu (folder) w prostokącie `box_`.
pub fn folder(paint: &mut PaintCtx, color: Color, box_: Rect) {
    let tab_h = box_.height() * 0.24;
    let tab_w = box_.width() * 0.46;
    paint.scene.fill_rounded_rect(
        Rect::from_xywh(box_.x(), box_.y(), tab_w, tab_h * 1.6),
        tab_h * 0.4,
        color,
    );
    paint.scene.fill_rounded_rect(
        Rect::from_xywh(
            box_.x(),
            box_.y() + tab_h * 0.9,
            box_.width(),
            box_.height() - tab_h * 0.9,
        ),
        box_.width() * 0.1,
        color,
    );
}

/// Ikona pliku (kartka z zagiętym rogiem) w prostokącie `box_`.
pub fn file(paint: &mut PaintCtx, color: Color, box_: Rect) {
    let w = box_.width();
    paint.scene.stroke_rounded_rect(box_, w * 0.12, 1.8, color);
    let fold = w * 0.34;
    paint.scene.stroke_line(
        Point::new(box_.right() - fold, box_.y()),
        Point::new(box_.right() - fold, box_.y() + fold),
        1.8,
        color,
    );
    paint.scene.stroke_line(
        Point::new(box_.right() - fold, box_.y() + fold),
        Point::new(box_.right(), box_.y() + fold),
        1.8,
        color,
    );
}

/// Krzyżyk (zamknięcie); środek w `box_`, ramię `arm`.
pub fn close(paint: &mut PaintCtx, color: Color, box_: Rect, arm: f64) {
    let c = box_.center();
    paint.scene.stroke_line(
        Point::new(c.x - arm, c.y - arm),
        Point::new(c.x + arm, c.y + arm),
        2.0,
        color,
    );
    paint.scene.stroke_line(
        Point::new(c.x + arm, c.y - arm),
        Point::new(c.x - arm, c.y + arm),
        2.0,
        color,
    );
}

/// Pozioma kreska (minimalizacja).
pub fn minimize(paint: &mut PaintCtx, color: Color, box_: Rect, arm: f64) {
    let c = box_.center();
    paint.scene.stroke_line(
        Point::new(c.x - arm, c.y),
        Point::new(c.x + arm, c.y),
        2.0,
        color,
    );
}

/// Kwadrat (maksymalizacja).
pub fn maximize(paint: &mut PaintCtx, color: Color, box_: Rect, half: f64) {
    let c = box_.center();
    paint.scene.stroke_rounded_rect(
        Rect::from_xywh(c.x - half, c.y - half, half * 2.0, half * 2.0),
        2.0,
        2.0,
        color,
    );
}

/// Oko (podgląd hasła); środek w `center`, półszerokość `size`.
pub fn eye(paint: &mut PaintCtx, color: Color, center: Point, size: f64) {
    use std::f64::consts::PI;
    let w = size;
    let h = size * 0.62;
    let n = 12;
    let mut prev = Point::new(center.x - w, center.y);
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let p = Point::new(center.x - w + 2.0 * w * t, center.y - h * (PI * t).sin());
        paint.scene.stroke_line(prev, p, 1.6, color);
        prev = p;
    }
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let p = Point::new(center.x - w + 2.0 * w * t, center.y + h * (PI * t).sin());
        paint.scene.stroke_line(prev, p, 1.6, color);
        prev = p;
    }
    let r = size * 0.22;
    paint.scene.fill_rounded_rect(
        Rect::from_xywh(center.x - r, center.y - r, r * 2.0, r * 2.0),
        r,
        color,
    );
}

/// Oko przekreślone (hasło ukryte).
pub fn eye_off(paint: &mut PaintCtx, color: Color, center: Point, size: f64) {
    eye(paint, color, center, size);
    paint.scene.stroke_line(
        Point::new(center.x - size * 0.9, center.y + size * 0.9),
        Point::new(center.x + size * 0.9, center.y - size * 0.9),
        1.6,
        color,
    );
}

/// Trzy poziome kreski (menu „hamburger”); środek w `center`, szerokość `size`, grubość `width`.
pub fn burger(paint: &mut PaintCtx, color: Color, center: Point, size: f64, width: f64) {
    for i in -1..=1 {
        let y = center.y + i as f64 * size * 0.42;
        paint.scene.stroke_line(
            Point::new(center.x - size, y),
            Point::new(center.x + size, y),
            width,
            color,
        );
    }
}
