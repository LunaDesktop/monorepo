//! Paleta zrzutów ekranu (`Super+F12`): wszystkie monitory, numer monitora
//! albo zaznaczenie obszaru.
//!
//! Paletę rysuje wątek wyjścia razem ze sceną pulpitu, a wejście obsługuje
//! wątek protokołu (jak podgląd `Super+Tab`). Dzięki temu nie potrzebujemy
//! osobnego klienta Wayland tylko po to, by pokazać pastylkę.
//!
//! Zrzuty robimy przez [`crate::output::capture_output_rgba`] — tę samą ścieżkę,
//! której używa `wlr-screencopy`, więc w kadrze są też okna `dmabuf`. UI palety
//! jest wtedy wyłączone (nie trafia na zrzut).

use std::path::PathBuf;
use std::process::Stdio;

use libluna_core::{ButtonState, Point, PointerButton, Rect};
use libluna_i18n::tr;

use crate::state::{ShotUi, State};
use crate::sync::LockExt;

/// Wysokość pastylki.
pub const PILL_H: f64 = 56.0;
/// Wysokość przycisku (chipa) w pastylce.
const CHIP_H: f64 = 38.0;
/// Wewnętrzny margines pastylki.
const PAD: f64 = 9.0;
/// Odstęp między chipami.
const GAP: f64 = 8.0;
/// Odległość pastylki od dolnej krawędzi monitora.
const BOTTOM: f64 = 48.0;

const KEY_ESC: u32 = 0xff1b;
const KEY_TAB: u32 = 0xff09;
const KEY_ENTER: u32 = 0xff0d;
const KEY_KP_ENTER: u32 = 0xff8d;
const KEY_SPACE: u32 = 0x20;
const KEY_LEFT: u32 = 0xff51;
const KEY_UP: u32 = 0xff52;
const KEY_RIGHT: u32 = 0xff53;
const KEY_DOWN: u32 = 0xff54;

/// Liczba opcji w pastylce: wszystkie monitory + `monitors` numerów + obszar.
pub fn option_count(monitors: usize) -> usize {
    monitors.max(1) + 2
}

/// Przybliżona szerokość etykiety (ten sam wzór w rysowaniu i trafieniach).
fn label_width(label: &str) -> f64 {
    (label.chars().count() as f64 * 7.4 + 26.0).max(CHIP_H)
}

/// Etykieta opcji: `0` = wszystkie, `1..=monitors` = numer, reszta = obszar.
pub fn chip_label(index: usize, monitors: usize) -> String {
    let monitors = monitors.max(1);
    if index == 0 {
        tr("All monitors")
    } else if index <= monitors {
        index.to_string()
    } else {
        tr("Select area")
    }
}

/// Prostokąt pastylki wyśrodkowany w monitorze-gospodarzu.
pub fn pill_rect(host: Rect, monitors: usize) -> Rect {
    let monitors = monitors.max(1);
    let chips: f64 = (0..option_count(monitors))
        .map(|index| label_width(&chip_label(index, monitors)))
        .sum();
    let width = (chips + GAP * (option_count(monitors) - 1) as f64 + 2.0 * PAD)
        .min((host.width() - 24.0).max(120.0));
    let x = host.center().x - width / 2.0;
    let y = (host.bottom() - BOTTOM - PILL_H).max(host.y() + 12.0);
    Rect::from_xywh(x, y, width, PILL_H)
}

/// Prostokąty chipów wewnątrz pastylki (indeks opcji + geometria).
pub fn chip_rects(pill: Rect, monitors: usize) -> Vec<(usize, Rect)> {
    let monitors = monitors.max(1);
    let y = pill.y() + (pill.height() - CHIP_H) / 2.0;
    let mut x = pill.x() + PAD;
    let mut out = Vec::with_capacity(option_count(monitors));
    for index in 0..option_count(monitors) {
        let w = label_width(&chip_label(index, monitors));
        out.push((index, Rect::from_xywh(x, y, w, CHIP_H)));
        x += w + GAP;
    }
    out
}

