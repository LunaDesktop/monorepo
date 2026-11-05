//! Luna Editor — edytor tekstu i kodu środowiska Luna na libLunie.
//!
//! Najważniejsze elementy:
//! - [`syntax`] — płaskie leksery (Rust, TOML, JSON, Markdown, Shell, C/C++, Python,
//!   HTML/XML, CSS, JavaScript/TypeScript, PHP, YAML),
//! - [`app`] — dokument, pliki, undo/redo, wyszukiwanie i okna dialogowe,
//! - [`prefs`] — preferencje programu (`~/.config/luna/editor.conf`),
//! - [`ui`] — układ, rysowanie i obsługa wejścia.
//!
//! Podświetlanie rysuje [`libluna_widgets::CodeView`] — widget dodany do libLuny
//! razem z edytorem, więc mogą z niego korzystać także inne aplikacje.

pub mod app;
pub mod browser;
pub mod document;
pub mod plugin_ui;
pub mod plugins;
pub mod prefs;
pub mod syntax;
pub mod ui;
pub mod workspace;

use std::error::Error;

/// Binarka `luna-editor`.
pub fn run_editor() -> Result<(), Box<dyn Error>> {
    app::run()
}
