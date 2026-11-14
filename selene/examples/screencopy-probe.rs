//! Klient testowy `wlr-screencopy` — robi zrzut pierwszego wyjścia do PPM.
//!
//! Uruchomienie w sesji Selene:
//!
//! ```text
//! WAYLAND_DISPLAY=wayland-0 cargo run -p selene --example screencopy-probe -- /tmp/zrzut.ppm
//! ```
//!
//! Sprawdza całą ścieżkę: `capture_output` → `buffer`/`buffer_done` → `copy` →
//! `flags` + `ready`, oraz zapis do bufora `wl_shm` po stronie kompozytora.
//! Plik PPM otwiera każdy edytor obrazów (i PIL).

use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::os::fd::{AsFd, OwnedFd};

use wayland_client::protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::{
    globals::{registry_queue_init, GlobalListContents},
    Connection, Dispatch, QueueHandle, WEnum,
};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1 as frame, zwlr_screencopy_manager_v1 as manager,
};

#[derive(Default)]
struct App {
    buffer: Option<(u32, u32, u32)>, // format, width, height (stride = width*4)
    ready: bool,
    failed: bool,
}

impl Dispatch<wl_shm::WlShm, ()> for App {
    fn event(
        _state: &mut Self,
        _proxy: &wl_shm::WlShm,
        _event: wl_shm::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for App {
    fn event(
        _state: &mut Self,
        _proxy: &wl_shm_pool::WlShmPool,
        _event: wl_shm_pool::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for App {
    fn event(
        _state: &mut Self,
        _proxy: &wl_buffer::WlBuffer,
        _event: wl_buffer::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_output::WlOutput, ()> for App {
    fn event(
        _state: &mut Self,
        _proxy: &wl_output::WlOutput,
        _event: wl_output::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<manager::ZwlrScreencopyManagerV1, ()> for App {
    fn event(
        _state: &mut Self,
        _proxy: &manager::ZwlrScreencopyManagerV1,
        _event: manager::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<frame::ZwlrScreencopyFrameV1, ()> for App {
    fn event(
        state: &mut Self,
        _proxy: &frame::ZwlrScreencopyFrameV1,
        event: frame::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            frame::Event::Buffer {
                format,
                width,
                height,
                ..
            } => {
                state.buffer = Some((match format { WEnum::Value(v) => v as u32, WEnum::Unknown(v) => v }, width, height))
            }
            frame::Event::Flags { .. } => {}
            frame::Event::Ready { .. } => state.ready = true,
            frame::Event::Failed => state.failed = true,
            frame::Event::Damage { .. }
            | frame::Event::LinuxDmabuf { .. }
            | frame::Event::BufferDone => {}
            _ => {}
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for App {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "screencopy.ppm".to_owned());
    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<App>(&conn)?;
    let qh = queue.handle();
    let mut app = App::default();

    let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ())?;
    let manager: manager::ZwlrScreencopyManagerV1 = globals.bind(&qh, 1..=3, ())?;
    let output: wl_output::WlOutput = globals.bind(&qh, 1..=4, ())?;
    queue.roundtrip(&mut app)?;

    let frame = manager.capture_output(0, &output, &qh, ());
    // `buffer`/`buffer_done` przychodzą od razu po utworzeniu klatki.
    for _ in 0..5 {
        conn.flush()?;
        queue.roundtrip(&mut app)?;
        if app.buffer.is_some() {
            break;
        }
    }
    let Some((format, width, height)) = app.buffer else {
        return Err("kompozytor nie przysłał parametrów bufora".into());
    };
    eprintln!(
        "screencopy: format={format} {width}x{height} (stride {})",
        width * 4
    );

    let size = (width as usize) * (height as usize) * 4;
    let fd = rustix::fs::memfd_create("selene-screencopy", rustix::fs::MemfdFlags::CLOEXEC)?;
    let file = File::from(OwnedFd::from(fd));
    file.set_len(size as u64)?;
    let pool = shm.create_pool(file.as_fd(), size as i32, &qh, ());
    let buffer = pool.create_buffer(
        0,
        width as i32,
        height as i32,
        (width * 4) as i32,
        wl_shm::Format::Xrgb8888,
        &qh,
        (),
    );
    frame.copy(&buffer);

    for _ in 0..20 {
        conn.flush()?;
        queue.roundtrip(&mut app)?;
        if app.ready || app.failed {
            break;
        }
    }
    if app.failed {
        return Err("kompozytor odrzucił klatkę (failed)".into());
    }
    if !app.ready {
        return Err("brak zdarzenia ready".into());
    }

    // Odczyt z tej samej pamięci, którą kompozytor wypełnił (MAP_SHARED).
    let map = unsafe {
        rustix::mm::mmap(
            std::ptr::null_mut(),
            size,
            rustix::mm::ProtFlags::READ,
            rustix::mm::MapFlags::SHARED,
            &file,
            0,
        )?
    };
    let pixels = unsafe { std::slice::from_raw_parts(map as *const u8, size) };
    let mut out = File::create(&out_path)?;
    write!(out, "P6\n{width} {height}\n255\n")?;
    // Bufor klienta: XRGB8888 w pamięci = B,G,R,X.
    let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
    for px in pixels.chunks_exact(4) {
        rgb.extend_from_slice(&[px[2], px[1], px[0]]);
    }
    out.write_all(&rgb)?;
    unsafe {
        let _ = rustix::mm::munmap(map, size);
    }
    println!("zapisano {out_path} ({width}x{height})");
    Ok(())
}
