//! „Luna — widgety”: dokumentacja widgetów libLuny w prawdziwym oknie.
//!
//! Ten sam ekran co `luna-demo`: lista widgetów, działające przykłady, kod użycia
//! i warianty `HeaderBar`. Pasek tytułu jest prawdziwym CSD (przeciąganie,
//! minimalizacja, maksymalizacja, zamykanie).
//!
//! Uruchom: `cargo run -p luna-demo --bin showcase`

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_demo::app::run_widgets()
}
