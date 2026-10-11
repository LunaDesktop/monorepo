//! `wl_shm` — współdzielona pamięć klienta (bufory ARGB8888 → RGBA).

use std::ffi::c_void;
use std::os::fd::OwnedFd;
use std::sync::Arc;

use wayland_server::{
    protocol::{wl_buffer, wl_shm, wl_shm_pool},
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

use crate::state::State;

/// Górny limit pojedynczego obrazu (RGBA) — chroni przed OOM wymuszonym przez klienta.
/// 256 MiB pokrywa 8K na kilku monitorach, a odcina absurdalne żądania.
const MAX_IMAGE_BYTES: usize = 256 * 1024 * 1024;

/// Dane puli pamięci (`wl_shm_pool`): deskryptor, rozmiar i **zbuforowana mapa**.
///
/// Mapę tworzymy raz (przy `create_pool`) i zwalniamy przy zniszczeniu puli — zamiast
/// mapować/odmapowywać przy każdym commicie.
pub struct ShmPoolData {
    #[allow(dead_code)]
    pub fd: OwnedFd,
    pub size: usize,
    map: *mut c_void,
}

// Mapa jest tylko do odczytu i współdzielona — bezpieczna do wysyłania między wątkami.
unsafe impl Send for ShmPoolData {}
unsafe impl Sync for ShmPoolData {}

impl ShmPoolData {
    /// Mapuje pulę (tylko odczyt). Przy niepowodzeniu zostaje pusta (bez paniki).
    pub fn new(fd: OwnedFd, declared_size: usize) -> Self {
        // Faktyczny rozmiar obiektu shm: klient może zadeklarować więcej, niż istnieje,
        // a odczyt poza obiekt kończy się SIGBUS (nie do złapania). Przycinamy rozmiar.
        let real = rustix::fs::fstat(&fd)
            .map(|st| st.st_size.max(0) as usize)
            .unwrap_or(0);
        let size = declared_size.min(real);
        let map = if size == 0 {
            std::ptr::null_mut()
        } else {
            // SAFETY: `fd` żyje w strukturze, mapujemy tylko do odczytu.
            unsafe {
                rustix::mm::mmap(
                    std::ptr::null_mut::<c_void>(),
                    size,
                    rustix::mm::ProtFlags::READ,
                    rustix::mm::MapFlags::PRIVATE,
                    &fd,
                    0,
                )
            }
            .unwrap_or(std::ptr::null_mut())
        };
        Self { fd, size, map }
    }

    /// Odczyt jednego bajtu przez `read_volatile`.
    ///
    /// Klient (osobny proces) może pisać do tej pamięci w dowolnym momencie, więc nie
    /// tworzymy `&[u8]` na współdzielonym mapowaniu (to formalny wyścig). `None`, gdy
    /// indeks poza mapą albo mapowanie się nie powiodło.
    pub fn read_u8(&self, index: usize) -> Option<u8> {
        if self.map.is_null() || index >= self.size {
            return None;
        }
        // SAFETY: `index < self.size`, a mapa obejmuje `self.size` bajtów.
        Some(unsafe { std::ptr::read_volatile((self.map as *const u8).add(index)) })
    }
}

impl Drop for ShmPoolData {
    fn drop(&mut self) {
        if self.map.is_null() {
            return;
        }
        // SAFETY: odmapowujemy ten sam zakres, który zmapowaliśmy.
        unsafe {
            let _ = rustix::mm::munmap(self.map, self.size);
        }
    }
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
        if end > self.pool.size {
            return None;
        }
        // Limit alokacji chroniący przed OOM wymuszonym przez klienta.
        let out_len = w.checked_mul(h)?.checked_mul(4)?;
        if out_len > MAX_IMAGE_BYTES {
            return None;
        }
        let mut out = vec![0u8; out_len];
        for y in 0..h {
            let row = self.offset + y * stride;
            for x in 0..w {
                let i = row + x * 4;
                let o = (y * w + x) * 4;
                // ARGB8888 (pamięć: B,G,R,A) → RGBA8; XRGB8888 → alfa wymuszona na 1.
                out[o] = self.pool.read_u8(i + 2)?;
                out[o + 1] = self.pool.read_u8(i + 1)?;
                out[o + 2] = self.pool.read_u8(i)?;
                out[o + 3] = if self.format == wl_shm::Format::Xrgb8888 {
                    255
                } else {
                    self.pool.read_u8(i + 3)?
                };
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn pool(bytes: &[u8]) -> Arc<ShmPoolData> {
        let fd =
            rustix::fs::memfd_create("selene-shm-test", rustix::fs::MemfdFlags::CLOEXEC).unwrap();
        let mut file = std::fs::File::from(fd);
        file.write_all(bytes).unwrap();
        Arc::new(ShmPoolData::new(file.into(), bytes.len()))
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
}
