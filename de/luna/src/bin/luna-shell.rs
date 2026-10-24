/// Powłoka Luny na żywo — pasek, launcher, OSD, powiadomienia i wysuwane panele
/// (sesja, sieć Wi-Fi, dźwięk/głośność, kalendarz i zegar).
///
/// Interakcja:
/// - Klik w **burger** otwiera płynnie launcher (wysuwany z góry z przyciemnieniem tła).
/// - Klik w **zegar** na środku paska otwiera wysuwany kalendarz i zegar.
/// - Klik w przełącznik **Wi-Fi** otwiera menu sieci z listą i włącznikiem.
/// - Klik w przełącznik **dźwięku** otwiera panel regulacji głośności i wyciszenia.
/// - Klik w **zasilanie** lub nazwę użytkownika otwiera menu sesji (zablokuj, wyloguj, restart, wyłącz).
/// - Kliknięcie **poza** aktywnym panelem/launcherem lub naciśnięcie **Esc** zamyka go płynnie.
use std::error::Error;
use std::time::Instant;

use libluna_core::{ButtonState, Event, PointerButton, Rect, ThemePreference};
use libluna_platform::{run, AppConfig, Frame, InputRegion, LayerConfig};
use libluna_widgets::{paint_icon, IconName, PaintCtx};
use luna::dbus_notifications::{Incoming, NotificationsBus};
use luna::notifications::{Notification, Notifications};
use luna::osd::Osd;
use luna::panel::{
    self, ActivePopup, App, CalendarAction, Launcher, PanelState, Toggle, ToggleAction,
    VolumeAction, WifiAction, LAUNCHER_H, LAUNCHER_W, THICKNESS,
};

/// Wysokość powierzchni „chrome” (pasek + launcher + OSD/powiadomienia).
const CHROME_H: u32 = 680;

/// Jednostka przewijania w `wl_pointer.axis`: jeden „ząbek” kółka = tyle pikseli
/// powierzchni (patrz `selene::seat::WHEEL_STEP_PX`). Musi być z tym zgodna.
const SCROLL_UNIT: f64 = 20.0;
/// O ile pikseli przewijamy listę/siatkę launchera za jeden ząbek kółka.
/// ~64 px to około jeden wiersz listy — jedno kliknięcie kółka = jeden wiersz.
const LAUNCHER_SCROLL_PER_NOTCH: f64 = 64.0;

