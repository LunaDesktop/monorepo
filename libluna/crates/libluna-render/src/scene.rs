//! Graf sceny — opis tego, co ma zostać narysowane, w pikselach logicznych.
//!
//! Scena jest niezależna od backendu: budują ją widgety, a [`crate::SoftwareRenderer`]
//! (albo przyszły backend GPU) ją rasteryzuje.

use libluna_core::style::weight;
use libluna_core::{Color, Point, Rect};
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::Arc;

use crate::text::TextEngine;

/// Gotowy obraz (premultiplied RGBA8) — np. skomponowany bufor okna z `wl_shm`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    /// Piksele premultiplied RGBA8, dokładnie `width * height * 4` bajtów.
    pub rgba: Vec<u8>,
    /// Czy obraz jest **rzadki** (dużo w pełni przezroczystych pikseli).
    /// Renderer używa wtedy blitowania z pomijaniem `alpha == 0` zamiast
    /// blendowania całego prostokąta — panele i popupy mają >50% pustych pikseli.
    pub sparse: bool,
    /// Czy obraz jest **w pełni nieprzezroczysty** (każdy piksel ma `alpha == 255`).
    /// Renderer może wtedy skopiować wiersze zamiast blendować piksel po pikselu.
    /// Ustawiane przez kompozytor dla buforów `Xrgb8888`/`Xbgr8888`.
    pub opaque: bool,
    /// Skalowanie **najbliższym sąsiadem** zamiast bilinear.
    ///
    /// Dla klatek wideo odtwarzanych w czasie rzeczywistym interpolacja bilinear
    /// potrafi zjeść większość budżetu klatki; lekko ostrzejsze krawędzie są
    /// lepszym kompromisem niż 18 zamiast 30 fps.
    pub nearest: bool,
}

impl ImageData {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            rgba,
            sparse: false,
            opaque: false,
            nearest: false,
        }
    }

    /// Oznacza obraz jako rzadki (patrz [`ImageData::sparse`]).
    pub fn with_sparse(mut self, sparse: bool) -> Self {
        self.sparse = sparse;
        self
    }

    /// Oznacza obraz jako w pełni nieprzezroczysty (patrz [`ImageData::opaque`]).
    pub fn with_opaque(mut self, opaque: bool) -> Self {
        self.opaque = opaque;
        self
    }

    /// Skalowanie najbliższym sąsiadem (patrz [`ImageData::nearest`]).
    pub fn with_nearest(mut self, nearest: bool) -> Self {
        self.nearest = nearest;
        self
    }

    /// Pusty (przezroczysty) obraz o zadanym rozmiarze.
    pub fn empty(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0; width as usize * height as usize * 4],
            sparse: true,
            opaque: false,
            nearest: false,
        }
    }
}

/// Pojedynczy fragment tekstu do narysowania.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    /// Lewy-górny róg obszaru tekstu; `origin.y` to linia bazowa pierwszego wiersza.
    pub origin: Point,
    pub text: String,
    /// Rozmiar czcionki w pikselach logicznych.
    pub size: f64,
    /// Grubość (patrz [`libluna_core::style::weight`]).
    pub weight: u16,
    pub color: Color,
}

impl TextRun {
    pub fn new(origin: Point, text: impl Into<String>, size: f64) -> Self {
        Self {
            origin,
            text: text.into(),
            size,
            weight: weight::NORMAL,
            color: Color::BLACK,
        }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn with_weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }
}

/// Pojedynczy plan bufora `dmabuf` (klient GPU).
///
/// Deskryptor jest trzymany przy obrazie, żeby import EGL mógł nastąpić
/// leniwie (przy pierwszym rysowaniu) i żeby pamięć żyła do końca bufora.
pub struct DmabufPlane {
    pub fd: OwnedFd,
    pub offset: u32,
    pub stride: u32,
}

impl std::fmt::Debug for DmabufPlane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DmabufPlane")
            .field("fd", &self.fd.as_raw_fd())
            .field("offset", &self.offset)
            .field("stride", &self.stride)
            .finish()
    }
}

impl PartialEq for DmabufPlane {
    fn eq(&self, other: &Self) -> bool {
        self.fd.as_raw_fd() == other.fd.as_raw_fd()
            && self.offset == other.offset
            && self.stride == other.stride
    }
}

