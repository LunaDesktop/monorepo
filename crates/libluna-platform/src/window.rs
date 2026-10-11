//! Pojedyncze okno Wayland (`xdg-shell`) z buforem `wl_shm`, wejściem i pętlą zdarzeń.
//!
//! Kontrakt: [`run`] otwiera okno i woła domknięcie rysujące. Domknięcie otrzymuje
//! [`Frame`], który daje dostęp do sceny, motywu, silnika tekstu oraz **zdarzeń wejścia**
//! od ostatniej klatki. Motywem steruje się z poziomu domknięcia przez
//! [`Frame::set_preference`] i [`Frame::set_accent`] (zmiana jest natychmiast odzwierciedlona
//! i zapisana).
//!
//! Dekoracje są po stronie klienta (CSD) — rysujemy własny HeaderBar.

use std::error::Error;
use std::time::{Duration, Instant};

use libluna_core::{
    Accent, ButtonState, Color, Event, KeyEvent, Modifiers, Point, PointerButton, PointerEvent,
    ScrollEvent, Size, Theme, ThemePreference, ThemeSettings, WindowEvent,
};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};

use smithay_client_toolkit::reexports::calloop::{EventLoop, LoopHandle};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
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
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    Connection, QueueHandle,
};

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
    pub fn top_chrome(namespace: impl Into<String>, bar: u32, total: u32) -> Self {
        Self {
            namespace: namespace.into(),
            layer: Layer::Top,
            anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
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
}

/// Kontekst rysowania i obsługi zdarzeń przekazywany do domknięcia aplikacji.
///
/// Scena jest w pikselach **logicznych**; [`Frame::scale`] mówi, ile pikseli
/// fizycznych przypada na jednostkę. Zdarzenia z [`Frame::events`] dotyczą okresu
/// od poprzedniej klatki.
pub struct Frame<'a> {
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

    /// Czas od poprzedniej klatki w sekundach (do kroku animacji).
    pub fn delta_time(&self) -> f64 {
        self.dt
    }

    /// Prosi o kolejną klatkę — używane, gdy trwa animacja (utrzymuje płynność).
    /// Dopóki aplikacja nie zawoła tej metody, okno odświeża się tylko na zdarzenia.
    pub fn request_frame(&mut self) {
        *self.frame_requested = true;
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
                None,
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
        output_state: OutputState::new(&globals, &qh),
        shm,
        loop_handle: event_loop.handle(),
        exit: false,
        pool,
        width: iw,
        height: ih,
        scale: 1.0,
        system_dark: config.system_dark,
        transparent: config.transparent,
        buffer: None,
        target,
        configured: config.layer.is_none(),
        keyboard: None,
        pointer: None,
        modifiers: Modifiers::NONE,
        renderer: SoftwareRenderer::new(iw, ih),
        settings: config.settings,
        scene: Scene::new(),
        render: Box::new(render),
        pending_events: Vec::new(),
        needs_redraw: true,
        actions: WindowActions::default(),
        frame_requested: false,
        last_tick: Instant::now(),
        maximized: false,
        wake,
    };

    while !app.exit {
        // Gdy trwa animacja (aplikacja prosi o klatki), odświeżamy płynnie (~60 Hz).
        let timeout = if app.needs_redraw {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(100)
        };
        event_loop.dispatch(timeout, &mut app)?;
        if app.needs_redraw && app.configured {
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
    /// Czy tło powierzchni ma być przezroczyste (panele/OSD).
    transparent: bool,
    buffer: Option<Buffer>,
    target: Target,
    /// Powierzchnia warstwowa ma sens dopiero po pierwszej konfiguracji.
    configured: bool,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
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
    /// Znacznik czasu poprzedniej klatki (do liczenia `dt`).
    last_tick: Instant,
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
        self.pending_events.push(event);
        self.needs_redraw = true;
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

        self.present();
        self.apply_actions();

        // Aplikacja poprosiła o kolejną klatkę (animacja) — odśwież ponownie.
        if self.frame_requested {
            self.needs_redraw = true;
        }
    }

    /// Wykonuje akcje okna zgłoszone w trakcie klatki.
    fn apply_actions(&mut self) {
        if self.actions.close {
            self.exit = true;
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
        }
    }

    fn render_frame(&mut self, theme: Theme, events: &[Event], dt: f64) {
        let (width, height, scale, system_dark) =
            (self.width, self.height, self.scale, self.system_dark);
        let phys_w = (width as f64 * scale).round().max(1.0) as u32;
        let phys_h = (height as f64 * scale).round().max(1.0) as u32;
        self.renderer.resize(phys_w, phys_h);

        {
            let App {
                render,
                scene,
                renderer,
                settings,
                actions,
                frame_requested,
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
        self.renderer.draw(&self.scene, scale, bg);
    }

    fn present(&mut self) {
        let (width, height, scale) = (self.width, self.height, self.scale);
        let phys_w = (width as f64 * scale).round().max(1.0) as u32;
        let phys_h = (height as f64 * scale).round().max(1.0) as u32;
        let stride = phys_w as i32 * 4;

        let buffer = self.buffer.get_or_insert_with(|| {
            self.pool
                .create_buffer(
                    phys_w as i32,
                    phys_h as i32,
                    stride,
                    wl_shm::Format::Argb8888,
                )
                .expect("shm buffer allocation")
                .0
        });

        let canvas = match self.pool.canvas(buffer) {
            Some(canvas) => canvas,
            None => {
                // Kompozytor wciąż trzyma poprzedni bufor — potrzebny drugi (double buffering).
                let (second, canvas) = self
                    .pool
                    .create_buffer(
                        phys_w as i32,
                        phys_h as i32,
                        stride,
                        wl_shm::Format::Argb8888,
                    )
                    .expect("second shm buffer allocation");
                *buffer = second;
                canvas
            }
        };

        // tiny-skia daje RGBA8888; wl_shm Argb8888 to w pamięci BGRA.
        let pixels = self.renderer.data();
        let count = (phys_w as usize * phys_h as usize).min(canvas.len() / 4);
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

        let surface = self.target.surface();
        surface.damage_buffer(0, 0, phys_w as i32, phys_h as i32);
        buffer.attach_to(surface).expect("buffer attach");
        self.target.commit();
    }
}

/// Zamienia kod przycisku evdev na typ libLuny.
fn map_button(code: u32) -> PointerButton {
    match code {
        0x110 => PointerButton::Left,
        0x111 => PointerButton::Right,
        0x112 => PointerButton::Middle,
        0x116 => PointerButton::Back,
        0x115 => PointerButton::Forward,
        other => PointerButton::Other(other),
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
        self.buffer = None;
        self.width = configure.new_size.0.map(|v| v.get()).unwrap_or(self.width);
        self.height = configure.new_size.1.map(|v| v.get()).unwrap_or(self.height);
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

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
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
                PointerEventKind::Press { button, .. } => {
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
