//! Wykrywanie sesji graficznych z plików `.desktop` (XDG).
//!
//! Greeter nie zgaduje, jaki kompozytor uruchomić: czyta standardowe katalogi
//! `wayland-sessions/` i `xsessions/` z `XDG_DATA_HOME`/`XDG_DATA_DIRS`. Dzięki
//! temu działa zarówno `luna-session`, jak i zainstalowane obok GNOME, KDE,
//! sway czy labwc. Fallback (`luna-session`) jest używany tylko wtedy, gdy
//! system nie ma żadnego pliku sesji.

use std::collections::HashSet;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Typ sesji — wpływa na `XDG_SESSION_TYPE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionKind {
    Wayland,
    X11,
}

impl SessionKind {
    pub fn label(self) -> &'static str {
        match self {
            SessionKind::Wayland => "Wayland",
            SessionKind::X11 => "X11",
        }
    }

    fn env_value(self) -> &'static str {
        match self {
            SessionKind::Wayland => "wayland",
            SessionKind::X11 => "x11",
        }
    }
}

/// Sesja wyświetlana w greeterze.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// Identyfikator (nazwa pliku `.desktop` bez rozszerzenia).
    pub id: String,
    /// Nazwa dla użytkownika (z `Name[lang]`, jeśli jest).
    pub name: String,
    /// Polecenie z `Exec=` podzielone na argv.
    pub command: Vec<String>,
    /// Wartość `DesktopNames` dla `XDG_CURRENT_DESKTOP` (albo nazwa).
    pub desktop: String,
    pub kind: SessionKind,
    pub path: PathBuf,
}

impl Session {
    /// Środowisko przekazywane przez greetd przy `start_session`.
    pub fn env(&self) -> Vec<String> {
        vec![
            format!("XDG_SESSION_TYPE={}", self.kind.env_value()),
            format!("XDG_SESSION_DESKTOP={}", self.desktop),
            format!("XDG_CURRENT_DESKTOP={}", self.desktop),
        ]
    }

    /// Czy to domyślna sesja Luny.
    pub fn is_luna(&self) -> bool {
        self.command
            .iter()
            .any(|part| part.contains("luna-session"))
            || self.desktop.eq_ignore_ascii_case("Luna")
    }
}

/// Sesja awaryjna, gdy system nie ma żadnych plików `.desktop`.
pub fn fallback_session() -> Session {
    Session {
        id: "luna".to_owned(),
        name: "Luna".to_owned(),
        command: crate::greetd::session_command(),
        desktop: "Luna".to_owned(),
        kind: SessionKind::Wayland,
        path: PathBuf::new(),
    }
}

/// Dodatkowa sesja z `LUNA_GREETER_SESSION` (np. do testów własnego kompozytora).
fn manual_session() -> Option<Session> {
    let value = std::env::var("LUNA_GREETER_SESSION").ok()?;
    if value.trim().is_empty() {
        return None;
    }
    Some(Session {
        id: "custom".to_owned(),
        name: "Luna (custom)".to_owned(),
        command: crate::greetd::session_command(),
        desktop: "Luna".to_owned(),
        kind: SessionKind::Wayland,
        path: PathBuf::new(),
    })
}

/// Domyślny indeks: Luna, potem pierwsza Wayland, potem cokolwiek.
pub fn default_index(sessions: &[Session]) -> usize {
    sessions
        .iter()
        .position(Session::is_luna)
        .or_else(|| sessions.iter().position(|s| s.kind == SessionKind::Wayland))
        .unwrap_or(0)
}

/// Czyta wszystkie sesje z systemu. Wynik jest posortowany: Luna pierwsza,
/// potem Wayland, potem X11, alfabetycznie.
pub fn system_sessions() -> Vec<Session> {
    let lang = libluna_i18n::locale().language().to_owned();
    let mut seen: HashSet<(SessionKind, String)> = HashSet::new();
    let mut sessions = Vec::new();

    for base in data_dirs() {
        for (kind, subdir) in [
            (SessionKind::Wayland, "wayland-sessions"),
            (SessionKind::X11, "xsessions"),
        ] {
            let dir = base.join(subdir);
            // Tylko katalogi systemowe (root). Katalog zapisywalny przez greetera
            // mógłby podmienić polecenie sesji uruchamianej jako inny użytkownik.
            let trusted = std::fs::metadata(&dir)
                .map(|m| m.uid() == 0)
                .unwrap_or(false);
            if !trusted {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                    continue;
                }
                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_owned();
                if id.is_empty() || !seen.insert((kind, id.clone())) {
                    continue;
                }
                let Ok(content) = std::fs::read_to_string(&path) else {
                    continue;
                };
                if let Some(session) = parse_desktop(&content, &lang, kind, &id, path) {
                    sessions.push(session);
                }
            }
        }
    }

    if sessions.is_empty() {
        sessions.push(fallback_session());
    } else if let Some(custom) = manual_session() {
        sessions.push(custom);
    }
    sessions.sort_by_key(|s| {
        let rank = if s.is_luna() {
            0
        } else if s.kind == SessionKind::Wayland {
            1
        } else {
            2
        };
        (rank, s.name.to_lowercase())
    });
    sessions
}

/// Parsuje plik sesji `.desktop`. Wydzielone dla testów.
pub fn parse_desktop(
    content: &str,
    lang: &str,
    kind: SessionKind,
    id: &str,
    path: PathBuf,
) -> Option<Session> {
    let mut in_entry = false;
    let mut name = None;
    let mut name_localized = None;
    let mut exec = None;
    let mut try_exec: Option<String> = None;
    let mut desktop_names = None;
    let mut hidden = false;
    let mut nodisplay = false;
    let name_key = format!("Name[{lang}]");

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "Name" => name = Some(value.to_owned()),
            "Exec" => exec = Some(value.to_owned()),
            "TryExec" => try_exec = Some(value.to_owned()),
            "DesktopNames" => desktop_names = Some(value.to_owned()),
            "Hidden" => hidden = value.eq_ignore_ascii_case("true"),
            "NoDisplay" => nodisplay = value.eq_ignore_ascii_case("true"),
            _ if key == name_key => name_localized = Some(value.to_owned()),
            _ => {}
        }
    }
    if hidden || nodisplay {
        return None;
    }
    let name = name_localized.or(name)?;
    let mut command = split_exec(&exec?);
    if command.is_empty() {
        return None;
    }
    // Sprawdź `TryExec`, a gdy go brak — pierwszy człon `Exec`, o ile nie jest
    // pośrednikiem w stylu `env`/`sh`.
    if let Some(cmd) = try_exec.as_deref() {
        if !command_available(cmd) {
            return None;
        }
    } else if !launcher(&command[0]) && !command_available(&command[0]) {
        return None;
    }
    if !command[0].contains('/') {
        if let Some(abs) = command_path(&command[0]) {
            command[0] = abs;
        }
    }
    let desktop = desktop_names
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| s.split(';').next().map(str::trim).map(str::to_owned))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| name.clone());
    Some(Session {
        id: id.to_owned(),
        name,
        command,
        desktop,
        kind,
        path,
    })
}

/// Dzieli `Exec=` na argumenty z obsługą cudzysłowów i backslashy.
pub fn split_exec(exec: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = exec.chars().peekable();
    let mut quote: Option<char> = None;
    let mut any = false;
    while let Some(ch) = chars.next() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else if ch == '\\' && q == '"' {
                    if let Some(next) = chars.next() {
                        cur.push(next);
                    }
                } else {
                    cur.push(ch);
                }
            }
            None => match ch {
                '\'' | '"' => {
                    quote = Some(ch);
                    any = true;
                }
                '\\' => {
                    if let Some(next) = chars.next() {
                        cur.push(next);
                        any = true;
                    }
                }
                ch if ch.is_whitespace() => {
                    if any || !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                        any = false;
                    }
                }
                ch => {
                    cur.push(ch);
                    any = true;
                }
            },
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    // Kody pól (`%f`, `%U`, także w środku argumentu) w sesjach nie mają sensu.
    for arg in &mut out {
        *arg = strip_field_codes(arg);
    }
    out.retain(|arg| !arg.is_empty());
    out
}

fn strip_field_codes(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut chars = arg.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('f' | 'F' | 'u' | 'U' | 'i' | 'c' | 'k') => {}
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

fn launcher(cmd: &str) -> bool {
    matches!(
        cmd,
        "env" | "sh" | "bash" | "dash" | "flatpak" | "gio" | "dbus-run-session"
    )
}

fn command_available(cmd: &str) -> bool {
    let cmd = cmd.trim().trim_matches('"').trim_matches('\'');
    if cmd.is_empty() {
        return false;
    }
    if cmd.contains('/') {
        return Path::new(cmd).is_file();
    }
    command_path(cmd).is_some()
}

fn command_path(cmd: &str) -> Option<String> {
    if cmd.contains('/') {
        return Path::new(cmd).is_file().then(|| cmd.to_owned());
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(path) = std::env::var("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    for dir in [
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/local/sbin",
        "/usr/sbin",
        "/sbin",
    ] {
        dirs.push(PathBuf::from(dir));
    }
    dirs.into_iter()
        .map(|dir| dir.join(cmd))
        .find(|p| p.is_file())
        .map(|p| p.display().to_string())
}

fn data_dirs() -> Vec<PathBuf> {
    // Celowo **nie** czytamy `XDG_DATA_HOME`: to katalog użytkownika `greeter`,
    // więc wpis sesji umieszczony tam mógłby uruchomić dowolne polecenie po
    // zalogowaniu się ofiary. Greeter ufa tylko systemowym katalogom.
    let mut dirs = Vec::new();
    let data_dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned());
    for d in data_dirs.split(':').filter(|d| !d.is_empty()) {
        dirs.push(PathBuf::from(d));
    }
    for fallback in ["/usr/local/share", "/usr/share"] {
        let path = PathBuf::from(fallback);
        if !dirs.contains(&path) {
            dirs.push(path);
        }
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wayland_session_with_localized_name() {
        let content = "\
[Desktop Entry]
Type=Application
Name=Luna
Name[pl]=Luna
Comment=Selene
Exec=luna-session
TryExec=sh
DesktopNames=Luna
";
        let s = parse_desktop(
            content,
            "pl",
            SessionKind::Wayland,
            "selene",
            PathBuf::from("/tmp/selene.desktop"),
        )
        .expect("sesja");
        assert_eq!(s.id, "selene");
        assert_eq!(s.name, "Luna");
        assert_eq!(s.desktop, "Luna");
        assert_eq!(s.kind, SessionKind::Wayland);
        assert!(s.is_luna());
        assert_eq!(
            s.env(),
            vec![
                "XDG_SESSION_TYPE=wayland".to_owned(),
                "XDG_SESSION_DESKTOP=Luna".to_owned(),
                "XDG_CURRENT_DESKTOP=Luna".to_owned(),
            ]
        );
    }

    #[test]
    fn hidden_or_broken_sessions_are_skipped() {
        let hidden = "[Desktop Entry]\nName=Hidden\nExec=hidden\nHidden=true\n";
        assert!(
            parse_desktop(hidden, "pl", SessionKind::Wayland, "hidden", PathBuf::new()).is_none()
        );
        let no_exec = "[Desktop Entry]\nName=NoExec\n";
        assert!(parse_desktop(
            no_exec,
            "pl",
            SessionKind::Wayland,
            "noexec",
            PathBuf::new()
        )
        .is_none());
    }

    #[test]
    fn split_exec_handles_quotes_and_field_codes() {
        assert_eq!(
            split_exec("sh -lc 'echo hello world' %U"),
            vec!["sh", "-lc", "echo hello world"]
        );
        assert_eq!(split_exec("app --file=%f %%"), vec!["app", "--file=", "%"]);
        assert_eq!(
            split_exec("\"/opt/My App/bin/app\" --flag"),
            vec!["/opt/My App/bin/app", "--flag"]
        );
    }

    #[test]
    fn desktop_names_use_first_entry_for_session_env() {
        let content = "[Desktop Entry]\nName=GNOME\nExec=sh\nDesktopNames=GNOME;Unity;\n";
        let s =
            parse_desktop(content, "pl", SessionKind::X11, "gnome", PathBuf::new()).expect("sesja");
        assert_eq!(s.desktop, "GNOME");
        assert!(s.env().contains(&"XDG_SESSION_TYPE=x11".to_owned()));
        assert!(s.env().contains(&"XDG_CURRENT_DESKTOP=GNOME".to_owned()));
    }

    #[test]
    fn luna_is_the_default_when_present() {
        let sessions = vec![
            Session {
                id: "sway".into(),
                name: "Sway".into(),
                command: vec!["sh".into()],
                desktop: "sway".into(),
                kind: SessionKind::Wayland,
                path: PathBuf::new(),
            },
            Session {
                id: "selene".into(),
                name: "Luna".into(),
                command: vec!["luna-session".into()],
                desktop: "Luna".into(),
                kind: SessionKind::Wayland,
                path: PathBuf::new(),
            },
        ];
        assert_eq!(default_index(&sessions), 1);
    }
}