fn main() -> Result<(), Box<dyn Error>> {
    let system_dark = std::env::var("LUNA_SYSTEM_DARK")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);
    let cfg = AppConfig::new("Luna Shell")
        .app_id("eu.paganlinux.Luna.Shell")
        .layer(LayerConfig::top_chrome(
            "luna-shell",
            THICKNESS as u32,
            CHROME_H,
        ))
        .transparent(true)
        .system_dark(system_dark);

    let mut state = PanelState::sample();
    let desktop = luna::apps::scan();
    let apps = if desktop.is_empty() {
        App::sample()
    } else {
        App::from_desktop(&desktop)
    };
    let mut launcher = Launcher::new();

    let mut animating_popup = ActivePopup::None;
    let mut anim_progress: f64 = 0.0;
    let mut target_progress: f64 = 0.0;
    // Menu kontekstowe przycisku taskbara (PPM): zadanie, etykiety i akcje.
    let mut task_menu: Option<TaskMenuState> = None;
    let mut task_menu_hover: Option<usize> = None;
    let mut last_menu_sig: (usize, usize) = (0, 0);
    // Diagnostyka (`LUNA_STATS=1`): zmiany otwartego popupu (launcher, sieć, dźwięk…).
    let mut logged_popup = ActivePopup::None;

    // Menu zasilania
    let power_items = luna::power::items();

    // Data i kalendarz
    let mut cal_year = 2026i64;
    let mut cal_month = 10u32;
    let mut cal_selected: Option<u32> = None;

    // Dźwięk
    let mut volume_pct: u8 = 80;
    let mut volume_muted = false;

    // Wi-Fi (lista sieci pobierana w tle — `nmcli` potrafi trwać sekundy).
    let wifi_scan = luna::system::WifiScan::spawn();

    let mut osd = Osd::new(IconName::Volume, "Głośność", 0.0);
    let mut notes = Notifications::new();

    // Daemon powiadomień D‑Bus — rejestrowany **w tle**, żeby pierwsza klatka panelu nie
    // czekała na magistralę (zbus potrafi blokować, gdy bus dopiero wstaje po zalogowaniu).
    let bus_slot: BusSlot = std::sync::Arc::new(std::sync::Mutex::new(None));
    let bus_ready = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut bus_started = false;

    let mut brightness_pct: u8 = 0;
    let mut brightness_state: Option<u8> = None;
    let mut battery_state: Option<luna::system::BatteryStatus> = None;
    let mut bluetooth_state: Option<(bool, usize)> = None;
    let mut audio_sink: Option<String> = None;
    // Tryb „nie przeszkadzać” — trzymany w pliku, żeby przetrwał przelogowanie.
    let mut dnd = luna::system::dnd();
    // Night light (gamma LUT) i oszczędzanie energii.
    let mut night_light = luna::system::night_light_percent();
    let mut night_available = false;
    let mut night_checked: Option<Instant> = None;
    let mut night_sent = false;
    let mut power_saver = luna::system::power_saver();
    let mut power_sent = false;
    // Odtwarzacz MPRIS (tytuł/wykonawca w Quick Panelu).
    let media = luna::media::Media::spawn();
    // Lista okien: tworzymy ją w pierwszej klatce, gdy mamy już `waker` — zmiana
    // listy okien ma natychmiast odświeżyć panel (inaczej przycisk programu pojawiał
    // się dopiero przy zmianie zegara).
    let mut toplevels: Option<luna::toplevels::Toplevels> = None;
    let workspaces = luna::workspaces::Workspaces::spawn();
    let system = luna::system::spawn();
    let volume = luna::volume::VolumeWatcher::spawn();
    // Systemowy motyw (portal) → plik dla wszystkich procesów Luny (tryb Auto).
    luna::system::watch_system_theme();

    let mut last_sig: (usize, u32, i64, usize, usize, usize, String, u8, bool, u64) =
        (0, 0, 0, 0, 0, usize::MAX, String::new(), 0, false, u64::MAX);
    let mut vol_seq: Option<u64> = None;
    let mut last_ws: Option<usize> = None;
    let mut last_media: Option<luna::media::MediaInfo> = None;

    // Diagnostyka wydajności (`LUNA_STATS=1`): ile klatek na sekundę robi powłoka i ile
    // czasu spędza w swoim kodzie (budowa sceny + trafienia). Pozwala odróżnić koszt
    // powłoki od kosztu kompozytora i page-flipu.
    let stats = std::env::var("LUNA_STATS").is_ok();
    // Przez pierwsze sekundy po starcie budzimy powłokę częściej — kompozytor DRM
    // może jeszcze dopisywać flagę gamma, a zapisany night light trzeba zastosować.
    let startup = Instant::now();
    if stats {
        eprintln!(
            "luna-shell: profil={} (debug_assertions={}), wysokość chrome={} px",
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            cfg!(debug_assertions),
            CHROME_H
        );
    }
    let mut stat_frames = 0u64;
    let mut stat_ms = 0.0f64;
    let mut stat_report = Instant::now();

    run(cfg, move |frame: &mut Frame| {
        if toplevels.is_none() {
            toplevels = luna::toplevels::Toplevels::spawn(Some(frame.waker()));
        }
        let dt = frame.delta_time();
        let t_frame = Instant::now();

        // Podświetlenie pozycji pod kursorem w launcherze/popupie wymaga reakcji na
        // ruch myszy. Włączamy to tylko, gdy coś jest otwarte — w spoczynku ruch
        // kursora nie wywołuje wtedy żadnych commitów bufora (patrz `libluna-platform`).
        frame.set_motion_redraw(animating_popup != ActivePopup::None && target_progress > 0.0);

        if !bus_started {
            bus_started = true;
            let slot = std::sync::Arc::clone(&bus_slot);
            let ready = std::sync::Arc::clone(&bus_ready);
            let waker = frame.waker();
            std::thread::spawn(move || {
                let bus = NotificationsBus::spawn(waker);
                if let Ok(mut g) = slot.lock() {
                    *g = bus;
                }
                ready.store(true, std::sync::atomic::Ordering::Relaxed);
            });
        }
        if bus_ready.load(std::sync::atomic::Ordering::Relaxed) {
            with_notifications(&bus_slot, |b| {
                for inc in b.drain() {
                    if dnd {
                        // Tryb „nie przeszkadzać”: nie pokazujemy toasta, ale zamykamy
                        // powiadomienie po stronie nadawcy (inaczej czekałby na odpowiedź).
                        b.signal_closed(inc.id, 2);
                        if stats {
                            eprintln!("luna-shell: powiadomienie pominięte (DND)");
                        }
                    } else {
                        if stats {
                            eprintln!("luna-shell: powiadomienie: {}", inc.summary);
                        }
                        notes.push(notification_from(inc));
                    }
                }
            });
        }

        let size = frame.size;
        let theme = &frame.theme;
        let text = frame.text;
        let bar = Rect::from_xywh(0.0, 0.0, size.width, THICKNESS);

        // Zegar i data
        let (clock, date, today_year, today_month, today_day) = now_clock_full();
        state.clock = clock;
        state.date = date;

        state.title.clear();
        if let Some(tl) = &toplevels {
            let tasks = tl.tasks();
            if stats && tasks.len() != state.tasks.len() {
                eprintln!(
                    "luna-shell: taskbar: {} okien: {}",
                    tasks.len(),
                    tasks
                        .iter()
                        .map(|t| t.title.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ")
                );
            }
            state.tasks = tasks;
        } else {
            state.tasks.clear();
        }

        // Stan systemu
        if let Ok(sys) = system.lock() {
            state.user = sys.user.clone();
            state.toggles = build_toggles(&sys);
            brightness_pct = sys.brightness.unwrap_or(0);
            brightness_state = sys.brightness;
            battery_state = sys.battery;
            bluetooth_state = sys.bluetooth;
            audio_sink = sys.audio_sink.clone();
            if let Some((v, m)) = sys.volume {
                volume_pct = v;
                volume_muted = m;
            }
            if let Some(profile) = &sys.power_profile {
                power_saver = profile == "power-saver";
            }
        }
        // Jednorazowe zastosowanie zapisanego trybu oszczędzania. Preferujemy
        // `power-profiles-daemon`; bez niego ograniczamy tempo prezentacji kompozytora.
        if !power_sent {
            power_sent = true;
            if luna::system::ppd_available() {
                if power_saver {
                    luna::system::set_power_profile("power-saver");
                }
            } else if power_saver {
                control_send("power-saver 1");
            }
        }
        // Dostępność night lightu ogłasza kompozytor plikiem `luna-gamma`.
        if night_checked.map_or(true, |t| t.elapsed() >= std::time::Duration::from_secs(1)) {
            night_checked = Some(Instant::now());
            night_available = std::env::var_os("XDG_RUNTIME_DIR")
                .map(|rt| std::path::Path::new(&rt).join("luna-gamma"))
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|s| s.trim() == "1")
                .unwrap_or(false);
            if night_available && !night_sent {
                night_sent = true;
                if stats {
                    eprintln!("luna-shell: night light → {night_light}%");
                }
                control_send(&format!("night-light {night_light}"));
            }
        }
        // Lista sieci z wątku skanującego jest czytana bezpośrednio przy rysowaniu
        // i obsłudze kliknięć (świeża po `refresh()` przy otwarciu popupu).

        // Przestrzenie robocze
        match &workspaces {
            Some(ws) => {
                let list = ws.list();
                state.workspaces = list.len();
                state.current_ws = list.iter().position(|w| w.active).unwrap_or(0);
            }
            None => {
                state.workspaces = 0;
                state.current_ws = 0;
            }
        }

        // OSD głośności
        let (vseq, vvol) = volume.current();
        if let (Some(prev), Some((pct, muted))) = (vol_seq, vvol) {
            if vseq != prev {
                volume_pct = pct;
                volume_muted = muted;
                osd.icon = IconName::Volume;
                osd.show(luna::volume::label(muted), pct as f64 / 100.0);
            }
        }
        vol_seq = Some(vseq);

        // OSD przestrzeni roboczych
        if state.workspaces > 0 {
            if let Some(prev) = last_ws {
                if prev != state.current_ws {
                    osd.icon = IconName::Grid;
                    let frac = (state.current_ws + 1) as f64 / state.workspaces as f64;
                    osd.show(format!("Przestrzeń {}", state.current_ws + 1), frac);
                }
            }
            last_ws = Some(state.current_ws);
        }

        // Otwieranie/zamykanie **bez animacji wysuwania**: przy software'owym
        // kompozytorze „slide” wyglądał jak szarpanie (popup przesuwał się o 28 px
        // bez zanikania, po czym nagle znikał). Stan jest teraz natychmiastowy;
        // `anim_progress` zostaje jako stan 0/1 dla rysowania poniżej.
        let _ = anim_progress;
        anim_progress = target_progress;
        if anim_progress == 0.0 {
            animating_popup = ActivePopup::None;
        }

        let lw = LAUNCHER_W.min((size.width - 80.0).max(320.0));
        let larea = Rect::from_xywh((size.width - lw) / 2.0, THICKNESS + 24.0, lw, LAUNCHER_H);

        // Prostokąty przycisków paska
        let burger = panel::burger_rect(bar, &frame.theme);
        let clock_r = panel::clock_rect(bar, &frame.theme, text, &state);
        let power_r = panel::power_rect(bar, &frame.theme);
        let user_r = panel::user_rect(bar, &frame.theme, text, &state);

        // ── Obsługa zdarzeń wejścia ──
        for ev in frame.events {
            match ev {
                // ESC zamyka dowolne otwarte menu/launcher
                Event::Key(ke) if ke.state == ButtonState::Pressed && ke.keysym == 0xff1b => {
                    if animating_popup == ActivePopup::Launcher && !launcher.search.is_empty() {
                        launcher.search.clear();
                        launcher.scroll = 0.0;
                        launcher.selected = Some(0);
                    } else if animating_popup != ActivePopup::None {
                        target_progress = 0.0;
                    }
                }

                // Nawigacja klawiaturą i wyszukiwanie w launcherze
                Event::Key(ke)
                    if ke.state == ButtonState::Pressed
                        && animating_popup == ActivePopup::Launcher
                        && target_progress > 0.0 =>
                {
                    match ke.keysym {
                        0xff08 => {
                            // BackSpace — usuwa znak z pola wyszukiwania
                            launcher.search.pop();
                            launcher.scroll = 0.0;
                            launcher.selected = Some(0);
                        }
                        0xff52 | 0xff54 => {
                            let dy = if ke.keysym == 0xff52 { -1 } else { 1 };
                            let count = launcher.filter_apps(&apps).len();
                            launcher.navigate(count, 0, dy);
                            panel::ensure_selection_visible_count(
                                count,
                                &mut launcher,
                                larea,
                                &frame.theme,
                                text,
                            );
                        }
                        0xff51 | 0xff53 => {
                            let dx = if ke.keysym == 0xff51 { -1 } else { 1 };
                            let count = launcher.filter_apps(&apps).len();
                            launcher.navigate(count, dx, 0);
                            panel::ensure_selection_visible_count(
                                count,
                                &mut launcher,
                                larea,
                                &frame.theme,
                                text,
                            );
                        }
                        0xff0d | 0xff8d => {
                            let filtered = launcher.filter_apps(&apps);
                            if let Some(i) = launcher.selected_index(filtered.len()) {
                                if let Some(app) = filtered.get(i) {
                                    launch_app(app);
                                }
                                target_progress = 0.0;
                            }
                        }
                        _ => {
                            // Najpierw **tekst z xkb** (obsługuje Shift, AltGr, martwe
                            // klawisze i kompozycję): „ ”, —, €, ą… Wcześniejsze mapowanie
                            // keysym→znak znało tylko ASCII/Latin-1/Extended-A, więc np.
                            // cudzysłowy drukarskie (keysym 0x0ad2) były po cichu gubione.
                            if ke.modifiers.logo || ke.modifiers.ctrl {
                                continue;
                            }
                            let mut typed = false;
                            if let Some(text) = &ke.text {
                                for c in text.chars().filter(|c| !c.is_control()) {
                                    launcher.search.push(c);
                                    typed = true;
                                }
                            } else if let Some(c) = keysym_to_char(ke.keysym) {
                                launcher.search.push(c);
                                typed = true;
                            }
                            if typed {
                                launcher.scroll = 0.0;
                                launcher.selected = Some(0);
                                if stats {
                                    eprintln!("luna-shell: szukaj: {:?}", launcher.search);
                                }
                            }
                        }
                    }
                }

                // 1) Przewijanie listy lub kart w launcherze.
                // `se.dy` jest w jednostkach powierzchni (jeden ząbek kółka = `SCROLL_UNIT`).
                // Lista/siatka przewija się płynnie w pikselach; karuzela skacze o kartę.
                Event::Scroll(se) => {
                    let notches = se.dy / SCROLL_UNIT;
                    let vertical = se.dy.abs() >= se.dx.abs();
                    let dir = if vertical { se.dy } else { se.dx };
                    if animating_popup == ActivePopup::Launcher && target_progress > 0.0 {
                        if larea.contains(se.position) {
                            let filtered_len = launcher.filter_apps(&apps).len();
                            if launcher.view == panel::LauncherView::Carousel {
                                let steps = (notches.round() as i64).clamp(-3, 3);
                                if steps != 0 {
                                    // W dół (dodatnie) → następna karta.
                                    launcher.navigate(filtered_len, steps, 0);
                                }
                            } else {
                                let max = panel::launcher_scroll_max_count(
                                    filtered_len,
                                    &launcher,
                                    larea,
                                    &frame.theme,
                                    text,
                                );
                                // Kółko w górę (dy < 0) → przewijamy listę w górę.
                                launcher.scroll_by_notches(notches, LAUNCHER_SCROLL_PER_NOTCH, max);
                            }
                        }
                    } else if animating_popup == ActivePopup::Calendar && target_progress > 0.0 {
                        // 2) Kółko nad kalendarzem zmienia miesiąc (w górę = następny).
                        let crect = panel::calendar_popup_rect(bar, &frame.theme);
                        if crect.contains(se.position) && dir != 0.0 {
                            // W dół (dodatnie) → następny miesiąc.
                            if dir > 0.0 {
                                if cal_month == 12 {
                                    cal_month = 1;
                                    cal_year += 1;
                                } else {
                                    cal_month += 1;
                                }
                            } else {
                                if cal_month == 1 {
                                    cal_month = 12;
                                    cal_year -= 1;
                                } else {
                                    cal_month -= 1;
                                }
                            }
                        }
                    } else {
                        // 3) Kółko nad pigułką głośności na pasku reguluje dźwięk
                        let toggles = panel::toggle_rects(bar, &frame.theme, text, &state);
                        let vol_idx = state
                            .toggles
                            .iter()
                            .position(|t| t.action == ToggleAction::Volume);
                        let is_vol_pill = vol_idx
                            .and_then(|i| toggles.get(i))
                            .map(|r| r.contains(se.position))
                            .unwrap_or(false);
                        let is_vol_popup = animating_popup == ActivePopup::Volume
                            && panel::volume_popup_rect(bar, &frame.theme, text, &state)
                                .contains(se.position);
                        if (is_vol_pill || is_vol_popup) && dir != 0.0 {
                            // W górę (ujemne) → głośniej; w dół → ciszej.
                            let delta = if dir < 0.0 { 5 } else { -5 };
                            volume_pct = luna::system::adjust_volume(volume_pct, delta);
                        }

                        // 4) Kółko nad kropkami przestrzeni roboczych przełącza pulpit
                        let ws_rects = panel::workspace_rects(bar, &frame.theme, text, &state);
                        let is_ws_area = ws_rects.iter().any(|r| r.contains(se.position));
                        if is_ws_area && dir != 0.0 {
                            if let Some(ws) = &workspaces {
                                // W dół (dodatnie) → następna przestrzeń.
                                if dir > 0.0 && state.current_ws + 1 < state.workspaces {
                                    ws.activate(state.current_ws + 1);
                                } else if dir < 0.0 && state.current_ws > 0 {
                                    ws.activate(state.current_ws - 1);
                                }
                            }
                        }
                    }
                }

                // Ruch kursora nad launcherem — podświetlamy pozycję pod wskaźnikiem.
                // Test trafienia robimy tylko, gdy kursor jest nad launcherem (poza nim
                // nie ma czego podświetlać, a `launcher_app_at` alokuje listę aplikacji).
                Event::PointerMotion { position, .. }
                    if animating_popup == ActivePopup::Launcher
                        && target_progress > 0.0
                        && larea.contains(*position) =>
                {
                    if let Some(i) = panel::launcher_app_at(
                        &apps,
                        &launcher,
                        larea,
                        &frame.theme,
                        text,
                        *position,
                    ) {
                        if launcher.selected != Some(i) {
                            launcher.selected = Some(i);
                        }
                    }
                }

                // Podświetlenie pozycji menu kontekstowego taskbara.
                Event::PointerMotion { position, .. }
                    if animating_popup == ActivePopup::TaskMenu && target_progress > 0.0 =>
                {
                    if let Some(menu) = &task_menu {
                        task_menu_hover =
                            panel::task_menu_item_at(menu.rect, menu.labels.len(), *position);
                    }
                }

                // PPM na przycisku okna — menu kontekstowe (pokaż/zamknij/przenieś).
                Event::PointerButton(pe)
                    if pe.state == ButtonState::Pressed && pe.button == PointerButton::Right =>
                {
                    let idx = panel::task_at(bar, &frame.theme, text, &state.tasks, pe.position);
                    if let Some(i) = idx {
                        if let Some(task) = state.tasks.get(i) {
                            let first = if task.minimized {
                                panel::TaskMenuAction::Show
                            } else {
                                panel::TaskMenuAction::Minimize
                            };
                            let mut actions = vec![first, panel::TaskMenuAction::Close];
                            // Pozycje „przenieś” tylko gdy kompozytor zgłasza przestrzenie.
                            for ws in 0..state.workspaces {
                                actions.push(panel::TaskMenuAction::MoveTo(ws));
                            }
                            let labels: Vec<String> = actions
                                .iter()
                                .map(|a| match a {
                                    panel::TaskMenuAction::Show => libluna_i18n::tr("Show"),
                                    panel::TaskMenuAction::Minimize => libluna_i18n::tr("Hide"),
                                    panel::TaskMenuAction::Close => libluna_i18n::tr("Close"),
                                    panel::TaskMenuAction::MoveTo(ws) => libluna_i18n::tr_args(
                                        "Move to workspace {n}",
                                        &[("n", &(ws + 1).to_string())],
                                    ),
                                })
                                .collect();
                            if stats {
                                eprintln!(
                                    "luna-shell: task menu {:?} → {:?} {:?}",
                                    task.title, actions, labels
                                );
                            }
                            let rects = panel::task_rects(bar, &frame.theme, text, &state.tasks);
                            let anchor = rects.get(i).copied().unwrap_or_else(|| {
                                Rect::from_xywh(pe.position.x - 40.0, bar.y(), 80.0, bar.height())
                            });
                            let rect =
                                panel::task_menu_rect(anchor, bar, actions.len(), &frame.theme);
                            task_menu = Some(TaskMenuState {
                                task: i,
                                title: task.title.clone(),
                                rect,
                                labels,
                                actions,
                            });
                            task_menu_hover = None;
                            animating_popup = ActivePopup::TaskMenu;
                            target_progress = 1.0;
                        }
                    } else {
                        task_menu = None;
                        task_menu_hover = None;
                        target_progress = 0.0;
                    }
                }

                // Dowolny inny przycisk (prawy/środkowy) zamyka otwarty popup/launcher.
                Event::PointerButton(pe)
                    if pe.state == ButtonState::Pressed
                        && pe.button != PointerButton::Left
                        && animating_popup != ActivePopup::None
                        && target_progress > 0.0 =>
                {
                    target_progress = 0.0;
                }

                // Lewy przycisk myszy
                Event::PointerButton(pe)
                    if pe.state == ButtonState::Pressed && pe.button == PointerButton::Left =>
                {
                    let pos = pe.position;
                    let in_bar = pos.y >= bar.y() && pos.y <= bar.bottom();
                    let is_burger = in_bar && pos.x >= bar.x() && pos.x <= burger.right() + 8.0;
                    let is_clock =
                        in_bar && pos.x >= clock_r.x() - 4.0 && pos.x <= clock_r.right() + 4.0;
                    let is_power = in_bar && pos.x >= power_r.x() - 4.0 && pos.x <= bar.right();
                    let is_user = in_bar && pos.x >= user_r.x() - 4.0 && pos.x < power_r.x() - 4.0;
                    let is_quick =
                        in_bar && panel::quick_button_rect(bar, theme, text, &state).contains(pos);

                    // Gdy któreś menu jest otwarte (lub animowane)
                    if animating_popup != ActivePopup::None && target_progress > 0.0 {
                        let mut handled = false;

                        match animating_popup {
                            ActivePopup::TaskMenu => {
                                if let Some(menu) = task_menu.take() {
                                    if let Some(k) =
                                        panel::task_menu_item_at(menu.rect, menu.labels.len(), pos)
                                    {
                                        match menu.actions.get(k) {
                                            Some(panel::TaskMenuAction::Show) => {
                                                if let Some(tl) = &toplevels {
                                                    tl.show(menu.task);
                                                }
                                            }
                                            Some(panel::TaskMenuAction::Minimize) => {
                                                if let Some(tl) = &toplevels {
                                                    tl.minimize(menu.task);
                                                }
                                            }
                                            Some(panel::TaskMenuAction::Close) => {
                                                if let Some(tl) = &toplevels {
                                                    tl.close(menu.task);
                                                }
                                            }
                                            Some(panel::TaskMenuAction::MoveTo(ws)) => {
                                                control_send(&format!(
                                                    "move-to-workspace {} {}",
                                                    ws,
                                                    one_line(&menu.title)
                                                ));
                                            }
                                            None => {}
                                        }
                                    }
                                }
                                task_menu_hover = None;
                                target_progress = 0.0;
                                handled = true;
                            }
                            ActivePopup::Quick => {
                                let qrect = panel::quick_panel_rect(bar, &frame.theme);
                                if let Some(hit) = panel::quick_hit(qrect, &frame.theme, pos) {
                                    if stats {
                                        eprintln!("luna-shell: quick {:?}", hit);
                                    }
                                    match hit {
                                        panel::QuickHit::Power => {
                                            animating_popup = ActivePopup::Session;
                                            target_progress = 1.0;
                                        }
                                        panel::QuickHit::Theme => {
                                            let mut s = libluna_core::ThemeSettings::load();
                                            let i = ThemePreference::ALL
                                                .iter()
                                                .position(|p| *p == s.preference())
                                                .unwrap_or(2);
                                            s.set_preference(
                                                ThemePreference::ALL
                                                    [(i + 1) % ThemePreference::ALL.len()],
                                            );
                                            let _ = s.save();
                                        }
                                        panel::QuickHit::Wifi => {
                                            // Kafelek pokazuje tylko bieżącą sieć — kliknięcie
                                            // otwiera listę sieci (radio wyłączamy z popupu).
                                            wifi_scan.refresh();
                                            animating_popup = ActivePopup::Wifi;
                                            target_progress = 1.0;
                                        }
                                        panel::QuickHit::Bluetooth => {
                                            if let Some((on, _)) = bluetooth_state {
                                                luna::system::set_bluetooth(!on);
                                            }
                                        }
                                        panel::QuickHit::Airplane => {
                                            let wifi_on = state
                                                .toggles
                                                .iter()
                                                .find(|t| t.action == ToggleAction::Wifi)
                                                .map(|t| t.active)
                                                .unwrap_or(true);
                                            let bt_on = matches!(bluetooth_state, Some((true, _)));
                                            let airplane = !wifi_on && !bt_on;
                                            luna::system::set_airplane(!airplane);
                                        }
                                        panel::QuickHit::Dnd => {
                                            dnd = !dnd;
                                            luna::system::set_dnd(dnd);
                                            if dnd {
                                                notes.clear();
                                            }
                                        }
                                        panel::QuickHit::NightLight => {
                                            if night_available {
                                                night_light = if night_light > 0 { 0 } else { 60 };
                                                luna::system::set_night_light_percent(night_light);
                                                control_send(&format!("night-light {night_light}"));
                                            }
                                        }
                                        panel::QuickHit::PowerSaver => {
                                            power_saver = !power_saver;
                                            luna::system::set_power_saver(power_saver);
                                            if luna::system::ppd_available() {
                                                luna::system::set_power_profile(if power_saver {
                                                    "power-saver"
                                                } else {
                                                    "balanced"
                                                });
                                            } else {
                                                control_send(&format!(
                                                    "power-saver {}",
                                                    u8::from(power_saver)
                                                ));
                                            }
                                        }
                                        panel::QuickHit::Mute => {
                                            luna::system::toggle_mute();
                                            volume_muted = !volume_muted;
                                        }
                                        panel::QuickHit::Volume(v) => {
                                            luna::system::set_volume(v);
                                            volume_pct = v;
                                        }
                                        panel::QuickHit::Brightness(v) => {
                                            // Suwak jest aktywny tylko przy realnym podświetleniu.
                                            if brightness_state.is_some() {
                                                luna::system::set_brightness(v);
                                                brightness_pct = v;
                                                brightness_state = Some(v);
                                            }
                                        }
                                        panel::QuickHit::MediaPrevious => {
                                            if let Some(m) = media.current() {
                                                luna::media::send(&m.player, "Previous");
                                            }
                                        }
                                        panel::QuickHit::MediaPlayPause => {
                                            if let Some(m) = media.current() {
                                                luna::media::send(&m.player, "PlayPause");
                                            }
                                        }
                                        panel::QuickHit::MediaNext => {
                                            if let Some(m) = media.current() {
                                                luna::media::send(&m.player, "Next");
                                            }
                                        }
                                        panel::QuickHit::Lock => {
                                            luna::power::perform(luna::power::PowerAction::Lock);
                                            target_progress = 0.0;
                                        }
                                        panel::QuickHit::Logout => {
                                            luna::power::perform(luna::power::PowerAction::Logout);
                                            target_progress = 0.0;
                                        }
                                        panel::QuickHit::Reboot => {
                                            luna::power::perform(luna::power::PowerAction::Reboot);
                                            target_progress = 0.0;
                                        }
                                    }
                                    handled = true;
                                }
                            }
                            ActivePopup::Launcher => {
                                if let Some(v) =
                                    panel::launcher_view_at(larea, &frame.theme, text, pos)
                                {
                                    launcher.view = v;
                                    launcher.scroll = 0.0;
                                    handled = true;
                                } else if let Some(i) = panel::launcher_app_at(
                                    &apps,
                                    &launcher,
                                    larea,
                                    &frame.theme,
                                    text,
                                    pos,
                                ) {
                                    launch(&apps, i);
                                    target_progress = 0.0;
                                    handled = true;
                                } else if larea.contains(pos) {
                                    // Kliknięcie we wnętrze launchera (puste tło) nie zamyka go
                                    handled = true;
                                }
                            }
                            ActivePopup::Session => {
                                let srect = panel::session_menu_rect(bar, &frame.theme);
                                if srect.contains(pos) {
                                    if let Some(act) =
                                        panel::session_item_at(bar, &frame.theme, pos)
                                    {
                                        luna::power::perform(act);
                                    }
                                    target_progress = 0.0;
                                    handled = true;
                                }
                            }
                            ActivePopup::Wifi => {
                                let wrect = panel::wifi_popup_rect(bar, &frame.theme, text, &state);
                                if wrect.contains(pos) {
                                    let conn = state
                                        .toggles
                                        .iter()
                                        .find(|t| t.action == ToggleAction::Wifi && t.active)
                                        .map(|t| t.label.as_str());
                                    if let Some(act) = panel::wifi_popup_hit(
                                        wrect,
                                        &frame.theme,
                                        pos,
                                        conn,
                                        &wifi_scan.current(),
                                    ) {
                                        match act {
                                            WifiAction::TogglePower => {
                                                let on = state
                                                    .toggles
                                                    .iter()
                                                    .find(|t| t.action == ToggleAction::Wifi)
                                                    .map(|t| t.active)
                                                    .unwrap_or(true);
                                                luna::system::set_wifi(!on);
                                            }
                                            WifiAction::SelectNetwork(ssid) => {
                                                let _ = std::process::Command::new("nmcli")
                                                    .args(&["dev", "wifi", "connect", &ssid])
                                                    .spawn();
                                            }
                                        }
                                    }
                                    handled = true;
                                }
                            }
                            ActivePopup::Volume => {
                                let vrect =
                                    panel::volume_popup_rect(bar, &frame.theme, text, &state);
                                if vrect.contains(pos) {
                                    if let Some(act) =
                                        panel::volume_popup_hit(vrect, &frame.theme, pos)
                                    {
                                        match act {
                                            VolumeAction::SetLevel(pct) => {
                                                luna::system::set_volume(pct);
                                                volume_pct = pct;
                                            }
                                            VolumeAction::StepDown => {
                                                volume_pct =
                                                    luna::system::adjust_volume(volume_pct, -5);
                                            }
                                            VolumeAction::StepUp => {
                                                volume_pct =
                                                    luna::system::adjust_volume(volume_pct, 5);
                                            }
                                            VolumeAction::ToggleMute => {
                                                luna::system::toggle_mute();
                                                volume_muted = !volume_muted;
                                            }
                                        }
                                    }
                                    handled = true;
                                }
                            }
                            ActivePopup::Calendar => {
                                let crect = panel::calendar_popup_rect(bar, &frame.theme);
                                if crect.contains(pos) {
                                    if let Some(act) = panel::calendar_popup_hit(
                                        crect,
                                        &frame.theme,
                                        pos,
                                        cal_year,
                                        cal_month,
                                    ) {
                                        match act {
                                            CalendarAction::PrevMonth => {
                                                if cal_month == 1 {
                                                    cal_month = 12;
                                                    cal_year -= 1;
                                                } else {
                                                    cal_month -= 1;
                                                }
                                            }
                                            CalendarAction::NextMonth => {
                                                if cal_month == 12 {
                                                    cal_month = 1;
                                                    cal_year += 1;
                                                } else {
                                                    cal_month += 1;
                                                }
                                            }
                                            CalendarAction::SelectDay(d) => {
                                                cal_selected = Some(d);
                                            }
                                        }
                                    }
                                    handled = true;
                                }
                            }
                            ActivePopup::None => {}
                        }

                        if !handled {
                            // Sprawdzamy, czy kliknięto przycisk na pasku (przełączenie lub zamknięcie)
                            if is_burger {
                                if animating_popup == ActivePopup::Launcher {
                                    target_progress = 0.0;
                                } else {
                                    animating_popup = ActivePopup::Launcher;
                                    target_progress = 1.0;
                                }
                            } else if is_clock {
                                if animating_popup == ActivePopup::Calendar {
                                    target_progress = 0.0;
                                } else {
                                    cal_year = today_year;
                                    cal_month = today_month;
                                    cal_selected = Some(today_day);
                                    animating_popup = ActivePopup::Calendar;
                                    target_progress = 1.0;
                                }
                            } else if is_power || is_user {
                                if animating_popup == ActivePopup::Session {
                                    target_progress = 0.0;
                                } else {
                                    animating_popup = ActivePopup::Session;
                                    target_progress = 1.0;
                                }
                            } else if is_quick {
                                if animating_popup == ActivePopup::Quick {
                                    target_progress = 0.0;
                                } else {
                                    animating_popup = ActivePopup::Quick;
                                    target_progress = 1.0;
                                }
                            } else if let Some(i) =
                                panel::toggle_at(bar, &frame.theme, text, &state, pos)
                            {
                                match state.toggles[i].action {
                                    ToggleAction::Wifi => {
                                        if animating_popup == ActivePopup::Wifi {
                                            target_progress = 0.0;
                                        } else {
                                            wifi_scan.refresh();
                                            animating_popup = ActivePopup::Wifi;
                                            target_progress = 1.0;
                                        }
                                    }
                                    ToggleAction::Volume => {
                                        if animating_popup == ActivePopup::Volume {
                                            target_progress = 0.0;
                                        } else {
                                            animating_popup = ActivePopup::Volume;
                                            target_progress = 1.0;
                                        }
                                    }
                                    ToggleAction::Brightness => {
                                        let next =
                                            luna::system::next_brightness_level(brightness_pct);
                                        luna::system::set_brightness(next);
                                        brightness_pct = next;
                                        brightness_state = Some(next);
                                        osd.icon = IconName::Brightness;
                                        osd.show("Jasność", next as f64 / 100.0);
                                        target_progress = 0.0;
                                    }
                                    ToggleAction::None => {
                                        target_progress = 0.0;
                                    }
                                }
                            } else if let Some(i) =
                                panel::task_at(bar, &frame.theme, text, &state.tasks, pos)
                            {
                                if let Some(tl) = &toplevels {
                                    tl.activate(i);
                                }
                                target_progress = 0.0;
                            } else if let Some(i) =
                                panel::workspace_at(bar, &frame.theme, text, &state, pos)
                            {
                                if let Some(ws) = &workspaces {
                                    ws.activate(i);
                                }
                                target_progress = 0.0;
                            } else {
                                // Kliknięcie gdziekolwiek poza otwartym popupem/launcherem — ZAMYKA GO!
                                target_progress = 0.0;
                            }
                        }
                    } else {
                        // Żaden popup nie jest otwarty — standardowa obsługa paska
                        if is_burger {
                            let max = panel::launcher_scroll_max(
                                &apps,
                                &launcher,
                                larea,
                                &frame.theme,
                                text,
                            );
                            launcher.scroll = launcher.scroll.clamp(0.0, max);
                            animating_popup = ActivePopup::Launcher;
                            target_progress = 1.0;
                        } else if is_clock {
                            cal_year = today_year;
                            cal_month = today_month;
                            cal_selected = Some(today_day);
                            animating_popup = ActivePopup::Calendar;
                            target_progress = 1.0;
                        } else if is_power || is_user {
                            animating_popup = ActivePopup::Session;
                            target_progress = 1.0;
                        } else if is_quick {
                            animating_popup = ActivePopup::Quick;
                            target_progress = 1.0;
                        } else if let Some(i) =
                            panel::toggle_at(bar, &frame.theme, text, &state, pos)
                        {
                            match state.toggles[i].action {
                                ToggleAction::Wifi => {
                                    wifi_scan.refresh();
                                    animating_popup = ActivePopup::Wifi;
                                    target_progress = 1.0;
                                }
                                ToggleAction::Volume => {
                                    animating_popup = ActivePopup::Volume;
                                    target_progress = 1.0;
                                }
                                ToggleAction::Brightness => {
                                    let next = luna::system::next_brightness_level(brightness_pct);
                                    luna::system::set_brightness(next);
                                    brightness_pct = next;
                                    brightness_state = Some(next);
                                    osd.icon = IconName::Brightness;
                                    osd.show("Jasność", next as f64 / 100.0);
                                }
                                ToggleAction::None => {}
                            }
                        } else if let Some(i) =
                            panel::task_at(bar, &frame.theme, text, &state.tasks, pos)
                        {
                            if let Some(tl) = &toplevels {
                                tl.activate(i);
                            }
                        } else if let Some(i) =
                            panel::workspace_at(bar, &frame.theme, text, &state, pos)
                        {
                            if let Some(ws) = &workspaces {
                                ws.activate(i);
                            }
                        } else {
                            // Obsługa toastów powiadomień
                            let area = Rect::from_xywh(
                                0.0,
                                THICKNESS + 8.0,
                                size.width,
                                size.height - THICKNESS,
                            );
                            if let Some((item, ak)) = notes.action_at(area, pos, &frame.theme, text)
                            {
                                if let Some(n) = notes.get(item) {
                                    if let Some((key, _)) = n.actions.get(ak) {
                                        let key = key.clone();
                                        let id = n.id;
                                        with_notifications(&bus_slot, |b| {
                                            b.signal_action(id, &key);
                                            b.signal_closed(id, 2);
                                        });
                                    } else {
                                        let id = n.id;
                                        with_notifications(&bus_slot, |b| b.signal_closed(id, 2));
                                    }
                                }
                                notes.remove(item);
                            } else if let Some(i) = notes.hit(area, pos) {
                                if let Some(n) = notes.get(i) {
                                    let id = n.id;
                                    with_notifications(&bus_slot, |b| b.signal_closed(id, 2));
                                }
                                notes.remove(i);
                            }
                        }
                    }
                }

                // Środkowy klik na zadaniu w pasku — zamyka okno
                Event::PointerButton(pe)
                    if pe.state == ButtonState::Pressed && pe.button == PointerButton::Middle =>
                {
                    if let Some(i) =
                        panel::task_at(bar, &frame.theme, text, &state.tasks, pe.position)
                    {
                        if let Some(tl) = &toplevels {
                            tl.close(i);
                        }
                    }
                }
                _ => {}
            }
        }

        // Otwarty launcher/popup przechwytuje **całą** powierzchnię panelu. Dzięki temu
        // kliknięcie poza obrysem (puste tło, inny monitor, dolna część ekranu) trafia
        // do powłoki i zamyka popup, zamiast aktywować okno pod spodem. Ustawiamy to
        // **po** obsłudze zdarzeń, żeby zmiana działała już w tej samej klatce.
        let popup_open = animating_popup != ActivePopup::None && target_progress > 0.0;
        if std::env::var("LUNA_STATS").is_ok() && animating_popup != logged_popup {
            eprintln!(
                "luna-shell: popup {:?} → {:?} (cel {:.0})",
                logged_popup, animating_popup, target_progress
            );
            logged_popup = animating_popup;
        }
        frame.set_input_region(if popup_open {
            InputRegion::Rects(vec![Rect::from_xywh(
                0.0,
                0.0,
                frame.size.width,
                frame.size.height,
            )])
        } else {
            // Powierzchnia powłoki jest wyższa niż pasek (mieści popupy), więc
            // region wejścia musi obejmować **tylko** pasek i ewentualne toasty.
            // Bez tego przezroczysty pas do 680 px pożerał kliknięcia okien pod nim
            // (nieklikalne menu Zed, przyciski tytułu Firefoksa itd.).
            let mut rects = vec![Rect::from_xywh(0.0, 0.0, frame.size.width, THICKNESS)];
            if notes.visible() {
                let area = Rect::from_xywh(
                    0.0,
                    THICKNESS + 8.0,
                    frame.size.width,
                    frame.size.height - THICKNESS,
                );
                rects.push(luna::notifications::Notifications::stack_rect(
                    area,
                    notes.len(),
                ));
            }
            InputRegion::Rects(rects)
        });

        osd.advance(dt);
        notes.advance(dt);

        // Sygnatura stanu — liczymy ją **przed** rysowaniem, żeby dało się pominąć całą
        // klatkę, gdy nic się nie zmieniło (np. sam ruch kursora nad tym samym kafelkiem).
        let mut th: u64 = state.tasks.len() as u64;
        for t in &state.tasks {
            th = th
                .wrapping_mul(1_099_511_628_211)
                .wrapping_add(u64::from(t.active) ^ t.title.len() as u64);
        }
        for tg in &state.toggles {
            let mut lh: u64 = 0;
            for b in tg.label.bytes() {
                lh = lh.wrapping_mul(31).wrapping_add(u64::from(b));
            }
            th = th
                .wrapping_mul(1_099_511_628_211)
                .wrapping_add(u64::from(tg.active) ^ lh);
        }
        th = th
            .wrapping_mul(1_099_511_628_211)
            .wrapping_add(state.user.len() as u64);
        th = th
            .wrapping_mul(1_099_511_628_211)
            .wrapping_add(state.workspaces as u64 ^ ((state.current_ws as u64) << 8));

        let sig = (
            animating_popup as usize,
            (anim_progress * 1000.0) as u32,
            launcher.scroll as i64,
            launcher.search.len(),
            launcher.view as usize,
            launcher.selected.unwrap_or(usize::MAX),
            state.clock.clone(),
            volume_pct,
            volume_muted,
            th,
        );
        let menu_sig = (
            task_menu.as_ref().map(|m| m.task + 1).unwrap_or(0),
            task_menu_hover.map(|i| i + 1).unwrap_or(0),
        );
        let changed = sig != last_sig || menu_sig != last_menu_sig;
        let animating =
            osd.visible() || notes.visible() || (anim_progress - target_progress).abs() > 0.002;
        let interactive = frame
            .events
            .iter()
            .any(|e| !matches!(e, Event::PointerMotion { .. }));
        let motion_only = !frame.events.is_empty()
            && frame
                .events
                .iter()
                .all(|e| matches!(e, Event::PointerMotion { .. } | Event::PointerLeave));

        // Szybka ścieżka: wyłącznie ruch kursora i nic się nie zmieniło → nie rysujemy
        // ani nie wysyłamy bufora. Bez tego ruch myszy nad otwartym launcherem zajmował
        // procesor na okrągło (odczuwalne jako „zamrożenie” i 5 FPS).
        if motion_only && !animating && !changed {
            last_sig = sig;
            last_menu_sig = menu_sig;
            return;
        }

        // ── Rysowanie sceny ──
        let preference = frame.preference();
        let mut ctx = PaintCtx::new(frame.scene, theme, text);

        // Uwaga: **nie rysujemy przygaszonego tła**. Powierzchnia powłoki ma wysokość
        // chrome (680 px), więc takie przygaszenie kończyło się twardą krawędzią
        // w połowie ekranu („cień na pół ekranu”). Pełnoekranowe przygaszenie wymaga
        // powierzchni na cały ekran — do zrobienia osobno.

        // Pasek panelu (zawsze na wierzchu tła)
        panel::paint_panel(&state, &mut ctx, bar);

        // Rysowanie wysuniętego popupu/launchera z płynnym ruchem (slide)
        if anim_progress > 0.001 {
            match animating_popup {
                ActivePopup::Launcher => {
                    let slide = (1.0 - anim_progress) * -28.0;
                    let cur_larea = Rect::from_xywh(
                        larea.x(),
                        larea.y() + slide,
                        larea.width(),
                        larea.height(),
                    );
                    panel::paint_launcher(&apps, &launcher, &mut ctx, cur_larea);
                }
                ActivePopup::Session => {
                    let slide = (1.0 - anim_progress) * -12.0;
                    let srect = panel::session_menu_rect(bar, &frame.theme);
                    let cur_srect = Rect::from_xywh(
                        srect.x(),
                        srect.y() + slide,
                        srect.width(),
                        srect.height(),
                    );
                    ctx.scene.fill_rounded_rect(
                        cur_srect,
                        theme.radius.medium,
                        theme.colors.card_bg,
                    );
                    ctx.scene.stroke_rounded_rect(
                        cur_srect,
                        theme.radius.medium,
                        1.0,
                        theme.colors.border,
                    );
                    let x = cur_srect.x() + 6.0;
                    let y = cur_srect.y() + 6.0;
                    let w = cur_srect.width() - 12.0;
                    for (i, item) in power_items.iter().enumerate() {
                        let row = Rect::from_xywh(
                            x,
                            y + i as f64 * panel::POWER_ITEM_H,
                            w,
                            panel::POWER_ITEM_H,
                        );
                        let icon_name = match item.action {
                            luna::power::PowerAction::Lock => IconName::Star,
                            luna::power::PowerAction::Logout => IconName::User,
                            luna::power::PowerAction::Reboot => IconName::Clock,
                            luna::power::PowerAction::PowerOff => IconName::Power,
                        };
                        let isz = 16.0;
                        let icon_rect =
                            Rect::from_xywh(row.x() + 8.0, row.center().y - isz / 2.0, isz, isz);
                        paint_icon(&mut ctx, icon_name, icon_rect, theme.colors.text);
                        ctx.label_left(
                            icon_rect.right() + 10.0,
                            row.center().y,
                            &item.label,
                            theme.typography.body.size,
                            libluna_core::style::weight::NORMAL,
                            theme.colors.text,
                        );
                    }
                }
                ActivePopup::Wifi => {
                    let slide = (1.0 - anim_progress) * -12.0;
                    let wrect = panel::wifi_popup_rect(bar, &frame.theme, text, &state);
                    let cur_wrect = Rect::from_xywh(
                        wrect.x(),
                        wrect.y() + slide,
                        wrect.width(),
                        wrect.height(),
                    );
                    let wifi_on = state
                        .toggles
                        .iter()
                        .find(|t| t.action == ToggleAction::Wifi)
                        .map(|t| t.active)
                        .unwrap_or(true);
                    let conn_ssid = state
                        .toggles
                        .iter()
                        .find(|t| t.action == ToggleAction::Wifi && t.active)
                        .map(|t| t.label.as_str());
                    panel::paint_wifi_popup(
                        &mut ctx,
                        cur_wrect,
                        wifi_on,
                        conn_ssid,
                        &wifi_scan.current(),
                    );
                }
                ActivePopup::Volume => {
                    let slide = (1.0 - anim_progress) * -12.0;
                    let vrect = panel::volume_popup_rect(bar, &frame.theme, text, &state);
                    let cur_vrect = Rect::from_xywh(
                        vrect.x(),
                        vrect.y() + slide,
                        vrect.width(),
                        vrect.height(),
                    );
                    panel::paint_volume_popup(&mut ctx, cur_vrect, volume_pct, volume_muted);
                }
                ActivePopup::Calendar => {
                    let slide = (1.0 - anim_progress) * -12.0;
                    let crect = panel::calendar_popup_rect(bar, &frame.theme);
                    let cur_crect = Rect::from_xywh(
                        crect.x(),
                        crect.y() + slide,
                        crect.width(),
                        crect.height(),
                    );
                    panel::paint_calendar_popup(
                        &mut ctx,
                        cur_crect,
                        &state.clock,
                        &state.date,
                        cal_year,
                        cal_month,
                        today_year,
                        today_month,
                        today_day,
                        cal_selected,
                    );
                }
                ActivePopup::Quick => {
                    let slide = (1.0 - anim_progress) * -10.0;
                    let qrect = panel::quick_panel_rect(bar, &frame.theme);
                    let cur = Rect::from_xywh(
                        qrect.x(),
                        qrect.y() + slide,
                        qrect.width(),
                        qrect.height(),
                    );
                    let wifi_on = state
                        .toggles
                        .iter()
                        .find(|t| t.action == ToggleAction::Wifi)
                        .map(|t| t.active)
                        .unwrap_or(true);
                    let ssid = state
                        .toggles
                        .iter()
                        .find(|t| t.action == ToggleAction::Wifi && t.active)
                        .map(|t| t.label.as_str());
                    let now_playing = media.current();
                    if stats && now_playing != last_media {
                        match &now_playing {
                            Some(m) => eprintln!("luna-shell: media: {} — {}", m.title, m.artist),
                            None => eprintln!("luna-shell: media: brak odtwarzacza"),
                        }
                        last_media = now_playing.clone();
                    }
                    let media_info = now_playing.as_ref().map(|m| panel::QuickMedia {
                        title: &m.title,
                        artist: &m.artist,
                        playing: m.playing,
                    });
                    let data = panel::QuickData {
                        user: &state.user,
                        preference,
                        dark: frame.theme.appearance == libluna_core::style::Appearance::Dark,
                        wifi_on,
                        ssid,
                        bluetooth: bluetooth_state,
                        airplane: !wifi_on && !matches!(bluetooth_state, Some((true, _))),
                        dnd,
                        night_light,
                        gamma_available: night_available,
                        power_saver,
                        power_fallback: !luna::system::ppd_available(),
                        volume: volume_pct,
                        muted: volume_muted,
                        brightness: brightness_state,
                        sink: audio_sink.as_deref(),
                        battery: battery_state,
                        media: media_info,
                    };
                    panel::paint_quick_panel(&mut ctx, cur, &data);
                }
                ActivePopup::TaskMenu => {
                    if let Some(menu) = &task_menu {
                        let slide = (1.0 - anim_progress) * -8.0;
                        let cur = Rect::from_xywh(
                            menu.rect.x(),
                            menu.rect.y() + slide,
                            menu.rect.width(),
                            menu.rect.height(),
                        );
                        panel::paint_task_menu(
                            &mut ctx,
                            cur,
                            &menu.labels,
                            &menu.actions,
                            task_menu_hover,
                        );
                    }
                }
                ActivePopup::None => {}
            }
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

        // Sygnatura była policzona przed rysowaniem (`sig`, `changed`, `animating`,
        // `interactive`) — tutaj tylko podejmujemy decyzję o kolejnej klatce.
        if interactive || animating || changed {
            frame.request_frame();
        }
        // Zegar i stan systemu odświeżają się także bez ruchu myszy.
        let keep_alive = if startup.elapsed() < std::time::Duration::from_secs(8) {
            std::time::Duration::from_millis(500)
        } else {
            std::time::Duration::from_secs(5)
        };
        frame.request_frame_after(keep_alive);
        last_sig = sig;
        last_menu_sig = menu_sig;

        if stats {
            stat_frames += 1;
            stat_ms += t_frame.elapsed().as_secs_f64() * 1000.0;
            let secs = stat_report.elapsed().as_secs_f64();
            if secs >= 2.0 {
                eprintln!(
                    "luna-shell: {:.1} klatek/s | kod powłoki {:.2} ms/klatke",
                    stat_frames as f64 / secs,
                    stat_ms / stat_frames.max(1) as f64
                );
                stat_frames = 0;
                stat_ms = 0.0;
                stat_report = Instant::now();
            }
        }
    })
}

