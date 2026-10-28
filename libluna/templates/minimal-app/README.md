# Szablon aplikacji środowiska Luna (libLuna)

Minimalny start: jedno okno Wayland, motyw jasny/ciemny i gotowy `prelude`.

## Szybki start (scaffolder)

Z katalogu głównego libLuny:

```bash
./tools/libluna-new.sh ~/projekty/moja-apka
cd ~/projekty/moja-apka
cargo run
```

Skrypt utworzy katalog, wygeneruje `Cargo.toml` z poprawną ścieżką do libLuny
i skopiuje `src/main.rs`.

## Ręcznie

1. Skopiuj ten katalog jako bazę swojej aplikacji.
2. W `Cargo.toml` podmień `__LIBLUNA_PATH__` na ścieżkę do `crates/libluna`
   (albo użyj zależności `git`/rejestrowej):

   ```toml
   [dependencies]
   libluna = { path = "../libluna/crates/libluna" }
   # lub:
   # libluna = { git = "https://github.com/mijsys/libLuna.git" }
   ```
3. Podmień `__APP_NAME__` na nazwę pakietu.

## Struktura

```rust
use libluna::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::new("Moja apka").initial_size(800, 600);
    run(config, |frame: &mut Frame| {
        // frame.events  — zdarzenia wejścia
        // frame.scene   — graf sceny do wypełnienia
        // frame.theme   — rozwiązany motyw (jasny/ciemny + akcent)
        // frame.text    — pomiar tekstu (frame.measure_text)
    })
}
```
