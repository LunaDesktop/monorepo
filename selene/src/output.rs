//! Wyjście: nested (okno na nadrzędnym kompozytorze) oraz **realne DRM/KMS** (scanout).
//!
//! Wątek nested pełni dwie role: rysuje scenę przez `libluna-render` i odbiera wejście
//! od nadrzędnego kompozytora, przekazując je do magistrali (`crate::input`).
//! Wątek DRM renderuje scenę wprost do buforów „dumb” i skanuje je na monitorach
//! (page-flip). Wejście w trybie DRM (evdev) jest kolejnym krokiem.

use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::Duration;

use libluna_core::style::weight;
use libluna_core::{Color, Point, Rect, Size, Theme, ThemeSettings};
use libluna_i18n::tr;
use libluna_platform::{run, AppConfig, Frame};
use libluna_render::gpu::GpuRenderer;
use libluna_render::{
    DrawCommand, ImageData, Renderer, Scene, SoftwareRenderer, TextEngine, TextRun,
};
use libluna_widgets::{ControlsSide, HeaderBar, PaintCtx};

use crate::drm::{DrmBackend, DrmError, PixelRect};
use crate::input::{self, Events};
use crate::monitors::{layout_horizontal, OutputInfo, Outputs};
use crate::state::{
    CornerHint, CornerKind, CursorModel, Overview, OverviewModel, ScreenshotModel, ShotUi, Window,
    WindowModel, WorkspaceCountModel, WorkspaceModel,
};
use crate::sync::LockExt;
use wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface};
use wayland_server::Resource;

/// Informuje klientów, na których wyjściach leżą ich powierzchnie (`wl_surface.enter`).
///
/// Protokół wymaga tego od kompozytora: klient dobiera na tej podstawie skalę
/// i stan „widoczności” okna. Brak tych zdarzeń nie przeszkadza w rysowaniu, ale
/// **Chromium trzyma na nich stan okien potomnych** — bez `enter` bąble rysowane
/// jako `wl_subsurface` (menu rozszerzeń, menu profilu) nie przyjmują wskaźnika:
/// podświetlenie zostaje na treści pod bąblem, a klik jest traktowany jak klik
/// poza menu (menu się zamyka).
///
/// Zdarzenia wysyłamy **tylko przy zmianie** zbioru wyjść, więc koszt w pętli
/// głównej to porównanie kilku liczb na okno.
pub fn sync_surface_outputs(state: &crate::state::State) {
    let outs: Vec<(usize, f64, f64, f64, f64)> = state
        .outputs
        .lock_or_recover()
        .iter()
        .enumerate()
        .map(|(i, o)| (i, o.x as f64, o.y as f64, o.width as f64, o.height as f64))
        .collect();
    if outs.is_empty() {
        return;
    }
    let windows = state.windows.lock_or_recover();
    // Jeden bufor na wszystkie okna — `sync_surface_outputs` chodzi w każdej
    // iteracji pętli głównej, więc alokacja `Vec` per okno to zbędny narzut.
    let mut hits: Vec<usize> = Vec::with_capacity(outs.len());
    for (slot, w) in windows.iter().enumerate() {
        let Some(surface) = state.slots.get(slot) else {
            continue;
        };
        hits.clear();
        if w.mapped && !w.minimized {
            let r = w.outer().union(&w.buffer_rect());
            for (i, x, y, ow, oh) in &outs {
                if r.right() > *x && r.x() < x + ow && r.bottom() > *y && r.y() < y + oh {
                    hits.push(*i);
                }
            }
        }
        apply_entered(state, surface, &hits);
        // Powierzchnie potomne leżą wewnątrz rodzica — dostają ten sam zbiór.
        // Bez klonowania listy: `children` i `entered_outputs` to niezależne mutexy,
        // a `apply_entered` nie sięga do listy dzieci, więc trzymanie blokady przez
        // czas kolejkowania `enter`/`leave` nie tworzy cyklu (kolejność blokad jest
        // ta sama co w `seat::child_at`: najpierw `windows`, potem `children`).
        if let Some(pd) = surface.data::<crate::compositor::SurfaceData>() {
            let children = pd.children.lock_or_recover();
            for child in children.iter() {
                apply_entered(state, child, &hits);
            }
        }
    }
}

/// Wysyła różnicę `enter`/`leave` dla jednej powierzchni.
fn apply_entered(state: &crate::state::State, surface: &WlSurface, hits: &[usize]) {
    let Some(sd) = surface.data::<crate::compositor::SurfaceData>() else {
        return;
    };
    let mut entered = sd.entered_outputs.lock_or_recover();
    if *entered == hits {
        return;
    }
    let client = surface.client();
    for idx in entered.iter() {
        if !hits.contains(idx) {
            if let Some(out) = output_res(state, client.as_ref(), *idx) {
                let _ = surface
                    .send_event(wayland_server::protocol::wl_surface::Event::Leave { output: out });
            }
        }
    }
    for idx in hits {
        if !entered.contains(idx) {
            if let Some(out) = output_res(state, client.as_ref(), *idx) {
                let _ = surface
                    .send_event(wayland_server::protocol::wl_surface::Event::Enter { output: out });
            }
        }
    }
    *entered = hits.to_vec();
}

/// Zasób `wl_output` danego klienta dla monitora `idx`.
fn output_res(
    state: &crate::state::State,
    client: Option<&wayland_server::Client>,
    idx: usize,
) -> Option<WlOutput> {
    state
        .bound_outputs
        .iter()
        .find(|(i, r)| *i == idx && r.is_alive() && r.client().as_ref() == client)
        .map(|(_, r)| r.clone())
}

/// Uruchamia wątek wyjścia (nested Wayland) o zadanym rozmiarze.
pub fn spawn(
    model: WindowModel,
    outputs: Outputs,
    events: Events,
    wake: UnixStream,
    ws: WorkspaceModel,
    overview: OverviewModel,
    screenshot: ScreenshotModel,
    present: Arc<crate::present::PresentQueue>,
    ws_count: WorkspaceCountModel,
    width: u32,
    height: u32,
) {
    std::thread::spawn(move || {
        // Tryb nested nie ma sprzętowej tablicy gamma — chowamy kafelek night lightu.
        write_gamma_flag(false);
        let mut wake = wake;
        let config = AppConfig::new("Selene")
            .app_id("eu.paganlinux.Luna.Selene")
            .initial_size(width, height)
            .settings(ThemeSettings::load())
            // Esc należy do sesji gościa — nie może zamykać okna testowego Selene.
            .escape_to_exit(false);
        let theme = user_theme();
        let text = TextEngine::system();
        // Zrzut nested: trzymamy jeden renderer przez całą sesję — tworzenie
        // kontekstu EGL przy każdym zrzucie to kilkadziesiąt ms i zbędny churn.
        let mut shot: Option<Box<dyn Renderer>> = None;
        let mut first_frame = true;
        let result = run(config, move |frame: &mut Frame| {
            // Poprzednia klatka została już pokazana przez nadrzędny kompozytor —
            // dopiero teraz wolno oddać klientom bufory z tamtej klatki.
            if first_frame {
                first_frame = false;
            } else {
                present.presented();
            }
            // Zgłoś bieżący rozmiar monitora (okno może być zmieniane przez rodzica).
            {
                let mut outs = outputs.lock_or_recover();
                if let Some(out) = outs.first_mut() {
                    out.width = frame.size.width as i32;
                    out.height = frame.size.height as i32;
                }
            }
            // Przekaż wejście do magistrali.
            input::push(&events, &mut wake, frame.events);
            // Narysuj scenę.
            let size = frame.size;
            let cur_ws = *ws.lock_or_recover();
            let ov = *overview.lock_or_recover();
            let ws_count = ws_count.load(std::sync::atomic::Ordering::Relaxed);
            build_scene(
                frame.scene,
                size,
                0.0,
                0.0,
                &model,
                cur_ws,
                ws_count,
                ov,
                true,
                None,
                true,
                &theme,
                &text,
            );
            // Klatka zostanie pokazana przez nadrzędny kompozytor po powrocie
            // z tego callbacku — od teraz liczy się jako „przekazana”.
            present.mark_submitted();
            // Zrzut ekranu także w trybie nested: kompozytor nie ma tu skanouta,
            // więc kodujemy bieżącą scenę bezpośrednio do PNG.
            if screenshot.swap(false, std::sync::atomic::Ordering::Relaxed) {
                let (w, h) = (size.width as u32, size.height as u32);
                // Zrzut przez GPU, gdy dostępne: tylko wtedy widać okna `dmabuf`
                // (CPU renderer rysuje dla nich zastępczy prostokąt).
                let renderer = shot.get_or_insert_with(|| match GpuRenderer::new(w, h) {
                    Ok(r) => Box::new(r) as Box<dyn Renderer>,
                    Err(_) => Box::new(SoftwareRenderer::new(w, h)),
                });
                renderer.resize(w, h);
                renderer.draw(frame.scene, frame.scale, Color::TRANSPARENT);
                if let Some(png) = renderer.encode_png() {
                    let stamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let path = screenshot_dir().join(format!("luna-{stamp}-nested.png"));
                    match std::fs::write(&path, png) {
                        Ok(()) => {
                            eprintln!("selene: zrzut ekranu: {}", path.display());
                            crate::screenshot::notify_saved(&path);
                        }
                        Err(e) => eprintln!("selene: nie udało się zapisać zrzutu: {e}"),
                    }
                }
            }
            // Utrzymuj klatki — model okien zmienia się poza tą pętlą.
            frame.request_frame();
        });
        // Nested rysuje do okna na nadrzędnym kompozytorze Wayland. Gdy go nie ma
        // (np. uruchomienie na X11), nie połykamy błędu — inaczej wygląda to jak
        // „kompozytor działa, ale nic nie widać”.
        if let Err(e) = result {
            eprintln!(
                "{}: {e}",
                libluna_i18n::tr("nested output unavailable (no parent compositor)")
            );
        }
    });
}

/// Otwiera backend DRM, czekając chwilę, gdy mastera trzyma jeszcze greeter LightDM.
///
/// LightDM potrafi wystartować sesję Wayland, **zanim ubije greeter** (Xorg), który
/// trzyma master DRM. Bez ponowień Selene od razu widzi `MasterBusy` i zostawia czarny
/// ekran.
///
/// Greeter oddaje master zwykle w ułamku sekundy, dlatego pierwsze próby są **gęste**
/// (20 ms) — wcześniej każde opóźnienie było płacone w krokach po 100 ms, co dawało
/// odczucie „panel wyskakuje po ~5 sekundach”. Po pół sekundy przechodzimy na 100 ms,
/// żeby nie palić procesora bez potrzeby.
fn open_drm_waiting(preferences: &libluna_core::DisplaySettings) -> Result<DrmBackend, DrmError> {
    let stats = std::env::var("SELENE_STATS").is_ok();
    let started = std::time::Instant::now();
    let deadline = started + Duration::from_secs(15);
    let mut attempt = 0u32;
    loop {
        let err = match DrmBackend::open(preferences) {
            Ok(backend) => {
                if stats && attempt > 0 {
                    eprintln!(
                        "selene/drm: przejęto ekran po {:.0} ms",
                        started.elapsed().as_secs_f64() * 1000.0
                    );
                }
                return Ok(backend);
            }
            Err(e) => e,
        };
        if std::time::Instant::now() >= deadline {
            return Err(err);
        }
        if attempt == 0 && stats {
            eprintln!("selene/drm: init nieudany ({err:?}) — czekam na zwolnienie ekranu…");
        }
        let wait_ms = if attempt < 25 { 20 } else { 100 };
        attempt += 1;
        std::thread::sleep(Duration::from_millis(wait_ms));
    }
}

/// Informuje powłokę (plikiem w `$XDG_RUNTIME_DIR`), czy night light jest dostępny.
fn write_gamma_flag(supported: bool) {
    let Some(rt) = std::env::var_os("XDG_RUNTIME_DIR") else {
        return;
    };
    let path = std::path::PathBuf::from(rt).join("luna-gamma");
    let _ = std::fs::write(path, if supported { "1" } else { "0" });
}

/// Opóźnienie do limitu ~30 FPS w trybie oszczędzania (`None` = bez limitu).
fn throttle_delay(power_saver: bool, elapsed: Duration) -> Option<Duration> {
    if !power_saver {
        return None;
    }
    const MIN: Duration = Duration::from_millis(33);
    (elapsed < MIN).then(|| MIN - elapsed)
}

/// Zapisuje migawkę monitorów dla Ustawień: `nazwa|szer|wys|hz|x|y|skala|tryby`.
///
/// Dzięki temu strona „Ekrany” zna tryby z modesetu (z odświeżaniem), a nie tylko
/// pierwszą linię z `/sys/class/drm/*/modes`. Plik nazywa się `luna-monitors`,
/// bo `luna-outputs` jest markerem sesji (liczba monitorów → ile paneli uruchomić).
fn write_outputs_snapshot(
    outputs: &[OutputInfo],
    detailed: &[(String, u32, u32, u32, Vec<libluna_core::OutputMode>)],
) {
    let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") else {
        return;
    };
    let mut text = String::from("# luna-monitors: nazwa|szer|wys|hz|x|y|skala|tryby\n");
    for (info, (_, _, _, _, modes)) in outputs.iter().zip(detailed) {
        let modes = modes
            .iter()
            .map(|mode| mode.config_value())
            .collect::<Vec<_>>()
            .join(",");
        text.push_str(&format!(
            "{}|{}|{}|{}|{}|{}|{}|{}\n",
            info.name, info.width, info.height, info.refresh_hz, info.x, info.y, info.scale, modes
        ));
    }
    let path = std::path::PathBuf::from(runtime).join("luna-monitors");
    let _ = std::fs::write(path, text);
}

