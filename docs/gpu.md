 # GPU w Lunie — pomiary, architektura i etapy

Ten dokument jest **planem wejścia GPU do rdzenia** (libLuna + Selene), opartym na
realnym pomiarze, a nie na założeniach. Punkt wyjścia: renderer CPU
(`SoftwareRenderer`, `docs/rendering.md`) i kompozytor, który składa okna obrazkami
na CPU.

## 1. Co zmierzyliśmy

Sonda `libluna-render/examples/gpu_probe` (bez nowych zależności: `dlopen` + EGL/GLES2):

```sh
cargo run --release --manifest-path libluna/Cargo.toml \
    -p libluna-render --example gpu_probe

# JSON do CI
cargo run --release ... --example gpu_probe -- --json
```

Środowisko pomiaru (dev): NVIDIA GTX 1660 SUPER, sterownik 610.57.04, EGL 1.5,
GLES 3.2, **EGL surfaceless** (bez X11/Waylanda), 1920×1080:

| Operacja | Czas |
|---|---:|
| `clear` + kompozycja 30 okien (GPU) | **1,37 ms** |
| ta sama scena na CPU (`SoftwareRenderer`) | **49,3 ms** |
| pulpit: tło + 6 okien + SSD + kursor, GPU | **1,56 ms** |
| ten sam pulpit, CPU | **12,63 ms** |

Wniosek: przy 30 oknach GPU jest ~36× szybsze, a realistyczny pulpit z tekstem
(paski SSD) ~8× — mimo że tekst jest rasteryzowany na CPU w małych warstwach.
Czas GPU zawiera już `glReadPixels` (wyjście bez zero-copy).

### Dostępne rozszerzenia (ta maszyna)

| Rozszerzenie | Wynik | Znaczenie |
|---|---|---|
| `EGL_KHR_surfaceless_context` | tak | kontekst bez okna — działa headless i w DRM |
| `EGL_EXT_image_dma_buf_import` | **tak** | import buforów GPU klientów (`zwp_linux_dmabuf_v1`) |
| `EGL_EXT_image_dma_buf_import_modifiers` | **tak** | formaty z modyfikatorami (tiling) |
| `EGL_KHR_fence_sync` | tak | synchronizacja GPU bez zgadywania |
| `EGL_ANDROID_native_fence_sync` | tak | fence fd zamiast `glFinish` |
| `EGL_KHR_swap_buffers_with_damage` | tak | przyszłe zero-copy wyjście |
| `EGL_PLATFORM_GBM` (`gbm_create_device`) | **nie** | brak `nvidia-drm` backendu GBM na tej maszynie |

Konsekwencja: **import dmabuf jest wykonalny już teraz**, ale zero-copy wyjście do
DRM na NVIDIA proprietary wymaga pakietu `libnvidia-egl-gbm`/nowszego sterownika.
Na Mesa (Intel/AMD) `EGL_PLATFORM_GBM` jest standardem — tam zero-copy będzie
domyślne. Projekt musi mieć **oba warianty**: dmabuf→FBO→page-flip (Mesa) oraz
FBO→glReadPixels→shm/dumb (fallback, dziś NVIDIA).

## 2. Architektura docelowa

```text
klienci Wayland (wl_shm; dmabuf — etap 2)
        │
        ▼
   Selene: model okien → Scene (obrazy, SSD, kursor, overview)
        │  Renderer::draw(scene, scale, background)
        ▼
   libluna-render
        ├─ SoftwareRenderer (CPU, domyślnie)
        └─ GpuRenderer (EGL/GLES; LUNA_GPU=1 / SELENE_GPU=1)
              ├─ GPU: obrazy, prostokąty, zaokrąglenia, obwódki, okręgi, clip
              ├─ CPU: tekst i wektory → małe warstwy wstawiane w kolejności z
              └─ wyjście: glReadPixels → wl_shm (nested) / dumb buffer (DRM)
```

Widgety i aplikacje **nie wiedzą** o EGL. Backend wchodzi wyłącznie za `Renderer`,
a Selene/aplikacja wybiera GPU tylko wtedy, gdy EGL działa — inaczej zostaje CPU.
Zero-copy (`dmabuf` → tekstura → page-flip) dochodzi w etapach 2 i 4.

## 3. Etapy (każdy kończy się pomiarem i fallbackiem)

### ETAP 1 — `GpuCompositor` w Selene (największy zysk, najmniejsze ryzyko)

**Status: ✅ zaimplementowany jako `libluna_render::gpu::GpuRenderer`.**

Zakres (zrobione):
1. `gpu.rs`: loader EGL/GLES (`dlopen`/`eglGetProcAddress`), kontekst surfaceless,
   shadery (tekstura z clipem; kształty SDF: prostokąt, zaokrąglenie, obwódka, okrąg).
2. Import `wl_shm` jako tekstur z cache per `Arc<ImageData>` (eviction po 120 klatkach).
3. Kolejność `z` zachowana: tekst, łamane i wielokąty (ikony, kursor, tytuły SSD)
   idą do małych **warstw CPU** rasteryzowanych przez `SoftwareRenderer` i rysowanych
   jako quad dokładnie w tym miejscu sceny — bez zmiany wyglądu.
