# Luna Settings — centrum ustawień środowiska i systemu

Cel: **jedna aplikacja** `luna-settings` (odpowiednik GNOME Settings / KDE System Settings),
a nie tylko strona ikon. Ma obejmować zarówno ustawienia **środowiska** (wygląd, powłoka,
powiadomienia, skróty), jak i **systemu** (monitory, dźwięk, sieć, Bluetooth, zasilanie,
język, użytkownicy, sterowniki).

## Stan pierwszej wersji

- Działa `apps/luna-settings` na libLunie (`NavList` z sekcjami, `PreferencesPage`,
  `ActionRow`, `Switch`, `Slider`, `ViewSwitcher`, `Button`, `HeaderBar`).
- Sekcja **Środowisko**: **Wygląd** (tryb, akcent, styl), **Ikony** (styl + zestawy
  zewnętrzne, np. Papirus, z podglądem), **Ekran logowania** (tapety z miniaturami,
  formularz hasła administratora), **Ekran blokady** (per użytkownik),
  **Powiadomienia** (DND + róg ekranu + test), **Język i region** (wyszukiwany wybór
  języka, locale i strefy) oraz **Przestrzenie robocze** (liczba + nazwy na żywo).
- Sekcja **Sprzęt**: **Ekrany**, **Dźwięk**, **Zasilanie**. Sekcja **Informacje**:
  **System**, **O Luna**.
- Wejście: Quick Panel na pasku (ikona koła zębatego), wpis `.desktop`, `scripts/install.sh`.
- Podglądy headless: `cargo run --manifest-path apps/luna-settings/Cargo.toml --bin preview` →
  `docs/images/luna-settings-*.png` (w tym `luna-settings-language-picker.png`).
- Kolejne kroki: Bluetooth, użytkownicy, autostart, prywatność, drukarki,
  backend sterowników; pełne sterowanie monitorami po stronie Selene.

## Aplikacja

- `apps/luna-settings` (bin + logika w libce) oraz `de/session/eu.luna.Settings.desktop`.
- UI na `libluna-widgets`: `NavList` (sekcje + strony), `PreferencesPage`/`PreferencesGroup`,
  `ActionRow`, `Switch`, `Slider`, `ViewSwitcher`, `Button`, `HeaderBar`, `Toast`.
- Ten sam podział co w panelu: **model** (`SettingsApp`, `Page`, dane strony) oddzielony
  od rysowania (`PaintCtx`). Dzięki temu strony mają testy i podglądy headless.
- Wejścia: Quick Panel na pasku (ikona koła zębatego), wpis w launcherze
  (`eu.luna.Settings.desktop`) i bezpośrednio `luna-settings`.
- Ustawienia stosują się od razu, o ile backend to umożliwia. **Język** przełącza
  działające aplikacje Luny bez restartu (patrz „Język — przełączanie na żywo”).

## Strony i kolejność

