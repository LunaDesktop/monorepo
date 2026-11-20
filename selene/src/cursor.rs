//! Kursor: kształty z motywu XCursor i obrazy dostarczane przez klientów.
//!
//! Kompozytor musi umieć dwie rzeczy:
//!
//! - **`wl_pointer.set_cursor`** — klient przysyła własną powierzchnię z buforem
//!   i hotspotem (tak robią starsze toolkitty i aplikacje malujące kursor same),
//! - **`wp_cursor_shape_v1`** — klient podaje tylko nazwę kształtu (`text`, `pointer`,
//!   `ew-resize`…), a my rysujemy go z motywu XCursor użytkownika.
//!
//! Obraz trzymamy w jednym miejscu ([`CURRENT`]), bo korzysta z niego i wątek
//! wyjścia (płaszczyzna sprzętowa DRM albo rysowanie w scenie), i `luna-control`.
//! Motyw ładujemy leniwie i cache'ujemy per kształt — czytanie plików XCursor
//! na każdą klatkę byłoby marnotrawstwem.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use libluna_core::{Color, Point, Rect};
use libluna_render::{gpu, ImageData, Scene, SoftwareRenderer};
use wayland_server::Resource;

use crate::state::{CursorImage, State};
use crate::sync::LockExt;

/// Ustawia powierzchnię kursora z `wl_pointer.set_cursor` (`None` = domyślna strzałka).
///
/// Klient może podać powierzchnię z buforem już wcześniej zaattachowanym — wtedy
/// od razu ją odczytujemy; zwykle jednak robi `attach` + `commit` zaraz po żądaniu,
/// więc obraz trafia do modelu w [`on_commit`](crate::cursor::on_commit).
pub fn set_surface(
    state: &mut State,
    surface: Option<wayland_server::protocol::wl_surface::WlSurface>,
    hotspot: (i32, i32),
) {
    match surface {
        None => {
            // NULL w `set_cursor` oznacza **ukrycie** kursora (protokół
            // wl_pointer), a nie powrót do strzałki. Gry (np. 0 A.D.) i
            // odtwarzacze rysują własny kursor i chowają systemowy —
            // wcześniej nad ich obrazem zostawała domyślna strzałka.
            state.cursor_surface = None;
            *state.cursor_image.lock_or_recover() = CursorImage::Hidden;
        }
        Some(surface) => {
            state.cursor_surface = Some(surface.clone());
            state.cursor_hotspot = hotspot;
            if let Some(data) = surface.data::<crate::compositor::SurfaceData>() {
                if let Some(rgba) = copy_buffer(&data, hotspot) {
                    *state.cursor_image.lock_or_recover() = rgba;
                }
            }
        }
    }
}

/// `commit` powierzchni kursora: kopiujemy nowy bufor do modelu i oddajemy go klientowi.
pub fn on_commit(
    state: &mut State,
    _surface: &wayland_server::protocol::wl_surface::WlSurface,
    data: &crate::compositor::SurfaceData,
) {
    let hotspot = state.cursor_hotspot;
    if let Some(image) = copy_buffer(data, hotspot) {
        *state.cursor_image.lock_or_recover() = image;
    }
}

/// Klient zniszczył powierzchnię kursora — wracamy do domyślnej strzałki.
pub fn surface_destroyed(state: &mut State) {
    state.cursor_surface = None;
    *state.cursor_image.lock_or_recover() = CursorImage::Default;
}

