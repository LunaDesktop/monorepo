//! Luna Calculator — kalkulator standardowy i naukowy środowiska Luna.
//!
//! Aplikacja jest całkowicie lokalna: wszystkie obliczenia wykonuje wbudowany
//! parser wyrażeń, a stan (tryb, kąt, ostatni wynik) żyje tylko w pamięci procesu.
//! Nie ma bazy danych, konta ani połączeń sieciowych.

pub mod app;
pub mod eval;
pub mod history;
pub mod prefs;
pub mod ui;

use std::error::Error;

/// Binarka `luna-calculator`.
pub fn run_calculator() -> Result<(), Box<dyn Error>> {
    app::run()
}
