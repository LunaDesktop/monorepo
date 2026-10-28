//! # libluna-platform
//!
//! Warstwa platformy libLuny — odpowiednik GDK w GTK.
//!
//! Odpowiada za połączenie z kompozytorem Wayland (Selene), tworzenie okien przez
//! `xdg-shell` z dekoracjami po stronie klienta (CSD), bufory `wl_shm`, obsługę
//! wejścia (`seat`, `xkbcommon`) oraz pętlę zdarzeń.
//!
//! Punkt wejścia to [`run`] z [`AppConfig`]: otwiera okno i wywołuje domknięcie
//! rysujące przy każdym odświeżeniu. Gdy [`AppConfig::layer`] jest ustawione, zamiast
//! okna powstaje **powierzchnia warstwowa** (`wlr-layer-shell`) — tak działa panel Luny.

pub mod window;

pub use window::{run, AppConfig, Frame, InputRegion, LayerConfig, WakeHandle};

/// Warstwy powierzchni (`wlr-layer-shell`) — reeksport dla wygody odbiorców.
pub use smithay_client_toolkit::shell::wlr_layer::{Anchor, Layer};
