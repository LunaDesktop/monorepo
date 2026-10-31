# Luna Settings — centrum ustawień środowiska i systemu

Cel: **jedna aplikacja** `luna-settings` (odpowiednik GNOME Settings / KDE System Settings),
a nie tylko strona ikon. Ma obejmować zarówno ustawienia **środowiska** (wygląd, powłoka,
powiadomienia, skróty), jak i **systemu** (monitory, dźwięk, sieć, Bluetooth, zasilanie,
język, użytkownicy, sterowniki).

## Stan pierwszej wersji

- Działa `apps/luna-settings` na libLunie (`NavList` z sekcjami, `PreferencesPage`,
  `ActionRow`, `Switch`, `Slider`, `ViewSwitcher`, `Button`, `HeaderBar`).
- Sekcja **Środowisko**: **Wygląd i ikony** (motyw, akcent, styl, ikony i podgląd
  w jednym miejscu), **Ekran logowania** (tapety z miniaturami, **wybór dowolnego PNG
  z dysku**, formularz hasła administratora), **Ekran blokady** (per użytkownik),
  **Powiadomienia** (DND + róg ekranu + test), **Język i region** (wyszukiwany wybór
  języka, locale i strefy, NTP) oraz **Przestrzenie robocze** (liczba, nazwy i
  przełączanie dolnymi rogami ekranu).
- Sekcja **Sprzęt**: **Sieć** (Wi-Fi przez NetworkManager), **Ekrany**, **Dźwięk**,
  **Zasilanie**. Sekcja **Informacje**: **System**, **O Luna**.
- Szczegóły techniczne (ścieżki konfiguracji, backendy, ograniczenia) nie zaśmiecają
  treści stron — są w panelu **Info** (przycisk w nagłówku okna, `BottomSheet`).
- Wejście: Quick Panel na pasku (ikona koła zębatego), wpis `.desktop`, `scripts/install.sh`.
- Podglądy headless: `cargo run --manifest-path apps/luna-settings/Cargo.toml --bin preview` →
  `docs/images/luna-settings-*.png` (w tym `luna-settings-language-picker.png`).
- Kolejne kroki: Bluetooth, użytkownicy, autostart, prywatność, drukarki,
  backend sterowników; pełne sterowanie monitorami po stronie Selene.

## Aplikacja

- `apps/luna-settings` (bin + logika w libce) oraz `de/session/eu.luna.Settings.desktop`.
- **Nazwa w tytule okna i nagłówku pochodzi z systemu** (`/etc/os-release`,
  `NAME` → `PRETTY_NAME` → `ID`), np. „Ustawienia — Pagan Linux”. Nie ma zaszytej
  nazwy „Luna Settings”; ta sama zasada działa na PaganOS, Pagix i innych systemach.
- **Instalator podstawia nazwę systemu do wpisu `.desktop`** (launcher), np.
  `Pagan Linux Settings` / `Ustawienia — Pagan Linux`. Źródłowy plik w repo
  pozostaje neutralny, więc działa na każdej dystrybucji.
