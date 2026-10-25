//! Pojedyncze okno Wayland (`xdg-shell`) z buforem `wl_shm`, wejściem i pętlą zdarzeń.
//!
//! Kontrakt: [`run`] otwiera okno i woła domknięcie rysujące. Domknięcie otrzymuje
//! [`Frame`], który daje dostęp do sceny, motywu, silnika tekstu oraz **zdarzeń wejścia**
//! od ostatniej klatki. Motywem steruje się z poziomu domknięcia przez
//! [`Frame::set_preference`] i [`Frame::set_accent`] (zmiana jest natychmiast odzwierciedlona
//! i zapisana).
//!
//! Dekoracje są po stronie klienta (CSD) — rysujemy własny HeaderBar.

use std::cell::RefCell;
use std::error::Error;
use std::time::{Duration, Instant};

use libluna_core::{
    Accent, ButtonState, Color, Event, KeyEvent, Modifiers, Point, PointerButton, PointerEvent,
    Rect, ScrollEvent, Size, Theme, ThemePreference, ThemeSettings, WindowEvent,
};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};

use smithay_client_toolkit::reexports::calloop::{EventLoop, LoopHandle};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::csd_frame::WindowState;
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm, delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent as SctkKeyEvent, KeyboardHandler, Keysym, Modifiers as KbModifiers},
        pointer::{PointerEvent as SctkPointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        xdg::{
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell,
        },
        WaylandSurface,
    },
    shm::{
        slot::{Buffer, SlotPool},
        Shm, ShmHandler,
    },
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_region, wl_seat, wl_shm, wl_surface},
    Connection, Dispatch, QueueHandle,
};

/// Minimalny odstęp między klatkami wywołanymi **wyłącznie** ruchem kursora.
/// Bez tego każdy ruch myszy (setki zdarzeń/s) wysyłałby cały bufor 1920×680.
const MOTION_FRAME: Duration = Duration::from_millis(12);

/// Maksymalne tempo prezentacji powłoki (~60 Hz). Bez tego pętla potrafiła wysłać
/// kilkaset buforów na sekundę: każde wybudzenie przez `wl_buffer.release` kończyło się
/// kolejnym `commit`, a kompozytor dopiero co zwolnił bufor. Powłoka paliła CPU, a obraz
/// i tak nie był szybszy niż odświeżanie monitora.
const FRAME_INTERVAL: Duration = Duration::from_millis(16);

/// Domknięcie rysujące jedną klatkę (łącznie z obsługą zdarzeń).
type RenderFn = dyn for<'a> FnMut(&mut Frame<'a>);

/// Uchwyt do wybudzania pętli zdarzeń z innego wątku.
///
/// Używany np. przez powłokę po odebraniu powiadomienia D-Bus: [`WakeHandle::wake`]
/// sprawia, że okno zostanie przerysowane, mimo że nie było żadnego zdarzenia wejścia.
#[derive(Clone)]
pub struct WakeHandle {
    ping: calloop::ping::Ping,
}

impl WakeHandle {
    /// Budzi pętlę zdarzeń (bezpieczne z dowolnego wątku).
    pub fn wake(&self) {
        self.ping.ping();
    }
}

/// Konfiguracja okna.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub title: String,
    pub app_id: String,
    pub initial_size: (u32, u32),
    pub settings: ThemeSettings,
    /// Czy system zgłasza tryb ciemny (dla preferencji `System`).
    pub system_dark: bool,
    /// Gdy ustawione — zamiast okna `xdg-shell` tworzymy powierzchnię `wlr-layer-shell`
    /// (panel/dock/tło). To tryb używany przez powłokę Luna.
    pub layer: Option<LayerConfig>,
    /// Przezroczyste tło powierzchni (dla paneli/OSD warstwowych).
    pub transparent: bool,
}

/// Region wejścia powierzchni.
///
/// `Auto` pozwala kompozytorowi wyliczyć region z kanału alfa (panele) — kliknięcia
/// w przezroczyste części przechodzą do okien pod spodem. `Rects` ustawia jawny region;
/// powłoka używa go, by przy otwartym popupie przechwycić **wszystkie** kliknięcia
/// i zamknąć popup także po kliknięciu poza jego obrysem.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum InputRegion {
    #[default]
    Auto,
    Rects(Vec<Rect>),
}

/// Konfiguracja powierzchni warstwowej (`wlr-layer-shell`).
#[derive(Debug, Clone)]
pub struct LayerConfig {
    /// Przestrzeń nazw (np. „luna-panel”).
    pub namespace: String,
    /// Warstwa rysowania.
    pub layer: Layer,
    /// Kotwice krawędzi (np. `TOP|LEFT|RIGHT` = pasek na całą szerokość).
    pub anchor: Anchor,
    /// Rozmiar (0 = wybierze kompozytor/klient).
    pub size: (u32, u32),
    /// Marginesy: góra, prawa, dół, lewa.
    pub margin: (i32, i32, i32, i32),
    /// Strefa wyłączna (zarezerwowane miejsce; 0 = brak).
    pub exclusive_zone: i32,
    /// Czy panel przyjmuje fokus klawiatury na żądanie.
    pub keyboard: bool,
}

impl LayerConfig {
    /// Pasek na górnej krawędzi, na całą szerokość, o zadanej grubości.
    pub fn top_panel(namespace: impl Into<String>, thickness: u32, exclusive: bool) -> Self {
        Self {
            namespace: namespace.into(),
            layer: Layer::Top,
            anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            size: (0, thickness),
            margin: (0, 0, 0, 0),
            exclusive_zone: if exclusive { thickness as i32 } else { 0 },
            keyboard: true,
        }
    }

    /// Panel nakładkowy (OSD/notyfikacje) na środku, bez strefy wyłącznej.
    pub fn overlay(namespace: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            namespace: namespace.into(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP,
            size: (width, height),
            margin: (24, 0, 0, 0),
            exclusive_zone: 0,
            keyboard: false,
        }
    }

    /// Powierzchnia „chrome”: pasek o grubości `bar` plus przezroczysty obszar pod nim
    /// (na OSD i powiadomienia). Strefa wyłączna obejmuje tylko pasek.
    /// Gdy `total == 0`, powierzchnia rozciąga się na cały monitor (`Anchor::BOTTOM`).
    pub fn top_chrome(namespace: impl Into<String>, bar: u32, total: u32) -> Self {
        let anchor = if total == 0 {
            Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT
        } else {
            Anchor::TOP | Anchor::LEFT | Anchor::RIGHT
        };
        Self {
            namespace: namespace.into(),
            layer: Layer::Top,
            anchor,
            size: (0, total),
            margin: (0, 0, 0, 0),
            exclusive_zone: bar as i32,
            keyboard: true,
        }
    }
}

