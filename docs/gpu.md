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
(paski SSD) ~8× — mimo że **tekst** jest już rysowany glifami z atlasu (bez warstw
CPU), a wektory nadal idą przez małe warstwy CPU. Sam render to ~0,4–0,6 ms; reszta
to `glReadPixels` (wyjście bez zero-copy). Zawężenie odczytu do zmian (damage
320×160) zbija całą klatkę pulpitu z 1,66 do **0,48 ms** — patrz sekcja 4.

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
              ├─ GPU: tekst z atlasu glifów (krycie w alfie, kolor z uniformu)
              ├─ CPU: wektory (ikony, kursor) → małe warstwy wstawiane w kolejności z
              └─ wyjście: glReadPixels → wl_shm (nested) / dumb buffer (DRM)
                        (tylko region damage, gdy wołający go zna)
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
3. Kolejność `z` zachowana: **tekst idzie glifami z atlasu** (jedna tekstura,
krycie w alfie, kolor z uniformu, cache per znak/rozmiar), a wektory (ikony,
kursor) idą do małych **warstw CPU** rasteryzowanych przez `SoftwareRenderer`
i rysowanych jako quad dokładnie w tym miejscu sceny — bez zmiany wyglądu.
4. Wyjście: `glReadPixels` do bufora `Renderer::data()` (fallback bez GBM).
5. Włączenie: `LUNA_GPU=1` (każda aplikacja libLuny) lub `SELENE_GPU=1` (kompozytor
   zagnieżdżony). Brak EGL = log + CPU, sesja żyje.
6. Kryterium: renderer desktop 12,63 → **1,56 ms**; `luna-panel` z `LUNA_GPU=1`
   daje obrazy zgodne z CPU (różnice tylko antyaliasing zaokrągleń).

Pozostało w tym etapie: pomiar w realnej sesji na słabym sprzęcie.

**Ścieżka DRM też używa GPU** (`SELENE_GPU=1`): każdy monitor dostaje własny
kontekst (EGL jest przypięty do wątku), render jest sekwencyjny — przy ~1,5 ms na
klatkę to wciąż szybciej niż równoległy CPU (7–10 ms na monitor). Klatka wraca do
`dumb buffer` przez `glReadPixels`, bo na tej maszynie nie ma `EGL_PLATFORM_GBM`.
Zrzuty ekranu (`Super+F12`) idą z aktywnego backendu, więc z GPU widać także okna
`dmabuf`.

### ETAP 2 — `zwp_linux_dmabuf_v1` (klienci GPU)

**Status: ✅ zaimplementowany (zakres etapu 2).**

Zakres (zrobione):
1. Global `zwp_linux_dmabuf_v1` **v3** + `zwp_linux_buffer_params_v1`
   (`create` → `created`, `create_immed`, `add`, `destroy`). Global powstaje
   **tylko** gdy `LUNA_GPU=1` i EGL potrafi zaimportować dmabuf.
2. Formaty **jednoplanowe** `ARGB/XRGB/ABGR/XBGR8888`, modyfikatory z
   `eglQueryDmaBufFormatsEXT`/`eglQueryDmaBufModifiersEXT` (odfiltrowane do
   tego, co naprawdę importujemy). Inne formaty → `failed`; klient zostaje przy shm.
3. Import: `eglCreateImageKHR(EGL_LINUX_DMA_BUF_EXT)` +
   `glEGLImageTargetTexture2DOES` → tekstura w cache `GpuRenderer`
   (klucz `Arc<DmabufImage>`, zwalniana razem z `EGLImage`).
4. `wl_buffer.release` jest wysyłany **przy następnym commicie** tej powierzchni
   (GPU czyta poprzednią klatkę jeszcze po `commit`); renderer kończy klatkę
   `glFinish`. Jawne fence (`EGL_KHR_fence_sync`) to optymalizacja na później.
5. Weryfikacja: test round-trip w `libluna-render` (tekstura GL → eksport MESA
   do dmabuf → import → identyczne piksele), testy walidacji parametrów
   w `selene/src/dmabuf.rs` oraz `luna-control dmabuf` w teście sesji
   (`enabled=true` tylko z GPU).
6. **Zed natywnie na Waylandzie** (bez `LUNA_X11_APPS`) w zagnieżdżonym Selene
   z `SELENE_GPU=1` — log potwierdza rzeczywisty dmabuf klienta:

   ```text
   selene/dmabuf: okno "tmp" 1536x1095 format=0x34325258 modifier=0x0300000000e08014
   libluna-render: dmabuf 1536x1095 (…, premultiplied) → tekstura 3
   ```

   Format `XR24` z modyfikatorem liniowym NVIDII — czyli kompozycja okna GPU
   działa end-to-end, bez pośredniego shm.

Ograniczenia etapu 2: brak formatów wieloplanowych (YUV/wideo), brak
`y_invert`/`interlaced`, brak `feedback` z v4, powierzchnie warstwowe
(`wlr-layer-shell`) nadal wymagają shm.

Kryterium na kolejny krok: klient GPU (Zed/Chrome) startuje natywnie na
Waylandzie bez `LUNA_X11_APPS` — **spełnione dla Zed** (patrz pkt 6).

### ETAP 3 — pełny GPU dla `Scene` (ścieżki, tekst, clip)

**Status: ✅ podstawy działają** (obrazy, prostokąty, zaokrąglenia, obwódki,
okręgi, clip prostokątny/zaokrąglony, **tekst z atlasu glifów**; wektory przez
warstwy CPU).

