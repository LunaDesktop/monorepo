//! `MediaControls` — kontrolki odtwarzania (odpowiednik GTK4 `GtkMediaControls`).
//!
//! Widget immediate: [`MediaControls::rects`] liczy rozmieszczenie (przycisk play/pauza,
//! tytuł/artysta, pasek postępu z uchwytem, czasy), [`MediaControls::seek_at`] tłumaczy
//! klik na pozycję 0..1, a [`MediaControls::paint`] rysuje całość.

use libluna_core::style::weight;
use libluna_core::{Insets, Point, Rect, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;

/// Szerokość paska postępu.
const SEEK_W: f64 = 180.0;

/// Kontrolki odtwarzania.
#[derive(Debug, Clone, Default)]
pub struct MediaControls {
    playing: bool,
    position: f64,
    title: String,
    artist: String,
    elapsed: String,
    total: String,
}

/// Rozmieszczenie elementów.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaRects {
    pub play: Rect,
    pub title: Rect,
    pub artist: Rect,
    pub seek: Rect,
    pub knob: Rect,
    pub elapsed: Rect,
    pub total: Rect,
}

impl MediaControls {
    pub fn new(title: impl Into<String>, artist: impl Into<String>) -> Self {
        Self {
            playing: true,
            position: 0.0,
            title: title.into(),
            artist: artist.into(),
            elapsed: "0:00".to_owned(),
            total: "0:00".to_owned(),
        }
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
    }

    pub fn toggle(&mut self) {
        self.playing = !self.playing;
    }

    /// Pozycja 0..1.
    pub fn position(&self) -> f64 {
        self.position
    }

    pub fn set_position(&mut self, position: f64) {
        self.position = position.clamp(0.0, 1.0);
    }

    pub fn set_times(&mut self, elapsed: impl Into<String>, total: impl Into<String>) {
        self.elapsed = elapsed.into();
        self.total = total.into();
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.banner.min_height
    }

