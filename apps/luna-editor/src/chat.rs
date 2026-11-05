//! Panel czatu z wtyczką AI i pętla narzędzi (agent).
//!
//! Panel pojawia się tylko wtedy, gdy zainstalowana jest wtyczka deklarująca
//! w manifeście `chat = true`. Edytor wysyła jej akcję [`plugins::CHAT_ACTION`]
//! z JSON-em: historią rozmowy, listą plików projektu, treścią aktywnego pliku
//! i listą dostępnych narzędzi. Wtyczka może odpowiedzieć samym tekstem albo
//! `tool_calls` — wtedy edytor wykonuje narzędzia (`read_file`, `list_dir`,
//! `search`, `write_file`, `create_file`, `delete_file`), dokłada wyniki do
//! historii i wywołuje wtyczkę ponownie. Dzięki temu model pracuje na prawdziwym
//! projekcie, a nie na jednym wklejonym fragmencie.
//!
//! Bezpieczeństwo: narzędzia widzą wyłącznie pliki wewnątrz katalogu projektu,
//! a zapis do pliku otwartego z niezapisanymi zmianami jest odrzucany.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Instant;

use libluna_core::style::weight;
use libluna_core::{ButtonState, Color, Event, KeyEvent, Point, PointerButton, Rect, Size, Theme};
use libluna_i18n::tr;
use libluna_render::TextEngine;
use libluna_widgets::{
    clipboard_command, Button, ButtonVariant, Entry, PaintCtx, TextClipboardCommand,
};

use crate::app::App;
use crate::plugins::{self, Plugin};
use crate::ui::Layout;

/// Maksymalna liczba wywołań wtyczki w jednej turze (kroki agenta).
pub const MAX_STEPS: usize = 6;
/// Szerokość panelu.
pub const PANEL_W: f64 = 340.0;
/// Ile plików wysyłamy w manifeście projektu.
const MAX_FILES: usize = 400;
/// Łączny budżet treści plików wysyłanych od razu (bez wywołań narzędzi).
const MAX_INLINE: usize = 48 * 1024;
/// Maksymalny rozmiar jednego pliku wysyłanego „w całości”.
const MAX_INLINE_FILE: u64 = 24 * 1024;
/// Limit treści zwracanej przez `read_file`.
const MAX_TOOL_BYTES: usize = 48 * 1024;
const PAD: f64 = 12.0;
const HEADER_H: f64 = 36.0;
const INPUT_H: f64 = 34.0;
const FOOTER_H: f64 = 22.0;
const LINE_GAP: f64 = 1.35;

/// Narzędzia udostępniane wtyczkom czatu.
pub const TOOLS: [&str; 6] = [
    "read_file",
    "list_dir",
    "search",
    "write_file",
    "create_file",
    "delete_file",
];

/// Rola wiadomości w historii rozmowy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    /// Wynik narzędzia (wysyłany modelowi jako `role: tool`).
    Tool,
    /// Komunikat samego edytora (np. błąd), nie wysyłany do modelu.
    Note,
}

/// Jedna wiadomość w panelu czatu.
#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub text: String,
}

/// Zdarzenie z wątku agenta.
enum ChatEvent {
    Progress(String),
    Reply { text: String, written: Vec<PathBuf> },
    Failed(String),
}

/// Stan panelu czatu.
pub struct Chat {
    pub visible: bool,
    pub focused: bool,
    pub input: Entry,
    pub messages: Vec<Message>,
    pub scroll: f64,
    pub busy: bool,
    pub status: Option<String>,
    pub hover_send: bool,
    pub hover_close: bool,
    pub hover_clear: bool,
    pub hover_model: bool,
    pub hover_gear: bool,
    pub pointer: Option<Point>,
    /// Maksymalne przewinięcie listy wiadomości (liczone przy rysowaniu).
    pub scroll_max: f64,
    /// Popup wyboru modelu (jak w czacie Zed/VS Code).
    pub picker: Option<Picker>,
    /// Modele pobrane z API dostawcy.
    pub models: Vec<String>,
    pub models_loading: bool,
    pub models_error: Option<String>,
    /// Po pobraniu listy otwórz popup automatycznie.
    models_sig: String,
    models_job: Option<Receiver<Result<Vec<String>, String>>>,
    /// Faza migania kursora w polu pytania (reset przy pisaniu).
    caret_anchor: Instant,
    caret_last: (usize, usize),
    /// Przewiń na koniec po dodaniu wiadomości.
    stick_to_bottom: bool,
    job: Option<Receiver<ChatEvent>>,
}

/// Popup wyboru modelu z filtrowaniem.
pub struct Picker {
    pub query: Entry,
    pub scroll: f64,
    pub hover: Option<usize>,
}

impl Default for Chat {
    fn default() -> Self {
        Self::new(false)
    }
}

impl Chat {
    pub fn new(visible: bool) -> Self {
        let mut input = Entry::new(tr("Ask about the project…"));
        input.set_focused(false);
        input.set_blinking(true);
        Self {
            visible,
            focused: false,
            input,
            messages: Vec::new(),
            scroll: 0.0,
            busy: false,
            status: None,
            hover_send: false,
            hover_close: false,
            hover_clear: false,
            hover_model: false,
            hover_gear: false,
            pointer: None,
            scroll_max: 0.0,
            picker: None,
            models: Vec::new(),
            models_loading: false,
            models_error: None,
            models_sig: String::new(),
            models_job: None,
            caret_anchor: Instant::now(),
            caret_last: (0, 0),
            stick_to_bottom: true,
            job: None,
        }
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.scroll = 0.0;
        self.status = None;
        self.stick_to_bottom = true;
    }

    fn push(&mut self, role: Role, text: impl Into<String>) {
        self.messages.push(Message {
            role,
            text: text.into(),
        });
        self.stick_to_bottom = true;
    }
}

/// Czy jest zainstalowana wtyczka obsługująca czat.
pub fn available(app: &App) -> bool {
    app.plugins.plugins.iter().any(|plugin| plugin.chat)
}

/// Indeks wtyczki używanej przez panel (pierwsza z `chat = true`).
pub fn plugin_index(app: &App) -> Option<usize> {
    app.plugins.plugins.iter().position(|plugin| plugin.chat)
}

/// Nazwa aktywnej wtyczki czatu (do nagłówka panelu).
pub fn plugin_name(app: &App) -> Option<String> {
    plugin_index(app).and_then(|index| {
        app.plugins
            .plugins
            .get(index)
            .map(|plugin| plugin.name.clone())
    })
}

/// Klucz ustawienia typu `models` (np. `model`).
fn models_key(plugin: &Plugin) -> Option<String> {
    plugin
        .settings
        .iter()
        .find(|setting| setting.kind == plugins::SettingKind::Models)
        .map(|setting| setting.key.clone())
}

/// Aktualnie wybrany model (pusty, gdy wtyczka go nie deklaruje).
pub fn current_model(app: &App) -> Option<String> {
    let plugin = app.plugins.plugins.get(plugin_index(app)?)?;
    let key = models_key(plugin)?;
    plugin
        .values()
        .get(&key)
        .cloned()
        .filter(|value| !value.is_empty())
}

/// Sygnatura pozostałych ustawień — zmiana klucza/dostawcy wymusza odświeżenie listy.
fn settings_signature(plugin: &Plugin) -> String {
    let values = plugin.values();
    let mut parts: Vec<String> = plugin
        .settings
        .iter()
        .filter(|setting| setting.kind != plugins::SettingKind::Models)
        .map(|setting| {
            format!(
                "{}={}",
                setting.key,
                values.get(&setting.key).cloned().unwrap_or_default()
            )
        })
        .collect();
    parts.sort();
    parts.join("|")
}

