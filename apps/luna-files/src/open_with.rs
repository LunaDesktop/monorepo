//! „Otwórz za pomocą…” — wpisy `.desktop`, typy MIME i uruchamianie aplikacji.
//!
//! Zgodnie z resztą repozytorium nie linkujemy bibliotek: typ pliku ustala
//! `xdg-mime`, wpisy aplikacji czytamy wprost z katalogów `applications`
//! (`$XDG_DATA_HOME` + `$XDG_DATA_DIRS`), a `Exec=` rozwijamy sami. Do zapisu
//! skojarzenia używamy `xdg-mime default`, więc wybór trafia do `mimeapps.list`
//! dokładnie tam, gdzie zaglądają inne aplikacje i portale.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use libluna_core::{Point, Rect};
use libluna_widgets::ScrollView;

/// Wysokość wiersza listy aplikacji (wspólna dla rysowania i trafień).
pub const ROW_H: f64 = 36.0;

/// Ile wierszy mieści się bez przewijania.
pub const MAX_VISIBLE_ROWS: usize = 6;

/// Wpis `.desktop` potrzebny do otwarcia pliku.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopApp {
    /// Identyfikator freedesktop, np. `org.gnome.gedit.desktop`.
    pub desktop_id: String,
    /// Ścieżka wpisu (do `gio launch`, gdy własne `Exec` zawiedzie).
    pub desktop_path: PathBuf,
    /// Nazwy: klucz `""` = `Name`, pozostałe = `Name[xx]`.
    names: BTreeMap<String, String>,
    pub icon: Option<String>,
    pub exec: String,
    /// Katalog roboczy z `Path=` (jeśli podany).
    pub work_dir: Option<PathBuf>,
    pub categories: Vec<String>,
    pub mime_types: Vec<String>,
    pub terminal: bool,
    pub try_exec: Option<String>,
}

impl DesktopApp {
    /// Nazwa w wybranym języku, z fallbackiem na `Name` bazowe.
    pub fn display_name(&self, lang: &str) -> &str {
        self.names
            .get(lang)
            .or_else(|| self.names.get(""))
            .map(String::as_str)
            .unwrap_or_default()
    }

    /// Czy da się ją uruchomić (istnieje `TryExec` i program z `Exec`).
    pub fn available(&self) -> bool {
        if let Some(try_exec) = self.try_exec.as_deref() {
            if !command_exists(try_exec) {
                return false;
            }
        }
        exec_program(&self.exec).is_some_and(|program| command_exists(&program))
    }

    /// Czy obsługuje podany typ MIME.
    pub fn handles(&self, mime: &str) -> bool {
        self.mime_types.iter().any(|have| have == mime)
    }
}

/// Wynik wyszukania kandydatów do otwarcia pliku.
pub struct Candidates {
    pub mime: Option<String>,
    pub default_id: Option<String>,
    pub apps: Vec<DesktopApp>,
}

