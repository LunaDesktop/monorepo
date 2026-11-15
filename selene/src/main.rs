//! Selene — kompozytor Wayland środowiska Luna.
//!
//! Świadomie **bez smithay**: protokół obsługujemy własną warstwą na `wayland-server`
//! (rdzeń + xdg-shell + output + seat), a pętla zdarzeń to bezpośredni `poll` po gnieździe
//! nasłuchu, deskryptorze backendu i magistrali wejścia. Wyjście (nested) i kompozycja
//! idą przez `libluna-platform` + `libluna-render`.

mod activation;
mod compositor;
mod content_type;
mod control;
mod cursor;
mod data;
mod data_control;
mod decoration;
mod dmabuf;
mod drm;
mod evdev;
mod foreign_toplevel;
mod fractional_scale;
mod idle_inhibit;
mod input;
mod keyboard_shortcuts_inhibit;
mod layer;
mod monitors;
mod output;
mod pointer_constraints;
mod pointer_ext;
mod pointer_warp;
mod popup;
mod present;
mod primary_selection;
mod protocols;
mod screencopy;
mod screenshot;
mod seat;
mod shell;
mod shm;
mod single_pixel_buffer;
mod state;
mod subsurface;
mod sync;
mod syncobj;
mod tearing_control;
mod test_input;
mod text_input;
mod viewporter;
mod vt;
mod workspace;
mod xdg_foreign;
mod xdg_output;
mod xwayland;
mod xwm;

use std::error::Error;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use libluna_core::Event;
use crate::protocols::input_method_v2::zwp_input_method_manager_v2::ZwpInputMethodManagerV2;
use crate::protocols::virtual_keyboard_v1::zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1;
use wayland_protocols::ext::data_control::v1::server::ext_data_control_manager_v1::ExtDataControlManagerV1;
use wayland_protocols::ext::workspace::v1::server::ext_workspace_manager_v1::ExtWorkspaceManagerV1;
use wayland_protocols::wp::content_type::v1::server::wp_content_type_manager_v1::WpContentTypeManagerV1;
use wayland_protocols::wp::cursor_shape::v1::server::wp_cursor_shape_manager_v1::WpCursorShapeManagerV1;
use wayland_protocols::wp::fractional_scale::v1::server::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols::wp::idle_inhibit::zv1::server::zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1;
use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::server::zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1;
use wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1;
use wayland_protocols::wp::pointer_constraints::zv1::server::zwp_pointer_constraints_v1::ZwpPointerConstraintsV1;
use wayland_protocols::wp::pointer_warp::v1::server::wp_pointer_warp_v1::WpPointerWarpV1;
use wayland_protocols::wp::primary_selection::zv1::server::zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1;
use wayland_protocols::wp::relative_pointer::zv1::server::zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1;
use wayland_protocols::wp::single_pixel_buffer::v1::server::wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1;
use wayland_protocols::wp::tearing_control::v1::server::wp_tearing_control_manager_v1::WpTearingControlManagerV1;
use wayland_protocols::wp::text_input::zv3::server::zwp_text_input_manager_v3::ZwpTextInputManagerV3;
use wayland_protocols::wp::viewporter::server::wp_viewporter::WpViewporter;
use wayland_protocols::xdg::activation::v1::server::xdg_activation_v1::XdgActivationV1;
use wayland_protocols::xdg::decoration::zv1::server::zxdg_decoration_manager_v1;
use wayland_protocols::xdg::shell::server::xdg_wm_base;
use wayland_protocols::xdg::xdg_output::zv1::server::zxdg_output_manager_v1::ZxdgOutputManagerV1;
use wayland_protocols::xwayland::shell::v1::server::xwayland_shell_v1::XwaylandShellV1;
use wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1;
use wayland_protocols_wlr::layer_shell::v1::server::zwlr_layer_shell_v1;
use wayland_server::protocol::{
    wl_compositor, wl_data_device_manager, wl_output, wl_seat, wl_shm, wl_subcompositor,
};
use wayland_server::{Display, ListeningSocket};