- UI na `libluna-widgets`: `NavList` (sekcje + strony), `PreferencesPage`/`PreferencesGroup`,
  `ActionRow`, `Switch`, `Slider`, `ViewSwitcher`, `Button`, `HeaderBar`, `Toast`, `BottomSheet`.
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
| **Wygląd i ikony** | jasny/ciemny/auto, akcent, styl z pliku oraz **ikony** (dołączony Luna-Icons, styl wektorowy i zewnętrzne zestawy freedesktop) z podglądem; **tapeta pulpitu** (miniatury + wybór PNG z dysku, zmiana bez wylogowania) | `ThemeSettings`, `theme_file`, `~/.config/luna/icons.conf`, `~/.config/luna/wallpaper.conf` | ✅ |
| **Ekran logowania** | tapety PNG w **siatce 3×N z miniaturami**, kafel **„Więcej tapet”** (źródła: Obrazy, Tapety systemowe, katalog domowy, **Zaskocz mnie**, przeglądanie dysku), tryb/akcent, zegar greetera; **szkic zmian + „Zastosuj zmiany” + formularz hasła administratora** (`sudo`) | `/etc/luna/greeter.conf`, `luna-greeter-config` | ✅ |
| **Ekran blokady** | tryb/akcent, zegar, tapeta pulpitu lub jednolite tło oraz **automatyczna blokada po bezczynności** | `~/.config/luna/lock.conf`, `~/.config/luna/idle.conf`, `luna-lock` (bez roota) | ✅ |
| **Język i region** | wyszukiwany wybór języka, locale (`locale -a`/`SUPPORTED`) i strefy (`zone.tab`/`timedatectl`); zapis `locale.conf` + `region.conf`; język działa **na żywo** we wszystkich aplikacjach Luny; przełącznik NTP | `libluna-i18n`, `libluna-platform`, sesja (`LC_TIME`, `TZ`), `timedatectl` | ✅ |
| **Powiadomienia** | DND, pozycja toastów w jednym z czterech rogów, **przycisk „Wyślij test”** (D-Bus), status daemona | `~/.config/luna/notifications.conf`, `luna-shell` | ✅ |
| **Przestrzenie robocze** | liczba (na żywo przez `luna-control workspace-count`) i nazwy; panel, pulpit oraz podgląd `Super+Tab` aktualizują się od razu; opcjonalne **przełączanie dolnymi rogami** i **podgląd w lewym górnym rogu obszaru roboczego** (to samo co `Super+Tab`); róg działa po **0,5 s przytrzymania**, a rosnąca **poświata w narożniku** pokazuje odliczanie (wjazd po drodze do launchera nic nie otwiera) | `~/.config/luna/workspaces.conf`, Selene (`ext_workspace_v1`, `luna-control workspace-corner` / `workspace-overview-corner`) | ✅ |
| **Sieć** | Wi‑Fi: skanowanie, łączenie (także z hasłem i sieci ukryte), włączanie/wyłączanie radia, zapisane sieci („Automatyczne łączenie”, „Zapomnij”); przewodowe: DHCP lub statyczny IPv4 z bramą i DNS; flaga połączenia taryfowanego; **VPN**: lista profili, łączenie/rozłączanie, import `.ovpn`/`.conf` | NetworkManager (D-Bus, warstwa `luna::network` i `luna::vpn`); `nmcli` **tylko** do importu pliku | ✅ |
| **Dźwięk** | głośność, wyciszenie, wybór **wyjścia** i **wejścia** (lista urządzeń PipeWire) oraz **10-pasmowy korektor** z przedwzmacniaczem i krzywymi (Płaski/Bas/Wokal/Rock/Soprany); stan korektora i głośność są wspólne z panelem | `wpctl` / PipeWire, wtyczka LADSPA `luna-eq`, `~/.config/luna/equalizer.conf` | ✅ |
| **Ekrany** | **graficzny układ monitorów** (przeciąganie prostokątów z przyciąganiem do krawędzi, „rozmieść automatycznie”, klik wybiera ekran), lista złączy z rozdzielczością, pozycją i liczbą trybów, wybór **rozdzielczości** i **odświeżania** (przełączany **na żywo**, zapisywany per monitor), **barwa ekranu** (night light — gamma LUT, na żywo), jasność oraz **profile kolorów ICC** przypisywane ekranom | Selene (migawka `luna-monitors`, tryby z modesetu, `luna-control output-mode`), `~/.config/luna/displays.conf`, `/sys/class/backlight`, **colord** (D-Bus) | ✅ |
| **Zasilanie** | bateria, **wybór profilu zasilania** (oszczędzanie/zbalansowany/wydajny), **wygaszanie ekranu** i **usypianie po bezczynności** (oraz „blokuj przy wygaszeniu”) | `power-profiles-daemon` (D-Bus), `/sys`, `~/.config/luna/idle.conf`, Selene | ✅ |
| **Użytkownicy** | lista lokalnych kont, bieżące konto, rola (administrator/zwykły), katalog i powłoka; **dodawanie kont** i **zmiana hasła** przez formularz z hasłem administratora; **sesje innych użytkowników** z przełączaniem | `/etc/passwd`, `/etc/group`, pomocnik `luna-accounts` (`sudo`), logind (D-Bus) | ✅ |
| **Dostępność** | **skala tekstu** (100/125/150 %), **wysoki kontrast**, **ograniczenie animacji** — nakładane przez motyw, więc działają w całym środowisku | `~/.config/luna/accessibility.conf`, `ThemeSettings::resolve_themed` | ✅ |
| **Domyślne aplikacje** | przeglądarka, poczta, menedżer plików, obrazy, wideo, muzyka, PDF, edytor — wybór z zainstalowanych wpisów `.desktop`; zapis przez `xdg-mime` (`mimeapps.list`) | `xdg-mime`, `$XDG_DATA_DIRS/applications`, `~/.config/mimeapps.list` | ✅ |
| **Bluetooth** | adapter (włącz/wyłącz), skanowanie, lista urządzeń z RSSI, parowanie, łączenie/rozłączanie i usuwanie | BlueZ (D-Bus, `org.bluez`) | ✅ |
| **Klawiatura i mysz** | opóźnienie i szybkość powtarzania klawiszy, naturalne przewijanie (na żywo przez `luna-control input …`); tap-to-click, gesty i akceleracja — jasno oznaczone jako „w trakcie rozwoju” | `~/.config/luna/input.conf`, wejście Selene | ◐ |
| **Skróty klawiszowe** | **profil Luna** (Super+…) i 19 akcji pulpitu (okna, kafle, podgląd, przestrzenie, zrzut, wylogowanie) z **nagrywaniem kombinacji**; zmiana działa **bez wylogowania** (`mtime` pliku) | `~/.config/luna/shortcuts.conf`, `libluna-core::shortcuts`, Selene | ✅ |
| **Aplikacje** | domyślne aplikacje, autostart, uprawnienia/portale | `xdg-mime`, XDG autostart | ⏳ |
| **Pakiety ikon** | wybór **zainstalowanych** zestawów freedesktop działa (globalnie); pobieranie/instalacja nowych i usuwanie — kolejny krok | `~/.config/luna/icons.conf`, katalogi `icons` | ◐ |
| **Prywatność i blokada** | czas bezczynności, keyring/polkit (wygląd blokady już jest) | `ext_session_lock`, `luna-keyring` | ◐ |
| **Drukarki** | lista kolejek CUPS ze stanem i adresem IPP, drukarka domyślna, włączanie/wyłączanie, usuwanie oraz dodawanie drukarki sieciowej (driverless IPP) | CUPS (`lpstat`, `lpadmin` przez `pkexec`) | ✅ |
| **Sterowniki i aktualizacje** | status sprzętu, aktualizacje systemu | backend dystrybucji (PaganOS/Arch) — **nie** uniwersalny manager | ⏳ |

