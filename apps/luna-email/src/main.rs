//! `luna-email` — klient poczty środowiska Luna.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_email::run_email()
}
