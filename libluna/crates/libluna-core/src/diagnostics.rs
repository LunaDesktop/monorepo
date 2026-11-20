//! Diagnostyka do zgłaszania błędów — zbiór danych **po filtrze prywatności**.
//!
//! Raport jest budowany wyłącznie z allowlisty: wersje, sprzęt, ogon logu sesji
//! i kilka plików konfiguracyjnych. Zanim cokolwiek opuści komputer, tekst jest
//! redagowany: katalog domowy, nazwa użytkownika, host, adresy IP/MAC, e-maile,
//! URL-e i tokeny trafiają do `<...>`. Podgląd w Ustawieniach pokazuje dokładnie
//! ten sam tekst, który zostałby zapisany lub wysłany.
//!
//! Nic nie jest wysyłane automatycznie: [`Report::save`] zapisuje plik lokalnie,
//! a [`Report::mailto`] tylko buduje link do klienta poczty użytkownika.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{config_file, parse_pairs, save_atomic};

/// Adres, na który trafiają raporty (kanał zapasowy — w aplikacji jest przycisk
/// otwierający klienta poczty; docelowo także upload na serwer).
pub const REPORT_EMAIL: &str = "luna@mijsys.pl";
/// Prefiks tematu, żeby łatwo filtrować zgłoszenia.
pub const REPORT_SUBJECT_PREFIX: &str = "[luna-report]";

/// Ile bajtów końcówki logu sesji czytamy (plik bywa dziesiątki MB).
const SESSION_TAIL_BYTES: u64 = 1_500_000;
/// Maksymalna liczba linii z jednej sesji w raporcie.
const SESSION_MAX_LINES: usize = 800;
/// Plik konfiguracyjny większy niż to zostanie pominięty.
const CONFIG_MAX_BYTES: u64 = 64 * 1024;

/// Sekcja raportu — użytkownik może ją wyłączyć przed wysłaniem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SectionKind {
    /// Wersje, jądro, CPU/GPU, monitory, zmienne sesji.
    System,
    /// Ogon `~/.local/state/luna/session.log` (ostatnia sesja).
    SessionLog,
    /// Wybrane pliki `~/.config/luna/*.conf` (bez schowka i sekretów).
    Configs,
}

impl SectionKind {
    pub const ALL: [SectionKind; 3] = [
        SectionKind::System,
        SectionKind::SessionLog,
        SectionKind::Configs,
    ];

    /// Klucz tłumaczenia tytułu sekcji.
    pub fn title_key(self) -> &'static str {
        match self {
            Self::System => "System and hardware",
            Self::SessionLog => "Session log (last run)",
            Self::Configs => "Luna configuration",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::SessionLog => "session",
            Self::Configs => "configs",
        }
    }

    /// Klucz tłumaczenia krótkiego opisu sekcji.
    pub fn hint_key(self) -> &'static str {
        match self {
            Self::System => "Versions, CPU, GPU, monitors and session variables",
            Self::SessionLog => "Last run of the session (paths and identifiers redacted)",
            Self::Configs => "Selected Luna config files (never clipboard or keyrings)",
        }
    }
}

/// Jedna sekcja raportu (już po redakcji).
#[derive(Debug, Clone)]
pub struct Section {
    pub kind: SectionKind,
    pub text: String,
}

/// Gotowy raport: sekcje + czas wygenerowania.
#[derive(Debug, Clone)]
pub struct Report {
    pub sections: Vec<Section>,
    /// Czas wygenerowania (sekundy od epoki, UTC).
    pub generated: u64,
}

impl Report {
    /// Zbiera wszystkie sekcje i od razu je redaguje.
    pub fn collect() -> Self {
        let redactor = Redactor::detect();
        let mut sections = Vec::with_capacity(SectionKind::ALL.len());
        for kind in SectionKind::ALL {
            let raw = match kind {
                SectionKind::System => system_text(),
                SectionKind::SessionLog => session_log_text(),
                SectionKind::Configs => configs_text(),
            };
            sections.push(Section {
                kind,
                text: redactor.text(&raw),
            });
        }
        Self {
            sections,
            generated: now_secs(),
        }
    }