/// Opcja pod punktem (do myszy).
pub fn hit(pill: Rect, monitors: usize, p: Point) -> Option<usize> {
    chip_rects(pill, monitors)
        .into_iter()
        .find(|(_, rect)| rect.contains(p))
        .map(|(index, _)| index)
}

/// Kolejna/poprzednia opcja z zawijaniem.
pub fn cycle(selected: usize, monitors: usize, delta: isize) -> usize {
    let count = option_count(monitors) as isize;
    (selected as isize + delta).rem_euclid(count) as usize
}

/// Otwiera paletę na monitorze pod kursorem (albo pierwszym).
pub fn open(state: &mut State) {
    let monitors = monitor_rects(state);
    let cursor = Point::new(state.pointer_pos.0, state.pointer_pos.1);
    let host = monitors
        .iter()
        .copied()
        .find(|rect| rect.contains(cursor))
        .or_else(|| monitors.first().copied())
        .unwrap_or_else(|| Rect::from_xywh(0.0, 0.0, 1280.0, 800.0));
    let count = monitors.len().max(1);
    let pill = pill_rect(host, count);
    let mut ov = state.overview.lock_or_recover();
    // Paleta i podgląd okien wykluczają się — obie zajmują cały ekran.
    ov.on = false;
    ov.shot = Some(ShotUi {
        selected: 0,
        monitors: count,
        hover: None,
        selecting: false,
        area: None,
        highlight: None,
        pill,
    });
    drop(ov);
    // Klient nie może dalej dostawać ruchu „pod” paletą.
    crate::seat::pointer_leave(state);
}

/// Zamyka paletę (Esc, klik poza pastylką albo po zrzucie).
pub fn close(state: &mut State) {
    state.overview.lock_or_recover().shot = None;
}

fn monitor_rects(state: &State) -> Vec<Rect> {
    state
        .outputs
        .lock_or_recover()
        .iter()
        .map(|output| output.rect())
        .collect()
}

fn active(state: &State) -> Option<ShotUi> {
    state.overview.lock_or_recover().shot
}

fn update(state: &mut State, ui: ShotUi) {
    state.overview.lock_or_recover().shot = Some(ui);
}

/// Podświetlenie monitora odpowiadającego opcji (tylko dla numerów).
fn highlight(state: &State, option: Option<usize>) -> Option<Rect> {
    let option = option?;
    if option == 0 || option > monitor_rects(state).len() {
        return None;
    }
    monitor_rects(state).get(option - 1).copied()
}

/// Obsługuje klawiaturę palety; `true` = klawisz zużyty.
pub fn handle_key(state: &mut State, keysym: u32, st: ButtonState) -> bool {
    let Some(mut ui) = active(state) else {
        return false;
    };
    // Wydarzenia zwolnienia też zjadamy, żeby nie trafiły do klienta.
    if st != ButtonState::Pressed {
        return true;
    }
    if keysym == KEY_ESC {
        close(state);
        return true;
    }
    // Powtórne `Super+F12` / `Print` zamyka paletę (przełącznik jak w GNOME).
    if keysym == 0xff61 || (keysym == 0xffc9 && state.sent_mods.logo) {
        close(state);
        return true;
    }
    if ui.selecting {
        // W trybie zaznaczania klawiatura tylko anuluje; resztę obsługuje mysz.
        return true;
    }
    let shift = state.sent_mods.shift;
    match keysym {
        KEY_LEFT | KEY_UP => ui.selected = cycle(ui.selected, ui.monitors, -1),
        KEY_RIGHT | KEY_DOWN => ui.selected = cycle(ui.selected, ui.monitors, 1),
        KEY_TAB => {
            ui.selected = cycle(ui.selected, ui.monitors, if shift { -1 } else { 1 });
        }
        KEY_ENTER | KEY_KP_ENTER | KEY_SPACE => {
            ui.hover = None;
            ui.highlight = highlight(state, Some(ui.selected));
            update(state, ui);
            commit(state);
            return true;
        }
        0x31..=0x39 => {
            // Cyfra = numer monitora (bez Enter).
            let index = (keysym - 0x30) as usize;
            if index <= ui.monitors {
                ui.selected = index;
                ui.hover = None;
                ui.highlight = highlight(state, Some(index));
                update(state, ui);
                commit(state);
                return true;
            }
        }
        _ => {}
    }
    ui.hover = None;
    ui.highlight = highlight(state, Some(ui.selected));
    update(state, ui);
    true
}