Legenda: ✅ działa, ◐ tylko odczyt/częściowo, ⏳ zaplanowane.

## Konta użytkowników

- Strona **Użytkownicy** czyta `/etc/passwd` i `/etc/group` (bez procesów): login,
  pełna nazwa, rola (`wheel`/`sudo`/`admin`), katalog domowy i powłoka.
- **Dodaj konto** i **Zmień hasło** otwierają modal: nowe hasło + powtórzenie +
  hasło administratora. Walidacja (login, nazwa, długość hasła) odbywa się także
  po stronie roota.
- Zmiany wykonuje pomocnik `bin/luna-accounts`: Ustawienia najpierw robią
  `sudo -S -v` (uwierzytelnienie hasłem administratora), a potem uruchamiają
  pomocnika przez `sudo -n` — `useradd -m -s /bin/bash` + `chpasswd` (nowe konto)
  albo `chpasswd` (zmiana hasła). **Hasła idą wyłącznie przez stdin** — nigdy jako
  argumenty, więc nie widać ich w `ps`; pomocnik dostaje tylko nowe hasło, także
  wtedy, gdy `sudo` ma już ważny „timestamp”.
- Gdy ustawienie hasła nowego konta się nie uda, konto jest usuwane
  (`userdel -r`), żeby nie zostało konto bez hasła.
- Wymagania: `sudo` w systemie i zainstalowany pomocnik
  (`sudo ./scripts/install.sh --only luna`). Bez niego strona pokazuje komunikat.

## Bluetooth (BlueZ, D-Bus)

- Warstwa `luna::bluetooth` czyta `ObjectManager.GetManagedObjects` i pokazuje adaptery
  (`Adapter1`) oraz urządzenia (`Device1`) z nazwą, adresem, RSSI, stanem
  sparowania/połączenia i zaufaniem.
- Akcje: `Powered`, `StartDiscovery`/`StopDiscovery`, `Pair`, `Connect`,
  `Disconnect`, `RemoveDevice` — wszystko przez `org.bluez`.
- Parowanie rejestruje tymczasowego agenta `NoInputNoOutput` (tylko na czas operacji),
  więc działa dla słuchawek/klawiatur typu „Just Works”. Urządzenia wymagające
  PIN-u lub passkeya pokazują czytelny błąd (planowany agent z promptem).
