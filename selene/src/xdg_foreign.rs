//! `xdg_foreign_unstable_v1` / `v2` — uchwyty powierzchni między klientami.
//!
//! Klient (np. aplikacja GTK) eksportuje swoje okno i przekazuje uchwyt przez
//! D-Bus do portalu. Backend portalu importuje ten uchwyt i ustawia importowaną
//! powierzchnię jako rodzica swojego dialogu (`zxdg_imported_v1.set_parent_of`).
//!
//! Bez tego protokołu `xdg-desktop-portal-gtk` loguje „Unhandled parent window
//! type”, a okna wyboru pliku nie są powiązane z aplikacją (brak modalności,
//! pojawiają się w losowym miejscu). Protokół jest też używany przez Flatpaki
//! do osadzania okien innych klientów.
//!
//! Implementujemy obie wersje, bo GTK3 używa v1, a GTK4/Firefox — v2. Rejestr
//! uchwytów jest wspólny, więc uchwyt wyeksportowany przez v1 można zaimportować
//! przez v2 (i odwrotnie), co upraszcza zgodność między wersjami klientów.

use std::collections::HashMap;

use wayland_protocols::xdg::foreign::zv1::server::{
    zxdg_exported_v1 as exported1, zxdg_exporter_v1 as exporter1, zxdg_imported_v1 as imported1,
    zxdg_importer_v1 as importer1,
};
use wayland_protocols::xdg::foreign::zv2::server::{
    zxdg_exported_v2 as exported2, zxdg_exporter_v2 as exporter2, zxdg_imported_v2 as imported2,
    zxdg_importer_v2 as importer2,
};
use wayland_server::protocol::wl_surface;
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};

use crate::state::State;
use crate::sync::LockExt;

/// Globalne interfejsy (typy wygodne dla `main`).
pub type ExporterV1 = exporter1::ZxdgExporterV1;
pub type ImporterV1 = importer1::ZxdgImporterV1;
pub type ExporterV2 = exporter2::ZxdgExporterV2;
pub type ImporterV2 = importer2::ZxdgImporterV2;

/// Rejestr aktywnych uchwytów. Trzymany w [`State`], więc ginie razem z sesją
/// (żadnych resztek między uruchomieniami kompozytora).
#[derive(Default)]
pub struct Handles {
    next: u64,
    by_handle: HashMap<String, wl_surface::WlSurface>,
}

impl Handles {
    /// Eksportuje powierzchnię i zwraca unikalny, nieprzezroczysty uchwyt.
    fn export(&mut self, surface: &wl_surface::WlSurface) -> String {
        // Sprzątamy uchwyty po powierzchniach, które już zniknęły (klient mógł
        // zginąć bez `zxdg_exported_v1.destroy`).
        self.by_handle.retain(|_, s| s.is_alive());
        self.next = self.next.wrapping_add(1);
        let handle = format!("luna-{:x}", self.next);
        self.by_handle.insert(handle.clone(), surface.clone());
        handle
    }

    fn resolve(&self, handle: &str) -> Option<wl_surface::WlSurface> {
        self.by_handle
            .get(handle)
            .filter(|surface| surface.is_alive())
            .cloned()
    }

    fn revoke(&mut self, handle: &str) {
        self.by_handle.remove(handle);
    }
}

/// Dane `zxdg_exported_v{1,2}` — uchwyt do unieważnienia przy `destroy`.
pub struct ExportedData {
    handle: String,
}

/// Dane `zxdg_imported_v{1,2}` — powierzchnia rodzica (jeśli uchwyt był ważny).
pub struct ImportedData {
    parent: Option<wl_surface::WlSurface>,
}

/// Ustawia relację rodzic→dziecko: zapisuje rodzica i (gdy oba okna już istnieją
/// w modelu) centruje dialog nad oknem aplikacji oraz podnosi go.
fn apply_parent(state: &State, parent: &wl_surface::WlSurface, child: &wl_surface::WlSurface) {
    // `XdgSurfaceData.parent` jest czytane przez kompozytor m.in. przy układaniu
    // popupów, a od teraz także przy mapowaniu okna-dziecka.
    if let Some(xdg) = child.data::<crate::shell::XdgSurfaceData>() {
        *xdg.parent.lock_or_recover() = Some(parent.clone());
    }

    let Some(child_slot) = child
        .data::<crate::compositor::SurfaceData>()
        .and_then(|d| *d.slot.lock_or_recover())
    else {
        return;
    };
    center_on_parent(state, parent, child_slot);
}

