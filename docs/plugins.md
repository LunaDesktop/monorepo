# Wtyczki Luna Editor — dokumentacja dla twórców

Edytor **nie zawiera wtyczek na stałe**. Wtyczka to osobny program instalowany
z zewnątrz (np. z GitHuba) do katalogu użytkownika. Dzięki temu można ją napisać
w dowolnym języku i rozwijać bez przebudowywania Edytora.

Pierwsza oficjalna wtyczka to **AI Writer** — agent AI do pisania, redakcji,
tłumaczenia i streszczania tekstu (Gemini, OpenAI, Claude, DeepSeek, OpenRouter,
Mistral, Groq, Ollama). Repozytorium:
<https://github.com/LunaDesktop/luna-editor-ai-writer>.

![Menedżer wtyczek w Luna Editor](images/luna-editor-plugins.png)

## Instalacja

W Edytorze otwórz **menu ☰ → Plugins…** i wklej adres:

```
https://github.com/<owner>/<repo>
```

Instalator:

1. pobiera archiwum `main` (a jeśli nie istnieje — `master`) przez `curl` lub
   `wget`, albo bezpośredni URL archiwum `.tar.gz`,
2. rozpakowuje je `tar`-em,
3. szuka pliku `plugin.conf` (do 3 poziomów katalogów),
4. kopiuje katalog wtyczki do
   `$XDG_DATA_HOME/luna/editor/plugins/<id>/`
   (domyślnie `~/.local/share/luna/editor/plugins/<id>/`).

Wymagane narzędzia systemowe: `curl` (lub `wget`) oraz `tar`.

Wtyczkę usuwa się przyciskiem **Remove** w menedżerze. Ustawienia (w tym klucze
API) są przechowywane osobno w
`$XDG_CONFIG_HOME/luna/editor/plugins/<id>.conf` z prawami `0600`.

Po instalacji polecenia wtyczki są dostępne także **w menu głównym Edytora**
(sekcja z nazwą wtyczki), obok menedżera `Plugins…`.

## Manifest `plugin.conf`

Plik musi leżeć w katalogu głównym repozytorium (albo w podkatalogu — instalator
znajdzie go do 3 poziomów). Format jest liniowy: `klucz = wartość`, komentarze
zaczynają się od `#`.

```ini
id = example
name = Example plugin
version = 0.1.0
author = Twoje imię
description = Krótki opis widoczny w menedżerze wtyczek.
entry = plugin.py
interpreter = python3

command = hello|Przywitaj się||selection|insert
command = write|Napisz tekst…|prompt|document|insert

setting = tone|Ton wypowiedzi|select|neutralny,formalny,swobodny|neutralny
setting = api_key|Klucz API|secret||
```

### Klucze manifestu

| Klucz | Wymagany | Opis |
|---|---|---|
| `id` | tak | Unikalny identyfikator: litery, cyfry, `.`, `_`, `-` |
| `name` | nie | Nazwa wyświetlana (domyślnie `id`) |
| `version` | nie | Wersja widoczna w menedżerze |
| `author` | nie | Autor |
| `description` | nie | Opis w menedżerze |
| `entry` | tak | Program względem katalogu wtyczki |
| `interpreter` | nie | Np. `python3`, `node`, `sh`. Bez niego plik musi być wykonywalny |
| `command` | tak (≥1) | Polecenie widoczne w UI |
| `setting` | nie | Pole konfiguracji |

### `command = id|tytuł|prompt|kontekst|wyjście`

| Pole | Wartości | Znaczenie |
|---|---|---|
| `id` | tekst | Trafia do `LUNA_PLUGIN_ACTION` |
| `tytuł` | tekst | Etykieta przycisku (można użyć polskich znaków) |
| `prompt` | `prompt` lub puste | `prompt` = najpierw zapytaj o instrukcję |
| `kontekst` | `document`, `selection`, `none` | Co trafi na standardowe wejście |
| `wyjście` | `insert`, `replace_selection`, `replace_document`, `append` | Co zrobić z wynikiem |

- `insert` — wstaw w miejscu kursora (zastępuje zaznaczenie),
- `replace_selection` — zamień zaznaczenie; bez zaznaczenia wstaw w kursorze,
- `replace_document` — zamień całą treść dokumentu,
- `append` — dopisz na końcu dokumentu.

### `setting = klucz|etykieta|typ|opcje|domyślna`

| Typ | Opis |
|---|---|
| `text` | Zwykłe pole tekstowe |
| `secret` | Pole maskowane (np. klucz API) |
| `select` | Lista wyboru; opcje oddzielone przecinkami |
| `models` | Lista pobierana z wtyczki po kliknięciu (np. modele LLM); pokazywana w popupie z wyszukiwaniem |

Dla typu `models` wtyczka dostaje akcję `__list_models__` i powinna wypisać
identyfikatory modeli **jeden na linię** na standardowe wyjście. Edytor pokazuje
je w popupie z filtrowaniem i przewijaniem; wybrany model trafia do zapisu
ustawień.

