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
| `clear` 1920×1080 | 0,03 ms |
| upload tekstury 1920×1080 RGBA | 0,98 ms |
| kompozycja 30 okien 640×480 (blending) | **0,80 ms** |
| readback 1920×1080 do RGBA | **1,19 ms** |
| **razem: kompozycja + readback** | **1,98 ms** |
| ta sama scena na CPU (`SoftwareRenderer`) | **49,8 ms** |

Wniosek: dla sceny „pulpit z 30 oknami” GPU jest ~60× szybsze od CPU, a nawet
ścieżka z readbackiem (wymagana, gdy nie ma zero-copy) mieści się w budżecie
**2 ms** z 16,7 ms klatki. Readback jest jedynym realnym kosztem wyjścia.

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
                        ┌──────────────────────────────────────────┐
 klienci Wayland ──┐    │ Selene                                   │
   wl_shm          ├──► │  GpuCompositor (EGL/GLES)                │
   dmabuf (GPU) ───┘    │   ├─ import: shm → tex, dmabuf → EGLImage│
                        │   ├─ sceny: obrazy + SSD + kursor         │
                        │   └─ wyjście: GBM/dmabuf | readback → shm│
                        └──────────────────────────────────────────┘
 aplikacje libLuna ──► libluna-render::Renderer
                          ├─ SoftwareRenderer (dziś, domyślny)
                          └─ GpuRenderer (etap 3)
```

Kluczowa zasada z `docs/api-stability.md`: widgety i aplikacje **nie wiedzą**
o EGL. Backend wchodzi wyłącznie za `Renderer`, a Selene wybiera kompozytor
GPU/CPU na podstawie dostępności EGL.

## 3. Etapy (każdy kończy się pomiarem i fallbackiem)

### ETAP 1 — `GpuCompositor` w Selene (największy zysk, najmniejsze ryzyko)

Zakres:
1. `selene/src/gpu.rs`: loader EGL/GLES (ten sam co w sondzie), kontekst
   surfaceless, shader „tekstura + alpha”, VBO z quadem, FBO na wyjście.
2. Import `wl_shm` jako tekstur (`glTexSubImage2D`), cache per `wl_buffer`
   (usuwać przy `release`).
3. Kompozycja sceny Selene: tło, okna (z `image_offset`), SSD, kursor, overview.
   Popupy i warstwy: te same obrazy w kolejności `z`.
4. Wyjście: `SELENE_GPU=1` → FBO + readback do dotychczasowego bufora
   (`ImageData`), więc **reszta kodu zostaje bez zmian**. Brak EGL = automatyczny
   powrót do CPU (jedna linia w logu).
5. Kryterium: 30 okien 1080p, p95 klatki ≤ 8 ms; brak różnic pikseli poza
   antyaliasingiem.

Dlaczego najpierw to: nie dotyka protokołu, nie wymaga dmabuf, a od razu zdejmuje
z CPU ~50 ms/klatkę w gęstej sesji.

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

### ETAP 3 — `GpuRenderer` dla libLuna (ścieżki, tekst, clip)

`Scene` ma ścieżki i glify, więc sam „quad z teksturą” nie wystarczy. Kolejność:

1. **Warstwa obrazów i prostokątów** (okna, tła, separatory) — shader SDF dla
   zaokrągleń i obwódek (bez teselacji).
2. **Cache rastrowy kształtów** (ten sam, o którym mówi `docs/rendering.md`):
   zaokrąglona karta/ikona rasteryzowana raz do atlasu, potem rysowana jako quad.
   To łączy zalety CPU (wierność tiny-skia) i GPU (tanie składanie).
3. **Tekst**: atlas glifów (już mamy cache w `text.rs`) → instancjonowane quady.
4. **Clip**: `glScissor` dla prostokątów, stencil dla zaokrągleń.
5. Kryterium: te same scenariusze co `render_bench`, p95 ≤ 8 ms przy 2560×1440,
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
2. [ ] `GpuCompositor` (etap 1) za `SELENE_GPU=1`, z fallbackiem i pomiarem.
3. [ ] `zwp_linux_dmabuf_v1` (etap 2) + test z klientem GPU w izolowanej sesji.
4. [ ] `GpuRenderer` dla `Scene` (etap 3), najpierw obrazy/prostokąty/zaokrąglenia.
5. [ ] Zero-copy DRM (etap 4) na Mesa; NVIDIA — readback.
