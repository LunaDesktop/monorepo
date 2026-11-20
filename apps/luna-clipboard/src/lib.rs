//! Luna Clipboard — historia schowka środowiska Luna.
//!
//! Aplikacja nie wysyła nic do sieci: historia trafia wyłącznie do
//! `~/.config/luna/clipboard.conf` (prawa `0600`). Czyta schowek systemowy
//! przez `wl_data_device` i zapisuje tylko **tekst** — obrazy i pliki zostają
//! w schowku systemowym, bo menedżer historii nie jest archiwum plików.

pub mod app;
pub mod history;
pub mod ui;

use std::error::Error;

/// Binarka `luna-clipboard`.
pub fn run_clipboard() -> Result<(), Box<dyn Error>> {
    app::run()
}
