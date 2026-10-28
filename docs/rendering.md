# Renderowanie i wydajność

Ten dokument opisuje obecny backend rysowania, sposób pomiaru i **kryteria decyzji
o GPU**. Liczby są punktem odniesienia — nie obietnicą, że CPU wystarczy na zawsze.

## 1. Architektura

```text
widgety / aplikacje
        │  budują Scene (piksele logiczne)
        ▼
   libluna-render::Scene
        │  Renderer::draw(scene, scale, background)
        ▼
┌───────────────────────┐
│  SoftwareRenderer     │  ← CPU: tiny-skia + fontdue, bufor RGBA8888
│  (backend CPU)        │     gotowy do wl_shm
└───────────────────────┘
┌───────────────────────┐
│  GpuRenderer          │  ← EGL (surfaceless) + GLES2; LUNA_GPU=1
│  (backend GPU)        │     obrazy/kształty na GPU, tekst/wektory w warstwach
└───────────────────────┘
        │  data() → wl_shm (dla GPU: readback)
        ▼
     Selene / kompozytor
```

Kontrakt backendu to `libluna_render::Renderer` (`renderer.rs`):

- `name()`, `resize()`, `width()`, `height()`,
- `draw(&Scene, scale, background)`,
- `data() -> &[u8]` (RGBA8888),
- `encode_png()`,
- `text_engine()`.

`libluna-platform` trzyma `Box<dyn Renderer>`, więc podmiana backendu nie wymaga
zmian w widgetach ani aplikacjach. Backend GPU (`GpuRenderer`, `LUNA_GPU=1`)
spełnia ten sam kontrakt; szczegóły EGL/dmabuf/synchronizacji zostają w jego
implementacji. Przy braku EGL platforma wraca do CPU z jednym logiem.

### Co robi `Scene`, a czego nie robi

- `Scene` jest **niezależna od backendu**: prostokąty, zaokrąglenia, obwódki,
  clip (także zaokrąglony), linie, okręgi, wielokąty, tekst, obrazy, tło.
- `Scene` nie zna pikseli fizycznych — `scale` przelicza współrzędne logiczne.
- Każda klatka jest budowana od zera (`Scene::clear()`). Platforma porównuje gotowy
  bufor z poprzednią klatką (`damage::diff_bounds`) i:
  - pomija `wl_surface.commit`, gdy klatka jest identyczna,
  - zawęża `wl_surface.damage_buffer` do najmniejszego zmienionego prostokąta.
- Dodatkowo `Scene::same_as` porównuje **listę komend** z poprzednio narysowaną sceną
  (obrazy po `Arc`, nie po zawartości). Jeśli komendy są identyczne i nie zmienił się
  rozmiar/skala/tło, platforma pomija samo `renderer.draw`.
- To **damage lite + scene fingerprint** — nadal nie zastępuje selektywnego rysowania
  sceny (pełny damage tracking raportowany przez widgety jest osobnym etapem).

## 2. Benchmark

```sh
# obowiązkowo release — na debug liczby nie mają sensu
cargo run --release --manifest-path libluna/Cargo.toml \
    -p libluna-render --example render_bench

# wynik do CI / porównań
cargo run --release --manifest-path libluna/Cargo.toml \
    -p libluna-render --example render_bench -- --json

# jedna rozdzielczość i więcej klatek
cargo run --release --manifest-path libluna/Cargo.toml \
    -p libluna-render --example render_bench -- --width 1920 --height 1080 --iters 60
```

Sceny syntetyczne:

| Scena | Co zawiera |
|---|---|
| `rects` | 900 wypełnionych prostokątów |
| `rounded` | 700 kart (wypełnienie + obwódka) |
| `text` | 400 napisów (11–19 px) |
| `clip` | 48 przyciętych komórek po 8 kształtów |
| `icons` | 250 ikon wektorowych: okrąg + łamana + wielokąt (cache ścieżek) |
| `mixed` | 180 kart: tło, obrazek, nagłówek, 4 wiersze w clipie |

