//! `wl_shm` — współdzielona pamięć klienta (bufory ARGB8888 → RGBA).

use std::ffi::c_void;
use std::os::fd::OwnedFd;
use std::sync::{Arc, Mutex};

use wayland_server::{
    protocol::{wl_buffer, wl_shm, wl_shm_pool},
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

use crate::state::State;
use crate::sync::LockExt;

/// Górny limit pojedynczego obrazu (RGBA) — chroni przed OOM wymuszonym przez klienta.
/// 256 MiB pokrywa 8K na kilku monitorach, a odcina absurdalne żądania.
const MAX_IMAGE_BYTES: usize = 256 * 1024 * 1024;

/// Zmapowany region puli. Odświeżany przy `wl_shm_pool.resize`.
struct Mapping {
    ptr: *mut c_void,
    size: usize,
}

// SAFETY: mapa jest tylko do odczytu; dostęp do `ptr` serializuje `Mutex`
// (i tak dzieje się to w wątku protokołu).
unsafe impl Send for Mapping {}

/// Dane puli pamięci (`wl_shm_pool`): deskryptor + zmapowany region.
///
/// Klient może tworzyć pulę małą i **rozszerzać** ją (`wl_shm_pool.resize`) —
/// dlatego mapa i rozmiar żyją za `Mutex` i są powiększane na żądanie.
pub struct ShmPoolData {
    pub fd: OwnedFd,
    map: Mutex<Mapping>,
}

impl ShmPoolData {
    /// Mapuje początkowy zakres puli (tylko odczyt). Błąd mapowania = pusta pula.
    pub fn new(fd: OwnedFd, declared_size: usize) -> Self {
        // Faktyczny rozmiar obiektu shm: klient może zadeklarować więcej, niż istnieje,
        // a odczyt poza obiekt kończy się SIGBUS (nie do złapania). Przycinamy rozmiar.
        let real = real_size(&fd);
        let size = declared_size.min(real);
        let ptr = map_region(&fd, size);
        if debug() {
            eprintln!("selene/shm: create_pool declared={declared_size} real={real} used={size}");
        }
        Self {
            fd,
            map: Mutex::new(Mapping { ptr, size }),
        }
    }

    /// Powiększa mapę po `wl_shm_pool.resize` (klient wcześniej powiększył `fd`).
    ///
    /// Rozmiar bywa deklarowany większy niż realny obiekt — przycinamy do realnego,
    /// żeby nie czytać poza obiekt (SIGBUS).
    pub fn resize(&self, declared_size: usize) {
        let real = real_size(&self.fd);
        let size = declared_size.min(real);
        let mut map = self.map.lock_or_recover();
        if size <= map.size {
            return; // tylko rośnie
        }
        if !map.ptr.is_null() {
            // SAFETY: odmapowujemy poprzedni zakres.
            unsafe {
                let _ = rustix::mm::munmap(map.ptr, map.size);
            }
        }
        map.ptr = map_region(&self.fd, size);
        map.size = size;
        if debug() {
            eprintln!("selene/shm: pool resize -> used={size}");
        }
    }
}

impl Drop for ShmPoolData {
    fn drop(&mut self) {
        let map = self.map.get_mut().unwrap_or_else(|e| e.into_inner());
        if map.ptr.is_null() {
            return;
        }
        // SAFETY: odmapowujemy ten sam zakres, który zmapowaliśmy.
        unsafe {
            let _ = rustix::mm::munmap(map.ptr, map.size);
        }
    }
}

/// Realny rozmiar obiektu shm (bez deklaracji klienta).
fn real_size(fd: &OwnedFd) -> usize {
    rustix::fs::fstat(fd)
        .map(|st| st.st_size.max(0) as usize)
        .unwrap_or(0)
}

/// Mapuje `size` bajtów pliku `fd` (tylko odczyt). `null` przy niepowodzeniu.
fn map_region(fd: &OwnedFd, size: usize) -> *mut c_void {
    if size == 0 {
        return std::ptr::null_mut();
    }
    // SAFETY: `fd` żyje w strukturze, mapujemy tylko do odczytu.
    unsafe {
        rustix::mm::mmap(
            std::ptr::null_mut::<c_void>(),
            size,
            rustix::mm::ProtFlags::READ,
            rustix::mm::MapFlags::PRIVATE,
            fd,
            0,
        )
    }
    .unwrap_or(std::ptr::null_mut())
}

fn debug() -> bool {
    std::env::var("SELENE_STATS").is_ok()
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
    /// Waliduje geometrię względem rozmiaru puli (arytmetyka bez przepełnień) i ogranicza
    /// rozmiar alokacji, żeby klient nie wywołał paniki ani OOM.
    pub fn to_rgba(&self) -> Option<Vec<u8>> {
        if self.width <= 0 || self.height <= 0 || self.stride <= 0 {
            return None;
        }
        let (w, h) = (self.width as usize, self.height as usize);
        let stride = self.stride as usize;
        // Wiersz musi pomieścić `w` pikseli po 4 bajty (inaczej odczyt wyszedłby poza wiersz).
        if stride < w.checked_mul(4)? {
            return None;
        }
        // Cały zakres danych musi mieścić się w puli — na sprawdzonej arytmetyce.
        let end = self.offset.checked_add(stride.checked_mul(h)?)?;
        // Limit alokacji chroniący przed OOM wymuszonym przez klienta.
        let out_len = w.checked_mul(h)?.checked_mul(4)?;
        if out_len > MAX_IMAGE_BYTES {
            return None;
        }

        // Trzymamy blokadę mapy przez cały odczyt (mapa może rosnąć w `resize`).
        let map = self.pool.map.lock_or_recover();
        if map.ptr.is_null() || end > map.size {
            return None;
        }
        let base = map.ptr as *const u8;
        let xrgb = self.format == wl_shm::Format::Xrgb8888;
        let mut out = vec![0u8; out_len];
        for y in 0..h {
            let row = self.offset + y * stride;
            for x in 0..w {
                let i = row + x * 4;
                let o = (y * w + x) * 4;
                // SAFETY: `i + 3 < end <= map.size`, a mapa obejmuje `map.size` bajtów.
                // Odczyt przez `read_volatile` — klient może pisać równolegle.
                unsafe {
                    out[o] = std::ptr::read_volatile(base.add(i + 2));
                    out[o + 1] = std::ptr::read_volatile(base.add(i + 1));
                    out[o + 2] = std::ptr::read_volatile(base.add(i));
                    out[o + 3] = if xrgb {
                        255
                    } else {
                        std::ptr::read_volatile(base.add(i + 3))
                    };
                }
            }
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
            data_init.init(id, Arc::new(ShmPoolData::new(fd, size.max(0) as usize)));
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
        match request {
            wl_shm_pool::Request::CreateBuffer {
                id,
                offset,
                width,
                height,
                stride,
                format,
            } => {
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
            wl_shm_pool::Request::Resize { size } => {
                data.resize(size.max(0) as usize);
            }
            _ => {}
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn memfd(bytes: &[u8]) -> OwnedFd {
        let fd =
            rustix::fs::memfd_create("selene-shm-test", rustix::fs::MemfdFlags::CLOEXEC).unwrap();
        let mut file = std::fs::File::from(fd);
        file.write_all(bytes).unwrap();
        file.into()
    }

    fn pool(bytes: &[u8]) -> Arc<ShmPoolData> {
        Arc::new(ShmPoolData::new(memfd(bytes), bytes.len()))
    }

    fn buffer(pool: Arc<ShmPoolData>, w: i32, h: i32, stride: i32) -> BufferData {
        BufferData {
            pool,
            offset: 0,
            width: w,
            height: h,
            stride,
            format: wl_shm::Format::Argb8888,
        }
    }

    #[test]
    fn to_rgba_swizzles_bgra_to_rgba() {
        // 1x1: pamięć B,G,R,A = (10,20,30,40) → RGBA (30,20,10,40).
        let rgba = buffer(pool(&[10, 20, 30, 40]), 1, 1, 4).to_rgba().unwrap();
        assert_eq!(rgba, vec![30, 20, 10, 40]);
    }

    #[test]
    fn xrgb_forces_opaque_alpha() {
        let mut b = buffer(pool(&[1, 2, 3, 0]), 1, 1, 4);
        b.format = wl_shm::Format::Xrgb8888;
        assert_eq!(b.to_rgba().unwrap(), vec![3, 2, 1, 255]);
    }

    #[test]
    fn rejects_stride_smaller_than_row() {
        // stride 2 < width*4 → odrzucone, bez paniki/OOB.
        assert!(buffer(pool(&[0u8; 64]), 4, 1, 2).to_rgba().is_none());
    }

    #[test]
    fn rejects_buffer_outside_pool() {
        // 2x2 ARGB = 16 bajtów > pula 8 → odrzucone.
        assert!(buffer(pool(&[0u8; 8]), 2, 2, 8).to_rgba().is_none());
    }

    #[test]
    fn resize_extends_mapping() {
        // W pliku jest 8 B, ale deklarujemy na starcie 4 B (jak sctk: mała pula + resize).
        let pool = Arc::new(ShmPoolData::new(memfd(&[1, 2, 3, 4, 5, 6, 7, 8]), 4));
        // Bufor 2x1 (8 B) nie mieści się w zmapowanych 4 B.
        assert!(buffer(Arc::clone(&pool), 2, 1, 8).to_rgba().is_none());
        // Po `resize` mapa rośnie i bufor się mieści.
        pool.resize(8);
        let rgba = buffer(Arc::clone(&pool), 2, 1, 8).to_rgba().unwrap();
        assert_eq!(rgba, vec![3, 2, 1, 4, 7, 6, 5, 8]);
    }
}
