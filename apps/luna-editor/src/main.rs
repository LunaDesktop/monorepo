//! `luna-editor` — lekki edytor tekstu i kodu Luny.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_editor::run_editor()
}