/// Odczytuje bufor powierzchni kursora jako `ImageData` (premultiplied RGBA).
///
/// Obsługuje `wl_shm` oraz `dmabuf` (Zed/GPUI i inne klienty GPU ustawiają
/// kursor buforem GPU). Dla `dmabuf` robimy import EGL i readback — kursor
/// musi być dostępny dla płaszczyzny sprzętowej i rysowania w scenie.
fn copy_buffer(data: &crate::compositor::SurfaceData, hotspot: (i32, i32)) -> Option<CursorImage> {
    let buffer = data.buffer.lock_or_recover().clone()?;
    let buf_data = buffer.data::<crate::shm::BufferData>()?;
    if buf_data.width <= 0 || buf_data.height <= 0 {
        return None;
    }
    let (width, height, rgba) = if let Some(dmabuf) = buf_data.dmabuf.clone() {
        let rgba = gpu::GpuRenderer::dmabuf_to_rgba(&dmabuf)?;
        (dmabuf.width, dmabuf.height, rgba)
    } else {
        let rgba = buf_data.to_rgba()?;
        (buf_data.width as u32, buf_data.height as u32, rgba)
    };
    let image = libluna_render::ImageData::new(width, height, rgba);
    // Piksele mamy w pamięci — bufor można oddać od razu (klient czeka na `release`).
    if data.take_release() {
        let _ = buffer.send_event(wayland_server::protocol::wl_buffer::Event::Release);
    }
    Some(CursorImage::Custom {
        data: Arc::new(image),
        hotspot,
    })
}

/// Rozmiar wbudowanej strzałki (gdy nie ma żadnego motywu kursorów).
const FALLBACK_SIZE: u32 = 24;
/// Preferowany rozmiar kształtu z motywu (px). Płaszczyzna sprzętowa przyjmie
/// więcej (limit czytamy z `DRM_CAP_CURSOR_WIDTH/HEIGHT`, NVIDIA: 256×256),
/// ale kursor pulpitu ma wyglądać jak w reszcie systemu.
const PREFERRED_SIZE: u32 = 24;

/// Aktualny obraz kursora (piksele + hotspot) — wspólny dla wątku wyjścia i testów.
static CURRENT: Mutex<Option<(Arc<ImageData>, (i32, i32))>> = Mutex::new(None);

/// Ustawia bieżący obraz kursora na podstawie modelu współdzielonego z kompozytorem.
pub fn set_model(image: &CursorImage) {
    let resolved = resolve(image);
    *CURRENT.lock_or_recover() = Some(resolved);
}

/// Bieżący obraz kursora: `(piksele, hotspot)`. Bez ustawionego modelu — strzałka.
pub fn current() -> (Arc<ImageData>, (i32, i32)) {
    CURRENT
        .lock_or_recover()
        .clone()
        .unwrap_or_else(default_arrow)
}

/// Rysuje kursor w scenie w pozycji `pos` (software — bez płaszczyzny sprzętowej).
pub fn paint(scene: &mut Scene, pos: Point) {
    let (image, (hx, hy)) = current();
    let rect = Rect::from_xywh(
        pos.x - f64::from(hx),
        pos.y - f64::from(hy),
        f64::from(image.width),
        f64::from(image.height),
    );
    scene.image(rect, image);
}

/// Rozwiązuje model kursora na konkretne piksele.
pub fn resolve(image: &CursorImage) -> (Arc<ImageData>, (i32, i32)) {
    match image {
        CursorImage::Custom { data, hotspot } => (Arc::clone(data), *hotspot),
        CursorImage::Shape(name) => shape(name).unwrap_or_else(default_arrow),
        CursorImage::Hidden => hidden_image(),
        CursorImage::Default => default_arrow(),
    }
}

/// Przezroczysty 1×1 px — "kursor ukryty" bez specjalnego przypadku
/// w płaszczyźnie sprzętowej i w rysowaniu sceny.
fn hidden_image() -> (Arc<ImageData>, (i32, i32)) {
    static HIDDEN: OnceLock<(Arc<ImageData>, (i32, i32))> = OnceLock::new();
    HIDDEN
        .get_or_init(|| (Arc::new(ImageData::new(1, 1, vec![0, 0, 0, 0])), (0, 0)))
        .clone()
}

/// Czy obraz zmieści się na płaszczyźnie kursora o zadanym rozmiarze.
pub fn fits_plane(image: &ImageData, cap: (u32, u32)) -> bool {
    image.width > 0 && image.height > 0 && image.width <= cap.0 && image.height <= cap.1
}

