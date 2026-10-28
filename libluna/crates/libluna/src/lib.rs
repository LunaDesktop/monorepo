//! # libluna
//!
//! **Fasada** całego toolkitu — jedno wejście dla aplikacji. Zamiast dodawać cztery
//! zależności i pamiętać o ścieżkach re-eksportów, wystarczy:
//!
//! ```toml
//! [dependencies]
//! libluna = { path = "../libluna/crates/libluna" }   # albo { git = "…" }
//! ```
//!
//! i w kodzie:
//!
//! ```
//! use libluna::prelude::*;
//! ```
//!
//! ## Warstwy (dostępne też pod pełnymi nazwami)
//! - [`core`] — geometria, kolor, zdarzenia, tokeny motywu, ustawienia wyglądu.
//! - [`render`] — graf sceny i backend rysowania (CPU).
//! - [`platform`] — okna Wayland, wejście, pętla zdarzeń.
//! - [`i18n`] — wielojęzyczność oraz makro [`t!`](crate::t).
//!
//! ## Najszybszy start
//! Zobacz [`prelude`] oraz przykład `examples/minimal.rs`
//! (`cargo run -p libluna --example minimal`).

pub use libluna_core as core;
pub use libluna_i18n as i18n;
pub use libluna_platform as platform;
pub use libluna_render as render;
pub use libluna_widgets as widgets;

/// Makro tłumaczeń z `libluna-i18n` (np. `t!("Application")`).
pub use libluna_i18n::t;

/// Najczęściej używane typy — wystarczy `use libluna::prelude::*;`.
pub mod prelude {
    pub use crate::core::event::{ButtonState, Event, KeyEvent, Modifiers, PointerButton};
    pub use crate::core::{
        Accent, Color, Insets, Point, Rect, Size, Theme, ThemePreference, ThemeSettings,
    };
    pub use crate::i18n::{set_locale, tr, Locale};
    pub use crate::platform::{run, AppConfig, Frame};
    pub use crate::render::{Scene, SoftwareRenderer, TextEngine, TextRun};
    pub use crate::widgets::{NavEntry, NavList, PaintCtx, ViewSwitcher};
}