/// Uchwyt do usługi powiadomień, wypełniany raz w wątku tła (patrz `main`).
type BusSlot = std::sync::Arc<std::sync::Mutex<Option<NotificationsBus>>>;

/// Menu kontekstowe przycisku okna w taskbarze.
struct TaskMenuState {
    task: usize,
    title: String,
    rect: Rect,
    labels: Vec<String>,
    actions: Vec<panel::TaskMenuAction>,
}

/// Zamienia tytuł okna na jednoliniowy (protokół sterowania jest linijkowy).
fn one_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
}

/// Wysyła polecenie do Selene (`$XDG_RUNTIME_DIR/luna-control`).
fn control_send(line: &str) {
    use std::io::Write;
    let Ok(rt) = std::env::var("XDG_RUNTIME_DIR") else {
        return;
    };
    let path = std::path::Path::new(&rt).join("luna-control");
    if let Ok(mut s) = std::os::unix::net::UnixStream::connect(path) {
        let _ = writeln!(s, "{line}");
    }
}

/// Woła `f` z usługą powiadomień, jeśli jest już zarejestrowana.
fn with_notifications(slot: &BusSlot, f: impl FnOnce(&NotificationsBus)) {
    if let Ok(g) = slot.lock() {
        if let Some(bus) = g.as_ref() {
            f(bus);
        }
    }
}

