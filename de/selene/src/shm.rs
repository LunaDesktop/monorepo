//! `wl_shm` — współdzielona pamięć klienta (bufory ARGB8888 → RGBA).

use std::ffi::c_void;
use std::os::fd::OwnedFd;
use std::sync::Arc;

use wayland_server::{
    protocol::{wl_buffer, wl_shm, wl_shm_pool},
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

use crate::state::State;

/// Dane puli pamięci (`wl_shm_pool`): deskryptor + rozmiar. Współdzielone przez bufory.
pub struct ShmPoolData {
    pub fd: OwnedFd,
    pub size: usize,
}

/// Dane bufora (`wl_buffer`): odniesienie do puli + geometria.
pub struct BufferData {
    pub pool: Arc<ShmPoolData>,
    pub offset: usize,
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub format: wl_shm::Format,
}

impl BufferData {
    /// Odczytuje piksele bufora jako premultiplied RGBA8 (swizzle BGRA→RGBA).
    ///
    /// Mapujemy pulę `wl_shm` tylko na czas odczytu (MVP; docelowo mapa cache'owana).
    pub fn to_rgba(&self) -> Option<Vec<u8>> {
        if self.width <= 0 || self.height <= 0 || self.stride <= 0 {
            return None;
        }
        let len = self.pool.size;
        // SAFETY: mapujemy tylko do odczytu, na czas wywołania; `fd` żyje w puli.
        let map = unsafe {
            rustix::mm::mmap(
                std::ptr::null_mut::<c_void>(),
                len,
                rustix::mm::ProtFlags::READ,
                rustix::mm::MapFlags::PRIVATE,
                &self.pool.fd,
                0,
            )
        }
        .ok()?;
        let bytes = unsafe { std::slice::from_raw_parts(map as *const u8, len) };

        let (w, h) = (self.width as usize, self.height as usize);
        let stride = self.stride as usize;
        let mut out = vec![0u8; w * h * 4];
        for y in 0..h {
            let row = self.offset + y * stride;
            for x in 0..w {
                let i = row + x * 4;
                if i + 4 > bytes.len() {
                    break;
                }
                let o = (y * w + x) * 4;
                // ARGB8888 (pamięć: B,G,R,A) → RGBA8; XRGB8888 → alfa wymuszona na 1.
                out[o] = bytes[i + 2];
                out[o + 1] = bytes[i + 1];
                out[o + 2] = bytes[i];
                out[o + 3] = if self.format == wl_shm::Format::Xrgb8888 {
                    255
                } else {
                    bytes[i + 3]
                };
            }
        }
        // SAFETY: odmapowujemy dokładnie ten sam zakres.
        unsafe {
            let _ = rustix::mm::munmap(map, len);
        }
        Some(out)
    }
}

impl GlobalDispatch<wl_shm::WlShm, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<wl_shm::WlShm>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        let shm = data_init.init(resource, ());
        // Zadeklaruj obsługiwany format (klienci muszą znać choć jeden).
        let _ = shm.send_event(wl_shm::Event::Format {
            format: wayland_server::WEnum::Value(wl_shm::Format::Argb8888),
        });
    }
}

impl Dispatch<wl_shm::WlShm, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &wl_shm::WlShm,
        request: wl_shm::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let wl_shm::Request::CreatePool { id, fd, size } = request {
            data_init.init(
                id,
                Arc::new(ShmPoolData {
                    fd,
                    size: size.max(0) as usize,
                }),
            );
        }
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, Arc<ShmPoolData>> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &wl_shm_pool::WlShmPool,
        request: wl_shm_pool::Request,
        data: &Arc<ShmPoolData>,
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let wl_shm_pool::Request::CreateBuffer {
            id,
            offset,
            width,
            height,
            stride,
            format,
        } = request
        {
            data_init.init(
                id,
                BufferData {
                    pool: Arc::clone(data),
                    offset: offset.max(0) as usize,
                    width,
                    height,
                    stride,
                    format: format.into_result().unwrap_or(wl_shm::Format::Argb8888),
                },
            );
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, BufferData> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &wl_buffer::WlBuffer,
        _request: wl_buffer::Request,
        _data: &BufferData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}
