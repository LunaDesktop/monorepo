//! Selene — kompozytor Wayland środowiska Luna.
//!
//! Świadomie **bez smithay**: protokół obsługujemy własną warstwą na `wayland-server`
//! (rdzeń + xdg-shell + output + seat), a pętla zdarzeń to bezpośredni `poll` po gnieździe
//! nasłuchu, deskryptorze backendu i magistrali wejścia. Wyjście (nested) i kompozycja
//! idą przez `libluna-platform` + `libluna-render`.

mod compositor;
mod data;
mod decoration;
mod drm;
mod evdev;
mod foreign_toplevel;
mod input;
mod layer;
mod monitors;
mod output;
mod seat;
mod shell;
mod shm;
mod state;
mod sync;
mod workspace;

use std::error::Error;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use libluna_core::Event;
use wayland_protocols::ext::workspace::v1::server::ext_workspace_manager_v1::ExtWorkspaceManagerV1;
use wayland_protocols::xdg::decoration::zv1::server::zxdg_decoration_manager_v1;
use wayland_protocols::xdg::shell::server::xdg_wm_base;
use wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1;
use wayland_protocols_wlr::layer_shell::v1::server::zwlr_layer_shell_v1;
use wayland_server::protocol::{wl_compositor, wl_data_device_manager, wl_output, wl_seat, wl_shm};
use wayland_server::{Display, ListeningSocket};

use crate::sync::LockExt;
use state::State;

