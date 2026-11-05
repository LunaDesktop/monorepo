//! System wtyczek Luna Editor.
//!
//! Wtyczki są **zewnętrznymi programami** — edytor ich nie zawiera i nie linkuje.
//! Każda wtyczka to katalog z manifestem `plugin.conf` i programem wejściowym.
//! Edytor uruchamia program jako proces potomny i przekazuje kontekst przez
//! zmienne środowiskowe oraz standardowe wejście; wynik czyta ze standardowego
//! wyjścia. Dzięki temu wtyczka może być napisana w dowolnym języku.
//!
//! Instalacja: `https://github.com/<owner>/<repo>` (archiwum `main`/`master`)
//! albo bezpośredni URL archiwum `.tar.gz`. Katalog wtyczki trafia do
//! `$XDG_DATA_HOME/luna/editor/plugins/<id>/`, a ustawienia (w tym klucze API)
//! do `$XDG_CONFIG_HOME/luna/editor/plugins/<id>.conf` z prawami `0600`.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{SystemTime, UNIX_EPOCH};

/// Nazwa pliku manifestu w katalogu wtyczki.
pub const MANIFEST: &str = "plugin.conf";

/// Rodzaj pola ustawień wtyczki.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingKind {
    Text,
    /// Wartość maskowana w interfejsie (np. klucz API).
    Secret,
    Select(Vec<String>),
    /// Lista pobierana z wtyczki (np. modele LLM) po wpisaniu klucza.
    Models,
}

/// Pole ustawień deklarowane przez wtyczkę.
#[derive(Debug, Clone)]
pub struct PluginSetting {
    pub key: String,
    pub label: String,
    pub kind: SettingKind,
    pub default: String,
}

/// Co wtyczka dostaje na standardowe wejście.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    Document,
    Selection,
    None,
}

/// Co edytor robi z tekstem zwróconym przez wtyczkę.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// Wstaw w miejscu kursora (zastępuje zaznaczenie).
    Insert,
    /// Zamień zaznaczenie (bez zaznaczenia — wstaw w kursorze).
    ReplaceSelection,
    /// Zamień całą treść dokumentu.
    ReplaceDocument,
    /// Dopisz na końcu dokumentu.
    Append,
}

/// Polecenie udostępniane przez wtyczkę.
#[derive(Debug, Clone)]
pub struct PluginCommand {
    pub id: String,
    pub title: String,
    /// Czy przed uruchomieniem zapytać o instrukcję (tekst zadania).
    pub prompt: bool,
    pub context: Context,
    pub output: Output,
}

/// Zainstalowana wtyczka.
#[derive(Debug, Clone)]
pub struct Plugin {
    pub dir: PathBuf,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    /// Program wejściowy (względem `dir`).
    pub entry: String,
    /// Opcjonalny interpreter, np. `python3`.
    pub interpreter: Option<String>,
    pub commands: Vec<PluginCommand>,
    pub settings: Vec<PluginSetting>,
    /// Wtyczka obsługuje panel czatu (akcja [`CHAT_ACTION`]).
    pub chat: bool,
}

impl Plugin {
    pub fn command(&self, index: usize) -> Option<&PluginCommand> {
        self.commands.get(index)
    }

