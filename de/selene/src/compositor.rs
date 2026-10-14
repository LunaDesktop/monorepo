//! `wl_compositor`, `wl_surface`, `wl_region`, `wl_callback` — rdzeń powierzchni.

use std::sync::Mutex;

use libluna_core::Rect;
use libluna_render::ImageData;
use wayland_protocols::xdg::shell::server::{xdg_popup, xdg_surface, xdg_toplevel};
use wayland_protocols_wlr::layer_shell::v1::server::zwlr_layer_surface_v1;
use wayland_server::{
    protocol::{wl_buffer, wl_callback, wl_compositor, wl_region, wl_surface},
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

use crate::layer::LayerData;
use crate::shell::{XdgSurfaceData, XdgToplevelData};
use crate::shm::BufferData;
use crate::state::{State, Window};
use crate::sync::LockExt;

/// Dane powierzchni (`wl_surface`).
pub struct SurfaceData {
    /// Oczekujące `frame`-callbacki do wywołania przy następnym `commit`.
    pub frames: Mutex<Vec<wl_callback::WlCallback>>,
    /// Powiązana powierzchnia `xdg_surface` (jeśli klient ją utworzyła).
    pub xdg_surface: Mutex<Option<xdg_surface::XdgSurface>>,
    /// Ostatnio przypięty bufor (`wl_surface.attach`).
    pub buffer: Mutex<Option<wl_buffer::WlBuffer>>,
    /// Slot okna w modelu (przydzielany przy pierwszym commicie).
    pub slot: Mutex<Option<usize>>,
    /// Powiązana powierzchnia warstwowa (`wlr-layer-shell`), jeśli klient ją utworzył.
    pub layer: Mutex<Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>>,
    /// Uchwyty `zwlr_foreign_toplevel` (gdy to okno najwyższego poziomu).
    pub toplevel: Mutex<Option<crate::foreign_toplevel::ToplevelInfo>>,
    /// Region wejścia (`wl_surface.set_input_region`); `None` = cała powierzchnia.
    pub input: Mutex<Option<Vec<Rect>>>,
}

impl SurfaceData {
    pub fn new() -> Self {
        Self {
            frames: Mutex::new(Vec::new()),
            xdg_surface: Mutex::new(None),
            buffer: Mutex::new(None),
            slot: Mutex::new(None),
            layer: Mutex::new(None),
            toplevel: Mutex::new(None),
            input: Mutex::new(None),
        }
    }
}

impl Default for SurfaceData {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalDispatch<wl_compositor::WlCompositor, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<wl_compositor::WlCompositor>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &wl_compositor::WlCompositor,
        request: wl_compositor::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_compositor::Request::CreateSurface { id } => {
                data_init.init(id, SurfaceData::new());
                state.surfaces += 1;
            }
            wl_compositor::Request::CreateRegion { id } => {
                data_init.init(id, RegionRects::default());
            }
            _ => {}
        }
    }
}

/// Prostokąty regionu `wl_region` (współdzielone; klient może je zmieniać).
pub type RegionRects = std::sync::Arc<Mutex<Vec<Rect>>>;

