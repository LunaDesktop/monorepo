//! Test `xdg-foreign` v1: eksport powierzchni, import uchwytu i `set_parent_of`.
//!
//! Uruchomienie (w sesji Selene):
//!     cargo run --example xdg-foreign-probe
//!
//! Kod wyjścia 0 = kompozytor obsłużył oba globals i nie zerwał połączenia.

use std::sync::{Arc, Mutex};

use wayland_client::protocol::{wl_compositor, wl_registry, wl_surface};
use wayland_client::{
    globals::{registry_queue_init, GlobalListContents},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::xdg::foreign::zv1::client::{
    zxdg_exported_v1, zxdg_exporter_v1, zxdg_imported_v1, zxdg_importer_v1,
};

#[derive(Default)]
struct State {
    handle: Arc<Mutex<Option<String>>>,
    destroyed: Arc<Mutex<bool>>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
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

impl Dispatch<wl_compositor::WlCompositor, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zxdg_exporter_v1::ZxdgExporterV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &zxdg_exporter_v1::ZxdgExporterV1,
        _: zxdg_exporter_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zxdg_exported_v1::ZxdgExportedV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &zxdg_exported_v1::ZxdgExportedV1,
        event: zxdg_exported_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_exported_v1::Event::Handle { handle } = event {
            *state.handle.lock().unwrap() = Some(handle);
        }
    }
}

impl Dispatch<zxdg_importer_v1::ZxdgImporterV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &zxdg_importer_v1::ZxdgImporterV1,
        _: zxdg_importer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zxdg_imported_v1::ZxdgImportedV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &zxdg_imported_v1::ZxdgImportedV1,
        event: zxdg_imported_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_imported_v1::Event::Destroyed = event {
            *state.destroyed.lock().unwrap() = true;
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
    let qh = queue.handle();

    let mut state = State::default();
    let exporter: zxdg_exporter_v1::ZxdgExporterV1 = globals.bind(&qh, 1..=1, ())?;
    let importer: zxdg_importer_v1::ZxdgImporterV1 = globals.bind(&qh, 1..=1, ())?;
    let compositor: wl_compositor::WlCompositor = globals.bind(&qh, 4..=4, ())?;

    let exported_surface = compositor.create_surface(&qh, ());
    let child_surface = compositor.create_surface(&qh, ());

    exporter.export(&exported_surface, &qh, ());
    queue.roundtrip(&mut state)?;

    let handle = state
        .handle
        .lock()
        .unwrap()
        .clone()
        .ok_or("kompozytor nie wysłał zxdg_exported_v1.handle")?;
    println!("uchwyt: {handle}");

    let imported = importer.import(handle.clone(), &qh, ());
    imported.set_parent_of(&child_surface);
    queue.roundtrip(&mut state)?;

    if *state.destroyed.lock().unwrap() {
        return Err("kompozytor odrzucił ważny uchwyt (destroyed)".into());
    }
    println!("OK: eksport, import i set_parent_of obsłużone");
    Ok(())
}
