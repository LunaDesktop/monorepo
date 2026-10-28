//! Najmniejsza sensowna aplikacja na libLunie: okno, reakcja na klik, motyw i i18n.
//!
//! Uruchom: `cargo run -p libluna --example minimal` (wymaga kompozytora Wayland).

use libluna::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Język dobrany ze środowiska (LANG/LC_*); dołóż własne katalogi, jeśli chcesz.
    set_locale(Locale::from_env());

    let config = AppConfig::new("Hello libLuna").initial_size(640, 400);
    let mut highlighted = true;

    run(config, move |frame: &mut Frame| {
        // 1. Zdarzenia wejścia od poprzedniej klatki.
        for event in frame.events {
            if let Event::PointerButton(pe) = event {
                if pe.button == PointerButton::Left && pe.state == ButtonState::Pressed {
                    highlighted = !highlighted;
                }
            }
        }

        // 2. Rysowanie sceny (w pikselach logicznych).
        let theme = frame.theme;
        let (w, h) = (frame.size.width, frame.size.height);
        let scene = &mut *frame.scene;

        scene.fill_rect(Rect::from_xywh(0.0, 0.0, w, h), theme.colors.window_bg);
        scene.fill_rounded_rect(
            Rect::from_xywh(24.0, 24.0, w - 48.0, 96.0),
            theme.radius.large,
            if highlighted {
                theme.colors.accent
            } else {
                theme.colors.card_bg
            },
        );

        let fg = if highlighted {
            theme.colors.accent_fg
        } else {
            theme.colors.text
        };
        scene.text(
            TextRun::new(Point::new(44.0, 82.0), tr("Application"), 20.0)
                .with_weight(700)
                .with_color(fg),
        );
        scene.text(
            TextRun::new(Point::new(24.0, 168.0), "Click anywhere to toggle", 14.0)
                .with_color(theme.colors.dim_text),
        );
    })
}