/// Uruchamia polecenie i zwraca jego wyjście (albo `None` przy błędzie).
fn run_capture(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Typ MIME pliku według `xdg-mime` (`None`, gdy narzędzia brak lub plik nie istnieje).
pub fn mime_type(path: &Path) -> Option<String> {
    let file = path.display().to_string();
    let mime = run_capture("xdg-mime", &["query", "filetype", &file])?;
    if mime.is_empty() {
        None
    } else {
        Some(mime)
    }
}

/// Typ MIME wyłącznie po rozszerzeniu — zapas, gdy `xdg-mime` nie pomoże.
pub fn mime_by_extension(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let mime = match ext.as_str() {
        "txt" | "md" | "markdown" | "log" => "text/plain",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "csv" => "text/csv",
        "json" => "application/json",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "heic" => "image/heic",
        "avif" => "image/avif",
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "ogg" | "oga" => "audio/ogg",
        "wav" => "audio/x-wav",
        "m4a" => "audio/mp4",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" | "tgz" => "application/gzip",
        "xz" => "application/x-xz",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        "deb" => "application/vnd.debian.binary-package",
        "rpm" => "application/x-rpm",
        "epub" => "application/epub+zip",
        "c" | "h" => "text/x-csrc",
        "rs" => "text/rust",
        "py" => "text/x-python",
        "js" | "mjs" => "text/javascript",
        "sh" => "application/x-shellscript",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/yaml",
        "desktop" => "application/x-desktop",
        "dng" | "cr2" | "cr3" | "nef" | "arw" | "orf" | "raf" | "rw2" | "pef" | "srw" | "raw" => {
            "image/x-dcraw"
        }
        _ => return None,
    };
    Some(mime)
}

/// Czy typ MIME oznacza „nie wiadomo co” (a więc plik nierozpoznany).
pub fn is_unknown_mime(mime: &str) -> bool {
    mime.is_empty() || mime == "application/octet-stream"
}

/// Typ MIME pliku: najpierw `xdg-mime`, potem rozszerzenie.
pub fn file_mime(path: &Path) -> Option<String> {
    let detected = mime_type(path);
    match detected {
        Some(mime) if !is_unknown_mime(&mime) => Some(mime),
        Some(mime) => mime_by_extension(path).map(str::to_owned).or(Some(mime)),
        None => mime_by_extension(path).map(str::to_owned),
    }
}

/// Aplikacja domyślna dla typu MIME (`xdg-mime query default`).
pub fn default_app_id(mime: &str) -> Option<String> {
    if mime.is_empty() {
        return None;
    }
    let id = run_capture("xdg-mime", &["query", "default", mime])?;
    if id.is_empty() || id == "none" {
        None
    } else {
        Some(id)
    }
}

/// Ustawia aplikację domyślną dla typu MIME (`xdg-mime default`).
pub fn set_default_app(mime: &str, desktop_id: &str) -> Result<(), String> {
    let output = Command::new("xdg-mime")
        .args(["default", desktop_id, mime])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .map_err(|err| format!("xdg-mime: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("xdg-mime: {}", output.status))
    }
}

/// Otwiera plik domyślną aplikacją systemową (`xdg-open`).
pub fn open_default(path: &Path) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("xdg-open: {err}"))
}

/// Czy plik ma rozpoznany typ i **działającą** domyślną aplikację.
///
/// `None` = trzeba pokazać „Otwórz za pomocą…” (nieznany typ, brak skojarzenia
/// albo wpis `.desktop` wskazuje na nieistniejący program).
pub fn recognized_default(path: &Path) -> Option<String> {
    let mime = file_mime(path)?;
    if is_unknown_mime(&mime) {
        return None;
    }
    let id = default_app_id(&mime)?;
    // Gdy wpis znamy i wiemy, że nie da się go uruchomić — pokaż wybór aplikacji.
    // Nieznany nam wpis zostawiamy `xdg-open` (np. eksport Flatpaka).
    if let Some(app) = scan_apps().into_iter().find(|app| app.desktop_id == id) {
        if !app.available() {
            return None;
        }
    }
    Some(id)
}

/// Katalogi z wpisami `.desktop` w kolejności priorytetu.
fn desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    if let Some(home) = data_home {
        dirs.push(home.join("applications"));
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    for base in data_dirs.split(':').filter(|base| !base.is_empty()) {
        dirs.push(PathBuf::from(base).join("applications"));
    }
    dirs
}

/// Wczytuje wpisy `.desktop` (bez duplikatów — pierwszy katalog wygrywa).
fn scan_apps() -> Vec<DesktopApp> {
    let mut seen: BTreeMap<String, DesktopApp> = BTreeMap::new();
    for dir in desktop_dirs() {
        collect_desktop_files(&dir, &dir, &mut seen, 0);
    }
    let mut apps: Vec<DesktopApp> = seen.into_values().collect();
    apps.sort_by(|a, b| {
        a.display_name("")
            .to_lowercase()
            .cmp(&b.display_name("").to_lowercase())
            .then_with(|| a.desktop_id.cmp(&b.desktop_id))
    });
    apps
}

