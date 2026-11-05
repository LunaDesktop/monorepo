//! Sieć w kroku „Połączenie”: status, lista Wi-Fi i łączenie przez NetworkManager.
//!
//! Nie dodajemy zależności D-Bus — wystarcza `nmcli` (NetworkManager) i zwykłe
//! gniazdo TCP do sprawdzenia internetu. Hasło Wi-Fi idzie przez `stdin`
//! (`nmcli --ask`), nie przez `argv`.
//!
//! Na obrazie live NetworkManager odrzuca operacje `settings.modify.system`
//! („Insufficient privileges”), gdy nie ma agenta polkit. Dlatego każdą
//! operację najpierw próbujemy bezpośrednio, a przy błędzie uprawnień
//! powtarzamy ją przez `pkexec`, a potem przez `sudo -n` (bez pytania o hasło).

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Sieć Wi-Fi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wifi {
    pub ssid: String,
    pub signal: u8,
    pub security: String,
    pub active: bool,
}

impl Wifi {
    pub fn open(&self) -> bool {
        self.security.is_empty() || self.security == "--"
    }

    pub fn label(&self) -> String {
        let lock = if self.open() {
            libluna_i18n::tr("open")
        } else {
            self.security.clone()
        };
        format!("{}  ·  {}%  ·  {}", self.ssid, self.signal, lock)
    }
}

/// Migawka stanu sieci.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkState {
    pub available: bool,
    pub connected: bool,
    pub internet: bool,
    pub interface: Option<String>,
    pub wifi: Vec<Wifi>,
    pub error: Option<String>,
}

impl NetworkState {
    pub fn status(&self) -> String {
        if self.connected && self.internet {
            libluna_i18n::tr("Connected to the internet")
        } else if self.connected {
            libluna_i18n::tr("Connected, but the internet is not reachable")
        } else {
            libluna_i18n::tr("Not connected")
        }
    }
}

/// Pobiera stan sieci.
pub fn snapshot() -> NetworkState {
    let routes = std::fs::read_to_string("/proc/net/route").unwrap_or_default();
    let interface = default_interface(&routes);
    let nmcli = command_available("nmcli");
    let mut state = NetworkState {
        available: nmcli,
        connected: interface.is_some(),
        interface,
        ..NetworkState::default()
    };
    if state.connected {
        state.internet = internet_available();
    }
    if nmcli {
        match wifi_list() {
            Ok(wifi) => state.wifi = wifi,
            Err(error) => state.error = Some(error),
        }
    } else {
        state.error = Some(libluna_i18n::tr(
            "nmcli is not available — configure the network in the live session",
        ));
    }
    state
}

/// Czy jest internet (TCP do publicznego resolvera, bez ICMP).
pub fn internet_available() -> bool {
    const TARGETS: &[(&str, u16)] = &[("1.1.1.1", 53), ("9.9.9.9", 53), ("8.8.8.8", 53)];
    for (host, port) in TARGETS {
        let Ok(address) = format!("{host}:{port}").parse::<SocketAddr>() else {
            continue;
        };
        if TcpStream::connect_timeout(&address, Duration::from_millis(1200)).is_ok() {
            return true;
        }
    }
    false
}

/// Interfejs z domyślnej trasy w `/proc/net/route`.
pub fn default_interface(text: &str) -> Option<String> {
    for line in text.lines().skip(1) {
        let mut fields = line.split_whitespace();
        let iface = fields.next()?;
        let _dest = fields.next()?;
        let _gateway = fields.next()?;
        let flags = u32::from_str_radix(fields.next()?, 16).ok()?;
        // RTF_UP = 0x1, RTF_GATEWAY = 0x2
        if flags & 0x1 != 0 && flags & 0x2 != 0 {
            return Some(iface.to_owned());
        }
    }
    None
}

/// Lista Wi-Fi z `nmcli`.
pub fn wifi_list() -> Result<Vec<Wifi>, String> {
    const ARGS: &[&str] = &[
        "-t",
        "-f",
        "IN-USE,SSID,SIGNAL,SECURITY",
        "device",
        "wifi",
        "list",
    ];
    let output = run_nmcli(ARGS, "", SCAN_TIMEOUT)?;
    if output.success {
        return Ok(parse_nmcli_list(&output.stdout));
    }
    if !privilege_error(&output.message()) {
        return Err(friendly_error(&output.message()));
    }
    // Skanowanie bywa blokowane przez polkit (`wifi.scan`). Bez listy sieci
    // krok jest bezużyteczny, więc próbujemy podnieść uprawnienia.
    match elevated_nmcli(ARGS, "") {
        Ok(output) if output.success => Ok(parse_nmcli_list(&output.stdout)),
        Ok(output) => Err(friendly_error(&output.message())),
        Err(_) => Err(privilege_hint()),
    }
}