/// Domyślna strzałka: najpierw z motywu XCursor (`left_ptr`), a w razie braku
/// motywu w systemie — wbudowana strzałka wektorowa.
pub fn default_arrow() -> (Arc<ImageData>, (i32, i32)) {
    static ARROW: OnceLock<(Arc<ImageData>, (i32, i32))> = OnceLock::new();
    ARROW
        .get_or_init(|| {
            if let Some(themed) = shape("left_ptr") {
                return themed;
            }
            let mut scene = Scene::new();
            draw_arrow(&mut scene, Point::new(1.0, 1.0));
            let mut renderer = SoftwareRenderer::new(FALLBACK_SIZE, FALLBACK_SIZE);
            renderer.draw(&scene, 1.0, Color::rgba(0.0, 0.0, 0.0, 0.0));
            let image = ImageData::new(FALLBACK_SIZE, FALLBACK_SIZE, renderer.data().to_vec());
            (Arc::new(image), (1, 1))
        })
        .clone()
}

/// Klasyczna strzałka (biała z ciemnym obrysem) — zapas, gdy brak motywu.
fn draw_arrow(scene: &mut Scene, p: Point) {
    let arrow = [
        Point::new(p.x, p.y),
        Point::new(p.x, p.y + 17.0),
        Point::new(p.x + 4.2, p.y + 12.6),
        Point::new(p.x + 7.0, p.y + 18.5),
        Point::new(p.x + 9.7, p.y + 17.2),
        Point::new(p.x + 6.9, p.y + 11.4),
        Point::new(p.x + 11.5, p.y + 11.4),
    ];
    let white = Color::from_rgba8(0xf5, 0xf5, 0xf5, 0xff);
    let outline = Color::from_rgba8(0x10, 0x10, 0x10, 0xff);
    scene.fill_polygon(&arrow, white);
    let mut closed = arrow.to_vec();
    closed.push(arrow[0]);
    scene.stroke_polyline(&closed, 1.5, outline);
}

/// Kształt z motywu XCursor (cache per nazwa; `None` = brak motywu/pliku).
fn shape(name: &str) -> Option<(Arc<ImageData>, (i32, i32))> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<(Arc<ImageData>, (i32, i32))>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock_or_recover().get(name) {
        return hit.clone();
    }
    let loaded = load_shape(name);
    cache
        .lock_or_recover()
        .insert(name.to_owned(), loaded.clone());
    loaded
}

fn load_shape(name: &str) -> Option<(Arc<ImageData>, (i32, i32))> {
    let theme = theme()?;
    // Kolejność prób: nazwa z protokołu (Adwaita ma nazwy CSS), nazwa X11
    // (starsze motywy znają tylko `hand2`/`xterm`), uniwersalna strzałka.
    // Uszkodzony plik nie może schować kursora — próbujemy dalej.
    let alias = xcursor_alias(name);
    let candidates = [
        Some(name),
        (alias != name).then_some(alias),
        Some("left_ptr"),
        Some("default"),
    ];
    for candidate in candidates.into_iter().flatten() {
        let Some(path) = theme.load_icon(candidate) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Some(images) = xcursor::parser::parse_xcursor(&bytes) else {
            continue;
        };
        let Some(best) = images.iter().min_by_key(|image| {
            let size = i64::from(image.size.max(image.width).max(image.height));
            (size - i64::from(PREFERRED_SIZE)).abs()
        }) else {
            continue;
        };
        if best.width == 0 || best.height == 0 {
            continue;
        }
        let image = Arc::new(xcursor_to_image(best));
        return Some((image, (best.xhot as i32, best.yhot as i32)));
    }
    None
}