    /// Treść raportu z wybranymi sekcjami (`include` w kolejności [`SectionKind::ALL`]).
    pub fn preview(&self, include: &[bool]) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "Luna — raport diagnostyczny");
        let _ = writeln!(out, "Wygenerowano: {} UTC", utc_stamp(self.generated));
        let _ = writeln!(out, "libluna-core: {}", env!("CARGO_PKG_VERSION"));
        let _ = writeln!(out, "Kontakt: {REPORT_EMAIL}");
        let _ = writeln!(
            out,
            "Dane po filtrze: katalog domowy, użytkownik, host, IP/MAC, e-maile,\nURL-e i tokeny są zastąpione przez <...>."
        );
        for (index, section) in self.sections.iter().enumerate() {
            let on = include.get(index).copied().unwrap_or(true);
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "===== {} {} =====",
                section.kind.key(),
                if on { "" } else { "(pominięta)" }
            );
            if on {
                out.push_str(section.text.trim_end());
                out.push('\n');
            }
        }
        out
    }

    /// Zapisuje raport do `~/.local/state/luna/reports/` i zwraca ścieżkę.
    pub fn save(&self, include: &[bool]) -> io::Result<PathBuf> {
        let dir =
            reports_dir().ok_or_else(|| io::Error::other("brak katalogu stanu użytkownika"))?;
        fs::create_dir_all(&dir)?;
        let name = format!("luna-report-{}.txt", file_stamp(self.generated));
        let path = dir.join(name);
        fs::write(&path, self.preview(include))?;
        Ok(path)
    }

    /// Przykładowy raport do dokumentacji i podglądów (bez danych użytkownika).
    #[doc(hidden)]
    pub fn sample() -> Self {
        let demo = [
            (
                SectionKind::System,
                "System: Pagan Linux\nKernel: 6.12.4\nCPU: Example CPU @ 3.60GHz\nRdzenie: 8\nRAM: 31.2 GiB\n\nGPU:\n  card0: driver=amdgpu pci=1002:73ff\n\nMonitory:\n  card0-DP-1: 2560x1440 (enabled)",
            ),
            (
                SectionKind::SessionLog,
                "=== luna-session 2026-11-19T21:24:05+01:00 ===\nXDG_RUNTIME_DIR=<runtime> SELENE_BACKEND=drm SELENE_INPUT=evdev\nDetected outputs: DP-1 2560x1440\nSelene compositor started (wayland-0)\nluna-shell: panel.conf → look=Luna, tray=Icons",
            ),
            (
                SectionKind::Configs,
                "--- panel.conf (82 B) ---\nposition = top\nlook = luna\ntray = icons",
            ),
        ];
        Self {
            sections: demo
                .into_iter()
                .map(|(kind, text)| Section {
                    kind,
                    text: text.to_owned(),
                })
                .collect(),
            generated: 1_763_600_000,
        }
    }

    /// Buduje `mailto:` z tematem i krótkim opisem; raport dołącza użytkownik.
    pub fn mailto(&self, include: &[bool], note: &str, attachment: &Path) -> String {
        let mut subject = format!("{REPORT_SUBJECT_PREFIX} Luna {}", env!("CARGO_PKG_VERSION"));
        let included: Vec<&str> = self
            .sections
            .iter()
            .enumerate()
            .filter(|(index, _)| include.get(*index).copied().unwrap_or(true))
            .map(|(_, section)| section.kind.key())
            .collect();
        if !included.is_empty() {
            let _ = write!(subject, " ({})", included.join("+"));
        }
        let mut body = String::new();
        let desc = note.trim();
        if !desc.is_empty() {
            let _ = writeln!(body, "Opis problemu:\n{desc}\n");
        }
        let _ = writeln!(
            body,
            "Raport został zapisany tutaj:\n  {}\n\nDołącz go proszę jako załącznik (tekst, ~{} KB).",
            attachment.display(),
            self.preview(include).len() / 1024
        );
        let _ = writeln!(body, "\nSekcje: {}", included.join(", "));
        format!(
            "mailto:{REPORT_EMAIL}?subject={}&body={}",
            percent_encode(&subject),
            percent_encode(&body)
        )
    }
}

// ── Konfiguracja zgody (opt-in) ──────────────────────────────────────────────

fn reporting_path() -> Option<PathBuf> {
    config_file("reporting.conf")
}

/// Czy użytkownik włączył zbieranie diagnostyki (domyślnie **nie**).
pub fn enabled() -> bool {
    reporting_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|text| {
            parse_pairs(&text).into_iter().any(|(key, value)| {
                key == "enabled" && matches!(value.as_str(), "1" | "true" | "tak")
            })
        })
        .unwrap_or(false)
}

/// Zapisuje zgodę (albo jej brak).
pub fn set_enabled(on: bool) -> io::Result<()> {
    save_flag("enabled", on)
}