/// Zamienia powiadomienie z magistrali D-Bus na toast Luny.
fn notification_from(inc: Incoming) -> Notification {
    let icon = icon_for(&inc.app_name);
    let timeout = inc.timeout_secs();
    let mut n = Notification::new(inc.app_name, inc.summary, inc.body)
        .with_icon(icon)
        .with_id(inc.id)
        .with_actions(inc.actions);
    n.timeout = timeout;
    n
}

/// Dobiera ikonę pakietu do nazwy aplikacji wysyłającej powiadomienie.
fn icon_for(app: &str) -> IconName {
    let a = app.to_lowercase();
    if a.contains("file") || a.contains("plik") {
        IconName::Files
    } else if a.contains("term") {
        IconName::Terminal
    } else if a.contains("music") || a.contains("muzy") || a.contains("audio") {
        IconName::Music
    } else if a.contains("setting") || a.contains("ustaw") {
        IconName::Settings
    } else if a.contains("browser") || a.contains("przegl") {
        IconName::Search
    } else {
        IconName::Info
    }
}

/// Buduje przełączniki paska z realnego stanu systemu.
fn build_toggles(s: &luna::system::SystemStatus) -> Vec<Toggle> {
    let mut v = Vec::new();
    // Sieć i dźwięk na pasku to **same ikony** (bez nazwy sieci i bez „Dźwięk”) —
    // tak jak w nowoczesnych pulpitach. Pełna nazwa/SSID żyje w popupie.
    if let Some((on, ssid)) = &s.wifi {
        v.push(Toggle {
            label: ssid.clone().unwrap_or_else(|| "Wi-Fi".to_owned()),
            value: None,
            icon: IconName::Wifi,
            active: *on,
            action: ToggleAction::Wifi,
        });
    } else {
        v.push(Toggle {
            label: "Wi-Fi".to_owned(),
            value: None,
            icon: IconName::Wifi,
            active: false,
            action: ToggleAction::Wifi,
        });
    }
    if let Some((_pct, muted)) = s.volume {
        v.push(Toggle {
            label: "Dźwięk".to_owned(),
            value: None,
            icon: IconName::Volume,
            active: !muted,
            action: ToggleAction::Volume,
        });
    } else {
        v.push(Toggle {
            label: "Dźwięk".to_owned(),
            value: None,
            icon: IconName::Volume,
            active: true,
            action: ToggleAction::Volume,
        });
    }
    if let Some(b) = s.battery {
        v.push(Toggle {
            label: "Bateria".to_owned(),
            value: Some(format!("{}%", b.percent)),
            icon: IconName::Battery,
            active: b.charging,
            action: ToggleAction::None,
        });
    }
    if let Some(pct) = s.brightness {
        v.push(Toggle {
            label: "Jasność".to_owned(),
            value: Some(format!("{pct}%")),
            icon: IconName::Brightness,
            active: pct > 0,
            action: ToggleAction::Brightness,
        });
    }
    v
}