impl AppConfig {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            app_id: "eu.paganlinux.Luna.Demo".to_owned(),
            initial_size: (900, 560),
            settings: ThemeSettings::default(),
            system_dark: false,
            layer: None,
            transparent: false,
        }
    }

    /// Tworzy powierzchnię warstwową zamiast zwykłego okna.
    pub fn layer(mut self, layer: LayerConfig) -> Self {
        self.layer = Some(layer);
        self
    }

    /// Przezroczyste tło (panel/OSD warstwowy).
    pub fn transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    pub fn app_id(mut self, app_id: impl Into<String>) -> Self {
        self.app_id = app_id.into();
        self
    }

    pub fn initial_size(mut self, width: u32, height: u32) -> Self {
        self.initial_size = (width, height);
        self
    }

    pub fn settings(mut self, settings: ThemeSettings) -> Self {
        self.settings = settings;
        self
    }

    pub fn system_dark(mut self, dark: bool) -> Self {
        self.system_dark = dark;
        self
    }
}

/// Akcje okna zgłaszane przez aplikację (przez [`Frame::close`] itd.).
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowActions {
    pub close: bool,
    pub minimize: bool,
    pub toggle_maximize: bool,
    /// Żądanie rozpoczęcia przeciągania okna (klient przesuwa je przez kompozytor).
    pub move_requested: bool,
    /// Żądanie zmiany rozmiaru; bitmaska: 1 = góra, 2 = dół, 4 = lewo, 8 = prawo.
    pub resize_requested: Option<u8>,
}

/// Kontekst rysowania i obsługi zdarzeń przekazywany do domknięcia aplikacji.
///
/// Scena jest w pikselach **logicznych**; [`Frame::scale`] mówi, ile pikseli
/// fizycznych przypada na jednostkę. Zdarzenia z [`Frame::events`] dotyczą okresu
/// od poprzedniej klatki.
pub struct Frame<'a> {
    /// Scena w pikselach **logicznych**. Platforma czyści ją przed każdym wywołaniem
    /// domknięcia (aplikacja rysuje całą klatkę od zera — nie musi wołać `clear`).
    pub scene: &'a mut Scene,
    /// Rozwiązany motyw (jasny/ciemny + akcent) dla bieżącej klatki.
    pub theme: Theme,
    pub text: &'a TextEngine,
    pub size: Size,
    pub scale: f64,
    /// Zdarzenia wejścia od poprzedniej klatki.
    pub events: &'a [Event],
    /// Czas od poprzedniej klatki (sekundy) — dla animacji (`advance(dt, …)`).
    pub dt: f64,
    settings: &'a mut ThemeSettings,
    actions: &'a mut WindowActions,
    frame_requested: &'a mut bool,
    /// Termin kolejnej klatki (np. zegar), niezależny od animacji.
    frame_deadline: &'a mut Option<Instant>,
    motion_redraw: &'a mut bool,
    input_region: &'a RefCell<InputRegion>,
    system_dark: bool,
    /// Uchwyt do wybudzania pętli z innego wątku (np. po powiadomieniu D-Bus).
    waker: WakeHandle,
}

impl Frame<'_> {
    /// Prosi o zamknięcie okna.
    pub fn close(&mut self) {
        self.actions.close = true;
    }

    /// Prosi o zminimalizowanie okna.
    pub fn minimize(&mut self) {
        self.actions.minimize = true;
    }

    /// Przełącza maksymalizację okna.
    pub fn toggle_maximize(&mut self) {
        self.actions.toggle_maximize = true;
    }

    /// Rozpoczyna przeciąganie okna. Wywołaj przy wciśnięciu lewego przycisku na
    /// własnym pasku tytułu (CSD) — dalszy ruch wykonuje już kompozytor, więc okno
    /// nie „ucieka” względem kursora i działa też poza jego powierzchnią.
    ///
    /// Ma sens tylko dla zwykłych okien (`xdg_toplevel`), nie dla powierzchni
    /// warstwowych.
    pub fn begin_move(&mut self) {
        self.actions.move_requested = true;
    }

    /// Rozpoczyna zmianę rozmiaru okna z podanej krawędzi/rogu.
    ///
    /// `edges` to bitmaska: 1 = góra, 2 = dół, 4 = lewo, 8 = prawo (jak w xdg-shell).
    pub fn begin_resize(&mut self, edges: u8) {
        if edges & 0xf != 0 {
            self.actions.resize_requested = Some(edges & 0xf);
        }
    }

    /// Czas od poprzedniej klatki w sekundach (do kroku animacji).
    pub fn delta_time(&self) -> f64 {
        self.dt
    }

    /// Prosi o kolejną klatkę — używane, gdy trwa animacja (utrzymuje płynność).
    /// Dopóki aplikacja nie zawoła tej metody, okno odświeża się tylko na zdarzenia.
    pub fn request_frame(&mut self) {
        *self.frame_requested = true;
    }

    /// Prosi o klatkę po `delay` — np. zegar w panelu albo okresowe sprawdzanie stanu.
    /// Jeśli termin jest już ustawiony wcześniej, zostaje zachowany.
    pub fn request_frame_after(&mut self, delay: Duration) {
        let deadline = Instant::now() + delay;
        match *self.frame_deadline {
            Some(current) if current <= deadline => {}
            _ => *self.frame_deadline = Some(deadline),
        }
    }

    /// Włącza/wyłącza odświeżanie na **sam ruch wskaźnika** (np. podświetlenie pod
    /// kursorem w launcherze). Domyślnie wyłączone dla powierzchni warstwowych, bo
    /// ruch myszy potrafi wysyłać setki zdarzeń na sekundę i wywoływać lawinę
    /// commitów buforów. Powłoka włącza to tylko, gdy otwarte jest menu/launcher.
    pub fn set_motion_redraw(&mut self, on: bool) {
        *self.motion_redraw = on;
    }

    /// Czy włączone jest odświeżanie na ruch wskaźnika.
    pub fn motion_redraw(&self) -> bool {
        *self.motion_redraw
    }

    /// Ustawia region wejścia powierzchni (patrz [`InputRegion`]). Zmiana jest
    /// zastosowana przy najbliższym `commit` (czyli w tej samej klatce).
    ///
    /// Metoda bierze `&self`, bo powłoka wywołuje ją, trzymając już wypożyczone
    /// `frame.text`/`frame.events`.
    pub fn set_input_region(&self, region: InputRegion) {
        *self.input_region.borrow_mut() = region;
    }
    /// Bieżące ustawienia wyglądu (kopia).
    pub fn settings(&self) -> ThemeSettings {
        self.settings.clone()
    }

    /// Mierzy tekst w pikselach logicznych (szerokość i wysokość).
    pub fn measure_text(&self, text: &str, size: f64, weight: u16) -> Size {
        self.text.measure(text, size, weight)
    }

    /// Bieżąca preferencja trybu.
    pub fn preference(&self) -> ThemePreference {
        self.settings.preference()
    }

    /// Ustawia preferencję trybu (auto/jasny/ciemny); zmiana jest zapisywana.
    pub fn set_preference(&mut self, preference: ThemePreference) {
        self.settings.set_preference(preference);
        let _ = self.settings.save();
    }

    /// Bieżący kolor akcentu.
    pub fn accent(&self) -> Accent {
        self.settings.accent()
    }

    /// Ustawia kolor akcentu; zmiana jest zapisywana.
    pub fn set_accent(&mut self, accent: Accent) {
        self.settings.set_accent(accent);
        let _ = self.settings.save();
    }

    /// Nazwa wybranego stylu (instalowanego z pliku), jeśli ustawiona.
    pub fn style(&self) -> Option<&str> {
        self.settings.style()
    }

    /// Ustawia styl (instalowany z pliku); zapisuje i przerysowuje.
    pub fn set_style(&mut self, style: Option<String>) {
        self.settings.set_style(style);
        let _ = self.settings.save();
    }

    /// Czy system zgłasza tryb ciemny.
    pub const fn system_dark(&self) -> bool {
        self.system_dark
    }

    /// Uchwyt do wybudzania pętli zdarzeń z innego wątku.
    pub fn waker(&self) -> WakeHandle {
        self.waker.clone()
    }
}