/// Uruchamia opcję wybraną w pastylce.
fn commit(state: &mut State) {
    let Some(ui) = active(state) else {
        return;
    };
    if ui.selected == 0 {
        capture_all(state);
        close(state);
    } else if ui.selected <= ui.monitors {
        if let Some(path) = capture_monitor(state, ui.selected - 1) {
            notify_saved(&path);
        }
        close(state);
    } else {
        let mut ui = ui;
        ui.selecting = true;
        ui.area = None;
        ui.hover = None;
        ui.highlight = None;
        update(state, ui);
    }
}

/// Ruch kursora: podświetla opcję albo rozciąga zaznaczenie obszaru.
/// `true` = zdarzenie zużyte (nie trafia do klientów).
pub fn pointer_motion(state: &mut State, pos: Point) -> bool {
    let Some(mut ui) = active(state) else {
        return false;
    };
    if ui.selecting {
        if let Some((start, _)) = ui.area {
            ui.area = Some((start, pos));
        }
    } else {
        ui.hover = hit(ui.pill, ui.monitors, pos);
        ui.highlight = highlight(state, ui.hover.or(Some(ui.selected)));
    }
    update(state, ui);
    true
}

/// Kliknięcia w palecie / zaznaczanie obszaru; `true` = zdarzenie zużyte.
pub fn pointer_button(
    state: &mut State,
    pos: Point,
    button: PointerButton,
    st: ButtonState,
) -> bool {
    let Some(mut ui) = active(state) else {
        return false;
    };
    if button != PointerButton::Left {
        return true;
    }
    if ui.selecting {
        match st {
            ButtonState::Pressed => ui.area = Some((pos, pos)),
            ButtonState::Released => {
                if let Some((start, end)) = ui.area {
                    let area = normalize(start, end);
                    if area.width() >= 4.0 && area.height() >= 4.0 {
                        capture_region(state, area);
                        close(state);
                        return true;
                    }
                }
                // Za mały ruch — zostajemy w trybie zaznaczania.
                ui.area = None;
            }
        }
        update(state, ui);
        return true;
    }
    if st == ButtonState::Pressed {
        if let Some(index) = hit(ui.pill, ui.monitors, pos) {
            ui.selected = index;
            update(state, ui);
            commit(state);
        } else {
            // Klik poza pastylką zamyka paletę bez zrzutu.
            close(state);
        }
    }
    true
}

fn normalize(a: Point, b: Point) -> Rect {
    Rect::from_xywh(
        a.x.min(b.x),
        a.y.min(b.y),
        (a.x - b.x).abs(),
        (a.y - b.y).abs(),
    )
}

fn capture_all(state: &mut State) {
    let count = state.outputs.lock_or_recover().len().max(1);
    let mut last = None;
    for index in 0..count {
        last = capture_monitor(state, index).or(last);
    }
    if let Some(path) = last {
        // Jedno powiadomienie na całą serię — nie zasypujemy użytkownika toastami.
        notify_saved(&path);
    }
}

fn capture_monitor(state: &mut State, index: usize) -> Option<PathBuf> {
    let name = state
        .outputs
        .lock_or_recover()
        .get(index)
        .map(|output| sanitize(&output.name))
        .unwrap_or_else(|| format!("monitor-{}", index + 1));
    let Some((rgba, width, height)) =
        crate::output::capture_output_rgba(state, index, false, None, true)
    else {
        eprintln!("selene: nie udało się przechwycić monitora {index}");
        return None;
    };
    let path = crate::output::write_screenshot_png(&name, width, height, rgba);
    if let Some(path) = &path {
        eprintln!("selene: zrzut ekranu: {}", path.display());
    }
    path
}