/// Parser `nmcli -t` (z escapowaniem `\:`).
pub fn parse_nmcli_list(text: &str) -> Vec<Wifi> {
    let mut wifi: Vec<Wifi> = Vec::new();
    for line in text.lines() {
        let fields = split_escaped(line);
        if fields.len() < 4 {
            continue;
        }
        let ssid = fields[1].clone();
        if ssid.is_empty() {
            continue;
        }
        let signal = fields[2].parse::<u8>().unwrap_or(0).min(100);
        let security = fields[3].clone();
        let active = fields[0].contains('*');
        if let Some(existing) = wifi.iter_mut().find(|item| item.ssid == ssid) {
            if active || signal > existing.signal {
                *existing = Wifi {
                    ssid,
                    signal,
                    security,
                    active,
                };
            }
            continue;
        }
        wifi.push(Wifi {
            ssid,
            signal,
            security,
            active,
        });
    }
    wifi.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then_with(|| b.signal.cmp(&a.signal))
            .then_with(|| a.ssid.cmp(&b.ssid))
    });
    wifi
}

fn split_escaped(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ':' => {
                fields.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    fields.push(current);
    fields
}

/// Łączy z Wi-Fi. Hasło trafia do `stdin` (jeśli sieć go wymaga).
pub fn connect_wifi(ssid: &str, password: &str) -> Result<(), String> {
    if ssid.trim().is_empty() {
        return Err(libluna_i18n::tr("Choose a network"));
    }
    let args = ["--ask", "device", "wifi", "connect", ssid];
    let direct = run_nmcli(&args, password, CONNECT_TIMEOUT)?;
    if direct.success {
        return Ok(());
    }
    if !privilege_error(&direct.message()) {
        return Err(friendly_error(&direct.message()));
    }
    // Bezpośrednia próba została odrzucona przez polkit — powtarzamy
    // z podniesionymi uprawnieniami.
    match elevated_nmcli(&args, password) {
        Ok(output) if output.success => Ok(()),
        Ok(output) => Err(friendly_error(&output.message())),
        Err(_) => Err(privilege_hint()),
    }
}

/// Włącza Wi-Fi (przydatne na live ISO, gdy radio jest wyłączone).
pub fn enable_wifi() {
    const ARGS: &[&str] = &["radio", "wifi", "on"];
    let output = run_nmcli(ARGS, "", FAST_TIMEOUT);
    let denied = match &output {
        Ok(output) => !output.success && privilege_error(&output.message()),
        Err(_) => true,
    };
    if denied {
        let _ = elevated_nmcli(ARGS, "");
    }
}

/// Czy komunikat NetworkManagera oznacza brak uprawnień (polkit).
pub fn privilege_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    const NEEDLES: &[&str] = &[
        "insufficient privileges",
        "not authorized",
        "not authorised",
        "authorization",
        "authorisation",
        "no agents",
        "authentication agent",
        "polkit",
        "permission denied",
        "notauthorized",
        "settings.modify",
        "brak uprawnień",
        "niewystarczające uprawnienia",
        "autoryzacj",
    ];
    NEEDLES.iter().any(|needle| lower.contains(needle))
}

/// Czytelny komunikat zamiast surowego wyjścia `nmcli`.
fn friendly_error(message: &str) -> String {
    let lower = message.to_lowercase();
    if privilege_error(message) {
        libluna_i18n::tr("Not enough permissions to change the network settings")
    } else if lower.contains("secrets were required")
        || lower.contains("no secrets")
        || lower.contains("802-11-wireless-security")
    {
        libluna_i18n::tr("Wrong Wi-Fi password")
    } else if lower.contains("no network with ssid") || lower.contains("not found") {
        libluna_i18n::tr("The network is out of range")
    } else if message.trim().is_empty() {
        libluna_i18n::tr("Could not connect to the network")
    } else {
        message.trim().to_owned()
    }
}

fn privilege_hint() -> String {
    libluna_i18n::tr(
        "No authorization to change network settings — start luna-polkit or run the installer as root",
    )
}

/// Wynik uruchomienia procesu.
struct Output {
    success: bool,
    stdout: String,
    stderr: String,
}

impl Output {
    fn message(&self) -> String {
        if self.stderr.trim().is_empty() {
            self.stdout.trim().to_owned()
        } else {
            self.stderr.trim().to_owned()
        }
    }
}

const SCAN_TIMEOUT: Duration = Duration::from_secs(20);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(60);
const FAST_TIMEOUT: Duration = Duration::from_secs(10);

fn run_nmcli(args: &[&str], password: &str, timeout: Duration) -> Result<Output, String> {
    let mut command = Command::new("nmcli");
    command.args(args).env("LC_ALL", "C");
    run_command(command, password, timeout)
}