impl Dispatch<wl_region::WlRegion, RegionRects> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &wl_region::WlRegion,
        request: wl_region::Request,
        rects: &RegionRects,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let mut r = rects.lock_or_recover();
        match request {
            wl_region::Request::Add {
                x,
                y,
                width,
                height,
            } => r.push(Rect::from_xywh(
                x as f64,
                y as f64,
                width as f64,
                height as f64,
            )),
            // `subtract` upraszczamy (rzadkie w regionach wejścia).
            wl_region::Request::Subtract { .. } => {}
            wl_region::Request::Destroy => r.clear(),
            _ => {}
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &wl_callback::WlCallback,
        _request: wl_callback::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

/// Górny limit oczekujących frame-callbacków na powierzchnię (ochrona przed OOM).
const MAX_PENDING_FRAMES: usize = 256;

impl Dispatch<wl_surface::WlSurface, SurfaceData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &wl_surface::WlSurface,
        request: wl_surface::Request,
        data: &SurfaceData,
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_surface::Request::Frame { callback } => {
                let cb = data_init.init(callback, ());
                let mut frames = data.frames.lock_or_recover();
                // Limit oczekujących callbacków — klient bez `commit` nie wysypie pamięci.
                if frames.len() >= MAX_PENDING_FRAMES {
                    let _ = cb.send_event(wl_callback::Event::Done {
                        callback_data: state.time_ms(),
                    });
                } else {
                    frames.push(cb);
                }
            }
            wl_surface::Request::Attach { buffer, .. } => {
                *data.buffer.lock_or_recover() = buffer;
            }
            wl_surface::Request::SetInputRegion { region } => {
                // `None` = cała powierzchnia; inaczej migawka prostokątów regionu.
                *data.input.lock_or_recover() = region
                    .as_ref()
                    .and_then(|r| r.data::<RegionRects>().map(|m| m.lock_or_recover().clone()));
            }
            wl_surface::Request::Commit => {
                // Wywołaj oczekujące frame-callbacki.
                let now = state.time_ms();
                for cb in data.frames.lock_or_recover().drain(..) {
                    let _ = cb.send_event(wl_callback::Event::Done { callback_data: now });
                }
                // Pierwszy commit powierzchni z `xdg_surface` → wyślij konfigurację.
                maybe_configure_xdg(state, data);
                // Skomponuj okno (jeśli to toplevel z buforem).
                composite(state, resource, data);
            }
            _ => {}
        }
    }
}

/// Skomponuj powierzchnię do modelu okien (toplevel, popup lub powierzchnia warstwowa).
fn composite(state: &mut State, surface: &wl_surface::WlSurface, data: &SurfaceData) {
    // Powierzchnia warstwowa (panel/dock) — własna ścieżka (bez xdg_surface).
    if let Some(ls) = data.layer.lock_or_recover().clone() {
        composite_layer(state, surface, data, &ls);
        return;
    }
    let Some(xdg_surface) = data.xdg_surface.lock_or_recover().clone() else {
        return;
    };
    let (is_toplevel, is_popup) = match xdg_surface.data::<XdgSurfaceData>() {
        Some(d) => (
            d.toplevel.lock_or_recover().is_some(),
            *d.is_popup.lock_or_recover(),
        ),
        None => (false, false),
    };
    if !is_toplevel && !is_popup {
        return;
    }
    let Some(buffer) = data.buffer.lock_or_recover().clone() else {
        return;
    };
    let Some(buf_data) = buffer.data::<BufferData>() else {
        return;
    };
    let (w, h) = (buf_data.width, buf_data.height);
    let Some(rgba) = buf_data.to_rgba() else {
        return;
    };
    let image = std::sync::Arc::new(ImageData::new(w as u32, h as u32, rgba));
    // Położenie popupu liczymy przed zablokowaniem modelu (odczyt rodzica).
    let popup_xy = if is_popup {
        Some(popup_position(state, &xdg_surface))
    } else {
        None
    };
    let z = state.next_z();
    let title = if is_toplevel {
        xdg_surface
            .data::<XdgSurfaceData>()
            .and_then(|d| {
                d.toplevel.lock_or_recover().as_ref().and_then(|tl| {
                    tl.data::<XdgToplevelData>()
                        .map(|td| td.title.lock_or_recover().clone())
                })
            })
            .unwrap_or_default()
    } else {
        String::new()
    };

    let count = state.windows.lock_or_recover().len();
    let (px, py) = popup_xy.unwrap_or((60.0 + count as f64 * 36.0, 60.0 + count as f64 * 36.0));
    let (_, created) = upsert_window(
        state, surface, data, px, py, w, h, z, &title, is_popup, false, false, image,
    );
    // Popup: wyślij konfigurację (rozmiar + pozycja).
    if is_popup && created {
        send_popup_configure(state, &xdg_surface, popup_xy.unwrap_or((0.0, 0.0)), w, h);
    }
    // Pozwól klientowi ponownie użyć bufora.
    let _ = buffer.send_event(wl_buffer::Event::Release);
}