/// Zrzut obszaru w globalnych pikselach. Obszar może leżeć na kilku monitorach —
/// sklejamy wycinki w jeden obraz.
fn capture_region(state: &mut State, area: Rect) {
    let outputs: Vec<_> = state.outputs.lock_or_recover().clone();
    let width = area.width().round().max(1.0) as u32;
    let height = area.height().round().max(1.0) as u32;
    let mut canvas = vec![0u8; width as usize * height as usize * 4];
    let mut any = false;
    for (index, output) in outputs.iter().enumerate() {
        let Some(clip) = intersect(&area, &output.rect()) else {
            continue;
        };
        let local = Rect::from_xywh(
            clip.x() - output.x as f64,
            clip.y() - output.y as f64,
            clip.width(),
            clip.height(),
        );
        let Some((rgba, w, h)) = crate::output::capture_output_rgba(
            state,
            index,
            false,
            Some((
                local.x() as i32,
                local.y() as i32,
                local.width() as i32,
                local.height() as i32,
            )),
            true,
        ) else {
            continue;
        };
        let dx = (clip.x() - area.x()).round() as i64;
        let dy = (clip.y() - area.y()).round() as i64;
        if blit(&mut canvas, width, height, &rgba, w, h, dx, dy) {
            any = true;
        }
    }
    if !any {
        eprintln!("selene: pusty obszar zrzutu");
        return;
    }
    if let Some(path) = crate::output::write_screenshot_png("obszar", width, height, canvas) {
        eprintln!("selene: zrzut ekranu: {}", path.display());
        notify_saved(&path);
    }
}

/// Pokazuje natywne powiadomienie Luny (usługa `org.freedesktop.Notifications`).
///
/// Powiadomienie ma akcję „Otwórz lokalizację”: kliknięcie toasta lub przycisku
/// wysyła `ActionInvoked`, a `notify-send` (uruchomiony z `--action`) kończy się
/// i wypisuje `default` — wtedy otwieramy folder zrzutu przez `xdg-open`.
/// Czekanie idzie w osobnym wątku, żeby nie blokować pętli kompozytora.
///
/// Gdy `notify-send` nie jest zainstalowany, zostaje tylko wpis w logu — zrzut
/// i tak jest zapisany, więc nie traktujemy tego jako błąd.
pub(crate) fn notify_saved(path: &std::path::Path) {
    let title = tr("Screenshot saved");
    let body = path.display().to_string();
    let action = format!("--action=default={}", tr("Open location"));
    let opened = path.to_path_buf();
    let spawned = std::thread::Builder::new()
        .name("luna-screenshot-notify".to_owned())
        .spawn(move || {
            for program in [
                "notify-send",
                "/usr/bin/notify-send",
                "/usr/sbin/notify-send",
            ] {
                let child = std::process::Command::new(program)
                    .args(["--app-name=Luna", "--icon=luna", "--expire-time=6000"])
                    .arg(&action)
                    .arg(&title)
                    .arg(&body)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn();
                let Ok(child) = child else {
                    continue;
                };
                let chosen = child
                    .wait_with_output()
                    .ok()
                    .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
                    .unwrap_or_default();
                if chosen == "default" {
                    open_location(&opened);
                }
                return;
            }
        });
    if spawned.is_err() {
        eprintln!("selene: nie udało się pokazać powiadomienia o zrzucie");
    }
}