/// Nazwa kształtu CSS (`cursor-shape-v1`) → nazwa pliku w motywach X11.
///
/// Adwaita dostarcza oba zestawy nazw, ale DMZ i starsze motywy tylko X11 —
/// bez tego mapowania `pointer`/`text`/`ew-resize` spadałyby do strzałki.
fn xcursor_alias(name: &str) -> &str {
    match name {
        "default" | "arrow" | "context-menu" | "dnd-ask" | "zoom-in" | "zoom-out" => "left_ptr",
        "pointer" => "hand2",
        "text" | "vertical-text" => "xterm",
        "crosshair" | "cell" => "crosshair",
        "move" | "all-scroll" | "all-resize" => "fleur",
        "help" => "question_arrow",
        "wait" => "watch",
        "progress" => "left_ptr_watch",
        "grab" => "hand1",
        "grabbing" => "hand1",
        "not-allowed" | "no-drop" => "crossed_circle",
        "alias" => "link",
        "e-resize" => "sb_right_arrow",
        "w-resize" => "sb_left_arrow",
        "n-resize" => "sb_up_arrow",
        "s-resize" => "sb_down_arrow",
        "ew-resize" | "col-resize" => "sb_h_double_arrow",
        "ns-resize" | "row-resize" => "sb_v_double_arrow",
        "ne-resize" => "top_right_corner",
        "nw-resize" => "top_left_corner",
        "se-resize" => "bottom_right_corner",
        "sw-resize" => "bottom_left_corner",
        "nesw-resize" => "bd_double_arrow",
        "nwse-resize" => "fd_double_arrow",
        other => other,
    }
}

/// Konwersja obrazu XCursor → premultiplied RGBA (format `ImageData`).
///
/// Plik XCursor trzyma piksele jako 32-bitowe ARGB little-endian, czyli w pamięci
/// **B, G, R, A**; `pixels_rgba` z crate'a to surowe bajty pliku (nazwa myli).
fn xcursor_to_image(image: &xcursor::parser::Image) -> ImageData {
    let (w, h) = (image.width as usize, image.height as usize);
    let mut out = vec![0u8; w * h * 4];
    for (dst, src) in out
        .chunks_exact_mut(4)
        .zip(image.pixels_rgba.chunks_exact(4))
    {
        let (b, g, r, a) = (src[0], src[1], src[2], src[3]);
        let af = u32::from(a);
        dst[0] = (u32::from(r) * af / 255) as u8;
        dst[1] = (u32::from(g) * af / 255) as u8;
        dst[2] = (u32::from(b) * af / 255) as u8;
        dst[3] = a;
    }
    ImageData::new(w as u32, h as u32, out)
}

/// Motyw kursorów: pierwszy działający z listy preferencji.
fn theme() -> Option<&'static xcursor::CursorTheme> {
    static THEME: OnceLock<Option<xcursor::CursorTheme>> = OnceLock::new();
    THEME
        .get_or_init(|| {
            for name in theme_candidates() {
                let theme = xcursor::CursorTheme::load(&name);
                if theme.load_icon("left_ptr").is_some() || theme.load_icon("default").is_some() {
                    return Some(theme);
                }
            }
            None
        })
        .as_ref()
}

fn theme_candidates() -> Vec<String> {
    let mut names = Vec::new();
    for var in ["SELENE_CURSOR_THEME", "XCURSOR_THEME"] {
        if let Ok(value) = std::env::var(var) {
            for name in value.split([',', ';']) {
                push_theme(&mut names, name, 0);
            }
        }
    }
    if let Some(gtk) = gtk_cursor_theme() {
        for name in gtk.split([',', ';']) {
            push_theme(&mut names, name, 0);
        }
    }
    // Kolejność: motyw dystrybucji, potem klasyki. Motyw bez plików kursorów
    // (np. pusty `cursors/`) jest pomijany przez sprawdzenie `left_ptr`.
    for name in [
        "Luna-Cursors",
        "Pagan-Cursors",
        "Adwaita",
        "DMZ-Black",
        "DMZ-White",
        "default",
    ] {
        push_theme(&mut names, name, 0);
    }
    names
}

