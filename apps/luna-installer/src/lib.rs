//! Luna Installer — kreator instalacji systemu Luna z obrazu live.
//!
//! Aplikacja zbiera plan (dysk, konto, region), waliduje go i uruchamia
//! `luna-install-backend`. Sama nie formatuje dysków — cała praca z urządzeniami
//! jest w backendzie uruchamianym przez `pkexec`. Dzięki temu UI można testować
//! bez uprawnień roota, a dystrybucja może podmienić backend bez przebudowy
//! instalatora.

pub mod app;
pub mod backend;
pub mod checks;
pub mod disk;
pub mod keyboard;
pub mod live;
pub mod network;
pub mod plan;
pub mod translations;
pub mod ui;

pub use app::{App, Hit, Step};
pub use checks::Check;
pub use disk::{Disk, Partition};
pub use keyboard::Keyboard;
pub use live::LiveEnvironment;
pub use network::NetworkState;
pub use plan::{
    Account, Bootloader, Field, FileSystem, InstallOptions, InstallPlan, Issue, PartitionMode,
    RootMode, SwapChoice,
};

use std::error::Error;
use std::time::Duration;

use libluna_core::ThemeSettings;
use libluna_i18n::{set_locale, tr};
use libluna_platform::{run, AppConfig, Frame};

/// Uruchamia okno instalatora (binarka `luna-installer`).
pub fn run_installer() -> Result<(), Box<dyn Error>> {
    set_locale(libluna_i18n::preferred_locale());
    libluna_i18n::load_catalogs_from_dirs();

    let settings = ThemeSettings::load();
    let system_dark = std::env::var("LUNA_SYSTEM_DARK")
        .map(|value| !value.is_empty() && value != "0")
        .unwrap_or(false);

    let mut app = App::new();
    let mut ui_language = libluna_i18n::locale().language().to_owned();

    let config = AppConfig::new(tr("Install Luna"))
        .app_id("eu.luna.Installer")
        .initial_size(980, 720)
        .settings(settings)
        .system_dark(system_dark)
        .escape_to_exit(false);

    run(config, move |frame: &mut Frame| {
        let language = libluna_i18n::locale().language().to_owned();
        if language != ui_language {
            ui_language = language;
            frame.set_title(tr("Install Luna"));
        }

        app.poll_job();
        crate::ui::handle_events(&mut app, frame.events, frame.size, &frame.theme, frame.text);
        app.tick(frame.delta_time());
        crate::ui::paint(&app, frame.scene, frame.size, &frame.theme, frame.text);
        app.apply_pending(frame);

        if app.needs_frames() {
            frame.request_frame_after(Duration::from_millis(80));
        } else {
            frame.request_frame_after(Duration::from_secs(1));
        }
    })
}
