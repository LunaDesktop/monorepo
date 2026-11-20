# Luna Mail — klient poczty

`luna-email` to klasyczny klient poczty dla środowiska Luna: pasek folderów,
lista wiadomości i podgląd treści. Działa na `libLuna` (bez GTK) i nie zawiera
telemetrii — łączy się wyłącznie z serwerami IMAP/SMTP wskazanymi w konfiguracji.

![Luna Mail — skrzynka odbiorcza](images/email-inbox.png)

![Luna Mail — nowa wiadomość](images/email-compose.png)

![Luna Mail — ustawienia konta](images/email-account.png)

## Co robi

- **Foldery** — pobierane z serwera (`LIST`), z rozpoznaniem skrzynki odbiorczej,
  wysłanych, roboczych, kosza i archiwum. Kolejne wiadomości: strony po 50.
- **Lista wiadomości** — nadawca, temat, czas i początek treści; nieprzeczytane
  mają kropkę i pogrubienie, gwiazdka oznacza flagę, ikona dokumentu — załącznik.
- **Podgląd** — treść `text/plain`; gdy jej nie ma, HTML jest zamieniany na tekst
  bez pobierania zdalnych obrazów i bez wykonywania skryptów.
- **Odpowiedź i przekazanie** — cytat w treści, `Re:`/`Fwd:` bez podwajania.
- **Wysyłka SMTP** — z załącznikami i kopią w folderze „Wysłane”, jeśli serwer go ma.
- **Załączniki** — zapis kliknięciem do `~/Downloads` (nazwa nigdy nie nadpisuje
  istniejącego pliku), dodawanie przez przeciągnięcie plików na okno.
- **Wyszukiwanie** — po nadawcy, temacie i początku treści w pobranej stronie.
- **Tryb demo** — bez skonfigurowanego konta aplikacja pokazuje przykładową
  skrzynkę, więc da się ją obejrzeć i przetestować bez serwera.

## Konfiguracja konta

Ustawienia otwiera menu (☰) → **Ustawienia konta** albo przycisk na pustym
podglądzie. Formularz pozwala wpisać adres, nazwę, login, hasło i serwery.
**Wykryj ustawienia** rozpoznaje popularnych dostawców po domenie:

| Domena | IMAP | SMTP |
|---|---|---|
| gmail.com | `imap.gmail.com:993` (TLS) | `smtp.gmail.com:465` (TLS) |
| outlook.com / hotmail.com | `outlook.office365.com:993` | `smtp.office365.com:587` (STARTTLS) |
| yahoo.com | `imap.mail.yahoo.com:993` | `smtp.mail.yahoo.com:465` |
| icloud.com / me.com | `imap.mail.me.com:993` | `smtp.mail.me.com:587` (STARTTLS) |
| wp.pl / o2.pl / interia.pl / onet.pl / gazeta.pl | `imap.<domena>:993` | `smtp.<domena>:465` |
| proton.me | `127.0.0.1:1143` (Bridge) | `127.0.0.1:1025` |
| inna domena | `imap.<domena>:993` | `smtp.<domena>:465` |

Port `143`/`587`/`25` automatycznie oznacza STARTTLS.

### Hasła

Plik `~/.config/luna/email.conf` ma prawa `0600` i zawiera hasło (heksadecymalnie,
żeby tabulatory i nowe linie nie rozsypały rekordu). Zamiast zapisywać hasło
w pliku, można zostawić pole puste i ustawić zmienną środowiskową:

```sh
export LUNA_EMAIL_PASSWORD='hasło-aplikacji'
```

Dla Gmaila, Outlooka i większości dostawców **należy użyć hasła aplikacji** —
zwykłe hasło może być odrzucone przy włączonym uwierzytelnianiu dwuskładnikowym.
OAuth2 („Zaloguj przez Google/Microsoft”) nie jest jeszcze obsługiwany, bo wymaga
własnego Client ID i procesu zgody; to osobna decyzja produktowa.

## Klawiatura

| Skrót | Działanie |
|---|---|
| `Ctrl+N` | nowa wiadomość |
| `Ctrl+R` | odpowiedź |
| `Ctrl+Shift+F` | przekazanie |
| `Ctrl+F` | wyszukiwanie |
| `F5` | odświeżenie folderu |
| `Ctrl+Shift+R` / `Ctrl+Shift+U` | oznacz jako przeczytane / nieprzeczytane |
| `S` | przełącz gwiazdkę |
| `Delete` | przenieś do kosza (w koszu: usuń) |
| `↑` `↓` `PgUp` `PgDn` `Home` `End` | nawigacja po liście |
| `Enter` | oznacz jako przeczytane (podgląd jest zawsze widoczny) |
| `Ctrl+Enter` | wyślij (w edytorze) |
| `Esc` | zamknij edytor / wyszukiwanie / menu |
| `Ctrl+Q` | zakończ |
| `F1` | o aplikacji |

W polach tekstowych działają `Ctrl+A/C/X/V`, `Home`/`End`, `Shift`+strzałki
(zaznaczanie) oraz `Tab`/`Shift+Tab` (przejście do kolejnego pola).

## Wygląd i wąskie okna

Okno jest trójdzielne. Poniżej 860 px pasek folderów staje się szufladą nad
treścią (przycisk ☰/lista w lewym górnym rogu), a lista i podgląd dzielą
pozostałą szerokość. Długie tematy, adresy i początek treści są skracane
wielokropkiem **w granicach swojej kolumny** — także w powiadomieniach i menu.

## Prywatność i bezpieczeństwo

- Aplikacja łączy się tylko z hostami z konfiguracji; brak analityki i „domyślnych”
  serwerów.
- Treść HTML nigdy nie trafia do UI: jest zamieniana na tekst, a `<script>`,
  `<style>` i `<head>` są pomijane, więc zdalne trackery 1×1 nie mają jak się
  wykonać.
- Załączniki są zapisywane pod `~/Downloads` z unikalną nazwą; aplikacja nie
  uruchamia pobranych plików.
- Tylko tekst jest wysyłany jako `text/plain` — bez HTML i bez zdalnych zasobów.

## Ograniczenia

- Brak OAuth2, PGP/S/MIME, wątków (threading) i filtrów serwerowych — na razie
  wyszukiwanie działa po pobranej stronie.
- Brak trwałego cache offline: po restarcie lista jest pobierana ponownie.
- Wysyłka HTML nie jest jeszcze dostępna (edytor jest tekstowy).
- Brak okna wyboru plików (portal) — załączniki dodaje się przez przeciągnięcie.

## Podglądy

```sh
cargo run --manifest-path apps/luna-email/Cargo.toml --bin preview
```

Tworzy `docs/images/email-inbox.png`, `email-compose.png`, `email-account.png`,
`email-about.png` i `email-narrow.png` bez kompozytora.

## Testy

```sh
cargo test --manifest-path apps/luna-email/Cargo.toml --lib
```

Testy obejmują model (adresy, walidacja szkicu, daty, skracanie), konwersję
HTML→tekst, budowę wiadomości RFC 822, układ okna i zachowanie trybu demo.
