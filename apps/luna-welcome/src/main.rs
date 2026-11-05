//! `luna-welcome` — powitanie, nowości i skrót do instalatora.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_welcome::run_welcome()
}
