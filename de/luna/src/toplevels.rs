//! Klient `zwlr_foreign_toplevel_management` — lista okien dla paska (taskbar).
//!
//! Powłoka otwiera **własne** połączenie Wayland (niezależne od pętli `libluna-platform`),
//! binduje menedżera `zwlr_foreign_toplevel` i w osobnym wątku śledzi uchwyty okien.
//! UI (wątek rysowania) czyta gotową listę przez [`Toplevels::tasks`], a kliknięcia
//! tłumaczy na [`Toplevels::activate`] / [`Toplevels::close`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_registry;
use wayland_client::protocol::wl_seat;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1 as hdl, zwlr_foreign_toplevel_manager_v1 as mgr,
};

/// Stany okna (`zwlr_foreign_toplevel_handle_v1.state`).
pub const ST_MAXIMIZED: u32 = 0;
pub const ST_MINIMIZED: u32 = 1;
pub const ST_ACTIVATED: u32 = 2;
pub const ST_FULLSCREEN: u32 = 3;

/// Migawka okna dla UI.
#[derive(Debug, Clone, Default)]
pub struct Task {
    pub title: String,
    pub app_id: String,
    pub active: bool,
}

#[derive(Default)]
struct Entry {
    title: String,
    app_id: String,
    states: Vec<u32>,
    proxy: Option<hdl::ZwlrForeignToplevelHandleV1>,
    closed: bool,
}

#[derive(Default)]
struct Shared {
    order: Vec<u64>,
    map: HashMap<u64, Entry>,
    seat: Option<wl_seat::WlSeat>,
}

struct DispatchState {
    shared: Arc<Mutex<Shared>>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for DispatchState {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for DispatchState {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<mgr::ZwlrForeignToplevelManagerV1, ()> for DispatchState {
    fn event(
        state: &mut Self,
        _mgr: &mgr::ZwlrForeignToplevelManagerV1,
        event: mgr::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let mgr::Event::Toplevel { toplevel } = event {
            let id = toplevel.id().protocol_id() as u64;
            let mut s = state.shared.lock().unwrap();
            s.order.push(id);
            s.map.insert(
                id,
                Entry {
                    proxy: Some(toplevel),
                    ..Default::default()
                },
            );
        }
    }
}

impl Dispatch<hdl::ZwlrForeignToplevelHandleV1, ()> for DispatchState {
    fn event(
        state: &mut Self,
        handle: &hdl::ZwlrForeignToplevelHandleV1,
        event: hdl::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let id = handle.id().protocol_id() as u64;
        let mut s = state.shared.lock().unwrap();
        let Some(e) = s.map.get_mut(&id) else {
            return;
        };
        match event {
            hdl::Event::Title { title } => e.title = title,
            hdl::Event::AppId { app_id } => e.app_id = app_id,
            hdl::Event::State { state } => e.states = decode_states(&state),
            hdl::Event::Closed => e.closed = true,
            _ => {}
        }
    }
}

/// Powłoka połączona z listą okien kompozytora.
pub struct Toplevels {
    shared: Arc<Mutex<Shared>>,
    conn: Connection,
}

impl Toplevels {
    /// Łączy się z kompozytorem i uruchamia wątek śledzący okna.
    /// `None`, gdy nie ma kompozytora albo brakuje protokołu.
    pub fn spawn() -> Option<Self> {
        // Klient musi mieć `XDG_RUNTIME_DIR`, żeby znaleźć gniazdo kompozytora
        // (menedżer sesji zwykle je ustawia; gdy brak — wybieramy je jak kompozytor).
        let _ = libluna_core::runtime::ensure_runtime_dir();
        let conn = Connection::connect_to_env().ok()?;
        let (globals, mut queue) = registry_queue_init::<DispatchState>(&conn).ok()?;
        let qh = queue.handle();

        let shared = Arc::new(Mutex::new(Shared::default()));
        let mut state = DispatchState {
            shared: Arc::clone(&shared),
        };

        let _manager = globals
            .bind::<mgr::ZwlrForeignToplevelManagerV1, DispatchState, ()>(&qh, 1..=3, ())
            .ok()?;
        if let Ok(seat) = globals.bind::<wl_seat::WlSeat, DispatchState, ()>(&qh, 1..=1, ()) {
            shared.lock().unwrap().seat = Some(seat);
        }

        std::thread::spawn(move || loop {
            if queue.blocking_dispatch(&mut state).is_err() {
                break;
            }
        });

        Some(Self { shared, conn })
    }

    /// Bieżąca lista okien (kolejność jak w kompozytorze).
    pub fn tasks(&self) -> Vec<Task> {
        let s = self.shared.lock().unwrap();
        s.order
            .iter()
            .filter_map(|id| s.map.get(id))
            .filter(|e| !e.closed)
            .map(|e| Task {
                title: e.title.clone(),
                app_id: e.app_id.clone(),
                active: e.states.contains(&ST_ACTIVATED),
            })
            .collect()
    }

    /// Prosi kompozytor o aktywowanie okna o indeksie `index`.
    pub fn activate(&self, index: usize) {
        {
            let s = self.shared.lock().unwrap();
            if let (Some(id), Some(seat)) = (s.order.get(index), s.seat.as_ref()) {
                if let Some(h) = s.map.get(id).and_then(|e| e.proxy.as_ref()) {
                    let _ = h.activate(seat);
                }
            }
        }
        let _ = self.conn.flush();
    }

    /// Prosi kompozytor o zamknięcie okna o indeksie `index`.
    pub fn close(&self, index: usize) {
        {
            let s = self.shared.lock().unwrap();
            if let Some(id) = s.order.get(index) {
                if let Some(h) = s.map.get(id).and_then(|e| e.proxy.as_ref()) {
                    let _ = h.close();
                }
            }
        }
        let _ = self.conn.flush();
    }
}

/// Dekoduje tablicę `state` (każdy stan to 4-bajtowe `u32`, kolejność natywna).
fn decode_states(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .filter_map(|c| c.try_into().ok())
        .map(u32::from_ne_bytes)
        .collect()
}