/// Uruchamia aplikację o indeksie `i` (jeśli ma polecenie).
fn launch(apps: &[App], i: usize) {
    if let Some(app) = apps.get(i) {
        launch_app(app);
    }
}

/// Uruchamia aplikację, obsługując aplikacje konsolowe w emulatorze terminala oraz odpinając we/wy.
///
/// Każde uruchomienie (i każdy błąd) ląduje w `$XDG_STATE_HOME/luna/apps.log` — bez tego
/// „nie odpala się” nie da się zdiagnozować: `spawn` kończy się sukcesem nawet gdy
/// polecenia nie ma, a komunikat ginie w `/dev/null`.
fn launch_app(app: &App) {
    let Some(exec) = app.exec.clone() else { return };
    // Aplikacje GPU (Zed, Electron, Chrome) nie utworzą powierzchni Wayland w Selene —
    // kompozytor nie ma dmabuf/EGL. Uruchamiamy je przez Xwayland (X11), który działa.
    let force_x11 = needs_x11(app);
    // 1) `gio launch` — standardowa ścieżka dla wpisów `.desktop` (obsługuje `Path=`,
    //    kody pól, `DBusActivatable`). Uruchamiamy je w wątku, bo `gio` potrafi zwrócić
    //    błąd PO starcie (np. „Nie można wczytać informacji o programie”) — wtedy
    //    musimy mieć szansę na fallback, a nie blokować powłoki.
    //    Aplikacje terminalowe pomijamy — `gio` nie otworzy im emulatora.
    //    Aplikacji wymuszonych na X11 **nie** uruchamiamy przez `gio`: wpisy z
    //    `DBusActivatable=true` są aktywowane przez magistralę, która ma własne
    //    środowisko (z `WAYLAND_DISPLAY`), więc nasze `env_remove` byłoby ignorowane
    //    i np. Zed dalej wybierałby Wayland/Vulkan.
    if !app.terminal && !force_x11 {
        if let Some(path) = app.desktop_path.clone() {
            let exec = exec.clone();
            std::thread::spawn(move || {
                let mut cmd = std::process::Command::new("gio");
                cmd.arg("launch")
                    .arg(&path)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null());
                if force_x11 {
                    force_x11_env(&mut cmd);
                }
                apply_display(&mut cmd);
                match apps_log() {
                    Some(log) => {
                        cmd.stderr(std::process::Stdio::from(log));
                    }
                    None => {
                        cmd.stderr(std::process::Stdio::null());
                    }
                }
                let ok = cmd.status().map(|s| s.success()).unwrap_or(false);
                if ok {
                    log_apps(&format!(
                        "start (gio launch{}): {}",
                        if force_x11 { ", X11" } else { "" },
                        path.display()
                    ));
                } else {
                    log_apps(&format!(
                        "gio launch nie powiódł się ({}), próbuję Exec: {exec}",
                        path.display()
                    ));
                    launch_direct(&exec, false, force_x11);
                }
            });
            return;
        }
    }
    // 2) Bez `gio`: bezpośrednie `Exec`.
    launch_direct(&exec, app.terminal, force_x11);
}