fn collect_desktop_files(
    base: &Path,
    dir: &Path,
    out: &mut BTreeMap<String, DesktopApp>,
    depth: usize,
) {
    if depth > 2 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_desktop_files(base, &path, out, depth + 1);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
            continue;
        }
        let Some(id) = desktop_id(base, &path) else {
            continue;
        };
        if out.contains_key(&id) {
            continue;
        }
        if let Some(app) = parse_desktop_file(&path, &id) {
            out.insert(id, app);
        }
    }
}

/// `applications/foo/bar.desktop` → `foo-bar.desktop` (konwencja freedesktop).
fn desktop_id(base: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(base).ok()?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let text = component.as_os_str().to_str()?;
        parts.push(text.to_owned());
    }
    Some(parts.join("-"))
}

/// Parsuje wpis `.desktop`; `None` dla niepoprawnych/ukrytych/nie-aplikacji.
pub fn parse_desktop_file(path: &Path, id: &str) -> Option<DesktopApp> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut app = parse_desktop_entry(&text, id)?;
    app.desktop_path = path.to_path_buf();
    Some(app)
}

/// Czysty parser wpisu (testowalny bez systemu plików).
pub fn parse_desktop_entry(text: &str, id: &str) -> Option<DesktopApp> {
    let mut in_entry = false;
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut exec = None;
    let mut kind = None;
    let mut hidden = false;
    let mut icon = None;
    let mut work_dir = None;
    let mut terminal = false;
    let mut try_exec = None;
    let mut categories = Vec::new();
    let mut mime_types = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "Name" => {
                names.insert(String::new(), value.to_owned());
            }
            "Exec" => exec = Some(value.to_owned()),
            "Type" => kind = Some(value.to_owned()),
            "Icon" => icon = Some(value.to_owned()),
            "Path" => work_dir = Some(PathBuf::from(value)),
            "TryExec" => try_exec = Some(value.to_owned()),
            "Terminal" => terminal = matches!(value, "true" | "True" | "1"),
            "NoDisplay" | "Hidden" => hidden = hidden || matches!(value, "true" | "True" | "1"),
            "Categories" => {
                categories = value
                    .split(';')
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
                    .collect()
            }
            "MimeType" => {
                mime_types = value
                    .split(';')
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
                    .collect()
            }
            _ => {
                if let Some(lang) = key
                    .strip_prefix("Name[")
                    .and_then(|rest| rest.strip_suffix(']'))
                {
                    names.insert(lang.to_ascii_lowercase(), value.to_owned());
                }
            }
        }
    }
    if kind.as_deref() != Some("Application") || hidden {
        return None;
    }
    let exec = exec.filter(|exec| !exec.is_empty())?;
    if names.get("").map(|name| name.is_empty()).unwrap_or(true) {
        return None;
    }
    Some(DesktopApp {
        desktop_id: id.to_owned(),
        desktop_path: PathBuf::new(),
        names,
        icon: icon.filter(|icon| !icon.is_empty()),
        exec,
        work_dir: work_dir.filter(|path| path.is_dir()),
        categories,
        mime_types,
        terminal,
        try_exec: try_exec.filter(|value| !value.is_empty()),
    })
}

/// Układa kandydatów: domyślna aplikacja, potem obsługujące MIME, potem reszta.
pub fn order_apps(
    apps: &mut [DesktopApp],
    mime: Option<&str>,
    default_id: Option<&str>,
    lang: &str,
) {
    let rank = |app: &DesktopApp| -> u8 {
        if default_id == Some(app.desktop_id.as_str()) {
            0
        } else if mime.is_some_and(|mime| app.handles(mime)) {
            1
        } else {
            2
        }
    };
    apps.sort_by(|a, b| {
        rank(a).cmp(&rank(b)).then_with(|| {
            a.display_name(lang)
                .to_lowercase()
                .cmp(&b.display_name(lang).to_lowercase())
                .then_with(|| a.desktop_id.cmp(&b.desktop_id))
        })
    });
}

