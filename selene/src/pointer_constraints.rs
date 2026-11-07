//! `zwp_pointer_constraints_v1` — blokada i zamknięcie kursora w regionie.
//!
//! Gry i aplikacje 3D potrzebują dwóch rzeczy, których nie daje sam
//! `zwp_relative_pointer_v1`:
//!
//! - **lock** — kursor zostaje w miejscu, a klient dostaje tylko przesunięcia
//!   względne (mouselook / obracanie kamery),
//! - **confine** — kursor nie wychodzi poza region powierzchni (menu, mapy,
//!   przeciąganie w edytorze).
//!
//! Ograniczenie jest aktywne tylko wtedy, gdy powierzchnia ma fokus wskaźnika.
//! Po zmianie fokusu compositor wysyła `unlocked`/`unconfined`, więc klient
//! nie zostaje z „zamrożonym” kursorem. Ograniczenia `oneshot` kończą się po
//! pierwszym wciśnięciu przycisku (tak jak w innych kompozytorach).

use std::sync::Mutex;

use libluna_core::Rect;
use wayland_protocols::wp::pointer_constraints::zv1::server::{
    zwp_confined_pointer_v1::{
        Event as ConfinedEvent, Request as ConfinedRequest, ZwpConfinedPointerV1,
    },
    zwp_locked_pointer_v1::{Event as LockedEvent, Request as LockedRequest, ZwpLockedPointerV1},
    zwp_pointer_constraints_v1::{self as constraints, ZwpPointerConstraintsV1},
};
use wayland_server::{
    protocol::wl_surface::WlSurface, Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch,
    New, Resource, WEnum,
};

use crate::compositor::{RegionRects, SurfaceData};
use crate::state::State;
use crate::sync::LockExt;

/// Ograniczenie utworzone przez klienta.
pub struct PointerConstraint {
    pub kind: ConstraintKind,
    pub slot: usize,
    /// Region w współrzędnych powierzchni (`None` = cała treść okna).
    pub region: Mutex<Option<Vec<Rect>>>,
    /// `true` dla `lifetime = oneshot` — dezaktywuje się po pierwszym przycisku.
    pub oneshot: bool,
    /// Czy compositor wysłał już `locked`/`confined`.
    pub active: bool,
    /// `oneshot` już zużyty — nie aktywujemy go ponownie.
    pub dismissed: bool,
}

#[derive(Clone)]
pub enum ConstraintKind {
    Locked(ZwpLockedPointerV1),
    Confined(ZwpConfinedPointerV1),
}

impl ConstraintKind {
    fn resource_id(&self) -> wayland_server::backend::ObjectId {
        match self {
            ConstraintKind::Locked(r) => r.id(),
            ConstraintKind::Confined(r) => r.id(),
        }
    }
}

impl GlobalDispatch<ZwpPointerConstraintsV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpPointerConstraintsV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwpPointerConstraintsV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ZwpPointerConstraintsV1,
        request: constraints::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            constraints::Request::LockPointer {
                id,
                surface,
                pointer: _,
                region,
                lifetime,
            } => {
                let Some(slot) = slot_of(&surface) else {
                    return;
                };
                let resource = data_init.init(id, ());
                add(
                    state,
                    ConstraintKind::Locked(resource),
                    surface,
                    slot,
                    region,
                    lifetime_is_oneshot(lifetime),
                );
            }
            constraints::Request::ConfinePointer {
                id,
                surface,
                pointer: _,
                region,
                lifetime,
            } => {
                let Some(slot) = slot_of(&surface) else {
                    return;
                };
                let resource = data_init.init(id, ());
                add(
                    state,
                    ConstraintKind::Confined(resource),
                    surface,
                    slot,
                    region,
                    lifetime_is_oneshot(lifetime),
                );
            }
            constraints::Request::Destroy => {}
            _ => {}
        }
    }
}

fn lifetime_is_oneshot(lifetime: WEnum<constraints::Lifetime>) -> bool {
    matches!(lifetime, WEnum::Value(constraints::Lifetime::Oneshot))
}

