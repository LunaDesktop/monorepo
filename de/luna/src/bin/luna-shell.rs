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
use libluna_widgets::{IconName, PaintCtx};
use luna::notifications::Notifications;
use luna::osd::Osd;
use luna::panel::{
    self, App, Launcher, PanelState, Toggle, ToggleAction, LAUNCHER_H, LAUNCHER_W, THICKNESS,
};

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
    let mut osd = Osd::new(IconName::Volume, "Głośność", 0.0);
    let mut notes = Notifications::new();
    // Lista okien z kompozytora (foreign-toplevel) — własne połączenie + wątek.
    let toplevels = luna::toplevels::Toplevels::spawn();
    // Realny stan systemu (użytkownik, dźwięk, Wi-Fi, bateria) — odpytywany w tle.
    let system = luna::system::spawn();
    // Sygnatura stanu — przerysowujemy tylko, gdy coś się zmieniło.
    let mut last_sig: (bool, String, u64) = (false, String::new(), u64::MAX);

    run(cfg, move |frame: &mut Frame| {
        let dt = frame.delta_time();

        let size = frame.size;
        let theme = &frame.theme;
        let text = frame.text;
        let bar = Rect::from_xywh(0.0, 0.0, size.width, THICKNESS);

        // Zegar (UTC) i lista okien z kompozytora (taskbar) — na bieżąco.
        let (clock, date) = now_clock();
        state.clock = clock;
        state.date = date;
        state.title.clear();
        if let Some(tl) = &toplevels {
            state.tasks = tl.tasks();
        } else {
            state.tasks.clear();
        }
        // Realny stan systemu → użytkownik i przełączniki.
        if let Ok(sys) = system.lock() {
            state.user = sys.user.clone();
            state.toggles = build_toggles(&sys);
        }

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
                            launch(&apps, i);
                            open = false;
                        }
                    } else if let Some(i) =
                        panel::task_at(bar, &frame.theme, text, &state.tasks, pe.position)
                    {
                        // Klik w okno na pasku — aktywuj je.
                        if let Some(tl) = &toplevels {
                            tl.activate(i);
                        }
                    } else if let Some(i) =
                        panel::toggle_at(bar, &frame.theme, text, &state, pe.position)
                    {
                        // Klik w przełącznik — realna akcja systemowa.
                        match state.toggles[i].action {
                            ToggleAction::Wifi => {
                                luna::system::set_wifi(!state.toggles[i].active);
                            }
                            ToggleAction::Volume => luna::system::toggle_mute(),
                            ToggleAction::None => {}
                        }
                    }
                }
                Event::PointerButton(pe)
                    if pe.state == ButtonState::Pressed && pe.button == PointerButton::Middle =>
                {
                    // Środkowy klik na oknie na pasku — zamknij je.
                    if let Some(i) =
                        panel::task_at(bar, &frame.theme, text, &state.tasks, pe.position)
                    {
                        if let Some(tl) = &toplevels {
                            tl.close(i);
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
                Event::Key(ke) if ke.state == ButtonState::Pressed && open => match ke.keysym {
                    0xff1b => open = false, // Escape
                    // Góra/dół: pionowa nawigacja.
                    0xff52 | 0xff54 => {
                        let dy = if ke.keysym == 0xff52 { -1 } else { 1 };
                        launcher.navigate(apps.len(), 0, dy);
                        panel::ensure_selection_visible(
                            &apps,
                            &mut launcher,
                            larea,
                            &frame.theme,
                            text,
                        );
                    }
                    // Lewo/prawo: pozioma nawigacja (kolumna w siatce, karta w karuzeli).
                    0xff51 | 0xff53 => {
                        let dx = if ke.keysym == 0xff51 { -1 } else { 1 };
                        launcher.navigate(apps.len(), dx, 0);
                        panel::ensure_selection_visible(
                            &apps,
                            &mut launcher,
                            larea,
                            &frame.theme,
                            text,
                        );
                    }
                    // Enter / Enter (numeryczny): uruchom zaznaczoną pozycję.
                    0xff0d | 0xff8d => {
                        if let Some(i) = launcher.selected_index(apps.len()) {
                            launch(&apps, i);
                            open = false;
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        // OSD/powiadomienia sterowane zdarzeniami (demo usunięte).
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

        // Przerysowujemy tylko, gdy coś się zmieniło — inaczej powłoka nie commituje
        // bufora 1920x680 przy każdej klatce (mniej pracy dla kompozytora i CPU).
        let mut th: u64 = state.tasks.len() as u64;
        for t in &state.tasks {
            th = th
                .wrapping_mul(1_099_511_628_211)
                .wrapping_add(u64::from(t.active) ^ t.title.len() as u64);
        }
        for tg in &state.toggles {
            th = th
                .wrapping_mul(1_099_511_628_211)
                .wrapping_add(u64::from(tg.active) ^ tg.label.len() as u64);
        }
        th = th
            .wrapping_mul(1_099_511_628_211)
            .wrapping_add(state.user.len() as u64);
        let sig = (open, state.clock.clone(), th);
        let animating = osd.visible() || notes.visible();
        if !frame.events.is_empty() || animating || open || sig != last_sig {
            frame.request_frame();
        }
        last_sig = sig;
    })
}

/// Buduje przełączniki paska z realnego stanu systemu.
fn build_toggles(s: &luna::system::SystemStatus) -> Vec<Toggle> {
    let mut v = Vec::new();
    if let Some((on, ssid)) = &s.wifi {
        v.push(Toggle {
            label: ssid.clone().unwrap_or_else(|| "Wi-Fi".to_owned()),
            icon: IconName::Wifi,
            active: *on,
            action: ToggleAction::Wifi,
        });
    }
    if let Some((pct, muted)) = s.volume {
        v.push(Toggle {
            label: format!("{pct}%"),
            icon: IconName::Volume,
            active: !muted,
            action: ToggleAction::Volume,
        });
    }
    if let Some((pct, charging)) = s.battery {
        v.push(Toggle {
            label: format!("{pct}%"),
            icon: IconName::Battery,
            active: charging,
            action: ToggleAction::None,
        });
    }
    v
}

/// Uruchamia aplikację o indeksie `i` (jeśli ma polecenie).
fn launch(apps: &[App], i: usize) {
    if let Some(exec) = apps.get(i).and_then(|a| a.exec.clone()) {
        let _ = std::process::Command::new("sh").arg("-c").arg(exec).spawn();
    }
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
