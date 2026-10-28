//! Kontrakt backendu rysującego.
//!
//! Widgety i platforma budują [`Scene`] i nie wiedzą, czym jest rasteryzowana.
//! Dzięki temu za tym samym interfejsem mogą stać:
//!
//! - [`SoftwareRenderer`](crate::SoftwareRenderer) — CPU (tiny-skia + fontdue),
//!   dzisiejszy domyślny backend, wysyłający RGBA8888 do `wl_shm`,
//! - przyszły backend GPU (EGL/wgpu) — bez zmian w `libluna-widgets`.
//!
//! Kontrakt jest celowo wąski: rozmiar bufora, rysowanie sceny i dostęp do
//! pikseli/tekstu. Wszystko, co specyficzne dla GPU (tekstury, dmabuf, synchronizacja),
//! zostaje po stronie implementacji.

use libluna_core::{Color, Rect};

use crate::text::TextEngine;
use crate::Scene;

/// Backend rasteryzujący scenę do bufora gotowego do prezentacji.
pub trait Renderer {
    /// Krótka nazwa do diagnostyki (`cpu`, `gpu`…).
    fn name(&self) -> &'static str;

    /// Zmienia rozmiar bufora (w pikselach fizycznych).
    fn resize(&mut self, width: u32, height: u32);

    fn width(&self) -> u32;

    fn height(&self) -> u32;

    /// Piksele w kolejności RGBA8888, `width * height * 4` bajtów.
    fn data(&self) -> &[u8];

    /// Rasteryzuje scenę. `background` wypełnia całą powierzchnię przed komendami
    /// (dla powierzchni przezroczystych podawany jest kolor z alfą 0).
    fn draw(&mut self, scene: &Scene, scale: f64, background: Color);

    /// Jak [`Renderer::draw`], ale wołający wie z góry, który obszar się zmienił.
    ///
    /// Backend GPU odczyta z pamięci tylko ten prostokąt — piksele poza nim zostają
    /// z poprzedniej klatki, co jest poprawne, bo nic ich nie zmieniło. `None` =
    /// cała powierzchnia. Domyślna implementacja ignoruje podpowiedź (CPU i tak ma
    /// cały bufor w pamięci).
    fn draw_damage(&mut self, scene: &Scene, scale: f64, background: Color, damage: Option<Rect>) {
        let _ = damage;
        self.draw(scene, scale, background);
    }

    /// Prostokąt (w pikselach fizycznych) odczytany z GPU w ostatniej klatce.
    /// `None` = cała powierzchnia, `Some(Rect::ZERO)` = nic nie odczytano.
    fn last_readback(&self) -> Option<Rect> {
        None
    }

    /// Koduje bieżący bufor do PNG (diagnostyka, zrzuty headless).
    fn encode_png(&self) -> Option<Vec<u8>>;

    /// Silnik tekstu używany przez scenę (`TextRun`).
    ///
    /// Nawet backend GPU potrzebuje metryk i rasteryzacji glifów po stronie CPU;
    /// dlatego kontrakt nie zakłada, że tekst jest renderowany przez GPU.
    fn text_engine(&self) -> &TextEngine;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SoftwareRenderer;
    use libluna_core::{Color, Rect};

    #[test]
    fn software_renderer_is_an_object_safe_backend() {
        let mut renderer: Box<dyn Renderer> = Box::new(SoftwareRenderer::new(16, 8));
        assert_eq!(renderer.name(), "cpu");
        assert_eq!((renderer.width(), renderer.height()), (16, 8));

        let mut scene = Scene::new();
        scene.fill_rect(
            Rect::from_xywh(0.0, 0.0, 16.0, 8.0),
            Color::from_rgba8(0xff, 0x00, 0x00, 0xff),
        );
        renderer.draw(&scene, 1.0, Color::BLACK);

        assert_eq!(renderer.data().len(), 16 * 8 * 4);
        assert!(renderer.encode_png().is_some());
    }
}
