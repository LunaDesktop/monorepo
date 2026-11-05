//! `luna-installer` — kreator instalacji systemu Luna.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_installer::run_installer()
}