/// Czy aplikacja wymaga XWaylanda (X11).
///
/// Selene jest kompozytorem software'owym i nie oferuje `zwp_linux_dmabuf_v1`/EGL,
/// więc aplikacje renderujące przez GPU nie mogą utworzyć powierzchni Wayland.
/// Domyślnie obejmuje to Zeda, Electrona i Chrome; własne wpisy dodasz zmienną
/// `LUNA_X11_APPS` (np. `LUNA_X11_APPS=moja-aplikacja,inna`).
fn needs_x11(app: &App) -> bool {
    let mut hay =
        format!("{} {}", app.name, app.exec.as_deref().unwrap_or_default()).to_lowercase();
    if let Some(path) = &app.desktop_path {
        hay.push(' ');
        hay.push_str(&path.to_string_lossy().to_lowercase());
    }
    if let Ok(extra) = std::env::var("LUNA_X11_APPS") {
        for token in extra.split([',', ':', ';']) {
            let token = token.trim().to_lowercase();
            if !token.is_empty() && hay.contains(&token) {
                return true;
            }
        }
    }
    const X11_APPS: &[&str] = &[
        "dev.zed.zed",
        "/zed.app/",
        "google-chrome",
        "chromium",
        "code-insiders",
        "/usr/share/code/",
        "electron",
        "discord",
        "slack",
        "spotify",
    ];
    if X11_APPS.iter().any(|needle| hay.contains(needle)) {
        return true;
    }
    // Aplikacje XFCE/GTK3 z tradycyjnym paskiem menu: GTK3 pod Waylandem **zawsze**
    // rysuje własne CSD (nie pyta kompozytora o tryb dekoracji), więc nie da się mu
    // narzucić naszego paska. Przez Xwayland dostają nasz SSD — chyba że naprawdę
    // rysują CSD (`_MOTIF_WM_HINTS`), wtedy nie dublujemy ramki.
    const X11_UI: &[&str] = &[
        "thunar",
        "xfce4-",
        "xfce4_",
        "mousepad",
        "ristretto",
        "screenshooter",
    ];
    X11_UI.iter().any(|needle| hay.contains(needle))
}

