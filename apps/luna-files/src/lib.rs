//! Luna Files — menedżer plików środowiska Luna na libLunie.

pub mod app;
pub mod archive;
pub mod list;
pub mod model;
pub mod open_with;
pub mod ops;
pub mod places;
pub mod prefs;
pub mod trash;
pub mod ui;

use std::error::Error;

/// Binarka `luna-files`.
pub fn run_files() -> Result<(), Box<dyn Error>> {
    app::run("Files", "eu.luna.Files")
}

/// Binarka `gaminedes` — ta sama aplikacja pod inną nazwą (alias).
pub fn run_gaminedes() -> Result<(), Box<dyn Error>> {
    app::run("Gaminedes", "eu.luna.Gaminedes")
}
