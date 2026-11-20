//! Luna Mail — klasyczny klient poczty dla środowiska Luna.
//!
//! Aplikacja jest cienką warstwą nad `email-lib` (Pimalaya): IMAP odpowiada za
//! foldery i wiadomości, SMTP za wysyłkę. Sieć działa w osobnym wątku, więc
//! rysowanie nigdy nie czeka na serwer — UI dostaje gotowe, własne typy.

pub mod account;
pub mod app;
pub mod backend;
pub mod cli;
pub mod model;
pub mod notify;
pub mod ui;

use std::error::Error;

/// Binarka `luna-email`.
pub fn run_email() -> Result<(), Box<dyn Error>> {
    app::run(None)
}

/// Start z gotowym szkicem (`mailto:` albo `--to/--subject/--attach`).
pub fn run_email_with(request: cli::ComposeRequest) -> Result<(), Box<dyn Error>> {
    app::run(Some(request))
}