- Strona odświeża migawkę co 2 s (w tle), więc lista urządzeń rośnie podczas skanu.

## VPN (NetworkManager)

- Lista profili `type=vpn|wireguard` z `Settings.ListConnections`, stan aktywności
  z `ActiveConnections`; `connect`/`disconnect`/`remove` idą po D-Busie.
- Import pliku (`.ovpn`, `.conf`) robi `nmcli connection import` — to jedyne miejsce
  z CLI, bo NetworkManager nie ma metody D-Bus importu pliku.

## Drukarki (CUPS)

- Odczyt: `lpstat -p -d` + `lpstat -v` (nazwa, stan, URI, drukarka domyślna).
- Zmiany: `pkexec lpadmin`/`cupsenable`/`cupsdisable` — polkit pokazuje autoryzację
  (agent `luna-polkit`). Dodawanie używa modelu `everywhere` (driverless IPP),
  a nazwa drukarki jest wyliczana z adresu (`ipp://biuro.local:631/…` → `biuro.local`).

## Dostępność

- `~/.config/luna/accessibility.conf`: `text_scale` (100/125/150 %),
  `high_contrast`, `reduce_motion`.
- `ThemeSettings::resolve_themed` nakłada je **po** motywie i stylu, więc ta sama
  konfiguracja działa w Ustawieniach, Plikach, Terminalu, powłoce i greeterze.
  Skala zwiększa typografię i wysokości kontrolek; wysoki kontrast podmienia paletę
  (akcent użytkownika zostaje). `reduce_motion` wyłącza animacje toastów.
- Lupa i klawiatura ekranowa są oznaczone jako planowane (poza zakresem bety).

## Sesje i profile kolorów

- **Sesje** (`luna::sessions`): `logind.Manager.ListSessions` + właściwości
  `Type`/`Active`; „Przełącz” woła `ActivateSession` (logind może poprosić o zgodę).
- **Profile kolorów** (`luna::colors`): `colord.GetDevices/GetProfiles`;
  przypisanie idzie przez `pkexec colormgr device-add-profile`, import pliku ICC
  przez `pkexec colormgr import-profile`. Brak colorda to „usługa niedostępna”,
  nie błąd strony.

## Domyślne aplikacje

- Warstwa `luna::default_apps` czyta wpisy `.desktop` z `$XDG_DATA_HOME/applications`
  i `$XDG_DATA_DIRS/applications` (pierwszy katalog wygrywa, `NoDisplay`/`Hidden`
  są pomijane) oraz pyta `xdg-mime query default` o bieżące skojarzenia.
- Kategorie (przeglądarka, poczta, pliki, obrazy, wideo, muzyka, PDF, tekst) są
  filtrowane po `MimeType` wpisu i kategorii `.desktop` (`WebBrowser`,
  `FileManager`, `ImageViewer`…). Zapis: `xdg-mime default <id> <mime…>`,
  czyli standardowy `~/.config/mimeapps.list` — bez własnej bazy i bez roota.
- Brak `xdg-mime` jest pokazywany jako czytelny komunikat, a nie błąd strony.

## Skróty klawiszowe

- Plik `~/.config/luna/shortcuts.conf` (`profil` + `akcja = kombinacja`, np.
  `close-window = Super+Q`). Selene czyta go z cache po `mtime` przy każdym
  klawiszu, więc zmiana działa natychmiast, bez wylogowania.
- **Profil Luna** — jedyny wbudowany zestaw (`Super+…`); ręczna zmiana jednej
  kombinacji oznacza profil jako `custom`, a „Przywróć domyślne” wraca do Luny.
- Akcje: zamykanie/minimalizacja/maksymalizacja/przywracanie, pełny ekran, kafle
  w lewo/prawo, podgląd, przełączanie okien, 8 przestrzeni (z Shift — przenoszenie
  okna), zrzut ekranu i wylogowanie.
- Nagrywanie w Ustawieniach wymaga Super, Ctrl lub Alt (sam klawisz nie wystarcza),
  Esc anuluje, a `Alt+Tab`/`Alt+F4` są odrzucane jako zarezerwowane dla kompozytora.
  Konflikt powoduje **zamianę** kombinacji między akcjami (z komunikatem).
- Esc na stronie (bez otwartego modalu) **nie zamyka Ustawień** — okno ma
  `escape_to_exit(false)`; Esc zamyka tylko panele, pickery i formularze.