/// Aplikacje, którymi można otworzyć plik (w kolejności podpowiedzi).
pub fn candidates(path: &Path) -> Candidates {
    let mime = file_mime(path);
    let default_id = mime.as_deref().and_then(default_app_id);
    let mut apps = scan_apps();
    let lang = libluna_i18n::locale();
    order_apps(
        &mut apps,
        mime.as_deref(),
        default_id.as_deref(),
        lang.language(),
    );
    Candidates {
        mime,
        default_id,
        apps,
    }
}

/// Pierwszy program z `Exec=` (z pominięciem `env` i przypisań `A=B`).
pub fn exec_program(exec: &str) -> Option<String> {
    let mut tokens = split_exec(exec).into_iter();
    let mut program = tokens.next()?;
    if program == "env" {
        loop {
            let token = tokens.next()?;
            if token.starts_with('-') || token.contains('=') {
                continue;
            }
            program = token;
            break;
        }
    }
    Some(program)
}

/// Czy program istnieje (ścieżka w `Exec` albo wpis w `PATH`).
pub fn command_exists(program: &str) -> bool {
    if program.is_empty() {
        return false;
    }
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

/// Dzieli `Exec=` na argumenty (cudzysłowy i `\` jak w specyfikacji freedesktop).
pub fn split_exec(exec: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut chars = exec.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                    started = true;
                }
            }
            ch if ch.is_whitespace() && !quoted => {
                if started {
                    out.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            ch => {
                current.push(ch);
                started = true;
            }
        }
    }
    if started {
        out.push(current);
    }
    out
}

