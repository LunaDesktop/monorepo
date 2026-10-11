//! Realny stan systemu dla paska: użytkownik, dźwięk (PipeWire/`wpctl`), Wi-Fi (`nmcli`),
//! bateria (`/sys/class/power_supply`). Wartości są odpytywane w tle i podawane UI.

use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Migawka stanu systemu (to, co pasek pokazuje naprawdę).
#[derive(Debug, Clone, Default)]
pub struct SystemStatus {
    pub user: String,
    /// `(procent, wyciszone)`.
    pub volume: Option<(u8, bool)>,
    /// `(włączone, SSID gdy połączone)`.
    pub wifi: Option<(bool, Option<String>)>,
    /// `(procent, ładuje)`.
    pub battery: Option<(u8, bool)>,
    /// Jasność ekranu (procent), gdy jest podświetlenie (`/sys/class/backlight`).
    pub brightness: Option<u8>,
}

impl SystemStatus {
    /// Odczytuje bieżący stan (dzieła polecenia; wołać poza wątkiem UI).
    pub fn poll() -> Self {
        Self {
            user: user_name(),
            volume: read_volume(),
            wifi: wifi(),
            battery: battery(),
            brightness: read_brightness(),
        }
    }
}

/// Bieżąca głośność i wyciszenie domyślnego wyjścia (`(procent, wyciszone)`).
pub fn read_volume() -> Option<(u8, bool)> {
    volume()
}

/// Bieżąca jasność ekranu w procentach (z `/sys/class/backlight`), gdy dostępna.
pub fn read_brightness() -> Option<u8> {
    let dir = backlight_dir()?;
    let cur: u32 = std::fs::read_to_string(dir.join("brightness"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let max: u32 = std::fs::read_to_string(dir.join("max_brightness"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    if max == 0 {
        return None;
    }
    Some(((cur * 100 + max / 2) / max) as u8)
}

/// Ustawia jasność ekranu (procent). Bez uprawnień do sysfs cicho nic nie robi.
pub fn set_brightness(percent: u8) {
    let Some(dir) = backlight_dir() else { return };
    let Ok(text) = std::fs::read_to_string(dir.join("max_brightness")) else {
        return;
    };
    let Ok(max) = text.trim().parse::<u32>() else {
        return;
    };
    if max == 0 {
        return;
    }
    let value = (percent as u32 * max + 50) / 100;
    let _ = std::fs::write(dir.join("brightness"), format!("{value}\n"));
}

/// Katalog pierwszego podświetlenia (pierwszy wpis `/sys/class/backlight`).
fn backlight_dir() -> Option<std::path::PathBuf> {
    std::fs::read_dir("/sys/class/backlight")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .next()
}

/// Kolejny poziom jasności po kliknięciu (cykl 25 → 50 → 75 → 100 → 25).
pub fn next_brightness_level(current: u8) -> u8 {
    for level in [25u8, 50, 75, 100] {
        if current + 6 < level {
            return level;
        }
    }
    25
}

/// Współdzielony stan, aktualizowany w tle.
pub type Shared = Arc<Mutex<SystemStatus>>;

/// Startuje wątek odpytywania (co ~2 s) i zwraca współdzielony stan.
pub fn spawn() -> Shared {
    let shared: Shared = Arc::new(Mutex::new(SystemStatus::poll()));
    let writer = Arc::clone(&shared);
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        let fresh = SystemStatus::poll();
        if let Ok(mut s) = writer.lock() {
            *s = fresh;
        }
    });
    shared
}

/// Włącza/wyłącza Wi-Fi (przełącznik „Wi-Fi” w pasku).
pub fn set_wifi(on: bool) {
    spawn_cmd("nmcli", &["radio", "wifi", if on { "on" } else { "off" }]);
}

/// Przełącza wyciszenie domyślnego wyjścia audio.
pub fn toggle_mute() {
    spawn_cmd("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]);
}

fn spawn_cmd(program: &str, args: &[&str]) {
    let _ = Command::new(program).args(args).spawn();
}

fn user_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "user".to_owned())
}

/// Uruchamia polecenie (bez locale, by pola `nmcli` były angielskie) i zwraca stdout.
fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program)
        .env("LC_ALL", "C")
        .args(args)
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}

