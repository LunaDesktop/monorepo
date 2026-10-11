//! Powłoka Luny na żywo — pasek, launcher (Lista/Siatka/Karuzela), OSD i powiadomienia.
//!
//! `luna-shell` tworzy powierzchnię „chrome" (`wlr-layer-shell`): pasek na górze,
//! a pod nim przezroczysty obszar na launcher, OSD i powiadomienia.
//!
//! Interakcja: klik w **burger** otwiera/zamyka launcher; w launcherze przełącznik
//! widoku (Lista/Siatka/Karuzela), klik w aplikację zamyka launcher; `Esc` zamyka.

use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use libluna_core::{ButtonState, Event, PointerButton, Rect};
use libluna_platform::{run, AppConfig, Frame, LayerConfig};
use libluna_widgets::PaintCtx;
use luna::notifications::{Notification, Notifications};
use luna::osd::Osd;
use luna::panel::{self, App, Launcher, PanelState, LAUNCHER_H, LAUNCHER_W, THICKNESS};

/// Wysokość powierzchni „chrome" (pasek + launcher + OSD/powiadomienia).
const CHROME_H: u32 = 680;

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
    // Prawdziwe aplikacje z plików `.desktop`; gdy brak (np. kontener) — zestaw demo.
    let desktop = luna::apps::scan();
    let apps = if desktop.is_empty() {
        App::sample()
    } else {
        App::from_desktop(&desktop)
    };
    let mut launcher = Launcher::new();
    let mut open = false;
    let mut osd = Osd::new("◆", "Głośność", 0.0);
    let mut notes = Notifications::new();
    let mut timer = 0.0f64;

    run(cfg, move |frame: &mut Frame| {
        let dt = frame.delta_time();
        timer += dt;

        let size = frame.size;
        let theme = &frame.theme;
        let text = frame.text;
        let bar = Rect::from_xywh(0.0, 0.0, size.width, THICKNESS);

        let lw = LAUNCHER_W.min((size.width - 80.0).max(320.0));
        let larea = Rect::from_xywh((size.width - lw) / 2.0, THICKNESS + 24.0, lw, LAUNCHER_H);

        // ── Obsługa wejścia ──
        for ev in frame.events {
            match ev {
                Event::PointerButton(pe)
                    if pe.state == ButtonState::Pressed && pe.button == PointerButton::Left =>
                {
                    if panel::burger_rect(bar, &frame.theme).contains(pe.position) {
                        open = !open;
                        if open {
                            let max = panel::launcher_scroll_max(
                                &apps,
                                &launcher,
                                larea,
                                &frame.theme,
                                text,
                            );
                            launcher.scroll = launcher.scroll.clamp(0.0, max);
                        }
                    } else if open {
                        if let Some(v) =
                            panel::launcher_view_at(larea, &frame.theme, text, pe.position)
                        {
                            launcher.view = v;
                            launcher.scroll = 0.0;
                        } else if let Some(i) = panel::launcher_app_at(
                            &apps,
                            &launcher,
                            larea,
                            &frame.theme,
                            text,
                            pe.position,
                        ) {
                            // Uruchom aplikację (jeśli ma polecenie).
                            if let Some(exec) = apps.get(i).and_then(|a| a.exec.clone()) {
                                let _ =
                                    std::process::Command::new("sh").arg("-c").arg(exec).spawn();
                            }
                            open = false;
                        }
                    }
                }
                Event::Scroll(se) if open => {
                    if panel::launcher_rects(larea, &frame.theme, text)
                        .content
                        .contains(se.position)
                    {
                        let max =
                            panel::launcher_scroll_max(&apps, &launcher, larea, &frame.theme, text);
                        launcher.scroll = (launcher.scroll - se.dy * 24.0).clamp(0.0, max);
                    }
                }
                Event::Key(ke) if ke.keysym == 0xff1b => open = false, // Escape
                _ => {}
            }
        }

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
                    .with_glyph("◆"),
            );
        }
        osd.advance(dt);
        notes.advance(dt);

        // ── Rysowanie ──
        let mut ctx = PaintCtx::new(frame.scene, theme, text);
        panel::paint_panel(&state, &mut ctx, bar);
        if open {
            panel::paint_launcher(&apps, &launcher, &mut ctx, larea);
        }
        if osd.visible() {
            let w = 340.0;
            osd.paint(
                &mut ctx,
                Rect::from_xywh((size.width - w) / 2.0, THICKNESS + 16.0, w, 72.0),
            );
        }
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