    /// Wartości ustawień: domyślne z manifestu nadpisane plikiem konfiguracji.
    pub fn values(&self) -> HashMap<String, String> {
        let mut values: HashMap<String, String> = self
            .settings
            .iter()
            .map(|setting| (setting.key.clone(), setting.default.clone()))
            .collect();
        if let Ok(text) = fs::read_to_string(config_path(&self.id)) {
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    values.insert(key.trim().to_owned(), value.trim().to_owned());
                }
            }
        }
        values
    }

    /// Zapisuje ustawienia (tylko klucze zadeklarowane w manifeście).
    pub fn save_values(&self, values: &HashMap<String, String>) -> Result<(), String> {
        let path = config_path(&self.id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        let mut text = format!("# luna-editor plugin {}\n", self.id);
        for setting in &self.settings {
            let value = values.get(&setting.key).cloned().unwrap_or_default();
            text.push_str(&format!("{}={}\n", setting.key, value.replace('\n', " ")));
        }
        fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

/// Akcja zarezerwowana: wtyczka wypisuje dostępne modele (jeden na linię).
pub const LIST_MODELS_ACTION: &str = "__list_models__";

/// Akcja zarezerwowana: rozmowa w panelu czatu.
///
/// Wtyczka dostaje na standardowe wejście JSON z historią rozmowy, listą plików
/// projektu i kontekstem aktywnego pliku, a odpowiada JSON-em
/// `{"text": "…", "tool_calls": [{"tool": "read_file", "path": "…"}]}`.
/// Edytor wykonuje narzędzia i wywołuje wtyczkę ponownie (pętla agenta).
pub const CHAT_ACTION: &str = "__chat__";

/// Stan pracy w tle (instalacja, uruchomienie wtyczki, pobranie modeli).
pub enum Job {
    Install(Receiver<Result<String, String>>),
    Run {
        receiver: Receiver<Result<String, String>>,
        doc: usize,
        output: Output,
        title: String,
    },
    Models {
        receiver: Receiver<Result<Vec<String>, String>>,
        plugin: usize,
        setting: usize,
    },
}

/// Lista wtyczek i ewentualna praca w tle.
pub struct PluginHost {
    pub plugins: Vec<Plugin>,
    pub job: Option<Job>,
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginHost {
    pub fn new() -> Self {
        let mut host = Self {
            plugins: Vec::new(),
            job: None,
        };
        host.reload();
        host
    }

    pub fn reload(&mut self) {
        self.plugins = load_all();
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
}

/// Katalog danych użytkownika dla wtyczek.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("luna/editor/plugins")
}

/// Katalog konfiguracji wtyczek.
pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("luna/editor/plugins")
}

fn config_path(id: &str) -> PathBuf {
    config_dir().join(format!("{id}.conf"))
}

/// Wczytuje wszystkie wtyczki z katalogu danych (bez duplikatów `id`).
pub fn load_all() -> Vec<Plugin> {
    let dir = data_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut plugins = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest = path.join(MANIFEST);
        let Ok(text) = fs::read_to_string(&manifest) else {
            continue;
        };
        if let Ok(plugin) = parse_manifest(&path, &text) {
            if !plugins
                .iter()
                .any(|existing: &Plugin| existing.id == plugin.id)
            {
                plugins.push(plugin);
            }
        }
    }
    plugins.sort_by_key(|plugin| plugin.name.to_lowercase());
    plugins
}

/// Parsuje manifest `plugin.conf`.
///
/// Format jest liniowy (`klucz = wartość`), celowo prosty do ręcznej edycji:
///
/// ```text
/// id = ai-writer
/// name = AI Writer
/// version = 0.1.0
/// entry = ai_writer.py
/// interpreter = python3
/// command = write|Napisz tekst…|prompt|document|insert
/// setting = provider|Dostawca|select|gemini,openai|gemini
/// ```
///
/// - `command = id|tytuł|prompt|kontekst|wyjście`
/// - `setting = klucz|etykieta|text|secret|select|opcje,po,przecinku|domyślna`
pub fn parse_manifest(dir: &Path, text: &str) -> Result<Plugin, String> {
    let mut id = String::new();
    let mut name = String::new();
    let mut version = String::from("0.0.0");
    let mut author = String::new();
    let mut description = String::new();
    let mut entry = String::new();
    let mut interpreter = None;
    let mut commands = Vec::new();
    let mut settings = Vec::new();
    let mut chat = false;

    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, raw)) = line.split_once('=') else {
            return Err(format!(
                "wiersz {}: oczekiwano „klucz = wartość”",
                number + 1
            ));
        };
        let key = key.trim();
        let value = unquote(raw.trim());
        match key {
            "id" => id = value,
            "name" => name = value,
            "version" => version = value,
            "author" => author = value,
            "description" => description = value,
            "entry" => entry = value,
            "interpreter" => {
                interpreter = (!value.is_empty()).then_some(value);
            }
            "command" => commands.push(
                parse_command(&value).map_err(|error| format!("wiersz {}: {error}", number + 1))?,
            ),
            "chat" => chat = matches!(value.to_lowercase().as_str(), "1" | "true" | "yes" | "on"),
            "setting" => settings.push(
                parse_setting(&value).map_err(|error| format!("wiersz {}: {error}", number + 1))?,
            ),
            _ => {}
        }
    }

    if id.is_empty() || !valid_id(&id) {
        return Err("manifest: niepoprawne lub puste `id`".to_owned());
    }
    if name.is_empty() {
        name = id.clone();
    }
    if entry.is_empty() {
        return Err("manifest: brak `entry` (programu wtyczki)".to_owned());
    }
    if commands.is_empty() && !chat {
        return Err("manifest: wtyczka nie deklaruje żadnego `command` ani `chat`".to_owned());
    }
    Ok(Plugin {
        dir: dir.to_path_buf(),
        id,
        name,
        version,
        author,
        description,
        entry,
        interpreter,
        commands,
        settings,
        chat,
    })
}

