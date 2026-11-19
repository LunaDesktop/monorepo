//! Luna Tasks — menedżer procesów i monitor systemu środowiska Luna.
//!
//! Aplikacja czyta wyłącznie `/proc` i `/etc/passwd`: nie łączy się z siecią,
//! nie zapisuje historii procesów i nie wymaga uprawnień roota (własne procesy
//! może zakończyć każdy użytkownik, cudze tylko administrator).

pub mod app;
pub mod prefs;
pub mod procs;
pub mod ui;

use std::error::Error;

/// Binarka `luna-tasks`.
pub fn run_tasks() -> Result<(), Box<dyn Error>> {
    app::run()
}
