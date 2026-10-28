//! Graf sceny — opis tego, co ma zostać narysowane, w pikselach logicznych.
//!
//! Scena jest niezależna od backendu: budują ją widgety, a [`crate::SoftwareRenderer`]
//! (albo przyszły backend GPU) ją rasteryzuje.

use libluna_core::style::weight;
use libluna_core::{Color, Point, Rect};
use std::sync::Arc;

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
}

impl ImageData {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            rgba,
            sparse: false,
        }
    }

    /// Oznacza obraz jako rzadki (patrz [`ImageData::sparse`]).
    pub fn with_sparse(mut self, sparse: bool) -> Self {
        self.sparse = sparse;
        self
    }

    /// Pusty (przezroczysty) obraz o zadanym rozmiarze.
    pub fn empty(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0; width as usize * height as usize * 4],
            sparse: true,
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
}

fn same_command(a: &DrawCommand, b: &DrawCommand) -> bool {
    match (a, b) {
        (DrawCommand::Image { rect: ra, data: da }, DrawCommand::Image { rect: rb, data: db }) => {
            ra == rb && Arc::ptr_eq(da, db)
        }
        (DrawCommand::Background { data: da }, DrawCommand::Background { data: db }) => {
            Arc::ptr_eq(da, db)
        }
        _ => a == b,
    }
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
}