/// Otwiera okno i uruchamia pętlę zdarzeń, wołając `render` przy każdej potrzebie
/// odświeżenia (start, zmiana rozmiaru/skali, zdarzenia wejścia, zmiana motywu).
///
/// Funkcja wraca, gdy okno zostanie zamknięte (przez użytkownika lub `Escape`).
/// Ustawia `WAYLAND_DISPLAY`, gdy go brak (terminal uruchomiony w sesji X11).
///
/// Kolejność: zmienna → marker Selene (`luna-wayland`) → `wayland-0`/`wayland-1`.
/// Dzięki temu `luna-files`, `luna-demo` i inne aplikacje libLuny działają także
/// z terminala odziedziczonego po X11, gdzie `WAYLAND_DISPLAY` nie jest ustawione.
fn ensure_wayland_display() -> Result<(), Box<dyn Error>> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty()) {
        return Ok(());
    }
    let rt = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .ok_or("brak XDG_RUNTIME_DIR i WAYLAND_DISPLAY — uruchom aplikację w sesji Luny")?;

    let mut names: Vec<String> = Vec::new();
    if let Ok(name) = std::fs::read_to_string(rt.join("luna-wayland")) {
        let name = name.trim().to_owned();
        if !name.is_empty() {
            names.push(name);
        }
    }
    names.push("wayland-0".to_owned());
    names.push("wayland-1".to_owned());

    for name in names {
        let path = if name.starts_with('/') {
            std::path::PathBuf::from(&name)
        } else {
            rt.join(&name)
        };
        if path.exists() {
            std::env::set_var("WAYLAND_DISPLAY", &name);
            return Ok(());
        }
    }
    Err(format!(
        "Nie znaleziono kompozytora Wayland (brak WAYLAND_DISPLAY i gniazda w {}). Uruchom aplikację w sesji Luny.",
        rt.display()
    )
    .into())
}