fn unquote(value: &str) -> String {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_owned()
    } else {
        value.to_owned()
    }
}

fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphanumeric())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn parse_command(value: &str) -> Result<PluginCommand, String> {
    let parts: Vec<&str> = value.split('|').map(str::trim).collect();
    let id = parts.first().copied().unwrap_or("");
    let title = parts.get(1).copied().unwrap_or("");
    if id.is_empty() || title.is_empty() {
        return Err(format!("`command` wymaga id i tytułu: „{value}”"));
    }
    let prompt = matches!(
        parts.get(2).copied().unwrap_or(""),
        "prompt" | "1" | "true" | "yes"
    );
    let context = match parts.get(3).copied().unwrap_or("") {
        "selection" => Context::Selection,
        "none" => Context::None,
        _ => Context::Document,
    };
    let output = match parts.get(4).copied().unwrap_or("") {
        "replace_selection" => Output::ReplaceSelection,
        "replace_document" => Output::ReplaceDocument,
        "append" => Output::Append,
        _ => Output::Insert,
    };
    Ok(PluginCommand {
        id: id.to_owned(),
        title: title.to_owned(),
        prompt,
        context,
        output,
    })
}

fn parse_setting(value: &str) -> Result<PluginSetting, String> {
    let parts: Vec<&str> = value.split('|').map(str::trim).collect();
    let key = parts.first().copied().unwrap_or("");
    let label = parts.get(1).copied().unwrap_or(key);
    if key.is_empty() {
        return Err(format!("`setting` wymaga klucza: „{value}”"));
    }
    let kind = match parts.get(2).copied().unwrap_or("text") {
        "secret" => SettingKind::Secret,
        "models" => SettingKind::Models,
        "select" => SettingKind::Select(
            parts
                .get(3)
                .copied()
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|option| !option.is_empty())
                .map(str::to_owned)
                .collect(),
        ),
        _ => SettingKind::Text,
    };
    let default = parts.get(4).copied().unwrap_or("").to_owned();
    Ok(PluginSetting {
        key: key.to_owned(),
        label: label.to_owned(),
        kind,
        default,
    })
}

/// Uruchamia instalację z GitHuba (albo z bezpośredniego URL archiwum) w tle.
pub fn spawn_install(url: String) -> Receiver<Result<String, String>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(install_blocking(&url));
    });
    receiver
}