/// Czy karta w `luna-welcome` została zamknięta („Nie teraz”).
pub fn dismissed() -> bool {
    reporting_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|text| {
            parse_pairs(&text).into_iter().any(|(key, value)| {
                key == "dismissed" && matches!(value.as_str(), "1" | "true" | "tak")
            })
        })
        .unwrap_or(false)
}

/// Zapamiętuje zamknięcie karty w powitaniu.
pub fn set_dismissed(on: bool) -> io::Result<()> {
    save_flag("dismissed", on)
}

fn save_flag(key: &str, on: bool) -> io::Result<()> {
    let path = reporting_path().ok_or_else(|| io::Error::other("brak katalogu konfiguracji"))?;
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = parse_pairs(&existing)
        .into_iter()
        .map(|(k, v)| format!("{k} = {v}"))
        .collect();
    lines.retain(|line| {
        !line.starts_with(&format!("{key} ")) && !line.starts_with(&format!("{key}="))
    });
    lines.push(format!("{key} = {}", u8::from(on)));
    let mut text = String::from("# Luna — diagnostyka i zgłaszanie problemów\n");
    for line in lines {
        text.push_str(&line);
        text.push('\n');
    }
    save_atomic(&path, &text)
}

// ── Kolektory ────────────────────────────────────────────────────────────────

fn system_text() -> String {
    let mut out = String::new();
    let _ = writeln!(out, "System: {}", os_pretty_name());
    let _ = writeln!(out, "Kernel: {}", read_trim("/proc/sys/kernel/osrelease"));
    let _ = writeln!(out, "Architektura: {}", std::env::consts::ARCH);
    let _ = writeln!(out, "CPU: {}", cpu_model());
    let _ = writeln!(out, "Rdzenie: {}", cpu_count());
    let _ = writeln!(out, "RAM: {}", memory_total());
    let _ = writeln!(out, "Uptime: {}", uptime());
    out.push('\n');
    out.push_str("GPU:\n");
    for line in gpu_lines() {
        let _ = writeln!(out, "  {line}");
    }
    out.push_str("Monitory:\n");
    for line in connector_lines() {
        let _ = writeln!(out, "  {line}");
    }
    out.push('\n');
    out.push_str("Środowisko sesji:\n");
    for key in [
        "XDG_SESSION_TYPE",
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_DESKTOP",
        "WAYLAND_DISPLAY",
        "LANG",
        "LANGUAGE",
        "LC_TIME",
        "TZ",
        "SELENE_BACKEND",
        "SELENE_INPUT",
        "SELENE_GPU",
        "LUNA_GPU",
        "SHELL",
    ] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                let _ = writeln!(out, "  {key}={value}");
            }
        }
    }
    out
}

fn os_pretty_name() -> String {
    fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("PRETTY_NAME=").map(str::to_owned))
                .map(|value| value.trim_matches('"').to_owned())
        })
        .unwrap_or_else(|| "Linux".to_owned())
}

fn read_trim(path: &str) -> String {
    fs::read_to_string(path)
        .map(|text| text.trim().to_owned())
        .unwrap_or_else(|_| "?".to_owned())
}

fn cpu_model() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("model name")
                    .and_then(|rest| rest.split_once(':').map(|(_, v)| v.trim().to_owned()))
            })
        })
        .unwrap_or_else(|| "?".to_owned())
}

fn cpu_count() -> usize {
    fs::read_to_string("/proc/cpuinfo")
        .map(|text| {
            text.lines()
                .filter(|line| line.starts_with("processor"))
                .count()
        })
        .unwrap_or(0)
}

fn memory_total() -> String {
    let kib = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("MemTotal:"))
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
        })
        .unwrap_or(0);
    format!("{:.1} GiB", kib as f64 / 1024.0 / 1024.0)
}

fn uptime() -> String {
    let secs = fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|text| text.split_whitespace().next()?.parse::<u64>().ok())
        .unwrap_or(0);
    format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60)
}

fn gpu_lines() -> Vec<String> {
    let mut lines = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return lines;
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| is_card(name))
        .collect();
    names.sort();
    for name in names {
        let uevent =
            fs::read_to_string(format!("/sys/class/drm/{name}/device/uevent")).unwrap_or_default();
        let driver = uevent
            .lines()
            .find_map(|line| line.strip_prefix("DRIVER="))
            .unwrap_or("?");
        let pci = uevent
            .lines()
            .find_map(|line| line.strip_prefix("PCI_ID="))
            .unwrap_or("?");
        lines.push(format!("{name}: driver={driver} pci={pci}"));
    }
    lines
}

