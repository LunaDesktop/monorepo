//! Wspólny kontekst rysowania widgetów: scena + motyw + silnik tekstu.
//!
//! Widgety nie tworzą własnej sceny — rysują do przekazanej [`Scene`]. `PaintCtx`
//! daje im do tego wygodne skróty (pomiar i wyprowadzanie tekstu).

use libluna_core::style::{weight, FontStyle};
use libluna_core::{Color, Point, Theme};
use libluna_render::{Scene, TextEngine, TextRun};

/// Kontekst przekazywany do `paint` widgetów.
pub struct PaintCtx<'a> {
    pub scene: &'a mut Scene,
    pub theme: &'a Theme,
    pub text: &'a TextEngine,
}

impl<'a> PaintCtx<'a> {
    pub fn new(scene: &'a mut Scene, theme: &'a Theme, text: &'a TextEngine) -> Self {
        Self { scene, theme, text }
    }

    /// Szerokość napisu w pikselach logicznych.
    pub fn measure(&self, s: &str, size: f64, weight: u16) -> f64 {
        self.text.measure(s, size, weight).width
    }

    /// Szerokość napisu dla stylu z motywu.
    pub fn measure_style(&self, s: &str, style: FontStyle) -> f64 {
        self.measure(s, style.size, style.weight)
    }

    /// Rysuje napis; `baseline_y` to linia bazowa.
    pub fn label(
        &mut self,
        x: f64,
        baseline_y: f64,
        s: &str,
        size: f64,
        weight: u16,
        color: Color,
    ) {
        self.scene.text(
            TextRun::new(Point::new(x, baseline_y), s, size)
                .with_weight(weight)
                .with_color(color),
        );
    }

    /// Napis wyrównany do lewej, wyśrodkowany pionowo w `cy`.
    pub fn label_left(&mut self, x: f64, cy: f64, s: &str, size: f64, weight: u16, color: Color) {
        self.label(x, cy + size * 0.35, s, size, weight, color);
    }

    /// Napis wyśrodkowany poziomo w `cx` i pionowo w `cy`.
    pub fn label_center(
        &mut self,
        cx: f64,
        cy: f64,
        s: &str,
        size: f64,
        weight: u16,
        color: Color,
    ) {
        let w = self.measure(s, size, weight);
        self.label(cx - w / 2.0, cy + size * 0.35, s, size, weight, color);
    }

    /// Napis wyrównany do prawej (koniec w `right_x`), wyśrodkowany w `cy`.
    pub fn label_right(
        &mut self,
        right_x: f64,
        cy: f64,
        s: &str,
        size: f64,
        weight: u16,
        color: Color,
    ) {
        let w = self.measure(s, size, weight);
        self.label_left(right_x - w, cy, s, size, weight, color);
    }

    /// Domyślny kolor tekstu motywu.
    pub fn text_color(&self) -> Color {
        self.theme.colors.text
    }

    /// Kolor akcentu motywu.
    pub fn accent(&self) -> Color {
        self.theme.colors.accent
    }

    /// Kolor tekstu drugorzędnego.
    pub fn dim_text(&self) -> Color {
        self.theme.colors.dim_text
    }

    /// Domyślny styl tekstu treści.
    pub fn body(&self) -> FontStyle {
        self.theme.typography.body
    }

    /// Domyślna grubość „aktywna”.
    pub fn medium_weight(&self) -> u16 {
        weight::MEDIUM
    }
}