pub fn run<F>(config: AppConfig, render: F) -> Result<(), Box<dyn Error>>
where
    F: for<'a> FnMut(&mut Frame<'a>) + 'static,
{
    // Klient potrzebuje `XDG_RUNTIME_DIR`, żeby znaleźć gniazdo kompozytora. Menedżer
    // sesji zwykle go ustawia; gdy brak (np. zepsuty pam_systemd), wybieramy wspólny
    // katalog — ten sam co kompozytor — żeby uniknąć `NoCompositor`.
    if let Ok(Some(dir)) = libluna_core::runtime::ensure_runtime_dir() {
        eprintln!("libLuna: XDG_RUNTIME_DIR not set, using {}", dir.display());
    }
    // `wayland-client` **nie** zgaduje nazwy gniazda: bez `WAYLAND_DISPLAY` kończy
    // się `NoCompositor`. Terminale uruchamiane przez powłokę dostają czyste
    // środowisko X11, więc odnajdujemy gniazdo Selene po markerze / `wayland-0`.
    ensure_wayland_display()?;
    let conn = Connection::connect_to_env()?;
    let (globals, event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();

    let mut event_loop: EventLoop<App> = EventLoop::try_new()?;
    let loop_handle = event_loop.handle();
    WaylandSource::new(conn.clone(), event_queue).insert(loop_handle)?;

    // Wybudzanie pętli z innego wątku (np. powiadomienia z magistrali D‑Bus).
    let (ping, ping_source) = calloop::ping::make_ping()?;
    event_loop
        .handle()
        .insert_source(ping_source, |_, _, app: &mut App| app.needs_redraw = true)?;
    let wake = WakeHandle { ping };

    let compositor = CompositorState::bind(&globals, &qh)
        .map_err(|e| format!("wl_compositor unavailable: {e}"))?;
    let shm = Shm::bind(&globals, &qh).map_err(|e| format!("wl_shm unavailable: {e}"))?;

    // Stan wyjść tworzymy **przed** powierzchnią warstwową — pozwala to wybrać monitor,
    // do którego kotwiczymy panel (`LUNA_OUTPUT=<indeks>`). Dzięki temu każdy monitor ma
    // własną instancję powłoki, a nie kopię tego samego bufora.
    let output_state = OutputState::new(&globals, &qh);
    let chosen_output: Option<wl_output::WlOutput> = std::env::var("LUNA_OUTPUT")
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .and_then(|idx| output_state.outputs().nth(idx));

    let surface = compositor.create_surface(&qh);
    // Dwa tryby: zwykłe okno `xdg-shell` (CSD) albo powierzchnia warstwowa (panel/dock).
    let target = match &config.layer {
        Some(layer_cfg) => {
            let layer_shell = LayerShell::bind(&globals, &qh)
                .map_err(|e| format!("wlr-layer-shell unavailable: {e}"))?;
            let ls = layer_shell.create_layer_surface(
                &qh,
                surface,
                layer_cfg.layer,
                Some(layer_cfg.namespace.clone()),
                chosen_output.as_ref(),
            );
            ls.set_anchor(layer_cfg.anchor);
            ls.set_size(layer_cfg.size.0, layer_cfg.size.1);
            ls.set_margin(
                layer_cfg.margin.0,
                layer_cfg.margin.1,
                layer_cfg.margin.2,
                layer_cfg.margin.3,
            );
            ls.set_exclusive_zone(layer_cfg.exclusive_zone);
            ls.set_keyboard_interactivity(if layer_cfg.keyboard {
                KeyboardInteractivity::OnDemand
            } else {
                KeyboardInteractivity::None
            });
            // Pierwszy commit bez bufora — kompozytor odpowie konfiguracją (docelowy rozmiar).
            ls.commit();
            Target::Layer(ls)
        }
        None => {
            let xdg_shell =
                XdgShell::bind(&globals, &qh).map_err(|e| format!("xdg-shell unavailable: {e}"))?;
            // CSD: sami rysujemy HeaderBar (brak dekoracji serwera).
            let window = xdg_shell.create_window(surface, WindowDecorations::ClientOnly, &qh);
            window.set_title(&config.title);
            window.set_app_id(&config.app_id);
            window.set_min_size(Some((380, 260)));
            // Pierwszy commit bez bufora — kompozytor odpowie konfiguracją.
            window.commit();
            Target::Window(window)
        }
    };

    let (iw, ih) = match &config.layer {
        Some(l) => (l.size.0.max(1), l.size.1.max(1)),
        None => config.initial_size,
    };
    let pool = SlotPool::new((iw as usize * ih as usize * 4).max(1024), &shm)?;

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state,
        shm,
        loop_handle: event_loop.handle(),
        exit: false,
        pool,
        width: iw,
        height: ih,
        scale: 1.0,
        system_dark: config.system_dark,
        system_dark_config: config.system_dark,
        theme_stamp: None,
        transparent: config.transparent,
        buffer: None,
        buffer_format: None,
        target,
        configured: config.layer.is_none(),
        compositor: compositor.clone(),
        qh: qh.clone(),
        keyboard: None,
        pointer: None,
        seat: None,
        last_serial: 0,
        modifiers: Modifiers::NONE,
        renderer: SoftwareRenderer::new(iw, ih),
        settings: config.settings,
        scene: Scene::new(),
        render: Box::new(render),
        pending_events: Vec::new(),
        needs_redraw: true,
        actions: WindowActions::default(),
        frame_requested: false,
        frame_deadline: None,
        motion_redraw: false,
        input_region: RefCell::new(InputRegion::Auto),
        // Domyślnie regionu **nie dotykamy** (okna mają pełny obszar, panele liczą go
        // z alfy). Dopiero jawna zmiana tworzy `wl_region`.
        applied_input_region: Some(InputRegion::Auto),
        stats: std::env::var("LUNA_STATS").is_ok(),
        stat_frames: 0,
        stat_ms_draw: 0.0,
        stat_ms_present: 0.0,
        stat_report: Instant::now(),
        last_tick: Instant::now(),
        last_present: Instant::now() - Duration::from_millis(100),
        maximized: false,
        wake,
    };

    while !app.exit {
        // Gdy trwa animacja (aplikacja prosi o klatki), odświeżamy płynnie (~60 Hz).
        let mut timeout = if app.needs_redraw {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(100)
        };
        // Ruch kursora nie może wysyłać bufora częściej niż ~80 Hz — inaczej setki
        // zdarzeń na sekundę zamieniają powłokę w młynkiem commitów (1–5 FPS).
        if app.needs_redraw && app.motion_only_pending() {
            let since = app.last_present.elapsed();
            if since < MOTION_FRAME {
                timeout = timeout.min(MOTION_FRAME - since);
            }
        }
        // Ogólny limit ~60 Hz: dotyczy też animacji i odpowiedzi na zdarzenia.
        if app.needs_redraw {
            let since = app.last_present.elapsed();
            if since < FRAME_INTERVAL {
                timeout = timeout.min(FRAME_INTERVAL - since);
            }
        }
        // Zaplanowana klatka (np. zegar co kilka sekund) — budzimy pętlę na czas.
        if let Some(deadline) = app.frame_deadline {
            let now = Instant::now();
            if now >= deadline {
                app.frame_deadline = None;
                app.needs_redraw = true;
            } else {
                timeout = timeout.min(deadline - now);
            }
        }
        event_loop.dispatch(timeout, &mut app)?;
        // Motyw/system „auto” mogą zmienić się w innym procesie — sprawdzaj przy każdym
        // przebudzeniu pętli (stat pliku jest tani), żeby zmiana działała też bez zdarzeń.
        app.refresh_appearance();
        if app.needs_redraw && app.configured {
            if app.motion_only_pending() && app.last_present.elapsed() < MOTION_FRAME {
                // Za wcześnie — kolejna iteracja pętli dokończy klatkę.
                continue;
            }
            if app.last_present.elapsed() < FRAME_INTERVAL {
                // Kompozytor nie pokaże szybciej niż monitor i tak; nie zalewajmy go.
                continue;
            }
            app.redraw();
        }
    }
    Ok(())
}

