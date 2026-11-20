//! `luna-email` — klient poczty środowiska Luna.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // `mailto:` z Ustawień/`xdg-email` albo jawne `--to/--subject/--attach`.
    match luna_email::cli::parse(std::env::args().skip(1)) {
        Some(request) => luna_email::run_email_with(request),
        None => luna_email::run_email(),
    }
}