To **górna granica** gęstości, nie typowy ekran. Terminal 900×600 w praktyce
rysuje ~0,5–2 ms/klatkę, panel powłoki ~1–3 ms.

### Baseline (maszyna deweloperska, x86_64, release)

`--iters 30`, backend CPU:

| Rozdzielczość | Scena | Komendy | Śr. ms | FPS | Mpx/s |
|---|---:|---:|---:|---:|---:|
| 1280×800 | rects | 900 | 6,80 | 147 | 151 |
| 1280×800 | rounded | 1400 | 28,82 | 35 | 36 |
| 1280×800 | text | 400 | 13,50 | 74 | 76 |
| 1280×800 | clip | 480 | 6,76 | 148 | 152 |
| 1280×800 | icons | 750 | 8,72 | 115 | 117 |
| 1280×800 | mixed | 1800 | 47,10 | 21 | 22 |
| 1920×1080 | rects | 900 | 7,49 | 134 | 277 |
| 1920×1080 | rounded | 1400 | 30,65 | 33 | 68 |
| 1920×1080 | text | 400 | 13,98 | 72 | 148 |
| 1920×1080 | clip | 480 | 8,07 | 124 | 257 |
| 1920×1080 | icons | 750 | 8,73 | 115 | 238 |
| 1920×1080 | mixed | 1800 | 49,86 | 20 | 42 |
| 2560×1440 | rects | 900 | 8,55 | 117 | 431 |
| 2560×1440 | rounded | 1400 | 32,91 | 30 | 112 |
| 2560×1440 | text | 400 | 14,89 | 67 | 248 |
| 2560×1440 | clip | 480 | 11,19 | 89 | 330 |
| 2560×1440 | icons | 750 | 9,98 | 100 | 369 |
| 2560×1440 | mixed | 1800 | 57,87 | 17 | 64 |

Dodatkowo: PNG 1920×1080 ≈ **13,6 ms**, 2,2 MiB; `VmHWM` procesu benchmarku
≈ **52 MiB** (fonty, cache glifów, bufor 1920×1080).

Wnioski:

- **`FillRect` przez `Pixmap::fill_rect`** (bez budowania `Path`) dało `rects`
  10,4 → 7,5 ms przy 1080p (~28%). To najtańszy zysk i został włączony na stałe.
- **Cache ścieżek** (`PathCache`, 2048 wpisów, 0,25 px) pomaga powtarzalnym
  kształtom (ikony, stałe karty), ale w scenie `rounded` z unikalną geometrią
  prawie nic nie zmienił: **rasteryzacja dominuje nad budowaniem ścieżki**.
- Wąskim gardłem pozostają **zaokrąglenia/obwódki i clip** — nie wypełnianie tła.
- Samo wypełnienie 2560×1440 jest tanie; koszt rośnie z liczbą ścieżek i glifów.
- Bufor klatki to 4,1 MiB (1280×800), 8,3 MiB (1920×1080), 14,7 MiB (2560×1440).
  `wl_shm` z double bufferingiem może trzymać dwa takie bufory.
- **Damage lite** dokłada jedną kopię poprzedniej klatki na proces (tyle samo co
  bufor), żeby porównywać piksele. To świadomy koszt pamięci za mniejszy ruch do
  kompozytora i brak pustych commitów.

## 3. Kiedy GPU ma sens

Nie „bo tak”, tylko po pomiarach na **najsłabszym wspieranym sprzęcie**:

1. **Budżet klatki**: 16,7 ms przy 60 Hz. Cel: p95 `draw` ≤ 8 ms (reszta na
   kompozycję, `wl_shm`/copy, wejście).
2. **Najpierw CPU**: jeśli typowe ekrany (panel + lista + terminal) przekraczają
   budżet, taniej jest dodać **damage tracking** i cache niż backend GPU.
