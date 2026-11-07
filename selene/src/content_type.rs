//! `wp_content_type_v1` — klient opisuje rodzaj treści powierzchni.
//!
//! Chrome i Firefox zgłaszają tu `video`, zanim zaczną odtwarzać — kompozytor może
//! wtedy np. dobrać inny tryb skanoutu albo wyłączyć optymalizacje psujące obraz.
//! Protokół nie ma zdarzeń: przyjmujemy ustawienie i zapisujemy je w danych
//! powierzchni, żeby klient nie dostał błędu protokołu przy starcie odtwarzania.

use wayland_protocols::wp::content_type::v1::server::{
    wp_content_type_manager_v1 as manager, wp_content_type_v1 as content_type,
};
use wayland_server::protocol::wl_surface::WlSurface;
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, WEnum,
};

use crate::compositor::SurfaceData;
use crate::state::State;
use crate::sync::LockExt;

/// Rodzaj treści zgłoszony przez klienta (`none`, `photo`, `video`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentType {
    #[default]
    None,
    Photo,
    Video,
}

/// Stan zasobu `wp_content_type_v1`.
pub struct ContentTypeData {
    surface: WlSurface,
}

impl GlobalDispatch<manager::WpContentTypeManagerV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<manager::WpContentTypeManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<manager::WpContentTypeManagerV1, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &manager::WpContentTypeManagerV1,
        request: manager::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let manager::Request::GetSurfaceContentType { id, surface } = request {
            data_init.init(id, ContentTypeData { surface });
        }
    }
}

impl Dispatch<content_type::WpContentTypeV1, ContentTypeData> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &content_type::WpContentTypeV1,
        request: content_type::Request,
        data: &ContentTypeData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let content_type::Request::SetContentType { content_type } = request {
            let value = match content_type {
                WEnum::Value(content_type::Type::Photo) => ContentType::Photo,
                WEnum::Value(content_type::Type::Video) => ContentType::Video,
                _ => ContentType::None,
            };
            if let Some(surface_data) = data.surface.data::<SurfaceData>() {
                *surface_data.content_type.lock_or_recover() = value;
            }
        }
    }
}
