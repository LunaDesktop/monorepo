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
│  SoftwareRenderer     │  ← dziś: tiny-skia + fontdue, bufor RGBA8888
│  (backend CPU)        │     gotowy do wl_shm
└───────────────────────┘
        │  data() → wl_shm
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
zmian w widgetach ani aplikacjach. Backend GPU musi spełnić ten sam kontrakt;
szczegóły EGL/dmabuf/synchronizacji zostają w jego implementacji.

### Co robi `Scene`, a czego nie robi

- `Scene` jest **niezależna od backendu**: prostokąty, zaokrąglenia, obwódki,
  clip (także zaokrąglony), linie, okręgi, wielokąty, tekst, obrazy, tło.
- `Scene` nie zna pikseli fizycznych — `scale` przelicza współrzędne logiczne.
- Każda klatka jest budowana od zera (`Scene::clear()`). Platforma porównuje gotowy
  bufor z poprzednią klatką (`damage::diff_bounds`) i:
  - pomija `wl_surface.commit`, gdy klatka jest identyczna,
  - zawęża `wl_surface.damage_buffer` do najmniejszego zmienionego prostokąta.
  To **damage lite** — nie zastępuje jeszcze selektywnego rysowania sceny
  (pełny damage tracking raportowany przez widgety jest osobnym etapem).

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
| `mixed` | 180 kart: tło, obrazek, nagłówek, 4 wiersze w clipie |

To **górna granica** gęstości, nie typowy ekran. Terminal 900×600 w praktyce
rysuje ~0,5–2 ms/klatkę, panel powłoki ~1–3 ms.

### Baseline (maszyna deweloperska, x86_64, release)

`--iters 30`, backend CPU:

| Rozdzielczość | Scena | Komendy | Śr. ms | FPS | Mpx/s |
|---|---:|---:|---:|---:|---:|
| 1280×800 | rects | 900 | 10,27 | 97 | 100 |
| 1280×800 | rounded | 1400 | 31,53 | 32 | 33 |
| 1280×800 | text | 400 | 13,86 | 72 | 74 |
| 1280×800 | clip | 480 | 6,81 | 147 | 150 |
| 1280×800 | mixed | 1800 | 42,98 | 23 | 24 |
| 1920×1080 | rects | 900 | 10,40 | 96 | 199 |
| 1920×1080 | rounded | 1400 | 30,05 | 33 | 69 |
| 1920×1080 | text | 400 | 13,89 | 72 | 149 |
| 1920×1080 | clip | 480 | 7,81 | 128 | 266 |
| 1920×1080 | mixed | 1800 | 47,25 | 21 | 44 |
| 2560×1440 | rects | 900 | 10,44 | 96 | 353 |
| 2560×1440 | rounded | 1400 | 31,02 | 32 | 119 |
| 2560×1440 | text | 400 | 14,22 | 70 | 259 |
| 2560×1440 | clip | 480 | 10,27 | 97 | 359 |
| 2560×1440 | mixed | 1800 | 54,59 | 18 | 68 |

Dodatkowo: PNG 1920×1080 ≈ **12 ms**, 2,2 MiB; `VmHWM` procesu benchmarku
≈ **54 MiB** (fonty, cache glifów, bufor 1920×1080).

Wnioski:

- Wąskim gardłem są **kształty z zaokrągleniami/obwódkami** (tiny-skia rasteryzuje
  ścieżki na CPU) oraz **clip** — nie samo wypełnianie tła.
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
- [ ] Pełny damage tracking: `Scene`/widgety raportują prostokąty uszkodzeń, renderer
      nie rysuje całej sceny (dziś nadal budujemy ją od zera).
- [ ] Cache glifów i ścieżek między klatkami (dziś `tiny-skia` buduje ścieżki od nowa).
- [ ] Benchmark w CI z progiem (`--json` + porównanie do baseline).
- [ ] Backend GPU za `Renderer` (EGL/wgpu) dopiero po powyższych.
- [ ] Porównanie z GTK4 na tych samych scenach (Definition of Done z roadmapy).

## 5. Diagnostyka w działającej sesji

- `LUNA_STATS=1` — platforma wypisuje klatki/s, ile z nich pominięto (`damage lite`)
  oraz czas rysowania i wysyłki.
- `SELENE_STATS=1` — kompozytor: FPS wyjścia i tempo kursora.
- `scripts/session-smoke.sh` — poprawność przepływu (nie wydajność).
- Zrzuty: `Print` / `Super+F12` w Selene; podglądy headless renderują do PNG.