/// Skomponuj powierzchnię warstwową (`wlr-layer-shell`): konfiguracja + kotwiczenie.
fn composite_layer(
    state: &mut State,
    surface: &wl_surface::WlSurface,
    data: &SurfaceData,
    ls: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
) {
    let Some(ld) = ls.data::<LayerData>() else {
        return;
    };
    let out_idx = ld.output.unwrap_or(0);
    let (ox, oy, out_w, out_h) = output_rect(state, out_idx);
    let buffer = data.buffer.lock_or_recover().clone();
    let debug = std::env::var("SELENE_STATS").is_ok();
    // Pierwszy `commit` (zwykle bez bufora) → wyślij `configure` z rozmiarem.
    if !*ld.configured.lock_or_recover() {
        let st = *ld.state.lock_or_recover();
        let (cw, ch) = st.configure_size(out_w, out_h);
        let serial = state.next_serial();
        let _ = ls.send_event(zwlr_layer_surface_v1::Event::Configure {
            serial,
            width: cw.max(0) as u32,
            height: ch.max(0) as u32,
        });
        *ld.configured.lock_or_recover() = true;
        if debug {
            eprintln!("selene/layer: configure {}x{}", cw.max(0), ch.max(0));
        }
        if buffer.is_none() {
            return;
        }
    }
    let Some(buffer) = buffer else {
        return;
    };
    let Some(buf_data) = buffer.data::<BufferData>() else {
        if debug {
            eprintln!("selene/layer: commit bez BufferData");
        }
        return;
    };
    let (w, h) = (buf_data.width, buf_data.height);
    let Some(rgba) = buf_data.to_rgba() else {
        if debug {
            eprintln!(
                "selene/layer: bufor odrzucony (to_rgba None) {}x{} stride={} offset={}",
                buf_data.width, buf_data.height, buf_data.stride, buf_data.offset
            );
        }
        return;
    };
    let image = std::sync::Arc::new(ImageData::new(w as u32, h as u32, rgba));
    // Region wejścia wyprowadzony z przezroczystości: pasek (nieprzezroczysty) łapie
    // kliki, a przezroczysty obszar chromu przepuszcza je do okien pod spodem.
    let input_rects = opaque_input_rects(&image);
    let st = *ld.state.lock_or_recover();
    let (rx, ry) = st.position(out_w, out_h, w as f64, h as f64);
    let z = crate::layer::layer_z(st.layer);
    let (slot, created) = upsert_window(
        state,
        surface,
        data,
        ox + rx,
        oy + ry,
        w,
        h,
        z,
        "",
        false,
        true,
        true,
        image,
    );
    {
        let mut windows = state.windows.lock_or_recover();
        if let Some(win) = windows.get_mut(slot) {
            // Region klienta ma priorytet; inaczej nasz z alfy.
            if win.input.is_none() {
                win.input = Some(input_rects);
            }
        }
    }
    if debug && created {
        eprintln!(
            "selene/layer: okno utworzone {}x{} @ ({:.0},{:.0}), z={}",
            w,
            h,
            ox + rx,
            oy + ry,
            z
        );
    }
    let _ = buffer.send_event(wl_buffer::Event::Release);
}

