//! `gaminedes` — alias menedżera plików Luny (ta sama aplikacja, inna nazwa okna).

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_files::run_gaminedes()
}
