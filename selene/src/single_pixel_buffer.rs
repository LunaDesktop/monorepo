//! `wp_single_pixel_buffer_manager_v1` — bufory 1×1 znanej wartości.
//!
//! GTK4 i Chromium używają ich do jednolitych teł, świecących pixeli i masek:
//! zamiast wysyłać pełny bufor shm tworzą jeden pixel i skalują go viewporterem.
//! Bez tego globalu aplikacje działają, ale marnują pamięć i pasmo.

use std::io::Write;
use std::sync::Arc;

use wayland_protocols::wp::single_pixel_buffer::v1::server::wp_single_pixel_buffer_manager_v1 as manager;
use wayland_server::protocol::wl_shm;
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New};

use crate::shm::{BufferData, ShmPoolData};
use crate::state::State;

impl GlobalDispatch<manager::WpSinglePixelBufferManagerV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<manager::WpSinglePixelBufferManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<manager::WpSinglePixelBufferManagerV1, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &manager::WpSinglePixelBufferManagerV1,
        request: manager::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let manager::Request::CreateU32RgbaBuffer { id, r, g, b, a } = request {
            let Some(pool) = one_pixel_pool(r, g, b, a) else {
                return;
            };
            // `wl_shm` zawsze jest ARGB8888 little-endian: bajty w kolejności B, G, R, A.
            data_init.init(
                id,
                BufferData {
                    pool: Some(pool),
                    offset: 0,
                    width: 1,
                    height: 1,
                    stride: 4,
                    format: wl_shm::Format::Argb8888,
                    dmabuf: None,
                },
            );
        }
    }
}

/// Tworzy 4-bajtową pulę shm z jednym pikselem.
///
/// Wartości kanałów są niesprecyzowane (klient może podać > 255) — przycinamy do
/// 8 bitów, bo taki jest format `ARGB8888`.
fn one_pixel_pool(r: u32, g: u32, b: u32, a: u32) -> Option<Arc<ShmPoolData>> {
    let fd = rustix::fs::memfd_create(c"selene-single-pixel", rustix::fs::MemfdFlags::CLOEXEC).ok()?;
    let bytes = [
        (b & 0xff) as u8,
        (g & 0xff) as u8,
        (r & 0xff) as u8,
        (a & 0xff) as u8,
    ];
    let mut file = std::fs::File::from(fd.try_clone().ok()?);
    file.write_all(&bytes).ok()?;
    Some(Arc::new(ShmPoolData::new(fd, bytes.len())))
}