fn slot_of(surface: &WlSurface) -> Option<usize> {
    let data = surface.data::<SurfaceData>()?;
    *data.slot.lock_or_recover()
}

fn add(
    state: &mut State,
    kind: ConstraintKind,
    _surface: WlSurface,
    slot: usize,
    region: Option<wayland_server::protocol::wl_region::WlRegion>,
    oneshot: bool,
) {
    let rects = region.and_then(|r| r.data::<RegionRects>().map(|r| r.lock_or_recover().clone()));
    state.pointer_constraints.retain(|c| c.kind_alive());
    state.pointer_constraints.push(PointerConstraint {
        kind,
        slot,
        region: Mutex::new(rects),
        oneshot,
        active: false,
        dismissed: false,
    });
    sync(state);
}

impl PointerConstraint {
    fn kind_alive(&self) -> bool {
        match &self.kind {
            ConstraintKind::Locked(r) => r.is_alive(),
            ConstraintKind::Confined(r) => r.is_alive(),
        }
    }

    fn id(&self) -> wayland_server::backend::ObjectId {
        self.kind.resource_id()
    }

    fn set_active(&mut self, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        match &self.kind {
            ConstraintKind::Locked(r) => {
                let _ = r.send_event(if active {
                    LockedEvent::Locked
                } else {
                    LockedEvent::Unlocked
                });
            }
            ConstraintKind::Confined(r) => {
                let _ = r.send_event(if active {
                    ConfinedEvent::Confined
                } else {
                    ConfinedEvent::Unconfined
                });
            }
        }
    }
}

impl Dispatch<ZwpLockedPointerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwpLockedPointerV1,
        request: LockedRequest,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            LockedRequest::SetCursorPositionHint { .. } => {
                // Wayland nie pozwala „teleportować” kursora — to tylko podpowiedź
                // dla kompozytora, gdzie ustawić kursor po zwolnieniu blokady.
            }
            LockedRequest::SetRegion { region } => {
                if let Some(c) = find_mut(state, resource.id()) {
                    *c.region.lock_or_recover() = region
                        .and_then(|r| r.data::<RegionRects>().map(|r| r.lock_or_recover().clone()));
                }
            }
            LockedRequest::Destroy => remove(state, resource.id()),
            _ => {}
        }
    }
}

impl Dispatch<ZwpConfinedPointerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwpConfinedPointerV1,
        request: ConfinedRequest,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            ConfinedRequest::SetRegion { region } => {
                if let Some(c) = find_mut(state, resource.id()) {
                    *c.region.lock_or_recover() = region
                        .and_then(|r| r.data::<RegionRects>().map(|r| r.lock_or_recover().clone()));
                }
            }
            ConfinedRequest::Destroy => remove(state, resource.id()),
            _ => {}
        }
    }
}

fn find_mut(
    state: &mut State,
    id: wayland_server::backend::ObjectId,
) -> Option<&mut PointerConstraint> {
    state.pointer_constraints.iter_mut().find(|c| c.id() == id)
}

fn remove(state: &mut State, id: wayland_server::backend::ObjectId) {
    if let Some(index) = state.pointer_constraints.iter().position(|c| c.id() == id) {
        let mut c = state.pointer_constraints.remove(index);
        c.set_active(false);
    }
}

/// Aktywna blokada (jeśli jest) dla bieżącego fokusu wskaźnika.
pub fn active_locked(state: &State) -> bool {
    active_index(state)
        .is_some_and(|i| matches!(state.pointer_constraints[i].kind, ConstraintKind::Locked(_)))
}

/// Czy kursor jest zamknięty w regionie.
pub fn active_confined(state: &State) -> bool {
    active_index(state).is_some_and(|i| {
        matches!(
            state.pointer_constraints[i].kind,
            ConstraintKind::Confined(_)
        )
    })
}

fn active_index(state: &State) -> Option<usize> {
    state
        .pointer_constraints
        .iter()
        .position(|c| c.active && c.kind_alive())
}