## Bezczynność: wygaszanie, blokada, usypianie

- Jeden zegar bezczynności liczy czas od ostatniego **dowolnego** wejścia
  (klawiatura, mysz, kółko, dotyk). Selene zeruje go także wtedy, gdy aplikacja
  ma fokus wyłączny.
- Trzy niezależne progi w `~/.config/luna/idle.conf` (0 = nigdy):
  `screen_off` (domyślnie 300 s), `lock` (domyślnie 0), `suspend` (domyślnie 0).
  `lock_on_blank = true` dokłada blokadę do wygaszenia.
- **Wygaszenie** to czarna klatka i schowany kursor (także sprzętowy); pierwsze
  zdarzenie wejścia natychmiast przywraca obraz. Selene nie renderuje wtedy
  sceny, więc CPU/GPU odpoczywają — sprzętowe DPMS/backlight jest osobnym krokiem.
- **Blokada** uruchamia `luna-lock` (overlay `wlr-layer-shell`, hasło przez PAM).
- **Usypianie** woła `systemctl suspend` (logind); przed snem uruchamiana jest blokada.
- Zmiany działają **bez wylogowania**: Ustawienia zapisują plik i wysyłają
  `luna-control idle-reload`; Selene przelicza progi i budzi się najpóźniej co 1 s.
- Progi wybiera się z listy (nigdy / 1 / 5 / 10 / 20 / 30 / 60 min) — bez wpisywania liczb.

## Korektor (dźwięk)

- **Wtyczka** `apps/luna-eq` → `lib/luna/libluna-eq.so` (LADSPA, 10 pasm oktawowych
  31 Hz … 16 kHz + przedwzmacniacz). Jest samodzielna (bez libLuny), per-kanał.
- **Dlaczego LADSPA, a nie plugin `builtin`**: `filter-chain` domyślnie ładuje
  `libspa-filter-graph-plugin-builtin.so`, który w części systemów nie startuje
  (zależność `libsndfile` → brakujące symbole LAME). Ścieżka LADSPA używa
  `libspa-filter-graph-plugin-ladspa.so` i nie ma tej zależności.
- **Backend** `luna::equalizer` (Ustawienia i panel korzystają z tych samych funkcji):
  generuje prywatny plik `~/.config/pipewire/luna-equalizer.conf` (nie rusza
  wspólnego `filter-chain.conf.d`), uruchamia `pipewire -c luna-equalizer.conf`
  przez `setsid`, czeka na węzeł `luna_eq_sink` i ustawia go jako **domyślne wyjście**.
  Sprzętowe wyjście trafia do `playback.props.node.target`, więc WirePlumber sam
  podłącza łańcuch do karty.
- **Spójność z panelem**: panel czyta głośność/wyciszenie domyślnego wyjścia
  (`wpctl @DEFAULT_AUDIO_SINK@`). Po włączeniu korektora domyślnym wyjściem jest
  `luna_eq_sink`, a jego głośność jest kopiowana z karty — suwak w panelu i w
  Ustawieniach steruje tym samym.
- **Wyjście zmieniane przy włączonym korektorze** przepina łańcuch na nowe
  urządzenie (restart łańcucha, bez restartu PipeWire).
- **Zmiana pasm** jest wysyłana po ustabilizowaniu suwaka (~0,7 s) — kontrolki
  LADSPA nie są wystawione jako parametry węzła, więc nowe wartości wymagają
  restartu łańcucha; to krótka przerwa w dźwięku, nie w całym systemie.
- **CLI**: `luna-eq start|stop|status|preset <nazwa>|band <0-9> <dB>`.
  Sesja (`luna-session`) woła `luna-eq start`, więc korektor wraca po zalogowaniu.