fn install_blocking(url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("Podaj adres repozytorium GitHub albo archiwum .tar.gz.".to_owned());
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let temp =
        std::env::temp_dir().join(format!("luna-editor-plugin-{}-{stamp}", std::process::id()));
    let result = (|| {
        fs::create_dir_all(&temp).map_err(|error| format!("{}: {error}", temp.display()))?;
        let archive = temp.join("plugin.tar.gz");
        download_archive(url, &archive)?;
        extract(&archive, &temp)?;
        let manifest = find_manifest(&temp, 0)
            .ok_or_else(|| "W archiwum nie znaleziono plugin.conf.".to_owned())?;
        let text = fs::read_to_string(&manifest)
            .map_err(|error| format!("{}: {error}", manifest.display()))?;
        let source = manifest
            .parent()
            .ok_or_else(|| "Niepoprawna ścieżka manifestu.".to_owned())?;
        let plugin = parse_manifest(source, &text)?;
        let dest = data_dir().join(&plugin.id);
        if dest.exists() {
            fs::remove_dir_all(&dest).map_err(|error| format!("{}: {error}", dest.display()))?;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        copy_tree(source, &dest)?;
        Ok(format!("{} {}", plugin.name, plugin.version))
    })();
    let _ = fs::remove_dir_all(&temp);
    result
}

fn download_archive(input: &str, dest: &Path) -> Result<(), String> {
    let mut candidates = Vec::new();
    if let Some((owner, repo)) = github_parts(input) {
        for branch in ["main", "master"] {
            candidates.push(format!(
                "https://codeload.github.com/{owner}/{repo}/tar.gz/refs/heads/{branch}"
            ));
        }
    } else {
        candidates.push(input.to_owned());
    }

    let mut last = String::new();
    for url in candidates {
        match download_one(&url, dest) {
            Ok(()) => return Ok(()),
            Err(error) => last = error,
        }
    }
    Err(if last.is_empty() {
        "Nie udało się pobrać archiwum wtyczki.".to_owned()
    } else {
        format!("Nie udało się pobrać wtyczki: {last}")
    })
}

fn download_one(url: &str, dest: &Path) -> Result<(), String> {
    let mut attempts = Vec::new();
    if let Ok(status) = Command::new("curl")
        .args([
            "-fsSL",
            "--retry",
            "2",
            "--connect-timeout",
            "15",
            "--max-time",
            "180",
            "-o",
        ])
        .arg(dest)
        .arg(url)
        .status()
    {
        if status.success() {
            return Ok(());
        }
        attempts.push(format!("curl ({status})"));
    }
    if let Ok(status) = Command::new("wget")
        .args(["-q", "--timeout=120", "-O"])
        .arg(dest)
        .arg(url)
        .status()
    {
        if status.success() {
            return Ok(());
        }
        attempts.push(format!("wget ({status})"));
    }
    Err(format!(
        "{url} ({})",
        if attempts.is_empty() {
            "brak curl/wget".to_owned()
        } else {
            attempts.join(", ")
        }
    ))
}

fn github_parts(input: &str) -> Option<(String, String)> {
    let input = input.trim().trim_end_matches('/');
    let rest = input
        .strip_prefix("https://github.com/")
        .or_else(|| input.strip_prefix("http://github.com/"))
        .or_else(|| input.strip_prefix("github.com/"))?;
    let mut segments = rest.split('/');
    let owner = segments.next()?.trim();
    let repo = segments.next()?.trim().trim_end_matches(".git");
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner.to_owned(), repo.to_owned()))
}

fn extract(archive: &Path, dir: &Path) -> Result<(), String> {
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(archive)
        .arg("-C")
        .arg(dir)
        .status()
        .map_err(|error| format!("tar: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("tar zakończył się błędem ({status})"))
    }
}

fn find_manifest(dir: &Path, depth: usize) -> Option<PathBuf> {
    if depth > 3 {
        return None;
    }
    let manifest = dir.join(MANIFEST);
    if manifest.is_file() {
        return Some(manifest);
    }
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !path.is_dir() || name == ".git" || name.starts_with('.') {
            continue;
        }
        if let Some(found) = find_manifest(&path, depth + 1) {
            return Some(found);
        }
    }
    None
}

fn copy_tree(source: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|error| format!("{}: {error}", dest.display()))?;
    let entries = fs::read_dir(source).map_err(|error| format!("{}: {error}", source.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let target = dest.join(&name);
        if metadata.is_dir() {
            copy_tree(&path, &target)?;
        } else if metadata.is_file() {
            fs::copy(&path, &target).map_err(|error| format!("{}: {error}", target.display()))?;
        }
    }
    Ok(())
}

/// Usuwa wtyczkę i jej konfigurację.
pub fn remove(plugin: &Plugin) -> Result<(), String> {
    if plugin.dir.exists() {
        fs::remove_dir_all(&plugin.dir)
            .map_err(|error| format!("{}: {error}", plugin.dir.display()))?;
    }
    let config = config_path(&plugin.id);
    if config.exists() {
        fs::remove_file(&config).map_err(|error| format!("{}: {error}", config.display()))?;
    }
    Ok(())
}

/// Dane przekazywane wtyczce przy uruchomieniu.
pub struct RunRequest {
    pub plugin: Plugin,
    pub command: PluginCommand,
    pub settings: HashMap<String, String>,
    pub instruction: String,
    pub path: Option<PathBuf>,
    pub language: String,
    pub payload: String,
}

/// Uruchamia polecenie wtyczki w tle i zwraca wynik na kanale.
pub fn spawn_run(request: RunRequest) -> Receiver<Result<String, String>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(run_blocking(request));
    });
    receiver
}