/// Dodaje motyw i — rekurencyjnie — motywy z jego `Inherits`.
///
/// Crate `xcursor` traktuje `Inherits` jako **jedną** nazwę, więc lista
/// `Inherits=xcursor-dmz,Adwaita` (tak ma `Pagan-Cursors`) nie prowadzi do
/// żadnego motywu. Rozbijamy ją sami, inaczej pusty motyw zostaje wybrany
/// i kursor spada do wbudowanej strzałki.
fn push_theme(names: &mut Vec<String>, name: &str, depth: usize) {
    let name = name.trim();
    if name.is_empty() || depth > 8 || names.iter().any(|existing| existing == name) {
        return;
    }
    names.push(name.to_owned());
    for parent in inherited_themes(name) {
        push_theme(names, &parent, depth + 1);
    }
}

/// Katalogi z motywami XCursor, w kolejności przeszukiwania.
fn icon_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        roots.push(home.join(".local/share/icons"));
        roots.push(home.join(".icons"));
    }
    if let Some(dirs) = std::env::var_os("XDG_DATA_DIRS") {
        for dir in std::env::split_paths(&dirs) {
            roots.push(dir.join("icons"));
        }
    } else {
        roots.push(PathBuf::from("/usr/local/share/icons"));
        roots.push(PathBuf::from("/usr/share/icons"));
    }
    roots
}

/// Nazwy z `Inherits` motywu (lista rozdzielana przecinkami, średnikami lub spacjami).
fn inherited_themes(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    for root in icon_roots() {
        let Ok(text) = std::fs::read_to_string(root.join(name).join("index.theme")) else {
            continue;
        };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if !key.trim().eq_ignore_ascii_case("Inherits") {
                continue;
            }
            for part in value.split([',', ';', ' ', '\t']) {
                let part = part.trim();
                if !part.is_empty() && !out.iter().any(|existing| existing == part) {
                    out.push(part.to_owned());
                }
            }
        }
    }
    out
}