/// Bufor `dmabuf` klienta (GPU) — import robi backend\.
///
/// Scena tylko opisuje bufor; `GpuRenderer` tworzy z niego `EGLImage` przy
/// pierwszym użyciu. `SoftwareRenderer` nie potrafi go odczytać i rysuje
/// zastępczy prostokąt (taki bufor nie powinien się pojawić bez GPU, bo
/// Selene ogłasza `zwp_linux_dmabuf_v1` tylko przy włączonym backendzie GPU).
pub struct DmabufImage {
    pub width: u32,
    pub height: u32,
    /// Kod `DRM_FORMAT_*` (little-endian fourcc), np. `ARGB8888`.
    pub format: u32,
    /// Modyfikator układu (`DRM_FORMAT_MOD_*`); `u64::MAX` = nieznany/invalid.
    pub modifier: u64,
    pub planes: Vec<DmabufPlane>,
}

impl std::fmt::Debug for DmabufImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DmabufImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &format!("0x{:08x}", self.format))
            .field("modifier", &format!("0x{:016x}", self.modifier))
            .field("planes", &self.planes)
            .finish()
    }
}

impl PartialEq for DmabufImage {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.format == other.format
            && self.modifier == other.modifier
            && self.planes == other.planes
    }
}

/// Polecenie rysowania.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    /// Wypełniony prostokąt.
    FillRect { rect: Rect, color: Color },
    /// Wypełniony prostokąt z zaokrąglonymi narożnikami.
    FillRoundedRect {
        rect: Rect,
        radius: f64,
        color: Color,
    },
    /// Obramowanie zaokrąglonego prostokąta (linia o zadanej grubości).
    StrokeRoundedRect {
        rect: Rect,
        radius: f64,
        width: f64,
        color: Color,
    },
    /// Odcinek o zadanej grubości (z zaokrąglonymi końcami).
    StrokeLine {
        from: Point,
        to: Point,
        width: f64,
        color: Color,
    },
    /// Wypełniony okrąg.
    FillCircle {
        center: Point,
        radius: f64,
        color: Color,
    },
    /// Obwód okręgu (linia o zadanej grubości).
    StrokeCircle {
        center: Point,
        radius: f64,
        width: f64,
        color: Color,
    },
    /// Łamana (otwarta; powtórz pierwszy punkt, by zamknąć kontur).
    StrokePolyline {
        points: Vec<Point>,
        width: f64,
        color: Color,
    },
    /// Wypełniony wielokąt (domykany automatycznie).
    FillPolygon { points: Vec<Point>, color: Color },
    /// Ogranicza kolejne polecenia do prostokąta (zastosowanie kumulacyjne).
    PushClip { rect: Rect },
    /// Ogranicza kolejne polecenia do **zaokrąglonego** prostokąta (kumulacyjnie).
    PushClipRounded { rect: Rect, radius: f64 },
    /// Zdejmuje ostatnie ograniczenie.
    PopClip,
    /// Tekst.
    Text(TextRun),
    /// Obraz (np. bufor okna) wpisany w prostokąt docelowy.
    Image { rect: Rect, data: Arc<ImageData> },
    /// Bufor `dmabuf` klienta GPU; import wykonuje backend.
    ImageDmabuf { rect: Rect, data: Arc<DmabufImage> },
    /// Tło pulpitu: pełnoekranowy, **nieprzezroczysty** obraz. Renderer kopiuje piksele
    /// wprost — mieszanie 2 mln pikseli co klatkę kosztowało ~10 ms na 1080p.
    Background { data: Arc<ImageData> },
}