fn is_card(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("card") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

fn connector_lines() -> Vec<String> {
    let mut lines = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return lines;
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("card") && name.contains('-'))
        .collect();
    names.sort();
    for name in names {
        let base = format!("/sys/class/drm/{name}");
        let status = read_trim(&format!("{base}/status"));
        if status != "connected" {
            continue;
        }
        let mode = fs::read_to_string(format!("{base}/modes"))
            .ok()
            .and_then(|text| text.lines().next().map(str::to_owned))
            .unwrap_or_default();
        let enabled = read_trim(&format!("{base}/enabled"));
        lines.push(format!("{name}: {mode} ({enabled})"));
    }
    lines
}

fn session_log_text() -> String {
    let Some(path) = session_log_path() else {
        return "Brak ~/.local/state/luna/session.log".to_owned();
    };
    let Ok(text) = read_tail(&path, SESSION_TAIL_BYTES) else {
        return format!("Nie udało się odczytać {}", path.display());
    };
    let session = last_session(&text);
    if session.trim().is_empty() {
        return "Log sesji jest pusty".to_owned();
    }
    limit_lines(session, SESSION_MAX_LINES)
}

fn session_log_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(base.join("luna").join("session.log"))
}

fn read_tail(path: &Path, bytes: u64) -> io::Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    if len > bytes {
        file.seek(SeekFrom::Start(len - bytes))?;
    }
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    Ok(String::from_utf8_lossy(&data).into_owned())
}

/// Zwraca fragment od ostatniego nagłówka `=== luna-session`.
fn last_session(text: &str) -> &str {
    const MARKER: &str = "=== luna-session";
    match text.rfind(MARKER) {
        Some(index) => &text[index..],
        None => text,
    }
}