3. **Dopiero potem GPU**, za istniejącym `Renderer`:
   - EGL + `zwp_linux_dmabuf_v1` (zero-copy buforów klientów, wymóg dla Zed/Vulkan
     i wideo sprzętowego — patrz `docs/compositor.md`),
   - fallback CPU, gdy EGL/dmabuf niedostępne,
   - ten sam `Scene`, bez zmian w widgetach.
4. **Kryterium akceptacji**: te same sceny w benchmarku, p95 ≤ 8 ms przy
   2560×1440 na sprzęcie docelowym, bez regresji pamięci > 20%.

Anti-cel: GPU nie naprawi źle zbudowanej sceny (np. 1800 komend na klatkę).
Dlatego kolejność to **damage → mniej komend → ewentualnie GPU**.

## 4. Następne kroki (renderer)

- [x] **Damage lite**: `libluna_render::damage::diff_bounds` porównuje klatki,
      pomija identyczne `commit` i zawęża `damage_buffer` do zmienionego prostokąta.
- [x] **Scene fingerprint**: `Scene::same_as` + `last_scene` w platformie — gdy komendy
      są identyczne, `renderer.draw` jest pomijany (obrazy po `Arc`).
- [x] **Cache ścieżek** (`PathCache`): zaokrąglenia, okręgi, łamane i wielokąty;
      2048 wpisów FIFO, kwantyzacja 0,25 px. Plus szybka ścieżka `Pixmap::fill_rect`
      dla zwykłych prostokątów.
- [ ] **Cache rastrowy kształtów** (np. 9-slice dla kart/przycisków) — to dopiero
      zaatakuje dominujący koszt rasteryzacji zaokrągleń.
- [x] **Sonda GPU** (`examples/gpu_probe`): EGL surfaceless + GLES2, pomiar
      kompozycji vs CPU i wykrywanie `EGL_EXT_image_dma_buf_import`. Wyniki i plan:
      [`docs/gpu.md`](gpu.md).
- [x] **`GpuRenderer`** (`libluna_render::gpu`) za tym samym `Renderer`:
      `LUNA_GPU=1` (aplikacje) / `SELENE_GPU=1` (kompozytor nested), z automatycznym
      fallbackiem CPU. Na scenie pulpitu 12,63 → **1,56 ms** (z readbackiem).
      Tekst i wektory idą przez małe warstwy CPU, więc wygląd się nie zmienia.
- [x] **`zwp_linux_dmabuf_v1`** (etap 2) — import buforów GPU klientów
      (`EGLImage` + modyfikatory), cache per `wl_buffer`, release przy następnym commicie.
      Round-trip testowany w `libluna-render` (eksport MESA → import → piksele).
- [ ] Test z realnym klientem GPU (Zed) i formaty wieloplanowe (wideo).
- [ ] Kompozycja GPU w trybie DRM (kontekst per wątek wyjścia) i zero-copy na Mesa.
- [ ] Pełny damage tracking: `Scene`/widgety raportują prostokąty uszkodzeń, renderer
      nie rysuje całej sceny (dziś nadal budujemy ją od zera).
- [ ] Cache glifów i ścieżek między klatkami (dziś `tiny-skia` buduje ścieżki od nowa).
- [ ] Benchmark w CI z progiem (`--json` + porównanie do baseline).
- [ ] Porównanie z GTK4 na tych samych scenach (Definition of Done z roadmapy).

## 5. Diagnostyka w działającej sesji

- `LUNA_STATS=1` — platforma wypisuje klatki/s, ile z nich pominięto (`damage lite`)
  oraz czas rysowania i wysyłki. Po scene fingerprint na bezczynnym ekranie Ustawień
  rysowanie spada z ~3,8 ms do ~0,2–0,6 ms na próbę klatki.
- `SELENE_STATS=1` — kompozytor: FPS wyjścia i tempo kursora.
- `scripts/session-smoke.sh` — poprawność przepływu (nie wydajność).
- Zrzuty: `Print` / `Super+F12` w Selene; podglądy headless renderują do PNG.