struct App {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    #[allow(dead_code)]
    loop_handle: LoopHandle<'static, App>,
    exit: bool,
    pool: SlotPool,
    width: u32,
    height: u32,
    scale: f64,
    system_dark: bool,
    /// Wartość z `AppConfig` (jawny override). Reszta pochodzi z systemu (`auto`).
    system_dark_config: bool,
    /// Znacznik czasu `theme.conf` — wykrywa zmiany motywu w innych procesach.
    theme_stamp: Option<std::time::SystemTime>,
    /// Czy tło powierzchni ma być przezroczyste (panele/OSD).
    transparent: bool,
    buffer: Option<Buffer>,
    /// Format wybrany dla buforów shm (`Abgr/Xbgr` = bez swizzle; `Argb` = fallback).
    buffer_format: Option<wl_shm::Format>,
    target: Target,
    /// Powierzchnia warstwowa ma sens dopiero po pierwszej konfiguracji.
    configured: bool,
    compositor: CompositorState,
    /// Kolejka zdarzeń — potrzebna do tworzenia regionów wejścia w trakcie działania.
    qh: QueueHandle<App>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    /// Ostatni `wl_seat` — potrzebny do `xdg_toplevel.move`/`resize`.
    seat: Option<wl_seat::WlSeat>,
    /// Serial ostatniego zdarzenia wskaźnika (wymagany przez `move`/`resize`).
    last_serial: u32,
    modifiers: Modifiers,
    renderer: SoftwareRenderer,
    settings: ThemeSettings,
    scene: Scene,
    render: Box<RenderFn>,
    pending_events: Vec<Event>,
    needs_redraw: bool,
    actions: WindowActions,
    /// Aplikacja poprosiła o kolejną klatkę (animacja w toku).
    frame_requested: bool,
    /// Termin kolejnej klatki zaplanowanej przez aplikację (np. zegar).
    frame_deadline: Option<Instant>,
    /// Czy sam ruch kursora ma wywoływać przerysowanie (patrz [`Frame::set_motion_redraw`]).
    motion_redraw: bool,
    /// Żądany region wejścia powierzchni.
    input_region: RefCell<InputRegion>,
    /// Region faktycznie ustawiony (żeby nie tworzyć nowego `wl_region` co klatkę).
    applied_input_region: Option<InputRegion>,
    /// Diagnostyka (`LUNA_STATS=1`): koszt rysowania i wysyłki bufora.
    stats: bool,
    stat_frames: u64,
    stat_ms_draw: f64,
    stat_ms_present: f64,
    stat_report: Instant,
    /// Znacznik czasu poprzedniej klatki (do liczenia `dt`).
    last_tick: Instant,
    /// Kiedy ostatnio wysłaliśmy bufor (limit klatek na ruch kursora).
    last_present: Instant,
    maximized: bool,
    /// Uchwyt wybudzania pętli (np. z wątku D‑Bus).
    wake: WakeHandle,
}

/// Powierzchnia docelowa: zwykłe okno albo powierzchnia warstwowa (panel/dock).
enum Target {
    Window(Window),
    Layer(LayerSurface),
}

impl Target {
    fn surface(&self) -> &wl_surface::WlSurface {
        match self {
            Target::Window(w) => w.wl_surface(),
            Target::Layer(l) => l.wl_surface(),
        }
    }

    fn commit(&self) {
        match self {
            Target::Window(w) => w.commit(),
            Target::Layer(l) => l.commit(),
        }
    }
}

impl App {
    fn queue(&mut self, event: Event) {
        let is_motion = matches!(event, Event::PointerMotion { .. } | Event::PointerLeave);
        let is_layer = matches!(&self.target, Target::Layer(_));
        self.pending_events.push(event);
        // Dla powierzchni warstwowych (np. powłoka luna-shell) sam ruch kursora nie
        // wymusza przerysowania — chyba że aplikacja jawnie o to poprosi
        // ([`Frame::set_motion_redraw`], np. gdy otwarty jest launcher i chcemy
        // podświetlać pozycję pod kursorem). Zapobiega to lawinie commitów buforów
        // 1920x1080 i spadkowi wydajności do 1-5 FPS.
        if !(is_layer && is_motion) || self.motion_redraw {
            self.needs_redraw = true;
        }
    }

    /// Wykrywa zmiany motywu zapisane przez inny proces (np. Quick Panel w powłoce)
    /// i aktualizuje stan „system ciemny” dla trybu Auto.
    fn refresh_appearance(&mut self) {
        let stamp = ThemeSettings::config_path()
            .and_then(|p| std::fs::metadata(p).ok())
            .and_then(|m| m.modified().ok());
        if stamp.is_some() && stamp != self.theme_stamp {
            self.theme_stamp = stamp;
            self.settings = ThemeSettings::load();
            self.needs_redraw = true;
            if self.stats {
                eprintln!("luna-platform: przeładowano motyw z pliku");
            }
        }
        let effective = self.system_dark_config || libluna_core::appearance::system_dark();
        if effective != self.system_dark {
            self.system_dark = effective;
            self.needs_redraw = true;
            if self.stats {
                eprintln!("luna-platform: system dark = {effective}");
            }
        }
    }

    fn redraw(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_tick).as_secs_f64().clamp(0.0, 0.1);
        self.last_tick = now;

        let events = std::mem::take(&mut self.pending_events);
        self.needs_redraw = false;
        self.actions = WindowActions::default();
        self.frame_requested = false;

        self.settings.clear_dirty();
        let mut theme = self.settings.resolve(self.system_dark);
        let t_draw = Instant::now();
        self.render_frame(theme, &events, dt);

        // Jeśli domknięcie zmieniło motyw w trakcie, przerysuj bez zdarzeń.
        let mut guard = 0;
        while self.settings.is_dirty() && guard < 4 {
            self.settings.clear_dirty();
            theme = self.settings.resolve(self.system_dark);
            self.render_frame(theme, &[], dt);
            guard += 1;
        }
        if self.settings.is_dirty() {
            self.settings.clear_dirty();
            self.needs_redraw = true;
        }

        let is_layer = matches!(&self.target, Target::Layer(_));
        let motion_only = !events.is_empty()
            && events
                .iter()
                .all(|e| matches!(e, Event::PointerMotion { .. } | Event::PointerLeave));
        // Region wejścia ustawiamy **przed** ewentualnym wczesnym powrotem — zmiana
        // regionu (otwarcie/zamknięcie popupu) musi zostać wysłana do kompozytora.
        let input_changed = self.apply_input_region();
        if is_layer && !self.frame_requested && !self.motion_redraw && motion_only {
            // Brak żądania nowej klatki i tylko ruchy myszy — nie wysyłaj bufora do
            // kompozytora. Gdy powłoka prosi o odświeżanie na ruch (`motion_redraw`,
            // otwarty launcher/popup), klatkę jednak wysyłamy — inaczej podświetlenie
            // pod kursorem nie nadążało za myszą.
            if input_changed {
                self.target.commit();
            }
            return;
        }

