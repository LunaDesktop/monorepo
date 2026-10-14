//! Globalny stan kompozytora Selene.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use libluna_core::{Modifiers, Point, Rect};
use libluna_render::ImageData;
use wayland_protocols::ext::workspace::v1::server::{
    ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1,
    ext_workspace_handle_v1::ExtWorkspaceHandleV1, ext_workspace_manager_v1::ExtWorkspaceManagerV1,
};
use wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1;
use wayland_server::protocol::wl_data_device::WlDataDevice;
use wayland_server::protocol::wl_data_source::WlDataSource;
use wayland_server::protocol::wl_keyboard::WlKeyboard;
use wayland_server::protocol::wl_pointer::WlPointer;
use wayland_server::protocol::wl_surface::WlSurface;

use crate::data::DndState;
use crate::monitors::{OutputInfo, Outputs};
use crate::sync::LockExt;

/// Wysokość paska tytułu dekoracji (SSD), w pikselach logicznych.
pub const TITLEBAR: f64 = 30.0;
/// Szerokość przycisku sterowania oknem (minimalizuj/maksymalizuj/zamknij).
pub const BUTTON_W: f64 = 30.0;
/// Grubość krawędzi wyłapywanej do zmiany rozmiaru.
pub const BORDER: f64 = 6.0;

/// Zapamiętana geometria przed maksymalizacją/fullscreenem (do przywrócenia).
#[derive(Debug, Clone, Copy)]
pub struct SavedGeom {
    pub x: f64,
    pub y: f64,
    pub width: i32,
    pub height: i32,
}

/// Okno (toplevel/popup) w modelu kompozycji: pozycja, rozmiar, kolejność i piksele.
pub struct Window {
    pub x: f64,
    pub y: f64,
    /// Rozmiar **treści** (bufora klienta).
    pub width: i32,
    pub height: i32,
    /// Kolejność (wyższa = wyżej); do renderu i trafień.
    pub z: i32,
    /// Tytuł okna (z `xdg_toplevel.set_title`).
    pub title: String,
    /// Czy to `xdg_popup` (bez dekoracji, zamykany kliknięciem poza).
    pub popup: bool,
    /// Czy to powierzchnia `wlr-layer-shell` (panel/dock; bez dekoracji, kotwiczona).
    pub layer: bool,
    /// Czy powierzchnia jest widoczna (popup znika po `popup_done`).
    pub mapped: bool,
    /// Przestrzeń robocza, do której należy okno (panele/popupy są globalne).
    pub workspace: usize,
    /// Zminimalizowane (ukryte przez kompozytor; przywracane skrótem).
    pub minimized: bool,
    /// Zmaksymalizowane do obszaru monitora.
    pub maximized: bool,
    /// Pełny ekran.
    pub fullscreen: bool,
    /// Geometria zapamiętana przed maksymalizacją/fullscreenem.
    pub saved: Option<SavedGeom>,
    /// Piksele (premultiplied RGBA8) ostatnio zacommitowanego bufora.
    pub data: Arc<ImageData>,
}

impl Window {
    /// Czy okno pokazuje się w danej przestrzeni roboczej.
    ///
    /// Powierzchnie warstwowe (pasek, tło, OSD) są globalne — należą do każdej
    /// przestrzeni; zwykłe okna i popupy widoczne są tylko w swojej.
    pub fn in_workspace(&self, ws: usize) -> bool {
        self.layer || self.workspace == ws
    }

    /// Wysokość dekoracji (zero dla popupów, paneli i pełnego ekranu).
    fn bar(&self) -> f64 {
        if self.popup || self.layer || self.fullscreen {
            0.0
        } else {
            TITLEBAR
        }
    }

    /// Prostokąt całego okna (dekoracja + treść).
    pub fn outer(&self) -> Rect {
        Rect::from_xywh(
            self.x,
            self.y,
            self.width as f64,
            self.bar() + self.height as f64,
        )
    }

    /// Pasek tytułu (zerowy dla popupów).
    pub fn titlebar(&self) -> Rect {
        Rect::from_xywh(self.x, self.y, self.width as f64, self.bar())
    }

    /// Obszar treści (klienta).
    pub fn content(&self) -> Rect {
        Rect::from_xywh(
            self.x,
            self.y + self.bar(),
            self.width as f64,
            self.height as f64,
        )
    }

    /// Przycisk sterowania oknem licząc od prawej (0 = zamknij, 1 = maksymalizuj, 2 = minimalizuj).
    fn control_rect(&self, from_right: usize) -> Rect {
        if self.popup || self.layer {
            return Rect::ZERO;
        }
        Rect::from_xywh(
            self.x + self.width as f64 - BUTTON_W * (from_right as f64 + 1.0),
            self.y,
            BUTTON_W,
            TITLEBAR,
        )
    }

