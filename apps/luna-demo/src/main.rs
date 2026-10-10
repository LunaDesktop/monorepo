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
    // Wybrany styl (instalowany z pliku) — wczytany raz, nakładany na każdą klatkę.
    let style_patch = settings.style().and_then(libluna_core::theme_file::load);
    // Tryb systemowy: prosty przełącznik przez zmienną środowiskową (docelowo portal).
    let system_dark = std::env::var("LUNA_SYSTEM_DARK")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);

    let config = AppConfig::new("Aplikacja")
        .app_id("eu.paganlinux.Luna.Settings")
        .initial_size(900, 1028)
        .settings(settings.clone())
        .system_dark(system_dark);

    let mut demo = Demo::default();

    run(config, move |frame: &mut Frame| {
        // Motyw rozwiązujemy z aktualnych ustawień (+ styl z pliku), aby klatka była poprawna.
        let mut theme = settings.resolve(frame.system_dark());
        if let Some(patch) = &style_patch {
            theme = patch.build(theme);
        }

        // Logika interakcji jest niezależna od Waylanda: dostaje zdarzenia i ustawienia.
        let mut settings = frame.settings();
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

        // Motyw rozwiązujemy z aktualnych ustawień, aby klatka była już poprawna.
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
