//! Wspólny kontekst rysowania widgetów: scena + motyw + silnik tekstu.
//!
//! Widgety nie tworzą własnej sceny — rysują do przekazanej [`Scene`]. `PaintCtx`
//! daje im do tego wygodne skróty (pomiar i wyprowadzanie tekstu).

use libluna_core::style::{weight, FontStyle};
use libluna_core::{Color, Point, Rect, Theme};
use libluna_render::{Scene, TextEngine, TextRun};

use crate::display::{ellipsize_weighted, wrap_text};

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

    /// Jak [`PaintCtx::label_left`], ale napis jest skracany wielokropkiem do
    /// `max_width`. Zwraca rzeczywistą szerokość.
    pub fn label_left_fit(
        &mut self,
        x: f64,
        cy: f64,
        s: &str,
        size: f64,
        font_weight: u16,
        color: Color,
        max_width: f64,
    ) -> f64 {
        let fitted = ellipsize_weighted(s, max_width, size, font_weight, self.text);
        let width = self.measure(&fitted, size, font_weight);
        self.label_left(x, cy, &fitted, size, font_weight, color);
        width
    }

    /// Jak [`PaintCtx::label_right`], ale napis jest skracany wielokropkiem do
    /// `max_width`. Zwraca rzeczywistą szerokość.
    pub fn label_right_fit(
        &mut self,
        right_x: f64,
        cy: f64,
        s: &str,
        size: f64,
        font_weight: u16,
        color: Color,
        max_width: f64,
    ) -> f64 {
        let fitted = ellipsize_weighted(s, max_width, size, font_weight, self.text);
        let width = self.measure(&fitted, size, font_weight);
        self.label_right(right_x, cy, &fitted, size, font_weight, color);
        width
    }

    /// Rysuje akapit zawinięty do szerokości `rect` i **przycięty** do jego granic.
    ///
    /// Długie słowa (ścieżki, URL-e) są łamane po znakach. Gdy tekst nie mieści
    /// się w `max_lines` albo wysokości, ostatnia linia dostaje wielokropek.
    /// Zwraca wysokość zajętą przez narysowane linie.
    pub fn paragraph(
        &mut self,
        rect: Rect,
        text: &str,
        size: f64,
        line_height: f64,
        font_weight: u16,
        color: Color,
        max_lines: usize,
    ) -> f64 {
        if text.is_empty()
            || rect.width() <= 0.0
            || rect.height() <= 0.0
            || line_height <= 0.0
            || max_lines == 0
        {
            return 0.0;
        }
        let lines = wrap_text(text, rect.width(), size, font_weight, self.text);
        if lines.is_empty() {
            return 0.0;
        }
        let by_height = (rect.height() / line_height).floor().max(1.0) as usize;
        let limit = max_lines.min(by_height).max(1);
        let truncated = lines.len() > limit;

        // Clip jest częścią kontraktu: nawet pojedynczy znak szerszy od ramki
        // nie może wyjść na zewnątrz.
        self.scene.push_clip(rect);
        let mut drawn = 0;
        for (index, line) in lines.iter().take(limit).enumerate() {
            let line = if truncated && index + 1 == limit {
                ellipsize_weighted(
                    &format!("{line}…"),
                    rect.width(),
                    size,
                    font_weight,
                    self.text,
                )
            } else {
                line.clone()
            };
            self.label_left(
                rect.x(),
                rect.y() + line_height * (index as f64 + 0.5),
                &line,
                size,
                font_weight,
                color,
            );
            drawn += 1;
        }
        self.scene.pop_clip();
        drawn as f64 * line_height
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_fits_and_clips_to_rect() {
        let theme = Theme::dark();
        let text = TextEngine::system();
        let mut scene = Scene::new();
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        let rect = Rect::from_xywh(0.0, 0.0, 80.0, 30.0);
        let body = "/home/patryks/.local/zed.app/libexec/zed-editor ".repeat(4);
        let used = ctx.paragraph(
            rect,
            &body,
            12.0,
            16.0,
            weight::NORMAL,
            theme.colors.text,
            2,
        );
        assert!(used > 0.0);
        assert!(used <= rect.height());
    }

    #[test]
    fn fitted_labels_never_exceed_the_limit() {
        let theme = Theme::dark();
        let text = TextEngine::system();
        let mut scene = Scene::new();
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        let width = ctx.label_left_fit(
            0.0,
            10.0,
            "bardzo długi napis bez spacji",
            14.0,
            weight::NORMAL,
            theme.colors.text,
            60.0,
        );
        assert!(width <= 60.5, "napis ma {width} px");
        let width = ctx.label_right_fit(
            200.0,
            10.0,
            "bardzo długi napis bez spacji",
            14.0,
            weight::NORMAL,
            theme.colors.text,
            50.0,
        );
        assert!(width <= 50.5, "napis ma {width} px");
    }
}
