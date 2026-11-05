# Luna Editor (`luna-editor`)

Lekki edytor tekstu i kodu środowiska Luna, zbudowany wyłącznie na libLunie.
Podświetlanie składni rysuje `CodeView` z `libluna-widgets`, a zakładki —
`TabBar` w headerbarze.

## Funkcje

- języki: Rust, TOML, JSON, Markdown, Shell, C, C++, Python, HTML/XML, CSS,
  JavaScript/TypeScript, PHP i YAML (po rozszerzeniu),
- zakładki dla wielu plików, `●` przy niezapisanych zmianach, zamykanie `✕`,
- wbudowany manager plików dla „Otwórz”, „Zapisz jako…” i „Otwórz folder”,
- panel projektu z drzewem katalogów i podglądem pliku (`Ctrl+B`),
- menu kontekstowe drzewa: otwórz, nowy plik/folder, zmień nazwę, usuń,
  odśwież — plus nawigacja strzałkami, `Enter`, `F2` i `Delete`,
- zmiana nazwy i usuwanie aktualizują otwarte zakładki,
- numery linii, bieżąca linia, przewijanie w obu osiach, miękkie tabulatory,
- zaznaczanie, schowek, undo/redo, auto-wcięcie,
- `Ctrl+F` (znajdź), `Ctrl+G` (idź do linii), `Ctrl+O`/`Ctrl+S`/`Ctrl+Shift+S`,
- `Ctrl+W` zamyka kartę, `Ctrl+Tab` przełącza zakładki,
- ustawienia programu (`Ctrl+,`): szerokość tabulatora, auto-wcięcie, pliki
  ukryte, foldery budowania i panel projektu przy starcie — zapis w
  `~/.config/luna/editor.conf`,
- zachowanie LF/CRLF i BOM, atomowy zapis, ochrona przed plikami binarnymi
  i zbyt dużymi,
- dialogi i powiadomienia bez zależności od GTK,
- **wtyczki instalowane z GitHuba** (menu `Plugins…`, `Ctrl+Shift+P`):
  zewnętrzne procesy z manifestem `plugin.conf`, bez wbudowywania kodu
  w Edytor. Pierwsza oficjalna wtyczka to **AI Writer** (Gemini, OpenAI,
  Claude, DeepSeek, OpenRouter, Mistral, Groq, Ollama). Dokumentacja dla
  twórców: [`../../docs/plugins.md`](../../docs/plugins.md).

## Uruchomienie

```sh
cargo run -- plik.rs
cargo run -- --line 42 plik.rs
```

## Zrzuty do dokumentacji

```sh
cargo run --bin editor-render -- --no-workspace ../../docs/images/luna-editor.png
cargo run --bin editor-render -- --no-workspace --dark --lang en ../../docs/images/luna-editor-dark.png
cargo run --bin editor-render -- --folder ../../apps/luna-editor/src --preview syntax.rs ../../docs/images/luna-editor-workspace.png
cargo run --bin editor-render -- --folder ../../apps/luna-editor/src --preview syntax.rs --tree-menu ../../docs/images/luna-editor-tree-menu.png
cargo run --bin editor-render -- --folder ../../apps/luna-editor/src --preview syntax.rs --prompt new-file ../../docs/images/luna-editor-prompt.png
cargo run --bin editor-render -- --no-workspace --browser open ../../docs/images/luna-editor-browser.png
cargo run --bin editor-render -- --no-workspace --about ../../docs/images/luna-editor-about.png
cargo run --bin editor-render -- --no-workspace --settings ../../docs/images/luna-editor-settings.png
cargo run --bin editor-render -- --plugins ../../docs/images/luna-editor-plugins.png
```

## Licencja

GNU GPL v3.0 lub nowsza (`GPL-3.0-or-later`) — pełny tekst w [`LICENSE`](LICENSE).
© 2026 Luna Project.
