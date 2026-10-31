//! `luna-media` — przeglądarka obrazów i filmów Luny.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    luna_media::run_media()
}