/// Pobiera listę modeli z wtyczki (akcja [`LIST_MODELS_ACTION`]).
pub fn spawn_models(
    plugin: Plugin,
    settings: HashMap<String, String>,
) -> Receiver<Result<Vec<String>, String>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let request = RunRequest {
            plugin,
            command: PluginCommand {
                id: LIST_MODELS_ACTION.to_owned(),
                title: "Models".to_owned(),
                prompt: false,
                context: Context::None,
                output: Output::Insert,
            },
            settings,
            instruction: String::new(),
            path: None,
            language: String::new(),
            payload: String::new(),
        };
        let result = run_blocking(request).and_then(|text| {
            let mut models: Vec<String> = text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect();
            models.sort_by_key(|model| model.to_lowercase());
            models.dedup();
            if models.is_empty() {
                Err("Dostawca nie zwrócił listy modeli.".to_owned())
            } else {
                Ok(models)
            }
        });
        let _ = sender.send(result);
    });
    receiver
}

/// Uruchamia dowolną akcję wtyczki z podanym ładunkiem na stdin.
///
/// Używane przez panel czatu, który sam zarządza historią rozmowy i pętlą
/// narzędzi.
pub(crate) fn run_action(
    plugin: &Plugin,
    settings: &HashMap<String, String>,
    action: &str,
    payload: &str,
) -> Result<String, String> {
    run_blocking(RunRequest {
        plugin: plugin.clone(),
        command: PluginCommand {
            id: action.to_owned(),
            title: action.to_owned(),
            prompt: false,
            context: Context::None,
            output: Output::Insert,
        },
        settings: settings.clone(),
        instruction: String::new(),
        path: None,
        language: String::new(),
        payload: payload.to_owned(),
    })
}