    /// Kwadratowy przycisk zamknięcia (tylko dla topleveli).
    pub fn close_rect(&self) -> Rect {
        self.control_rect(0)
    }

    /// Przycisk maksymalizacji.
    pub fn maximize_rect(&self) -> Rect {
        self.control_rect(1)
    }

    /// Przycisk minimalizacji.
    pub fn minimize_rect(&self) -> Rect {
        self.control_rect(2)
    }

    /// Bitmaska krawędzi wyłapywanej do zmiany rozmiaru dla punktu (0 gdy brak).
    /// 1 = góra, 2 = dół, 4 = lewo, 8 = prawo.
    pub fn resize_edges(&self, p: Point) -> u8 {
        if self.popup || self.layer || self.fullscreen || self.maximized {
            return 0;
        }
        let o = self.outer();
        if !o.contains(p) {
            return 0;
        }
        let mut e = 0u8;
        if p.x <= o.x() + BORDER {
            e |= 4;
        }
        if p.x >= o.right() - BORDER {
            e |= 8;
        }
        if p.y <= o.y() + BORDER {
            e |= 1;
        }
        if p.y >= o.bottom() - BORDER {
            e |= 2;
        }
        e
    }
}

/// Współdzielony model okien (pisany przez wątek protokołu, czytany przez wyjście).
pub type WindowModel = Arc<Mutex<Vec<Window>>>;

/// Współdzielona pozycja kursora (tryb DRM/evdev); `None` = nie rysujemy kursora.
pub type CursorModel = Arc<Mutex<Option<Point>>>;

/// Współdzielony numer bieżącej przestrzeni roboczej (kompozytor ↔ wątek wyjścia).
pub type WorkspaceModel = Arc<Mutex<usize>>;

/// Współdzielony tryb podglądu okien/przestrzeni (kompozytor ↔ wątek wyjścia).
pub type OverviewModel = Arc<Mutex<bool>>;

/// Margines podglądu (od krawędzi ekranu).
pub const OVERVIEW_PAD: f64 = 56.0;
/// Odstęp między kafelkami podglądu.
pub const OVERVIEW_GAP: f64 = 16.0;
/// Wysokość pasa „chipsów” przestrzeni u dołu podglądu.
pub const OVERVIEW_STRIP_H: f64 = 56.0;
/// Wysokość paska tytułu kafelka.
pub const OVERVIEW_TITLE: f64 = 26.0;

/// Sloty okien bieżącej przestrzeni (bez paneli/popupów), w kolejności z-order.
#[allow(dead_code)]
pub fn overview_slots(windows: &[Window], current_ws: usize) -> Vec<usize> {
    let mut v: Vec<(usize, i32)> = windows
        .iter()
        .enumerate()
        .filter(|(_, w)| {
            w.mapped && !w.minimized && !w.popup && !w.layer && w.workspace == current_ws
        })
        .map(|(i, w)| (i, w.z))
        .collect();
    v.sort_by_key(|&(_, z)| z);
    v.into_iter().map(|(i, _)| i).collect()
}

/// Sloty okien bieżącej przestrzeni, których **środek** leży na wyjściu `[ox, ox+w)`.
pub fn overview_slots_on(windows: &[Window], current_ws: usize, ox: f64, w: f64) -> Vec<usize> {
    let mut v: Vec<(usize, i32)> = windows
        .iter()
        .enumerate()
        .filter(|(_, win)| {
            win.mapped && !win.minimized && !win.popup && !win.layer && win.workspace == current_ws
        })
        .filter(|(_, win)| {
            let cx = win.x + win.width as f64 / 2.0;
            cx >= ox && cx < ox + w
        })
        .map(|(i, w)| (i, w.z))
        .collect();
    v.sort_by_key(|&(_, z)| z);
    v.into_iter().map(|(i, _)| i).collect()
}

/// Prostokąty kafelków w siatce na obszarze `size` (zostawiając pas u dołu).
pub fn overview_tiles(count: usize, size: (f64, f64)) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let cols = (count as f64).sqrt().ceil() as usize;
    let cols = cols.max(1);
    let rows = count.div_ceil(cols);
    let avail_w = (size.0 - 2.0 * OVERVIEW_PAD).max(1.0);
    let avail_h = (size.1 - 2.0 * OVERVIEW_PAD - OVERVIEW_STRIP_H).max(1.0);
    let tw = ((avail_w - (cols - 1) as f64 * OVERVIEW_GAP) / cols as f64).max(1.0);
    let th = ((avail_h - (rows - 1) as f64 * OVERVIEW_GAP) / rows as f64).max(1.0);
    let grid_w = cols as f64 * tw + (cols - 1) as f64 * OVERVIEW_GAP;
    let grid_h = rows as f64 * th + (rows - 1) as f64 * OVERVIEW_GAP;
    let x0 = (size.0 - grid_w) / 2.0;
    let y0 = OVERVIEW_PAD + ((avail_h - grid_h) / 2.0).max(0.0);
    (0..count)
        .map(|k| {
            let c = (k % cols) as f64;
            let r = (k / cols) as f64;
            Rect::from_xywh(
                x0 + c * (tw + OVERVIEW_GAP),
                y0 + r * (th + OVERVIEW_GAP),
                tw,
                th,
            )
        })
        .collect()
}