    fn glyph(&self) -> &'static str {
        if self.playing {
            "▮▮"
        } else {
            "▶"
        }
    }

    pub fn rects(&self, area: Rect, theme: &Theme, text: &TextEngine) -> MediaRects {
        let m = theme.metrics.banner;
        let body = theme.typography.body;
        let caption = theme.typography.caption.size;
        let inner = area.deflate(Insets::splat(m.pad));
        let cy = inner.center().y;

        let play = Rect::from_xywh(inner.x(), cy - m.button_h / 2.0, m.button_h, m.button_h);

        // Prawa strona: total | seek | elapsed.
        let total_w = text
            .measure(&self.total, caption, weight::NORMAL)
            .width
            .max(28.0);
        let total = Rect::from_xywh(
            inner.right() - total_w,
            cy - caption,
            total_w,
            caption * 2.0,
        );
        let seek = Rect::from_xywh(total.x() - m.gap - SEEK_W, cy - 3.0, SEEK_W, 6.0);
        let elapsed_w = text
            .measure(&self.elapsed, caption, weight::NORMAL)
            .width
            .max(28.0);
        let elapsed = Rect::from_xywh(
            seek.x() - m.gap - elapsed_w,
            cy - caption,
            elapsed_w,
            caption * 2.0,
        );

        let tx = play.right() + m.gap;
        let tw = (elapsed.x() - m.gap - tx).max(0.0);
        let title = Rect::from_xywh(tx, cy - body.line_height, tw, body.line_height);
        let artist = Rect::from_xywh(tx, cy, tw, body.line_height);

        let knob = Rect::from_xywh(
            seek.x() + seek.width() * self.position - 5.0,
            cy - 5.0,
            10.0,
            10.0,
        );

        MediaRects {
            play,
            title,
            artist,
            seek,
            knob,
            elapsed,
            total,
        }
    }

    /// Czy kliknięto przycisk play/pauza.
    pub fn play_at(&self, area: Rect, theme: &Theme, text: &TextEngine, p: Point) -> bool {
        self.rects(area, theme, text).play.contains(p)
    }

    /// Pozycja 0..1 odpowiadająca kliknięciu na pasku postępu (jeśli trafiono).
    pub fn seek_at(&self, area: Rect, theme: &Theme, text: &TextEngine, p: Point) -> Option<f64> {
        let r = self.rects(area, theme, text);
        // Powiększamy obszar pionowo dla wygody klikania.
        let hit = Rect::from_xywh(r.seek.x(), r.seek.center().y - 12.0, r.seek.width(), 24.0);
        if !hit.contains(p) {
            return None;
        }
        Some(((p.x - r.seek.x()) / r.seek.width()).clamp(0.0, 1.0))
    }

    pub fn paint(&self, ctx: &mut PaintCtx, area: Rect) {
        let theme = *ctx.theme;
        let r = self.rects(area, &theme, ctx.text);

        // Tło pigułki.
        ctx.scene
            .fill_rounded_rect(area, theme.radius.medium, theme.colors.card_bg);
        ctx.scene
            .stroke_rounded_rect(area, theme.radius.medium, 1.0, theme.colors.border);

        // Przycisk play/pauza.
        ctx.scene
            .fill_rounded_rect(r.play, r.play.width() / 2.0, theme.colors.accent);
        ctx.label_center(
            r.play.center().x,
            r.play.center().y,
            self.glyph(),
            theme.typography.caption.size,
            weight::BOLD,
            theme.colors.accent_fg,
        );

        // Tytuł / artysta.
        ctx.label_left(
            r.title.x(),
            r.title.center().y,
            &self.title,
            theme.typography.body.size,
            weight::MEDIUM,
            theme.colors.text,
        );
        ctx.label_left(
            r.artist.x(),
            r.artist.center().y,
            &self.artist,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );

        // Czasy.
        ctx.label_left(
            r.elapsed.x(),
            r.elapsed.center().y,
            &self.elapsed,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
        ctx.label_right(
            r.total.right(),
            r.total.center().y,
            &self.total,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );

        // Pasek postępu + uchwyt.
        ctx.scene.fill_rounded_rect(r.seek, 3.0, theme.colors.shade);
        let fill = Rect::from_xywh(
            r.seek.x(),
            r.seek.y(),
            r.seek.width() * self.position,
            r.seek.height(),
        );
        ctx.scene.fill_rounded_rect(fill, 3.0, theme.colors.accent);
        ctx.scene
            .fill_rounded_rect(r.knob, 5.0, theme.colors.accent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_render::{Scene, TextEngine};

    fn area() -> Rect {
        Rect::from_xywh(0.0, 0.0, 520.0, 56.0)
    }

    #[test]
    fn layout_puts_play_left_and_times_right() {
        let theme = Theme::light();
        let text = TextEngine::none();
        let mc = MediaControls::new("Utwór", "Wykonawca");
        let r = mc.rects(area(), &theme, &text);
        assert!(r.play.x() < r.title.x());
        assert!(r.title.x() < r.seek.x());
        assert!(r.elapsed.right() <= r.seek.x());
        assert!(r.total.right() <= area().right());
    }

    #[test]
    fn seek_hit_maps_to_position() {
        let theme = Theme::light();
        let text = TextEngine::none();
        let mc = MediaControls::new("T", "A");
        let r = mc.rects(area(), &theme, &text);
        let mid = Point::new(r.seek.center().x, r.seek.center().y);
        let p = mc.seek_at(area(), &theme, &text, mid).unwrap();
        assert!((p - 0.5).abs() < 0.02);
        assert!(mc
            .seek_at(
                area(),
                &theme,
                &text,
                Point::new(r.play.center().x, r.play.center().y)
            )
            .is_none());
    }

    #[test]
    fn play_hit_and_toggle() {
        let theme = Theme::light();
        let text = TextEngine::none();
        let mut mc = MediaControls::new("T", "A");
        let r = mc.rects(area(), &theme, &text);
        assert!(mc.play_at(area(), &theme, &text, r.play.center()));
        mc.toggle();
        assert!(!mc.playing());
    }

    #[test]
    fn paints_without_window() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut scene = Scene::new();
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        MediaControls::new("Utwór", "Wykonawca").paint(&mut ctx, area());
        assert!(scene.len() > 8);
    }
}