/// Motyw wybrany w GTK (`gtk-cursor-theme-name`), jeśli użytkownik go ustawił.
fn gtk_cursor_theme() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    for rel in [
        ".config/gtk-4.0/settings.ini",
        ".config/gtk-3.0/settings.ini",
    ] {
        let path: PathBuf = PathBuf::from(&home).join(rel);
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for line in text.lines() {
            if let Some(value) = line.trim().strip_prefix("gtk-cursor-theme-name=") {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_owned());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xcursor_bytes_are_bgra() {
        // XCursor: u32 ARGB LE → bajty B,G,R,A. Czerwony nieprzezroczysty.
        let image = xcursor::parser::Image {
            size: 1,
            width: 1,
            height: 1,
            xhot: 0,
            yhot: 0,
            delay: 0,
            pixels_rgba: vec![0x00, 0x00, 0xff, 0xff],
            pixels_argb: vec![0xff, 0x00, 0x00, 0xff],
        };
        let out = xcursor_to_image(&image);
        assert_eq!(out.rgba, vec![0xff, 0x00, 0x00, 0xff]);
    }

    #[test]
    fn xcursor_premultiplies_alpha() {
        // Półprzezroczysty biały: 128/255 alfy → kanały ~128.
        let image = xcursor::parser::Image {
            size: 1,
            width: 1,
            height: 1,
            xhot: 0,
            yhot: 0,
            delay: 0,
            pixels_rgba: vec![0xff, 0xff, 0xff, 0x80],
            pixels_argb: vec![0x80, 0xff, 0xff, 0xff],
        };
        let out = xcursor_to_image(&image);
        assert_eq!(out.rgba[3], 0x80);
        assert!(out.rgba[0] >= 0x7f && out.rgba[0] <= 0x81);
    }

    #[test]
    fn default_arrow_has_pixels_and_hotspot() {
        let (image, hotspot) = default_arrow();
        assert_eq!((image.width, image.height), (FALLBACK_SIZE, FALLBACK_SIZE));
        assert!(image.rgba.chunks_exact(4).any(|px| px[3] > 0));
        assert!(hotspot.0 >= 0 && hotspot.1 >= 0);
    }

    #[test]
    fn css_shape_names_map_to_x11() {
        // Adwaita zna oba zestawy, ale DMZ tylko X11 — mapowanie ratuje I-beam/hand.
        assert_eq!(xcursor_alias("pointer"), "hand2");
        assert_eq!(xcursor_alias("text"), "xterm");
        assert_eq!(xcursor_alias("ew-resize"), "sb_h_double_arrow");
        assert_eq!(xcursor_alias("left_ptr"), "left_ptr");
    }

    #[test]
    fn shape_lookup_never_panics() {
        // Kontrakt: brak motywu/pliku to `None`, nie panika.
        let _ = shape("text");
        let _ = shape("__nie_ma_takiego_ksztaltu__");
    }

    #[test]
    fn paint_draws_something() {
        let mut scene = Scene::new();
        paint(&mut scene, Point::new(10.0, 10.0));
        assert!(!scene.is_empty());
    }

    #[test]
    fn hidden_cursor_is_transparent() {
        let (image, hotspot) = resolve(&CursorImage::Hidden);
        assert_eq!((image.width, image.height), (1, 1));
        assert_eq!(hotspot, (0, 0));
        assert!(image.rgba.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn bundled_luna_theme_has_every_required_shape() {
        // Motyw jest w repo, więc test pilnuje, że każdy kształt i alias
        // używany przez `cursor-shape-v1` naprawdę istnieje i jest poprawny.
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/cursors/Luna-Cursors/cursors");
        let required = [
            "left_ptr",
            "left_ptr_watch",
            "watch",
            "wait",
            "progress",
            "hand1",
            "hand2",
            "grab",
            "grabbing",
            "pointer",
            "xterm",
            "text",
            "vertical-text",
            "crosshair",
            "cell",
            "fleur",
            "move",
            "all-scroll",
            "sb_h_double_arrow",
            "sb_v_double_arrow",
            "ew-resize",
            "ns-resize",
            "col-resize",
            "row-resize",
            "e-resize",
            "w-resize",
            "n-resize",
            "s-resize",
            "bd_double_arrow",
            "fd_double_arrow",
            "nesw-resize",
            "nwse-resize",
            "top_left_corner",
            "top_right_corner",
            "bottom_left_corner",
            "bottom_right_corner",
            "sb_left_arrow",
            "sb_right_arrow",
            "sb_up_arrow",
            "sb_down_arrow",
            "crossed_circle",
            "not-allowed",
            "no-drop",
            "question_arrow",
            "help",
            "up_arrow",
            "copy",
            "dnd-copy",
            "dnd-move",
            "dnd-link",
            "dnd-no-drop",
            "context-menu",
            "zoom-in",
            "zoom-out",
            "default",
        ];
        for name in required {
            let path = dir.join(name);
            let bytes =
                std::fs::read(&path).unwrap_or_else(|error| panic!("brak kursora {name}: {error}"));
            let images = xcursor::parser::parse_xcursor(&bytes)
                .unwrap_or_else(|| panic!("niepoprawny XCursor: {name}"));
            assert!(!images.is_empty(), "{name}: brak obrazów");
            for image in &images {
                assert!(image.width > 0 && image.height > 0, "{name}: pusty obraz");
                assert!(
                    image.xhot < image.width as u32 && image.yhot < image.height as u32,
                    "{name}: hotspot poza obrazem"
                );
                assert_eq!(
                    image.pixels_rgba.len(),
                    (image.width * image.height * 4) as usize,
                    "{name}: zły rozmiar pikseli"
                );
            }
        }
    }

    #[test]
    fn luna_cursors_is_preferred_over_distro_themes() {
        let names = theme_candidates();
        let luna = names.iter().position(|name| name == "Luna-Cursors");
        assert!(luna.is_some(), "brak Luna-Cursors na liście motywów");
        let pagan = names.iter().position(|name| name == "Pagan-Cursors");
        if let (Some(luna), Some(pagan)) = (luna, pagan) {
            assert!(luna < pagan, "Luna-Cursors musi być przed Pagan-Cursors");
        }
    }
}