| Strona | Zakres | Backend / zależność | Stan |
|---|---|---|---|
| **O systemie** | host, kernel, architektura, użytkownik, wersje Luny/Selene | `/proc`, `/etc/os-release` | ✅ |
| **Wygląd** | jasny/ciemny/auto, akcent, styl z pliku | `ThemeSettings`, `theme_file` | ✅ |
| **Ikony** | domyślnie dołączony **Luna-Icons** (pastelowe PNG 16–512 px + SVG), styl wektorowy (pastel/monochrom) i **zewnętrzne zestawy freedesktop** (Papirus, Adwaita…) z podglądem; wybór globalny dla środowiska | `~/.config/luna/icons.conf`, `libluna-widgets::paint_icon`, launcher/taskbar | ✅ |
| **Ekran logowania** | tapety PNG z miniaturami, tryb/akcent, zegar greetera; **szkic zmian + „Zastosuj zmiany” + formularz hasła administratora** (`sudo`) | `/etc/luna/greeter.conf`, `luna-greeter-config` | ✅ |
| **Ekran blokady** | tryb/akcent, zegar, tapeta pulpitu lub jednolite tło | `~/.config/luna/lock.conf`, `luna-lock` (bez roota) | ✅ |
| **Język i region** | wyszukiwany wybór języka, locale (`locale -a`/`SUPPORTED`) i strefy (`zone.tab`/`timedatectl`); zapis `locale.conf` + `region.conf`; język działa **na żywo** we wszystkich aplikacjach Luny | `libluna-i18n`, `libluna-platform`, sesja (`LC_TIME`, `TZ`) | ✅ |
| **Powiadomienia** | DND, pozycja toastów w jednym z czterech rogów, **przycisk „Wyślij test”** (D-Bus), status daemona | `~/.config/luna/notifications.conf`, `luna-shell` | ✅ |
| **Przestrzenie robocze** | liczba (na żywo przez `luna-control workspace-count`) i nazwy; panel oraz podgląd `Super+Tab` aktualizują się od razu | `~/.config/luna/workspaces.conf`, Selene (`ext_workspace_v1`) | ✅ |
| **Dźwięk** | głośność, wyciszenie, domyślne wyjście (wejście/profile później) | `wpctl` / PipeWire | ✅ |
| **Ekrany** | odczyt złączy DRM + jasność | `/sys/class/drm`, `/sys/class/backlight` | ◐ |
| **Zasilanie** | bateria, profil power-saver (usypianie/wieko później) | `power-profiles-daemon`, `/sys` | ✅ |
| **Sieć** | Wi‑Fi (lista, połączenie), przewód, VPN później | `nmcli` / NetworkManager | ⏳ |
| **Bluetooth** | adapter, urządzenia, parowanie | BlueZ D-Bus / `bluetoothctl` | ⏳ |
| **Monitory** | rozdzielczość, skala, układ, obrót, monitor główny | **wymaga Selene** (`zwlr_output_manager_v1` lub rozszerzenie `luna-control`) | ⏳ |
| **Klawiatura i mysz** | powtarzanie, przewijanie naturalne, tap-to-click, tempo kursora | `luna-control` + wejście Selene | ⏳ |
| **Użytkownicy** | konto, awatar, hasło, sesje | `loginctl`, polkit | ⏳ |
| **Aplikacje** | domyślne aplikacje, autostart, uprawnienia/portale | `xdg-mime`, XDG autostart | ⏳ |
| **Pakiety ikon** | wybór **zainstalowanych** zestawów freedesktop działa (globalnie); pobieranie/instalacja nowych i usuwanie — kolejny krok | `~/.config/luna/icons.conf`, katalogi `icons` | ◐ |
| **Prywatność i blokada** | czas bezczynności, keyring/polkit (wygląd blokady już jest) | `ext_session_lock`, `luna-keyring` | ◐ |
| **Drukarki** | kolejki, sterowniki | CUPS | ⏳ |
| **Sterowniki i aktualizacje** | status sprzętu, aktualizacje systemu | backend dystrybucji (PaganOS/Arch) — **nie** uniwersalny manager | ⏳ |

Legenda: ✅ działa, ◐ tylko odczyt/częściowo, ⏳ zaplanowane.

### Ważne ograniczenia

- **Ekran logowania** czyta konfigurację systemową `/etc/luna/greeter.conf`, bo greeter
  działa jako osobny użytkownik. Ustawienia zapisują ją przez `luna-greeter-config
  write` pod `sudo -S` (formularz hasła administratora w aplikacji; hasło idzie
  przez stdin), a wybór trafia również do greetera startującego z tej samej maszyny.
  Tapety spoza katalogów systemowych (np. z katalogu domowego 0700) pomocnik
  kopiuje do `/var/lib/luna/wallpapers/`, żeby konto `greeter` mogło je odczytać.
  Język greetera pochodzi z systemowego `LANG` (ekran przed logowaniem nie zna
  jeszcze wyboru użytkownika); po zalogowaniu sesja eksportuje `LANGUAGE`
  z `locale.conf`.
- **Ekran blokady** to ustawienie **użytkownika** (`~/.config/luna/lock.conf`).
  `luna-lock` działa w jego sesji, więc zmiany nie wymagają roota — inaczej niż
  systemowy greeter. **Strefa czasowa** działa per sesja (`TZ`), a przycisk
  „Zastosuj w systemie” woła `pkexec timedatectl set-timezone` (polkit).