4. Wyjście: `glReadPixels` do bufora `Renderer::data()` (fallback bez GBM).
5. Włączenie: `LUNA_GPU=1` (każda aplikacja libLuny) lub `SELENE_GPU=1` (kompozytor
   zagnieżdżony). Brak EGL = log + CPU, sesja żyje.
6. Kryterium: renderer desktop 12,63 → **1,56 ms**; `luna-panel` z `LUNA_GPU=1`
   daje obrazy zgodne z CPU (różnice tylko antyaliasing zaokrągleń).

Pozostało w tym etapie: przenieść kompozycję DRM na GPU (dziś ścieżka DRM ma
osobny `SoftwareRenderer` i wątki per monitor — potrzebny jeden kontekst per wątek)
i dodać pomiar w realnej sesji na słabym sprzęcie.

### ETAP 2 — `zwp_linux_dmabuf_v1` (klienci GPU)

Zakres:
1. Global `zwp_linux_dmabuf_v1` v3 + `zwp_linux_buffer_params_v1`
   (`create`/`create_immed`, `add`, `created`/`failed`).
2. Dla każdego `wl_buffer`: `eglCreateImage` z `EGL_LINUX_DMA_BUF_EXT`
   (+ modyfikatory), `glEGLImageTargetTexture2DOES` → tekstura.
3. `wl_buffer.release` dopiero po fence (`EGL_KHR_fence_sync`) — bez tego klient
   rysuje po zwolnionym buforze.
4. Import tylko formatów, które faktycznie obsługujemy; inaczej `failed`
   (klient wybierze shm). Lista formatów/modyfikatorów z
   `eglQueryDmaBufFormatsEXT`/`eglQueryDmaBufModifiersEXT`.
5. Kryterium: Zed/Chrome/Electron startują **natywnie na Waylandzie**
   (bez `LUNA_X11_APPS`), wideo sprzętowe nie kopiuje klatek przez CPU.

### ETAP 3 — pełny GPU dla `Scene` (ścieżki, tekst, clip)

**Status: ✅ podstawy działają** (obrazy, prostokąty, zaokrąglenia, obwódki,
okręgi, clip prostokątny/zaokrąglony; tekst i wektory przez warstwy CPU).

Pozostało:
1. **Atlas glifów** — dziś tekst jest rasteryzowany przez `TextEngine` do warstwy
   CPU; docelowo atlas + instancjonowane quady (mniej kopii i uploadów).
2. **Ścieżki natywnie** — `StrokeLine`/`FillPolygon` (ikony, kursor) przez
   triangulację/SDF, żeby zniknęły warstwy CPU.
3. **Clip stencil** dla wielu zaokrąglonych clipów jednocześnie (dziś ostatni).
4. Kryterium: te same scenariusze co `render_bench`, p95 ≤ 8 ms przy 2560×1440,
   pamięć ≤ +20%, fallback CPU gdy brak EGL.

### ETAP 4 — wyjście zero-copy (DRM/Mesa)

1. `EGL_PLATFORM_GBM` + `gbm_surface`/`gbm_bo` (Mesa), `EGL_KHR_swap_buffers_with_damage`
   zamiast readbacku.
2. Na NVIDIA bez GBM: pozostaje readback (1,2 ms — akceptowalne) albo
   `libnvidia-egl-gbm`, gdy dostępne.
3. Kryterium: brak kopii klatki przez CPU na Intel/AMD; tear-free przez page-flip.

## 4. Ryzyka i zasady

- **Zawsze fallback CPU.** Brak `libEGL`, brak kontekstu, brak formatu dmabuf →
  log i dotychczasowa ścieżka. Sesja nie może umrzeć z powodu GPU.
- **Nie zgadujemy formatów dmabuf.** Lista z EGL, inaczej `failed`.
- **Nie trzymamy mapowań shm na stałe.** Cache tekstur per `wl_buffer`, usuwany
  przy `release`; inaczej wyciek pamięci przy dużych oknach.
- **Readback to nie porażka.** 1,2 ms na 1080p (NVIDIA) jest tańsze niż
  przekomplikowanie wyjścia; zero-copy dopiero, gdy platforma je oferuje.
- **Pomiary przed i po** tym samym narzędziem (`render_bench` + `gpu_probe`),
  inaczej „przyspieszenie” jest anegdotą.

## 5. Kolejność prac (konkret)

1. [x] Sonda EGL + benchmark (`gpu_probe`), potwierdzenie dmabuf/fence.
2. [x] `GpuRenderer` (etap 1) — `LUNA_GPU=1`/`SELENE_GPU=1`, fallback CPU, warstwy
   CPU dla tekstu/wektorów, pomiar 12,6 → 1,6 ms na scenie pulpitu.
3. [ ] Kompozycja GPU w trybie DRM (kontekst per wątek wyjścia).
4. [ ] `zwp_linux_dmabuf_v1` (etap 2) + test z klientem GPU w izolowanej sesji.
5. [ ] Zero-copy DRM (etap 4) na Mesa; NVIDIA — readback.