/// Otwiera popup wyboru modelu (i pobiera listę, jeśli jej nie ma).
pub fn open_model_picker(app: &mut App) {
    let Some(index) = plugin_index(app) else {
        return;
    };
    let plugin = app.plugins.plugins[index].clone();
    if models_key(&plugin).is_none() {
        return;
    }
    let signature = settings_signature(&plugin);
    if app.chat.models_sig != signature {
        app.chat.models.clear();
        app.chat.models_error = None;
        app.chat.models_sig = signature;
    }
    let mut query = Entry::new(tr("Search…"));
    query.set_focused(true);
    query.set_blinking(true);
    app.chat.picker = Some(Picker {
        query,
        scroll: 0.0,
        hover: None,
    });
    if app.chat.models.is_empty() && !app.chat.models_loading {
        app.chat.models_loading = true;
        app.chat.models_job = Some(plugins::spawn_models(plugin.clone(), plugin.values()));
    }
}

fn filtered_models(models: &[String], query: &str) -> Vec<String> {
    let needle = query.trim().to_lowercase();
    models
        .iter()
        .filter(|model| needle.is_empty() || model.to_lowercase().contains(&needle))
        .cloned()
        .collect()
}

/// Zapisuje wybrany model w konfiguracji wtyczki.
fn apply_model(app: &mut App, model: &str) {
    let Some(index) = plugin_index(app) else {
        return;
    };
    let plugin = app.plugins.plugins[index].clone();
    let Some(key) = models_key(&plugin) else {
        return;
    };
    let mut values = plugin.values();
    values.insert(key, model.to_owned());
    match plugin.save_values(&values) {
        Ok(()) => {
            app.chat.models_error = None;
            app.chat.status = Some(format!("{}: {}", tr("Model"), model));
        }
        Err(error) => app.chat.models_error = Some(error),
    }
    app.chat.picker = None;
}

/// Włącza/wyłącza panel. Gdy nie ma wtyczki czatu, otwiera menedżer wtyczek.
pub fn toggle(app: &mut App) {
    if !available(app) {
        app.plugin_ui.status = Some(tr("Install a plugin that declares `chat = true`"));
        app.open_plugins();
        return;
    }
    app.chat.visible = !app.chat.visible;
    if !app.chat.visible {
        app.chat.focused = false;
        app.chat.input.set_focused(false);
    }
    app.prefs.chat_open = app.chat.visible;
    app.prefs_dirty = true;
}

/// Fokus pola tekstowego czatu.
pub fn focus_input(app: &mut App) {
    app.chat.visible = true;
    app.chat.focused = true;
    app.chat.input.set_focused(true);
    app.chat.caret_anchor = Instant::now();
}

fn blur(app: &mut App) {
    app.chat.focused = false;
    app.chat.input.set_focused(false);
}

/// Wysyła wpisany tekst do wtyczki.
pub fn send(app: &mut App) {
    if app.chat.busy {
        return;
    }
    let text = app.chat.input.text().trim().to_owned();
    if text.is_empty() {
        return;
    }
    let Some(index) = plugin_index(app) else {
        app.chat.push(Role::Note, tr("No AI plugin is installed."));
        return;
    };
    let Some(plugin) = app.plugins.plugins.get(index).cloned() else {
        return;
    };
    let settings = plugin.values();
    // Brak wymaganego sekretu (np. klucza API) — podpowiadamy zamiast strzelać
    // do API i pokazywać błąd HTTP.
    let missing = plugin.settings.iter().find(|setting| {
        setting.kind == plugins::SettingKind::Secret
            && setting.key.to_lowercase().contains("key")
            && settings
                .get(&setting.key)
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
    });
    if let Some(setting) = missing {
        app.chat.push(
            Role::Note,
            format!(
                "{}: {}",
                tr("This plugin needs a key: open Plugins → Settings"),
                setting.label
            ),
        );
        return;
    }

    app.chat.input.clear();
    app.chat.caret_anchor = Instant::now();
    app.chat.push(Role::User, text.clone());
    app.chat.status = Some(tr("The plugin is working…"));
    app.chat.busy = true;

    let turn = Turn {
        plugin,
        settings,
        root: app.workspace.root.clone(),
        files: collect_files(&app.workspace.root),
        inline: Vec::new(),
        active: active_file(app),
        open: open_documents(app),
        messages: app.chat.messages.clone(),
    };
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut turn = turn;
        turn.inline = inline_files(&turn.root, turn.active.as_ref());
        run_turn(turn, sender);
    });
    app.chat.job = Some(receiver);
}

/// Odbiera wynik pracy agenta i odświeża dokumenty zmienione przez wtyczkę.
pub fn poll(app: &mut App) {
    if let Some(job) = app.chat.models_job.take() {
        match job.try_recv() {
            Ok(Ok(models)) => {
                app.chat.models_loading = false;
                app.chat.models = models;
                app.chat.models_error = None;
            }
            Ok(Err(error)) => {
                app.chat.models_loading = false;
                app.chat.models_error = Some(error);
            }
            Err(TryRecvError::Empty) => app.chat.models_job = Some(job),
            Err(TryRecvError::Disconnected) => {
                app.chat.models_loading = false;
                app.chat.models_error = Some(tr("Models ended without a result"));
            }
        }
    }
    let Some(receiver) = app.chat.job.take() else {
        return;
    };
    match receiver.try_recv() {
        Ok(ChatEvent::Progress(text)) => {
            app.chat.status = Some(text);
            app.chat.job = Some(receiver);
        }
        Ok(ChatEvent::Reply { text, written }) => {
            app.chat.busy = false;
            app.chat.status = None;
            app.chat.push(Role::Assistant, text);
            for path in &written {
                app.reload_from_disk(path);
            }
            if !written.is_empty() {
                app.workspace.refresh();
                app.status = Some(format!(
                    "{}: {}",
                    tr("Files updated by the assistant"),
                    written.len()
                ));
            }
        }
        Ok(ChatEvent::Failed(error)) => {
            app.chat.busy = false;
            app.chat.status = None;
            app.chat.push(Role::Note, error);
        }
        Err(TryRecvError::Empty) => app.chat.job = Some(receiver),
        Err(TryRecvError::Disconnected) => {
            app.chat.busy = false;
            app.chat.status = None;
            app.chat
                .push(Role::Note, tr("The plugin stopped without a result."));
        }
    }
}

// ── Kontekst projektu ───────────────────────────────────────────────────────

/// Plik aktywny w edytorze (to, co użytkownik widzi).
struct ActiveFile {
    path: String,
    language: String,
    content: String,
    selection: String,
}

/// Snapshot stanu projektu wysyłany wtyczce.
struct Turn {
    plugin: Plugin,
    settings: HashMap<String, String>,
    root: PathBuf,
    files: Vec<(String, u64)>,
    inline: Vec<(String, String)>,
    active: Option<ActiveFile>,
    /// Otwarte dokumenty i informacja, czy mają niezapisane zmiany.
    open: HashMap<PathBuf, bool>,
    messages: Vec<Message>,
}

