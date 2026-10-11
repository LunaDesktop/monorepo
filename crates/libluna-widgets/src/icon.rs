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

// ─────────────────────────────────────────────────────────────────────────────
// Nazwany pakiet ikon symbolicznych — rysowany wektorowo, w stylu Luny.
// ─────────────────────────────────────────────────────────────────────────────

/// Nazwa ikony z pakietu Luny.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    Menu,
    Search,
    Files,
    Settings,
    Terminal,
    Music,
    Star,
    Clock,
    Store,
    Edit,
    Wifi,
    Volume,
    Battery,
    Power,
    Grid,
    List,
    Carousel,
    ChevronLeft,
    ChevronRight,
    Close,
    Minimize,
    Maximize,
    Check,
    Plus,
    Home,
    User,
    Info,
    App,
}

impl IconName {
    /// Wszystkie ikony z pakietu (kolejność = kolejność w galerii).
    pub const ALL: [IconName; 28] = [
        IconName::Menu,
        IconName::Search,
        IconName::Files,
        IconName::Settings,
        IconName::Terminal,
        IconName::Music,
        IconName::Star,
        IconName::Clock,
        IconName::Store,
        IconName::Edit,
        IconName::Wifi,
        IconName::Volume,
        IconName::Battery,
        IconName::Power,
        IconName::Grid,
        IconName::List,
        IconName::Carousel,
        IconName::ChevronLeft,
        IconName::ChevronRight,
        IconName::Close,
        IconName::Minimize,
        IconName::Maximize,
        IconName::Check,
        IconName::Plus,
        IconName::Home,
        IconName::User,
        IconName::Info,
        IconName::App,
    ];

    /// Nazwa ikony (do galerii/diagnostyki).
    pub fn name(self) -> &'static str {
        match self {
            IconName::Menu => "Menu",
            IconName::Search => "Search",
            IconName::Files => "Files",
            IconName::Settings => "Settings",
            IconName::Terminal => "Terminal",
            IconName::Music => "Music",
            IconName::Star => "Star",
            IconName::Clock => "Clock",
            IconName::Store => "Store",
            IconName::Edit => "Edit",
            IconName::Wifi => "Wifi",
            IconName::Volume => "Volume",
            IconName::Battery => "Battery",
            IconName::Power => "Power",
            IconName::Grid => "Grid",
            IconName::List => "List",
            IconName::Carousel => "Carousel",
            IconName::ChevronLeft => "ChevronLeft",
            IconName::ChevronRight => "ChevronRight",
            IconName::Close => "Close",
            IconName::Minimize => "Minimize",
            IconName::Maximize => "Maximize",
            IconName::Check => "Check",
            IconName::Plus => "Plus",
            IconName::Home => "Home",
            IconName::User => "User",
            IconName::Info => "Info",
            IconName::App => "App",
        }
    }
}

/// Współrzędne w siatce 24×24 mapowane na prostokąt docelowy.
struct Grid {
    ox: f64,
    oy: f64,
    s: f64,
}

impl Grid {
    fn new(rect: Rect) -> Self {
        let side = rect.width().min(rect.height()).max(1.0);
        let s = side / 24.0;
        Self {
            ox: rect.x() + (rect.width() - 24.0 * s) / 2.0,
            oy: rect.y() + (rect.height() - 24.0 * s) / 2.0,
            s,
        }
    }

    fn p(&self, x: f64, y: f64) -> Point {
        Point::new(self.ox + x * self.s, self.oy + y * self.s)
    }

    fn d(&self, v: f64) -> f64 {
        v * self.s
    }
}