/// Uporządkowana lista poleceń rysowania.
#[derive(Debug, Clone, Default)]
pub struct Scene {
    commands: Vec<DrawCommand>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Czyści scenę do ponownego użycia (bez zwalniania pamięci).
    pub fn clear(&mut self) {
        self.commands.clear();
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) -> &mut Self {
        self.commands.push(DrawCommand::FillRect { rect, color });
        self
    }

    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: f64, color: Color) -> &mut Self {
        self.commands.push(DrawCommand::FillRoundedRect {
            rect,
            radius,
            color,
        });
        self
    }

    pub fn stroke_rounded_rect(
        &mut self,
        rect: Rect,
        radius: f64,
        width: f64,
        color: Color,
    ) -> &mut Self {
        self.commands.push(DrawCommand::StrokeRoundedRect {
            rect,
            radius,
            width,
            color,
        });
        self
    }

    pub fn stroke_line(&mut self, from: Point, to: Point, width: f64, color: Color) -> &mut Self {
        self.commands.push(DrawCommand::StrokeLine {
            from,
            to,
            width,
            color,
        });
        self
    }

    /// Wypełniony okrąg.
    pub fn fill_circle(&mut self, center: Point, radius: f64, color: Color) -> &mut Self {
        self.commands.push(DrawCommand::FillCircle {
            center,
            radius,
            color,
        });
        self
    }

    /// Obwód okręgu.
    pub fn stroke_circle(
        &mut self,
        center: Point,
        radius: f64,
        width: f64,
        color: Color,
    ) -> &mut Self {
        self.commands.push(DrawCommand::StrokeCircle {
            center,
            radius,
            width,
            color,
        });
        self
    }

    /// Łamana (szkielet ikon); z zaokrąglonymi końcami i zgięciami.
    pub fn stroke_polyline(&mut self, points: &[Point], width: f64, color: Color) -> &mut Self {
        self.commands.push(DrawCommand::StrokePolyline {
            points: points.to_vec(),
            width,
            color,
        });
        self
    }

    /// Wypełniony wielokąt (domykany automatycznie).
    pub fn fill_polygon(&mut self, points: &[Point], color: Color) -> &mut Self {
        self.commands.push(DrawCommand::FillPolygon {
            points: points.to_vec(),
            color,
        });
        self
    }

    pub fn push_clip(&mut self, rect: Rect) -> &mut Self {
        self.commands.push(DrawCommand::PushClip { rect });
        self
    }

    /// Ogranicza kolejne polecenia do zaokrąglonego prostokąta (np. narożniki okna).
    pub fn push_clip_rounded(&mut self, rect: Rect, radius: f64) -> &mut Self {
        self.commands
            .push(DrawCommand::PushClipRounded { rect, radius });
        self
    }

    pub fn pop_clip(&mut self) -> &mut Self {
        self.commands.push(DrawCommand::PopClip);
        self
    }

    pub fn text(&mut self, run: TextRun) -> &mut Self {
        self.commands.push(DrawCommand::Text(run));
        self
    }

    /// Rysuje obraz wpisany w `rect` (skalowany, jeśli rozmiary się różnią).
    pub fn image(&mut self, rect: Rect, data: Arc<ImageData>) -> &mut Self {
        self.commands.push(DrawCommand::Image { rect, data });
        self
    }

    /// Rysuje bufor `dmabuf` klienta (import po stronie backendu GPU).
    pub fn image_dmabuf(&mut self, rect: Rect, data: Arc<DmabufImage>) -> &mut Self {
        self.commands.push(DrawCommand::ImageDmabuf { rect, data });
        self
    }

    /// Ustawia tło pulpitu (pełnoekranowy, nieprzezroczysty obraz).
    pub fn background(&mut self, data: Arc<ImageData>) -> &mut Self {
        self.commands.push(DrawCommand::Background { data });
        self
    }

    /// Czy obie sceny zawierają identyczne polecenia?
    ///
    /// Obrazy (`Image`/`Background`) porównujemy po **wskaźniku `Arc`**, nie po
    /// zawartości — inaczej porównanie tapety 1080p co klatkę kosztowałoby więcej
    /// niż samo rysowanie. Różne `Arc` o tej samej treści = „różne” (bezpieczny
    /// false negative: po prostu przerysujemy).
    pub fn same_as(&self, other: &Scene) -> bool {
        self.commands.len() == other.commands.len()
            && self
                .commands
                .iter()
                .zip(&other.commands)
                .all(|(a, b)| same_command(a, b))
    }

    /// Konserwatywny prostokąt, w którym `self` i `next` mogą się różnić.
    ///
    /// `Some(Rect::ZERO)` = brak różnic, `None` = nie da się zawęzić (zmiana tła
    /// albo struktury clipów) i wołający musi przyjąć całą powierzchnię.
    ///
    /// Prostokąty są **nadmiarowe**, nigdy za małe: nadmiar kosztuje odczyt kilku
    /// dodatkowych pikseli, a zbyt wąski prostokąt zostawiłby na ekranie stare piksele.
    pub fn damage_bounds(&self, next: &Scene, text: &TextEngine) -> Option<Rect> {
        if self.commands.len() != next.commands.len() {
            return None;
        }
        let mut acc: Option<Rect> = None;
        for (a, b) in self.commands.iter().zip(next.commands.iter()) {
            if same_command(a, b) {
                continue;
            }
            let (Some(ra), Some(rb)) = (command_area(a, text), command_area(b, text)) else {
                return None;
            };
            let area = ra.union(&rb);
            acc = Some(match acc {
                Some(r) => r.union(&area),
                None => area,
            });
        }
        Some(acc.unwrap_or(Rect::ZERO))
    }
}