/// Ogranicza liczbę linii, zachowując nagłówek sesji i jej końcówkę.
fn limit_lines(text: &str, max: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max {
        return text.to_owned();
    }
    let head = 16.min(lines.len());
    let tail = max.saturating_sub(head + 1);
    let mut out = String::new();
    for line in &lines[..head] {
        out.push_str(line);
        out.push('\n');
    }
    let _ = writeln!(out, "… (pominięto {} linii) …", lines.len() - head - tail);
    for line in &lines[lines.len() - tail..] {
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn configs_text() -> String {
    // Allowlista: bez `clipboard.conf` (historia schowka) i bez keyringu.
    const FILES: [&str; 11] = [
        "theme.conf",
        "panel.conf",
        "locale.conf",
        "region.conf",
        "accessibility.conf",
        "input.conf",
        "notifications.conf",
        "workspaces.conf",
        "desktop.conf",
        "lock.conf",
        "icons.conf",
    ];
    let mut out = String::new();
    for name in FILES {
        let Some(path) = config_file(name) else {
            continue;
        };
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        let _ = writeln!(out, "--- {name} ({} B) ---", meta.len());
        if meta.len() > CONFIG_MAX_BYTES {
            let _ = writeln!(out, "(plik za duży — pominięty)");
            continue;
        }
        match fs::read_to_string(&path) {
            Ok(text) => {
                out.push_str(text.trim_end());
                out.push('\n');
            }
            Err(err) => {
                let _ = writeln!(out, "(nie udało się odczytać: {err})");
            }
        }
    }
    if out.trim().is_empty() {
        out.push_str("Brak plików konfiguracyjnych z allowlisty.\n");
    }
    out
}

fn reports_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(base.join("luna").join("reports"))
}

// ── Redakcja danych prywatnych ───────────────────────────────────────────────

/// Zamienia dane prywatne na znaczniki `<...>`.
struct Redactor {
    user: String,
    host: String,
    home: String,
    runtime: String,
}

impl Redactor {
    fn detect() -> Self {
        let user = std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .unwrap_or_default();
        let host = read_trim("/proc/sys/kernel/hostname");
        let home = std::env::var("HOME").unwrap_or_default();
        let uid = home_uid().map(|uid| uid.to_string()).unwrap_or_default();
        Self::new(user, host, home, uid)
    }

    fn new(user: String, host: String, home: String, uid: String) -> Self {
        Self {
            runtime: if uid.is_empty() {
                String::new()
            } else {
                format!("/run/user/{uid}")
            },
            user,
            host,
            home: home.trim_end_matches('/').to_owned(),
        }
    }

    fn text(&self, input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        for line in input.lines() {
            out.push_str(&self.line(line));
            out.push('\n');
        }
        out
    }

    fn line(&self, line: &str) -> String {
        let replaced = self.replace_known(line);
        let mut out = String::with_capacity(replaced.len());
        let mut token = String::new();
        let mut redact_next = false;
        for ch in replaced.chars() {
            if ch.is_whitespace() {
                if !token.is_empty() {
                    out.push_str(&self.token(&token, redact_next));
                    token.clear();
                    redact_next = false;
                }
                out.push(ch);
            } else {
                if redact_next {
                    out.push_str("<token>");
                    redact_next = false;
                    token.clear();
                    continue;
                }
                token.push(ch);
            }
        }
        if !token.is_empty() {
            out.push_str(&self.token(&token, redact_next));
        }
        out
    }

    /// Zamienia znane wartości (dom, runtime, użytkownik, host) niezależnie od tokenów.
    fn replace_known(&self, line: &str) -> String {
        let mut out = line.to_owned();
        if !self.home.is_empty() {
            out = out.replace(&self.home, "~");
        }
        if !self.runtime.is_empty() {
            out = out.replace(&self.runtime, "<runtime>");
        }
        if self.host.len() > 1 {
            out = out.replace(&self.host, "<host>");
        }
        if self.user.len() > 1 {
            out = out.replace(&self.user, "<user>");
        }
        out
    }

    fn token(&self, token: &str, redact_next: bool) -> String {
        if redact_next {
            return "<token>".to_owned();
        }
        if token.starts_with("http://") || token.starts_with("https://") {
            return "<url>".to_owned();
        }
        if is_email(token) {
            return "<email>".to_owned();
        }
        if is_mac(token) {
            return "<mac>".to_owned();
        }
        if is_ipv4(token) || is_ipv6(token) {
            return "<ip>".to_owned();
        }
        if is_secret(token) {
            return "<token>".to_owned();
        }
        // `klucz=wartość` — sprawdzamy wartość tymi samymi regułami.
        if let Some((key, value)) = token.split_once('=') {
            let redacted = self.token(value, false);
            if redacted != value {
                return format!("{key}={redacted}");
            }
        }
        let lower = token.to_ascii_lowercase();
        for key in ["password=", "passwd=", "token=", "secret=", "apikey="] {
            if let Some(start) = lower.find(key) {
                let cut = start + key.len();
                return format!("{}<redacted>", &token[..cut]);
            }
        }
        token.to_owned()
    }
}

fn home_uid() -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        fs::metadata("/proc/self").ok().map(|meta| meta.uid())
    }
    #[cfg(not(unix))]
    {
        None
    }
}

fn is_email(token: &str) -> bool {
    let Some((local, domain)) = token.split_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
}

fn is_mac(token: &str) -> bool {
    let sep = if token.contains(':') { ':' } else { '-' };
    let parts: Vec<&str> = token.split(sep).collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit()))
}

fn is_ipv4(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 3
                && part.chars().all(|c| c.is_ascii_digit())
                && part.parse::<u16>().map(|v| v <= 255).unwrap_or(false)
        })
}

fn is_ipv6(token: &str) -> bool {
    if !token.contains(':') {
        return false;
    }
    let groups = token.split(':').count();
    // Godziny (`12:30:05`) mają 3 grupy — wymagamy IPv6-podobnej struktury.
    (token.contains("::") || groups >= 6)
        && token
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
}

fn is_secret(token: &str) -> bool {
    const PREFIXES: [&str; 12] = [
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "xoxa-",
        "xoxr-",
        "AKIA",
        "ASIA",
    ];
    if PREFIXES.iter().any(|prefix| token.starts_with(prefix)) {
        return true;
    }
    token.starts_with("sk-") && token.len() > 24
}

// ── Czas i kodowanie ────────────────────────────────────────────────────────

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `YYYY-MM-DD HH:MM:SS` w UTC (bez zależności i bez strefy lokalnej).
fn utc_stamp(secs: u64) -> String {
    let (year, month, day, hour, minute, second) = utc_parts(secs);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
}