/// Punkty łuku (środek `c`, promień `r`, kąty w stopniach, y w dół).
fn arc(c: Point, r: f64, a0: f64, a1: f64, segs: usize) -> Vec<Point> {
    (0..=segs)
        .map(|i| {
            let t = a0 + (a1 - a0) * i as f64 / segs as f64;
            let a = t.to_radians();
            Point::new(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect()
}

/// Rysuje ikonę z pakietu w prostokącie `rect` w zadanym kolorze (styl symboliczny).
pub fn paint_icon(paint: &mut PaintCtx, name: IconName, rect: Rect, color: Color) {
    let g = Grid::new(rect);
    let w = g.d(2.0);
    let scene = &mut *paint.scene;

    match name {
        IconName::Menu => {
            for y in [6.5, 12.0, 17.5] {
                scene.stroke_line(g.p(4.0, y), g.p(20.0, y), w, color);
            }
        }
        IconName::Search => {
            scene.stroke_circle(g.p(10.5, 10.5), g.d(6.5), w, color);
            scene.stroke_line(g.p(15.3, 15.3), g.p(20.5, 20.5), w, color);
        }
        IconName::Files => {
            scene.fill_polygon(
                &[
                    g.p(3.0, 6.5),
                    g.p(9.0, 6.5),
                    g.p(11.0, 8.8),
                    g.p(21.0, 8.8),
                    g.p(21.0, 19.5),
                    g.p(3.0, 19.5),
                ],
                color,
            );
        }
        IconName::Settings => {
            scene.stroke_circle(g.p(12.0, 12.0), g.d(3.4), w, color);
            for k in 0..8 {
                let a = (k as f64 * 45.0).to_radians();
                let (c, s) = (a.cos(), a.sin());
                scene.stroke_line(
                    g.p(12.0 + 5.2 * c, 12.0 + 5.2 * s),
                    g.p(12.0 + 8.4 * c, 12.0 + 8.4 * s),
                    g.d(2.2),
                    color,
                );
            }
        }
        IconName::Terminal => {
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(3.0, 4.5).x, g.p(3.0, 4.5).y, g.d(18.0), g.d(15.0)),
                g.d(2.5),
                w,
                color,
            );
            scene.stroke_polyline(&[g.p(7.0, 10.0), g.p(9.5, 12.0), g.p(7.0, 14.0)], w, color);
            scene.stroke_line(g.p(12.0, 15.0), g.p(16.0, 15.0), w, color);
        }
        IconName::Music => {
            scene.stroke_circle(g.p(9.5, 17.0), g.d(2.6), w, color);
            scene.stroke_polyline(&[g.p(12.0, 17.0), g.p(12.0, 6.5), g.p(17.0, 8.0)], w, color);
        }
        IconName::Star => {
            let pts: Vec<Point> = (0..10)
                .map(|i| {
                    let a = (-90.0 + i as f64 * 36.0).to_radians();
                    let r = if i % 2 == 0 { 9.0 } else { 3.8 };
                    g.p(12.0 + r * a.cos(), 12.5 + r * a.sin())
                })
                .collect();
            scene.fill_polygon(&pts, color);
        }
        IconName::Clock => {
            scene.stroke_circle(g.p(12.0, 12.0), g.d(8.0), w, color);
            scene.stroke_polyline(&[g.p(12.0, 12.0), g.p(12.0, 7.5)], w, color);
            scene.stroke_polyline(&[g.p(12.0, 12.0), g.p(16.0, 13.8)], w, color);
        }
        IconName::Store => {
            scene.fill_polygon(
                &[
                    g.p(5.5, 8.0),
                    g.p(18.5, 8.0),
                    g.p(17.5, 20.0),
                    g.p(6.5, 20.0),
                ],
                color,
            );
            scene.stroke_polyline(
                &[g.p(9.0, 8.0), g.p(9.0, 6.0), g.p(15.0, 6.0), g.p(15.0, 8.0)],
                g.d(1.8),
                color,
            );
        }
        IconName::Edit => {
            scene.fill_polygon(
                &[
                    g.p(15.5, 3.5),
                    g.p(20.5, 8.5),
                    g.p(9.0, 20.0),
                    g.p(4.0, 21.0),
                    g.p(5.0, 16.0),
                ],
                color,
            );
        }
        IconName::Wifi => {
            scene.fill_circle(g.p(12.0, 18.0), g.d(1.4), color);
            for r in [4.0, 7.5, 11.0] {
                scene.stroke_polyline(
                    &arc(g.p(12.0, 18.0), g.d(r), 200.0, 340.0, 10),
                    g.d(1.9),
                    color,
                );
            }
        }
        IconName::Volume => {
            scene.fill_polygon(
                &[
                    g.p(3.0, 9.5),
                    g.p(7.0, 9.5),
                    g.p(11.0, 5.0),
                    g.p(11.0, 19.0),
                    g.p(7.0, 14.5),
                    g.p(3.0, 14.5),
                ],
                color,
            );
            for r in [5.0, 8.0] {
                scene.stroke_polyline(
                    &arc(g.p(11.0, 12.0), g.d(r), -50.0, 50.0, 8),
                    g.d(1.9),
                    color,
                );
            }
        }
        IconName::Battery => {
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(2.0, 7.0).x, g.p(2.0, 7.0).y, g.d(17.0), g.d(10.0)),
                g.d(2.5),
                w,
                color,
            );
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(19.5, 10.5).x, g.p(19.5, 10.5).y, g.d(2.5), g.d(3.0)),
                g.d(1.0),
                color,
            );
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(4.5, 9.5).x, g.p(4.5, 9.5).y, g.d(9.5), g.d(5.0)),
                g.d(1.2),
                color,
            );
        }
        IconName::Power => {
            scene.stroke_polyline(&arc(g.p(12.0, 12.0), g.d(7.5), -60.0, 240.0, 28), w, color);
            scene.stroke_line(g.p(12.0, 3.5), g.p(12.0, 12.0), w, color);
        }
        IconName::Grid => {
            for (x, y) in [(4.0, 4.0), (13.5, 4.0), (4.0, 13.5), (13.5, 13.5)] {
                scene.fill_rounded_rect(
                    Rect::from_xywh(g.p(x, y).x, g.p(x, y).y, g.d(6.5), g.d(6.5)),
                    g.d(1.6),
                    color,
                );
            }
        }
        IconName::List => {
            for y in [6.5, 12.0, 17.5] {
                scene.fill_circle(g.p(5.5, y), g.d(1.4), color);
                scene.stroke_line(g.p(9.5, y), g.p(19.5, y), g.d(1.8), color);
            }
        }
        IconName::Carousel => {
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(3.0, 8.5).x, g.p(3.0, 8.5).y, g.d(3.2), g.d(7.0)),
                g.d(1.2),
                color,
            );
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(17.8, 8.5).x, g.p(17.8, 8.5).y, g.d(3.2), g.d(7.0)),
                g.d(1.2),
                color,
            );
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(8.0, 6.0).x, g.p(8.0, 6.0).y, g.d(8.0), g.d(12.0)),
                g.d(1.6),
                color,
            );
        }
        IconName::ChevronLeft => {
            scene.stroke_polyline(
                &[g.p(14.5, 6.0), g.p(8.5, 12.0), g.p(14.5, 18.0)],
                g.d(2.4),
                color,
            );
        }
        IconName::ChevronRight => {
            scene.stroke_polyline(
                &[g.p(9.5, 6.0), g.p(15.5, 12.0), g.p(9.5, 18.0)],
                g.d(2.4),
                color,
            );
        }
        IconName::Close => {
            scene.stroke_line(g.p(6.5, 6.5), g.p(17.5, 17.5), w, color);
            scene.stroke_line(g.p(17.5, 6.5), g.p(6.5, 17.5), w, color);
        }
        IconName::Minimize => {
            scene.stroke_line(g.p(5.0, 12.0), g.p(19.0, 12.0), w, color);
        }
        IconName::Maximize => {
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(5.0, 5.0).x, g.p(5.0, 5.0).y, g.d(14.0), g.d(14.0)),
                g.d(1.5),
                w,
                color,
            );
        }
        IconName::Check => {
            scene.stroke_polyline(
                &[g.p(5.0, 12.5), g.p(10.0, 17.5), g.p(19.0, 6.5)],
                g.d(2.2),
                color,
            );
        }
        IconName::Plus => {
            scene.stroke_line(g.p(12.0, 5.0), g.p(12.0, 19.0), w, color);
            scene.stroke_line(g.p(5.0, 12.0), g.p(19.0, 12.0), w, color);
        }
        IconName::Home => {
            scene.fill_polygon(
                &[
                    g.p(12.0, 3.5),
                    g.p(21.0, 11.5),
                    g.p(18.5, 11.5),
                    g.p(18.5, 20.0),
                    g.p(13.5, 20.0),
                    g.p(13.5, 15.0),
                    g.p(10.5, 15.0),
                    g.p(10.5, 20.0),
                    g.p(5.5, 20.0),
                    g.p(5.5, 11.5),
                    g.p(3.0, 11.5),
                ],
                color,
            );
        }
        IconName::User => {
            scene.stroke_circle(g.p(12.0, 9.0), g.d(4.0), w, color);
            scene.stroke_polyline(
                &[
                    g.p(5.5, 20.5),
                    g.p(5.5, 18.5),
                    g.p(8.5, 15.5),
                    g.p(15.5, 15.5),
                    g.p(18.5, 18.5),
                    g.p(18.5, 20.5),
                ],
                w,
                color,
            );
        }
        IconName::Info => {
            scene.stroke_circle(g.p(12.0, 12.0), g.d(8.0), w, color);
            scene.fill_circle(g.p(12.0, 7.8), g.d(1.3), color);
            scene.stroke_line(g.p(12.0, 11.0), g.p(12.0, 16.5), w, color);
        }
        IconName::App => {
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(4.0, 4.0).x, g.p(4.0, 4.0).y, g.d(16.0), g.d(16.0)),
                g.d(3.0),
                w,
                color,
            );
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(8.5, 8.5).x, g.p(8.5, 8.5).y, g.d(7.0), g.d(7.0)),
                g.d(1.5),
                color,
            );
        }
    }
}

#[cfg(test)]
mod icon_tests {
    use super::*;
    use libluna_core::Theme;
    use libluna_render::{Scene, TextEngine};

    #[test]
    fn every_icon_draws_without_panic() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        for name in IconName::ALL {
            let mut scene = Scene::new();
            let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
            paint_icon(
                &mut ctx,
                name,
                Rect::from_xywh(0.0, 0.0, 24.0, 24.0),
                theme.colors.text,
            );
            assert!(!scene.is_empty(), "ikona {name:?} nic nie rysuje");
        }
    }
}