/// Region wejścia z kanału alfa: nieprzezroczyste bloki scalane poziomo w prostokąty.
fn opaque_input_rects(image: &ImageData) -> Vec<Rect> {
    const BLOCK: usize = 16;
    const ALPHA_THRESHOLD: u8 = 8;
    let iw = image.width as usize;
    let ih = image.height as usize;
    if iw == 0 || ih == 0 || image.rgba.len() < iw * ih * 4 {
        return Vec::new();
    }
    let bw = iw.div_ceil(BLOCK);
    let bh = ih.div_ceil(BLOCK);
    let mut rects = Vec::new();
    for by in 0..bh {
        let y0 = by * BLOCK;
        let y1 = (y0 + BLOCK).min(ih);
        let mut run: Option<usize> = None;
        for bx in 0..bw {
            let x0 = bx * BLOCK;
            let x1 = (x0 + BLOCK).min(iw);
            let mut opaque = false;
            'scan: for y in y0..y1 {
                let row = y * iw * 4;
                for x in x0..x1 {
                    if image.rgba[row + x * 4 + 3] > ALPHA_THRESHOLD {
                        opaque = true;
                        break 'scan;
                    }
                }
            }
            match (run, opaque) {
                (None, true) => run = Some(bx),
                (Some(s), false) => {
                    rects.push(block_run(s, bx, by, BLOCK, iw, ih));
                    run = None;
                }
                _ => {}
            }
        }
        if let Some(s) = run {
            rects.push(block_run(s, bw, by, BLOCK, iw, ih));
        }
    }
    rects
}

/// Prostokąt dla ciągu bloków `[bx0, bx1)` w rzędzie `by`.
fn block_run(bx0: usize, bx1: usize, by: usize, block: usize, iw: usize, ih: usize) -> Rect {
    let x = bx0 * block;
    let y = by * block;
    let w = (bx1 * block).min(iw).saturating_sub(x);
    let h = (y + block).min(ih) - y;
    Rect::from_xywh(x as f64, y as f64, w as f64, h as f64)
}

#[cfg(test)]
mod input_region_tests {
    use super::*;

    #[test]
    fn opaque_rects_track_alpha() {
        // 32×32, nieprzezroczysta tylko lewa połowa (blok 16 px).
        let mut rgba = vec![0u8; 32 * 32 * 4];
        for y in 0..32 {
            for x in 0..16 {
                rgba[(y * 32 + x) * 4 + 3] = 255;
            }
        }
        let rects = opaque_input_rects(&ImageData::new(32, 32, rgba));
        assert!(!rects.is_empty());
        assert!(rects.iter().all(|r| r.x() < 16.0));
        assert!(rects.iter().all(|r| r.x() + r.width() <= 16.001));
    }

    #[test]
    fn fully_transparent_yields_no_rects() {
        let rects = opaque_input_rects(&ImageData::new(40, 40, vec![0u8; 40 * 40 * 4]));
        assert!(rects.is_empty());
    }
}

/// Dodaje nowe okno lub aktualizuje istniejące (slot trzymany w [`SurfaceData`]).
#[allow(clippy::too_many_arguments)]
fn upsert_window(
    state: &mut State,
    surface: &wl_surface::WlSurface,
    data: &SurfaceData,
    x: f64,
    y: f64,
    w: i32,
    h: i32,
    z: i32,
    title: &str,
    popup: bool,
    layer: bool,
    reposition: bool,
    image: std::sync::Arc<ImageData>,
) -> (usize, bool) {
    let mut slot = data.slot.lock_or_recover();
    let ws = *state.ws_current.lock_or_recover();
    let input = data.input.lock_or_recover().clone();
    let mut windows = state.windows.lock_or_recover();
    let mut created = false;
    let idx = match *slot {
        Some(i) => {
            if let Some(win) = windows.get_mut(i) {
                // Pozycję stałą okna zmieniamy tylko dla powierzchni kotwiczonych.
                if reposition {
                    win.x = x;
                    win.y = y;
                    win.z = z;
                }
                win.width = w;
                win.height = h;
                win.mapped = true;
                win.popup = popup;
                win.layer = layer;
                if !title.is_empty() {
                    win.title = title.to_owned();
                }
                win.input = input.clone();
                win.data = image;
            }
            i
        }
        None => {
            let i = windows.len();
            windows.push(Window {
                x,
                y,
                width: w,
                height: h,
                z,
                title: title.to_owned(),
                popup,
                layer,
                mapped: true,
                workspace: ws,
                input,
                minimized: false,
                maximized: false,
                fullscreen: false,
                saved: None,
                data: image,
            });
            *slot = Some(i);
            created = true;
            i
        }
    };
    drop(windows);
    drop(slot);
    // Zarejestruj powierzchnię okna (indeks = slot) do trasowania wejścia.
    if created && state.slots.len() == idx {
        state.slots.push(surface.clone());
    }
    (idx, created)
}