fn file_stamp(secs: u64) -> String {
    let (year, month, day, hour, minute, second) = utc_parts(secs);
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// Data cywilna z sekund od epoki (algorytm Hinnanta).
fn utc_parts(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = if m <= 2 { y + 1 } else { y };
    (
        year,
        m,
        d,
        (rem / 3600) as u32,
        ((rem % 3600) / 60) as u32,
        (rem % 60) as u32,
    )
}

/// Kodowanie do `mailto:` (tylko znaki niereszerowane zostają bez zmian).
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char)
            }
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redactor() -> Redactor {
        Redactor::new(
            "patryks".to_owned(),
            "tobefilledbyoem".to_owned(),
            "/home/patryks".to_owned(),
            "1001".to_owned(),
        )
    }

    #[test]
    fn redacts_paths_user_and_host() {
        let r = redactor();
        let out =
            r.text("/home/patryks/luna/src/main.rs patryks@tobefilledbyoem /run/user/1001/bus");
        assert!(out.contains("~/luna/src/main.rs"), "{out}");
        assert!(!out.contains("/home/patryks"));
        assert!(!out.contains("patryks"));
        assert!(!out.contains("tobefilledbyoem"));
        assert!(out.contains("<runtime>/bus"), "{out}");
    }

    #[test]
    fn redacts_network_identifiers() {
        let r = redactor();
        let out = r.text("ip=192.168.1.20 mac=aa:bb:cc:dd:ee:ff v6=2001:db8::1");
        assert!(out.contains("ip=<ip>"));
        assert!(out.contains("mac=<mac>"));
        assert!(out.contains("v6=<ip>"));
    }

    #[test]
    fn keeps_clock_times_untouched() {
        let r = redactor();
        let out = r.text("21:24:05 luna-session: start");
        assert!(out.contains("21:24:05"), "{out}");
    }

    #[test]
    fn redacts_urls_emails_and_tokens() {
        let r = redactor();
        let out = r.text(
            "see https://example.com/a?b=c mail: ktos@example.com token ghp_abcdefghijklmnopqrstuvwxyz123456",
        );
        assert!(out.contains("<url>"));
        assert!(out.contains("<email>"));
        assert!(out.contains("<token>"));
        assert!(!out.contains("ghp_"));
        assert!(!out.contains("example.com"));
    }

    #[test]
    fn redacts_values_of_secret_keys() {
        let r = redactor();
        let out = r.text("token=abcdef123 password=hunter2 apikey=zzz");
        assert!(out.contains("token=<redacted>"), "{out}");
        assert!(out.contains("password=<redacted>"), "{out}");
        assert!(out.contains("apikey=<redacted>"), "{out}");
        assert!(!out.contains("hunter2"));
    }

    #[test]
    fn utc_date_is_correct() {
        assert_eq!(utc_stamp(0), "1970-01-01 00:00:00");
        assert_eq!(utc_stamp(1_700_000_000), "2023-11-14 22:13:20");
    }

    #[test]
    fn percent_encoding_escapes_mailto_reserved_chars() {
        assert_eq!(percent_encode("a b\nc"), "a%20b%0Ac");
        assert_eq!(percent_encode("luna@x.pl"), "luna%40x.pl");
    }

    #[test]
    fn last_session_keeps_only_current_run() {
        let text = "=== luna-session stara\nx\n=== luna-session nowa\ny\n";
        let tail = last_session(text);
        assert!(tail.starts_with("=== luna-session nowa"));
        assert!(!tail.contains('x'));
    }

    #[test]
    fn long_logs_keep_header_and_tail() {
        let mut text = String::from("=== luna-session 2026\n");
        for i in 0..2000 {
            text.push_str(&format!("linia {i}\n"));
        }
        let out = limit_lines(&text, 100);
        assert!(out.starts_with("=== luna-session"));
        assert!(out.contains("pominięto"));
        assert!(out.contains("linia 1999"));
        assert!(!out.contains("linia 500\n"));
    }

    #[test]
    fn mailto_contains_subject_and_attachment_path() {
        let report = Report {
            sections: SectionKind::ALL
                .iter()
                .map(|kind| Section {
                    kind: *kind,
                    text: "x".to_owned(),
                })
                .collect(),
            generated: 1_700_000_000,
        };
        let link = report.mailto(
            &[true, false, true],
            "Zawiesza się panel",
            Path::new("/tmp/r.txt"),
        );
        assert!(link.starts_with("mailto:luna@mijsys.pl?subject="));
        assert!(link.contains("%5Bluna-report%5D"));
        assert!(link.contains("Zawiesza"));
        assert!(link.contains("%2Ftmp%2Fr.txt"));
        assert!(!link.contains("session+"));
    }
}
