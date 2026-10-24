//! Demo aplikacji środowiska Luna — pełne, interaktywne okno Ustawień z
//! HeaderBar-em (CSD), sidebarem, motywami, akcentami, i18n, zakładkami,
//! widokiem plików, popoverami, powiadomieniem i dolnym panelem.
//!
//! Uruchom: `cargo run -p luna-demo --bin luna-demo`
//! To samo demo instaluje się jako `luna-showcase` („Luna — widgety”).

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_demo::app::run()
}