use crate::sync::LockExt;
use state::State;

fn main() -> Result<(), Box<dyn Error>> {
    // Wielojęzyczność: lokalizacja z otoczenia + katalogi `i18n/` doładowane w runtime.
    libluna_i18n::set_locale(libluna_i18n::Locale::from_env());
    libluna_i18n::load_catalogs_from_dirs();

    // `SELENE_GPU=1` włącza backend GPU dla warstwy wyjściowej (nested rysuje przez
    // libluna-platform, który wybiera backend po `LUNA_GPU`). Brak EGL = fallback CPU;
    // sesja nigdy nie może zginąć z powodu GPU. Etap 2 z `docs/gpu.md`.
    if std::env::var("SELENE_GPU").is_ok() && std::env::var_os("LUNA_GPU").is_none() {
        std::env::set_var("LUNA_GPU", "1");
    }

    // Ctrl+C / Ctrl+\ w terminalu nie mogą kończyć kompozytora (a więc i sesji).
    // Sygnał ma trafić do dziecka PTY, nie do procesu Selene. Przy okazji logujemy
    // SIGTERM/SIGHUP razem z pidem nadawcy — po nagłym końcu sesji wiadomo, kto ją zamknął.
    vt::install_signal_handlers();

    // Krótszy, jednoznaczny log panik — bez zasypywania sesji pełnym backtrace'em.
    // Paniki klientów są łapane niżej (`catch_unwind`), więc sesja żyje dalej.
    // Z `RUST_BACKTRACE=1` dopisujemy ślad — bez niego komunikat „objects from wrong
    // client” nie mówi, która ścieżka wysłała zdarzenie.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("Selene panic: {info}");
        let bt = std::env::var("RUST_BACKTRACE").unwrap_or_default();
        if !bt.is_empty() && bt != "0" {
            eprintln!("{}", std::backtrace::Backtrace::force_capture());
        }
    }));

    let mut display: Display<State> = Display::new()?;
    let mut handle = display.handle();

    // Diagnostyka DRM: `SELENE_DRM_PROBE=1` wypisuje karty, konektory i tryby EDID
    // (bez zmiany trybu — nie trzeba być DRM masterem).
    if std::env::var("SELENE_DRM_PROBE").as_deref() == Ok("1") {
        print!("{}", drm::probe_report());
        return Ok(());
    }

    let mut state = State::default();
    state.apply_workspace_layout();
    state.apply_input_settings();
    // Progi wygaszania/blokady/usypiania z `~/.config/luna/idle.conf`
    // (Ustawienia mogą je później zmienić przez `luna-control idle-reload`).
    state.reload_idle();
    // Diagnostyka: skryptowane wejście (`SELENE_INPUT_SCRIPT`) — używane w testach
    // izolowanej sesji, aby odtworzyć kliknięcia bez fizycznej myszy.
    let mut script_input = test_input::ScriptInput::open();

    // Wykrywanie monitorów: DRM/KMS, a gdy niedostępne — pojedyncze wyjście nested.
    if let Some(outs) = drm::discover() {
        if !outs.is_empty() {
            println!(
                "{}: {}",
                libluna_i18n::tr("Detected outputs"),
                outs.iter()
                    .map(|o| format!("{} {}x{}@{}", o.name, o.width, o.height, o.refresh_hz))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            *state.outputs.lock_or_recover() = outs;
        }
    }
    let output_count = state.outputs.lock_or_recover().len();
    let ws_count = std::sync::Arc::clone(&state.ws_count_model);
    state.display_handle = Some(handle.clone());

    // Globalne interfejsy protokołu.
    handle.create_global::<State, wl_compositor::WlCompositor, ()>(4, ());
    handle.create_global::<State, wl_subcompositor::WlSubcompositor, ()>(1, ());
    handle.create_global::<State, wl_shm::WlShm, ()>(1, ());
    // Bufory GPU klientów (`zwp_linux_dmabuf_v1`) ogłaszamy **tylko** z backendem GPU:
    // bez EGL nie umiemy ich wyświetlić, a obietnica bez pokrycia psuje aplikacje.
    let gpu_on = std::env::var("LUNA_GPU")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);
    if gpu_on && dmabuf::available() {
        handle.create_global::<State, ZwpLinuxDmabufV1, ()>(4, ());
        dmabuf::enable();
        if std::env::var("SELENE_STATS").is_ok() {
            let formats = libluna_render::gpu::dmabuf_formats();
            eprintln!("selene/dmabuf: global v3 ({} formatów EGL)", formats.len());
        }
        // Jawna synchronizacja buforów (`wp_linux_drm_syncobj_v1`). Ogłaszamy ją
        // tylko gdy sterownik naprawdę wspiera osie czasu — bez tego klienty
        // (Xwayland, Mesa) nie mają jak przekazać fence'ów, a my nie mamy czego
        // czekać. Zapisuje to też punkt „release”, więc klient nie czeka na
        // niejawną synchronizację sterownika.
        if syncobj::probe() {
            handle.create_global::<State, syncobj::Manager, ()>(1, ());
        }
    }
    // Jeden global `wl_output` na monitor.
    for index in 0..output_count {
        handle.create_global::<State, wl_output::WlOutput, usize>(4, index);
    }
    state.output_globals = output_count;
    // Wersja 3: `xdg_popup.reposition` + `xdg_positioner.set_parent_size`
    // (v3 obsługujemy w `shell.rs`; nieznane żądania są ignorowane). Niższa wersja
    // odbiera klientom możliwość przestawiania menu bez zamykania ich.
    handle.create_global::<State, xdg_wm_base::XdgWmBase, ()>(3, ());
    handle.create_global::<State, zxdg_decoration_manager_v1::ZxdgDecorationManagerV1, ()>(1, ());
    handle.create_global::<State, zwlr_layer_shell_v1::ZwlrLayerShellV1, ()>(4, ());
    handle.create_global::<State, wl_data_device_manager::WlDataDeviceManager, ()>(3, ());
    // Menedżer schowka (historia, przetrwanie zamknięcia źródła).
    handle.create_global::<State, ExtDataControlManagerV1, ()>(1, ());
    // Aktywacja okien z portali/launcherów (tokeny jednorazowe).
    handle.create_global::<State, XdgActivationV1, ()>(1, ());
    // Selekcja pierwotna (środkowy przycisk) — osobny schowek od Ctrl+C/Ctrl+V.
    handle.create_global::<State, ZwpPrimarySelectionDeviceManagerV1, ()>(1, ());
    // Wersja 4 to minimum dla `wl_keyboard.repeat_info`; wersja 1 kończyła klienta
    // twardym przerwaniem, gdy wysyłaliśmy zdarzenie ponad zadeklarowaną wersję obiektu.
    handle.create_global::<State, wl_seat::WlSeat, ()>(5, ());
    // Kształty kursora (`text`, `pointer`, `ew-resize`…): bez tego każda aplikacja
    // GTK/Qt pokazuje naszą strzałkę nawet nad polem tekstowym.
    handle.create_global::<State, WpCursorShapeManagerV1, ()>(2, ());
    // Względny ruch kursora: gry i mouselook (także przez XWayland).
    handle.create_global::<State, ZwpRelativePointerManagerV1, ()>(1, ());
    // Blokada/zamknięcie kursora w regionie (mouselook, mapy, edytory).
    handle.create_global::<State, ZwpPointerConstraintsV1, ()>(1, ());
    // Wideo/prezentacje nie wygaszają ekranu.
    handle.create_global::<State, ZwpIdleInhibitManagerV1, ()>(1, ());
    // IME: pole tekstowe w aplikacji + input method (fcitx5/ibus/własna).
    handle.create_global::<State, ZwpTextInputManagerV3, ()>(1, ());
    handle.create_global::<State, ZwpInputMethodManagerV2, ()>(1, ());
    handle.create_global::<State, ZwpVirtualKeyboardManagerV1, ()>(1, ());
    // Skalowanie/kadrowanie buforów i preferowana skala ułamkowa.
    handle.create_global::<State, WpViewporter, ()>(1, ());
    handle.create_global::<State, WpFractionalScaleManagerV1, ()>(1, ());
    handle.create_global::<State, ZwlrForeignToplevelManagerV1, ()>(3, ());
    // Kopiowanie ekranu (`wlr-screencopy`) — ScreenCast/Screenshot przez portal.
    // Renderujemy scenę tym samym kodem co monitor, więc zrzut jest 1:1 z obrazem.
    handle.create_global::<State, crate::screencopy::Manager, ()>(3, ());
    // Uchwyty powierzchni między klientami (`xdg-foreign`). Portal używa ich do
    // ustawienia okna dialogu jako dziecka okna aplikacji (v1: GTK3, v2: GTK4/Flatpak).
    handle.create_global::<State, crate::xdg_foreign::ExporterV1, ()>(1, ());
    handle.create_global::<State, crate::xdg_foreign::ImporterV1, ()>(1, ());
    handle.create_global::<State, crate::xdg_foreign::ExporterV2, ()>(1, ());
    handle.create_global::<State, crate::xdg_foreign::ImporterV2, ()>(1, ());
    handle.create_global::<State, ExtWorkspaceManagerV1, ()>(1, ());
    // X11 (XWayland): bez tego globalu Xwayland nie przypisze okien X11 do powierzchni.
    handle.create_global::<State, XwaylandShellV1, ()>(1, ());
    // Opis monitorów dla aplikacji (`xdg-output`): bez niego Qt ostrzega o nieznanym
    // ekranie, a Java/AWT, OBS i część gier widzą tylko pierwszy monitor.
    handle.create_global::<State, ZxdgOutputManagerV1, ()>(3, ());
    // Inhibicja skrótów pulpitu: gry, emulatory i klienty zdalnego pulpitu muszą
    // móc przechwycić Super/Alt+Tab, inaczej te klawisze nigdy do nich nie dotrą.
    handle.create_global::<State, ZwpKeyboardShortcutsInhibitManagerV1, ()>(1, ());
    // Rodzaj treści powierzchni (wideo/foto) — Chrome i Firefox zgłaszają go
    // przed odtwarzaniem; bez globalu zgłaszają brak wsparcia.
    handle.create_global::<State, WpContentTypeManagerV1, ()>(1, ());
    // Bufory 1×1 (GTK4/Chromium): tło, maska albo świecący piksel bez pełnego bufora.
    handle.create_global::<State, WpSinglePixelBufferManagerV1, ()>(1, ());
    // Przenoszenie kursora: aplikacje X11 przez Xwaylanda, Java/AWT i Wine.
    handle.create_global::<State, WpPointerWarpV1, ()>(1, ());
    // Preferencja tearingu (gry) — przyjmujemy, ale prezentacja zostaje przy vblanku.
    handle.create_global::<State, WpTearingControlManagerV1, ()>(1, ());

    // Gniazdo nasłuchu: `wayland-0`, `wayland-1`, …
    ensure_runtime_dir()?;
    let socket =
        ListeningSocket::bind_auto("wayland", 0..33).map_err(|e| format!("bind socket: {e:?}"))?;
    let socket_name = socket
        .socket_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    println!(
        "{} ({socket_name})",
        libluna_i18n::tr("Selene compositor started")
    );

    // Marker gotowości dla menedżera sesji: nazwa gniazda, na którym **naprawdę**
    // słuchamy. Bez tego sesja musi zgadywać `wayland-0` — a jeśli po poprzedniej
    // sesji został martwy plik gniazda, połączyłaby się z nikim.
    if let Ok(rt) = std::env::var("XDG_RUNTIME_DIR") {
        let path = std::path::Path::new(&rt).join("luna-wayland");
        let _ = std::fs::write(&path, socket_name.as_bytes());
        // Liczba monitorów dla menedżera sesji: tyle instancji powłoki uruchomi —
        // jedną na każdy wyświetlacz (każda wiąże swoje `wl_output`).
        let outs = std::path::Path::new(&rt).join("luna-outputs");
        let _ = std::fs::write(&outs, output_count.to_string());
    }

    // Aplikacje X11: uruchamiamy Xwayland i zapisujemy numer ekranu dla menedżera sesji.
    // Brak Xwaylanda nie może przewrócić sesji Wayland — dlatego wszystko jest opcjonalne.
    if std::env::var("SELENE_NO_XWAYLAND").is_err() {
        // Marker z poprzedniej sesji mógłby dać sesji `DISPLAY` na martwy ekran.
        xwayland::clear_display_marker();
        match xwayland::spawn(&socket_name) {
            Some(display) => {
                // XWM startuje w tle (Xwayland potrzebuje chwili, zanim zacznie
                // przyjmować połączenia) i sam rejestruje się, gdy będzie gotowy.
                xwm::spawn(display);
            }
            None => {
                if std::env::var("SELENE_STATS").is_ok() {
                    eprintln!(
                        "selene/xwayland: nie udało się uruchomić Xwayland (aplikacje X11 nie ruszą)"
                    );
                }
            }
        }
    }

    // Magistrala wejścia (wątek wyjścia → wątek protokołu) z parą gniazd do wybudzania.
    let (events, mut wake_read, wake_write) = input::channel();
    // Kanał wybudzania pętli głównej przez wątek wyjścia (po flipie / tick_frames),
    // żeby `display.flush_clients()` wysłało `wl_buffer.release` i `wl_callback.done` od razu.
    let (mut present_wake_read, present_wake_write) =
        std::os::unix::net::UnixStream::pair().expect("socketpair prezentacji");
    let _ = present_wake_read.set_nonblocking(true);
    state.present.set_wake(present_wake_write);
    let render_wake = output::RenderWake::new();

    // Wyjście: realne DRM/KMS (gdy `SELENE_BACKEND=drm`) lub nested na nadrzędnym kompozytorze.
    if std::env::var("SELENE_BACKEND").as_deref() == Ok("drm") {
        state.drm_backend = true;
        // Konsola zostaje w trybie tekstowym, więc Ctrl+C z klawiatury wysyła SIGINT do
        // grupy procesów pierwszego planu VT — czyli do greetd i całej sesji. Przejmujemy
        // pierwszy plan, żeby sygnały z klawiatury trafiały wyłącznie do Selene.
        vt::claim_foreground();
        output::spawn_drm(
            Arc::clone(&state.windows),
            Arc::clone(&state.cursor),
            Arc::clone(&state.cursor_image),
            Arc::clone(&state.ws_current),
            Arc::clone(&state.overview),
            Arc::clone(&state.screenshot),
            Arc::clone(&state.present),
            Arc::clone(&render_wake),
            Arc::clone(&state.night_light),
            Arc::clone(&state.power_saver),
            Arc::clone(&state.outputs),
            ws_count,
            Arc::clone(&state.display_request),
            Arc::clone(&state.position_request),
            Arc::clone(&state.vt_request),
        );
    } else {
        output::spawn(
            Arc::clone(&state.windows),
            Arc::clone(&state.outputs),
            Arc::clone(&events),
            wake_write,
            Arc::clone(&state.ws_current),
            Arc::clone(&state.overview),
            Arc::clone(&state.screenshot),
            Arc::clone(&state.present),
            ws_count,
            1280,
            800,
        );
    }

    let listener_fd = socket.as_raw_fd();

    // Wejście evdev (dla trybu DRM, gdzie nie ma nadrzędnego kompozytora).
    let mut evdev_input = if std::env::var("SELENE_INPUT").as_deref() == Ok("evdev") {
        match evdev::EvdevInput::open() {
            Some(input) => {
                println!("{}", libluna_i18n::tr("evdev input enabled"));
                Some(input)
            }
            None => {
                eprintln!("{}", libluna_i18n::tr("evdev input unavailable"));
                None
            }
        }
    } else {
        None
    };
    // Blokady (NumLock/CapsLock) mogły być włączone jeszcze przed startem sesji —
    // bez tego numpad dawał nawigację zamiast cyfr, dopóki nie przełączysz NumLock.
    if let Some(input) = &evdev_input {
        let (num_lock, caps_lock) = input.locks();
        state.num_lock = num_lock;
        state.caps_lock = caps_lock;
    }
    // Sterowanie z powłoki (PPM w taskbarze → „przenieś na przestrzeń”).
    let control = control::Control::spawn();

    // Diagnostyka (SELENE_STATS=1): częstość aktualizacji kursora (tempo wejścia evdev).
    let stats = std::env::var("SELENE_STATS").is_ok();
    let mut cursor_updates = 0u64;
    let mut stats_report = std::time::Instant::now();

    loop {
        // Most schowka Wayland → X11: uruchom pobranie i odbierz bajty, które
        // już przyszły. `wl_data_source.send` musi iść z tego wątku (protokół),
        // a rura jest dołączona do `poll` niżej — bez blokowania sesji.
        crate::data::pump_clipboard();
        // Kopia deskryptora na czas `poll` — menedżer schowka może zamknąć
        // oryginał w trakcie `dispatch_clients`.
        let clipboard_dup = crate::data::clipboard_fd().and_then(|fd| {
            let fd = unsafe { BorrowedFd::borrow_raw(fd) };
            rustix::io::dup(fd).ok()
        });
        // Skryptowane wejście (testy izolowanej sesji).
        if let Some(script) = script_input.as_mut() {
            script.poll(&mut state, &handle);
        }
        let backend_fd = display.backend().poll_fd().as_raw_fd();
        let wake_fd = wake_read.as_raw_fd();
        let present_wake_fd = present_wake_read.as_raw_fd();

        let listener = unsafe { BorrowedFd::borrow_raw(listener_fd) };
        let backend = unsafe { BorrowedFd::borrow_raw(backend_fd) };
        let wake = unsafe { BorrowedFd::borrow_raw(wake_fd) };
        let present_wake = unsafe { BorrowedFd::borrow_raw(present_wake_fd) };
        let input_raw = evdev_input.as_ref().map(|e| e.fds()).unwrap_or_default();
        let input_fds: Vec<BorrowedFd> = input_raw
            .iter()
            .map(|&r| unsafe { BorrowedFd::borrow_raw(r) })
            .collect();

        let mut fds = Vec::with_capacity(6 + input_fds.len());
        fds.push(rustix::event::PollFd::new(
            &listener,
            rustix::event::PollFlags::IN,
        ));
        fds.push(rustix::event::PollFd::new(
            &backend,
            rustix::event::PollFlags::IN,
        ));
        fds.push(rustix::event::PollFd::new(
            &wake,
            rustix::event::PollFlags::IN,
        ));
        for fd in &input_fds {
            fds.push(rustix::event::PollFd::new(fd, rustix::event::PollFlags::IN));
        }
        // Gniazdo sterowania powłoki (opcjonalne — brak nie może zatrzymać sesji).
        let control_fd = control
            .as_ref()
            .map(|c| unsafe { BorrowedFd::borrow_raw(c.fd()) });
        if let Some(fd) = &control_fd {
            fds.push(rustix::event::PollFd::new(fd, rustix::event::PollFlags::IN));
        }
        // Rura mostka schowka (jeśli trwa pobieranie tekstu dla X11).
        if let Some(fd) = &clipboard_dup {
            fds.push(rustix::event::PollFd::new(fd, rustix::event::PollFlags::IN));
        }
        let present_wake_idx = fds.len();
        fds.push(rustix::event::PollFd::new(
            &present_wake,
            rustix::event::PollFlags::IN,
        ));
        // Budzimy się co ~16 ms przy skrypcie wejścia (dopisane polecenia) oraz gdy
        // kursor stoi w „gorącym rogu” — poświata musi rosnąć, a aktywacja nie może
        // zależeć od dalszego ruchu myszy. Bez tego budzimy się najpóźniej co sekundę,
        // gdy czeka jakikolwiek próg bezczynności (wygaszenie/blokada/usypianie).
        let corner_wait = state.ws_corner_hover.is_some() && !state.ws_corner_latch;
        let idle_wait = crate::seat::idle_wait_ms(&state);
        // Czekając na dane schowka, nie możemy spać bez końca — deadline transferu
        // (1,5 s) egzekwuje dopiero kolejne `pump_clipboard`.
        let clipboard_waiting = crate::data::clipboard_pending();
        let poll_timeout = if script_input.is_some() || corner_wait {
            16
        } else if clipboard_waiting {
            20
        } else {
            idle_wait.map(|ms| ms.clamp(16, 1000) as i32).unwrap_or(-1)
        };
        match rustix::event::poll(&mut fds, poll_timeout) {
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(e) => return Err(e.into()),
        }
        if fds[present_wake_idx]
            .revents()
            .contains(rustix::event::PollFlags::IN)
        {
            use std::io::Read;
            let mut buf = [0u8; 64];
            while present_wake_read.read(&mut buf).is_ok_and(|n| n > 0) {}
        }

        // Odliczanie przytrzymania w rogu (rysowanie poświaty i ewentualna akcja).
        crate::seat::tick_corner(&mut state);
        // Automatyczne wygaszenie/blokada/usypianie po bezczynności.
        crate::seat::tick_idle(&mut state);

        // Sterowanie z powłoki (np. przeniesienie okna na inną przestrzeń).
        if let (Some(c), Some(_)) = (&control, &control_fd) {
            let idx = 3 + input_fds.len();
            if fds[idx].revents().contains(rustix::event::PollFlags::IN) {
                c.accept(&mut state);
            }
        }

        // Wejście evdev → trasowanie (jak z magistrali).
        if (0..input_fds.len()).any(|i| fds[3 + i].revents().contains(rustix::event::PollFlags::IN))
        {
            if let Some(input) = evdev_input.as_mut() {
                let bounds = {
                    let outs = state.outputs.lock_or_recover();
                    if outs.is_empty() {
                        (1280.0, 800.0)
                    } else {
                        let total_w = outs
                            .iter()
                            .map(|o| (o.x + o.width) as f64)
                            .fold(0.0, f64::max);
                        let total_h = outs
                            .iter()
                            .map(|o| (o.y + o.height) as f64)
                            .fold(0.0, f64::max);
                        (total_w, total_h)
                    }
                };
                let batch = input.read_events(bounds);
                *state.cursor.lock_or_recover() = Some(input.cursor());
                cursor_updates += 1;
                for event in &batch {
                    seat::handle_input(&mut state, &handle, event);
                }
            }
        }

        // Nowy klient.
        if fds[0].revents().contains(rustix::event::PollFlags::IN) {
            while let Ok(Some(stream)) = socket.accept() {
                match handle.insert_client(stream, Arc::new(())) {
                    Ok(_) => state.clients += 1,
                    Err(e) => eprintln!("client rejected: {e}"),
                }
            }
        }

        // Wejście z magistrali → trasowanie do klientów.
        if fds[2].revents().contains(rustix::event::PollFlags::IN) {
            input::drain_wake(&mut wake_read);
            let batch: Vec<Event> = {
                let mut queue = events.lock_or_recover();
                std::mem::take(&mut *queue)
            };
            for event in &batch {
                seat::handle_input(&mut state, &handle, event);
            }
        }

        // Odporność: panika albo błąd w obsłudze pojedynczego klienta nie może zabić
        // całej sesji — łapiemy panikę i kontynuujemy pętlę.
        match catch_unwind(AssertUnwindSafe(|| display.dispatch_clients(&mut state))) {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => eprintln!("{}", libluna_i18n::tr("client request panicked; ignoring")),
        }
        match catch_unwind(AssertUnwindSafe(|| display.flush_clients())) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => eprintln!("{}", libluna_i18n::tr("client request panicked; ignoring")),
        }

        // X11: tytuły okien i życzenia stanu od klientów (XWM działa w osobnym wątku).
        xwm::sync(&mut state);
        // Most schowka X11 → Wayland: XWM publikuje migawkę tekstu, my rozsyłamy oferty.
        crate::data::sync_x11_selection(&mut state, &handle);
        // Obudź wątek DRM, jeśli komit/wejście/XWM mogło zmienić scenę.
        render_wake.notify();
        // Nowe okno/popup pod **nieruchomym** kursorem musi dostać `wl_pointer.enter`.
        // Bez tego menu przeglądarki nie podświetla pozycji, dopóki nie ruszysz myszą,
        // a pierwszy klik bywa traktowany jak klik poza menu.
        crate::seat::refresh_pointer_focus(&mut state);
        // Klienci muszą wiedzieć, na którym wyjściu leży ich powierzchnia
        // (`wl_surface.enter`) — bez tego Chromium nie traktuje swoich okien
        // potomnych (bąble rysowane jako `wl_subsurface`) jako interaktywnych.
        crate::output::sync_surface_outputs(&state);
        // Faktyczny modeset DRM mógł poprawić rozmiar/odświeżanie — wyślij klientom
        // świeże `wl_output` (inaczej panel i okna zostają przy wczesnym probe).
        monitors::refresh_bound(&mut state);
        // Gdy DRM wykrył więcej monitorów niż wczesny probe, dołóż brakujące globale.
        let want_outputs = state.outputs.lock_or_recover().len();
        while state.output_globals < want_outputs {
            handle.create_global::<State, wl_output::WlOutput, usize>(4, state.output_globals);
            state.output_globals += 1;
        }

        if stats && stats_report.elapsed().as_secs_f64() >= 2.0 {
            let secs = stats_report.elapsed().as_secs_f64();
            let windows = state.windows.lock_or_recover().len();
            eprintln!(
                "selene/input: {:.0} aktualizacji kursora/s | klienci={} okna={} | x11(wm)={}",
                cursor_updates as f64 / secs,
                state.clients,
                windows,
                xwm::global().map(|x| x.is_wm()).unwrap_or(false)
            );
            cursor_updates = 0;
            stats_report = std::time::Instant::now();
        }
    }
}

/// Zapewnia, że `XDG_RUNTIME_DIR` jest ustawiony, zanim zwiążemy gniazdo Wayland.
///
/// Wspólna logika (wybór `/run/user/<uid>` albo prywatnego katalogu pod `$TMPDIR`)
/// mieszka w [`libluna_core::runtime`] — korzystają z niej też klienci (m.in. `luna-shell`),
/// żeby trafić na to samo gniazdo. Tu tylko raportujemy wybrany katalog.
fn ensure_runtime_dir() -> Result<(), Box<dyn Error>> {
    if let Some(dir) = libluna_core::runtime::ensure_runtime_dir()? {
        println!(
            "{} {}",
            libluna_i18n::tr("XDG_RUNTIME_DIR not set, using"),
            dir.display()
        );
    }
    Ok(())
}