/// Uruchamia wątek wyjścia DRM/KMS — renderuje scenę do buforów i skanuje na monitorach.
///
/// Wymaga bycia DRM masterem; jeśli inną sesję trzyma master, kończy bez efektu
/// (wywołujący powinien wtedy użyć wyjścia nested).
pub fn spawn_drm(
    model: WindowModel,
    cursor: CursorModel,
    cursor_image: crate::state::CursorImageModel,
    ws: WorkspaceModel,
    overview: OverviewModel,
    screenshot: crate::state::ScreenshotModel,
    present: Arc<crate::present::PresentQueue>,
    night_light: crate::state::NightLightModel,
    power_saver: crate::state::PowerSaverModel,
    outputs: Outputs,
    ws_count: WorkspaceCountModel,
    display_request: crate::state::DisplayRequestModel,
    position_request: crate::state::PositionRequestModel,
    vt_request: std::sync::Arc<std::sync::Mutex<Option<u32>>>,
) {
    std::thread::spawn(move || {
        let display = libluna_core::DisplaySettings::load();
        let mut backend = match open_drm_waiting(&display) {
            Ok(backend) => backend,
            Err(reason) => {
                // Rozróżniamy powód — inaczej „brak mastera” maskuje błędy inicjalizacji.
                let msg = match reason {
                    DrmError::MasterBusy => {
                        libluna_i18n::tr("DRM backend unavailable (master held by another session)")
                    }
                    DrmError::Resources => {
                        libluna_i18n::tr("DRM backend unavailable (cannot read card resources)")
                    }
                    DrmError::NoOutputs => {
                        libluna_i18n::tr("DRM backend unavailable (no monitor could be set up)")
                    }
                };
                eprintln!("{msg}");
                return;
            }
        };
        let mut infos = backend.outputs();
        // Night light: sprawdzamy **realne** wsparcie CRTC dla tablicy gamma i od razu
        // wgrywamy zapisaną rampę (to zarazem test ioctla). Flaga w katalogu runtime
        // mówi powłoce, czy kafelek „Tryb nocny” jest w ogóle dostępny.
        let initial_night = display.night_light;
        night_light.store(initial_night, std::sync::atomic::Ordering::Relaxed);
        let gamma_ok = backend.supports_night_light() && backend.set_night_light(initial_night);
        write_gamma_flag(gamma_ok);
        if std::env::var("SELENE_STATS").is_ok() {
            eprintln!(
                "selene/drm: night light {}",
                if gamma_ok {
                    "dostępny (gamma LUT)"
                } else {
                    "niedostępny"
                }
            );
        }
        // **Faktyczne** tryby z modesetu trafiają do wspólnego modelu monitorów.
        // Wcześniej model zostawał przy wyniku wczesnego `discover()`; gdy kernel
        // zgłosił inny tryb (albo inne złącze), panel i okna miały złą geometrię,
        // a drugi monitor potrafił dostać nie-natywną rozdzielczość.
        let mut origins: Vec<(f64, f64)>;
        {
            let detailed = backend.outputs_with_modes();
            let mut list: Vec<OutputInfo> = infos
                .iter()
                .map(|(name, w, h, hz)| OutputInfo {
                    name: name.clone(),
                    width: *w as i32,
                    height: *h as i32,
                    refresh_hz: *hz as i32,
                    scale: if *w >= 3840 || *h >= 2160 { 2 } else { 1 },
                    x: 0,
                    y: 0,
                })
                .collect();
            // Układ zapisany przez użytkownika (`output <złącze> position = x,y`).
            // Brakujące pozycje dokładamy po prawej stronie zapisanych.
            let mut positioned = false;
            let mut right = 0;
            for info in &mut list {
                if let Some((x, y)) = display.position_for(&info.name) {
                    info.x = x;
                    info.y = y;
                    positioned = true;
                    right = right.max(x + info.width);
                }
            }
            if positioned {
                for info in &mut list {
                    if display.position_for(&info.name).is_none() {
                        info.x = right;
                        info.y = 0;
                        right += info.width;
                    }
                }
            } else {
                layout_horizontal(&mut list);
            }
            write_outputs_snapshot(&list, &detailed);
            origins = list
                .iter()
                .map(|info| (info.x as f64, info.y as f64))
                .collect();
            *outputs.lock_or_recover() = list;
        }
        let mut sizes: Vec<(u32, u32)> = infos.iter().map(|(_, w, h, _)| (*w, *h)).collect();
        for (name, w, h, hz) in &infos {
            println!(
                "{}: {} {}x{}@{}Hz",
                libluna_i18n::tr("DRM output"),
                name,
                w,
                h,
                hz
            );
        }
        // Kursor widoczny od pierwszego kadru. Model pozycji startuje jako `None`
        // i zapełnia się dopiero pierwszym ruchem myszy — do tego czasu scena
        // (i płaszczyzna sprzętowa) nie miały czego narysować, więc użytkownik
        // widział „brak kursora” aż do ruchu. Startujemy na środku pierwszego
        // monitora: wskaźnik jest od razu, a ruch go tylko przenosi.
        if cursor.lock_or_recover().is_none() {
            if let (Some(&(w, h)), Some(&(ox, oy))) = (sizes.first(), origins.first()) {
                *cursor.lock_or_recover() =
                    Some(Point::new(ox + f64::from(w) / 2.0, oy + f64::from(h) / 2.0));
            }
        }
        let mut renderers: Vec<SoftwareRenderer> = sizes
            .iter()
            .map(|(w, h)| SoftwareRenderer::new(*w, *h))
            .collect();
        let mut gpu_renderers: Vec<GpuRenderer> = Vec::new();
        // Backend wyjścia: GPU, gdy poproszono (`SELENE_GPU`/`LUNA_GPU`) i EGL działa.
        // GPU komponuje okna `dmabuf` sprzętowo (CPU rysuje dla nich tylko zaślepkę),
        // a każdy monitor dostaje własny kontekst — render jest sekwencyjny, bo
        // kontekstu EGL nie można przenosić między wątkami.
        let mut use_gpu = if gpu_requested() {
            let mut gpu = Vec::with_capacity(sizes.len());
            let mut error = None;
            for (w, h) in &sizes {
                match GpuRenderer::new(*w, *h) {
                    Ok(r) => gpu.push(r),
                    Err(e) => {
                        error = Some(e);
                        break;
                    }
                }
            }
            if gpu.len() == sizes.len() {
                renderers.clear();
                gpu_renderers = gpu;
                true
            } else {
                eprintln!(
                    "selene/drm: GPU niedostępne ({}), zostaję przy CPU",
                    error.as_deref().unwrap_or("brak kontekstu EGL")
                );
                false
            }
        } else {
            false
        };
        if use_gpu && std::env::var("SELENE_STATS").is_ok() {
            eprintln!("selene/drm: backend gpu");
        }
        // Zero-copy skanout (GPU → `dmabuf` → `AddFB2`): domyślnie włączone przy GPU,
        // wyłączenie przez `SELENE_ZEROCOPY=0`. Każde wyjście osobno może spaść do
        // readbacku, gdy sterownik odrzuci bufor — więc to nigdy nie psuje obrazu.
        let zerocopy_requested = gpu_requested()
            && std::env::var("SELENE_ZEROCOPY")
                .map(|v| !v.is_empty() && v != "0")
                .unwrap_or(true);
        let mut zerocopy: Vec<bool> = if use_gpu && zerocopy_requested {
            vec![true; sizes.len()]
        } else {
            vec![false; sizes.len()]
        };
        if use_gpu {
            for (i, on) in zerocopy.iter().enumerate() {
                if let Some(renderer) = gpu_renderers.get_mut(i) {
                    renderer.set_scanout_only(*on);
                }
            }
            if std::env::var("SELENE_STATS").is_ok() {
                eprintln!(
                    "selene/drm: zero-copy {}",
                    if zerocopy_requested { "tak" } else { "nie" }
                );
                eprintln!(
                    "selene/drm: GBM (zero-copy NVIDIA): {}",
                    libluna_render::gbm::describe()
                );
            }
        }
        let bg = Color::from_rgba8(0x1e, 0x20, 0x28, 0xff);
        // Motyw i silnik tekstu dla dekoracji (widget `HeaderBar` z libLuny).
        // Bierzemy **motyw użytkownika** (jasny/ciemny, akcent, styl z pliku), a nie
        // twarde `Theme::dark()` — inaczej pasek okna X11 wyglądał inaczej niż okno
        // aplikacji libLuny. Co 2 s sprawdzamy plik ustawień, żeby zmiana motywu
        // w demo odświeżyła też dekoracje bez wylogowania.
        let mut theme = user_theme();
        let mut theme_checked = std::time::Instant::now();
        let text_engine = TextEngine::system();

        // Kursor: obraz bierzemy z modelu (kształt z `wp_cursor_shape_v1` albo bufor
        // klienta). Płaszczyzna sprzętowa dostaje własny BO; za duży obraz → scena.
        let mut cursor_source = crate::state::CursorImage::Default;
        crate::cursor::set_model(&cursor_source);
        let (mut cursor_gfx, mut cursor_hotspot) = crate::cursor::current();
        // Na kartach wirtualnych płaszczyzna kursora jest bezużyteczna (qxl
        // wywraca w niej jądro), więc kursor rysuje scena — bez MODE_CURSOR.
        let hw_cursor_supported = backend.supports_hw_cursor();
        let mut hw_cursor = hw_cursor_supported && crate::cursor::fits_hardware(&cursor_gfx);
        for i in 0..sizes.len() {
            // Bezpieczna próba na (0, 0) i natychmiastowe ukrycie — uodparnia na
            // sterowniki DRM odrzucające ujemne współrzędne w `MODE_CURSOR`.
            if !hw_cursor
                || !backend.set_cursor_image(
                    i,
                    &cursor_gfx.rgba,
                    cursor_gfx.width,
                    cursor_gfx.height,
                    0,
                    0,
                )
                || !backend.hide_cursor(i)
            {
                hw_cursor = false;
            }
        }
        if !hw_cursor {
            eprintln!(
                "{}",
                libluna_i18n::tr("DRM hardware cursor unavailable; using software cursor")
            );
        }
        let mut cursor_shown = vec![false; sizes.len()];

        // Diagnostyka (SELENE_STATS=1): fps i rozbicie czasu na render/present/wait_flip.
        let stats = std::env::var("SELENE_STATS").is_ok();
        let mut flip_error_logged = false;
        let mut frames = 0u64;
        let (mut ms_render, mut ms_present, mut ms_wait) = (0.0f64, 0.0f64, 0.0f64);
        let mut report = std::time::Instant::now();
        let mut last_sigs: Vec<Option<u64>> = vec![None; sizes.len()];
        let mut last_cur: Option<Point> = None;
        // Damage (tylko GPU): poprzednia scena per wyjście i prostokąt skopiowany
        // w poprzedniej klatce. Bufory są podwójne, więc do prezentacji idzie
        // suma dwóch ostatnich klatek (back był ostatnio pokazany dwa flipy temu).
        let mut prev_scene: Vec<Option<Scene>> = vec![None; sizes.len()];
        let mut prev_damage: Vec<Option<Rect>> = vec![None; sizes.len()];
        let mut applied_night: Option<u8> = None;
        let mut applied_power = false;
        let mut last_present = std::time::Instant::now();
        // Flipy zakolejkowane w poprzedniej iteracji, na które jeszcze nie czekaliśmy.
        let mut pending_flips = 0usize;
        // Wykrywanie powrotu z uśpienia: pętla po suspendzie wznawia się po
        // wielosekundowej przerwie. Jądro mogło zgubić stan CRTC, a zdarzenia
        // flipów sprzed snu nie dotyczą już nowych klatek.
        let mut last_loop = std::time::Instant::now();
        loop {
            let now = std::time::Instant::now();
            if now.duration_since(last_loop) > Duration::from_secs(2) {
                eprintln!(
                    "selene/drm: powrót z uśpienia ({:.1}s) — odtwarzam tryb",
                    now.duration_since(last_loop).as_secs_f64()
                );
                if !backend.resume() {
                    eprintln!("selene/drm: modeset po wybudzeniu nie powiódł się — próbuję dalej");
                }
                pending_flips = 0;
                last_sigs = vec![None; sizes.len()];
                prev_scene = vec![None; sizes.len()];
                prev_damage = vec![None; sizes.len()];
                cursor_shown = vec![false; sizes.len()];
                applied_night = None;
                applied_power = false;
                flip_error_logged = false;
                // Płaszczyzna kursora też została zresetowana przez resume.
                hw_cursor = hw_cursor_supported && crate::cursor::fits_hardware(&cursor_gfx);
                for i in 0..sizes.len() {
                    if !hw_cursor
                        || !backend.set_cursor_image(
                            i,
                            &cursor_gfx.rgba,
                            cursor_gfx.width,
                            cursor_gfx.height,
                            0,
                            0,
                        )
                        || !backend.hide_cursor(i)
                    {
                        hw_cursor = false;
                    }
                }
                last_present = now;
            }
            last_loop = now;
            // Flipy **poprzedniej** klatki zbieramy dopiero teraz: dopiero po nich
            // wolno pisać do bufora, który był skanowany. Dzięki temu render i
            // readback bieżącej klatki nakładają się na vblank poprzedniej.
            // Wcześniej `present` i czekanie stały obok siebie, więc każda klatka
            // płaciła pełny vblank, a przy dwóch monitorach (75 Hz + 60 Hz) gubiła
            // terminy — stąd spadki do kilkunastu klatek na sekundę.
            if pending_flips > 0 {
                let t = std::time::Instant::now();
                for _ in 0..pending_flips {
                    backend.wait_flip();
                }
                ms_wait += t.elapsed().as_secs_f64() * 1000.0;
                pending_flips = 0;
                // Flipy się zakończyły — klatki są na ekranie. Dopiero teraz
                // oddajemy klientom bufory, których już nikt nie skanuje.
                present.presented();
            }
            // Przełączenie VT (`Ctrl+Alt+F1…F12`). Najpierw oddajemy master DRM —
            // bez tego konsola jądra (fbcon) nie przejmie ekranu i nowy VT jest
            // czarny. Po powrocie robimy pełny modeset; `wait_active` blokuje ten
            // wątek do czasu powrotu (renderowanie nie ma wtedy czego pokazać).
            if let Some(target) = vt_request.lock_or_recover().take() {
                // Bez `XDG_VTNR` nie wiemy, na który VT wracać — wtedy nie ruszamy
                // terminala (lepiej zostać niż zawiesić sesję na obcej konsoli).
                if let Some(own) = crate::vt::current() {
                    if own != target {
                        backend.drop_master();
                        crate::vt::set_active(false);
                        let switched = crate::vt::activate(target);
                        if switched {
                            crate::vt::wait_active(own);
                        }
                        // Powrót (albo nieudane przełączenie): master i tryby z powrotem.
                        backend.resume();
                        // Ekran trzeba odrysować od zera — sygnatury i damage sprzed
                        // przełączenia opisują bufory, których już nie ma na skanoucie.
                        for sig in last_sigs.iter_mut() {
                            *sig = None;
                        }
                        for scene in prev_scene.iter_mut() {
                            *scene = None;
                        }
                        for damage in prev_damage.iter_mut() {
                            *damage = None;
                        }
                        cursor_shown = vec![false; sizes.len()];
                        applied_night = None;
                        applied_power = false;
                        flip_error_logged = false;
                        pending_flips = 0;
                        last_present = std::time::Instant::now();
                        // Modeset zresetował płaszczyznę kursora — ładujemy ją ponownie.
                        hw_cursor =
                            hw_cursor_supported && crate::cursor::fits_hardware(&cursor_gfx);
                        for i in 0..sizes.len() {
                            if !hw_cursor
                                || !backend.set_cursor_image(
                                    i,
                                    &cursor_gfx.rgba,
                                    cursor_gfx.width,
                                    cursor_gfx.height,
                                    0,
                                    0,
                                )
                                || !backend.hide_cursor(i)
                            {
                                hw_cursor = false;
                            }
                        }
                        crate::vt::set_active(true);
                    }
                }
            }
            // Motyw użytkownika mógł się zmienić (demo/Ustawienia) — odśwież, gdy inny.
            if theme_checked.elapsed() >= Duration::from_secs(2) {
                theme_checked = std::time::Instant::now();
                let next = user_theme();
                if next != theme {
                    theme = next;
                    // Wymuś pełny render (sygnatury nie zawierają kolorów motywu).
                    for sig in last_sigs.iter_mut() {
                        *sig = None;
                    }
                }
            }
            // Zmiana trybu monitora z Ustawień (`luna-control output-mode`):
            // realny modeset DRM + odbudowa buforów i rendererów tego monitora.
            if let Some((name, mode)) = display_request.lock_or_recover().take() {
                match backend.set_mode(&name, mode) {
                    Some(_index) => {
                        let previous = outputs.lock_or_recover().clone();
                        infos = backend.outputs();
                        sizes = infos.iter().map(|(_, w, h, _)| (*w, *h)).collect();
                        if use_gpu {
                            let mut gpu = Vec::with_capacity(sizes.len());
                            let mut failure = None;
                            for (w, h) in &sizes {
                                match GpuRenderer::new(*w, *h) {
                                    Ok(renderer) => gpu.push(renderer),
                                    Err(err) => {
                                        failure = Some(err);
                                        break;
                                    }
                                }
                            }
                            if failure.is_none() && gpu.len() == sizes.len() {
                                gpu_renderers = gpu;
                                // Nowe renderery startują z domyślnym readbackiem —
                                // przywracamy dla nich tryb skanoutu.
                                for (i, on) in zerocopy.iter().enumerate() {
                                    if let Some(renderer) = gpu_renderers.get_mut(i) {
                                        renderer.set_scanout_only(*on);
                                    }
                                }
                            } else {
                                eprintln!(
                                    "selene/drm: GPU po zmianie trybu niedostępne ({}) — przechodzę na CPU",
                                    failure.as_deref().unwrap_or("nieznany błąd")
                                );
                                use_gpu = false;
                                gpu_renderers.clear();
                                for active in zerocopy.iter_mut() {
                                    *active = false;
                                }
                                renderers = sizes
                                    .iter()
                                    .map(|(w, h)| SoftwareRenderer::new(*w, *h))
                                    .collect();
                            }
                        } else {
                            renderers = sizes
                                .iter()
                                .map(|(w, h)| SoftwareRenderer::new(*w, *h))
                                .collect();
                        }
                        // Płaszczyzna kursora mogła zostać zresetowana przez modeset.
                        hw_cursor =
                            hw_cursor_supported && crate::cursor::fits_hardware(&cursor_gfx);
                        for i in 0..sizes.len() {
                            if !hw_cursor
                                || !backend.set_cursor_image(
                                    i,
                                    &cursor_gfx.rgba,
                                    cursor_gfx.width,
                                    cursor_gfx.height,
                                    0,
                                    0,
                                )
                                || !backend.hide_cursor(i)
                            {
                                hw_cursor = false;
                            }
                        }
                        cursor_shown = vec![false; sizes.len()];
                        last_sigs = vec![None; sizes.len()];
                        // Nowe renderery/bufory: następna klatka musi być pełna.
                        prev_scene = vec![None; sizes.len()];
                        prev_damage = vec![None; sizes.len()];
                        last_present = std::time::Instant::now();
                        // Modeset odtworzył bufory — zdarzenia flipów sprzed zmiany
                        // trybu nie dotyczą już nowych framebufferów.
                        pending_flips = 0;
                        // Modeset zeruje LUT gamma — night light trzeba wgrać ponownie.
                        applied_night = None;

                        // Panel i Ustawienia dostają nową geometrię od razu.
                        // Pozycje z układu użytkownika zostają — zmienił się tylko rozmiar.
                        let detailed = backend.outputs_with_modes();
                        let mut list: Vec<OutputInfo> = infos
                            .iter()
                            .map(|(name, w, h, hz)| OutputInfo {
                                name: name.clone(),
                                width: *w as i32,
                                height: *h as i32,
                                refresh_hz: *hz as i32,
                                scale: if *w >= 3840 || *h >= 2160 { 2 } else { 1 },
                                x: 0,
                                y: 0,
                            })
                            .collect();
                        if previous.len() == list.len() {
                            for (info, old) in list.iter_mut().zip(previous.iter()) {
                                info.x = old.x;
                                info.y = old.y;
                            }
                        } else {
                            layout_horizontal(&mut list);
                        }
                        origins = list
                            .iter()
                            .map(|info| (info.x as f64, info.y as f64))
                            .collect();
                        write_outputs_snapshot(&list, &detailed);
                        *outputs.lock_or_recover() = list;
                        if stats {
                            eprintln!("selene/drm: tryb {name} zmieniony");
                        }
                    }
                    None => eprintln!("selene/drm: nie udało się ustawić trybu dla {name}"),
                }
            }
            // Przesunięcie monitora w układzie (`luna-control output-position`).
            // Bez modesetu: zmienia się tylko model wyjść i geometria dla klientów.
            if let Some((name, x, y)) = position_request.lock_or_recover().take() {
                let mut list = outputs.lock_or_recover().clone();
                if let Some(slot) = list.iter_mut().find(|output| output.name == name) {
                    slot.x = x;
                    slot.y = y;
                    origins = list
                        .iter()
                        .map(|info| (info.x as f64, info.y as f64))
                        .collect();
                    let detailed = backend.outputs_with_modes();
                    write_outputs_snapshot(&list, &detailed);
                    *outputs.lock_or_recover() = list;
                    // Inne kadrowanie monitorów — wymuś pełny render.
                    for sig in last_sigs.iter_mut() {
                        *sig = None;
                    }
                    last_cur = None;
                    if stats {
                        eprintln!("selene/drm: {name} -> {x},{y}");
                    }
                }
            }
            // Night light i oszczędzanie energii — zmiany z gniazda sterowania.
            if gamma_ok {
                let requested = night_light.load(std::sync::atomic::Ordering::Relaxed);
                if applied_night != Some(requested) {
                    applied_night = Some(requested);
                    backend.set_night_light(requested);
                    if stats {
                        eprintln!("selene/drm: night light {requested}%");
                    }
                }
            }
            let power = power_saver.load(std::sync::atomic::Ordering::Relaxed);
            if power != applied_power {
                applied_power = power;
                if stats {
                    eprintln!(
                        "selene/drm: power saver {}",
                        if power { "on" } else { "off" }
                    );
                }
            }
            // Zrzut ekranu (Super+F12): kompozytor trzyma gotowe klatki każdego monitora,
            // więc wystarczy je zapisać.
            if screenshot.swap(false, std::sync::atomic::Ordering::Relaxed) {
                // Zrzut z aktywnego backendu — z GPU widać też okna `dmabuf`.
                if use_gpu {
                    // W trybie zero-copy klatka siedzi wyłącznie w teksturze GPU —
                    // bez readbacku zrzut ekranu byłby nieaktualny.
                    for (i, renderer) in gpu_renderers.iter_mut().enumerate() {
                        if zerocopy.get(i).copied().unwrap_or(false) {
                            renderer.readback_full();
                        }
                    }
                    let refs: Vec<&dyn Renderer> =
                        gpu_renderers.iter().map(|r| r as &dyn Renderer).collect();
                    save_screenshots(&refs, &infos);
                } else {
                    let refs: Vec<&dyn Renderer> =
                        renderers.iter().map(|r| r as &dyn Renderer).collect();
                    save_screenshots(&refs, &infos);
                }
            }
            let ov = *overview.lock_or_recover();
            // Wygaszony ekran = brak kursora (także sprzętowego) do pierwszej aktywności.
            let cur = if ov.blanked {
                None
            } else {
                *cursor.lock_or_recover()
            };
            let cur_ws = *ws.lock_or_recover();
            let ws_count = ws_count.load(std::sync::atomic::Ordering::Relaxed);

            // Zmiana obrazu kursora (kształt z `wp_cursor_shape_v1` albo bufor klienta).
            // Nie zależy od ruchu myszy, więc wykrywamy ją osobno.
            let image_changed = {
                let next = cursor_image.lock_or_recover().clone();
                if next == cursor_source {
                    false
                } else {
                    cursor_source = next;
                    crate::cursor::set_model(&cursor_source);
                    let (gfx, hotspot) = crate::cursor::current();
                    cursor_gfx = gfx;
                    cursor_hotspot = hotspot;
                    true
                }
            };
            if image_changed {
                if hw_cursor && !crate::cursor::fits_hardware(&cursor_gfx) {
                    // Płaszczyzna bywa mniejsza niż kursor klienta — rysujemy w scenie.
                    hw_cursor = false;
                    for i in 0..sizes.len() {
                        backend.hide_cursor(i);
                        cursor_shown[i] = false;
                    }
                    eprintln!(
                        "selene/drm: kursor {}×{} za duży dla płaszczyzny — rysuję w scenie",
                        cursor_gfx.width, cursor_gfx.height
                    );
                }
                if hw_cursor {
                    apply_hw_cursor(
                        &mut backend,
                        &sizes,
                        &origins,
                        cur,
                        &cursor_gfx,
                        cursor_hotspot,
                        &mut cursor_shown,
                        true,
                    );
                }
                // Software: kursor jest częścią sceny — wymuś pełny render.
                for sig in last_sigs.iter_mut() {
                    *sig = None;
                }
            }

            let cur_changed = last_cur != cur;
            let mut any_changed = false;
            let mut changed = vec![false; sizes.len()];

            for i in 0..sizes.len() {
                let (w, h) = sizes[i];
                let (ox, oy) = origins[i];
                let sig = output_signature(
                    &model,
                    cur_ws,
                    ws_count,
                    ov,
                    if hw_cursor { None } else { cur },
                    ox,
                    oy,
                    w as f64,
                    h as f64,
                );
                if last_sigs[i] != Some(sig) {
                    last_sigs[i] = Some(sig);
                    changed[i] = true;
                    any_changed = true;
                }
            }

            // Zwalniane bufory czekają na prezentację. Gdy nic się nie zmieniło
            // (np. okno jest na innej przestrzeni), wymuś klatkę — inaczej klient
            // nigdy nie dostanie `wl_buffer.release` i stanie.
            if !any_changed && present.has_pending() {
                for value in changed.iter_mut() {
                    *value = true;
                }
                any_changed = true;
            }

            if !any_changed && !cur_changed {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
            last_cur = cur;

            // Kursor sprzętowy: ustaw/przesuń płaszczyznę kursora (bez dotykania sceny).
            if hw_cursor && (cur_changed || image_changed) {
                apply_hw_cursor(
                    &mut backend,
                    &sizes,
                    &origins,
                    cur,
                    &cursor_gfx,
                    cursor_hotspot,
                    &mut cursor_shown,
                    image_changed,
                );
            }

            if any_changed {
                // Tryb oszczędzania: maksymalnie ~30 klatek/s (oszczędza CPU/GPU
                // podczas animacji i przewijania; kursor sprzętowy pozostaje płynny).
                if let Some(delay) = throttle_delay(power, last_present.elapsed()) {
                    std::thread::sleep(delay);
                }
                last_present = std::time::Instant::now();
                // Renderowanie monitorów **równolegle**: każdy ma własny bufor i scenę,
                // więc na maszynie wielordzeniowej dwie klatki 1080p powstają w czasie
                // jednej. Sekwencyjny render dwóch monitorów zjadał cały budżet klatki
                // (7–10 ms każdy) i powłoka „zwalniała” do ~20 FPS.
                let t_render = std::time::Instant::now();
                // Damage bieżącej klatki (None = pełna klatka). Wypełniane tylko
                // w ścieżce GPU; CPU rysuje i czyta całość.
                let mut frame_damage: Vec<Option<Rect>> = vec![None; sizes.len()];
                if use_gpu {
                    // Sekwencyjnie: kontekst EGL jest przypięty do wątku tworzącego,
                    // a i tak jest znacznie szybszy od równoległego CPU.
                    for i in 0..sizes.len() {
                        if !changed[i] {
                            continue;
                        }
                        let (w, h) = sizes[i];
                        let (ox, oy) = origins[i];
                        let mut scene = Scene::new();
                        build_scene(
                            &mut scene,
                            Size::new(w as f64, h as f64),
                            ox,
                            oy,
                            &model,
                            cur_ws,
                            ws_count,
                            ov,
                            i == 0,
                            cur,
                            !hw_cursor,
                            &theme,
                            &text_engine,
                        );
                        let damage = match prev_scene[i].as_ref() {
                            // `Scene` porównuje obrazy po wskaźniku `Arc`. Dla
                            // `dmabuf` klient może narysować nową treść do tego
                            // samego bufora, więc różnica scen byłaby fałszywie
                            // zerowa — wtedy czytamy i kopiujemy całą klatkę.
                            Some(prev) if !scene_has_visible_dmabuf(&scene, w as f64, h as f64) => {
                                prev.damage_bounds(&scene, gpu_renderers[i].text_engine())
                            }
                            _ => None,
                        };
                        gpu_renderers[i].draw_damage(&scene, 1.0, bg, damage);
                        frame_damage[i] = damage;
                        prev_scene[i] = Some(scene);
                    }
                } else {
                    let theme = &theme;
                    let text_engine = &text_engine;
                    std::thread::scope(|scope| {
                        for (i, renderer) in renderers.iter_mut().enumerate() {
                            if !changed[i] {
                                continue;
                            }
                            let (w, h) = sizes[i];
                            let (ox, oy) = origins[i];
                            let model = &model;
                            scope.spawn(move || {
                                let mut scene = Scene::new();
                                build_scene(
                                    &mut scene,
                                    Size::new(w as f64, h as f64),
                                    ox,
                                    oy,
                                    model,
                                    cur_ws,
                                    ws_count,
                                    ov,
                                    i == 0,
                                    cur,
                                    !hw_cursor,
                                    &theme,
                                    &text_engine,
                                );
                                renderer.draw(&scene, 1.0, bg);
                            });
                        }
                    });
                }
                ms_render += t_render.elapsed().as_secs_f64() * 1000.0;

                let mut num_flips = 0;
                for i in 0..sizes.len() {
                    if !changed[i] {
                        continue;
                    }
                    let t = std::time::Instant::now();
                    // Prezentacja na GPU: najpierw próba zero-copy (tekstura → dmabuf),
                    // dopiero potem readback. Po odrzuceniu przez sterownik zostajemy
                    // przy readbacku do końca życia tego wyjścia.
                    let mut flipped = false;
                    if use_gpu && zerocopy[i] {
                        if let Some(frame) = gpu_renderers[i].export_dmabuf() {
                            flipped = backend.present_dmabuf(i, &frame);
                        }
                        if !flipped {
                            zerocopy[i] = false;
                            gpu_renderers[i].set_scanout_only(false);
                            gpu_renderers[i].readback_full();
                            // Cała klatka jest świeża — następna prezentacja musi
                            // skopiować wszystko (bufory mogły się rozjechać).
                            prev_damage[i] = None;
                            eprintln!(
                                "selene/drm: zero-copy odrzucone na wyjściu {i} — czytam przez CPU"
                            );
                        }
                    }
                    if !flipped {
                        let pixels = if use_gpu {
                            gpu_renderers[i].data()
                        } else {
                            renderers[i].data()
                        };
                        // Podwójny bufor: kopiujemy sumę damage z bieżącej i
                        // poprzedniej klatki — back był prezentowany dwa flipy temu.
                        let region = match (prev_damage[i], frame_damage[i]) {
                            (Some(a), Some(b)) => pixel_rect(a.union(&b), sizes[i].0, sizes[i].1),
                            _ => None,
                        };
                        flipped = match region {
                            Some(r) => backend.present_region(i, pixels, Some(r)),
                            None => backend.present(i, pixels),
                        };
                        prev_damage[i] = frame_damage[i];
                    }
                    if !flipped && !flip_error_logged {
                        eprintln!(
                            "{}",
                            libluna_i18n::tr("DRM page-flip failed (screen may stay blank)")
                        );
                        flip_error_logged = true;
                    }
                    ms_present += t.elapsed().as_secs_f64() * 1000.0;

                    if flipped {
                        num_flips += 1;
                    }
                }

                // Czekanie na flipy przenosimy na początek następnej iteracji
                // (patrz `pending_flips` wyżej) — tutaj tylko je zapamiętujemy.
                pending_flips = num_flips;
                if num_flips > 0 {
                    present.mark_submitted();
                }

                frames += 1;
                if stats && report.elapsed().as_secs_f64() >= 2.0 {
                    let secs = report.elapsed().as_secs_f64();
                    let frames_f = frames.max(1) as f64;
                    let per_monitor = frames_f * sizes.len() as f64;
                    eprintln!(
                        "selene/drm: {:.1} klatek/s | render {:.2} ms/klatke, present {:.2} ms, wait_flip {:.2} ms",
                        frames as f64 / secs,
                        ms_render / frames_f,
                        ms_present / per_monitor.max(1.0),
                        ms_wait / per_monitor.max(1.0)
                    );
                    frames = 0;
                    ms_render = 0.0;
                    ms_present = 0.0;
                    ms_wait = 0.0;
                    report = std::time::Instant::now();
                }
            }
        }
    });
}

/// Zapisuje surowe RGBA jako PNG w katalogu zrzutów (`luna-<czas>-<nazwa>.png`).
///
/// Używane przez paletę (`Super+F12`) — `wlr-screencopy` koduje przez własny
/// renderer, a tutaj potrzebujemy zapisać gotowy bufor (także sklejony obszar).
pub(crate) fn write_screenshot_png(
    name: &str,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
) -> Option<std::path::PathBuf> {
    if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        return None;
    }
    let mut scene = Scene::new();
    scene.image(
        Rect::from_xywh(0.0, 0.0, width as f64, height as f64),
        std::sync::Arc::new(ImageData::new(width, height, rgba)),
    );
    let mut renderer = SoftwareRenderer::new(width, height);
    renderer.draw(&scene, 1.0, Color::from_rgba8(0x00, 0x00, 0x00, 0xff));
    let png = renderer.encode_png()?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = screenshot_dir().join(format!("luna-{stamp}-{name}.png"));
    std::fs::write(&path, png).ok().map(|_| path)
}

