//! `luna-clock` — czas i kalendarz środowiska Luna (Luna Time).

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_clock::run_clock()
}
