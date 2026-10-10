//! Demo aplikacji środowiska Luna — interaktywne okno z HeaderBar-em (CSD),
//! panelem wyglądu i i18n. Zbudowane na libLunie.

use std::error::Error;

use libluna_core::ThemeSettings;
use libluna_i18n::{set_locale, Locale};
use libluna_platform::{run, AppConfig, Frame};
use luna_demo::ui::{self, Demo};

fn main() -> Result<(), Box<dyn Error>> {
    set_locale(Locale::from_env());
    // Doładuj języki ze standardowych katalogów (nadpisują wbudowane).
    libluna_i18n::load_catalogs_from_dirs();

    // Ustawienia wyglądu wczytane z dysku (jeśli są).
    let settings = ThemeSettings::load();
    // Tryb systemowy: prosty przełącznik przez zmienną środowiskową (docelowo portal).
    let system_dark = std::env::var("LUNA_SYSTEM_DARK")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);

    let config = AppConfig::new("Aplikacja")
        .app_id("eu.paganlinux.Luna.Settings")
        .initial_size(900, 1028)
        .settings(settings)
        .system_dark(system_dark);

    let mut demo = Demo::default();

    run(config, move |frame: &mut Frame| {
        let mut settings = frame.settings();

        // Motyw (z ewentualnym stylem z pliku) do obsługi zdarzeń.
        let theme = settings.resolve_themed(frame.system_dark());
        ui::handle_events(
            &mut settings,
            frame.events,
            frame.size,
            &theme,
            frame.text,
            &mut demo,
        );

        // Akcje okna zgłoszone przez HeaderBar (kontrolki).
        if demo.pending_close {
            frame.close();
            demo.pending_close = false;
        }
        if demo.pending_minimize {
            frame.minimize();
            demo.pending_minimize = false;
        }
        if demo.pending_maximize {
            frame.toggle_maximize();
            demo.pending_maximize = false;
        }

        if settings.preference() != frame.preference() {
            frame.set_preference(settings.preference());
        }
        if settings.accent() != frame.accent() {
            frame.set_accent(settings.accent());
        }
        if settings.style() != frame.style() {
            frame.set_style(settings.style().map(str::to_owned));
        }

        // Motyw po zmianach (np. wybór stylu) — klatka od razu poprawna.
        let theme = settings.resolve_themed(frame.system_dark());
        ui::build_scene(
            frame.scene,
            &theme,
            frame.text,
            frame.size,
            &demo,
            settings.preference(),
            settings.accent(),
        );
    })
}
