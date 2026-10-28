//! # luna-greeter
//!
//! Menedżer logowania Luny: frontend **greetd** zbudowany na libLunie.
//!
//! Podział:
//! - [`greetd`] — protokół JSON po gnieździe Unix i wątek roboczy,
//! - [`users`] — lokalna lista kont do szybkiego wyboru,
//! - [`app`] — stan, obsługa wejścia i rysowanie ekranu logowania.
//!
//! Greeter działa jako nieuprzywilejowany użytkownik (`greeter`); PAM, logind
//! i uruchomienie sesji zostają w daemonie greetd.

pub mod app;
pub mod avatars;
pub mod greetd;
pub mod sessions;
pub mod state;
pub mod users;
