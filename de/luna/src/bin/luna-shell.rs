//! Panel Luny na żywo — powierzchnia warstwowa (`wlr-layer-shell`) na Selene.
//!
//! Uruchom w sesji Luny: `luna-shell` (albo `cargo run -p luna --bin luna-shell`).
//! Wymaga kompozytora obsługującego `zwlr_layer_shell_v1` (Selene tak).

use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use libluna_core::{Rect, Theme};
use libluna_platform::{run, AppConfig, Frame, LayerConfig};
use libluna_widgets::PaintCtx;
use luna::panel::{self, PanelState, THICKNESS};

fn main() -> Result<(), Box<dyn Error>> {
    let cfg = AppConfig::new("Luna Panel")
        .app_id("eu.paganlinux.Luna.Panel")
        .layer(LayerConfig::top_panel("luna-panel", THICKNESS as u32, true));

    let mut state = PanelState::sample();

    run(cfg, move |frame: &mut Frame| {
        // Żywy zegar (UTC) — panel odświeża się w stałym rytmie.
        let (clock, date) = now_clock();
        state.clock = clock;
        state.date = date;

        let size = frame.size;
        let theme: &Theme = &frame.theme;
        let mut ctx = PaintCtx::new(frame.scene, theme, frame.text);
        panel::paint_panel(
            &state,
            &mut ctx,
            Rect::from_xywh(0.0, 0.0, size.width, THICKNESS),
        );

        frame.request_frame();
    })
}

/// Bieżąca godzina i data w UTC (bez zależności na TZ) — `("HH:MM", "DD.MM")`.
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

/// Data cywilna (rok, miesiąc, dzień) z liczby dni od 1970-01-01 (algorytm Hinnanta).
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