- **Monitory** nie mogą być w pełni ustawiane, dopóki Selene nie dostanie protokołu
  zarządzania wyjściami (`zwlr_output_manager_v1`) albo poleceń w `luna-control`.
  Do tego czasu strona jest tylko do odczytu.
- **Sterowniki** są zależne od dystrybucji. Robimy interfejs backendu (np. PaganOS/pacman),
  a nie „uniwersalny instalator sterowników”.
- **Język** jest wspólny dla całego środowiska i działa **bez restartu** aplikacji:
  `libluna-platform` obserwuje mtime `~/.config/luna/locale.conf`, przeładowuje katalog
  `libluna-i18n`, ustawia `LANGUAGE` w procesie i odrysowuje okno. Aplikacje, które
  cache'ują napisy (sidebar `luna-files`, nawigacja Ustawień, tytuł okna), odświeżają je
  po wykryciu nowego języka. Nowo uruchamiane procesy dziedziczą `LANGUAGE` z sesji.
  Locale i strefa czasowa są per sesja (`LC_TIME`, `TZ`) — zadziałają po przelogowaniu.
- **Powiadomienia**: pozycja jest liczona w obszarze powierzchni „chrome” powłoki
  (pasek + launcher). Górne rogi są dokładnie na ekranie; dla dolnych potrzebna będzie
  osobna powierzchnia overlay na cały ekran (kolejny krok).
- **Przestrzenie robocze**: nazwy i liczba działają od razu (`luna-control
  workspace-name` / `workspace-count`); Selene dodaje i usuwa uchwyty
  `ext_workspace_v1`, więc panel oraz podgląd `Super+Tab` nie wymagają wylogowania.
- **Ikony zewnętrzne**: zestawy muszą być w formacie freedesktop w
  `/usr/share/icons`, `~/.local/share/icons` lub `~/.icons` (z `index.theme`).
  libLuna czyta z nich pliki **PNG i SVG** (SVG jest rasteryzowane przez
  `resvg`; gdy brak obu, zostaje fallback wektorowy). Wybór jest globalny:
  launcher, taskbar, pliki i Ustawienia
  rysują ten sam zestaw; kontrolki UI (strzałki, „×”) pozostają wektorowe.
  W repo jest gotowy pakiet **Luna-Icons** (`assets/icons/Luna-Icons`, regeneracja:
  `make icons`), instalowany do `share/icons/Luna-Icons` — PNG 16–512 px
  i skalowalne SVG.
  Dodatkowo Ustawienia zapisują motyw do **GTK 3/4** (`settings.ini`), **GNOME**
  (`gsettings`), **KDE** (`kdeglobals`) i **Qt** (`qt5ct`/`qt6ct`), więc nowe
  aplikacje tych toolkitów też go użyją. Poprzedni motyw GTK jest zapamiętywany
  (`gtk_previous`) i przywracany po powrocie do ikon Luny.
- **Styl pastelowy**: domyślne ikony wektorowe Luny są wypełnione i wielokolorowe,
  ale każda rodzina ma własny, stały odcień: katalogi i pliki — maślany/błękitny,
  terminal — miętowy, ustawienia — perłowy, multimedia — różowy/liliowy, a stan
  (Wi‑Fi, bateria, głośność) — zielony/fioletowy. Kształty są znaczące: kartka ma
  zagięty róg i linie tekstu, terminal okno z promptem, aparat obiektyw, a koperta
  klapkę. Kontrolki interfejsu (menu, lupka, strzałki, „×”, „+”) celowo zostają
  jednokolorowe, żeby nie rozstrajać paska i przycisków. `Monochrome` wyłącza
  kolory i rysuje cały zestaw kolorem tekstu.

## Wzorzec backendów

- Każda strona ma własny model danych i funkcje `load`/`apply`; rysowanie nie woła
  shelli i nie czyta `/sys` w pętli klatki.
- Preferujemy D-Bus/portale/`/sys`; polecenia systemowe dostają timeouty i czytelne błędy.
- Wspólny moduł konfiguracji w `~/.config/luna/` (`libluna-core::config`):
  `theme.conf`, `locale.conf`, `region.conf`, `icons.conf`, `lock.conf`,
  `notifications.conf`, `workspaces.conf`. Powłoka, locker i ustawienia czytają
  ten sam format `klucz = wartość`.

