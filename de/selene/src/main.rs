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
mod input;
mod layer;
mod monitors;
mod output;
mod seat;
mod shell;
mod shm;
mod state;

use std::error::Error;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::sync::Arc;

use libluna_core::Event;
use wayland_protocols::xdg::decoration::zv1::server::zxdg_decoration_manager_v1;
use wayland_protocols::xdg::shell::server::xdg_wm_base;
use wayland_protocols_wlr::layer_shell::v1::server::zwlr_layer_shell_v1;
use wayland_server::protocol::{wl_compositor, wl_data_device_manager, wl_output, wl_seat, wl_shm};
use wayland_server::{Display, ListeningSocket};

use state::State;

fn main() -> Result<(), Box<dyn Error>> {
    // Wielojęzyczność: lokalizacja z otoczenia + katalogi `i18n/` doładowane w runtime.
    libluna_i18n::set_locale(libluna_i18n::Locale::from_env());
    libluna_i18n::load_catalogs_from_dirs();

    let mut display: Display<State> = Display::new()?;
    let mut handle = display.handle();

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
            *state.outputs.lock().unwrap() = outs;
        }
    }
    let output_count = state.outputs.lock().unwrap().len();

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

    // Gniazdo nasłuchu: `wayland-0`, `wayland-1`, …
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

    // Wyjście nested: okno na nadrzędnym kompozytorze, budowane w osobnym wątku.
    output::spawn(
        Arc::clone(&state.windows),
        Arc::clone(&state.outputs),
        Arc::clone(&events),
        wake_write,
        1280,
        800,
    );

    let listener_fd = socket.as_raw_fd();

    loop {
        let backend_fd = display.backend().poll_fd().as_raw_fd();
        let wake_fd = wake_read.as_raw_fd();

        let listener = unsafe { BorrowedFd::borrow_raw(listener_fd) };
        let backend = unsafe { BorrowedFd::borrow_raw(backend_fd) };
        let wake = unsafe { BorrowedFd::borrow_raw(wake_fd) };
        let mut fds = [
            rustix::event::PollFd::new(&listener, rustix::event::PollFlags::IN),
            rustix::event::PollFd::new(&backend, rustix::event::PollFlags::IN),
            rustix::event::PollFd::new(&wake, rustix::event::PollFlags::IN),
        ];
        rustix::event::poll(&mut fds, -1)?;

        // Nowy klient.
        if fds[0].revents().contains(rustix::event::PollFlags::IN) {
            while let Ok(Some(stream)) = socket.accept() {
                handle.insert_client(stream, Arc::new(()))?;
                state.clients += 1;
            }
        }

        // Wejście z magistrali → trasowanie do klientów.
        if fds[2].revents().contains(rustix::event::PollFlags::IN) {
            input::drain_wake(&mut wake_read);
            let batch: Vec<Event> = {
                let mut queue = events.lock().unwrap();
                std::mem::take(&mut *queue)
            };
            for event in &batch {
                seat::handle_input(&mut state, event);
            }
        }

        display.dispatch_clients(&mut state)?;
        display.flush_clients()?;
    }
}
