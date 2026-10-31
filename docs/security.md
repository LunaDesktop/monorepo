# Luna Security — monitor prywatności i zgód

`luna-security` to daemon i narzędzie CLI, które odpowiada na pytanie:
**która aplikacja właśnie używa mikrofonu/kamery i gdzie wysyła dane?**

Nie jest firewallem — zbiera dowody, pokazuje je na panelu i prosi o zgodę.
Egzekwowanie ruchu (nftables/BPF) to kolejny etap.

## Co monitoruje

| Sygnał | Źródło |
| --- | --- |
| mikrofon | strumienie PipeWire `Stream/Input/Audio`, bezpośrednie `open("/dev/snd/pcm*c")` |
| kamera | strumienie PipeWire `Stream/Input/Video`, `open("/dev/video*")`, `/dev/media*` |
| sieć | publiczne adresy z `/proc/net/tcp{,6}`, `udp{,6}` powiązane z deskryptorami procesu |
| aplikacja | pliki `.desktop` (`luna::apps`) po nazwie `Exec`, fallback `comm`/`exe` |

Loopback i sieci prywatne (`10/8`, `172.16/12`, `192.168/16`, `169.254/16`,
`fc00::/7`, `fe80::/10`) **nie** wywołują zgody. Systemowe narzędzia CLI
(`curl`, `git`, `pacman`, `ssh`, …) nie dostają modala — ich ruch trafia tylko
do dziennika.

## Komendy

```sh
luna-security                      # daemon (także: luna-security daemon)
luna-security status [--json]      # aktywne aplikacje i oczekujące zgody
luna-security watch                # status odświeżany co sekundę
luna-security pending              # tylko prośby o zgodę
luna-security allow <app> [cap]    # cap: mic|camera|network|all (domyślnie network)
luna-security deny  <app> [cap]
luna-security reset <app>          # usuń politykę aplikacji
luna-security log [n]              # ostatnie wpisy dziennika (domyślnie 40)
luna-security scan                 # jednorazowy skan, bez zapisu
```

`<app>` może być nazwą pliku `.desktop` (`firefox`), nazwą wyświetlaną
(`Firefox`) albo binarką (`google-chrome-stable`). Przykłady:

```sh
luna-security allow firefox                  # sieć
luna-security allow firefox mic,camera
luna-security deny com.play0ad.zeroad all
luna-security reset firefox
```

## Panel i modal

- Ikony **mikrofonu / kamery** pojawiają się na pasku, gdy aplikacja faktycznie
  ich używa. Ikona sieci pojawia się tylko przy **alercie** (prośba lub odmowa),
  a nie przy każdym dozwolonym połączeniu.
- Czerwona kropka = oczekująca prośba albo aktywność z polityką `deny`.
- Gdy aplikacja bez decyzji łączy się z publicznym adresem, powłoka pokazuje
  **modal „Luna Security”**:
  - **Zezwól** — zapisuje `allow` w polityce,
  - **Odmów** — zapisuje `deny` (aktywność nadal jest widoczna jako alert),
  - `Esc` = **Odmów** (bezpieczna domyślna decyzja).
- Decyzje są pamiętane w `~/.config/luna/security.conf`.

## Pliki

| Ścieżka | Rola |
| --- | --- |
| `~/.config/luna/security.conf` | trwała polityka `allow`/`deny` |
| `$XDG_RUNTIME_DIR/luna-security/state` | migawka dla powłoki (TSV) |
| `$XDG_RUNTIME_DIR/luna-security/decision` | decyzje z modala/CLI do przetworzenia |
| `$XDG_RUNTIME_DIR/luna-security/ready` | PID działającego daemona |
| `~/.local/state/luna/security.log` | dziennik zdarzeń i decyzji |

Format `security.conf`:

```text
allow=firefox:network,mic
allow=google-chrome:network
deny=com.play0ad.zeroad:network
```

`state` jest listą linii TSV (`activity` / `prompt`) — łatwo ją czytać i testować
bez D-Busa.

## Uruchamianie w sesji

`luna-session` startuje `luna-security` w tle obok `luna-keyring` i `luna-polkit`.
Do ręcznego testu:

```sh
XDG_RUNTIME_DIR=/tmp/luna-sec/rt \
XDG_CONFIG_HOME=/tmp/luna-sec/config \
XDG_STATE_HOME=/tmp/luna-sec/state \
luna-security daemon
```

## Prywatność i ograniczenia

- Daemon czyta **metadane** (`/proc`, `pw-dump`), nie treść pakietów ani dźwięk.
- `deny` nie blokuje połączenia — oznacza brak zgody, loguje naruszenie i pokazuje
  alert. Blokowanie wymaga reguł nftables/BPF (planowane).
- PipeWire pokazuje strumienie tylko klientów, którzy zgłosili `application.process.id`;
  gdy go brak, dopasowujemy po `application.name`.
- Aplikacje mogą otwierać urządzenia przez zewnętrzne usługi (np. portal).
  Wtedy widzimy strumień PipeWire, a nie deskryptor aplikacji — to zamierzone.
