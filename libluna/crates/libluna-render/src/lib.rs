//! # libluna-render
//!
//! Warstwa rysowania libLuny — odpowiednik Gsk w GTK.
//!
//! Składa się z dwóch niezależnych kawałków:
//!
//! - [`scene`] — **graf sceny** ([`Scene`]): lista prymitywów (prostokąty, zaokrąglone
//!   prostokąty, obramowania, clip, tekst). Platforma i widgety budują scenę, nie
//!   dotykając pikseli.
//! - [`SoftwareRenderer`] — backend **CPU** (tiny-skia + fontdue), który rasteryzuje
//!   scenę do bufora RGBA8888 gotowego do wysłania przez `wl_shm`.
//! - [`svg`] — rasteryzacja ikon SVG (`resvg`) do [`ImageData`] oraz eksport
//!   sceny do SVG ([`scene_to_svg`]).
//!
//! Rozdzielenie sceny od backendu pozwala później dodać backend GPU bez zmian
//! w widgetach.

pub mod damage;
pub mod gpu;
mod path_cache;
pub mod renderer;
pub mod scene;
pub mod software;
pub mod svg;
pub mod text;
pub mod wallpaper;

pub use damage::{diff_bounds, diff_bounds_in, DamageRect};
pub use renderer::Renderer;
pub use scene::{DmabufImage, DmabufPlane, DrawCommand, ImageData, Scene, TextRun};
pub use software::SoftwareRenderer;
pub use svg::{decode_svg, scene_to_svg};
pub use text::TextEngine;