/// Zapisuje bieżące klatki (po jednej na monitor) jako PNG — bezpośredni zrzut
/// (dawniej `Super+F12`, dziś `luna-control screenshot`).
pub fn save_screenshots(renderers: &[&dyn Renderer], infos: &[(String, u32, u32, u32)]) {
    let dir = screenshot_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut saved = false;
    for (i, renderer) in renderers.iter().enumerate() {
        let name = infos
            .get(i)
            .map(|(n, _, _, _)| n.as_str())
            .unwrap_or("ekran");
        let path = dir.join(format!("luna-{stamp}-{name}.png"));
        match renderer.encode_png() {
            Some(png) => match std::fs::write(&path, png) {
                Ok(()) => {
                    eprintln!("selene: zrzut ekranu: {}", path.display());
                    saved = true;
                }
                Err(e) => eprintln!("selene: nie udało się zapisać zrzutu: {e}"),
            },
            None => eprintln!("selene: nie udało się zakodować zrzutu"),
        }
    }
    if saved {
        crate::screenshot::notify_saved(&dir);
    }
}

/// Czy wyjście ma używać GPU: `SELENE_GPU` lub `LUNA_GPU` ustawione na niepustą
/// wartość inną niż `0`. Brak EGL wciąż spada do CPU (nigdy nie zabija sesji).
fn gpu_requested() -> bool {
    ["SELENE_GPU", "LUNA_GPU"].iter().any(|name| {
        std::env::var(name)
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}

/// Katalog na zrzuty: `XDG_PICTURES_DIR` → `~/.config/user-dirs.dirs` → `$HOME/Pictures`
/// (oraz polskie `Obrazy`) → `/tmp`.
pub(crate) fn screenshot_dir() -> std::path::PathBuf {
    let mut pictures = std::env::var("XDG_PICTURES_DIR")
        .ok()
        .filter(|d| !d.is_empty());
    // Wiele sesji nie eksportuje `XDG_PICTURES_DIR`; użytkownik ma je w `user-dirs.dirs`.
    if pictures.is_none() {
        if let Some(home) = std::env::var_os("HOME") {
            let cfg = std::path::PathBuf::from(&home)
                .join(".config")
                .join("user-dirs.dirs");
            if let Ok(text) = std::fs::read_to_string(cfg) {
                for line in text.lines() {
                    let line = line.trim();
                    if let Some(rest) = line.strip_prefix("XDG_PICTURES_DIR=") {
                        let value = rest
                            .trim()
                            .trim_matches('"')
                            .replace("$HOME", &home.to_string_lossy());
                        if !value.is_empty() {
                            pictures = Some(value);
                        }
                        break;
                    }
                }
            }
        }
    }
    let mut candidates: Vec<String> = Vec::new();
    if let Some(p) = pictures {
        candidates.push(p);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        candidates.push(home.join("Pictures").to_string_lossy().into_owned());
        candidates.push(home.join("Obrazy").to_string_lossy().into_owned());
    }
    for dir in candidates {
        let p = std::path::PathBuf::from(dir).join("Screenshots");
        if std::fs::create_dir_all(&p).is_ok() {
            return p;
        }
    }
    std::path::PathBuf::from("/tmp")
}

/// Ustawia obraz i pozycję kursora sprzętowego na wyjściach (DRM).
///
/// `force_image` wymusza nowy BO (po zmianie kształtu/obrazu), a nie tylko ruch —
/// `MODE_CURSOR` z flagą `MOVE` nie zmienia zawartości pamięci kursora.
#[allow(clippy::too_many_arguments)]
fn apply_hw_cursor(
    backend: &mut DrmBackend,
    sizes: &[(u32, u32)],
    origins: &[(f64, f64)],
    cur: Option<Point>,
    gfx: &libluna_render::ImageData,
    hotspot: (i32, i32),
    shown: &mut [bool],
    force_image: bool,
) {
    for i in 0..sizes.len() {
        let (ow, oh) = sizes[i];
        let (ox, oy) = origins[i];
        let inside = cur
            .map(|c| {
                let (lx, ly) = (c.x - ox, c.y - oy);
                lx >= 0.0 && ly >= 0.0 && lx < ow as f64 && ly < oh as f64
            })
            .unwrap_or(false);
        if inside {
            let c = cur.unwrap_or(Point::new(0.0, 0.0));
            let x = (c.x - ox) as i32 - hotspot.0;
            let y = (c.y - oy) as i32 - hotspot.1;
            if force_image || !shown[i] {
                backend.set_cursor_image(i, &gfx.rgba, gfx.width, gfx.height, x, y);
                shown[i] = true;
            } else {
                backend.move_cursor(i, x, y);
            }
        } else if shown[i] {
            backend.hide_cursor(i);
            shown[i] = false;
        }
    }
}

/// Sygnatura sceny dla konkretnego monitora — pozwala pominąć render monitora, na którym nic się nie zmieniło.
/// Czy na tym wyjściu jest widoczny bufor GPU klienta.
///
/// `Scene::damage_bounds` porównuje `Arc<DmabufImage>` po wskaźniku, a klient
/// może narysować nową treść do tego samego bufora — wtedy różnica scen byłaby
/// fałszywie zerowa. W takiej sytuacji bierzemy pełną klatkę.
fn scene_has_visible_dmabuf(scene: &Scene, w: f64, h: f64) -> bool {
    let screen = Rect::from_xywh(0.0, 0.0, w, h);
    scene
        .commands()
        .iter()
        .any(|c| matches!(c, DrawCommand::ImageDmabuf { rect, .. } if rect.intersects(&screen)))
}

/// Prostokąt damage (logiczny; w DRM skala 1.0) → piksele fizyczne.
///
/// Prostokąt zerowy też dostaje `Some`: `present_region` nic wtedy nie kopiuje,
/// ale page-flip jest potrzebny, jeśli zmiana wypadła poza tym wyjściem.
fn pixel_rect(r: Rect, w: u32, h: u32) -> Option<PixelRect> {
    let x0 = r.x().floor().max(0.0);
    let y0 = r.y().floor().max(0.0);
    let x1 = (r.x() + r.width()).ceil().min(f64::from(w));
    let y1 = (r.y() + r.height()).ceil().min(f64::from(h));
    if x1 <= x0 || y1 <= y0 {
        return Some(PixelRect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        });
    }
    Some(PixelRect {
        x: x0 as u32,
        y: y0 as u32,
        w: (x1 - x0) as u32,
        h: (y1 - y0) as u32,
    })
}

fn output_signature(
    model: &WindowModel,
    current_ws: usize,
    ws_count: usize,
    overview: Overview,
    cursor: Option<Point>,
    ox: f64,
    oy: f64,
    ow: f64,
    oh: f64,
) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: u64| {
        h ^= v;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    mix(current_ws as u64);
    // Liczba przestrzeni zmienia pasek/podgląd, nawet gdy bieżąca przestrzeń się nie zmienia.
    mix(ws_count as u64);
    // Tapeta z Ustawień działa bez restartu sesji.
    mix(wallpaper_stamp());
    mix(u64::from(overview.on));
    mix(u64::from(overview.blanked));
    // Paleta zrzutów: każda zmiana zaznaczenia, podświetlenia i prostokąta
    // musi dać nową klatkę — inaczej wątek wyjścia nie odświeży nakładki.
    match overview.shot {
        Some(shot) => {
            mix(1);
            mix(shot.selected as u64);
            mix(shot.monitors as u64);
            mix(shot.hover.map_or(u64::MAX, |v| v as u64));
            mix(u64::from(shot.selecting));
            if let Some((a, b)) = shot.area {
                mix(a.x.to_bits());
                mix(a.y.to_bits());
                mix(b.x.to_bits());
                mix(b.y.to_bits());
            }
            if let Some(r) = shot.highlight {
                mix(r.x().to_bits());
                mix(r.y().to_bits());
                mix(r.width().to_bits());
                mix(r.height().to_bits());
            }
            mix(shot.pill.x().to_bits());
            mix(shot.pill.y().to_bits());
        }
        None => mix(0),
    }
    if let Some((slot, (px, py))) = overview.drag {
        mix(slot as u64);
        mix(px.to_bits());
        mix(py.to_bits());
    }
    // Poświata rogu: kwantyzacja postępu (256 kroków) wystarcza, a nie tworzy
    // nowej sygnatury dla każdej klatki po zatrzymaniu się na 100%.
    match overview.corner {
        Some(hint) => {
            mix(1);
            mix(hint.kind as u64);
            mix((hint.progress.clamp(0.0, 1.0) * 255.0).round() as u64);
            mix(hint.top_inset.to_bits());
        }
        None => mix(0),
    }
    let mon_rect = Rect::from_xywh(ox, oy, ow, oh);
    {
        let windows = model.lock_or_recover();
        for w in windows.iter() {
            if !w.mapped || w.minimized || !w.in_workspace(current_ws) {
                continue;
            }
            if !w.outer().intersects(&mon_rect) {
                continue;
            }
            mix(w.x.to_bits());
            mix(w.y.to_bits());
            mix(w.z as u64);
            mix(u64::from(w.focused));
            mix(w.width as u64);
            mix(w.height as u64);
            mix(u64::from(w.mapped) | (u64::from(w.minimized) << 1) | (u64::from(w.layer) << 2));
            // Pasek tytułu, pełny ekran, popup i tytuł zmieniają wygląd bez zmiany geometrii.
            mix(u64::from(w.ssd) | (u64::from(w.fullscreen) << 1) | (u64::from(w.popup) << 2));
            mix(w.title.as_bytes().iter().fold(0u64, |acc, b| {
                acc.wrapping_mul(31).wrapping_add(u64::from(*b))
            }));
            mix(std::sync::Arc::as_ptr(&w.data) as u64);
        }
    }
    if let Some(c) = cursor {
        if mon_rect.contains(c) {
            mix(c.x.to_bits());
            mix(c.y.to_bits());
        }
    }
    h
}

#[allow(dead_code)]
/// Sygnatura sceny (okna + kursor + przestrzeń + podgląd) — do wykrywania zmian.
fn scene_signature(
    model: &WindowModel,
    current_ws: usize,
    overview: Overview,
    cursor: Option<Point>,
) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: u64| {
        h ^= v;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    mix(current_ws as u64);
    mix(u64::from(overview.on));
    mix(u64::from(overview.blanked));
    mix(wallpaper_stamp());
    if let Some(shot) = overview.shot {
        mix(1);
        mix(shot.selected as u64);
        mix(shot.monitors as u64);
        mix(shot.hover.map_or(u64::MAX, |v| v as u64));
        mix(u64::from(shot.selecting));
        if let Some((a, b)) = shot.area {
            mix(a.x.to_bits());
            mix(a.y.to_bits());
            mix(b.x.to_bits());
            mix(b.y.to_bits());
        }
        mix(shot.pill.x().to_bits());
        mix(shot.pill.y().to_bits());
    } else {
        mix(0);
    }
    if let Some((slot, (px, py))) = overview.drag {
        mix(slot as u64);
        mix(px.to_bits());
        mix(py.to_bits());
    }
    {
        let windows = model.lock_or_recover();
        mix(windows.len() as u64);
        for w in windows.iter() {
            mix(w.x.to_bits());
            mix(w.y.to_bits());
            mix(w.z as u64);
            mix(u64::from(w.focused));
            mix(w.width as u64);
            mix(w.height as u64);
            mix(w.image_offset.0.to_bits());
            mix(w.image_offset.1.to_bits());
            mix(u64::from(w.mapped) | (u64::from(w.minimized) << 1) | (u64::from(w.layer) << 2));
            mix(u64::from(w.ssd) | (u64::from(w.fullscreen) << 1) | (u64::from(w.popup) << 2));
            mix(w.title.as_bytes().iter().fold(0u64, |acc, b| {
                acc.wrapping_mul(31).wrapping_add(u64::from(*b))
            }));
            mix(std::sync::Arc::as_ptr(&w.data) as u64);
        }
    }
    if let Some(c) = cursor {
        mix(c.x.to_bits());
        mix(c.y.to_bits());
    }
    h
}

/// Motyw użytkownika dla dekoracji okien (`~/.config/luna/theme.conf` + styl z pliku).
///
/// Dzięki temu pasek SSD (okna X11 i klienci negocjujący `server_side`) ma dokładnie
/// ten sam wygląd co `HeaderBar` w aplikacjach libLuny — jasny/ciemny, akcent i styl.
fn user_theme() -> Theme {
    let system_dark = libluna_core::appearance::system_dark();
    ThemeSettings::load().resolve_themed(system_dark)
}

/// Buduje scenę okien w rozmiarze wyjścia, przesuniętą o `(ox, oy)` (origin monitora).
#[allow(clippy::too_many_arguments)]
fn build_scene(
    scene: &mut Scene,
    size: Size,
    ox: f64,
    oy: f64,
    model: &WindowModel,
    current_ws: usize,
    ws_count: usize,
    overview: Overview,
    _is_primary: bool,
    cursor: Option<Point>,
    show_cursor: bool,
    theme: &Theme,
    text: &TextEngine,
) {
    let sh = |r: Rect| r.offset(-ox, -oy);
    scene.clear();
    // Ekran wygaszony po bezczynności: czarna klatka zamiast pulpitu i kursora.
    // Wątek wyjścia i tak nie renderuje wtedy nic więcej (sygnatura się nie zmienia),
    // więc CPU/GPU odpoczywają do pierwszego zdarzenia wejścia.
    if overview.blanked {
        scene.fill_rect(
            Rect::from_size(size),
            Color::from_rgba8(0x00, 0x00, 0x00, 0xff),
        );
        return;
    }
    // Tło pulpitu: tapeta z `assets/branding` (gradient zostaje jako zapas).
    match wallpaper_for(size.width.max(1.0) as u32, size.height.max(1.0) as u32) {
        Some(img) => {
            scene.background(img);
        }
        None => fill_vgradient(
            scene,
            Rect::from_size(size),
            Color::from_rgba8(0x23, 0x28, 0x3a, 0xff),
            Color::from_rgba8(0x0f, 0x11, 0x18, 0xff),
        ),
    }

    {
        let windows = model.lock_or_recover();
        let mut order: Vec<&Window> = windows.iter().collect();
        order.sort_by_key(|w| w.z);
        // Przyciemniamy nieaktywne okna tylko wtedy, gdy któreś zwykłe okno ma fokus.
        // Dzięki temu panel/popup z fokusem nie przygasza całego pulpitu.
        let focused_normal = order.iter().any(|w| {
            w.focused
                && !w.popup
                && !w.layer
                && w.mapped
                && !w.minimized
                && w.in_workspace(current_ws)
        });
        let inactive_dim = Color::from_rgba8(0x00, 0x00, 0x00, 0x40);
        // Widoczny obszar tego monitora (globalnie) — do odsiania okien z innych wyjść.
        let out_rect = Rect::from_xywh(ox, oy, size.width, size.height);
        for w in order {
            if !w.mapped || w.minimized || !w.in_workspace(current_ws) {
                continue;
            }
            // Okno z innego monitora nie wnosi pikseli, a jego zaokrąglony clip
            // kosztowałby pełną maskę bufora (kilka ms na klatkę).
            let bounds = w.outer().union(&w.buffer_rect());
            if !bounds.intersects(&out_rect) {
                continue;
            }
            // Tło treści: **nie malujemy czerni** pod buforem klienta. Bufory bywają
            // częściowo przezroczyste (cienie i zaokrąglenia CSD w GTK), a czarne tło
            // przeświecało przez nie jako ramka wokół okna.
            // `buffer_rect()` uwzględnia cienie CSD (`set_window_geometry`) — bufor
            // jest większy od widocznego okna i musi być rysowany z przesunięciem.
            // Zaokrąglone narożniki: wymuszamy dla wszystkich zwykłych okien (także CSD),
            // promieniem z motywu. Maksymalizowane/fullscreen zostają ostre.
            let outer = sh(w.outer());
            // Okno na całą szerokość monitora („przyklejone” do obu krawędzi) nie
            // potrzebuje zaokrągleń — narożniki i tak leżą na krawędzi ekranu, a clip
            // wymusza budowę maski o rozmiarze całego bufora i blokuje szybki blit.
            let spans_width = outer.x() <= 0.5 && outer.right() >= size.width - 0.5;
            // Okna CSD mają w buforze własne marginesy cieni (`image_offset`).
            // Przycinanie do widocznego prostokąta z naszym promieniem obcinało
            // ten cień i zaokrąglenia klienta — okno wyglądało na płaskie.
            // Wyjątek: okna z **naszym** paskiem (SSD) — tam kształt okna definiujemy
            // my (jak w aplikacjach libLuny), więc cień klienta i tak jest nieistotny.
            let shadowed = !w.ssd && (w.image_offset.0 > 0.5 || w.image_offset.1 > 0.5);
            let radius = if !w.popup
                && !w.layer
                && !w.maximized
                && !w.fullscreen
                && !spans_width
                && !shadowed
            {
                theme.radius.large
            } else {
                0.0
            };
            if radius > 0.0 {
                scene.push_clip_rounded(outer, radius);
            }
            // Okno z naszym paskiem: najpierw tło okna (jak aplikacje libLuny).
            // Bez tego przezroczyste piksele zaokrąglonych narożników klienta
            // przepuszczały tapetę i w rogach pojawiały się jasne „zęby”.
            if w.ssd {
                scene.fill_rect(outer, theme.colors.window_bg);
            }
            match &w.dmabuf {
                Some(db) => {
                    scene.image_dmabuf(sh(w.buffer_rect()), std::sync::Arc::clone(db));
                }
                None => {
                    scene.image(sh(w.buffer_rect()), w.data.clone());
                }
            }
            // Pasek tytułu rysujemy **tylko** oknom z naszym SSD (`w.ssd`). Klienty
            // z własnymi dekoracjami (GTK/libadwaita, CSD) nie mają `ssd`, a wcześniej
            // rysowaliśmy im mimo to tytuł i przyciski okna — lewitowały nad treścią.
            if w.ssd {
                // Dokładnie ten sam widget, którego używają aplikacje libLuny
                // (`libluna_widgets::HeaderBar`) — dzięki temu dekoracje kompozytora
                // i aplikacji są identyczne (tło, typografia, kontrolki, odstępy).
                let bar = sh(w.titlebar());
                let mut ctx = PaintCtx::new(scene, theme, text);
                // Podświetlenie kontrolki pod kursorem — bez tego pasek SSD zachowywał
                // się „martwo” w porównaniu z oknami libLuny (tam hover działa).
                let hovered = cursor.and_then(|c| {
                    let p = Point::new(c.x - ox, c.y - oy);
                    if bar.contains(p) {
                        HeaderBar::new().control_at(bar, theme, p)
                    } else {
                        None
                    }
                });
                // Dokładnie ten sam widget i te same kolory co aplikacje libLuny:
                // tło z motywu (`colors.headerbar_bg`), bez barwienia pod treść
                // klienta — inaczej każda aplikacja miała inny pasek niż reszta Luny.
                let mut header = HeaderBar::new()
                    .title(w.title.clone())
                    .controls(ControlsSide::Right);
                // Adaptacja do jasności aplikacji: ciemny pasek nad jasnym oknem,
                // jasny nad ciemnym (neutralne warianty motywu, nie kolor pikseli).
                if let Some((bg, fg)) =
                    crate::decoration::adaptive_header(&w.data, w.image_offset.1, theme)
                {
                    header = header.tinted(bg, fg);
                }
                header.paint(&mut ctx, bar, hovered);
                // **Bez** własnej ramki wokół całego okna — libLuna rysuje tylko pasek
                // (z własną linią pod nim). Dodatkowy `stroke` wyglądał jak obca ramka
                // od innego środowiska, zwłaszcza na oknach X11.
            }
            // Nieaktywne okno lekko przygaszone — sąsiednie okna nie zlewają się
            // w jedną płaszczyznę, a fokus jest widoczny bez zgadywania.
            // Rysujemy **przed** zdjęciem clipu, żeby przygaszenie miało zaokrąglone
            // narożniki razem z oknem.
            if focused_normal && !w.focused && !w.popup && !w.layer {
                scene.fill_rect(outer, inactive_dim);
            }
            if radius > 0.0 {
                scene.pop_clip();
            }
        }
    }

    // Podgląd okien i przestrzeni (Super+Tab) — na każdym monitorze (jego okna).
    if overview.on {
        draw_overview(
            scene,
            size,
            ox,
            oy,
            model,
            current_ws,
            ws_count,
            overview.drag,
        );
    }

    // Poświata „gorącego rogu”: rośnie przy przytrzymaniu kursora w rogu ekranu.
    // Rysujemy ją po oknach, ale przed kursorem i tylko gdy podgląd jest zamknięty
    // (podgląd zajmuje cały ekran własnymi kafelkami).
    if !overview.on {
        if let Some(hint) = overview.corner {
            draw_corner_hint(scene, size, ox, oy, hint, theme);
        }
    }

    // Paleta zrzutów (`Super+F12`) — nad oknami i podglądem, pod kursorem.
    if let Some(shot) = overview.shot {
        draw_shot_ui(scene, size, ox, oy, shot, theme, text);
    }

    // Kursor rysuje ten monitor, na którym leży (tryb DRM — nikt go za nas nie narysuje).
    // Obraz (strzałka, kształt z motywu albo bufor klienta) pochodzi z `crate::cursor`.
    if let Some(c) = cursor.filter(|_| show_cursor) {
        let (cx, cy) = (c.x - ox, c.y - oy);
        if cx >= 0.0 && cy >= 0.0 && cx < size.width && cy < size.height {
            crate::cursor::paint(scene, Point::new(cx, cy));
        }
    }
}

/// Wypełnia prostokąt pionowym gradientem (pasami — tanie, a tło nie jest płaskie).
/// Renderuje scenę jednego wyjścia do **RGBA8** (góra→dół) — dla `wlr-screencopy`.
///
/// `region` jest w pikselach fizycznych wyjścia (przycięty do jego obszaru).
/// Renderer trzymamy w `thread_local`: kontekst EGL jest per wątek, a `dyn Renderer`
/// nie jest `Send`. Preferujemy GPU (ważne, gdy okna klientów to `dmabuf` — software
/// ich nie narysuje), z fallbackiem na CPU.
pub(crate) fn capture_output_rgba(
    state: &crate::state::State,
    index: usize,
    overlay_cursor: bool,
    region: Option<(i32, i32, i32, i32)>,
    hide_ui: bool,
) -> Option<(Vec<u8>, u32, u32)> {
    use std::cell::RefCell;
    thread_local! {
        static CAPTURE: RefCell<Option<Box<dyn Renderer>>> = const { RefCell::new(None) };
    }
    let (ox, oy, ow, oh) = {
        let outs = state.outputs.lock_or_recover();
        let out = outs.get(index)?;
        (
            out.x as f64,
            out.y as f64,
            out.width as u32,
            out.height as u32,
        )
    };
    if ow == 0 || oh == 0 {
        return None;
    }
    // Region w pikselach fizycznych; przycinamy do obszaru wyjścia.
    let (rx, ry, rw, rh) = match region {
        None => (0i32, 0i32, ow as i32, oh as i32),
        Some((x, y, w, h)) => {
            let x0 = x.max(0).min(ow as i32);
            let y0 = y.max(0).min(oh as i32);
            let x1 = x.saturating_add(w).clamp(0, ow as i32);
            let y1 = y.saturating_add(h).clamp(0, oh as i32);
            (x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
        }
    };
    if rw <= 0 || rh <= 0 {
        return None;
    }
    let theme = user_theme();
    let ws = *state.ws_current.lock_or_recover();
    let mut ov = *state.overview.lock_or_recover();
    // Własne zrzuty palety nie zawierają jej nakładki; screencopy (nagrywanie,
    // udostępnianie ekranu) widzi dokładnie to, co użytkownik — razem z paletą.
    if hide_ui {
        ov.shot = None;
    }
    let ws_count = state
        .ws_count_model
        .load(std::sync::atomic::Ordering::Relaxed);
    let cursor = if overlay_cursor {
        *state.cursor.lock_or_recover()
    } else {
        None
    };
    CAPTURE.with(|cell| {
        let mut slot = cell.borrow_mut();
        let stale = slot
            .as_ref()
            .map_or(true, |r| r.width() != ow || r.height() != oh);
        if stale {
            let fresh: Box<dyn Renderer> = match GpuRenderer::new(ow, oh) {
                Ok(renderer) => Box::new(renderer),
                Err(_) => Box::new(SoftwareRenderer::new(ow, oh)),
            };
            *slot = Some(fresh);
        }
        let renderer = slot.as_mut()?;
        renderer.resize(ow, oh);
        let mut scene = Scene::new();
        build_scene(
            &mut scene,
            Size::new(f64::from(ow), f64::from(oh)),
            ox,
            oy,
            &state.windows,
            ws,
            ws_count,
            ov,
            true,
            cursor,
            overlay_cursor,
            &theme,
            renderer.text_engine(),
        );
        renderer.draw(&scene, 1.0, Color::BLACK);
        let full = renderer.data();
        if rw as u32 == ow && rh as u32 == oh && rx == 0 && ry == 0 {
            return Some((full.to_vec(), ow, oh));
        }
        let stride = ow as usize * 4;
        let mut out = Vec::with_capacity(rw as usize * rh as usize * 4);
        for y in 0..rh as usize {
            let start = (ry as usize + y) * stride + rx as usize * 4;
            out.extend_from_slice(&full[start..start + rw as usize * 4]);
        }
        Some((out, rw as u32, rh as u32))
    })
}

/// Tapeta pulpitu (z `assets/branding`), przeskalowana „cover” do rozmiaru wyjścia.
///
/// Skalowanie robimy **raz** na rozmiar i trzymamy w cache — w pętli renderowania
/// idzie wtedy tani blit 1:1, a nie skalowanie pełnego ekranu co klatkę.
fn wallpaper_for(w: u32, h: u32) -> Option<std::sync::Arc<ImageData>> {
    type SizedCache = Vec<(u64, (u32, u32), std::sync::Arc<ImageData>)>;
    let (w, h) = (w.max(1), h.max(1));
    let stamp = wallpaper_stamp();
    static CACHE: std::sync::OnceLock<std::sync::Mutex<SizedCache>> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(Vec::new()));
    let mut guard = cache.lock_or_recover();
    // Nowa tapeta (mtime / ścieżka) unieważnia przeskalowane kopie starych rozmiarów.
    guard.retain(|(cached_stamp, _, _)| *cached_stamp == stamp);
    if let Some((_, _, img)) = guard
        .iter()
        .find(|(cached_stamp, (cw, ch), _)| *cached_stamp == stamp && *cw == w && *ch == h)
    {
        return Some(std::sync::Arc::clone(img));
    }
    let src = wallpaper_source(stamp)?;
    let img = std::sync::Arc::new(libluna_render::wallpaper::cover(&src, w, h));
    guard.push((stamp, (w, h), std::sync::Arc::clone(&img)));
    Some(img)
}

/// Sygnatura tapety: zmienia się przy podmianie pliku w Ustawieniach.
///
/// Kompozytor działa dalej bez restartu — wystarczy zapis `wallpaper.conf`.
fn wallpaper_stamp() -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for var in ["LUNA_WALLPAPER", "SELENE_WALLPAPER", "SELENE_NO_WALLPAPER"] {
        std::env::var_os(var).hash(&mut hasher);
    }
    if let Some(config) = libluna_render::wallpaper::config_path() {
        if let Ok(meta) = std::fs::metadata(&config) {
            meta.modified().ok().hash(&mut hasher);
            meta.len().hash(&mut hasher);
        }
    }
    if let Some(path) = libluna_render::wallpaper::configured_path() {
        path.hash(&mut hasher);
        if let Ok(meta) = std::fs::metadata(&path) {
            meta.modified().ok().hash(&mut hasher);
            meta.len().hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Dekoduje tapetę raz na wersję pliku. Źródło: `LUNA_WALLPAPER` / `SELENE_WALLPAPER` /
/// `~/.config/luna/wallpaper.conf`, a w ostateczności wbudowana grafika.
/// Wyłączenie (powrót do gradientu): `SELENE_NO_WALLPAPER=1`.
fn wallpaper_source(stamp: u64) -> Option<std::sync::Arc<ImageData>> {
    type SourceCache = Option<(u64, Option<std::sync::Arc<ImageData>>)>;
    static SRC: std::sync::OnceLock<std::sync::Mutex<SourceCache>> = std::sync::OnceLock::new();
    let cache = SRC.get_or_init(|| std::sync::Mutex::new(None));
    let mut guard = cache.lock_or_recover();
    if let Some((cached_stamp, src)) = guard.as_ref() {
        if *cached_stamp == stamp {
            return src.clone();
        }
    }
    let src = if std::env::var_os("SELENE_NO_WALLPAPER").is_some() {
        None
    } else {
        libluna_render::wallpaper::configured_path()
            .and_then(|path| libluna_render::wallpaper::load_path(&path))
            .map(std::sync::Arc::new)
            .or_else(|| {
                libluna_render::wallpaper::decode_png(include_bytes!(
                    "../../assets/branding/paganlinux.png"
                ))
                .map(std::sync::Arc::new)
            })
    };
    *guard = Some((stamp, src.clone()));
    src
}

fn fill_vgradient(scene: &mut Scene, rect: Rect, top: Color, bottom: Color) {
    const BANDS: usize = 64;
    let h = rect.height() / BANDS as f64;
    for i in 0..BANDS {
        let t = i as f32 / (BANDS - 1) as f32;
        let c = top.lerp(bottom, t);
        scene.fill_rect(
            Rect::from_xywh(rect.x(), rect.y() + i as f64 * h, rect.width(), h + 1.0),
            c,
        );
    }
}

/// Rysuje podgląd okien i przestrzeni — kafelki z podglądem treści + pasek przestrzeni.
#[allow(clippy::too_many_arguments)]
/// Rysuje poświatę „gorącego rogu”.
///
/// Kilka półprzezroczystych kół zakotwiczonych w narożniku ekranu (widoczna jest
/// tylko ćwiartka) daje miękką poświatę bez gradientów. Rozmiar i krycie rosną
/// z postępem przytrzymania, więc użytkownik widzi, że akcja zaraz zadziała.
fn draw_corner_hint(
    scene: &mut Scene,
    size: Size,
    ox: f64,
    oy: f64,
    hint: CornerHint,
    theme: &Theme,
) {
    let progress = hint.progress.clamp(0.0, 1.0) as f32;
    let (x, y) = match hint.kind {
        CornerKind::TopLeft => (ox, oy + hint.top_inset),
        CornerKind::BottomLeft => (ox, oy + size.height),
        CornerKind::BottomRight => (ox + size.width, oy + size.height),
    };
    let center = Point::new(x - ox, y - oy);
    let accent = theme.colors.accent;
    // Warstwy: (skala promienia, krycie). Środek jaśniejszy, brzeg delikatny.
    let radius = 56.0 + 84.0 * f64::from(progress);
    for (scale, alpha) in [
        (1.0f64, 0.08 + 0.10 * progress),
        (0.74, 0.09 + 0.11 * progress),
        (0.48, 0.10 + 0.12 * progress),
    ] {
        scene.fill_circle(center, radius * scale, accent.scale_alpha(alpha));
    }
}

/// Paleta zrzutu ekranu: pastylka z opcjami, podświetlenie monitora i prostokąt
/// zaznaczania. Rysowana tylko na monitorze, na którym leży pastylka (reszta
/// dostaje przyciemnienie i podświetlenie), dzięki czemu działa na wielu ekranach.
fn draw_shot_ui(
    scene: &mut Scene,
    size: Size,
    ox: f64,
    oy: f64,
    ui: ShotUi,
    theme: &Theme,
    text: &TextEngine,
) {
    let colors = theme.colors;
    let own = Rect::from_xywh(ox, oy, size.width, size.height);
    let mut ctx = PaintCtx::new(scene, theme, text);

    // Przyciemnienie pulpitu — pastylka i zaznaczenie muszą być czytelne na każdym tle.
    let dim = if ui.selecting { 0x66 } else { 0x33 };
    ctx.scene.fill_rect(
        Rect::from_size(size),
        Color::from_rgba8(0x00, 0x00, 0x00, dim),
    );

    if let Some(hl) = ui.highlight.filter(|rect| rect.intersects(&own)) {
        let local = Rect::from_xywh(
            hl.x() - ox + 6.0,
            hl.y() - oy + 6.0,
            (hl.width() - 12.0).max(1.0),
            (hl.height() - 12.0).max(1.0),
        );
        ctx.scene
            .stroke_rounded_rect(local, theme.radius.large, 3.0, colors.accent);
    }

    if let Some((a, b)) = ui.area {
        let rect = Rect::from_xywh(
            a.x.min(b.x) - ox,
            a.y.min(b.y) - oy,
            (a.x - b.x).abs(),
            (a.y - b.y).abs(),
        );
        if rect.width() > 0.0 && rect.height() > 0.0 {
            ctx.scene
                .fill_rounded_rect(rect, 4.0, colors.accent.with_alpha(0.14));
            ctx.scene.stroke_rounded_rect(rect, 4.0, 2.0, colors.accent);
            let label = format!(
                "{} × {}",
                rect.width().round() as i64,
                rect.height().round() as i64
            );
            let pill = Rect::from_xywh(rect.x(), (rect.y() - 28.0).max(4.0), 92.0, 22.0);
            ctx.scene.fill_rounded_rect(pill, 11.0, colors.accent);
            ctx.label_center(
                pill.center().x,
                pill.center().y,
                &label,
                theme.typography.caption.size,
                weight::MEDIUM,
                colors.accent_fg,
            );
        }
    }

    if ui.selecting {
        let hint = tr("Drag to select an area · Esc to cancel");
        let body = theme.typography.body.size;
        let width = (ctx.measure(&hint, body, weight::MEDIUM) + 40.0).min(size.width - 24.0);
        let pill = Rect::from_xywh(
            own.center().x - width / 2.0,
            own.bottom() - 72.0,
            width,
            40.0,
        );
        ctx.scene.fill_rounded_rect(pill, 20.0, colors.card_bg);
        ctx.scene
            .stroke_rounded_rect(pill, 20.0, 1.0, colors.border);
        ctx.label_center(
            pill.center().x,
            pill.center().y,
            &hint,
            body,
            weight::MEDIUM,
            colors.text,
        );
        return;
    }

    if !ui.pill.intersects(&own) {
        return;
    }
    let pill = Rect::from_xywh(
        ui.pill.x() - ox,
        ui.pill.y() - oy,
        ui.pill.width(),
        ui.pill.height(),
    );
    // Podpis nad pastylką — od razu wiadomo, do czego służą cyfry.
    let caption = tr("Screenshot");
    let caption_size = theme.typography.caption.size;
    let caption_w = ctx.measure(&caption, caption_size, weight::MEDIUM) + 22.0;
    let caption_rect = Rect::from_xywh(
        pill.center().x - caption_w / 2.0,
        pill.y() - 30.0,
        caption_w,
        22.0,
    );
    ctx.scene
        .fill_rounded_rect(caption_rect, 11.0, colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(caption_rect, 11.0, 1.0, colors.border);
    ctx.label_center(
        caption_rect.center().x,
        caption_rect.center().y,
        &caption,
        caption_size,
        weight::MEDIUM,
        colors.dim_text,
    );
    ctx.scene
        .fill_rounded_rect(pill, pill.height() / 2.0, colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(pill, pill.height() / 2.0, 1.0, colors.border);

    let body = theme.typography.body.size;
    for (index, chip) in crate::screenshot::chip_rects(ui.pill, ui.monitors) {
        let chip = Rect::from_xywh(chip.x() - ox, chip.y() - oy, chip.width(), chip.height());
        let selected = index == ui.selected;
        let hovered = ui.hover == Some(index);
        if selected {
            ctx.scene
                .fill_rounded_rect(chip, chip.height() / 2.0, colors.accent);
        } else if hovered {
            ctx.scene
                .fill_rounded_rect(chip, chip.height() / 2.0, colors.shade);
        }
        let fg = if selected {
            colors.accent_fg
        } else {
            colors.text
        };
        ctx.label_center(
            chip.center().x,
            chip.center().y,
            &crate::screenshot::chip_label(index, ui.monitors),
            body,
            weight::MEDIUM,
            fg,
        );
    }
}

fn draw_overview(
    scene: &mut Scene,
    size: Size,
    ox: f64,
    oy: f64,
    model: &WindowModel,
    current_ws: usize,
    ws_count: usize,
    drag: Option<(usize, (f64, f64))>,
) {
    // Przygaszenie tła.
    scene.fill_rect(
        Rect::from_size(size),
        Color::from_rgba8(0x0b, 0x0d, 0x13, 0xd6),
    );
    let slots = {
        let w = model.lock_or_recover();
        crate::state::overview_slots_on(&w, current_ws, ox, size.width)
    };
    let tiles = crate::state::overview_tiles(slots.len(), (size.width, size.height));
    let windows = model.lock_or_recover();
    for (k, &slot) in slots.iter().enumerate() {
        let (Some(r), Some(w)) = (tiles.get(k), windows.get(slot)) else {
            continue;
        };
        let title_h = crate::state::OVERVIEW_TITLE;
        scene.fill_rounded_rect(*r, 8.0, Color::from_rgba8(0x1a, 0x1d, 0x25, 0xff));
        let bar = Rect::from_xywh(r.x(), r.y(), r.width(), title_h);
        scene.fill_rounded_rect(bar, 8.0, Color::from_rgba8(0x2c, 0x2f, 0x38, 0xff));
        let img = Rect::from_xywh(
            r.x(),
            r.y() + title_h,
            r.width(),
            (r.height() - title_h).max(1.0),
        );
        match &w.dmabuf {
            Some(db) => scene.image_dmabuf(img, std::sync::Arc::clone(db)),
            None => scene.image(img, w.data.clone()),
        };
        scene.stroke_rounded_rect(*r, 8.0, 1.5, Color::from_rgba8(0x6c, 0x74, 0x86, 0xff));
        scene.text(
            TextRun::new(Point::new(r.x() + 10.0, r.y() + 18.0), &w.title, 13.0)
                .with_color(Color::from_rgba8(0xee, 0xee, 0xee, 0xff)),
        );
    }
    // Pasek przestrzeni roboczych.
    let chips = crate::state::overview_ws_chips(ws_count, (size.width, size.height));
    for (i, c) in chips.iter().enumerate() {
        let bg = if i == current_ws {
            Color::from_rgba8(0x5b, 0x8d, 0xef, 0xff)
        } else {
            Color::from_rgba8(0x2c, 0x2f, 0x38, 0xff)
        };
        scene.fill_rounded_rect(*c, c.height() / 2.0, bg);
        scene.text(
            TextRun::new(
                Point::new(c.center().x - 6.0, c.center().y + 6.0),
                &format!("{}", i + 1),
                16.0,
            )
            .with_color(Color::from_rgba8(0xf5, 0xf5, 0xf5, 0xff)),
        );
    }
    // „Duch” przeciąganego okna pod kursorem.
    if let Some((slot, (px, py))) = drag {
        let local = Point::new(px - ox, py - oy);
        let g = Rect::from_xywh(local.x - 90.0, local.y - 60.0, 180.0, 120.0);
        scene.fill_rounded_rect(g, 8.0, Color::from_rgba8(0x5b, 0x8d, 0xef, 0xcc));
        scene.stroke_rounded_rect(g, 8.0, 2.0, Color::from_rgba8(0xff, 0xff, 0xff, 0xdd));
        if let Some(w) = windows.get(slot) {
            scene.text(
                TextRun::new(Point::new(g.x() + 10.0, g.y() + 20.0), &w.title, 13.0)
                    .with_color(Color::from_rgba8(0xff, 0xff, 0xff, 0xff)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_render::SoftwareRenderer;

    /// Damage z `Scene` (f64) musi trafić w piksele fizyczne: podłoga na
    /// początku, sufit na końcu, przycięcie do bufora.
    #[test]
    fn pixel_rect_clamps_and_rounds() {
        let r = pixel_rect(Rect::from_xywh(10.2, 20.7, 30.6, 5.1), 100, 100).unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (10, 20, 31, 6));
        // Całkowicie poza ekranem → pusty prostokąt (sam flip, bez kopiowania).
        let z = pixel_rect(Rect::from_xywh(200.0, 200.0, 10.0, 10.0), 100, 100).unwrap();
        assert_eq!((z.w, z.h), (0, 0));
        // Wystający poza prawą/dolną krawędź zostaje przycięty.
        let c = pixel_rect(Rect::from_xywh(90.0, 90.0, 50.0, 50.0), 100, 100).unwrap();
        assert_eq!((c.x, c.y, c.w, c.h), (90, 90, 10, 10));
    }

    #[test]
    fn corner_glow_paints_the_corner() {
        let theme = Theme::dark();
        let size = Size::new(200.0, 200.0);
        let mut scene = Scene::new();
        scene.fill_rect(Rect::from_size(size), Color::from_rgba8(0, 0, 0, 255));
        draw_corner_hint(
            &mut scene,
            size,
            0.0,
            0.0,
            CornerHint {
                kind: CornerKind::BottomRight,
                progress: 0.9,
                top_inset: 0.0,
            },
            &theme,
        );
        let mut renderer = SoftwareRenderer::new(200, 200);
        renderer.draw(&scene, 1.0, Color::from_rgba8(0, 0, 0, 255));
        let data = renderer.data();
        let at = |x: usize, y: usize| {
            let i = (y * 200 + x) * 4;
            [data[i], data[i + 1], data[i + 2]]
        };
        assert_ne!(at(199, 199), [0, 0, 0], "róg ma być podświetlony");
        assert_ne!(at(170, 170), [0, 0, 0], "poświata ma mieć miękki zasięg");
        assert_eq!(at(60, 60), [0, 0, 0], "środek ekranu zostaje czysty");
    }

    #[test]
    fn blanked_scene_is_black() {
        use std::sync::{Arc, Mutex};
        let model: WindowModel = Arc::new(Mutex::new(Vec::new()));
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut scene = Scene::new();
        let overview = Overview {
            blanked: true,
            ..Overview::default()
        };
        build_scene(
            &mut scene,
            Size::new(64.0, 64.0),
            0.0,
            0.0,
            &model,
            0,
            4,
            overview,
            true,
            None,
            false,
            &theme,
            &text,
        );
        let mut renderer = SoftwareRenderer::new(64, 64);
        renderer.draw(&scene, 1.0, Color::from_rgba8(0, 0, 0, 255));
        assert!(
            renderer
                .data()
                .chunks_exact(4)
                .all(|px| px[0] == 0 && px[1] == 0 && px[2] == 0),
            "wygaszony ekran ma być cały czarny (bez tapety, okien i kursora)"
        );
    }

    #[test]
    fn software_cursor_paints_pixels() {
        // Kursor w scenie rysuje `crate::cursor` (domyślnie wbudowana strzałka).
        let mut scene = Scene::new();
        crate::cursor::paint(&mut scene, Point::new(8.0, 8.0));
        let mut renderer = SoftwareRenderer::new(48, 48);
        renderer.draw(&scene, 1.0, Color::from_rgba8(0, 0, 0, 255));
        let painted = renderer
            .data()
            .chunks_exact(4)
            .any(|px| px[0] > 0 || px[1] > 0 || px[2] > 0);
        assert!(painted, "kursor powinien zostawić niezerowe piksele");
    }

    #[test]
    #[ignore = "generuje podglad do docs/images (uruchom recznie)"]
    fn render_desktop_preview() {
        use std::sync::{Arc, Mutex};
        let model: WindowModel = Arc::new(Mutex::new(Vec::new()));
        let mut scene = Scene::new();
        build_scene(
            &mut scene,
            Size::new(1920.0, 1080.0),
            0.0,
            0.0,
            &model,
            0,
            1,
            Overview::default(),
            true,
            None,
            true,
            &Theme::dark(),
            &TextEngine::none(),
        );
        let mut renderer = SoftwareRenderer::new(1920, 1080);
        renderer.draw(&scene, 1.0, Color::from_rgba8(0x1e, 0x20, 0x28, 0xff));
        let png = renderer.encode_png().expect("PNG");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../docs/images/luna-desktop.png"
        );
        std::fs::write(path, png).expect("zapis PNG");
        println!("saved {path}");
    }

    #[test]
    fn power_saver_caps_presentation_at_30_fps() {
        assert_eq!(throttle_delay(false, Duration::from_millis(5)), None);
        assert_eq!(
            throttle_delay(true, Duration::from_millis(10)),
            Some(Duration::from_millis(23))
        );
        assert_eq!(throttle_delay(true, Duration::from_millis(40)), None);
    }

    #[test]
    fn window_content_is_visible_in_render() {
        use std::sync::{Arc, Mutex};
        let mut rgba = vec![0u8; 100 * 80 * 4];
        for px in rgba.chunks_exact_mut(4) {
            px.copy_from_slice(&[0xff, 0x00, 0x00, 0xff]);
        }
        let model: WindowModel = Arc::new(Mutex::new(vec![Window {
            x: 10.0,
            y: 10.0,
            width: 100,
            height: 80,
            image_offset: (0.0, 0.0),
            z: 1,
            title: String::new(),
            popup: false,
            layer: false,
            mapped: true,
            focused: true,
            workspace: 0,
            input: None,
            input_capture: false,
            minimized: false,
            maximized: false,
            fullscreen: false,
            ssd: false,
            saved: None,
            image_size_override: None,
            dmabuf: None,
            data: Arc::new(ImageData::new(100, 80, rgba)),
        }]));
        let mut scene = Scene::new();
        build_scene(
            &mut scene,
            Size::new(200.0, 150.0),
            0.0,
            0.0,
            &model,
            0,
            1,
            Overview::default(),
            true,
            None,
            false,
            &Theme::dark(),
            &TextEngine::none(),
        );
        let mut renderer = SoftwareRenderer::new(200, 150);
        renderer.draw(&scene, 1.0, Color::BLACK);
        let px = &renderer.data()[(50 * 200 + 50) * 4..][..4];
        assert_eq!(px, [0xff, 0x00, 0x00, 0xff], "treść okna musi być widoczna");

        // Ten sam test na GPU — tu wychodziły różnice (treść okna bywała pomijana).
        if let Ok(mut gpu) = libluna_render::gpu::GpuRenderer::new(200, 150) {
            gpu.draw(&scene, 1.0, Color::BLACK);
            let px = &gpu.data()[(50 * 200 + 50) * 4..][..4];
            assert_eq!(
                px,
                [0xff, 0x00, 0x00, 0xff],
                "GPU: treść okna musi być widoczna"
            );
        }
    }

    #[test]
    fn desktop_background_is_the_wallpaper() {
        use std::sync::{Arc, Mutex};
        let model: WindowModel = Arc::new(Mutex::new(Vec::new()));
        let mut scene = Scene::new();
        build_scene(
            &mut scene,
            Size::new(320.0, 180.0),
            0.0,
            0.0,
            &model,
            0,
            1,
            Overview::default(),
            true,
            None,
            true,
            &Theme::dark(),
            &TextEngine::none(),
        );
        // Pierwsze polecenie to tło pulpitu — tapeta przeskalowana dokładnie do rozmiaru
        // wyjścia (renderer kopiuje ją wprost), a nie gradient z 64 pasów.
        match &scene.commands()[0] {
            libluna_render::DrawCommand::Background { data } => {
                assert_eq!((data.width, data.height), (320, 180));
            }
            other => panic!("oczekiwano tapety, a jest {other:?}"),
        }
    }

    #[test]
    fn unfocused_window_is_dimmed() {
        use libluna_render::DrawCommand;
        use std::sync::{Arc, Mutex};
        let data = Arc::new(ImageData::new(100, 80, vec![0xffu8; 100 * 80 * 4]));
        let window = |x: f64, z: i32, focused: bool| Window {
            x,
            y: 0.0,
            width: 100,
            height: 80,
            image_offset: (0.0, 0.0),
            z,
            title: String::new(),
            popup: false,
            layer: false,
            mapped: true,
            focused,
            workspace: 0,
            input: None,
            input_capture: false,
            minimized: false,
            maximized: false,
            fullscreen: false,
            ssd: false,
            saved: None,
            image_size_override: None,
            dmabuf: None,
            data: data.clone(),
        };
        let model: WindowModel = Arc::new(Mutex::new(vec![
            window(0.0, 1, false),
            window(20.0, 2, true),
        ]));
        let mut scene = Scene::new();
        build_scene(
            &mut scene,
            Size::new(400.0, 300.0),
            0.0,
            0.0,
            &model,
            0,
            1,
            Overview::default(),
            true,
            None,
            false,
            &Theme::dark(),
            &TextEngine::none(),
        );
        let dim = Color::from_rgba8(0x00, 0x00, 0x00, 0x40);
        let dimmed: Vec<Rect> = scene
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::FillRect { rect, color } if *color == dim => Some(*rect),
                _ => None,
            })
            .collect();
        assert_eq!(dimmed, vec![Rect::from_xywh(0.0, 0.0, 100.0, 80.0)]);
    }

    #[test]
    fn ssd_header_highlights_hovered_close_like_libluna() {
        use libluna_render::DrawCommand;
        use std::sync::{Arc, Mutex};
        let data = Arc::new(ImageData::new(400, 300, vec![0u8; 400 * 300 * 4]));
        let windows = vec![Window {
            x: 0.0,
            y: 0.0,
            width: 400,
            height: 300,
            image_offset: (0.0, 0.0),
            z: 1,
            title: "Test".to_owned(),
            popup: false,
            layer: false,
            mapped: true,
            focused: false,
            workspace: 0,
            input: None,
            input_capture: false,
            minimized: false,
            maximized: false,
            fullscreen: false,
            ssd: true,
            saved: None,
            image_size_override: None,
            dmabuf: None,
            data,
        }];
        let model: WindowModel = Arc::new(Mutex::new(windows));
        let theme = Theme::light();
        let has_error_fill = |scene: &Scene| {
            scene.commands().iter().any(|c| {
                matches!(
                    c,
                    DrawCommand::FillRoundedRect { color, .. } if *color == theme.colors.error
                )
            })
        };
        // Środek kontrolki „zamknij”: szerokość 400, pad 6, bok 30.
        let close = Point::new(400.0 - 6.0 - 30.0 + 15.0, 23.0);
        let mut scene = Scene::new();
        build_scene(
            &mut scene,
            Size::new(800.0, 600.0),
            0.0,
            0.0,
            &model,
            0,
            1,
            Overview::default(),
            true,
            Some(close),
            false,
            &theme,
            &TextEngine::none(),
        );
        assert!(
            has_error_fill(&scene),
            "hover na „zamknij” powinien dać czerwone tło jak HeaderBar libLuny"
        );

        // Poza kontrolkami — brak podświetlenia.
        let mut scene = Scene::new();
        build_scene(
            &mut scene,
            Size::new(800.0, 600.0),
            0.0,
            0.0,
            &model,
            0,
            1,
            Overview::default(),
            true,
            Some(Point::new(100.0, 23.0)),
            false,
            &theme,
            &TextEngine::none(),
        );
        assert!(!has_error_fill(&scene));
    }

    #[test]
    fn ssd_header_uses_theme_colors_like_apps() {
        use libluna_render::DrawCommand;
        use std::sync::{Arc, Mutex};
        // Treść w jaskrawym kolorze **nie** może zmieniać paska: w aplikacjach
        // libLuny pasek bierze `colors.headerbar_bg` z motywu, więc dekoracje okna
        // muszą wyglądać tak samo niezależnie od tego, co rysuje klient.
        let (w, h) = (400u32, 300u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for px in rgba.chunks_exact_mut(4) {
            px.copy_from_slice(&[0x11, 0x22, 0x33, 0xff]);
        }
        let model: WindowModel = Arc::new(Mutex::new(vec![Window {
            x: 0.0,
            y: 0.0,
            width: 400,
            height: 300,
            image_offset: (0.0, 0.0),
            z: 1,
            title: "Test".to_owned(),
            popup: false,
            layer: false,
            mapped: true,
            focused: false,
            workspace: 0,
            input: None,
            input_capture: false,
            minimized: false,
            maximized: false,
            fullscreen: false,
            ssd: true,
            saved: None,
            image_size_override: None,
            dmabuf: None,
            data: Arc::new(ImageData::new(w, h, rgba)),
        }]));
        let mut scene = Scene::new();
        let theme = Theme::dark();
        build_scene(
            &mut scene,
            Size::new(800.0, 600.0),
            0.0,
            0.0,
            &model,
            0,
            1,
            Overview::default(),
            true,
            None,
            false,
            &theme,
            &TextEngine::none(),
        );
        assert!(
            scene.commands().iter().any(|c| matches!(
                c,
                DrawCommand::FillRect { rect, color }
                    if *color == theme.colors.headerbar_bg
                        && rect.y() == 0.0
                        && rect.height() == crate::state::TITLEBAR
            )),
            "pasek SSD ma kolory motywu, jak HeaderBar w aplikacjach"
        );
        // Tło okna pod treścią — zaokrąglone narożniki nie przepuszczają tapety.
        assert!(
            scene.commands().iter().any(|c| matches!(
                c,
                DrawCommand::FillRect { rect, color }
                    if *color == theme.colors.window_bg
                        && rect.y() == 0.0
                        && rect.height() == crate::state::TITLEBAR + 300.0
            )),
            "okno z naszym paskiem ma tło `window_bg`, jak aplikacje libLuny"
        );
    }

    #[test]
    #[ignore = "benchmark reczny"]
    fn bench_1080p_frame() {
        use std::sync::{Arc, Mutex};
        use std::time::Instant;
        let model: WindowModel = Arc::new(Mutex::new(Vec::new()));
        let bg = Color::from_rgba8(0x1e, 0x20, 0x28, 0xff);
        let mut renderer = SoftwareRenderer::new(1920, 1080);
        let n = 30;
        let t = Instant::now();
        for _ in 0..n {
            let mut scene = Scene::new();
            build_scene(
                &mut scene,
                Size::new(1920.0, 1080.0),
                0.0,
                0.0,
                &model,
                0,
                1,
                Overview::default(),
                true,
                Some(Point::new(100.0, 100.0)),
                true,
                &Theme::dark(),
                &TextEngine::none(),
            );
            renderer.draw(&scene, 1.0, bg);
        }
        println!(
            "render 1920x1080: {:.2} ms/klatke",
            t.elapsed().as_secs_f64() * 1000.0 / n as f64
        );
    }

    #[test]
    #[ignore = "benchmark reczny"]
    fn bench_1080p_with_panel() {
        use std::sync::{Arc, Mutex};
        use std::time::Instant;
        let data = Arc::new(libluna_render::ImageData::new(
            1920,
            680,
            vec![128u8; 1920 * 680 * 4],
        ));
        let windows = vec![Window {
            x: 0.0,
            y: 0.0,
            width: 1920,
            height: 680,
            image_offset: (0.0, 0.0),
            z: 1_000_000,
            title: String::new(),
            popup: false,
            layer: true,
            mapped: true,
            focused: false,
            workspace: 0,
            input: None,
            input_capture: false,
            minimized: false,
            maximized: false,
            fullscreen: false,
            ssd: true,
            saved: None,
            image_size_override: None,
            dmabuf: None,
            data,
        }];
        let model: WindowModel = Arc::new(Mutex::new(windows));
        let bg = Color::from_rgba8(0x1e, 0x20, 0x28, 0xff);
        let mut renderer = SoftwareRenderer::new(1920, 1080);
        let n = 20;
        // Sam render sceny:
        let t = Instant::now();
        for _ in 0..n {
            let mut scene = Scene::new();
            build_scene(
                &mut scene,
                Size::new(1920.0, 1080.0),
                0.0,
                0.0,
                &model,
                0,
                1,
                Overview::default(),
                true,
                Some(Point::new(100.0, 100.0)),
                true,
                &Theme::dark(),
                &TextEngine::none(),
            );
            renderer.draw(&scene, 1.0, bg);
        }
        println!(
            "render 1080p + panel 1920x680: {:.2} ms/klatke",
            t.elapsed().as_secs_f64() * 1000.0 / n as f64
        );
    }

    #[test]
    #[ignore = "benchmark reczny"]
    fn bench_sparse_panel() {
        use std::sync::{Arc, Mutex};
        use std::time::Instant;
        let (w, h) = (1920u32, 680u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        let mut fill = |x0: u32, y0: u32, x1: u32, y1: u32| {
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = ((y * w + x) * 4) as usize;
                    rgba[i..i + 4].copy_from_slice(&[30, 30, 30, 255]);
                }
            }
        };
        // Pasek na całą szerokość + launcher 720×600 (reszta przezroczysta).
        fill(0, 0, w, 48);
        fill(600, 72, 1320, 672);
        let data = Arc::new(ImageData::new(w, h, rgba).with_sparse(true));
        let windows = vec![Window {
            x: 0.0,
            y: 0.0,
            width: w as i32,
            height: h as i32,
            image_offset: (0.0, 0.0),
            z: 1_000_000,
            title: String::new(),
            popup: false,
            layer: true,
            mapped: true,
            focused: false,
            workspace: 0,
            input: None,
            input_capture: false,
            minimized: false,
            maximized: false,
            fullscreen: false,
            ssd: true,
            saved: None,
            image_size_override: None,
            dmabuf: None,
            data,
        }];
        let model: WindowModel = Arc::new(Mutex::new(windows));
        let bg = Color::from_rgba8(0x1e, 0x20, 0x28, 0xff);
        let mut renderer = SoftwareRenderer::new(1920, 1080);
        let n = 20;
        let t = Instant::now();
        for _ in 0..n {
            let mut scene = Scene::new();
            build_scene(
                &mut scene,
                Size::new(1920.0, 1080.0),
                0.0,
                0.0,
                &model,
                0,
                1,
                Overview::default(),
                true,
                Some(Point::new(100.0, 100.0)),
                true,
                &Theme::dark(),
                &TextEngine::none(),
            );
            renderer.draw(&scene, 1.0, bg);
        }
        println!(
            "render 1080p + sparse panel: {:.2} ms/klatke",
            t.elapsed().as_secs_f64() * 1000.0 / n as f64
        );
    }

    #[test]
    fn shot_palette_and_area_selection_render() {
        use crate::screenshot::pill_rect;
        use std::sync::{Arc, Mutex};
        let model: WindowModel = Arc::new(Mutex::new(Vec::new()));
        let theme = Theme::dark();
        let size = Size::new(1280.0, 800.0);
        let pill = pill_rect(Rect::from_size(size), 2);
        let palette = ShotUi {
            selected: 1,
            monitors: 2,
            hover: Some(2),
            selecting: false,
            area: None,
            highlight: Some(Rect::from_xywh(0.0, 0.0, 640.0, 400.0)),
            pill,
        };
        let accent = |scene: &Scene| {
            scene.commands().iter().any(|command| {
                matches!(command, DrawCommand::FillRoundedRect { color, .. } if *color == theme.colors.accent)
            })
        };
        let mut scene = Scene::new();
        build_scene(
            &mut scene,
            size,
            0.0,
            0.0,
            &model,
            0,
            1,
            Overview {
                shot: Some(palette),
                ..Overview::default()
            },
            true,
            None,
            true,
            &theme,
            &TextEngine::none(),
        );
        assert!(
            accent(&scene),
            "zaznaczony chip i obrys monitora rysują się akcentem"
        );

        let selecting = ShotUi {
            selecting: true,
            area: Some((Point::new(100.0, 100.0), Point::new(400.0, 300.0))),
            hover: None,
            highlight: None,
            ..palette
        };
        let mut scene = Scene::new();
        build_scene(
            &mut scene,
            size,
            0.0,
            0.0,
            &model,
            0,
            1,
            Overview {
                shot: Some(selecting),
                ..Overview::default()
            },
            true,
            None,
            true,
            &theme,
            &TextEngine::none(),
        );
        assert!(accent(&scene), "prostokąt zaznaczania rysuje się akcentem");
    }
}