fn same_command(a: &DrawCommand, b: &DrawCommand) -> bool {
    match (a, b) {
        (DrawCommand::Image { rect: ra, data: da }, DrawCommand::Image { rect: rb, data: db }) => {
            ra == rb && Arc::ptr_eq(da, db)
        }
        (
            DrawCommand::ImageDmabuf { rect: ra, data: da },
            DrawCommand::ImageDmabuf { rect: rb, data: db },
        ) => ra == rb && Arc::ptr_eq(da, db),
        (DrawCommand::Background { data: da }, DrawCommand::Background { data: db }) => {
            Arc::ptr_eq(da, db)
        }
        _ => a == b,
    }
}

/// Nadmiarowy prostokąt zajmowany przez pojedyncze polecenie.
///
/// `None` = polecenie może zmienić dowolny piksel (tło pulpitu, struktura clipów).
pub(crate) fn command_area(cmd: &DrawCommand, text: &TextEngine) -> Option<Rect> {
    match cmd {
        DrawCommand::Background { .. } | DrawCommand::PopClip => None,
        DrawCommand::FillRect { rect, .. }
        | DrawCommand::FillRoundedRect { rect, .. }
        | DrawCommand::Image { rect, .. }
        | DrawCommand::ImageDmabuf { rect, .. }
        | DrawCommand::PushClip { rect }
        | DrawCommand::PushClipRounded { rect, .. } => Some(*rect),
        DrawCommand::StrokeRoundedRect { rect, width, .. } => {
            Some(pad_rect(*rect, width * 0.5 + 1.0))
        }
        DrawCommand::StrokeLine {
            from, to, width, ..
        } => {
            let pad = width * 0.5 + 2.0;
            let (x0, x1) = (from.x.min(to.x), from.x.max(to.x));
            let (y0, y1) = (from.y.min(to.y), from.y.max(to.y));
            Some(Rect::from_xywh(
                x0 - pad,
                y0 - pad,
                x1 - x0 + 2.0 * pad,
                y1 - y0 + 2.0 * pad,
            ))
        }
        DrawCommand::FillCircle { center, radius, .. } => Some(circle_rect(*center, *radius)),
        DrawCommand::StrokeCircle {
            center,
            radius,
            width,
            ..
        } => Some(circle_rect(*center, radius + width * 0.5 + 1.0)),
        DrawCommand::StrokePolyline { points, width, .. } => {
            points_bounds(points, width * 0.5 + 2.0)
        }
        DrawCommand::FillPolygon { points, .. } => points_bounds(points, 2.0),
        DrawCommand::Text(run) => {
            let m = text.measure(&run.text, run.size, run.weight);
            // `origin.y` to linia bazowa; zapas na akcenty, ogonki i podkreślenia.
            let top = run.origin.y - run.size * 1.5;
            let h = m.height.max(run.size) + run.size * 0.9;
            Some(Rect::from_xywh(run.origin.x - 3.0, top, m.width + 6.0, h))
        }
    }
}

fn pad_rect(r: Rect, pad: f64) -> Rect {
    Rect::from_xywh(
        r.x() - pad,
        r.y() - pad,
        r.width() + 2.0 * pad,
        r.height() + 2.0 * pad,
    )
}

