//! Dokumentacja widgetów libLuny — jedyne okno aplikacji demo.
//!
//! Uruchom: `cargo run -p luna-demo --bin luna-demo`
//! To samo okno instaluje się jako `luna-showcase` („Luna — widgety”).

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_demo::app::run()
}