impl Turn {
    fn payload(&self, history: &[Message]) -> String {
        let files: Vec<serde_json::Value> = self
            .files
            .iter()
            .map(|(path, size)| serde_json::json!({ "path": path, "size": size }))
            .collect();
        let contents: Vec<serde_json::Value> = self
            .inline
            .iter()
            .map(|(path, text)| serde_json::json!({ "path": path, "text": text }))
            .collect();
        let messages: Vec<serde_json::Value> = history
            .iter()
            .filter(|message| message.role != Role::Note)
            .map(|message| {
                let role = match message.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                    Role::Note => "note",
                };
                serde_json::json!({ "role": role, "content": message.text })
            })
            .collect();
        let active = self.active.as_ref().map(|file| {
            serde_json::json!({
                "path": file.path,
                "language": file.language,
                "content": file.content,
                "selection": file.selection,
            })
        });
        serde_json::json!({
            "version": 1,
            "workspace": {
                "name": self
                    .root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(""),
                "root": self.root.display().to_string(),
                "files": files,
                "contents": contents,
            },
            "file": active,
            "messages": messages,
            "tools": TOOLS,
        })
        .to_string()
    }
}

/// Odpytuje wtyczkę krok po kroku, wykonując narzędzia, aż dostanie odpowiedź.
fn run_turn(turn: Turn, sender: mpsc::Sender<ChatEvent>) {
    let mut history = turn.messages.clone();
    let mut written: Vec<PathBuf> = Vec::new();
    for step in 0..MAX_STEPS {
        let payload = turn.payload(&history);
        let reply =
            match plugins::run_action(&turn.plugin, &turn.settings, plugins::CHAT_ACTION, &payload)
            {
                Ok(text) => text,
                Err(error) => {
                    let _ = sender.send(ChatEvent::Failed(error));
                    return;
                }
            };
        let parsed = Reply::parse(&reply);
        if parsed.tool_calls.is_empty() {
            let text = if parsed.text.trim().is_empty() {
                tr("The model did not return an answer.")
            } else {
                parsed.text
            };
            let _ = sender.send(ChatEvent::Reply { text, written });
            return;
        }

        if !parsed.text.trim().is_empty() {
            history.push(Message {
                role: Role::Assistant,
                text: parsed.text.clone(),
            });
        }
        let mut results = Vec::with_capacity(parsed.tool_calls.len());
        for call in &parsed.tool_calls {
            let _ = sender.send(ChatEvent::Progress(format!(
                "{} {}",
                call.tool,
                call.path_or_query()
            )));
            let outcome = execute_tool(&turn.root, &turn.open, call);
            if let Some(path) = outcome.written {
                if !written.contains(&path) {
                    written.push(path);
                }
            }
            results.push(Message {
                role: Role::Tool,
                text: outcome.message,
            });
        }
        if step + 1 == MAX_STEPS {
            let _ = sender.send(ChatEvent::Reply {
                text: format!(
                    "{}\n\n{}",
                    parsed.text,
                    tr("The agent reached the step limit — ask it to continue.")
                ),
                written,
            });
            return;
        }
        history.extend(results);
    }
}

fn active_file(app: &App) -> Option<ActiveFile> {
    let doc = app.doc();
    let path = doc.path.as_ref()?;
    let root = &app.workspace.root;
    let relative = path
        .strip_prefix(root)
        .ok()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| path.display().to_string());
    let content = truncate(doc.view.text(), MAX_INLINE as usize);
    let selection = doc.view.selected_text().unwrap_or_default().to_owned();
    Some(ActiveFile {
        path: relative,
        language: doc.language.name().to_owned(),
        content,
        selection,
    })
}

fn open_documents(app: &App) -> HashMap<PathBuf, bool> {
    app.docs
        .iter()
        .filter_map(|doc| doc.path.clone().map(|path| (path, doc.dirty)))
        .collect()
}

fn collect_files(root: &Path) -> Vec<(String, u64)> {
    let mut files = Vec::new();
    walk_files(root, root, 0, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

fn walk_files(root: &Path, dir: &Path, depth: usize, files: &mut Vec<(String, u64)>) {
    if depth > 6 || files.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<(PathBuf, bool, u64)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || matches!(name.as_str(), "target" | "node_modules") {
                return None;
            }
            let path = entry.path();
            let metadata = entry.metadata().ok()?;
            let is_dir = metadata.is_dir();
            Some((path, is_dir, metadata.len()))
        })
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, is_dir, size) in items {
        if files.len() >= MAX_FILES {
            return;
        }
        if is_dir {
            walk_files(root, &path, depth + 1, files);
        } else if let Ok(relative) = path.strip_prefix(root) {
            files.push((relative.display().to_string(), size));
        }
    }
}

fn inline_files(root: &Path, active: Option<&ActiveFile>) -> Vec<(String, String)> {
    let mut picked: Vec<(String, String)> = Vec::new();
    let mut budget = MAX_INLINE;
    let add = |path: &str, picked: &mut Vec<(String, String)>, budget: &mut usize| {
        if *budget == 0 || picked.iter().any(|(known, _)| known == path) {
            return;
        }
        let full = root.join(path);
        let Ok(metadata) = std::fs::metadata(&full) else {
            return;
        };
        if metadata.len() > MAX_INLINE_FILE {
            return;
        }
        let Ok(bytes) = std::fs::read(&full) else {
            return;
        };
        if bytes.contains(&0) {
            return;
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let take = text.len().min(*budget);
        let mut end = take;
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        picked.push((path.to_owned(), text[..end].to_owned()));
        *budget -= end;
    };

    if let Some(active) = active {
        add(&active.path, &mut picked, &mut budget);
    }
    add("README.md", &mut picked, &mut budget);
    for (path, size) in collect_files(root) {
        if size > MAX_INLINE_FILE {
            continue;
        }
        add(&path, &mut picked, &mut budget);
        if budget == 0 {
            break;
        }
    }
    picked
}

fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n…", &text[..end])
}

// ── Odpowiedź wtyczki i narzędzia ───────────────────────────────────────────

/// Odpowiedź wtyczki: tekst + opcjonalne wywołania narzędzi.
#[derive(Debug, Default)]
struct Reply {
    text: String,
    tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Default, Clone)]
struct ToolCall {
    tool: String,
    path: String,
    content: String,
    query: String,
}

impl ToolCall {
    fn path_or_query(&self) -> String {
        if !self.path.is_empty() {
            self.path.clone()
        } else {
            self.query.clone()
        }
    }
}

impl Reply {
    fn parse(raw: &str) -> Reply {
        let trimmed = raw.trim();
        let body = strip_fence(trimmed);
        if !body.starts_with('{') {
            return Reply {
                text: trimmed.to_owned(),
                tool_calls: Vec::new(),
            };
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) else {
            return Reply {
                text: trimmed.to_owned(),
                tool_calls: Vec::new(),
            };
        };
        let text = value
            .get("text")
            .or_else(|| value.get("message"))
            .or_else(|| value.get("content"))
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_owned();
        let mut tool_calls = Vec::new();
        let list = value
            .get("tool_calls")
            .or_else(|| value.get("tools"))
            .or_else(|| value.get("actions"));
        if let Some(list) = list.and_then(|list| list.as_array()) {
            for entry in list {
                if let Some(call) = parse_call(entry) {
                    tool_calls.push(call);
                }
            }
        }
        Reply { text, tool_calls }
    }
}

fn strip_fence(text: &str) -> String {
    if !text.starts_with("```") {
        return text.to_owned();
    }
    let body = text
        .trim_start_matches("```")
        .trim_start_matches(['j', 's', 'o', 'n'])
        .trim_end_matches("```")
        .trim();
    body.to_owned()
}