fn main() -> Result<(), Box<dyn Error>> {
    // Wielojęzyczność: lokalizacja z otoczenia + katalogi `i18n/` doładowane w runtime.
    libluna_i18n::set_locale(libluna_i18n::Locale::from_env());
    libluna_i18n::load_catalogs_from_dirs();

    // Krótszy, jednoznaczny log panik — bez zasypywania sesji pełnym backtrace'em.
    // Paniki klientów są łapane niżej (`catch_unwind`), więc sesja żyje dalej.
    std::panic::set_hook(Box::new(|info| eprintln!("Selene panic: {info}")));

    let mut display: Display<State> = Display::new()?;
    let mut handle = display.handle();

    // Diagnostyka DRM: `SELENE_DRM_PROBE=1` sprawdza dostępność mastera bez zmiany trybu.
    if std::env::var("SELENE_DRM_PROBE").as_deref() == Ok("1") {
        match drm::check_master() {
            Some(card) => println!("DRM master available: {}", card.display()),
            None => println!("DRM master unavailable (held by another session)"),
        }
        return Ok(());
    }

    let mut state = State::default();

    // Wykrywanie monitorów: DRM/KMS, a gdy niedostępne — pojedyncze wyjście nested.
    if let Some(outs) = drm::discover() {
        if !outs.is_empty() {
            println!(
                "{}: {}",
                libluna_i18n::tr("Detected outputs"),
                outs.iter()
                    .map(|o| format!("{} {}x{}", o.name, o.width, o.height))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            *state.outputs.lock_or_recover() = outs;
        }
    }
    let output_count = state.outputs.lock_or_recover().len();
    let ws_count = state.ws_count;

    // Globalne interfejsy protokołu.
    handle.create_global::<State, wl_compositor::WlCompositor, ()>(4, ());
    handle.create_global::<State, wl_shm::WlShm, ()>(1, ());
    // Jeden global `wl_output` na monitor.
    for index in 0..output_count {
        handle.create_global::<State, wl_output::WlOutput, usize>(4, index);
    }
    handle.create_global::<State, xdg_wm_base::XdgWmBase, ()>(1, ());
    handle.create_global::<State, zxdg_decoration_manager_v1::ZxdgDecorationManagerV1, ()>(1, ());
    handle.create_global::<State, zwlr_layer_shell_v1::ZwlrLayerShellV1, ()>(4, ());
    handle.create_global::<State, wl_data_device_manager::WlDataDeviceManager, ()>(3, ());
    handle.create_global::<State, wl_seat::WlSeat, ()>(1, ());
    handle.create_global::<State, ZwlrForeignToplevelManagerV1, ()>(3, ());
    handle.create_global::<State, ExtWorkspaceManagerV1, ()>(1, ());

    // Gniazdo nasłuchu: `wayland-0`, `wayland-1`, …
    ensure_runtime_dir()?;
    let socket =
        ListeningSocket::bind_auto("wayland", 0..33).map_err(|e| format!("bind socket: {e:?}"))?;
    println!(
        "{} ({})",
        libluna_i18n::tr("Selene compositor started"),
        socket
            .socket_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    );

    // Magistrala wejścia (wątek wyjścia → wątek protokołu) z parą gniazd do wybudzania.
    let (events, mut wake_read, wake_write) = input::channel();

    // Wyjście: realne DRM/KMS (gdy `SELENE_BACKEND=drm`) lub nested na nadrzędnym kompozytorze.
    if std::env::var("SELENE_BACKEND").as_deref() == Ok("drm") {
        output::spawn_drm(
            Arc::clone(&state.windows),
            Arc::clone(&state.cursor),
            Arc::clone(&state.ws_current),
            Arc::clone(&state.overview),
            ws_count,
        );
    } else {
        output::spawn(
            Arc::clone(&state.windows),
            Arc::clone(&state.outputs),
            Arc::clone(&events),
            wake_write,
            Arc::clone(&state.ws_current),
            Arc::clone(&state.overview),
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

    // Diagnostyka (SELENE_STATS=1): częstość aktualizacji kursora (tempo wejścia evdev).
    let stats = std::env::var("SELENE_STATS").is_ok();
    let mut cursor_updates = 0u64;
    let mut stats_report = std::time::Instant::now();

    loop {
        let backend_fd = display.backend().poll_fd().as_raw_fd();
        let wake_fd = wake_read.as_raw_fd();

        let listener = unsafe { BorrowedFd::borrow_raw(listener_fd) };
        let backend = unsafe { BorrowedFd::borrow_raw(backend_fd) };
        let wake = unsafe { BorrowedFd::borrow_raw(wake_fd) };
        let input_raw = evdev_input.as_ref().map(|e| e.fds()).unwrap_or_default();
        // SAFETY: deskryptory należą do `evdev_input`, który żyje przez całą iterację.
        let input_fds: Vec<BorrowedFd> = input_raw
            .iter()
            .map(|&r| unsafe { BorrowedFd::borrow_raw(r) })
            .collect();
        let mut fds = vec![
            rustix::event::PollFd::new(&listener, rustix::event::PollFlags::IN),
            rustix::event::PollFd::new(&backend, rustix::event::PollFlags::IN),
            rustix::event::PollFd::new(&wake, rustix::event::PollFlags::IN),
        ];
        for fd in &input_fds {
            fds.push(rustix::event::PollFd::new(fd, rustix::event::PollFlags::IN));
        }
        rustix::event::poll(&mut fds, -1)?;

        // Wejście evdev → trasowanie (jak z magistrali).
        if input_fds
            .iter()
            .enumerate()
            .any(|(i, _)| fds[3 + i].revents().contains(rustix::event::PollFlags::IN))
        {
            if let Some(input) = evdev_input.as_mut() {
                let bounds = state
                    .outputs
                    .lock_or_recover()
                    .first()
                    .map(|o| (o.width as f64, o.height as f64))
                    .unwrap_or((1280.0, 800.0));
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

        if stats && stats_report.elapsed().as_secs_f64() >= 2.0 {
            let secs = stats_report.elapsed().as_secs_f64();
            let windows = state.windows.lock_or_recover().len();
            eprintln!(
                "selene/input: {:.0} aktualizacji kursora/s | klienci={} okna={}",
                cursor_updates as f64 / secs,
                state.clients,
                windows
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