/// Uzgadnia stan ograniczeń z bieżącym fokusem wskaźnika.
pub fn sync(state: &mut State) {
    state.pointer_constraints.retain(|c| c.kind_alive());
    let focused = state.pointer_focus.as_ref().map(|(slot, _)| *slot);
    let wanted = focused.and_then(|slot| {
        state
            .pointer_constraints
            .iter()
            .position(|c| !c.dismissed && c.kind_alive() && c.slot == slot)
    });
    let current = state.pointer_constraints.iter().position(|c| c.active);
    if current == wanted {
        return;
    }
    if let Some(index) = current {
        state.pointer_constraints[index].set_active(false);
    }
    if let Some(index) = wanted {
        state.pointer_constraints[index].set_active(true);
    }
}

/// Filtr pozycji kursora: blokada zatrzymuje go w miejscu, zamknięcie przycina
/// do regionu powierzchni. Zwraca pozycję, którą naprawdę widzi klient.
pub fn filter_motion(state: &mut State, pos: libluna_core::Point) -> libluna_core::Point {
    sync(state);
    let Some(index) = active_index(state) else {
        return pos;
    };
    match &state.pointer_constraints[index].kind {
        ConstraintKind::Locked(_) => {
            // Kursor nie może się ruszyć; klient dostaje `relative_motion`.
            let (x, y) = state.pointer_pos;
            libluna_core::Point::new(x, y)
        }
        ConstraintKind::Confined(_) => {
            let c = &state.pointer_constraints[index];
            let bounds = region_bounds(state, c);
            match bounds {
                Some(rect) => libluna_core::Point::new(
                    pos.x.clamp(rect.x(), rect.right()),
                    pos.y.clamp(rect.y(), rect.bottom()),
                ),
                None => pos,
            }
        }
    }
}

/// Zamienia region powierzchni na prostokąt globalny (unia prostokątów).
fn region_bounds(state: &State, c: &PointerConstraint) -> Option<Rect> {
    let (ox, oy) = surface_origin(state, c.slot)?;
    let local = c.region.lock_or_recover().clone();
    match local {
        Some(rects) if !rects.is_empty() => {
            let mut bounds = rects[0];
            for r in &rects[1..] {
                let x0 = bounds.x().min(r.x());
                let y0 = bounds.y().min(r.y());
                let x1 = bounds.right().max(r.right());
                let y1 = bounds.bottom().max(r.bottom());
                bounds = Rect::from_xywh(x0, y0, x1 - x0, y1 - y0);
            }
            Some(Rect::from_xywh(
                bounds.x() + ox,
                bounds.y() + oy,
                bounds.width(),
                bounds.height(),
            ))
        }
        _ => {
            let windows = state.windows.lock_or_recover();
            let w = windows.get(c.slot)?;
            Some(w.content())
        }
    }
}

/// Pozycja (0,0) współrzędnych powierzchni w układzie globalnym.
fn surface_origin(state: &State, slot: usize) -> Option<(f64, f64)> {
    let windows = state.windows.lock_or_recover();
    let w = windows.get(slot)?;
    let c = w.content();
    Some((
        c.x() - f64::from(w.image_offset.0),
        c.y() - f64::from(w.image_offset.1),
    ))
}

/// Wciśnięcie przycisku kończy ograniczenia `oneshot`.
pub fn button_pressed(state: &mut State) {
    for c in state.pointer_constraints.iter_mut() {
        if c.active && c.oneshot {
            c.dismissed = true;
            c.set_active(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_core::Point;

    #[test]
    fn no_constraint_is_inactive() {
        let state = State::default();
        assert!(!active_locked(&state));
        assert!(!active_confined(&state));
    }

    #[test]
    fn clamps_into_bounds() {
        let rect = Rect::from_xywh(10.0, 20.0, 100.0, 50.0);
        let p = Point::new(200.0, 0.0);
        let clamped = Point::new(
            p.x.clamp(rect.x(), rect.right()),
            p.y.clamp(rect.y(), rect.bottom()),
        );
        assert_eq!((clamped.x, clamped.y), (110.0, 20.0));
    }
}