Pozostało:
1. **Ścieżki natywnie** — `StrokeLine`/`FillPolygon` (ikony, kursor) przez
   triangulację/SDF, żeby zniknęły ostatnie warstwy CPU.
2. **Clip stencil** dla wielu zaokrąglonych clipów jednocześnie (dziś ostatni).
3. Kryterium: te same scenariusze co `render_bench`, p95 ≤ 8 ms przy 2560×1440,
   pamięć ≤ +20%, fallback CPU gdy brak EGL.

Zrobione: **atlas glifów** — `TextEngine::glyph` daje krycie + metryki, `GpuRenderer`
pakuje glify do jednej tekstury (512², półkowy packer, cache per znak/rozmiar) i
rysuje każdy glif quadem. `advance` liczymy z dokładnego rozmiaru, więc napisy nie
rozjeżdżają się względem `measure`. Pomiar: desktop 1,49 ms, z czego render 0,40 ms
(reszta to readback).

Zrobione: **clip zaokrąglony na GPU**. Uniform `round` pakował `(x, y, szerokość,
promień)`, a shader czytał z tego `(szerokość, wysokość)` i promień — czyli
odrzucał całe okno, bo „wysokość” clipu była równa promieniowi. Objaw: GPU rysował
tapetę i pasek SSD, ale **żadnej treści okna** (także `dmabuf`). Promień ma teraz
własny uniform `round_r`, a rozmiar clipu idzie w `zw`; pilnuje tego test
`gpu_draws_shm_image` oraz `window_content_is_visible_in_render` w Selene.

### ETAP 4 — wyjście zero-copy (DRM/Mesa)

1. `EGL_PLATFORM_GBM` + `gbm_surface`/`gbm_bo` (Mesa), `EGL_KHR_swap_buffers_with_damage`
   zamiast readbacku.
2. Na NVIDIA bez GBM: pozostaje readback (1,09 ms na 1080p — akceptowalne) albo
   `libnvidia-egl-gbm`, gdy dostępne.
3. [x] **Readback tylko z regionu uszkodzenia** (platforma/nested): `Renderer::draw_damage`
   + `Scene::damage_bounds` (konserwatywny prostokąt różnicy scen, tekst mierzony
   silnikiem tekstu). `GpuRenderer` czyta z FBO tylko ten prostokąt, a
   `libluna-platform` kopiuje potem **tylko ten region** do `wl_shm` i aktualizuje
   `last_frame` lokalnie. Pomiar: pulpit 1,66 ms → **0,48 ms** przy damage 320×160
   (readback 1,08 → 0,22 ms). Zmiana rozmiaru, skali albo **tła** (poza sceną)
   wymusza pełną klatkę — inaczej zostałby stary obraz.
4. Do zrobienia: ten sam mechanizm w ścieżce DRM (`present` z prostokątem na
   dumb bufferze). Kryterium: przy animacji panelu odczyt ≤ 10% klatki.
   Brak kopii klatki przez CPU na Intel/AMD (zero-copy) i tear-free przez page-flip.

## 4. Ryzyka i zasady

- **Zawsze fallback CPU.** Brak `libEGL`, brak kontekstu, brak formatu dmabuf →
  log i dotychczasowa ścieżka. Sesja nie może umrzeć z powodu GPU.
- **Nie zgadujemy formatów dmabuf.** Lista z EGL, inaczej `failed`.
- **Nie trzymamy mapowań shm na stałe.** Cache tekstur per `wl_buffer`, usuwany
  przy `release`; inaczej wyciek pamięci przy dużych oknach.
- **Readback to nie porażka.** 1,09 ms na 1080p (NVIDIA) jest tańsze niż
  przekomplikowanie wyjścia; zero-copy dopiero, gdy platforma je oferuje.
- **Pomiary przed i po** tym samym narzędziem (`render_bench` + `gpu_probe`),
  inaczej „przyspieszenie” jest anegdotą.

## 5. Kolejność prac (konkret)

1. [x] Sonda EGL + benchmark (`gpu_probe`), potwierdzenie dmabuf/fence.
2. [x] `GpuRenderer` (etap 1) — `LUNA_GPU=1`/`SELENE_GPU=1`, fallback CPU, pomiar
   12,6 → 1,6 ms na scenie pulpitu.
3. [x] `zwp_linux_dmabuf_v1` v3 (etap 2): formaty jednoplanowe + modyfikatory,
   `create_immed`/`create`, cache `EGLImage`, release przy następnym commicie.
4. [x] Test z realnym klientem GPU (Zed w osobnej instancji) — dmabuf `XR24`
   z modyfikatorem NVIDII zaimportowany i wyświetlony.
5. [x] Kompozycja GPU w trybie DRM (kontekst per monitor, render sekwencyjny,
   readback do dumb buffera, zrzuty z aktywnego backendu).
6. [x] Atlas glifów: tekst bez warstw CPU (render pulpitu 0,40 ms; readback 1,08 ms).
7. [ ] Ścieżki natywnie (ikony, kursor) — ostatnie warstwy CPU.
8. [x] Readback z damage w platformie/nested (pulpit 1,66 → 0,48 ms przy 320×160).
9. [ ] Readback z damage w DRM + zero-copy na Mesa; NVIDIA — readback.
10. [ ] Fence (`EGL_KHR_fence_sync`) zamiast synchronicznego readbacku, formaty
    wieloplanowe (wideo), dmabuf dla `wlr-layer-shell`.