/// Centruje okno-dziecko nad rodzicem i podnosi je ponad nie.
///
/// Wywoływane także przy mapowaniu: portal potrafi ustawić rodzica, zanim okno
/// dialogu dostanie bufor, więc samo `set_parent_of` nie wystarcza.
pub(crate) fn center_on_parent(state: &State, parent: &wl_surface::WlSurface, child_slot: usize) {
    let Some(parent_slot) = parent
        .data::<crate::compositor::SurfaceData>()
        .and_then(|d| *d.slot.lock_or_recover())
    else {
        return;
    };

    let mut windows = state.windows.lock_or_recover();
    let Some((px, py, pw, ph, pz)) = windows
        .get(parent_slot)
        .map(|p| (p.x, p.y, f64::from(p.width), f64::from(p.height), p.z))
    else {
        return;
    };
    let Some(child_win) = windows.get_mut(child_slot) else {
        return;
    };
    if child_win.popup || child_win.layer {
        return;
    }
    child_win.x = px + (pw - f64::from(child_win.width)) / 2.0;
    child_win.y = py + (ph - f64::from(child_win.height)) / 2.0;
    child_win.z = pz + 1;
}

/// Rodzic ustawiony przez `zxdg_imported_v*.set_parent_of` (jeśli jest).
pub(crate) fn parent_surface(child: &wl_surface::WlSurface) -> Option<wl_surface::WlSurface> {
    child
        .data::<crate::shell::XdgSurfaceData>()
        .and_then(|d| d.parent.lock_or_recover().clone())
}

// ——— v1: eksporter ———

impl GlobalDispatch<ExporterV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ExporterV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ExporterV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ExporterV1,
        request: exporter1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            exporter1::Request::Export { id, surface } => {
                let handle = state.xdg_foreign.lock_or_recover().export(&surface);
                let exported = data_init.init(
                    id,
                    ExportedData {
                        handle: handle.clone(),
                    },
                );
                let _ = exported.send_event(exported1::Event::Handle { handle });
            }
            exporter1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<exported1::ZxdgExportedV1, ExportedData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &exported1::ZxdgExportedV1,
        request: exported1::Request,
        data: &ExportedData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let exported1::Request::Destroy = request {
            state.xdg_foreign.lock_or_recover().revoke(&data.handle);
        }
    }
}

// ——— v1: importer ———

impl GlobalDispatch<ImporterV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ImporterV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ImporterV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ImporterV1,
        request: importer1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            importer1::Request::Import { id, handle } => {
                let parent = state.xdg_foreign.lock_or_recover().resolve(&handle);
                let imported = data_init.init(
                    id,
                    ImportedData {
                        parent: parent.clone(),
                    },
                );
                if parent.is_none() {
                    let _ = imported.send_event(imported1::Event::Destroyed);
                }
            }
            importer1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<imported1::ZxdgImportedV1, ImportedData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &imported1::ZxdgImportedV1,
        request: imported1::Request,
        data: &ImportedData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let imported1::Request::SetParentOf { surface } = request {
            if let Some(parent) = &data.parent {
                apply_parent(state, parent, &surface);
            }
        }
    }
}

// ——— v2: eksporter ———

impl GlobalDispatch<ExporterV2, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ExporterV2>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ExporterV2, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ExporterV2,
        request: exporter2::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            exporter2::Request::ExportToplevel { id, surface } => {
                let handle = state.xdg_foreign.lock_or_recover().export(&surface);
                let exported = data_init.init(
                    id,
                    ExportedData {
                        handle: handle.clone(),
                    },
                );
                let _ = exported.send_event(exported2::Event::Handle { handle });
            }
            exporter2::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<exported2::ZxdgExportedV2, ExportedData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &exported2::ZxdgExportedV2,
        request: exported2::Request,
        data: &ExportedData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let exported2::Request::Destroy = request {
            state.xdg_foreign.lock_or_recover().revoke(&data.handle);
        }
    }
}

// ——— v2: importer ———

impl GlobalDispatch<ImporterV2, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ImporterV2>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ImporterV2, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ImporterV2,
        request: importer2::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            importer2::Request::ImportToplevel { id, handle } => {
                let parent = state.xdg_foreign.lock_or_recover().resolve(&handle);
                let imported = data_init.init(
                    id,
                    ImportedData {
                        parent: parent.clone(),
                    },
                );
                if parent.is_none() {
                    let _ = imported.send_event(imported2::Event::Destroyed);
                }
            }
            importer2::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<imported2::ZxdgImportedV2, ImportedData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &imported2::ZxdgImportedV2,
        request: imported2::Request,
        data: &ImportedData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let imported2::Request::SetParentOf { surface } = request {
            if let Some(parent) = &data.parent {
                apply_parent(state, parent, &surface);
            }
        }
    }
}

/// Unieważnia wszystkie uchwyty powierzchni, która została zniszczona.
///
/// Wywoływane z pętli niszczenia `wl_surface`: klient nie zawsze zamyka
/// `zxdg_exported_v1` przed zniszczeniem powierzchni.
pub(crate) fn forget_surface(state: &State, surface: &wl_surface::WlSurface) {
    let mut handles = state.xdg_foreign.lock_or_recover();
    let dead: Vec<String> = handles
        .by_handle
        .iter()
        .filter(|(_, s)| !s.is_alive() || s.id() == surface.id())
        .map(|(handle, _)| handle.clone())
        .collect();
    for handle in dead {
        handles.by_handle.remove(&handle);
    }
}
