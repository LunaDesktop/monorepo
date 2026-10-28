//! `luna-greeter` — ekran logowania Luny dla greetd.
//!
//! Uruchomienie produkcyjne (przez greetd / `luna-greeter-session`):
//! `luna-greeter`
//!
//! Podgląd na zwykłym pulpicie (bez przejmowania ekranu, bez logowania):
//! `luna-greeter --preview`

use std::error::Error;

use libluna_core::ThemeSettings;
use libluna_i18n::set_locale;
use libluna_platform::{run, AppConfig, Frame, LayerConfig};
use luna_greeter::app::GreeterApp;
use luna_greeter::{greetd, sessions, users};

fn main() -> Result<(), Box<dyn Error>> {
    let mut preview = std::env::var("LUNA_GREETER_PREVIEW")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);
    let mut list = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--preview" | "--demo" => preview = true,
            "--list" => list = true,
            "-h" | "--help" => {
                println!(
                    "luna-greeter — ekran logowania Luny (greetd)\n\n\
                     Użycie: luna-greeter [--preview] [--list]\n\n\
                     Opcje:\n  \
                       --preview   okno podglądu bez logowania (do testów UI)\n  \
                       --list      wypisz wykryte konta i sesje systemowe\n\n\
                     Zmienne:\n  \
                       GREETD_SOCK             gniazdo greetd (domyślnie /run/greetd-ipc)\n  \
                       LUNA_GREETER_SESSION    dodatkowa sesja / fallback\n  \
                       LUNA_GREETER_PREVIEW=1  to samo co --preview\n"
                );
                return Ok(());
            }
            _ => {}
        }
    }

    set_locale(libluna_i18n::preferred_locale());
    libluna_i18n::load_catalogs_from_dirs();

    if list {
        println!("Sesje:");
        for session in sessions::system_sessions() {
            println!(
                "  {:<24} {:<7} {}",
                session.name,
                session.kind.label(),
                session.command.join(" ")
            );
        }
        println!("Konta:");
        for user in users::local_users() {
            println!("  {:<20} {}", user.name, user.display);
        }
        let settings = libluna_core::GreeterSettings::load();
        println!("Ekran logowania:");
        println!(
            "  konfiguracja: {}",
            libluna_core::GreeterSettings::config_path().display()
        );
        println!(
            "  tapeta:      {}",
            settings
                .wallpaper
                .as_deref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "gradient akcentu".to_owned())
        );
        println!(
            "  motyw:       {} (akcent: {}, zegar: {})",
            settings.theme.name(),
            settings
                .accent
                .map(|accent| accent.name())
                .unwrap_or("domyślny"),
            if settings.show_clock { "tak" } else { "nie" }
        );
        return Ok(());
    }

    let system_dark = std::env::var("LUNA_SYSTEM_DARK")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);

    let socket = greetd::socket_path();
    let mut app = GreeterApp::new(socket, preview);

    // Motyw i akcent ekranu logowania pochodzą z `/etc/luna/greeter.conf`
    // (Ustawienia → Ekran logowania). Reszta wyglądu — z ustawień greetera.
    let mut theme_settings = ThemeSettings::load();
    theme_settings.set_preference(app.settings.theme);
    if let Some(accent) = app.settings.accent {
        theme_settings.set_accent(accent);
    }

    let mut config = AppConfig::new("Luna Greeter")
        .app_id("eu.luna.Greeter")
        .settings(theme_settings)
        .system_dark(system_dark)
        // Esc czyści pole, ale nie może zostawić systemu bez ekranu logowania.
        .escape_to_exit(false);
    if preview {
        // Podgląd uruchamiamy jako zwykłe okno — nie przejmujemy ekranu.
        config = config.initial_size(1200, 800);
    } else {
        config = config.layer(LayerConfig::lock_screen("luna-greeter"));
    }

    run(config, move |frame: &mut Frame| {
        app.update(frame);
        let theme = frame.theme;
        app.paint(frame.scene, frame.size, &theme, frame.text);
    })
}
