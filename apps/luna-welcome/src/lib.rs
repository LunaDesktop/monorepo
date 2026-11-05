//! Luna Welcome — powitanie w środowisku Luna.
//!
//! Aplikacja pokazuje wersję systemu, najnowsze zmiany i — na obrazie live —
//! wyraźne przejście do instalatora. Nie zmienia dysku ani ustawień: potrafi
//! tylko uruchomić inne programy (`luna-installer`, `luna-store`,
//! `luna-settings`) albo otworzyć dokumentację.

pub mod app;
pub mod live;
pub mod news;
pub mod ui;

pub use app::{Action, App};
pub use live::LiveEnvironment;
pub use news::{Notes, Release};

use std::error::Error;
use std::time::Duration;

use libluna_core::ThemeSettings;
use libluna_i18n::{set_locale, tr};
use libluna_platform::{run, AppConfig, Frame};

/// Uruchamia okno powitania (binarka `luna-welcome`).
pub fn run_welcome() -> Result<(), Box<dyn Error>> {
    set_locale(libluna_i18n::preferred_locale());
    libluna_i18n::load_catalogs_from_dirs();

    let settings = ThemeSettings::load();
    let system_dark = std::env::var("LUNA_SYSTEM_DARK")
        .map(|value| !value.is_empty() && value != "0")
        .unwrap_or(false);

    let mut app = App::new();
    let title = tr("Welcome to Luna");

    let config = AppConfig::new(title.clone())
        .app_id("eu.luna.Welcome")
        .initial_size(980, 760)
        .settings(settings)
        .system_dark(system_dark)
        .escape_to_exit(true);

    let mut ui_language = libluna_i18n::locale().language().to_owned();

    run(config, move |frame: &mut Frame| {
        let language = libluna_i18n::locale().language().to_owned();
        if language != ui_language {
            ui_language = language;
            frame.set_title(tr("Welcome to Luna"));
        }

        crate::ui::handle_events(&mut app, frame.events, frame.size, &frame.theme, frame.text);
        app.tick(frame.delta_time());
        crate::ui::paint(&app, frame.scene, frame.size, &frame.theme, frame.text);
        app.apply_pending(frame);

        if app.needs_frames() {
            frame.request_frame_after(Duration::from_millis(250));
        }
    })
}