/// Ustawia środowisko wymuszające backend X11 (XWayland).
fn force_x11_env(cmd: &mut std::process::Command) {
    cmd.env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .env("GDK_BACKEND", "x11")
        .env("QT_QPA_PLATFORM", "xcb")
        .env("SDL_VIDEODRIVER", "x11")
        .env("CLUTTER_BACKEND", "x11")
        .env("MOZ_ENABLE_WAYLAND", "0");
    apply_display(cmd);
}

/// `DISPLAY` dla uruchamianych aplikacji.
///
/// Sesja startuje powłokę **od razu** po gnieździe Waylanda, zanim Xwayland zdąży
/// zgłosić gotowość, więc w środowisku powłoki może nie być `DISPLAY`. Numer ekranu
/// czytamy wtedy z markera `$XDG_RUNTIME_DIR/luna-display` (Selene zapisuje go, gdy
/// XWM naprawdę połączy się z X11). Sprawdzamy też gniazdo, żeby nie dać martwego `:N`.
fn display_for_children() -> Option<String> {
    if let Ok(d) = std::env::var("DISPLAY") {
        if !d.is_empty() {
            return Some(d);
        }
    }
    let rt = std::env::var("XDG_RUNTIME_DIR").ok()?;
    let num = std::fs::read_to_string(std::path::Path::new(&rt).join("luna-display")).ok()?;
    let num = num.trim();
    if num.is_empty() || !std::path::Path::new(&format!("/tmp/.X11-unix/X{num}")).exists() {
        return None;
    }
    Some(format!(":{num}"))
}

