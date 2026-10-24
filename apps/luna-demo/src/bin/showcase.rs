//! „Luna — widgety”: pełne demo Ustawień libLuny w prawdziwym oknie.
//!
//! To ta sama scena, którą widać na obrazach `docs/images/demo-step-*.png`:
//! sidebar, motyw jasny/ciemny/auto, paleta akcentów, wybór języka, przełączniki,
//! suwak, zakładki, pasek ścieżki, wyszukiwarka, lista/siatka plików, popovery
//! (☰, Opcje, język), powiadomienie z „Cofnij” i dolny panel „O aplikacji”.
//!
//! Uruchom: `cargo run -p luna-demo --bin showcase`

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_demo::app::run_widgets()
}