        let t_present = Instant::now();
        self.present();
        let ms_present = t_present.elapsed().as_secs_f64() * 1000.0;
        self.apply_actions();

        // Diagnostyka (`LUNA_STATS=1`): ile kosztuje rysowanie sceny, a ile wysyłka
        // bufora do kompozytora. To rozstrzyga, czy wąskim gardłem jest powłoka, czy
        // samo `present` (konwersja + `commit`).
        if self.stats {
            self.stat_ms_draw += t_draw.elapsed().as_secs_f64() * 1000.0 - ms_present;
            self.stat_ms_present += ms_present;
            self.stat_frames += 1;
            let secs = self.stat_report.elapsed().as_secs_f64();
            if secs >= 2.0 {
                let n = self.stat_frames.max(1) as f64;
                eprintln!(
                    "luna-platform: {:.1} prezentacji/s | rysowanie {:.2} ms, wysyłka {:.2} ms",
                    self.stat_frames as f64 / secs,
                    self.stat_ms_draw / n,
                    self.stat_ms_present / n
                );
                self.stat_frames = 0;
                self.stat_ms_draw = 0.0;
                self.stat_ms_present = 0.0;
                self.stat_report = Instant::now();
            }
        }

        // Aplikacja poprosiła o kolejną klatkę (animacja) — odśwież ponownie.
        if self.frame_requested {
            self.needs_redraw = true;
        }
    }

    /// Wykonuje akcje okna zgłoszone w trakcie klatki.
    fn apply_actions(&mut self) {
        if self.actions.close {
            if std::env::var("LUNA_DEBUG_INPUT").is_ok() {
                eprintln!("platform: actions.close → exit");
            }
            self.exit = true;
        }
        let (want_move, want_resize) = (self.actions.move_requested, self.actions.resize_requested);
        if std::env::var("LUNA_DEBUG_INPUT").is_ok() && (want_move || want_resize.is_some()) {
            eprintln!(
                "platform: actions move={want_move} resize={want_resize:?} seat={} target_window={}",
                self.seat.is_some(),
                matches!(&self.target, Target::Window(_))
            );
        }
        if let Target::Window(window) = &self.target {
            if self.actions.minimize {
                window.set_minimized();
            }
            if self.actions.toggle_maximize {
                self.maximized = !self.maximized;
                if self.maximized {
                    window.set_maximized();
                } else {
                    window.unset_maximized();
                }
            }
            if want_move || want_resize.is_some() {
                if let Some(seat) = self.seat.clone() {
                    let serial = self.last_serial;
                    if std::env::var("LUNA_DEBUG_INPUT").is_ok() {
                        eprintln!(
                            "platform: move={want_move} resize={want_resize:?} serial={serial}"
                        );
                    }
                    if want_move {
                        window.move_(&seat, serial);
                    } else if let Some(edges) = want_resize {
                        window.resize(&seat, serial, resize_edge(edges));
                    }
                }
            }
        }
    }

    fn render_frame(&mut self, theme: Theme, events: &[Event], dt: f64) {
        let (width, height, scale, system_dark) =
            (self.width, self.height, self.scale, self.system_dark);
        let phys_w = (width as f64 * scale).round().max(1.0) as u32;
        let phys_h = (height as f64 * scale).round().max(1.0) as u32;
        self.renderer.resize(phys_w, phys_h);

        // Scena jest wielokrotnego użytku, więc **musi** zostać wyczyszczona przed każdym
        // wywołaniem domknięcia. Bez tego komendy rysowania sumowały się z klatki na
        // klatkę i powłoka zwalniała z każdą sekundą (panel schodził do 1–5 FPS).
        // Kanarek: gdyby ktoś usunął czyszczenie, build z `debug_assertions` to zgłosi.
        debug_assert!(
            self.scene.is_empty(),
            "scena nie została wyczyszczona po poprzedniej klatce"
        );
        self.scene.clear();

        {
            let App {
                render,
                scene,
                renderer,
                settings,
                actions,
                frame_requested,
                frame_deadline,
                motion_redraw,
                input_region,
                wake,
                ..
            } = self;
            let mut frame = Frame {
                scene,
                theme,
                text: renderer.text_engine(),
                size: Size::new(width as f64, height as f64),
                scale,
                events,
                dt,
                settings,
                actions,
                frame_requested,
                frame_deadline,
                motion_redraw,
                input_region: &*input_region,
                system_dark,
                waker: wake.clone(),
            };
            render(&mut frame);
        }

        // Panele/OSD warstwowe mają przezroczyste tło (widać pulpit pod spodem).
        let bg = if self.transparent {
            Color::rgba(0.0, 0.0, 0.0, 0.0)
        } else {
            theme.colors.window_bg
        };

        // Klatka zawierająca wyłącznie ruch kursora, pusta scena i brak żądania klatki:
        // nie ma czego rysować ani wysyłać. Wypełnianie bufora (1920x680 ≈ 5 MB)
        // dla każdego zdarzenia myszy zapychało procesor bez żadnego efektu.
        let motion_only = events
            .iter()
            .all(|e| matches!(e, Event::PointerMotion { .. } | Event::PointerLeave));
        let is_layer = matches!(&self.target, Target::Layer(_));
        if self.transparent
            && is_layer
            && self.scene.is_empty()
            && motion_only
            && !self.frame_requested
        {
            return;
        }

        self.renderer.draw(&self.scene, scale, bg);
    }

    fn present(&mut self) {
        let (width, height, scale) = (self.width, self.height, self.scale);
        let phys_w = (width as f64 * scale).round().max(1.0) as u32;
        let phys_h = (height as f64 * scale).round().max(1.0) as u32;
        let stride = phys_w as i32 * 4;

        // Preferujemy format natywny dla renderera (`RGBA` w pamięci): `Abgr8888`
        // (z alfą, panele) albo `Xbgr8888` (okna nieprzezroczyste). Dzięki temu
        // `present` to zwykły `memcpy`, a nie podmiana R↔B na 5 MB każdą klatkę.
        // Gdy kompozytor ich nie ogłasza — zostaje `Argb8888` + swizzle.
        let format = match self.buffer_format {
            Some(f) => f,
            None => {
                let want = if self.transparent {
                    wl_shm::Format::Abgr8888
                } else {
                    wl_shm::Format::Xbgr8888
                };
                let f = if self.shm.formats().contains(&want) {
                    want
                } else {
                    wl_shm::Format::Argb8888
                };
                self.buffer_format = Some(f);
                if self.stats {
                    eprintln!("luna-platform: format shm {:?}", f);
                }
                f
            }
        };

        let buffer = self.buffer.get_or_insert_with(|| {
            self.pool
                .create_buffer(phys_w as i32, phys_h as i32, stride, format)
                .expect("shm buffer allocation")
                .0
        });

        let canvas = match self.pool.canvas(buffer) {
            Some(canvas) => canvas,
            None => {
                // Kompozytor wciąż trzyma poprzedni bufor — potrzebny drugi (double buffering).
                let (second, canvas) = self
                    .pool
                    .create_buffer(phys_w as i32, phys_h as i32, stride, format)
                    .expect("second shm buffer allocation");
                *buffer = second;
                canvas
            }
        };

        let pixels = self.renderer.data();
        let count = (phys_w as usize * phys_h as usize)
            .min(canvas.len() / 4)
            .min(pixels.len() / 4);
        let direct = matches!(format, wl_shm::Format::Abgr8888 | wl_shm::Format::Xbgr8888);
        if direct {
            // Pamięć już w kolejności RGBA — kopiujemy całość jednym `memcpy`.
            canvas[..count * 4].copy_from_slice(&pixels[..count * 4]);
        } else {
            #[cfg(target_endian = "little")]
            {
                // Po 4 bajty naraz (u32): podmiana R↔B to kilka operacji bitowych.
                // Bajt-po-bajcie na 1920×680 zajmowało ~2.5 ms **na każdą klatkę**.
                let src32 =
                    unsafe { std::slice::from_raw_parts(pixels.as_ptr() as *const u32, count) };
                let dst32 = unsafe {
                    std::slice::from_raw_parts_mut(canvas.as_mut_ptr() as *mut u32, count)
                };
                for (d, &s) in dst32.iter_mut().zip(src32.iter()) {
                    *d = (s & 0xFF00_FF00) | ((s & 0x00FF_0000) >> 16) | ((s & 0x0000_00FF) << 16);
                }
            }
            #[cfg(not(target_endian = "little"))]
            {
                for (dst, src) in canvas
                    .chunks_exact_mut(4)
                    .zip(pixels.chunks_exact(4))
                    .take(count)
                {
                    dst[0] = src[2];
                    dst[1] = src[1];
                    dst[2] = src[0];
                    dst[3] = src[3];
                }
            }
        }

        let surface = self.target.surface();
        surface.damage_buffer(0, 0, phys_w as i32, phys_h as i32);
        buffer.attach_to(surface).expect("buffer attach");
        self.target.commit();
        self.last_present = Instant::now();
    }

    /// Ustawia region wejścia, gdy zmienił się od poprzedniej klatki.
    ///
    /// `InputRegion::Auto` wysyła **pusty** region — kompozytor traktuje go jako
    /// „wróć do regionu z kanału alfa”. `Rects` wysyła jawne prostokąty.
    fn apply_input_region(&mut self) -> bool {
        let desired = self.input_region.borrow().clone();
        if self.applied_input_region.as_ref() == Some(&desired) {
            return false;
        }
        let surface = self.target.surface().clone();
        let region = self.compositor.wl_compositor().create_region(&self.qh, ());
        if let InputRegion::Rects(rects) = &desired {
            for r in rects {
                region.add(
                    r.x().round() as i32,
                    r.y().round() as i32,
                    r.width().round().max(0.0) as i32,
                    r.height().round().max(0.0) as i32,
                );
            }
        }
        surface.set_input_region(Some(&region));
        region.destroy();
        self.applied_input_region = Some(desired);
        true
    }

    /// Czy oczekujące zdarzenia to wyłącznie ruch kursora (bez kliknięć/klawiszy).
    fn motion_only_pending(&self) -> bool {
        !self.pending_events.is_empty()
            && self
                .pending_events
                .iter()
                .all(|e| matches!(e, Event::PointerMotion { .. } | Event::PointerLeave))
    }
}