- **Wymagania instalacji**: `sudo ./scripts/install.sh --only luna` instaluje
  `bin/luna-eq` i `lib/luna/libluna-eq.so`. Bez wtyczki strona pokazuje jasny
  komunikat „Wtyczka luna-eq nie jest zainstalowana”.

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
- **Sieć** ma własną, cienką warstwę `luna::network` (D-Bus), z której korzystają
  **zarówno Ustawienia, jak i panel** (lista sieci, stan radia, łączenie). `Snapshot` z urządzeniami, punktami
  dostępowymi, zapisanymi profilami i konfiguracją IPv4 jest odświeżany w wątku tła
  (`Monitor`), więc klatka UI nigdy nie czeka na magistralę. Hasła trafiają do
  NetworkManagera w słowniku `a{sa{sv}}` — nigdy przez `argv`. Statyczny IPv4 jest
  zapisywany przez `address-data`/`dns-data` (NM ≥ 1.54), a starsze pola `addresses`
  i `dns` są nadal odczytywane. Nowy profil przewodowy przy aktywnym interfejsie
  powstaje przez `AddAndActivateConnection` (uprawnienie `network-control`), więc
  łączenie nie wymaga hasła; **zmiana** zapisanego profilu systemowego wymaga
  autoryzacji administracyjnej (polkit; komunikat w aplikacji jest wtedy jasny).
  Sieci WEP i 802.1X są rozpoznawane, ale oznaczone jako nieobsługiwane.
- **Układ monitorów** jest zapisywany w `displays.conf` (`output <złącze> position = x,y`)
  i stosowany przez Selene **na żywo** (`luna-control output-position`): zmienia się
  model wyjść i geometria wysyłana klientom, bez modesetu. Przeciąganie w Ustawieniach
  przyciąga krawędzie (tolerancja 24 px logiczne), a „rozmieść automatycznie” układa
  ekrany poziomo. Brak pozycji dla nowego monitora dokłada go po prawej stronie.
- **Ekrany**: lista pochodzi z dwóch źródeł — migawki `$XDG_RUNTIME_DIR/luna-monitors`,
  którą Selene zapisuje po modesecie (nazwy, geometria, pełna lista trybów z Hz),
  oraz `sysfs` (`/sys/class/drm/*/modes`) jako zapas, gdy kompozytor nie działa.
  Wybór rozdzielczości/odświeżania idzie do Selene jako `luna-control output-mode`:
  wątek wyjścia tworzy nowe bufory i framebuffery, przepina CRTC, zwalnia stare
  framebuffery i odbudowuje renderery (także GPU), więc nowy tryb działa **od razu**.
  Wybór jest zapisywany w `~/.config/luna/displays.conf` i przy starcie wybierany
  przez `pick_mode` (pasuje rozdzielczość + Hz; bez zgodnego trybu wracamy do
  natywnego). Night light idzie przez `luna-control night-light` (gamma LUT) i po
  zmianie trybu jest wgrywany ponownie, bo modeset zeruje tablicę.
- **Tapeta pulpitu** jest ustawieniem użytkownika (`~/.config/luna/wallpaper.conf`).
  Selene obserwuje mtime pliku i ścieżki obrazu (`wallpaper_stamp`), więc podmiana
  działa **bez wylogowania**; wybór z dysku nie wymaga uprawnień administratora
  (inaczej niż tapeta ekranu logowania, która trafia do `/etc/luna/greeter.conf`).
- **Wybór tapety** (`Ekran logowania` → „Wybierz z dysku”, `Wygląd` → tapeta pulpitu)
  pokazuje **siatkę miniatur 3 w rzędzie** (prawdziwe podglądy PNG, cache 384×216).
  Pierwszy kafel to **„Więcej tapet”**: Obrazy, Tapety systemowe domyślne
  (`/usr/share/wallpapers`, `/usr/share/backgrounds`), katalog domowy, **„Zaskocz
  mnie”** (losowa tapeta z bieżącego katalogu, od razu zastosowana) oraz
  **„Przeglądaj dysk…”**, który przełącza na listę plików. Kafel przełącznika obok
  nazwy folderu wraca do siatki; strzałki/Enter obsługują wybór z klawiatury.
  Dla **pulpitu** kliknięcie kafla daje **podgląd na żywo** (zapis
  `wallpaper.conf`, Selene podmienia tło od razu); **„Anuluj”/Esc przywraca**
  poprzednią tapetę, a „Wybierz” zostawia nową. Ekran logowania zostaje przy
  szkicu, bo jego plik wymaga uprawnień administratora.
- **Powiadomienia**: pozycja jest liczona w obszarze powierzchni „chrome” powłoki
  (pasek + launcher). Górne rogi są dokładnie na ekranie; dla dolnych potrzebna będzie
  osobna powierzchnia overlay na cały ekran (kolejny krok).
  Usługa D-Bus deklaruje teraz `body-markup`, `icon-static` i `sound`; znaczniki HTML
  są sprowadzane do czystego tekstu, a hint `image-path`/`desktop-entry` wybiera ikonę
  toasta. Powłoka co sekundę odczytuje też plik DND, więc przełącznik w Ustawieniach
  działa bez restartu sesji.