fn parse_call(entry: &serde_json::Value) -> Option<ToolCall> {
    if let Some(name) = entry.as_str() {
        return Some(ToolCall {
            tool: name.to_owned(),
            ..ToolCall::default()
        });
    }
    let object = entry.as_object()?;
    let string = |key: &str| {
        object
            .get(key)
            .and_then(|value| value.as_str())
            .map(str::to_owned)
    };
    let args = object.get("args").and_then(|value| value.as_object());
    let pick = |key: &str| -> String {
        if let Some(value) = args
            .and_then(|args| args.get(key))
            .and_then(|value| value.as_str())
        {
            return value.to_owned();
        }
        object
            .get(key)
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    let tool = string("tool")
        .or_else(|| string("name"))
        .or_else(|| string("action"))
        .unwrap_or_default();
    if tool.is_empty() {
        return None;
    }
    Some(ToolCall {
        tool,
        path: pick("path"),
        content: pick("content"),
        query: pick("query"),
    })
}

struct Outcome {
    message: String,
    written: Option<PathBuf>,
}

impl Outcome {
    fn error(tool: &str, path: &str, error: impl std::fmt::Display) -> Self {
        Self {
            message: format!("[{tool} {path}] ERROR: {error}"),
            written: None,
        }
    }
}

/// Wykonuje jedno wywołanie narzędzia z odpowiedzi wtyczki.
fn execute_tool(root: &Path, open: &HashMap<PathBuf, bool>, call: &ToolCall) -> Outcome {
    let tool = call.tool.trim();
    let path = call.path.trim();
    match tool {
        "read_file" => match resolve(root, path) {
            Ok(full) => match std::fs::read(&full) {
                Ok(bytes) => {
                    if bytes.contains(&0) {
                        return Outcome::error(tool, path, "plik binarny");
                    }
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    Outcome {
                        message: format!("[read_file {path}]\n{}", truncate(&text, MAX_TOOL_BYTES)),
                        written: None,
                    }
                }
                Err(error) => Outcome::error(tool, path, error),
            },
            Err(error) => Outcome::error(tool, path, error),
        },
        "list_dir" => {
            let full = match if path.is_empty() {
                Ok(root.to_path_buf())
            } else {
                resolve(root, path)
            } {
                Ok(full) => full,
                Err(error) => return Outcome::error(tool, path, error),
            };
            match std::fs::read_dir(&full) {
                Ok(entries) => {
                    let mut lines = Vec::new();
                    for entry in entries.flatten().take(300) {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        let meta = entry.metadata().ok();
                        let is_dir = meta.as_ref().map(|meta| meta.is_dir()).unwrap_or(false);
                        let size = meta.map(|meta| meta.len()).unwrap_or(0);
                        if is_dir {
                            lines.push(format!("{name}/"));
                        } else {
                            lines.push(format!("{name} ({size} B)"));
                        }
                    }
                    lines.sort();
                    if lines.is_empty() {
                        lines.push("(pusty katalog)".to_owned());
                    }
                    Outcome {
                        message: format!(
                            "[list_dir {}]\n{}",
                            if path.is_empty() { "." } else { path },
                            lines.join("\n")
                        ),
                        written: None,
                    }
                }
                Err(error) => Outcome::error(tool, path, error),
            }
        }
        "search" => {
            let query = call.query.trim();
            if query.is_empty() {
                return Outcome::error(tool, path, "puste zapytanie");
            }
            let mut hits = Vec::new();
            search_files(root, path, query, &mut hits);
            let message = if hits.is_empty() {
                format!("[search {query}]\n(brak trafień)")
            } else {
                format!("[search {query}]\n{}", hits.join("\n"))
            };
            Outcome {
                message,
                written: None,
            }
        }
        "write_file" | "create_file" => {
            let full = match resolve(root, path) {
                Ok(full) => full,
                Err(error) => return Outcome::error(tool, path, error),
            };
            if let Some(true) = open.get(&full) {
                return Outcome::error(
                    tool,
                    path,
                    "plik jest otwarty w edytorze z niezapisanymi zmianami",
                );
            }
            if tool == "create_file" && full.exists() {
                return Outcome::error(tool, path, "plik już istnieje");
            }
            if let Some(parent) = full.parent() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    return Outcome::error(tool, path, error);
                }
            }
            match std::fs::write(&full, &call.content) {
                Ok(()) => Outcome {
                    message: format!("[{tool} {path}] zapisano {} B", call.content.len()),
                    written: Some(full),
                },
                Err(error) => Outcome::error(tool, path, error),
            }
        }
        "delete_file" => {
            let full = match resolve(root, path) {
                Ok(full) => full,
                Err(error) => return Outcome::error(tool, path, error),
            };
            if let Some(true) = open.get(&full) {
                return Outcome::error(
                    tool,
                    path,
                    "plik jest otwarty w edytorze z niezapisanymi zmianami",
                );
            }
            if !full.is_file() {
                return Outcome::error(tool, path, "to nie jest plik");
            }
            match std::fs::remove_file(&full) {
                Ok(()) => Outcome {
                    message: format!("[delete_file {path}] usunięto"),
                    written: Some(full),
                },
                Err(error) => Outcome::error(tool, path, error),
            }
        }
        other => Outcome::error(
            other,
            path,
            format!("nieznane narzędzie (dostępne: {})", TOOLS.join(", ")),
        ),
    }
}

fn search_files(root: &Path, path: &str, query: &str, hits: &mut Vec<String>) {
    let start = if path.is_empty() {
        root.to_path_buf()
    } else {
        match resolve(root, path) {
            Ok(full) => full,
            Err(_) => return,
        }
    };
    let needle = query.to_lowercase();
    if start.is_file() {
        scan_file(root, &start, &needle, hits);
        return;
    }
    let mut stack = vec![start];
    while let Some(dir) = stack.pop() {
        if hits.len() >= 20 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || matches!(name.as_str(), "target" | "node_modules") {
                continue;
            }
            let full = entry.path();
            let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
            if is_dir {
                stack.push(full);
            } else {
                scan_file(root, &full, &needle, hits);
                if hits.len() >= 20 {
                    return;
                }
            }
        }
    }
}

fn scan_file(root: &Path, path: &Path, needle: &str, hits: &mut Vec<String>) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    if metadata.len() > 512 * 1024 {
        return;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    if bytes.contains(&0) {
        return;
    }
    let relative = match path.strip_prefix(root) {
        Ok(relative) => relative.display().to_string(),
        Err(_) => path.display().to_string(),
    };
    let text = String::from_utf8_lossy(&bytes);
    for (number, line) in text.lines().enumerate() {
        if line.to_lowercase().contains(needle) {
            let line: String = line.chars().take(160).collect();
            hits.push(format!("{}:{}: {}", relative, number + 1, line.trim_end()));
            if hits.len() >= 20 {
                return;
            }
        }
    }
}

/// Zamienia ścieżkę względną na absolutną i pilnuje, żeby nie wyszła z projektu.
fn resolve(root: &Path, path: &str) -> Result<PathBuf, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("pusta ścieżka".to_owned());
    }
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Err("ścieżka musi być względna wobec projektu".to_owned());
    }
    let mut full = root.to_path_buf();
    for component in candidate.components() {
        match component {
            Component::Normal(name) => full.push(name),
            Component::CurDir => {}
            _ => return Err("ścieżka wychodzi poza projekt".to_owned()),
        }
    }
    Ok(full)
}

// ── Rysowanie ───────────────────────────────────────────────────────────────