/// Prostokąty „chipsów” przestrzeni na dole podglądu.
pub fn overview_ws_chips(count: usize, size: (f64, f64)) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let (w, h, gap) = (128.0, 36.0, 12.0);
    let total = count as f64 * w + (count - 1) as f64 * gap;
    let x0 = (size.0 - total) / 2.0;
    let y = size.1 - OVERVIEW_PAD - h;
    (0..count)
        .map(|i| Rect::from_xywh(x0 + i as f64 * (w + gap), y, w, h))
        .collect()
}

/// Trwające przeciąganie okna.
#[derive(Debug, Clone, Copy)]
pub struct DragState {
    pub slot: usize,
    /// Przesunięcie kursora względem lewego-górnego narożnika okna w chwili wciśnięcia.
    pub off_x: f64,
    pub off_y: f64,
}

/// Trwające przeskalowanie okna (za dowolną krawędź/narożnik).
#[derive(Debug, Clone, Copy)]
pub struct ResizeState {
    pub slot: usize,
    /// Bitmaska krawędzi (1=góra, 2=dół, 4=lewo, 8=prawo).
    pub edges: u8,
    pub start_x: f64,
    pub start_y: f64,
    pub start_w: f64,
    pub start_h: f64,
    pub start_px: f64,
    pub start_py: f64,
}

/// Stan współdzielony przez wszystkie handlery protokołu.
pub struct State {
    /// Start pracy kompozytora — do znaczników czasu (frame callbacks).
    pub start: Instant,
    /// Licznik seriali (dla `xdg_surface.configure` itp.).
    serial: u32,
    /// Liczniki diagnostyczne.
    pub clients: u32,
    pub surfaces: u32,
    pub toplevels: u32,
    /// Model okien współdzielony z wątkiem wyjścia.
    pub windows: WindowModel,
    /// Pozycja kursora do narysowania (tryb DRM/evdev), współdzielona z wątkiem wyjścia.
    pub cursor: CursorModel,
    /// Powierzchnie okien (indeks = slot w [`State::windows`]).
    pub slots: Vec<WlSurface>,
    /// Licznik kolejności (z-order).
    z_counter: i32,
    /// Monitory (współdzielone z wątkiem wyjścia).
    pub outputs: Outputs,
    /// Podłączeni menedżerowie `zwlr_foreign_toplevel` (paski/taskbary).
    pub toplevel_managers: Vec<ZwlrForeignToplevelManagerV1>,

    // — Przestrzenie robocze (`ext_workspace_v1`) —
    /// Liczba przestrzeni roboczych.
    pub ws_count: usize,
    /// Bieżąca przestrzeń robocza (współdzielona z wątkiem wyjścia).
    pub ws_current: WorkspaceModel,
    /// Tryb podglądu okien/przestrzeni (`Super+Tab`), współdzielony z wątkiem wyjścia.
    pub overview: OverviewModel,
    /// Podłączeni menedżerowie `ext_workspace` oraz ich grupy i uchwyty przestrzeni.
    pub ws_managers: Vec<ExtWorkspaceManagerV1>,
    pub ws_groups: Vec<ExtWorkspaceGroupHandleV1>,
    pub ws_handles: Vec<ExtWorkspaceHandleV1>,

