//! `luna-files` — menedżer plików Luny.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_files::run_files()
}