## Język — przełączanie na żywo

- Wybór języka w Ustawieniach zapisuje `~/.config/luna/locale.conf` (`language = pl|en|de|…`).
- Każdy proces na libLunie sprawdza mtime tego pliku przy przebudzeniu pętli
  (`libluna-platform::refresh_appearance`). Po zmianie: `set_locale(preferred_locale())`,
  ponowne `load_catalogs_from_dirs()` i `std::env::set_var("LANGUAGE", …)`.
- Katalogi `i18n/<kod>.lang` są wbudowane w binarki; dodatkowe można dołożyć do
  `/usr/share/luna/i18n`, `~/.local/share/luna/i18n` lub `~/.config/luna/i18n`.
- Aplikacje, które trzymają gotowe napisy w modelu (sidebar plików, nawigacja Ustawień,
  tytuł okna), wykrywają zmianę `locale()` i przebudowują te elementy; `Frame::set_title`
  pozwala zaktualizować tytuł `xdg_toplevel` bez restartu.
- Język jest wspólny dla powłoki, launchera, popupów, OSD, powiadomień, `luna-lock`,
  `luna-polkit`, `luna-files`, greetera i Ustawień. Locale (`LC_TIME`) i `TZ` ustawia
  sesja przy logowaniu — zmiana tych pól wymaga przelogowania.
- **Cała sesja**: `save_preferred_locale` zapisuje `~/.config/environment.d/00-luna-locale.conf`
  (`LANGUAGE`, a gdy locale jest wygenerowane — także `LANG`), woła
  `dbus-update-activation-environment --systemd` i `systemctl --user import-environment`.
  Dzięki temu nowe programy spoza Luny startują w wybranym języku; już działające
  trzeba uruchomić ponownie (tak samo w GNOME/KDE). `luna-session` robi to także
  przy logowaniu.
- **Brakujące locale** (`pl_PL`, `en_US`, `de_DE`, `C.UTF-8`) są generowane bez roota
  przez `localedef` do `~/.local/share/locale`, a sesja dostaje `LOCPATH`. Dzięki temu
  np. niemiecki działa też w aplikacjach Qt/GTK na systemie, gdzie wygenerowano tylko
  polski i angielski.
- **Po aktualizacji** upewnij się, że w `PATH` nie ma starszej kopii powłoki
  (np. `/usr/local/bin/luna-shell` przed nowszym `/usr/bin/luna-shell`). Stara
  powłoka nie zna mechanizmu odświeżania i język nie zmieni się w panelu/launcherze,
  dopóki nie zostanie podmieniona i zrestartowana.

## Powiadomienia — stan obecny

- **Nie ma osobnego procesu-daemona.** `luna-shell` rejestruje usługę
  `org.freedesktop.Notifications` na magistrali sesyjnej (`dbus_notifications.rs`),
  więc to ona jest daemonem powiadomień.
- Aplikacje (`libnotify`, `notify-send`, Firefox, menedżery plików) wołają standardowy
  D-Bus; powłoka zamienia żądanie na toast i wpis w centrum powiadomień.
- Jeśli nazwę trzyma inny daemon (np. `dunst`/`mako` w innej sesji), `NotificationsBus::spawn`
  zwraca `None` i Luna nic nie przejmuje.
- Obsługiwane: `actions`, `body`, `persistence` (`expire_timeout = 0`), `replaces_id`
  (zastąpienie istniejącego wpisu) i `CloseNotification` (powód 3). Powłoka wysyła
  `ActionInvoked` i `NotificationClosed`; wygaśnięcie to powód 1, klik użytkownika — 2.
- Pozostałe ograniczenia: brak hintów ikony/dźwięku (`image-path`, `image_data`,
  `sound-name`) i `body-markup`; przy `LUNA_PANELS=all` nazwę przejmuje tylko pierwsza
  instancja powłoki, a historia żyje w jej procesie.
- Dalej: jeśli powiadomienia mają działać niezależnie od powłoki i spójnie na wielu
  monitorach — wydzielić `luna-notifyd`: jeden właściciel usługi, historia i DND w jednym
  miejscu, a powłoki subskrybują zdarzenia i rysują toasty/centrum.