/// Rysuje panel czatu (wołane z `ui::paint`).
pub fn paint(app: &mut App, ctx: &mut PaintCtx, lay: &Layout) {
    if !app.chat.visible || !available(app) {
        return;
    }
    let theme = *ctx.theme;
    let colors = theme.colors;
    let panel = lay.chat;
    ctx.scene.fill_rect(panel, colors.sidebar_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(panel.x(), panel.y(), 1.0, panel.height()),
        colors.border,
    );

    let title = plugin_name(app).unwrap_or_else(|| tr("AI Assistant"));
    ctx.label_left(
        lay.chat_header.x(),
        lay.chat_header.center().y,
        &elide(
            ctx,
            &title,
            lay.chat_header.width(),
            theme.typography.body.size,
        ),
        theme.typography.body.size,
        weight::MEDIUM,
        colors.text,
    );
    let pointer = app.chat.pointer;
    app.chat.hover_clear = pointer.is_some_and(|p| lay.chat_clear.contains(p));
    app.chat.hover_close = pointer.is_some_and(|p| lay.chat_close.contains(p));
    app.chat.hover_gear = pointer.is_some_and(|p| lay.chat_gear.contains(p));
    app.chat.hover_model = pointer.is_some_and(|p| lay.chat_model.contains(p));
    app.chat.hover_send = pointer.is_some_and(|p| lay.chat_send.contains(p));

    let mut clear = Button::new(tr("Clear"));
    clear.set_hovered(app.chat.hover_clear);
    clear.paint(ctx, lay.chat_clear);
    // Zębatka — ustawienia wtyczki bez wychodzenia z czatu.
    if app.chat.hover_gear {
        ctx.scene
            .fill_rounded_rect(lay.chat_gear, theme.radius.small, colors.shade);
    }
    libluna_widgets::paint_icon(
        ctx,
        libluna_widgets::IconName::Settings,
        lay.chat_gear.deflate(libluna_core::Insets::splat(5.0)),
        colors.text,
    );
    let mut close = Button::new("×");
    close.set_hovered(app.chat.hover_close);
    close.paint(ctx, lay.chat_close);

    // Selektor modelu (jak w czacie Zed/VS Code) — pełna szerokość pod nagłówkiem.
    let model = current_model(app).unwrap_or_else(|| tr("Default model"));
    let model_rect = lay.chat_model;
    ctx.scene.fill_rounded_rect(
        model_rect,
        theme.radius.small,
        if app.chat.hover_model {
            colors.shade
        } else {
            colors.card_bg
        },
    );
    ctx.scene
        .stroke_rounded_rect(model_rect, theme.radius.small, 1.0, colors.border);
    let label = format!("{}: {}", tr("Model"), model);
    ctx.label_left(
        model_rect.x() + 10.0,
        model_rect.center().y,
        &elide(
            ctx,
            &label,
            model_rect.width() - 34.0,
            theme.typography.caption.size,
        ),
        theme.typography.caption.size,
        weight::NORMAL,
        colors.text,
    );
    ctx.label_right(
        model_rect.right() - 10.0,
        model_rect.center().y,
        "▾",
        theme.typography.caption.size,
        weight::NORMAL,
        colors.dim_text,
    );

    paint_messages(app, ctx, lay);

    // Miganie kursora: nowy znak/pozycja resetuje fazę, więc kursor jest
    // od razu widoczny po kliknięciu i po wpisaniu litery.
    let state = (app.chat.input.text().len(), app.chat.input.cursor());
    if state != app.chat.caret_last {
        app.chat.caret_last = state;
        app.chat.caret_anchor = Instant::now();
    }
    let elapsed = app.chat.caret_anchor.elapsed().as_millis() % 1100;
    app.chat.input.set_blinking(true);
    app.chat.input.set_caret_visible(elapsed < 550);

    let mut input = app.chat.input.clone();
    input.set_focused(app.chat.focused);
    input.paint(ctx, lay.chat_input);
    let mut send = Button::new(tr("Send")).variant(ButtonVariant::Suggested);
    send.set_hovered(app.chat.hover_send && !app.chat.busy);
    send.paint(ctx, lay.chat_send);

    let status_rect = Rect::from_xywh(
        panel.x() + PAD,
        lay.chat_input.y() - FOOTER_H,
        panel.width() - 2.0 * PAD,
        FOOTER_H,
    );
    if let Some(status) = app.chat.status.as_deref() {
        let text = elide(
            ctx,
            status,
            status_rect.width(),
            theme.typography.caption.size,
        );
        ctx.label_left(
            status_rect.x(),
            status_rect.center().y,
            &text,
            theme.typography.caption.size,
            weight::NORMAL,
            colors.accent,
        );
    } else if app.chat.messages.is_empty() {
        let hint = if available(app) {
            tr("Ask the assistant about the project.")
        } else {
            tr("Install an AI plugin (Plugins…) to chat about the project.")
        };
        let lines = wrap(
            ctx,
            &hint,
            status_rect.width(),
            theme.typography.caption.size,
        );
        let y = status_rect.center().y + 6.0;
        for line in lines.iter().take(2) {
            ctx.label_left(
                status_rect.x(),
                y,
                line,
                theme.typography.caption.size,
                weight::NORMAL,
                colors.dim_text,
            );
        }
    }

    if app.chat.picker.is_some() {
        paint_picker(app, ctx, lay);
    }
}

fn paint_messages(app: &mut App, ctx: &mut PaintCtx, lay: &Layout) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let area = lay.chat_messages;
    let width = area.width() - 2.0 * PAD;
    let body = theme.typography.body.size;
    let caption = theme.typography.caption.size;
    let line_h = body * LINE_GAP;

    // Najpierw mierzymy całą treść, żeby znać zakres przewijania.
    let mut blocks: Vec<(String, Color, Vec<String>, f64)> = Vec::new();
    let mut total = 0.0;
    for message in &app.chat.messages {
        let (label, color) = match message.role {
            Role::User => (tr("You"), colors.accent),
            Role::Assistant => (
                plugin_name(app).unwrap_or_else(|| tr("Assistant")),
                colors.text,
            ),
            Role::Tool => (tr("Tool"), colors.dim_text),
            Role::Note => (tr("Editor"), colors.warning),
        };
        let lines = wrap(ctx, &message.text, width, body);
        let height = caption * LINE_GAP + lines.len() as f64 * line_h + 8.0;
        total += height;
        blocks.push((label, color, lines, height));
    }
    let max_scroll = (total - area.height()).max(0.0);
    if app.chat.stick_to_bottom {
        app.chat.scroll = max_scroll;
        app.chat.stick_to_bottom = false;
    }
    app.chat.scroll = app.chat.scroll.clamp(0.0, max_scroll);

    ctx.scene.push_clip(area);
    let mut y = area.y() + 4.0 - app.chat.scroll;
    for (label, color, lines, height) in blocks {
        if y + height >= area.y() && y <= area.bottom() {
            ctx.label_left(
                area.x() + PAD,
                y + caption * 0.7,
                &label,
                caption,
                weight::MEDIUM,
                color,
            );
            let mut line_y = y + caption * LINE_GAP + body * 0.8;
            for line in &lines {
                ctx.label_left(
                    area.x() + PAD,
                    line_y,
                    line,
                    body,
                    weight::NORMAL,
                    if color == colors.accent {
                        colors.text
                    } else {
                        color
                    },
                );
                line_y += line_h;
            }
        }
        y += height;
    }
    ctx.scene.pop_clip();
    app.chat.scroll_max = max_scroll;
}

