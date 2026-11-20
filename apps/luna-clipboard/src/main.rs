//! `luna-clipboard` — historia schowka środowiska Luna.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_clipboard::run_clipboard()
}
