//! Powłoka Luny na żywo — pasek, OSD i powiadomienia jako powierzchnia warstwowa.
//!
//! `luna-shell` tworzy powierzchnię „chrome" (`wlr-layer-shell`): pasek na górze,
//! a pod nim przezroczysty obszar na OSD i powiadomienia. Bez usługi D-Bus
//! `org.freedesktop.Notifications` (kolejny krok) demo samo cyklicznie pokazuje OSD
//! i powiadomienia, żeby warstwę prezentacji dało się zobaczyć na żywo.

use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use libluna_core::{Rect, Theme};
use libluna_platform::{run, AppConfig, Frame, LayerConfig};
use libluna_widgets::PaintCtx;
use luna::notifications::{Notification, Notifications};
use luna::osd::Osd;
use luna::panel::{self, PanelState, THICKNESS};

/// Wysokość powierzchni „chrome" (pasek + obszar nakładek).
const CHROME_H: u32 = 240;

fn main() -> Result<(), Box<dyn Error>> {
    let cfg = AppConfig::new("Luna Shell")
        .app_id("eu.paganlinux.Luna.Shell")
        .layer(LayerConfig::top_chrome(
            "luna-shell",
            THICKNESS as u32,
            CHROME_H,
        ))
        .transparent(true);

    let mut state = PanelState::sample();
    let mut osd = Osd::new("◆", "Głośność", 0.0);
    let mut notes = Notifications::new();
    let mut timer = 0.0f64;

    run(cfg, move |frame: &mut Frame| {
        let dt = frame.delta_time();
        timer += dt;

        // Zegar (UTC).
        let (clock, date) = now_clock();
        state.clock = clock;
        state.date = date;

        // Demo: co ~4 s OSD, co ~7 s powiadomienie.
        if timer % 7.0 < dt {
            let value = 0.15 + (timer as i64 % 8) as f64 / 10.0;
            osd.show("Głośność", value);
        }
        if timer % 4.0 < dt {
            notes.push(
                Notification::new("Pliki", "Kopiowanie zakończone", "12 plików → Dokumenty")
                    .with_glyph("▦"),
            );
        }
        osd.advance(dt);
        notes.advance(dt);

        let size = frame.size;
        let theme: &Theme = &frame.theme;
        let mut ctx = PaintCtx::new(frame.scene, theme, frame.text);
        // Pasek na górze.
        panel::paint_panel(
            &state,
            &mut ctx,
            Rect::from_xywh(0.0, 0.0, size.width, THICKNESS),
        );
        // OSD wyśrodkowany pod paskiem.
        if osd.visible() {
            let w = 340.0;
            osd.paint(
                &mut ctx,
                Rect::from_xywh((size.width - w) / 2.0, THICKNESS + 16.0, w, 72.0),
            );
        }
        // Powiadomienia w prawym-górnym narożniku (pod paskiem).
        notes.paint(
            &mut ctx,
            Rect::from_xywh(0.0, THICKNESS + 8.0, size.width, size.height - THICKNESS),
        );

        frame.request_frame();
    })
}

/// Bieżąca godzina i data w UTC — `("HH:MM", "DD.MM")`.
fn now_clock() -> (String, String) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let secs_of_day = secs % 86_400;
    let (h, m) = (secs_of_day / 3600, (secs_of_day % 3600) / 60);
    let (_, mo, d) = civil_from_days(days);
    (format!("{h:02}:{m:02}"), format!("{d:02}.{mo:02}"))
}

/// Data cywilna z liczby dni od 1970-01-01 (algorytm Hinnanta).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