/// Dopisuje `DISPLAY` (jeśli znamy działający ekran X11).
fn apply_display(cmd: &mut std::process::Command) {
    if let Some(display) = display_for_children() {
        cmd.env("DISPLAY", display);
    }
}

/// Emulatory terminala, których używamy dla wpisów `Terminal=true` (w kolejności).
const TERMINALS: &[&str] = &[
    "xfce4-terminal",
    "karton-terminal",
    "gnome-terminal",
    "konsole",
    "alacritty",
    "kitty",
    "foot",
    "xterm",
];

/// Buduje polecenie „terminal + program” dla wpisów `Terminal=true`.
///
/// Wcześniej jedynym kandydatem był `x-terminal-emulator` (na PaganOS nie istnieje),
/// więc **żaden** program konsolowy nie startował z launchera.
fn terminal_command(exec: &str) -> String {
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(t) = std::env::var("TERMINAL") {
        if !t.trim().is_empty() {
            candidates.push(t);
        }
    }
    candidates.extend(TERMINALS.iter().map(|t| (*t).to_owned()));
    for term in candidates {
        let bin = term.split_whitespace().next().unwrap_or("");
        let found = if bin.contains('/') {
            std::path::Path::new(bin).is_file()
        } else {
            std::env::var_os("PATH")
                .map(|path| std::env::split_paths(&path).any(|d| d.join(bin).is_file()))
                .unwrap_or(false)
        };
        if found {
            return format!("{term} -e {exec}");
        }
    }
    exec.to_owned()
}

/// Bezpośredni start `sh -c exec …` (fallback i aplikacje terminalowe).
fn launch_direct(exec: &str, terminal: bool, force_x11: bool) {
    // `%U`, `%F` itp. to kody pól `.desktop`; `gio` je rozwija, ale `sh -c` nie —
    // bez tego program dostawał dosłowny argument `%U`.
    let exec = strip_field_codes(exec);
    let line = if terminal {
        terminal_command(&exec)
    } else {
        format!("exec {exec}")
    };
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(&line);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    if force_x11 {
        force_x11_env(&mut cmd);
    } else {
        apply_display(&mut cmd);
    }
    // Stderr aplikacji kierujemy do logu — tam trafia „command not found”, brak dyspleja itp.
    match apps_log() {
        Some(log) => {
            cmd.stderr(std::process::Stdio::from(log));
        }
        None => {
            cmd.stderr(std::process::Stdio::null());
        }
    }
    match cmd.spawn() {
        Ok(_) => log_apps(&format!(
            "start{}: {line}",
            if force_x11 { " (X11)" } else { "" }
        )),
        Err(e) => log_apps(&format!("BŁĄD startu ({exec}): {e}")),
    }
}

/// Usuwa kody pól `.desktop` (`%U`, `%F`, `%i`, …) z linii `Exec`.
///
/// Zostawiamy `%%` jako literalny znak `%` (tak definiuje to specyfikacja).
fn strip_field_codes(exec: &str) -> String {
    let mut out = String::with_capacity(exec.len());
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some('%') => {
                chars.next();
                out.push('%');
            }
            Some(f) if f.is_ascii_alphabetic() => {
                chars.next();
            }
            _ => out.push('%'),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Uchwyt logu uruchamiania aplikacji (dopisywanie).
fn apps_log() -> Option<std::fs::File> {
    let base = std::env::var("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .ok()
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
            std::path::PathBuf::from(home).join(".local/state")
        });
    let dir = base.join("luna");
    let _ = std::fs::create_dir_all(&dir);
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("apps.log"))
        .ok()
}

/// Dopisuje znacznikowaną godziną linię do logu uruchamiania aplikacji.
fn log_apps(msg: &str) {
    use std::io::Write;
    if let Some(mut f) = apps_log() {
        let _ = writeln!(f, "{} luna-shell: {}", now_clock_full().0, msg);
    }
}

/// Bieżąca godzina i data w czasie lokalnym — `("HH:MM", "DD.MM", rok, miesiąc, dzień)`.
fn now_clock_full() -> (String, String, i64, u32, u32) {
    let t = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    let res = unsafe { libc::localtime_r(&t, tm.as_mut_ptr()) };
    if !res.is_null() {
        let tm = unsafe { tm.assume_init() };
        let h = tm.tm_hour;
        let m = tm.tm_min;
        let y = tm.tm_year as i64 + 1900;
        let mo = tm.tm_mon as u32 + 1;
        let d = tm.tm_mday as u32;
        (
            format!("{h:02}:{m:02}"),
            format!("{d:02}.{mo:02}"),
            y,
            mo,
            d,
        )
    } else {
        let secs = t as u64;
        let days = (secs / 86_400) as i64;
        let secs_of_day = secs % 86_400;
        let (h, m) = (secs_of_day / 3600, (secs_of_day % 3600) / 60);
        let (y, mo, d) = civil_from_days(days);
        (
            format!("{h:02}:{m:02}"),
            format!("{d:02}.{mo:02}"),
            y,
            mo,
            d,
        )
    }
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

/// Tłumaczy keysym XKB na znak tekstowy do wyszukiwania w launcherze.
fn keysym_to_char(keysym: u32) -> Option<char> {
    match keysym {
        // ASCII (bez znaków sterujących).
        0x20..=0x7e => char::from_u32(keysym),
        // Latin-1 (np. ó, ä, ö, ü) — te keysymy pokrywają się z Unicode.
        0x00a0..=0x00ff => char::from_u32(keysym),
        // Latin Extended-A/B (polskie ą, ć, ę, ł, ń, ś, ź, ż) i dalsze bloki XKB,
        // których numeracja jest zgodna z Unicode.
        0x0100..=0x17ff => char::from_u32(keysym),
        // Jawny zakres Unicode w XKB (0x01000000 + U+XXXX).
        0x01000000..=0x0110ffff => char::from_u32(keysym - 0x01000000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, exec: &str) -> App {
        App {
            icon: IconName::App,
            name: name.to_owned(),
            category: String::new(),
            exec: Some(exec.to_owned()),
            terminal: false,
            desktop_path: None,
        }
    }

    #[test]
    fn direct_launch_strips_desktop_field_codes() {
        assert_eq!(strip_field_codes("zed %U"), "zed");
        assert_eq!(
            strip_field_codes("thunar %F --new-window"),
            "thunar --new-window"
        );
        assert_eq!(strip_field_codes("app %%U %f"), "app %U");
        assert_eq!(strip_field_codes("app --file=%f"), "app --file=");
    }

    #[test]
    fn gpu_apps_use_xwayland() {
        assert!(needs_x11(&app(
            "Zed",
            "/home/user/.local/zed.app/bin/zed %U"
        )));
        assert!(needs_x11(&app("Google Chrome", "google-chrome-stable %U")));
    }

    #[test]
    fn xfce_apps_use_xwayland_for_our_decorations() {
        // GTK3 pod Waylandem zawsze rysuje własne CSD; przez X11 dostaje nasz pasek.
        assert!(needs_x11(&app("Thunar", "thunar %U")));
        assert!(needs_x11(&app("Terminal", "xfce4-terminal")));
        assert!(needs_x11(&app("Mousepad", "mousepad %F")));
    }

    #[test]
    fn native_wayland_apps_stay_on_wayland() {
        assert!(!needs_x11(&app("Luna — widgety", "luna-showcase")));
    }
}
