//! # luna
//!
//! Powłoka środowiska graficznego **Luna** — odpowiednik GNOME Shell, ale zbudowany
//! na libLunie i działający na kompozytorze **Selene**.
//!
//! Moduły rysują się przez `libluna-widgets::PaintCtx`, więc ten sam kod obsługuje
//! podglądy headless (zrzuty do dokumentacji) i sesję na żywo.

pub mod apps;
pub mod dbus_notifications;
pub mod notifications;
pub mod osd;
pub mod panel;
pub mod power;
pub mod system;
pub mod toplevels;
pub mod volume;
pub mod workspaces;
