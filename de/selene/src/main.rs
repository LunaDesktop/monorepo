//! Selene — kompozytor Wayland środowiska Luna.
//!
//! Świadomie **bez smithay**: protokół obsługujemy własną warstwą na `wayland-server`
//! (rdzeń + xdg-shell), a pętla zdarzeń to bezpośredni `poll` po gnieździe nasłuchu
//! i deskryptorze backendu. Renderowanie docelowo przez `libluna-render`.

mod compositor;
mod seat;
mod shell;
mod shm;
mod state;

use std::error::Error;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::sync::Arc;

use wayland_protocols::xdg::shell::server::xdg_wm_base;
use wayland_server::protocol::{wl_compositor, wl_seat, wl_shm};
use wayland_server::{Display, ListeningSocket};

use state::State;

fn main() -> Result<(), Box<dyn Error>> {
    // Wielojęzyczność: lokalizacja z otoczenia + katalogi `i18n/` doładowane w runtime.
    libluna_i18n::set_locale(libluna_i18n::Locale::from_env());
    libluna_i18n::load_catalogs_from_dirs();

    let mut display: Display<State> = Display::new()?;
    let mut handle = display.handle();

    // Globalne interfejsy protokołu.
    handle.create_global::<State, wl_compositor::WlCompositor, ()>(4, ());
    handle.create_global::<State, wl_shm::WlShm, ()>(1, ());
    handle.create_global::<State, xdg_wm_base::XdgWmBase, ()>(1, ());
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

    let mut state = State::default();
    let listener_fd = socket.as_raw_fd();

    loop {
        let backend_fd = display.backend().poll_fd().as_raw_fd();

        let listener = unsafe { BorrowedFd::borrow_raw(listener_fd) };
        let backend = unsafe { BorrowedFd::borrow_raw(backend_fd) };
        let mut fds = [
            rustix::event::PollFd::new(&listener, rustix::event::PollFlags::IN),
            rustix::event::PollFd::new(&backend, rustix::event::PollFlags::IN),
        ];
        rustix::event::poll(&mut fds, -1)?;

        if fds[0].revents().contains(rustix::event::PollFlags::IN) {
            while let Ok(Some(stream)) = socket.accept() {
                handle.insert_client(stream, Arc::new(()))?;
                state.clients += 1;
            }
        }

        display.dispatch_clients(&mut state)?;
        display.flush_clients()?;
    }
}