fn wrap(ctx: &PaintCtx, text: &str, width: f64, size: f64) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        if raw.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in raw.split(' ') {
            let candidate = if current.is_empty() {
                word.to_owned()
            } else {
                format!("{current} {word}")
            };
            if ctx.measure(&candidate, size, weight::NORMAL) <= width {
                current = candidate;
                continue;
            }
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            if ctx.measure(word, size, weight::NORMAL) <= width {
                current = word.to_owned();
            } else {
                // Bardzo długi token (np. ścieżka) — łamiemy po znakach.
                let mut chunk = String::new();
                for ch in word.chars() {
                    chunk.push(ch);
                    if ctx.measure(&chunk, size, weight::NORMAL) > width {
                        chunk.pop();
                        lines.push(std::mem::take(&mut chunk));
                        chunk.push(ch);
                    }
                }
                current = chunk;
            }
        }
        if !current.is_empty() || raw == " " {
            lines.push(current);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn elide(ctx: &PaintCtx, text: &str, width: f64, size: f64) -> String {
    if width <= 0.0 || ctx.measure(text, size, weight::NORMAL) <= width {
        return text.to_owned();
    }
    let dots = "…";
    let dots_w = ctx.measure(dots, size, weight::NORMAL);
    let mut out = String::new();
    for ch in text.chars() {
        let mut probe = out.clone();
        probe.push(ch);
        if ctx.measure(&probe, size, weight::NORMAL) + dots_w > width {
            break;
        }
        out = probe;
    }
    out.push('…');
    out
}

// ── Popup wyboru modelu ──────────────────────────────────────────────────────

const PICKER_ROW_H: f64 = 26.0;

fn picker_rect(lay: &Layout) -> Rect {
    let anchor = lay.chat_model;
    let height = 232.0f64.min((lay.chat.bottom() - anchor.bottom() - 12.0).max(96.0));
    Rect::from_xywh(anchor.x(), anchor.bottom() + 4.0, anchor.width(), height)
}

fn picker_search(card: Rect) -> Rect {
    Rect::from_xywh(
        card.x() + 8.0,
        card.y() + 8.0,
        (card.width() - 16.0).max(0.0),
        28.0,
    )
}

fn picker_list(card: Rect) -> Rect {
    let search = picker_search(card);
    Rect::from_xywh(
        card.x() + 4.0,
        search.bottom() + 6.0,
        (card.width() - 8.0).max(0.0),
        (card.bottom() - 6.0 - (search.bottom() + 6.0)).max(0.0),
    )
}

fn paint_picker(app: &mut App, ctx: &mut PaintCtx, lay: &Layout) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let Some(picker) = app.chat.picker.as_ref() else {
        return;
    };
    let card = picker_rect(lay);
    ctx.scene
        .fill_rounded_rect(card, theme.radius.medium, colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(card, theme.radius.medium, 1.0, colors.border);

    let search = picker_search(card);
    let mut query = picker.query.clone();
    query.set_focused(true);
    query.paint(ctx, search);

    let list = picker_list(card);
    let models = filtered_models(&app.chat.models, picker.query.text());
    let current = current_model(app);
    ctx.scene.push_clip(list);
    if app.chat.models_loading && app.chat.models.is_empty() {
        ctx.label_left(
            list.x() + 8.0,
            list.y() + 18.0,
            &tr("Loading models…"),
            theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
    } else if let Some(error) = app.chat.models_error.as_deref() {
        let lines = wrap(
            ctx,
            error,
            list.width() - 16.0,
            theme.typography.caption.size,
        );
        for (index, line) in lines.iter().take(3).enumerate() {
            ctx.label_left(
                list.x() + 8.0,
                list.y() + 18.0 + index as f64 * 16.0,
                line,
                theme.typography.caption.size,
                weight::NORMAL,
                colors.error,
            );
        }
    } else if models.is_empty() {
        ctx.label_left(
            list.x() + 8.0,
            list.y() + 18.0,
            &tr("No models available"),
            theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
    } else {
        let offset = picker.scroll;
        for (index, model) in models.iter().enumerate() {
            let y = list.y() + index as f64 * PICKER_ROW_H - offset;
            if y + PICKER_ROW_H < list.y() || y > list.bottom() {
                continue;
            }
            let row = Rect::from_xywh(list.x(), y, list.width(), PICKER_ROW_H);
            let selected = current.as_deref() == Some(model.as_str());
            if picker.hover == Some(index) {
                ctx.scene
                    .fill_rounded_rect(row, theme.radius.small, colors.shade);
            } else if selected {
                ctx.scene.fill_rounded_rect(
                    row,
                    theme.radius.small,
                    colors.accent.scale_alpha(0.16),
                );
            }
            ctx.label_left(
                row.x() + 8.0,
                row.center().y,
                &elide(
                    ctx,
                    model,
                    row.width() - 24.0,
                    theme.typography.caption.size,
                ),
                theme.typography.caption.size,
                weight::NORMAL,
                if selected { colors.accent } else { colors.text },
            );
        }
    }
    ctx.scene.pop_clip();
}

/// Obsługa popupu modelu. Zwraca `true`, gdy zdarzenie zostało skonsumowane.
fn handle_picker(app: &mut App, event: &Event, lay: &Layout, _theme: &Theme) -> bool {
    let Some(picker) = app.chat.picker.as_ref() else {
        return false;
    };
    let card = picker_rect(lay);
    let list = picker_list(card);
    let models = filtered_models(&app.chat.models, picker.query.text());
    match event {
        Event::PointerMotion { position, .. } => {
            if list.contains(*position) {
                let scroll = app.chat.picker.as_ref().map(|p| p.scroll).unwrap_or(0.0);
                let row = ((position.y - list.y() + scroll) / PICKER_ROW_H).floor();
                let index = if row.is_sign_negative() {
                    None
                } else {
                    let index = row as usize;
                    (index < models.len()).then_some(index)
                };
                if let Some(picker) = app.chat.picker.as_mut() {
                    picker.hover = index;
                }
            } else if let Some(picker) = app.chat.picker.as_mut() {
                picker.hover = None;
            }
            true
        }
        Event::PointerButton(button)
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            let point = button.position;
            if list.contains(point) {
                let scroll = app.chat.picker.as_ref().map(|p| p.scroll).unwrap_or(0.0);
                let index = ((point.y - list.y() + scroll) / PICKER_ROW_H).floor();
                if !index.is_sign_negative() {
                    let index = index as usize;
                    if let Some(model) = models.get(index).cloned() {
                        apply_model(app, &model);
                    }
                }
                return true;
            }
            if !card.contains(point) && !lay.chat_model.contains(point) {
                app.chat.picker = None;
                // Klik poza popupem ma nadal działać (np. pole pytania).
                return false;
            }
            true
        }
        Event::Scroll(scroll) => {
            if card.contains(scroll.position) {
                let max = (models.len() as f64 * PICKER_ROW_H - list.height()).max(0.0);
                if let Some(picker) = app.chat.picker.as_mut() {
                    picker.scroll = (picker.scroll + scroll.dy / 20.0 * 64.0).clamp(0.0, max);
                }
                return true;
            }
            false
        }
        Event::Key(key) if key.state == ButtonState::Pressed => {
            if key.keysym == 0xff1b {
                app.chat.picker = None;
                return true;
            }
            if key.keysym == 0xff0d || key.keysym == 0xff8d {
                let hover = app.chat.picker.as_ref().and_then(|p| p.hover);
                if let Some(model) = hover.and_then(|index| models.get(index).cloned()) {
                    apply_model(app, &model);
                }
                return true;
            }
            if key.keysym == 0xff54 || key.keysym == 0xff52 {
                let count = models.len();
                if count > 0 {
                    if let Some(picker) = app.chat.picker.as_mut() {
                        let next = match (picker.hover, key.keysym) {
                            (None, 0xff54) => 0,
                            (None, _) => count - 1,
                            (Some(index), 0xff54) => (index + 1).min(count - 1),
                            (Some(index), _) => index.saturating_sub(1),
                        };
                        picker.hover = Some(next);
                        let visible = (list.height() / PICKER_ROW_H).floor().max(1.0);
                        if (next as f64) < picker.scroll / PICKER_ROW_H {
                            picker.scroll = next as f64 * PICKER_ROW_H;
                        } else if next as f64 >= picker.scroll / PICKER_ROW_H + visible {
                            picker.scroll = (next as f64 + 1.0 - visible) * PICKER_ROW_H;
                        }
                    }
                }
                return true;
            }
            let Some(picker) = app.chat.picker.as_mut() else {
                return true;
            };
            let command = clipboard_command(&mut picker.query, key);
            if let Some(command) = command {
                match command {
                    TextClipboardCommand::Copy(Some(value))
                    | TextClipboardCommand::Cut(Some(value)) => app.clipboard_text = Some(value),
                    TextClipboardCommand::Paste => app.paste_requested = true,
                    _ => {}
                }
                return true;
            }
            match key.keysym {
                0xff08 => picker.query.backspace(),
                0xffff => picker.query.delete_forward(),
                0xff51 => picker.query.move_left(),
                0xff53 => picker.query.move_right(),
                0xff50 => picker.query.home(),
                0xff57 => picker.query.end(),
                _ => insert_key_text(&mut picker.query, key),
            }
            picker.hover = None;
            true
        }
        Event::Text(value) => {
            if let Some(picker) = app.chat.picker.as_mut() {
                let plain: String = value.chars().filter(|ch| !ch.is_control()).collect();
                if !plain.is_empty() {
                    picker.query.insert_str(&plain);
                    picker.hover = None;
                }
            }
            true
        }
        Event::Paste(value) => {
            if let Some(picker) = app.chat.picker.as_mut() {
                picker.query.insert_str(value);
                picker.hover = None;
            }
            true
        }
        _ => false,
    }
}

// ── Obsługa wejścia ─────────────────────────────────────────────────────────

/// Obsługuje zdarzenie, gdy panel czatu jest widoczny.
///
/// Zwraca `true`, jeśli zdarzenie zostało skonsumowane (edytor go nie widzi).
pub fn handle_events(
    app: &mut App,
    event: &Event,
    lay: &Layout,
    theme: &Theme,
    _text: &TextEngine,
) -> bool {
    if !app.chat.visible || !available(app) {
        return false;
    }
    if app.chat.picker.is_some() && handle_picker(app, event, lay, theme) {
        return true;
    }
    let panel = lay.chat;
    match event {
        Event::PointerMotion { position, .. } => {
            if panel.contains(*position) {
                app.chat.pointer = Some(*position);
                return true;
            }
            app.chat.pointer = None;
            false
        }
        Event::PointerLeave => {
            app.chat.pointer = None;
            false
        }
        Event::Scroll(scroll) => {
            if panel.contains(scroll.position) {
                let max = app.chat.scroll_max;
                app.chat.scroll = (app.chat.scroll + scroll.dy / 20.0 * 64.0).clamp(0.0, max);
                app.chat.stick_to_bottom = false;
                return true;
            }
            false
        }
        Event::PointerButton(button)
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            if !panel.contains(button.position) {
                if app.chat.focused {
                    blur(app);
                }
                return false;
            }
            if lay.chat_close.contains(button.position) {
                app.chat.visible = false;
                blur(app);
                app.prefs.chat_open = false;
                app.prefs_dirty = true;
            } else if lay.chat_clear.contains(button.position) {
                app.chat.clear();
            } else if lay.chat_gear.contains(button.position) {
                if let Some(index) = plugin_index(app) {
                    crate::plugin_ui::open_settings_for(app, index);
                }
            } else if lay.chat_model.contains(button.position) {
                open_model_picker(app);
            } else if lay.chat_send.contains(button.position) {
                send(app);
            } else if lay.chat_input.contains(button.position) {
                focus_input(app);
            } else if lay.chat_messages.contains(button.position) {
                blur(app);
            }
            true
        }
        Event::Key(key) if key.state == ButtonState::Pressed => {
            if !app.chat.focused {
                return false;
            }
            if key.keysym == 0xff1b {
                blur(app);
                return true;
            }
            if key.keysym == 0xff0d || key.keysym == 0xff8d {
                send(app);
                return true;
            }
            let command = clipboard_command(&mut app.chat.input, key);
            if let Some(command) = command {
                match command {
                    TextClipboardCommand::Copy(Some(value))
                    | TextClipboardCommand::Cut(Some(value)) => app.clipboard_text = Some(value),
                    TextClipboardCommand::Paste => app.paste_requested = true,
                    _ => {}
                }
                return true;
            }
            match key.keysym {
                0xff08 => app.chat.input.backspace(),
                0xffff => app.chat.input.delete_forward(),
                0xff51 => app.chat.input.move_left(),
                0xff53 => app.chat.input.move_right(),
                0xff50 => app.chat.input.home(),
                0xff57 => app.chat.input.end(),
                0x61 if key.modifiers.ctrl => app.chat.input.select_all(),
                _ => insert_key_text(&mut app.chat.input, key),
            }
            true
        }
        Event::Text(value) => {
            if !app.chat.focused {
                return false;
            }
            let plain: String = value.chars().filter(|ch| !ch.is_control()).collect();
            if !plain.is_empty() {
                app.chat.input.insert_str(&plain);
            }
            true
        }
        Event::Paste(value) => {
            if !app.chat.focused {
                return false;
            }
            app.chat.input.insert_str(value);
            true
        }
        _ => false,
    }
}

fn insert_key_text(entry: &mut Entry, key: &KeyEvent) {
    if key.modifiers.ctrl || key.modifiers.alt {
        return;
    }
    if let Some(value) = &key.text {
        let plain: String = value.chars().filter(|ch| !ch.is_control()).collect();
        if !plain.is_empty() {
            entry.insert_str(&plain);
        }
    }
}

/// Wymiary panelu przy danej szerokości okna.
pub fn panel_width(size: Size) -> f64 {
    PANEL_W.min((size.width * 0.42).max(264.0))
}

// Marginalia używane przez `ui::layout`.
pub fn header_height() -> f64 {
    HEADER_H
}

pub fn input_height() -> f64 {
    INPUT_H
}

pub fn padding() -> f64 {
    PAD
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "luna-chat-{}-{}-{}",
            std::process::id(),
            name,
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("README.md"), "# Projekt\n").unwrap();
        std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
        dir
    }

    #[test]
    fn plain_answers_and_json_answers_are_understood() {
        let plain = Reply::parse("Gotowe, opis w README.");
        assert_eq!(plain.text, "Gotowe, opis w README.");
        assert!(plain.tool_calls.is_empty());

        let json = Reply::parse(
            r#"{"text":"Sprawdzam","tool_calls":[{"tool":"read_file","path":"src/main.rs"}]}"#,
        );
        assert_eq!(json.text, "Sprawdzam");
        assert_eq!(json.tool_calls.len(), 1);
        assert_eq!(json.tool_calls[0].tool, "read_file");
        assert_eq!(json.tool_calls[0].path, "src/main.rs");

        let fenced = Reply::parse("```json\n{\"text\":\"OK\"}\n```");
        assert_eq!(fenced.text, "OK");
    }

    #[test]
    fn tool_calls_accept_nested_args() {
        let reply = Reply::parse(
            r#"{"tool_calls":[{"name":"write_file","args":{"path":"a.txt","content":"x"}}]}"#,
        );
        assert_eq!(reply.tool_calls[0].tool, "write_file");
        assert_eq!(reply.tool_calls[0].path, "a.txt");
        assert_eq!(reply.tool_calls[0].content, "x");
    }

    #[test]
    fn paths_cannot_escape_the_project() {
        let root = Path::new("/tmp/projekt");
        assert_eq!(
            resolve(root, "src/main.rs").unwrap(),
            PathBuf::from("/tmp/projekt/src/main.rs")
        );
        assert!(resolve(root, "../etc/passwd").is_err());
        assert!(resolve(root, "/etc/passwd").is_err());
        assert!(resolve(root, "").is_err());
    }

    #[test]
    fn tools_read_write_search_and_delete() {
        let root = temp_dir("tools");
        let open = HashMap::new();

        let read = execute_tool(
            &root,
            &open,
            &ToolCall {
                tool: "read_file".to_owned(),
                path: "src/main.rs".to_owned(),
                ..ToolCall::default()
            },
        );
        assert!(read.message.contains("fn main"));

        let write = execute_tool(
            &root,
            &open,
            &ToolCall {
                tool: "write_file".to_owned(),
                path: "src/lib.rs".to_owned(),
                content: "pub fn hello() {}\n".to_owned(),
                ..ToolCall::default()
            },
        );
        assert_eq!(write.written, Some(root.join("src/lib.rs")));
        assert!(root.join("src/lib.rs").exists());

        let created = execute_tool(
            &root,
            &open,
            &ToolCall {
                tool: "create_file".to_owned(),
                path: "src/lib.rs".to_owned(),
                content: "x".to_owned(),
                ..ToolCall::default()
            },
        );
        assert!(created.message.contains("ERROR"));

        let search = execute_tool(
            &root,
            &open,
            &ToolCall {
                tool: "search".to_owned(),
                query: "hello".to_owned(),
                ..ToolCall::default()
            },
        );
        assert!(search.message.contains("src/lib.rs"));

        let list = execute_tool(
            &root,
            &open,
            &ToolCall {
                tool: "list_dir".to_owned(),
                path: String::new(),
                ..ToolCall::default()
            },
        );
        assert!(list.message.contains("README.md"));
        assert!(list.message.contains("src/"));

        let removed = execute_tool(
            &root,
            &open,
            &ToolCall {
                tool: "delete_file".to_owned(),
                path: "src/lib.rs".to_owned(),
                ..ToolCall::default()
            },
        );
        assert!(removed.written.is_some());
        assert!(!root.join("src/lib.rs").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn agent_loop_executes_tools_and_reports_writes() {
        let root = temp_dir("loop");
        let dir =
            std::env::temp_dir().join(format!("luna-chat-plugin-{}-loop", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("run.sh"),
            r#"#!/bin/sh
payload=$(cat)
case "$payload" in
  *\[write_file*) printf '{"text":"Gotowe"}' ;;
  *) printf '{"text":"Zapisuję","tool_calls":[{"tool":"write_file","path":"OUT.txt","content":"treść"}]}' ;;
esac
"#,
        )
        .unwrap();
        let plugin = Plugin {
            dir: dir.clone(),
            id: "loop".into(),
            name: "Loop".into(),
            version: "1".into(),
            author: String::new(),
            description: String::new(),
            entry: "run.sh".into(),
            interpreter: Some("sh".into()),
            commands: Vec::new(),
            settings: Vec::new(),
            chat: true,
        };
        let turn = Turn {
            plugin,
            settings: HashMap::new(),
            root: root.clone(),
            files: Vec::new(),
            inline: Vec::new(),
            active: None,
            open: HashMap::new(),
            messages: vec![Message {
                role: Role::User,
                text: "zapisz plik".into(),
            }],
        };
        let (sender, receiver) = mpsc::channel();
        run_turn(turn, sender);
        let mut reply = None;
        while let Ok(event) = receiver.recv() {
            if let ChatEvent::Reply { text, written } = event {
                reply = Some((text, written));
            }
        }
        let (text, written) = reply.expect("brak odpowiedzi agenta");
        assert_eq!(text, "Gotowe");
        assert_eq!(written, vec![root.join("OUT.txt")]);
        assert!(root.join("OUT.txt").exists());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn chat_plugin(dir: &Path) -> Plugin {
        Plugin {
            dir: dir.to_path_buf(),
            id: "ai-test".into(),
            name: "AI Test".into(),
            version: "1".into(),
            author: String::new(),
            description: String::new(),
            entry: "run.sh".into(),
            interpreter: Some("sh".into()),
            commands: Vec::new(),
            settings: vec![
                plugins::PluginSetting {
                    key: "model".into(),
                    label: "Model".into(),
                    kind: plugins::SettingKind::Models,
                    default: "m1".into(),
                },
                plugins::PluginSetting {
                    key: "api_key".into(),
                    label: "Klucz API".into(),
                    kind: plugins::SettingKind::Secret,
                    default: String::new(),
                },
            ],
            chat: true,
        }
    }

    #[test]
    fn model_setting_and_filtering_work() {
        let dir = std::env::temp_dir().join("luna-chat-model-plugin");
        let plugin = chat_plugin(&dir);
        assert_eq!(models_key(&plugin).as_deref(), Some("model"));
        let models = vec![
            "gpt-4o".to_owned(),
            "gpt-4o-mini".to_owned(),
            "claude-3".to_owned(),
        ];
        assert_eq!(filtered_models(&models, "").len(), 3);
        assert_eq!(filtered_models(&models, "gpt").len(), 2);
        assert_eq!(filtered_models(&models, "CLAUDE"), vec!["claude-3"]);
        assert!(filtered_models(&models, "nic").is_empty());
    }

    #[test]
    fn applying_a_model_writes_the_plugin_config() {
        let base = std::env::temp_dir().join(format!(
            "luna-chat-config-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let previous = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", &base);

        let mut app = App::new();
        app.plugins.plugins = vec![chat_plugin(
            &std::env::temp_dir().join("luna-chat-plugin-dir"),
        )];
        apply_model(&mut app, "model-2");
        assert_eq!(
            app.plugins.plugins[0].values().get("model"),
            Some(&"model-2".to_string())
        );
        assert!(plugins::config_dir().join("ai-test.conf").exists());

        match previous {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn dirty_documents_block_writes() {
        let root = temp_dir("dirty");
        let mut open = HashMap::new();
        open.insert(root.join("README.md"), true);
        let outcome = execute_tool(
            &root,
            &open,
            &ToolCall {
                tool: "write_file".to_owned(),
                path: "README.md".to_owned(),
                content: "nowa treść".to_owned(),
                ..ToolCall::default()
            },
        );
        assert!(outcome.written.is_none());
        assert!(outcome.message.contains("ERROR"));
        assert_eq!(
            std::fs::read_to_string(root.join("README.md")).unwrap(),
            "# Projekt\n"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn project_snapshot_lists_files_and_inlines_small_ones() {
        let root = temp_dir("snapshot");
        let files = collect_files(&root);
        assert!(files.iter().any(|(path, _)| path == "README.md"));
        assert!(files.iter().any(|(path, _)| path == "src/main.rs"));
        let inline = inline_files(&root, None);
        assert!(inline
            .iter()
            .any(|(path, text)| path == "README.md" && text.contains("Projekt")));
        let _ = std::fs::remove_dir_all(&root);
    }
}
