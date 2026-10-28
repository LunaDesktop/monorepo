//! Cache ścieżek `tiny-skia` dla powtarzalnych kształtów.
//!
//! W typowym interfejsie te same karty, przyciski i ikony są rysowane w tych
//! samych miejscach w każdej klatce. Budowanie `Path` od nowa (krzywe, łuki,
//! zaokrąglenia) było znaczącą częścią kosztu `draw` w benchmarku. Cache jest
//! kluczowany po **zgeometryzowanym** prostokącie/okręgu/łańcuchu punktów
//! (0,25 px), a nie po kolorze — ścieżka nie zależy od koloru ani skali.
//!
//! Ograniczenie: 2048 wpisów, eviction FIFO. To chroni pamięć w scenach
//! z tysiącami unikalnych kształtów (np. wykresy), a w UI i tak dominują
//! powtarzalne geometrie.

use std::collections::{HashMap, VecDeque};

use libluna_core::{Point, Rect};
use tiny_skia::{Path, PathBuilder};

/// Maksymalna liczba ścieżek w cache (FIFO).
const LIMIT: usize = 2048;

/// Kwantyzacja do 0,25 px — drobne różnice subpikselowe nie tworzą nowych wpisów.
fn q(v: f64) -> i32 {
    (v * 4.0).round() as i32
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PathKey {
    Rect {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        r: i32,
    },
    Circle {
        cx: i32,
        cy: i32,
        r: i32,
    },
    Line {
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
    },
    Polyline(Vec<(i32, i32)>),
    Polygon(Vec<(i32, i32)>),
}

/// Cache ścieżek; jedna instancja na `SoftwareRenderer`.
#[derive(Default)]
pub(crate) struct PathCache {
    map: HashMap<PathKey, Path>,
    order: VecDeque<PathKey>,
}

impl PathCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    pub(crate) fn rect(&mut self, rect: Rect, radius: f64) -> Option<&Path> {
        let key = PathKey::Rect {
            x: q(rect.x()),
            y: q(rect.y()),
            w: q(rect.width()),
            h: q(rect.height()),
            r: q(radius),
        };
        if !self.map.contains_key(&key) {
            let path = build_rect(rect, radius)?;
            self.insert(key.clone(), path);
        }
        self.map.get(&key)
    }

    pub(crate) fn circle(&mut self, center: Point, radius: f64) -> Option<&Path> {
        let key = PathKey::Circle {
            cx: q(center.x),
            cy: q(center.y),
            r: q(radius),
        };
        if !self.map.contains_key(&key) {
            let path = build_circle(center, radius)?;
            self.insert(key.clone(), path);
        }
        self.map.get(&key)
    }

    pub(crate) fn line(&mut self, from: Point, to: Point) -> Option<&Path> {
        let key = PathKey::Line {
            x1: q(from.x),
            y1: q(from.y),
            x2: q(to.x),
            y2: q(to.y),
        };
        if !self.map.contains_key(&key) {
            let path = build_line(from, to)?;
            self.insert(key.clone(), path);
        }
        self.map.get(&key)
    }

    pub(crate) fn polyline(&mut self, points: &[Point]) -> Option<&Path> {
        let key = PathKey::Polyline(key_points(points));
        if !self.map.contains_key(&key) {
            let path = build_polyline(points)?;
            self.insert(key.clone(), path);
        }
        self.map.get(&key)
    }

    pub(crate) fn polygon(&mut self, points: &[Point]) -> Option<&Path> {
        let key = PathKey::Polygon(key_points(points));
        if !self.map.contains_key(&key) {
            let path = build_polygon(points)?;
            self.insert(key.clone(), path);
        }
        self.map.get(&key)
    }

    fn insert(&mut self, key: PathKey, path: Path) {
        if self.map.len() >= LIMIT {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, path);
    }
}

fn key_points(points: &[Point]) -> Vec<(i32, i32)> {
    points.iter().map(|p| (q(p.x), q(p.y))).collect()
}

fn build_line(from: Point, to: Point) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(from.x as f32, from.y as f32);
    pb.line_to(to.x as f32, to.y as f32);
    pb.finish()
}

fn build_rect(rect: Rect, radius: f64) -> Option<Path> {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return None;
    }
    let x = rect.x() as f32;
    let y = rect.y() as f32;
    let w = rect.width() as f32;
    let h = rect.height() as f32;
    let mut pb = PathBuilder::new();
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0) as f32;
    if r > 0.0 {
        pb.move_to(x + r, y);
        pb.line_to(x + w - r, y);
        pb.quad_to(x + w, y, x + w, y + r);
        pb.line_to(x + w, y + h - r);
        pb.quad_to(x + w, y + h, x + w - r, y + h);
        pb.line_to(x + r, y + h);
        pb.quad_to(x, y + h, x, y + h - r);
        pb.line_to(x, y + r);
        pb.quad_to(x, y, x + r, y);
        pb.close();
    } else {
        pb.push_rect(tiny_skia::Rect::from_xywh(x, y, w, h)?);
    }
    pb.finish()
}

fn build_circle(center: Point, radius: f64) -> Option<Path> {
    if radius <= 0.0 {
        return None;
    }
    PathBuilder::from_circle(center.x as f32, center.y as f32, radius as f32)
}

fn build_polyline(points: &[Point]) -> Option<Path> {
    if points.len() < 2 {
        return None;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(points[0].x as f32, points[0].y as f32);
    for p in &points[1..] {
        pb.line_to(p.x as f32, p.y as f32);
    }
    pb.finish()
}

fn build_polygon(points: &[Point]) -> Option<Path> {
    if points.len() < 3 {
        return None;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(points[0].x as f32, points[0].y as f32);
    for p in &points[1..] {
        pb.line_to(p.x as f32, p.y as f32);
    }
    pb.close();
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64) -> Rect {
        Rect::from_xywh(x, y, 100.0, 40.0)
    }

    #[test]
    fn repeated_geometry_reuses_the_same_path() {
        let mut cache = PathCache::new();
        let a = cache.rect(rect(10.0, 20.0), 8.0).unwrap() as *const Path;
        let b = cache.rect(rect(10.0, 20.0), 8.0).unwrap() as *const Path;
        assert_eq!(a, b);
        assert_eq!(cache.len(), 1);

        // Inny promień = inny wpis.
        cache.rect(rect(10.0, 20.0), 12.0).unwrap();
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn subpixel_changes_are_quantized() {
        let mut cache = PathCache::new();
        cache.rect(rect(10.0, 20.0), 8.0).unwrap();
        cache.rect(rect(10.05, 20.0), 8.0).unwrap();
        assert_eq!(cache.len(), 1, "0,25 px kwantyzacji");
    }

    #[test]
    fn zero_sized_rect_is_not_cached() {
        let mut cache = PathCache::new();
        let empty = Rect::from_xywh(0.0, 0.0, 0.0, 10.0);
        assert!(cache.rect(empty, 4.0).is_none());
        assert_eq!(cache.len(), 0);
    }
}
