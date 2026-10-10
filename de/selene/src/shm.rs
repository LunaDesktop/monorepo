//! `wl_shm` — współdzielona pamięć klienta (bufory ARGB8888).

use wayland_server::{
    protocol::{wl_buffer, wl_shm, wl_shm_pool},
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

use crate::state::State;

/// Dane puli pamięci (`wl_shm_pool`).
#[allow(dead_code)] // pola wykorzystamy przy komponowaniu buforów
pub struct ShmPoolData {
    pub size: usize,
}

/// Dane bufora (`wl_buffer`).
#[allow(dead_code)] // metadane potrzebne do komponowania
pub struct BufferData {
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub offset: i32,
    pub format: wl_shm::Format,
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
        if let wl_shm::Request::CreatePool { id, fd: _, size } = request {
            data_init.init(
                id,
                ShmPoolData {
                    size: size.max(0) as usize,
                },
            );
        }
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ShmPoolData> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &wl_shm_pool::WlShmPool,
        request: wl_shm_pool::Request,
        _data: &ShmPoolData,
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
                    width,
                    height,
                    stride,
                    offset,
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