fn volume() -> Option<(u8, bool)> {
    // `wpctl get-volume` → „Volume: 0.40” lub „Volume: 0.40 [MUTED]”.
    parse_volume(&run("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"])?)
}

/// Parsuje wynik `wpctl get-volume` → `(procent, wyciszone)`.
fn parse_volume(out: &str) -> Option<(u8, bool)> {
    let muted = out.contains("MUTED");
    let value: f64 = out.split_whitespace().nth(1)?.parse().ok()?;
    Some(((value * 100.0).round() as u8, muted))
}

fn wifi() -> Option<(bool, Option<String>)> {
    // „enabled:connected” / „disabled:…”.
    let general = run("nmcli", &["-t", "-f", "WIFI,STATE", "general"])?;
    let (enabled, connected) = parse_wifi_general(&general)?;
    let ssid = if connected {
        run("nmcli", &["-t", "-f", "ACTIVE,SSID", "dev", "wifi"]).and_then(|o| parse_ssid(&o))
    } else {
        None
    };
    Some((enabled, ssid))
}

/// Parsuje `general` z `nmcli -t -f WIFI,STATE general` → `(włączone, połączone)`.
///
/// Stan bywa dłuższy („connected (local only)”), a „disconnected” zawiera
/// podciąg „connected”, więc porównujemy początek pola.
fn parse_wifi_general(out: &str) -> Option<(bool, bool)> {
    let (wifi, state) = out.trim().split_once(':')?;
    Some((wifi == "enabled", state.starts_with("connected")))
}

/// Parsuje `nmcli -t -f ACTIVE,SSID dev wifi` → SSID aktywnej sieci.
fn parse_ssid(out: &str) -> Option<String> {
    out.lines()
        .find_map(|l| l.strip_prefix("yes:").map(str::to_owned))
}

fn battery() -> Option<(u8, bool)> {
    for entry in std::fs::read_dir("/sys/class/power_supply").ok()?.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("BAT") {
            continue;
        }
        let dir = entry.path();
        let cap: u8 = std::fs::read_to_string(dir.join("capacity"))
            .ok()?
            .trim()
            .parse()
            .ok()?;
        let status = std::fs::read_to_string(dir.join("status")).unwrap_or_default();
        let charging = matches!(status.trim(), "Charging" | "Full");
        return Some((cap, charging));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_cycles_levels() {
        assert_eq!(next_brightness_level(0), 25);
        assert_eq!(next_brightness_level(25), 50);
        assert_eq!(next_brightness_level(50), 75);
        assert_eq!(next_brightness_level(75), 100);
        assert_eq!(next_brightness_level(100), 25);
    }

    #[test]
    fn parses_volume_and_mute() {
        assert_eq!(parse_volume("Volume: 0.40\n"), Some((40, false)));
        assert_eq!(parse_volume("Volume: 0.40 [MUTED]\n"), Some((40, true)));
        assert_eq!(parse_volume("Volume: 1.00\n"), Some((100, false)));
        assert_eq!(parse_volume("Nieoczekiwane\n"), None);
    }

    #[test]
    fn parses_wifi_general_and_ssid() {
        assert_eq!(parse_wifi_general("enabled:connected"), Some((true, true)));
        assert_eq!(
            parse_wifi_general("enabled:connected (local only)\n"),
            Some((true, true))
        );
        assert_eq!(
            parse_wifi_general("enabled:disconnected\n"),
            Some((true, false))
        );
        assert_eq!(
            parse_wifi_general("enabled:connecting\n"),
            Some((true, false))
        );
        assert_eq!(
            parse_wifi_general("disabled:unavailable"),
            Some((false, false))
        );
        assert_eq!(parse_wifi_general("śmieci"), None);

        let scan = "no:Dom\nyes:PaganNet\nno:Sasiad\n";
        assert_eq!(parse_ssid(scan).as_deref(), Some("PaganNet"));
        assert_eq!(parse_ssid("no:Dom\nno:Sasiad\n"), None);
    }
}