Wartości trafiają do procesu jako zmienne `LUNA_SETTING_<KLUCZ>` (wielkie litery,
znaki inne niż alfanumeryczne zamieniane na `_`). Przykład: `api_key` →
`LUNA_SETTING_API_KEY`.

## Protokół uruchomienia

Edytor uruchamia program w katalogu wtyczki i przekazuje:

| Zmienna | Znaczenie |
|---|---|
| `LUNA_PLUGIN_ID` | `id` wtyczki |
| `LUNA_PLUGIN_ACTION` | `id` wybranego polecenia |
| `LUNA_PLUGIN_INSTRUCTION` | Tekst wpisany w oknie polecenia (może być pusty) |
| `LUNA_PLUGIN_PATH` | Ścieżka dokumentu (może być pusta dla nowego pliku) |
| `LUNA_PLUGIN_LANGUAGE` | Język dokumentu, np. `Rust`, `Markdown`, `Plain` |
| `LUNA_SETTING_*` | Wartości ustawień z manifestu i konfiguracji |

Dodatkowo, gdy użytkownik kliknie pole typu `models`, edytor uruchamia wtyczkę
z `LUNA_PLUGIN_ACTION=__list_models__` (bez danych na stdin) i oczekuje listy
modeli — po jednym na linię. To akcja zarezerwowana przez system wtyczek.

**Standardowe wejście** zawiera kontekst wybrany w manifeście (cały dokument,
zaznaczenie albo nic). Wtyczka powinna najpierw przeczytać stdin.

**Standardowe wyjście** to wynik — czysty tekst do wstawienia. Edytor ignoruje
końcowe znaki nowej linii przy `insert`/`replace_selection`.

**Standardowe wyjście błędów** i kod wyjścia ≠ 0 oznaczają błąd; jego treść
zostanie pokazana użytkownikowi w oknie alertu.

Wtyczki działają z uprawnieniami użytkownika. Nie ma piaskownicy — instaluj
tylko zaufane repozytoria.

## Minimalny przykład (Python)

`plugin.conf`:

```ini
id = hello
name = Hello
version = 0.1.0
entry = hello.py
interpreter = python3
command = hello|Przywitaj się||document|replace_document
```

`hello.py`:

```python
import os
import sys

context = sys.stdin.read()
action = os.environ.get("LUNA_PLUGIN_ACTION", "")
print(f"Witaj! Akcja: {action}, znaków w dokumencie: {len(context)}")
```

## Minimalny przykład (Shell)

```ini
id = upper
name = Wielkie litery
version = 0.1.0
entry = upper.sh
command = upper|Zamień na wielkie litery||selection|replace_selection
```

```sh
#!/bin/sh
tr '[:lower:]' '[:upper:]'
```

Plik `upper.sh` musi być wykonywalny (`chmod +x`). Zamiast tego można ustawić
`interpreter = sh`.

## Publikacja wtyczki na GitHubie

1. Utwórz repozytorium z `plugin.conf` w katalogu głównym.
2. Dodaj program wejściowy i (opcjonalnie) README.
3. Użytkownik instaluje przez URL repozytorium. Domyślnie używane są gałęzie
   `main` i `master`; nie są potrzebne żadne wydania ani tagi.
4. Aktualizacja = ponowna instalacja z tego samego URL (katalog jest
   zastępowany).

## AI Writer — pierwsza wtyczka

Instalacja:

```
https://github.com/LunaDesktop/luna-editor-ai-writer
```

Po instalacji: **Plugins… → AI Writer → Settings** — wybierz dostawcę, model
i wpisz klucz API. Klucz jest zapisywany lokalnie (`0600`) i wysyłany tylko do
wybranego dostawcy.

Obsługiwani dostawcy: Gemini, ChatGPT (OpenAI), Claude (Anthropic), DeepSeek,
OpenRouter, Mistral, Groq oraz lokalny Ollama (bez klucza).

Po wpisaniu klucza API pole **Model** pobiera listę modeli bezpośrednio od
dostawcy i otwiera popup z wyszukiwaniem. Polecenie **Zapytaj AI…** pozwala
wpisać dowolne pytanie lub prompt i wstawić odpowiedź do dokumentu.

Polecenia: **Zapytaj AI…**, **Napisz tekst…**, **Kontynuuj pisanie**,
**Popraw zaznaczenie**, **Przetłumacz zaznaczenie…**, **Streść zaznaczenie**.

## Rozwiązywanie problemów

| Objaw | Przyczyna / rozwiązanie |
|---|---|
| „W archiwum nie znaleziono plugin.conf” | Manifest musi być w repozytorium (do 3 poziomów katalogów) |
| „Nie udało się pobrać wtyczki” | Brak `curl`/`wget`, zły URL albo prywatne repo |
| „Nie udało się uruchomić wtyczki” | Zły `interpreter`, brak `python3` lub plik niewykonywalny |
| „Brak klucza API” (AI Writer) | Uzupełnij ustawienia wtyczki |
| Wtyczka nic nie zwraca | Program musi wypisać tekst na standardowe wyjście |

Zobacz też: [dokumentacja Edytora](editor.md), [system ikon](icons.md).