fn circle_rect(center: Point, radius: f64) -> Rect {
    Rect::from_xywh(
        center.x - radius,
        center.y - radius,
        2.0 * radius,
        2.0 * radius,
    )
}

fn points_bounds(points: &[Point], pad: f64) -> Option<Rect> {
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for p in points {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    if !x0.is_finite() {
        return None;
    }
    Some(Rect::from_xywh(
        x0 - pad,
        y0 - pad,
        x1 - x0 + 2.0 * pad,
        y1 - y0 + 2.0 * pad,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_commands_in_order() {
        let mut s = Scene::new();
        s.fill_rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), Color::BLACK)
            .push_clip(Rect::from_xywh(0.0, 0.0, 5.0, 5.0))
            .pop_clip();
        assert_eq!(s.len(), 3);
        assert!(matches!(s.commands()[0], DrawCommand::FillRect { .. }));
        assert!(matches!(s.commands()[1], DrawCommand::PushClip { .. }));
        assert!(matches!(s.commands()[2], DrawCommand::PopClip));
    }

    /// Scena jest buforem wielokrotnego użytku: **bez `clear()` komendy sumują się**
    /// z klatki na klatkę, przez co renderer rysuje coraz więcej i aplikacja zwalnia
    /// z każdą sekundą. Platforma czyści scenę przed każdym wywołaniem domknięcia;
    /// ten test pilnuje samego kontraktu, na którym to polega.
    #[test]
    fn reused_scene_accumulates_until_cleared() {
        let mut s = Scene::new();
        let rect = Rect::from_xywh(0.0, 0.0, 4.0, 4.0);
        for _ in 0..100 {
            s.fill_rect(rect, Color::BLACK);
        }
        assert_eq!(s.len(), 100, "bez clear() komendy sumują się");
        s.clear();
        assert!(s.is_empty(), "clear() opróżnia scenę do reużycia");
    }

    #[test]
    fn same_as_compares_commands_and_images_by_pointer() {
        let rect = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
        let image = Arc::new(ImageData::empty(4, 4));

        let mut a = Scene::new();
        a.fill_rect(rect, Color::BLACK).image(rect, image.clone());
        let mut b = a.clone();
        assert!(a.same_as(&b));

        // Ta sama treść, inny `Arc` — traktujemy jako zmianę (bezpieczny false negative).
        b.image(rect, Arc::new(ImageData::empty(4, 4)));
        assert!(!a.same_as(&b));

        // Inna geometria/komenda.
        let mut c = a.clone();
        c.fill_rect(rect, Color::WHITE);
        assert!(!a.same_as(&c));

        // Tło również jest porównywane po wskaźniku.
        let mut d = Scene::new();
        d.background(image.clone());
        let mut e = Scene::new();
        e.background(image);
        assert!(d.same_as(&e));
    }

    #[test]
    fn damage_bounds_narrows_to_changed_commands() {
        let text = TextEngine::none();
        let scene = |second: Color| {
            let mut s = Scene::new();
            s.fill_rect(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), Color::BLACK);
            s.fill_rect(Rect::from_xywh(50.0, 50.0, 10.0, 10.0), second);
            s
        };
        let a = scene(Color::BLACK);
        assert_eq!(a.damage_bounds(&a.clone(), &text), Some(Rect::ZERO));

        let b = scene(Color::WHITE);
        let d = a.damage_bounds(&b, &text).expect("zawężony prostokąt");
        assert_eq!(
            (d.x(), d.y(), d.width(), d.height()),
            (50.0, 50.0, 10.0, 10.0),
            "tylko zmienione polecenie"
        );

        // Inna liczba poleceń = nie da się zawęzić.
        let mut c = a.clone();
        c.fill_rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0), Color::BLACK);
        assert_eq!(a.damage_bounds(&c, &text), None);

        // Zmiana tła może zmienić dowolny piksel.
        let mut bg1 = Scene::new();
        bg1.background(Arc::new(ImageData::empty(4, 4)));
        let mut bg2 = Scene::new();
        bg2.background(Arc::new(ImageData::empty(4, 4)));
        assert_eq!(bg1.damage_bounds(&bg2, &text), None);
    }
}