/// Otwiera folder ze zrzutem w domyślnym menedżerze plików.
fn open_location(path: &std::path::Path) {
    let folder = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let _ = std::process::Command::new("xdg-open")
        .arg(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn intersect(a: &Rect, b: &Rect) -> Option<Rect> {
    let x0 = a.x().max(b.x());
    let y0 = a.y().max(b.y());
    let x1 = a.right().min(b.right());
    let y1 = a.bottom().min(b.bottom());
    (x1 > x0 && y1 > y0).then(|| Rect::from_xywh(x0, y0, x1 - x0, y1 - y0))
}

/// Kopiuje wycinek `src` do `dst` w punkcie `(dx, dy)` (przycina do brzegów).
fn blit(
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    src: &[u8],
    src_w: u32,
    src_h: u32,
    dx: i64,
    dy: i64,
) -> bool {
    let stride = dst_w as usize * 4;
    let src_stride = src_w as usize * 4;
    let mut copied = false;
    for y in 0..src_h as i64 {
        let ty = dy + y;
        if ty < 0 || ty >= dst_h as i64 {
            continue;
        }
        for x in 0..src_w as i64 {
            let tx = dx + x;
            if tx < 0 || tx >= dst_w as i64 {
                continue;
            }
            let si = y as usize * src_stride + x as usize * 4;
            let di = ty as usize * stride + tx as usize * 4;
            dst[di..di + 4].copy_from_slice(&src[si..si + 4]);
            copied = true;
        }
    }
    copied
}

fn sanitize(name: &str) -> String {
    let out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if out.trim_matches('-').is_empty() {
        "ekran".to_owned()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> Rect {
        Rect::from_xywh(0.0, 0.0, 1280.0, 800.0)
    }

    #[test]
    fn pill_is_centered_and_inside_the_host() {
        let pill = pill_rect(host(), 2);
        assert!(pill.width() > 0.0);
        assert!(pill.right() <= host().right());
        assert!((pill.center().x - host().center().x).abs() < 1.0);
        assert!(pill.bottom() < host().bottom());
    }

    #[test]
    fn chips_do_not_overlap_and_cover_every_option() {
        let monitors = 3;
        let pill = pill_rect(host(), monitors);
        let chips = chip_rects(pill, monitors);
        assert_eq!(chips.len(), option_count(monitors));
        for pair in chips.windows(2) {
            assert!(pair[0].1.right() <= pair[1].1.x());
        }
        for (_, chip) in &chips {
            assert!(chip.x() >= pill.x() && chip.right() <= pill.right() + 0.5);
        }
    }

    #[test]
    fn hit_maps_numbers_to_monitors() {
        let monitors = 2;
        let pill = pill_rect(host(), monitors);
        let chips = chip_rects(pill, monitors);
        assert_eq!(hit(pill, monitors, chips[0].1.center()), Some(0));
        assert_eq!(hit(pill, monitors, chips[1].1.center()), Some(1));
        assert_eq!(hit(pill, monitors, chips[2].1.center()), Some(2));
        assert_eq!(hit(pill, monitors, chips[3].1.center()), Some(3));
        assert_eq!(hit(pill, monitors, Point::new(0.0, 0.0)), None);
    }

    #[test]
    fn options_cycle_both_ways() {
        let monitors = 2; // 0=all, 1..2=monitors, 3=area
        assert_eq!(cycle(0, monitors, -1), 3);
        assert_eq!(cycle(3, monitors, 1), 0);
        assert_eq!(cycle(1, monitors, 1), 2);
    }

    #[test]
    fn area_is_normalized_from_any_drag_direction() {
        let r = normalize(Point::new(40.0, 30.0), Point::new(10.0, 60.0));
        assert_eq!(
            (r.x(), r.y(), r.width(), r.height()),
            (10.0, 30.0, 30.0, 30.0)
        );
    }

    #[test]
    fn region_is_clipped_to_monitor() {
        let out = Rect::from_xywh(0.0, 0.0, 100.0, 50.0);
        let clip = intersect(&Rect::from_xywh(90.0, 40.0, 50.0, 50.0), &out).unwrap();
        assert_eq!((clip.width(), clip.height()), (10.0, 10.0));
        assert!(intersect(&Rect::from_xywh(200.0, 200.0, 10.0, 10.0), &out).is_none());
    }

    #[test]
    fn blit_places_the_region_and_clips_edges() {
        let mut dst = vec![0u8; 4 * 4 * 4];
        let src = vec![7u8; 3 * 3 * 4];
        assert!(blit(&mut dst, 4, 4, &src, 3, 3, 3, 3));
        assert_eq!(&dst[(3 * 4 + 3) * 4..(3 * 4 + 3) * 4 + 4], &[7, 7, 7, 7]);
        // Poza obszarem nic nie jest kopiowane.
        assert!(!blit(&mut dst, 4, 4, &src, 3, 3, 10, 10));
    }
}