/// Zamienia kod przycisku evdev na typ libLuny.
fn map_button(code: u32) -> PointerButton {
    match code {
        0x110 => PointerButton::Left,
        0x111 => PointerButton::Right,
        0x112 => PointerButton::Middle,
        // Myszy raportują „wstecz/dalej” na dwa sposoby: BTN_SIDE/BTN_EXTRA
        // (0x113/0x114 — tak robi większość myszy i X11 jako 8/9) oraz
        // BTN_BACK/BTN_FORWARD (0x116/0x115). Obsługujemy oba warianty.
        0x113 | 0x116 => PointerButton::Back,
        0x114 | 0x115 => PointerButton::Forward,
        other => PointerButton::Other(other),
    }
}

/// Bitmaska krawędzi (1=góra, 2=dół, 4=lewo, 8=prawo) → `xdg_toplevel::ResizeEdge`.
fn resize_edge(bits: u8) -> xdg_toplevel::ResizeEdge {
    match bits & 0xf {
        1 => xdg_toplevel::ResizeEdge::Top,
        2 => xdg_toplevel::ResizeEdge::Bottom,
        4 => xdg_toplevel::ResizeEdge::Left,
        8 => xdg_toplevel::ResizeEdge::Right,
        5 => xdg_toplevel::ResizeEdge::TopLeft,
        9 => xdg_toplevel::ResizeEdge::TopRight,
        6 => xdg_toplevel::ResizeEdge::BottomLeft,
        10 => xdg_toplevel::ResizeEdge::BottomRight,
        _ => xdg_toplevel::ResizeEdge::None,
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        self.scale = new_factor as f64;
        self.queue(Event::Window(WindowEvent::ScaleChanged(self.scale)));
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        self.needs_redraw = true;
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl WindowHandler for App {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        if std::env::var("LUNA_DEBUG_INPUT").is_ok() {
            eprintln!(
                "platform: configure size={:?} state={:?}",
                configure.new_size, configure.state
            );
        }
        self.buffer = None;
        self.width = configure.new_size.0.map(|v| v.get()).unwrap_or(self.width);
        self.height = configure.new_size.1.map(|v| v.get()).unwrap_or(self.height);
        // Synchronizuj stan z kompozytorem — okno może zostać zmaksymalizowane
        // także z panelu/przełącznika, nie tylko przyciskiem w HeaderBarze.
        self.maximized = configure.state.contains(WindowState::MAXIMIZED)
            || configure.state.contains(WindowState::FULLSCREEN);
        self.queue(Event::Window(WindowEvent::Resize(Size::new(
            self.width as f64,
            self.height as f64,
        ))));
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.buffer = None;
        self.width = configure.new_size.0.max(1);
        self.height = configure.new_size.1.max(1);
        self.configured = true;
        self.queue(Event::Window(WindowEvent::Resize(Size::new(
            self.width as f64,
            self.height as f64,
        ))));
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.seat = Some(seat);
    }

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        // SCTK nie zawsze woła `new_seat` przy podłączaniu do istniejącej sesji —
        // jedynym pewnym miejscem, w którym mamy uchwyt do globalnego `wl_seat`,
        // jest `new_capability`. Bez tego `self.seat` zostawało `None` i żądania
        // `xdg_toplevel.move`/`resize` (przeciąganie okna za pasek) były po cichu
        // pomijane.
        if self.seat.is_none() {
            self.seat = Some(seat.clone());
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            if let Ok(keyboard) = self.seat_state.get_keyboard(qh, &seat, None) {
                self.keyboard = Some(keyboard);
            }
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            if let Ok(pointer) = self.seat_state.get_pointer(qh, &seat) {
                self.pointer = Some(pointer);
            }
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            if let Some(kbd) = self.keyboard.take() {
                kbd.release();
            }
        }
        if capability == Capability::Pointer {
            if let Some(ptr) = self.pointer.take() {
                ptr.release();
            }
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw: &[u32],
        _keysyms: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: SctkKeyEvent,
    ) {
        if event.keysym == Keysym::Escape {
            self.exit = true;
        }
        self.queue(Event::Key(KeyEvent {
            keysym: u32::from(event.keysym),
            keycode: event.raw_code,
            text: event.utf8.clone(),
            state: ButtonState::Pressed,
            modifiers: self.modifiers,
            repeat: false,
        }));
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: SctkKeyEvent,
    ) {
        self.queue(Event::Key(KeyEvent {
            keysym: u32::from(event.keysym),
            keycode: event.raw_code,
            text: None,
            state: ButtonState::Released,
            modifiers: self.modifiers,
            repeat: false,
        }));
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _serial: u32,
        modifiers: KbModifiers,
        _layout: u32,
    ) {
        self.modifiers = Modifiers {
            ctrl: modifiers.ctrl,
            alt: modifiers.alt,
            shift: modifiers.shift,
            logo: modifiers.logo,
            // SCTK nie raportuje osobno poziomu 3; stan i tak ustawia kompozytor.
            level3: false,
        };
    }
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[SctkPointerEvent],
    ) {
        for event in events {
            if &event.surface != self.target.surface() {
                continue;
            }
            let position = Point::new(event.position.0, event.position.1);
            match event.kind {
                PointerEventKind::Enter { .. } => {
                    self.queue(Event::PointerMotion {
                        position,
                        modifiers: self.modifiers,
                    });
                }
                PointerEventKind::Leave { .. } => {
                    self.queue(Event::PointerLeave);
                }
                PointerEventKind::Motion { .. } => {
                    self.queue(Event::PointerMotion {
                        position,
                        modifiers: self.modifiers,
                    });
                }
                PointerEventKind::Press { button, serial, .. } => {
                    // Serial jest wymagany przez `xdg_toplevel.move`/`resize`.
                    self.last_serial = serial;
                    self.queue(Event::PointerButton(PointerEvent {
                        position,
                        button: map_button(button),
                        state: ButtonState::Pressed,
                        modifiers: self.modifiers,
                    }));
                }
                PointerEventKind::Release { button, .. } => {
                    self.queue(Event::PointerButton(PointerEvent {
                        position,
                        button: map_button(button),
                        state: ButtonState::Released,
                        modifiers: self.modifiers,
                    }));
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    ..
                } => {
                    self.queue(Event::Scroll(ScrollEvent {
                        position,
                        dx: horizontal.absolute,
                        dy: vertical.absolute,
                    }));
                }
            }
        }
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_keyboard!(App);
delegate_pointer!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
delegate_layer!(App);
delegate_registry!(App);

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

// `wl_region` nie ma zdarzeń — potrzebny tylko do utworzenia regionu wejścia.
impl Dispatch<wl_region::WlRegion, ()> for App {
    fn event(
        _state: &mut Self,
        _proxy: &wl_region::WlRegion,
        _event: <wl_region::WlRegion as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_edges_map_to_xdg_edges() {
        assert_eq!(resize_edge(1), xdg_toplevel::ResizeEdge::Top);
        assert_eq!(resize_edge(2), xdg_toplevel::ResizeEdge::Bottom);
        assert_eq!(resize_edge(4), xdg_toplevel::ResizeEdge::Left);
        assert_eq!(resize_edge(8), xdg_toplevel::ResizeEdge::Right);
        // Kombinacje to sumy bitów, nie „albo” — to był realny błąd w pierwszej wersji.
        assert_eq!(resize_edge(1 | 4), xdg_toplevel::ResizeEdge::TopLeft);
        assert_eq!(resize_edge(1 | 8), xdg_toplevel::ResizeEdge::TopRight);
        assert_eq!(resize_edge(2 | 4), xdg_toplevel::ResizeEdge::BottomLeft);
        assert_eq!(resize_edge(2 | 8), xdg_toplevel::ResizeEdge::BottomRight);
        assert_eq!(resize_edge(0), xdg_toplevel::ResizeEdge::None);
    }

    #[test]
    fn mouse_side_buttons_map_to_back_and_forward() {
        // Większość myszy: BTN_SIDE/BTN_EXTRA (X11: 8/9).
        assert_eq!(map_button(0x113), PointerButton::Back);
        assert_eq!(map_button(0x114), PointerButton::Forward);
        // Niektóre urządzenia raportują BTN_BACK/BTN_FORWARD.
        assert_eq!(map_button(0x116), PointerButton::Back);
        assert_eq!(map_button(0x115), PointerButton::Forward);
        assert_eq!(map_button(0x112), PointerButton::Middle);
        assert_eq!(map_button(0x999), PointerButton::Other(0x999));
    }
}
