//! `luna-calculator` — kalkulator standardowy i naukowy środowiska Luna.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_calculator::run_calculator()
}