/// Ścieżka jako URI `file://` (dla pól `%u` / `%U`).
pub fn file_uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for &byte in path.as_os_str().as_encoded_bytes() {
        let keep = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/');
        if keep {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Rozwija `Exec=` do argumentów wywołania (pola `%f`, `%u`, `%i`, `%c`, `%k`…).
///
/// Gdy wpis nie zawiera pola pliku, ścieżka jest dopisywana na końcu — inaczej
/// „Otwórz za pomocą” nie przekazałoby pliku aplikacjom typu `Exec=gedit`.
pub fn exec_argv(
    exec: &str,
    path: &Path,
    name: &str,
    icon: Option<&str>,
    desktop_path: &Path,
) -> Vec<String> {
    let file = path.display().to_string();
    let uri = file_uri(path);
    let desktop = desktop_path.display().to_string();
    let mut args = Vec::new();
    let mut used_file = false;
    for token in split_exec(exec) {
        if token == "%i" {
            if let Some(icon) = icon.filter(|icon| !icon.is_empty()) {
                args.push("--icon".to_owned());
                args.push(icon.to_owned());
            }
            continue;
        }
        let mut out = String::new();
        let mut chars = token.chars();
        while let Some(ch) = chars.next() {
            if ch != '%' {
                out.push(ch);
                continue;
            }
            match chars.next() {
                Some('f') | Some('F') => {
                    out.push_str(&file);
                    used_file = true;
                }
                Some('u') | Some('U') => {
                    out.push_str(&uri);
                    used_file = true;
                }
                Some('c') => out.push_str(name),
                Some('k') => out.push_str(&desktop),
                Some('i') => {}
                Some('%') => out.push('%'),
                // Nieznane/przestarzałe pola (`%d`, `%n`, `%v`, `%m`) pomijamy.
                Some(_) => {}
                None => out.push('%'),
            }
        }
        if !out.is_empty() || token.is_empty() {
            args.push(out);
        }
    }
    if !used_file {
        args.push(file);
    }
    args
}

/// Terminale i ich sposób przekazania polecenia.
const TERMINALS: &[(&str, &[&str])] = &[
    ("luna-terminal", &["-e"]),
    ("xfce4-terminal", &["-x"]),
    ("konsole", &["-e"]),
    ("gnome-terminal", &["--"]),
    ("alacritty", &["-e"]),
    ("kitty", &[]),
    ("foot", &[]),
    ("wezterm", &["start", "--"]),
    ("xterm", &["-e"]),
];

/// Uruchamia polecenie w tle, bez terminala i bez przejmowania wejścia.
fn spawn_detached(program: &str, args: &[String], dir: Option<&Path>) -> bool {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    command.spawn().is_ok()
}

/// Uruchamia polecenie w pierwszym dostępnym terminalu.
fn spawn_in_terminal(argv: &[String], dir: Option<&Path>) -> bool {
    if argv.is_empty() {
        return false;
    }
    let mut candidates: Vec<(String, Vec<String>)> = Vec::new();
    if let Ok(term) = std::env::var("TERMINAL") {
        let term = term.trim();
        if !term.is_empty() {
            candidates.push((term.to_owned(), vec!["-e".to_owned()]));
        }
    }
    for (program, prefix) in TERMINALS {
        candidates.push((
            (*program).to_owned(),
            prefix.iter().map(|arg| (*arg).to_owned()).collect(),
        ));
    }
    for (program, prefix) in candidates {
        let mut command = Command::new(&program);
        command
            .args(&prefix)
            .args(argv)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(dir) = dir {
            command.current_dir(dir);
        }
        if command.spawn().is_ok() {
            return true;
        }
    }
    false
}

/// Otwiera plik wybraną aplikacją.
///
/// Najpierw własne rozwinięcie `Exec=`. Gdy program nie istnieje albo wymaga
/// terminala, którego nie ma, próbujemy jeszcze `gio launch` i `gtk-launch`
/// (one same obsługują `Terminal=`, `Path=` i pola plików).
pub fn launch(app: &DesktopApp, path: &Path) -> Result<(), String> {
    let locale = libluna_i18n::locale();
    let argv = exec_argv(
        &app.exec,
        path,
        app.display_name(locale.language()),
        app.icon.as_deref(),
        &app.desktop_path,
    );
    let dir = app.work_dir.as_deref();
    let started = match argv.split_first() {
        Some((program, rest)) => {
            if app.terminal {
                spawn_in_terminal(&argv, dir)
            } else {
                spawn_detached(program, rest, dir)
            }
        }
        None => false,
    };
    if started {
        return Ok(());
    }
    if !app.desktop_path.as_os_str().is_empty() {
        let desktop = app.desktop_path.display().to_string();
        let file = path.display().to_string();
        if spawn_detached(
            "gio",
            &["launch".to_owned(), desktop.clone(), file.clone()],
            None,
        ) {
            return Ok(());
        }
        if spawn_detached("gtk-launch", &[app.desktop_id.clone(), file], None) {
            return Ok(());
        }
    }
    Err(libluna_i18n::tr("Could not start the application"))
}

/// Stan otwartego okna „Otwórz za pomocą…” (bez geometrii — ta jest w `ui`).
pub struct OpenWithDialog {
    pub path: PathBuf,
    pub mime: Option<String>,
    pub default_id: Option<String>,
    pub apps: Vec<DesktopApp>,
    pub selected: Option<usize>,
    pub remember: bool,
    pub hovered: Option<usize>,
    pub scroll: ScrollView,
    /// Ostatni klik wiersza (do wykrycia podwójnego kliknięcia).
    pub last_click: Option<(usize, Instant)>,
}

impl OpenWithDialog {
    /// Buduje okno na podstawie systemu plików i `xdg-mime`.
    pub fn load(path: PathBuf) -> Self {
        let candidates = candidates(&path);
        Self::new(
            path,
            candidates.mime,
            candidates.default_id,
            candidates.apps,
        )
    }

    /// Buduje okno z gotowej listy (bez wejścia/wyjścia — wygodne w testach).
    pub fn new(
        path: PathBuf,
        mime: Option<String>,
        default_id: Option<String>,
        apps: Vec<DesktopApp>,
    ) -> Self {
        // Dla rozpoznanego typu zapamiętujemy wybór domyślnie: użytkownik, który
        // wybiera aplikację, zwykle chce, żeby ten typ otwierał się nią także
        // następnym razem. Przełącznik pozwala to wyłączyć.
        let remember = mime.as_deref().is_some_and(|mime| !is_unknown_mime(mime));
        let selected = apps
            .iter()
            .position(DesktopApp::available)
            .or_else(|| (!apps.is_empty()).then_some(0));
        let mut scroll = ScrollView::new();
        scroll.set_content_height(apps.len() as f64 * ROW_H);
        Self {
            path,
            mime,
            default_id,
            apps,
            selected,
            remember,
            hovered: None,
            scroll,
            last_click: None,
        }
    }

    pub fn selected_app(&self) -> Option<&DesktopApp> {
        self.selected.and_then(|index| self.apps.get(index))
    }

    /// Czy można zapamiętać wybór (tylko dla rozpoznanego typu MIME).
    pub fn can_remember(&self) -> bool {
        self.mime
            .as_deref()
            .is_some_and(|mime| !is_unknown_mime(mime))
    }

    /// Indeks aplikacji pod kursorem (z uwzględnieniem przewinięcia).
    pub fn index_at(&self, list: Rect, p: Point) -> Option<usize> {
        if !list.contains(p) {
            return None;
        }
        let y = p.y - list.y() + self.scroll.offset();
        if y < 0.0 {
            return None;
        }
        let index = (y / ROW_H).floor() as usize;
        (index < self.apps.len()).then_some(index)
    }

    /// Przesuwa zaznaczenie o `delta`, pomijając niedostępne aplikacje.
    pub fn move_selection(&mut self, delta: i32) {
        if self.apps.is_empty() {
            self.selected = None;
            return;
        }
        let mut index = self.selected.unwrap_or(0) as i32;
        let last = self.apps.len() as i32 - 1;
        for _ in 0..self.apps.len() {
            index = (index + delta).clamp(0, last);
            if self.apps[index as usize].available() {
                break;
            }
        }
        self.selected = Some(index as usize);
    }

    /// Przewija listę tak, aby zaznaczony wiersz był widoczny.
    pub fn scroll_to_selection(&mut self, viewport_h: f64) {
        let Some(index) = self.selected else {
            return;
        };
        let viewport = Rect::from_xywh(0.0, 0.0, 1.0, viewport_h);
        let content = self.apps.len() as f64 * ROW_H;
        let max = (content - viewport_h).max(0.0);
        let top = index as f64 * ROW_H;
        let offset = self.scroll.offset();
        let next = if top < offset {
            top
        } else if top + ROW_H > offset + viewport_h {
            top + ROW_H - viewport_h
        } else {
            offset
        };
        self.scroll.set_offset(next.clamp(0.0, max), viewport);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEDIT: &str = "\
[Desktop Entry]
Type=Application
Name=Text Editor
Name[pl]=Edytor tekstów
Exec=gedit %U
Icon=org.gnome.gedit
Categories=Utility;TextEditor;
MimeType=text/plain;
";

    #[test]
    fn parses_desktop_entry_with_localized_name() {
        let app = parse_desktop_entry(GEDIT, "org.gnome.gedit.desktop").expect("wpis");
        assert_eq!(app.display_name("pl"), "Edytor tekstów");
        assert_eq!(app.display_name("de"), "Text Editor");
        assert_eq!(app.icon.as_deref(), Some("org.gnome.gedit"));
        assert_eq!(app.exec, "gedit %U");
        assert!(app.handles("text/plain"));
        assert!(app.categories.contains(&"TextEditor".to_owned()));
    }

    #[test]
    fn skips_hidden_entries_and_non_applications() {
        let hidden = GEDIT.replace("Type=Application", "Type=Application\nNoDisplay=true");
        assert!(parse_desktop_entry(&hidden, "ukryta.desktop").is_none());
        let link = GEDIT.replace("Type=Application", "Type=Link");
        assert!(parse_desktop_entry(&link, "link.desktop").is_none());
        assert!(parse_desktop_entry(
            "[Desktop Entry]\nType=Application\nName=X\n",
            "brak.desktop"
        )
        .is_none());
    }

    #[test]
    fn desktop_id_uses_dashes_for_subdirectories() {
        let base = Path::new("/usr/share/applications");
        let path = Path::new("/usr/share/applications/kde/foo.desktop");
        assert_eq!(desktop_id(base, path).as_deref(), Some("kde-foo.desktop"));
    }

    #[test]
    fn splits_exec_honouring_quotes() {
        assert_eq!(split_exec("sh -c \"echo %f\""), vec!["sh", "-c", "echo %f"]);
        assert_eq!(
            split_exec("app  --flag\tvalue"),
            vec!["app", "--flag", "value"]
        );
        assert_eq!(split_exec("app \"\""), vec!["app", ""]);
    }

    #[test]
    fn exec_argv_expands_file_fields() {
        let path = Path::new("/tmp/notatka.txt");
        let args = exec_argv("gedit %f", path, "Editor", None, Path::new("/x/a.desktop"));
        assert_eq!(args, vec!["gedit", "/tmp/notatka.txt"]);

        let args = exec_argv("mpv -- %U", path, "Player", None, Path::new("/x/a.desktop"));
        assert_eq!(args, vec!["mpv", "--", "file:///tmp/notatka.txt"]);

        let args = exec_argv(
            "app %i",
            path,
            "App",
            Some("ikona"),
            Path::new("/x/a.desktop"),
        );
        assert_eq!(args, vec!["app", "--icon", "ikona", "/tmp/notatka.txt"]);

        let args = exec_argv(
            "app %c",
            path,
            "Moja Aplikacja",
            None,
            Path::new("/x/a.desktop"),
        );
        assert_eq!(args, vec!["app", "Moja Aplikacja", "/tmp/notatka.txt"]);
    }

    #[test]
    fn exec_argv_appends_file_when_entry_has_no_field_code() {
        let path = Path::new("/tmp/x.pdf");
        let args = exec_argv("app", path, "App", None, Path::new("/x/a.desktop"));
        assert_eq!(args, vec!["app", "/tmp/x.pdf"]);
    }

    #[test]
    fn exec_program_skips_env_assignments() {
        assert_eq!(exec_program("env FOO=1 bar --flag").as_deref(), Some("bar"));
        assert_eq!(
            exec_program("/usr/bin/foo %f").as_deref(),
            Some("/usr/bin/foo")
        );
    }

    #[test]
    fn orders_default_then_mime_then_rest() {
        let mut apps = vec![
            parse_desktop_entry(
                "[Desktop Entry]\nType=Application\nName=Zeta\nExec=zeta %f\n",
                "zeta.desktop",
            )
            .unwrap(),
            parse_desktop_entry(GEDIT, "gedit.desktop").unwrap(),
            parse_desktop_entry(
                "[Desktop Entry]\nType=Application\nName=Alfa\nExec=alfa %f\n",
                "alfa.desktop",
            )
            .unwrap(),
        ];
        order_apps(&mut apps, Some("text/plain"), Some("zeta.desktop"), "");
        let ids: Vec<&str> = apps.iter().map(|app| app.desktop_id.as_str()).collect();
        assert_eq!(ids, vec!["zeta.desktop", "gedit.desktop", "alfa.desktop"]);
    }

    #[test]
    fn mime_falls_back_to_extension() {
        assert_eq!(mime_by_extension(Path::new("a.PNG")), Some("image/png"));
        assert_eq!(mime_by_extension(Path::new("a.raw")), Some("image/x-dcraw"));
        assert_eq!(mime_by_extension(Path::new("a.dng")), Some("image/x-dcraw"));
        assert_eq!(mime_by_extension(Path::new("a")), None);
    }

    #[test]
    fn file_uri_encodes_special_characters() {
        assert_eq!(
            file_uri(Path::new("/tmp/plik z nazwą.txt")),
            "file:///tmp/plik%20z%20nazw%C4%85.txt"
        );
    }

    #[test]
    fn dialog_hit_testing_respects_scroll() {
        let mut dialog = OpenWithDialog::new(
            PathBuf::from("/tmp/plik.bin"),
            Some("application/octet-stream".to_owned()),
            None,
            vec![
                parse_desktop_entry(GEDIT, "gedit.desktop").unwrap(),
                parse_desktop_entry(
                    "[Desktop Entry]\nType=Application\nName=Inny\nExec=inny %f\n",
                    "inny.desktop",
                )
                .unwrap(),
                parse_desktop_entry(
                    "[Desktop Entry]\nType=Application\nName=Trzeci\nExec=trzeci %f\n",
                    "trzeci.desktop",
                )
                .unwrap(),
            ],
        );
        let list = Rect::from_xywh(0.0, 100.0, 400.0, ROW_H * 2.0);
        assert_eq!(dialog.index_at(list, Point::new(10.0, 110.0)), Some(0));
        assert_eq!(
            dialog.index_at(list, Point::new(10.0, 100.0 + ROW_H + 4.0)),
            Some(1)
        );
        assert_eq!(dialog.index_at(list, Point::new(10.0, 50.0)), None);
        // Przewinięcie o jeden wiersz przesuwa trafienia.
        dialog.scroll.set_offset(ROW_H, list);
        assert_eq!(dialog.index_at(list, Point::new(10.0, 110.0)), Some(1));
        assert!(!dialog.can_remember());
    }

    #[test]
    fn remember_defaults_to_true_for_known_types() {
        let gedit = parse_desktop_entry(GEDIT, "gedit.desktop").unwrap();
        let known = OpenWithDialog::new(
            PathBuf::from("/tmp/notatka.txt"),
            Some("text/plain".to_owned()),
            None,
            vec![gedit.clone()],
        );
        assert!(known.can_remember());
        assert!(
            known.remember,
            "wybór typu powinien być zapamiętywany domyślnie"
        );

        let unknown = OpenWithDialog::new(
            PathBuf::from("/tmp/plik.bin"),
            Some("application/octet-stream".to_owned()),
            None,
            vec![gedit],
        );
        assert!(!unknown.remember, "nieznanego typu nie da się zapamiętać");
    }

    #[test]
    #[ignore = "wymaga systemowych wpisów .desktop (uruchom ręcznie)"]
    fn lists_system_applications() {
        let candidates = candidates(Path::new("/tmp/notatka.txt"));
        assert!(
            candidates.apps.len() > 3,
            "oczekiwano wpisów .desktop w systemie"
        );
        assert!(candidates
            .apps
            .iter()
            .all(|app| app.desktop_id.ends_with(".desktop")));
    }

    #[test]
    #[ignore = "uruchamia proces (uruchom ręcznie)"]
    fn launch_runs_exec_from_desktop_entry() {
        let dir = std::env::temp_dir().join(format!("luna-open-with-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("uruchomione");
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=Test\nExec=sh -c \"touch {}\"\n",
            marker.display()
        );
        let app = parse_desktop_entry(&entry, "test.desktop").unwrap();
        launch(&app, &dir.join("plik.bin")).expect("uruchomienie");
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        while Instant::now() < deadline && !marker.exists() {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(marker.exists(), "polecenie z Exec nie zostało uruchomione");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_selection_stays_within_bounds() {
        let mut dialog = OpenWithDialog::new(
            PathBuf::from("/tmp/a.txt"),
            Some("text/plain".to_owned()),
            None,
            vec![
                parse_desktop_entry(GEDIT, "gedit.desktop").unwrap(),
                parse_desktop_entry(
                    "[Desktop Entry]\nType=Application\nName=Inny\nExec=inny %f\n",
                    "inny.desktop",
                )
                .unwrap(),
            ],
        );
        assert_eq!(dialog.selected, Some(0));
        dialog.move_selection(1);
        assert_eq!(dialog.selected, Some(1));
        dialog.move_selection(1);
        assert_eq!(dialog.selected, Some(1));
        dialog.move_selection(-5);
        assert_eq!(dialog.selected, Some(0));
        assert!(dialog.can_remember());
    }
}