    // — Wejście —
    pub pointer: Option<WlPointer>,
    pub keyboard: Option<WlKeyboard>,
    pub pointer_focus: Option<(usize, WlSurface)>,
    pub keyboard_focus: Option<(usize, WlSurface)>,
    /// Modyfikatory, o których już powiadomiliśmy klienta klawiatury.
    pub sent_mods: Modifiers,
    /// Bieżąca selekcja (schowek): źródło danych.
    pub selection: Option<WlDataSource>,
    /// Urządzenia schowka zarejestrowane przez klientów (do rozsyłania selekcji).
    pub data_devices: Vec<WlDataDevice>,
    /// Stan trwającego przeciągania (drag & drop).
    pub dnd: Option<DndState>,
    /// Ostatnia pozycja wskaźnika (współrzędne globalne).
    pub pointer_pos: (f64, f64),
    /// Trwające przeciąganie okna.
    pub drag: Option<DragState>,
    /// Trwające przeskalowanie okna.
    pub resize: Option<ResizeState>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            serial: 0,
            clients: 0,
            surfaces: 0,
            toplevels: 0,
            windows: Arc::new(Mutex::new(Vec::new())),
            cursor: Arc::new(Mutex::new(None)),
            slots: Vec::new(),
            z_counter: 1,
            outputs: Arc::new(Mutex::new(vec![OutputInfo::default()])),
            toplevel_managers: Vec::new(),
            ws_count: 4,
            ws_current: Arc::new(Mutex::new(0)),
            overview: Arc::new(Mutex::new(false)),
            ws_managers: Vec::new(),
            ws_groups: Vec::new(),
            ws_handles: Vec::new(),
            pointer: None,
            keyboard: None,
            pointer_focus: None,
            keyboard_focus: None,
            sent_mods: Modifiers::NONE,
            selection: None,
            data_devices: Vec::new(),
            dnd: None,
            pointer_pos: (0.0, 0.0),
            drag: None,
            resize: None,
        }
    }
}

impl State {
    /// Kolejny serial protokołu.
    pub fn next_serial(&mut self) -> u32 {
        self.serial = self.serial.wrapping_add(1).max(1);
        self.serial
    }

    /// Czas od startu w milisekundach (dla `wl_callback.done`).
    pub fn time_ms(&self) -> u32 {
        self.start.elapsed().as_millis() as u32
    }

    /// Kolejna wartość z-order (podnosi okno na wierzch).
    pub fn next_z(&mut self) -> i32 {
        self.z_counter += 1;
        self.z_counter
    }

    /// Czy trwa podgląd okien/przestrzeni.
    pub fn overview_on(&self) -> bool {
        *self.overview.lock_or_recover()
    }

    /// Włącza/wyłącza podgląd okien/przestrzeni.
    pub fn set_overview(&self, on: bool) {
        *self.overview.lock_or_recover() = on;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serials_are_non_zero_and_increasing() {
        let mut s = State::default();
        let a = s.next_serial();
        let b = s.next_serial();
        assert!(a >= 1);
        assert!(b > a);
    }

    #[test]
    fn z_order_increases() {
        let mut s = State::default();
        let a = s.next_z();
        let b = s.next_z();
        assert!(b > a);
    }

    fn window_at_ws(ws: usize, z: i32) -> Window {
        Window {
            x: 0.0,
            y: 0.0,
            width: 10,
            height: 10,
            z,
            title: String::new(),
            popup: false,
            layer: false,
            mapped: true,
            workspace: ws,
            minimized: false,
            maximized: false,
            fullscreen: false,
            saved: None,
            data: std::sync::Arc::new(libluna_render::ImageData::empty(1, 1)),
        }
    }

    #[test]
    fn overview_slots_filter_and_order() {
        let mut panel = window_at_ws(0, 100);
        panel.layer = true;
        let ws = vec![
            window_at_ws(0, 5),
            window_at_ws(1, 9),
            window_at_ws(0, 1),
            panel,
        ];
        // Tylko okna bieżącej przestrzeni, bez panelu, w kolejności z.
        assert_eq!(overview_slots(&ws, 0), vec![2, 0]);
        assert_eq!(overview_slots(&ws, 1), vec![1]);
    }

    #[test]
    fn overview_slots_split_by_output() {
        let mut a = window_at_ws(0, 1);
        a.x = 100.0;
        a.width = 200; // środek 200 → monitor 0
        let mut b = window_at_ws(0, 2);
        b.x = 2000.0;
        b.width = 100; // środek 2050 → monitor 1
        let ws = vec![a, b];
        assert_eq!(overview_slots_on(&ws, 0, 0.0, 1920.0), vec![0]);
        assert_eq!(overview_slots_on(&ws, 0, 1920.0, 1280.0), vec![1]);
        assert!(overview_slots_on(&ws, 1, 0.0, 1920.0).is_empty());
    }

    #[test]
    fn overview_tiles_and_chips_layout() {
        let size = (1920.0, 1080.0);
        assert!(overview_tiles(0, size).is_empty());
        let four = overview_tiles(4, size);
        assert_eq!(four.len(), 4);
        // Drugi kafelek na prawo od pierwszego i bez nachodzenia.
        assert!(four[1].x() > four[0].x());
        assert!(four[0].right() <= four[1].x());
        for r in &four {
            assert!(r.x() >= 0.0 && r.right() <= size.0 + 0.5);
            assert!(r.bottom() <= size.1 + 0.5);
        }
        let chips = overview_ws_chips(4, size);
        assert_eq!(chips.len(), 4);
        assert!(chips[0].x() < chips[3].x());
    }
}