/// Ponawia operację z podniesionymi uprawnieniami: `pkexec`, a potem `sudo -n`.
///
/// `sudo -n` nie pyta o hasło — działa tylko wtedy, gdy użytkownik sesji live
/// ma bezhasłowe `sudo` (typowe dla obrazów live) albo świeży znacznik sudo.
fn elevated_nmcli(args: &[&str], password: &str) -> Result<Output, String> {
    let elevators: [(&str, &[&str]); 2] = [("pkexec", &["nmcli"]), ("sudo", &["-n", "nmcli"])];
    let mut last: Option<String> = None;
    for (program, prefix) in elevators {
        if !command_available(program) {
            continue;
        }
        let mut command = Command::new(program);
        command.args(prefix).args(args).env("LC_ALL", "C");
        match run_command(command, password, CONNECT_TIMEOUT) {
            Ok(output) => {
                if output.success {
                    return Ok(output);
                }
                if privilege_error(&output.message()) || !output.stderr.trim().is_empty() {
                    last = Some(output.message());
                }
            }
            Err(error) => last = Some(error),
        }
    }
    Err(last.unwrap_or_else(privilege_hint))
}

fn run_command(mut command: Command, input: &str, timeout: Duration) -> Result<Output, String> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            let error = err.to_string();
            return Err(libluna_i18n::tr_args(
                "Could not run {program}: {error}",
                &[("program", &program), ("error", &error)],
            ));
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        if !input.is_empty() {
            let _ = stdin.write_all(input.as_bytes());
            let _ = stdin.write_all(b"\n");
            let _ = stdin.flush();
        }
    }
    let stdout = child.stdout.take().map(read_pipe);
    let stderr = child.stderr.take().map(read_pipe);
    let status = wait_timeout(&mut child, timeout)?;
    Ok(Output {
        success: status,
        stdout: join_pipe(stdout),
        stderr: join_pipe(stderr),
    })
}

fn read_pipe(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buffer = String::new();
        let _ = pipe.read_to_string(&mut buffer);
        buffer
    })
}

fn join_pipe(handle: Option<std::thread::JoinHandle<String>>) -> String {
    handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default()
}

/// Czeka na zakończenie procesu, ale nie dłużej niż `timeout` (polkit potrafi
/// zawiesić się bez agenta).
fn wait_timeout(child: &mut Child, timeout: Duration) -> Result<bool, String> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.success()),
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(libluna_i18n::tr("The network operation timed out"));
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(err) => return Err(format!("nmcli: {err}")),
        }
    }
}

pub fn command_available(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .any(|candidate| std::fs::metadata(candidate).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_route() {
        let text = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\nwlan0\t00000000\t0101A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n";
        assert_eq!(default_interface(text).as_deref(), Some("wlan0"));
    }

    #[test]
    fn parses_nmcli_with_escaped_colon() {
        let text = "*:Moja\\:Sieć:82:WPA2\n:Inna:40:--\n";
        let wifi = parse_nmcli_list(text);
        assert_eq!(wifi.len(), 2);
        assert_eq!(wifi[0].ssid, "Moja:Sieć");
        assert!(wifi[0].active);
        assert_eq!(wifi[1].signal, 40);
        assert!(wifi[1].open());
    }

    #[test]
    fn sorts_active_and_strongest_first() {
        let wifi = parse_nmcli_list(":A:10:--\n:B:90:WPA2\n*:C:20:WPA2\n");
        assert_eq!(wifi[0].ssid, "C");
        assert_eq!(wifi[1].ssid, "B");
        assert_eq!(wifi[2].ssid, "A");
    }

    #[test]
    fn detects_privilege_errors() {
        for message in [
            "Error: Insufficient privileges.",
            "Error: Failed to add/activate new connection: Not authorized to control networking.",
            "Error executing command as another user: No authentication agent found.",
            "org.freedesktop.NetworkManager.settings.modify.system",
            "Permission denied",
            "Błąd: Brak uprawnień.",
        ] {
            assert!(privilege_error(message), "{message}");
        }

        for message in [
            "Error: No network with SSID 'Dom' found.",
            "Error: Connection activation failed: Secrets were required, but not provided.",
            "",
        ] {
            assert!(!privilege_error(message), "{message}");
        }
    }

    #[test]
    fn maps_common_errors_to_readable_messages() {
        assert_eq!(
            friendly_error("Error: Insufficient privileges."),
            libluna_i18n::tr("Not enough permissions to change the network settings")
        );
        assert_eq!(
            friendly_error("Error: Secrets were required, but not provided."),
            libluna_i18n::tr("Wrong Wi-Fi password")
        );
        assert_eq!(
            friendly_error(""),
            libluna_i18n::tr("Could not connect to the network")
        );
        assert_eq!(
            friendly_error("Error: something odd"),
            "Error: something odd"
        );
    }
}
