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
mod input;
mod layer;
mod monitors;
mod output;
mod seat;
mod shell;
mod shm;
mod state;
mod sync;

use std::error::Error;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::os::unix::fs::MetadataExt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use libluna_core::Event;
use wayland_protocols::xdg::decoration::zv1::server::zxdg_decoration_manager_v1;
use wayland_protocols::xdg::shell::server::xdg_wm_base;
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
        output::spawn_drm(Arc::clone(&state.windows));
    } else {
        output::spawn(
            Arc::clone(&state.windows),
            Arc::clone(&state.outputs),
            Arc::clone(&events),
            wake_write,
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
    }
}

/// Zapewnia, że `XDG_RUNTIME_DIR` wskazuje prywatny katalog użytkownika, zanim
/// zwiążemy gniazdo Wayland.
///
/// Menedżer sesji (logind/`pam_systemd`) normalnie ustawia tę zmienną. Gdy jej brak
/// lub jest bezużyteczna (np. `pam_systemd` nie zdołał założyć sesji i zmienna nie
/// trafiła do środowiska), `ListeningSocket::bind_auto` kończy się błędem
/// `RuntimeDirNotSet`, a kompozytor wychodzi natychmiast — co z zewnątrz wygląda jak
/// „sesja się nie uruchomiła”. Dlatego wybieramy `/run/user/<uid>`, a gdy ono nie
/// istnieje — tworzymy własny katalog `0700` pod `$TMPDIR`.
fn ensure_runtime_dir() -> Result<(), Box<dyn Error>> {
    let uid = rustix::process::getuid().as_raw() as u32;

    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        if is_private_dir_owned_by(&PathBuf::from(dir), uid) {
            return Ok(());
        }
    }

    let run_user = PathBuf::from(format!("/run/user/{uid}"));
    let dir = if is_private_dir_owned_by(&run_user, uid) {
        run_user
    } else {
        let base = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        let dir = base.join(format!("selene-runtime-{uid}"));
        match rustix::fs::mkdir(&dir, rustix::fs::Mode::RWXU) {
            Ok(()) | Err(rustix::io::Errno::EXIST) => {}
            Err(e) => return Err(format!("mkdir {}: {e}", dir.display()).into()),
        }
        if !is_private_dir_owned_by(&dir, uid) {
            return Err(format!(
                "runtime dir {} nie należy do uid {uid} lub jest dostępny dla grupy/innych",
                dir.display()
            )
            .into());
        }
        dir
    };

    println!(
        "{} {}",
        libluna_i18n::tr("XDG_RUNTIME_DIR not set, using"),
        dir.display()
    );
    std::env::set_var("XDG_RUNTIME_DIR", &dir);
    Ok(())
}

/// Czy `path` to istniejący, bezwzględny katalog należący do `uid`, bez dostępu dla
/// grupy i innych użytkowników (tryb `0700`)?
fn is_private_dir_owned_by(path: &Path, uid: u32) -> bool {
    path.is_absolute()
        && std::fs::metadata(path)
            .map(|m| m.is_dir() && m.uid() == uid && m.mode() & 0o077 == 0)
            .unwrap_or(false)
}

#[cfg(test)]
mod runtime_dir_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    #[test]
    fn private_dir_owned_by_uid_is_accepted() {
        let uid = rustix::process::getuid().as_raw() as u32;
        let dir = std::env::temp_dir().join(format!("selene-rdir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();

        assert!(
            is_private_dir_owned_by(&dir, uid),
            "katalog 0700 naszego uid"
        );
        // Obcy uid: nie jest nasz.
        assert!(!is_private_dir_owned_by(&dir, uid.wrapping_add(1)));
        // Zbyt otwarte uprawnienia: grupa/inny mają dostęp.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!is_private_dir_owned_by(&dir, uid));
        // Nieistniejąca ścieżka.
        assert!(!is_private_dir_owned_by(&dir.join("missing"), uid));
        // Ścieżka względna nie nadaje się na `XDG_RUNTIME_DIR`.
        assert!(!is_private_dir_owned_by(Path::new("relative/dir"), uid));

        std::fs::remove_dir_all(&dir).ok();
    }
}
