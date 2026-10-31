//! Luna Media — przeglądarka multimediów środowiska Luna na libLunie.
//!
//! Obrazy dekodujemy natywnie (PNG/JPEG/SVG), a wszystko inne oraz filmy —
//! przez `ffmpeg`. Dzięki temu nie wciągamy ciężkich bibliotek multimedialnych,
//! a obsługa formatów zależy od tego, co potrafi zainstalowany ffmpeg.

pub mod app;
pub mod decode;
pub mod model;
pub mod places;
pub mod prefs;
pub mod thumb;
pub mod ui;
pub mod video;

use std::error::Error;

/// Binarka `luna-media`.
pub fn run_media() -> Result<(), Box<dyn Error>> {
    app::run("Media", "eu.luna.Media")
}