- **Przestrzenie robocze**: nazwy i liczba działają od razu (`luna-control
  workspace-name` / `workspace-count`); Selene dodaje i usuwa uchwyty
  `ext_workspace_v1` (z wymaganym `done`), więc panel oraz podgląd `Super+Tab` nie
  wymagają wylogowania. Liczba jest współdzielona z wątkiem wyjścia i wchodzi do
  sygnatury klatki — pulpit nie zostaje przy starej liczbie kropek. Opcja
  `workspace-corner` włącza przełączanie dolnymi rogami, a
  `workspace-overview-corner` otwiera podgląd (Super+Tab) w lewym górnym rogu
  obszaru roboczego — poniżej panelu, żeby nie kolidować z jego menu.
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

## Style z plików (`*.theme`)

- Styl jest nakładany przez `ThemeSettings::resolve_themed` i platforma używa go do
  **całego** `frame.theme`: tło okna, panel, karty, kontrolki, promienie, odstępy,
  typografia i metryki. Wcześniej `App::redraw` wołał tylko `resolve()` (tryb + akcent),
  więc wybór stylu nie zmieniał interfejsu.
- Rozwiązany motyw jest cache'owany (`App::theme_cache`), bo czytanie katalogów stylów
  na każdej klatce byłoby zbyt kosztowne. Zmiana w Ustawieniach unieważnia cache.
- Styl może nadpisać akcent (`colors.accent` albo `accent = …`); jeśli akcent ma
  zawsze wygrywać z wyborem użytkownika, nie ustawiaj go w pliku stylu.
- Ręczna edycja pliku `.theme` w trakcie działania aplikacji jest widoczna po
  ponownym wybraniu stylu (albo restarcie); mtime pliku nie jest obserwowany.

## Wzorzec backendów

- Każda strona ma własny model danych i funkcje `load`/`apply`; rysowanie nie woła
  shelli i nie czyta `/sys` w pętli klatki.
- Preferujemy D-Bus/portale/`/sys`; polecenia systemowe dostają timeouty i czytelne błędy.
- Wspólny moduł konfiguracji w `~/.config/luna/` (`libluna-core::config`):
  `theme.conf`, `locale.conf`, `region.conf`, `icons.conf`, `lock.conf`,
  `notifications.conf`, `workspaces.conf`, `input.conf`. Powłoka, locker i ustawienia
  czytają ten sam format `klucz = wartość`.

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
  zwraca `None` i Luna nic nie przejmuje. Jeśli nazwę trzyma **stary `luna-shell`**,
  nowa instancja przejmuje ją (`ReplaceExisting`); powłoka ponawia też rejestrację
  co 5 s, gdy nazwa nie ma właściciela albo połączenie z magistralą zostało zerwane.
- Strona **Powiadomienia** pokazuje realnego właściciela usługi i jego wersję
  (`GetServerInformation`), a nie statyczny napis „luna-shell”.
- Obsługiwane: `actions`, `body`, `body-markup`, `icon-static`, `sound`, `persistence`
  (`expire_timeout = 0`), `replaces_id` (zastąpienie istniejącego wpisu) i
  `CloseNotification` (powód 3). Powłoka wysyła `ActionInvoked` i `NotificationClosed`;
  wygaśnięcie to powód 1, klik użytkownika — 2.
- Treść `body-markup` jest sprowadzana do czystego tekstu (renderer libLuny nie rysuje
  HTML), a hint `image-path`/`desktop-entry` lub `app_icon` wybiera ikonę toasta.
- Pozostałe ograniczenia: `image_data` i `sound-name` nie są jeszcze rysowane/odtwarzane;
  przy `LUNA_PANELS=all` nazwę przejmuje tylko pierwsza instancja powłoki, a historia
  żyje w jej procesie. Aplikacje spakowane w Flatpak/Snap wymagają działającego
  `xdg-desktop-portal` (on przekazuje powiadomienia do `org.freedesktop.Notifications`).
- Dalej: jeśli powiadomienia mają działać niezależnie od powłoki i spójnie na wielu
  monitorach — wydzielić `luna-notifyd`: jeden właściciel usługi, historia i DND w jednym
  miejscu, a powłoki subskrybują zdarzenia i rysują toasty/centrum.