fn run_blocking(request: RunRequest) -> Result<String, String> {
    let mut command = match &request.plugin.interpreter {
        Some(interpreter) => {
            let mut parts = interpreter.split_whitespace();
            let program = parts.next().unwrap_or("sh");
            let mut command = Command::new(program);
            for arg in parts {
                command.arg(arg);
            }
            command.arg(request.plugin.dir.join(&request.plugin.entry));
            command
        }
        None => Command::new(request.plugin.dir.join(&request.plugin.entry)),
    };
    command
        .current_dir(&request.plugin.dir)
        .env("LUNA_PLUGIN_ID", &request.plugin.id)
        .env("LUNA_PLUGIN_ACTION", &request.command.id)
        .env("LUNA_PLUGIN_INSTRUCTION", &request.instruction)
        .env("LUNA_PLUGIN_LANGUAGE", &request.language)
        .env(
            "LUNA_PLUGIN_PATH",
            request
                .path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in &request.settings {
        command.env(format!("LUNA_SETTING_{}", env_key(key)), value);
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("Nie udało się uruchomić wtyczki: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(request.payload.as_bytes())
            .map_err(|error| format!("Zapis do wtyczki: {error}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("Wtyczka: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        return Err(if message.is_empty() {
            format!("Wtyczka zakończyła się błędem ({})", output.status)
        } else {
            message.to_owned()
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if stdout.trim().is_empty() {
        return Err("Wtyczka nie zwróciła tekstu.".to_owned());
    }
    Ok(stdout)
}

fn env_key(key: &str) -> String {
    key.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_commands_and_settings() {
        let text = "\
id = demo
name = Demo plugin
version = 1.2.3
entry = run.py
interpreter = python3
command = write|Napisz|prompt|document|insert
command = fix|Popraw||selection|replace_selection
setting = provider|Dostawca|select|gemini,openai|gemini
setting = api_key|Klucz|secret||
";
        let plugin = parse_manifest(Path::new("/tmp/demo"), text).unwrap();
        assert_eq!(plugin.id, "demo");
        assert_eq!(plugin.name, "Demo plugin");
        assert_eq!(plugin.commands.len(), 2);
        assert!(plugin.commands[0].prompt);
        assert_eq!(plugin.commands[0].output, Output::Insert);
        assert_eq!(plugin.commands[1].context, Context::Selection);
        assert_eq!(plugin.commands[1].output, Output::ReplaceSelection);
        assert_eq!(plugin.settings.len(), 2);
        assert_eq!(
            plugin.settings[0].kind,
            SettingKind::Select(vec!["gemini".into(), "openai".into()])
        );
        assert_eq!(plugin.settings[1].kind, SettingKind::Secret);
    }

    #[test]
    fn manifest_rejects_invalid_id_and_missing_entry() {
        assert!(parse_manifest(Path::new("."), "id = Zły Id\nentry = x").is_err());
        assert!(parse_manifest(Path::new("."), "id = ok\n").is_err());
        assert!(parse_manifest(Path::new("."), "id = ok\nentry = x").is_err());
    }

    #[test]
    fn github_urls_are_recognized() {
        assert_eq!(
            github_parts("https://github.com/LunaDesktop/luna-editor-ai/"),
            Some(("LunaDesktop".into(), "luna-editor-ai".into()))
        );
        assert_eq!(
            github_parts("github.com/owner/repo.git"),
            Some(("owner".into(), "repo".into()))
        );
        assert_eq!(github_parts("https://example.com/plugin.tar.gz"), None);
    }

    #[test]
    fn env_names_are_sanitized() {
        assert_eq!(env_key("api-key"), "API_KEY");
        assert_eq!(env_key("model"), "MODEL");
    }

    #[test]
    fn installs_a_plugin_from_an_archive() {
        let base =
            std::env::temp_dir().join(format!("luna-editor-plugin-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let source = base.join("source/demo");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join(MANIFEST),
            "id = demo\nname = Demo\nentry = run.sh\ncommand = run|Run||document|insert\n",
        )
        .unwrap();
        std::fs::write(source.join("run.sh"), "#!/bin/sh\nprintf x\n").unwrap();
        let archive = base.join("demo.tar.gz");
        let status = Command::new("tar")
            .args(["-czf"])
            .arg(&archive)
            .arg("-C")
            .arg(base.join("source"))
            .arg("demo")
            .status()
            .expect("tar");
        assert!(status.success());

        let data = base.join("data");
        let previous = std::env::var_os("XDG_DATA_HOME");
        std::env::set_var("XDG_DATA_HOME", &data);
        let result = install_blocking(&format!("file://{}", archive.display()));
        match previous {
            Some(value) => std::env::set_var("XDG_DATA_HOME", value),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        assert!(result.is_ok(), "{result:?}");
        assert!(data
            .join("luna/editor/plugins/demo")
            .join(MANIFEST)
            .is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    #[ignore = "wymaga sieci"]
    fn installs_the_ai_writer_from_github() {
        let base =
            std::env::temp_dir().join(format!("luna-editor-plugin-github-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let previous = std::env::var_os("XDG_DATA_HOME");
        std::env::set_var("XDG_DATA_HOME", &base);
        let result = install_blocking("https://github.com/LunaDesktop/luna-editor-ai-writer");
        match previous {
            Some(value) => std::env::set_var("XDG_DATA_HOME", value),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        assert!(result.is_ok(), "{result:?}");
        let manifest = base.join("luna/editor/plugins/ai-writer").join(MANIFEST);
        assert!(manifest.is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn listing_models_reads_lines_from_the_plugin() {
        let dir =
            std::env::temp_dir().join(format!("luna-editor-plugin-models-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("models.sh"), "#!/bin/sh\nprintf 'b\\na\\nb\\n'\n").unwrap();
        let plugin = Plugin {
            dir: dir.clone(),
            id: "models-test".into(),
            name: "Models".into(),
            version: "1".into(),
            author: String::new(),
            description: String::new(),
            entry: "models.sh".into(),
            interpreter: Some("sh".into()),
            commands: vec![PluginCommand {
                id: "run".into(),
                title: "Run".into(),
                prompt: false,
                context: Context::None,
                output: Output::Insert,
            }],
            settings: Vec::new(),
            chat: false,
        };
        let receiver = spawn_models(plugin, HashMap::new());
        let models = receiver.recv().unwrap().unwrap();
        assert_eq!(models, vec!["a", "b"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn running_a_plugin_pipes_the_payload() {
        let dir =
            std::env::temp_dir().join(format!("luna-editor-plugin-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("run.sh"), "#!/bin/sh\nprintf 'echo:'; cat\n").unwrap();
        let plugin = Plugin {
            dir: dir.clone(),
            id: "run-test".into(),
            name: "Run test".into(),
            version: "1".into(),
            author: String::new(),
            description: String::new(),
            entry: "run.sh".into(),
            interpreter: Some("sh".into()),
            commands: vec![PluginCommand {
                id: "echo".into(),
                title: "Echo".into(),
                prompt: false,
                context: Context::Selection,
                output: Output::Insert,
            }],
            settings: Vec::new(),
            chat: false,
        };
        let command = plugin.commands[0].clone();
        let receiver = spawn_run(RunRequest {
            plugin,
            command,
            settings: HashMap::new(),
            instruction: "test".into(),
            path: None,
            language: "Plain".into(),
            payload: "abc".into(),
        });
        let output = receiver.recv().unwrap().unwrap();
        assert_eq!(output, "echo:abc");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