/// Geometria monitora `idx` (x, y, szerokość, wysokość); `idx` poza zakresem → pierwszy.
fn output_rect(state: &State, idx: usize) -> (f64, f64, f64, f64) {
    let outs = state.outputs.lock_or_recover();
    let o = outs.get(idx).or_else(|| outs.first());
    match o {
        Some(o) => (o.x as f64, o.y as f64, o.width as f64, o.height as f64),
        None => (0.0, 0.0, 1280.0, 800.0),
    }
}

/// Pozycja popupu względem treści rodzica (prostokąt kotwicy + offset).
fn popup_position(state: &State, xdg_surface: &xdg_surface::XdgSurface) -> (f64, f64) {
    let Some(d) = xdg_surface.data::<XdgSurfaceData>() else {
        return (0.0, 0.0);
    };
    let geom = *d.geom.lock_or_recover();
    let parent_origin = d
        .parent
        .lock_or_recover()
        .clone()
        .and_then(|ps| {
            ps.data::<SurfaceData>()
                .map(|sd| *sd.slot.lock_or_recover())
        })
        .flatten()
        .and_then(|slot| {
            state.windows.lock_or_recover().get(slot).map(|w| {
                let c = w.content();
                (c.x(), c.y())
            })
        })
        .unwrap_or((0.0, 0.0));
    (
        parent_origin.0 + geom.anchor_x + geom.offset_x,
        parent_origin.1 + geom.anchor_y + geom.anchor_h + geom.offset_y,
    )
}

/// Wysyła `xdg_popup.configure` + `xdg_surface.configure` (raz).
fn send_popup_configure(
    state: &mut State,
    xdg_surface: &xdg_surface::XdgSurface,
    (x, y): (f64, f64),
    w: i32,
    h: i32,
) {
    let serial = state.next_serial();
    if let Some(d) = xdg_surface.data::<XdgSurfaceData>() {
        if let Some(popup) = d.popup.lock_or_recover().as_ref() {
            let _ = popup.send_event(xdg_popup::Event::Configure {
                x: x as i32,
                y: y as i32,
                width: w,
                height: h,
            });
        }
        if !*d.configured.lock_or_recover() {
            let _ = xdg_surface.send_event(xdg_surface::Event::Configure { serial });
            *d.configured.lock_or_recover() = true;
        }
    }
}

/// Jeśli toplevel nie ma jeszcze konfiguracji, wyślij `configure` (popupy mają własną).
fn maybe_configure_xdg(state: &mut State, data: &SurfaceData) {
    let Some(xdg_surface) = data.xdg_surface.lock_or_recover().clone() else {
        return;
    };
    let Some(xdg_data) = xdg_surface.data::<XdgSurfaceData>() else {
        return;
    };
    if *xdg_data.is_popup.lock_or_recover() {
        return;
    }
    if *xdg_data.configured.lock_or_recover() {
        return;
    }
    let serial = state.next_serial();
    let _ = xdg_surface.send_event(xdg_surface::Event::Configure { serial });
    if let Some(toplevel) = xdg_data.toplevel.lock_or_recover().as_ref() {
        let _ = toplevel.send_event(xdg_toplevel::Event::Configure {
            width: 1024,
            height: 720,
            states: vec![4], // ACTIVATED
        });
        let _ = toplevel.send_event(xdg_toplevel::Event::WmCapabilities {
            capabilities: vec![],
        });
    }
    *xdg_data.configured.lock_or_recover() = true;
}
