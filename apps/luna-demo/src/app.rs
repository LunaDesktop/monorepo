//! Pełne, interaktywne demo Ustawień Luny — to samo, które widać na obrazach
//! `docs/images/demo-step-*.png` (sidebar, motywy, akcenty, język, zakładki,
//! pliki, popovery, powiadomienia i dolny panel).
//!
//! Wspólne dla binarek `luna-demo` i `luna-showcase` („Luna — widgety”), żeby
//! instalowana aplikacja pokazywała **cały** zestaw widgetów, a nie uproszczony mockup.

use std::error::Error;

use crate::ui::{self, Demo};
use libluna_core::ThemeSettings;
use libluna_i18n::set_locale;
use libluna_platform::{run as run_app, AppConfig, Frame};

/// Otwiera okno pełnego demo i uruchamia pętlę zdarzeń (start: „Wygląd”).
pub fn run() -> Result<(), Box<dyn Error>> {
    run_with_start(None)
}

/// Otwiera demo od razu na stronie „Widgety” (galeria komponentów).
pub fn run_widgets() -> Result<(), Box<dyn Error>> {
    run_with_start(Some(ui::WIDGETS_NAV))
}

fn run_with_start(start_nav: Option<usize>) -> Result<(), Box<dyn Error>> {
    set_locale(libluna_i18n::preferred_locale());
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
        .initial_size(1200, 1028)
        .settings(settings)
        .system_dark(system_dark);

    let mut demo = Demo::default();
    if let Some(i) = start_nav {
        demo.nav_selected = Some(i);
    }

    run_app(config, move |frame: &mut Frame| {
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

        // „Kopiuj” w galerii widgetów → systemowy schowek (wl_data_device).
        if let Some(text) = demo.gallery.copy_request.take() {
            frame.copy_to_clipboard(text);
        }
        // Ctrl+C/X w polach tekstowych → systemowy schowek.
        if let Some(text) = demo.clipboard_text.take() {
            frame.copy_to_clipboard(text);
        }
        // Ctrl+V → odczyt schowka; wynik wróci do UI jako `Event::Paste`.
        if demo.paste_requested {
            demo.paste_requested = false;
            frame.paste_from_clipboard();
        }

        // Akcje okna zgłoszone przez HeaderBar (kontrolki).
        if demo.pending_close {
            if std::env::var("LUNA_DEBUG_INPUT").is_ok() {
                eprintln!("demo: frame.close()");
            }
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
        // Przeciąganie okna za pasek tytułu (CSD) — ruch wykonuje kompozytor.
        if demo.pending_move {
            if std::env::var("LUNA_DEBUG_INPUT").is_ok() {
                eprintln!("demo: frame.begin_move()");
            }
            frame.begin_move();
            demo.pending_move = false;
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

        // Animacja powiadomienia: krok i prośba o kolejną klatkę, dopóki trwa.
        if demo.toast.is_animating() {
            demo.toast.advance(frame.delta_time(), 0.3);
            frame.request_frame();
        }

        // Animacja przejścia zakładek (`Stack` slide).
        if demo.tab_stack.is_animating() {
            demo.tab_stack.advance(frame.delta_time(), 0.25);
            frame.request_frame();
        }

        // Animacja dolnego panelu (wysuwanie) i przyciągania karuzeli.
        if demo.sheet.is_animating() {
            demo.sheet.advance(frame.delta_time(), 0.3);
            frame.request_frame();
        }
        if demo.pager.is_animating() {
            demo.pager.advance(frame.delta_time(), 0.3);
            frame.request_frame();
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
