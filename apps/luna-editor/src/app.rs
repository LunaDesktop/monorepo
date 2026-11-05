//! Stan edytora: wiele zakładek, workspace, wbudowany manager plików i dialogi.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use libluna_core::{Point, Rect, ThemeSettings, WindowEvent};
use libluna_i18n::{set_locale, tr};
use libluna_platform::{run as run_app, AppConfig, Frame};
use libluna_widgets::{
    AlertDialog, CodeView, Entry, Menu, MenuEntry, SearchEntry, Tab, TabBar, WindowControl,
};

use crate::browser::{BrowserMode, BrowserOutcome, FileBrowser};
use crate::document::{find_all, Document, LineEnding};
use crate::plugin_ui::PluginUi;
use crate::plugins::{self, Job, Output, RunRequest};
use crate::prefs::Prefs;
use crate::syntax::Language;
use crate::workspace::{Row, Workspace};

/// Co zrobić po udanym zapisie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AfterSave {
    CloseTab(usize),
    CloseWindow,
}

/// Potwierdzenie zapisu zmian.
#[derive(Debug, Clone)]
pub struct Confirm {
    pub after: AfterSave,
    pub dialog: AlertDialog,
    /// Czy okno oferuje „Zapisz” (trzy przyciski). Gdy któregoś pliku nie da
    /// się zapisać bez ścieżki, zostają tylko „Odrzuć” i „Anuluj”.
    pub can_save: bool,
}

/// Okno „Idź do linii”.
#[derive(Debug, Clone)]
pub struct GoToDialog {
    pub entry: Entry,
    pub error: Option<String>,
}

/// Rodzaj okna z polem nazwy w panelu projektu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    NewFile,
    NewFolder,
    Rename,
}

impl PromptKind {
    pub fn title(self) -> String {
        match self {
            Self::NewFile => tr("New file"),
            Self::NewFolder => tr("New folder"),
            Self::Rename => tr("Rename"),
        }
    }

    pub fn placeholder(self) -> String {
        match self {
            Self::NewFile => tr("File name"),
            Self::NewFolder => tr("Folder name"),
            Self::Rename => tr("New name"),
        }
    }

    pub fn submit_label(self) -> String {
        match self {
            Self::Rename => tr("Rename"),
            _ => tr("Create"),
        }
    }
}

/// Okno nazwy nowego pliku/folderu albo zmiany nazwy.
#[derive(Debug, Clone)]
pub struct Prompt {
    pub kind: PromptKind,
    /// Katalog nowego elementu albo ścieżka elementu do zmiany nazwy.
    pub target: PathBuf,
    pub entry: Entry,
    pub error: Option<String>,
}

/// Menu kontekstowe drzewa projektu.
#[derive(Debug, Clone)]
pub struct TreeMenu {
    pub path: PathBuf,
    pub anchor: Rect,
    pub menu: Menu,
}

/// Indeksy akcji w menu kontekstowym drzewa.
pub const TREE_OPEN: usize = 0;
pub const TREE_NEW_FILE: usize = 2;
pub const TREE_NEW_FOLDER: usize = 3;
pub const TREE_RENAME: usize = 5;
pub const TREE_DELETE: usize = 6;
pub const TREE_REFRESH: usize = 8;

/// Stan wyszukiwania.
pub struct FindState {
    pub entry: SearchEntry,
    pub matches: Vec<(usize, usize)>,
    pub index: usize,
}

/// Zmiany do zastosowania na `Frame` po obsłudze zdarzeń.
#[derive(Debug, Default)]
pub struct Pending {
    pub close: bool,
    pub minimize: bool,
    pub maximize: bool,
    pub begin_move: bool,
}

/// Stan aplikacji.
pub struct App {
    pub docs: Vec<Document>,
    pub active: usize,
    pub tabs: TabBar,
    pub workspace: Workspace,
    pub browser: Option<FileBrowser>,
    pub goto: Option<GoToDialog>,
    pub prompt: Option<Prompt>,
    pub tree_menu: Option<TreeMenu>,
    pub pending_delete: Option<(PathBuf, AlertDialog)>,
    pub settings_open: bool,
    pub prefs: Prefs,
    prefs_dirty: bool,
    pub find: Option<FindState>,
    pub confirm: Option<Confirm>,
    pub overwrite: Option<(PathBuf, AlertDialog)>,
    pub error: Option<AlertDialog>,
    pub after_save: Option<AfterSave>,
    pub status: Option<String>,
    pub menu: Menu,
    pub menu_open: bool,
    /// Mapowanie pozycji menu na polecenia wtyczek (indeks wtyczki, indeks polecenia).
    menu_map: Vec<Option<(usize, usize)>>,
    pub hover_control: Option<WindowControl>,
    pub hover_menu: bool,
    pub hover_workspace_toggle: bool,
    pub hover_workspace_row: Option<usize>,
    pub hover_workspace_open: bool,
    /// Ostatnia pozycja kursora — podświetlenie przycisków w dialogach.
    pub pointer: Option<Point>,
    pub pending: Pending,
    pub reveal_cursor: bool,
    pub clipboard_text: Option<String>,
    pub paste_requested: bool,
    pub dragging_selection: bool,
    pub drag_anchor: usize,
    pub workspace_focused: bool,
    last_workspace_click: Option<(Instant, PathBuf)>,
    /// Zainstalowane wtyczki i praca w tle.
    pub plugins: plugins::PluginHost,
    /// Stan okien wtyczek (menedżer, ustawienia, pytanie o instrukcję).
    pub plugin_ui: PluginUi,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self::with_prefs(Prefs::default())
    }

    /// Tworzy aplikację z wczytanymi preferencjami (używane przez `run`).
    pub fn with_prefs(prefs: Prefs) -> Self {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let docs = vec![Document::new()];
        let mut workspace = Workspace::new(root);
        workspace.configure(prefs.show_hidden, prefs.skip_build_dirs);
        workspace.visible = prefs.show_workspace;
        let mut app = Self {
            docs,
            active: 0,
            tabs: TabBar::default(),
            workspace,
            browser: None,
            goto: None,
            prompt: None,
            tree_menu: None,
            pending_delete: None,
            settings_open: false,
            prefs,
            prefs_dirty: false,
            find: None,
            confirm: None,
            overwrite: None,
            error: None,
            after_save: None,
            status: None,
            menu: Menu::new(Vec::new()),
            menu_open: false,
            menu_map: Vec::new(),
            hover_control: None,
            hover_menu: false,
            hover_workspace_toggle: false,
            hover_workspace_row: None,
            hover_workspace_open: false,
            pointer: None,
            pending: Pending::default(),
            reveal_cursor: true,
            clipboard_text: None,
            paste_requested: false,
            dragging_selection: false,
            drag_anchor: 0,
            workspace_focused: false,
            last_workspace_click: None,
            plugins: plugins::PluginHost::new(),
            plugin_ui: PluginUi::new(),
        };
        app.docs[0].view.set_tab_width(app.prefs.tab_width);
        app.rebuild_menu();
        app.activate_active();
        app
    }

    /// Dokument z tekstem (używany przez `editor-render`).
    pub fn with_text(path: Option<PathBuf>, text: &str) -> Self {
        let mut app = Self::new();
        let mut document = Document::with_text(path, text);
        document.view.set_tab_width(app.prefs.tab_width);
        app.docs[0] = document;
        app.activate_active();
        app
    }

    // ── Dostęp do aktywnego dokumentu ──────────────────────────────────────

    pub fn doc(&self) -> &Document {
        &self.docs[self.active]
    }

    pub fn doc_mut(&mut self) -> &mut Document {
        &mut self.docs[self.active]
    }

    pub fn view(&self) -> &CodeView {
        &self.doc().view
    }

    pub fn view_mut(&mut self) -> &mut CodeView {
        &mut self.doc_mut().view
    }

    pub fn dirty(&self) -> bool {
        self.doc().dirty
    }

    pub fn language(&self) -> Language {
        self.doc().language
    }

    pub fn line_ending(&self) -> LineEnding {
        self.doc().line_ending
    }

    pub fn path(&self) -> Option<&Path> {
        self.doc().path.as_deref()
    }

    pub fn file_label(&self) -> String {
        self.doc().file_label()
    }

    pub fn window_title(&self) -> String {
        let doc = self.doc();
        if doc.dirty {
            format!("● {} — Luna Editor", doc.name())
        } else {
            format!("{} — Luna Editor", doc.name())
        }
    }

    /// Odświeża etykiety zakładek po zmianie nazwy/stanu.
    pub fn sync_tabs(&mut self) {
        let titles: Vec<String> = self.docs.iter().map(Document::tab_title).collect();
        let same = self.tabs.len() == titles.len()
            && self
                .tabs
                .tabs()
                .iter()
                .zip(&titles)
                .all(|(tab, title)| &tab.title == title);
        if !same {
            let tabs = titles
                .into_iter()
                .map(|title| Tab::new(title).closable(true))
                .collect();
            let mut bar = TabBar::with_tabs(tabs);
            bar.set_selected(self.active);
            self.tabs = bar;
        } else {
            self.tabs.set_selected(self.active);
        }
    }

    fn activate_active(&mut self) {
        for (index, doc) in self.docs.iter_mut().enumerate() {
            doc.view.set_focused(index == self.active);
        }
        self.sync_tabs();
        self.reveal_cursor = true;
        if self.find.is_some() {
            self.refresh_find();
        }
    }

    pub fn select_tab(&mut self, index: usize) {
        if index >= self.docs.len() || index == self.active {
            return;
        }
        self.active = index;
        self.activate_active();
    }

    pub fn select_next_tab(&mut self) {
        if self.docs.len() > 1 {
            self.select_tab((self.active + 1) % self.docs.len());
        }
    }

    pub fn select_prev_tab(&mut self) {
        if self.docs.len() > 1 {
            self.select_tab((self.active + self.docs.len() - 1) % self.docs.len());
        }
    }

    /// Dodaje nowy, pusty dokument jako zakładkę.
    pub fn new_file(&mut self) {
        let mut document = Document::new();
        document.view.set_tab_width(self.prefs.tab_width);
        self.docs.push(document);
        self.active = self.docs.len() - 1;
        self.workspace_focused = false;
        self.status = Some(tr("New file"));
        self.activate_active();
    }

    /// Otwiera plik w nowej zakładce (albo przełącza istniejącą).
    pub fn open_path(&mut self, path: PathBuf) {
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if let Some(index) = self
            .docs
            .iter()
            .position(|doc| doc.path.as_deref() == Some(path.as_path()))
        {
            self.select_tab(index);
            return;
        }
        match Document::load(path) {
            Ok(mut document) => {
                document.view.set_tab_width(self.prefs.tab_width);
                // Otwarcie pliku poza projektem przenosi panel na jego katalog.
                if let Some(opened) = document.path.as_deref() {
                    let root = std::fs::canonicalize(&self.workspace.root)
                        .unwrap_or_else(|_| self.workspace.root.clone());
                    if !opened.starts_with(&root) {
                        if let Some(parent) = opened.parent() {
                            self.workspace.set_root(parent.to_path_buf());
                        }
                    }
                }
                if self.doc().is_empty_untitled() {
                    self.docs[self.active] = document;
                } else {
                    self.docs.push(document);
                    self.active = self.docs.len() - 1;
                }
                self.status = Some(tr("Opened"));
                self.workspace_focused = false;
                self.workspace.refresh();
                let opened = self.doc().path.clone();
                if let Some(path) = opened {
                    self.workspace.reveal(&path);
                }
                self.activate_active();
            }
            Err(error) => self.show_error(error),
        }
    }

    pub fn close_tab(&mut self, index: usize) {
        let Some(doc) = self.docs.get(index) else {
            return;
        };
        if doc.dirty {
            self.active = index;
            self.activate_active();
            self.confirm = Some(Confirm {
                after: AfterSave::CloseTab(index),
                dialog: save_confirm(tr("Save changes before closing the tab?")),
                can_save: true,
            });
        } else {
            self.remove_tab(index);
        }
    }

    fn remove_tab(&mut self, index: usize) {
        if index >= self.docs.len() {
            return;
        }
        self.docs.remove(index);
        if self.docs.is_empty() {
            self.pending.close = true;
            return;
        }
        if self.active > index {
            self.active -= 1;
        } else if self.active >= self.docs.len() {
            self.active = self.docs.len() - 1;
        }
        self.activate_active();
    }

    pub fn request_close(&mut self) {
        let dirty = self.docs.iter().filter(|doc| doc.dirty).count();
        if dirty == 0 {
            self.pending.close = true;
            return;
        }
        let all_have_paths = self
            .docs
            .iter()
            .filter(|doc| doc.dirty)
            .all(|doc| doc.path.is_some());
        let dialog = if all_have_paths {
            save_confirm(tr("Save changes before closing?"))
        } else {
            AlertDialog::new(
                tr("Unsaved changes"),
                tr("Some files have no path. Unsaved changes will be lost."),
            )
            .button(tr("Discard"))
            .button(tr("Cancel"))
        };
        self.confirm = Some(Confirm {
            after: AfterSave::CloseWindow,
            dialog,
            can_save: all_have_paths,
        });
    }

    pub fn answer_confirm(&mut self, action: usize) {
        let Some(confirm) = self.confirm.take() else {
            return;
        };
        let can_save = confirm.can_save;
        match confirm.after {
            AfterSave::CloseTab(index) => match action {
                0 => {
                    self.after_save = Some(AfterSave::CloseTab(index));
                    self.save();
                }
                1 => self.remove_tab(index),
                _ => {}
            },
            AfterSave::CloseWindow => {
                if can_save {
                    match action {
                        0 => self.save_all(),
                        1 => self.pending.close = true,
                        _ => {}
                    }
                } else if action == 0 {
                    // Okno ma tylko „Odrzuć” i „Anuluj” — nie próbuj zapisywać
                    // plików bez ścieżki.
                    self.pending.close = true;
                }
            }
        }
    }

    fn save_all(&mut self) {
        for doc in &mut self.docs {
            if !doc.dirty {
                continue;
            }
            if let Err(error) = doc.save() {
                self.show_error(error);
                return;
            }
        }
        self.status = Some(tr("File saved"));
        self.sync_tabs();
        self.workspace.refresh();
        self.perform_after_save(AfterSave::CloseWindow);
    }

    // ── Edycja ─────────────────────────────────────────────────────────────

    fn after_edit(&mut self) {
        self.reveal_cursor = true;
        self.sync_tabs();
    }

    pub fn insert_text(&mut self, text: &str) {
        self.doc_mut().insert_text(text);
        self.after_edit();
    }

    pub fn newline(&mut self) {
        if self.prefs.auto_indent {
            self.doc_mut().newline();
        } else {
            self.doc_mut().insert_text("\n");
        }
        self.after_edit();
    }

    pub fn backspace(&mut self) {
        self.doc_mut().backspace();
        self.after_edit();
    }

    pub fn delete_forward(&mut self) {
        self.doc_mut().delete_forward();
        self.after_edit();
    }

    pub fn tab(&mut self, unindent: bool) {
        self.doc_mut().tab(unindent);
        self.after_edit();
    }

    pub fn undo(&mut self) {
        self.doc_mut().undo();
        self.after_edit();
    }

    pub fn redo(&mut self) {
        self.doc_mut().redo();
        self.after_edit();
    }

    pub fn copy(&mut self) {
        if let Some(text) = self.doc().copy() {
            self.clipboard_text = Some(text);
            self.status = Some(tr("Copied"));
        }
    }

    pub fn cut(&mut self) {
        if let Some(text) = self.doc_mut().cut() {
            self.clipboard_text = Some(text);
            self.status = Some(tr("Cut"));
            self.after_edit();
        }
    }

    pub fn paste_text(&mut self, text: &str) {
        self.insert_text(text);
    }

    // ── Zapis i menedżer plików ────────────────────────────────────────────

    pub fn save(&mut self) {
        if self.doc().path.is_none() {
            self.open_save_browser();
            return;
        }
        let result = self.doc_mut().save();
        match result {
            Ok(()) => self.saved(),
            Err(error) => self.show_error(error),
        }
    }

    fn saved(&mut self) {
        self.status = Some(tr("File saved"));
        self.workspace_focused = false;
        self.sync_tabs();
        self.workspace.refresh();
        if let Some(after) = self.after_save.take() {
            self.perform_after_save(after);
        }
    }

    fn perform_after_save(&mut self, after: AfterSave) {
        match after {
            AfterSave::CloseTab(index) => self.remove_tab(index),
            AfterSave::CloseWindow => self.pending.close = true,
        }
    }

    pub fn open_open_browser(&mut self) {
        let start = self
            .doc()
            .path
            .clone()
            .or_else(|| Some(self.workspace.root.clone()))
            .unwrap_or_else(|| self.workspace.root.clone());
        self.browser = Some(FileBrowser::new(BrowserMode::Open, start));
    }

    pub fn open_save_browser(&mut self) {
        let start = self
            .doc()
            .path
            .clone()
            .unwrap_or_else(|| self.workspace.root.join("untitled.txt"));
        self.browser = Some(FileBrowser::new(BrowserMode::Save, start));
    }

    pub fn open_folder_browser(&mut self) {
        self.browser = Some(FileBrowser::new(
            BrowserMode::Folder,
            self.workspace.root.clone(),
        ));
    }

    pub fn submit_browser(&mut self) {
        let outcome = self.browser.as_mut().and_then(FileBrowser::submit);
        if let Some(outcome) = outcome {
            self.apply_browser_outcome(outcome);
        }
    }

    pub fn apply_browser_outcome(&mut self, outcome: BrowserOutcome) {
        self.browser = None;
        match outcome {
            BrowserOutcome::Open(path) => self.open_path(path),
            BrowserOutcome::Folder(path) => {
                self.workspace.set_root(path);
                self.workspace.visible = true;
                self.status = Some(tr("Workspace folder changed"));
            }
            BrowserOutcome::Save(path) => {
                if path.exists() {
                    self.overwrite = Some((
                        path,
                        AlertDialog::new(tr("File exists"), tr("Overwrite the existing file?"))
                            .button(tr("Overwrite"))
                            .button(tr("Cancel")),
                    ));
                } else {
                    self.save_as(path);
                }
            }
        }
    }

    pub fn answer_overwrite(&mut self, overwrite: bool) {
        let Some((path, _)) = self.overwrite.take() else {
            return;
        };
        if overwrite {
            self.save_as(path);
        } else {
            self.after_save = None;
        }
    }

    fn save_as(&mut self, path: PathBuf) {
        let result = self.doc_mut().save_as(path);
        match result {
            Ok(()) => self.saved(),
            Err(error) => self.show_error(error),
        }
    }

    // ── Workspace ──────────────────────────────────────────────────────────

    pub fn toggle_workspace(&mut self) {
        self.workspace.toggle_visible();
        self.prefs.show_workspace = self.workspace.visible;
        self.prefs_dirty = true;
        if !self.workspace.visible {
            self.workspace_focused = false;
            self.tree_menu = None;
        }
    }

    /// Otwiera menu kontekstowe dla wiersza drzewa. `anchor` to prostokąt wiersza.
    pub fn open_tree_menu(&mut self, index: usize, anchor: Rect) {
        let Some(row) = self.workspace.rows.get(index).cloned() else {
            return;
        };
        self.workspace.select(row.path.clone());
        self.workspace_focused = true;
        self.menu_open = false;
        self.tree_menu = Some(TreeMenu {
            path: row.path.clone(),
            anchor,
            menu: workspace_menu(&row),
        });
    }

    /// Wykonuje akcję menu kontekstowego drzewa.
    pub fn tree_menu_action(&mut self, action: usize) {
        let Some(menu) = self.tree_menu.take() else {
            return;
        };
        let path = menu.path;
        match action {
            TREE_OPEN => {
                if path.is_dir() {
                    self.workspace.toggle_dir(&path);
                } else {
                    self.open_path(path);
                }
            }
            TREE_NEW_FILE => self.begin_prompt(PromptKind::NewFile, self.workspace.target_dir()),
            TREE_NEW_FOLDER => {
                self.begin_prompt(PromptKind::NewFolder, self.workspace.target_dir())
            }
            TREE_RENAME => self.begin_prompt(PromptKind::Rename, path),
            TREE_DELETE => self.request_delete(path),
            TREE_REFRESH => self.workspace.refresh(),
            _ => {}
        }
        self.workspace_focused = true;
    }

    /// Otwiera okno nazwy dla nowego pliku/folderu albo zmiany nazwy.
    pub fn begin_prompt(&mut self, kind: PromptKind, target: PathBuf) {
        let mut entry = Entry::new(kind.placeholder());
        if kind == PromptKind::Rename {
            if let Some(name) = target.file_name().and_then(|name| name.to_str()) {
                entry.set_text(name);
                entry.select_all();
            }
        }
        entry.set_focused(true);
        self.prompt = Some(Prompt {
            kind,
            target,
            entry,
            error: None,
        });
        self.tree_menu = None;
    }

    pub fn cancel_prompt(&mut self) {
        self.prompt = None;
        self.focus_editor();
    }

    /// Przywraca fokus edytorowi (zdejmuje go z drzewa projektu).
    pub fn focus_editor(&mut self) {
        self.workspace_focused = false;
        self.view_mut().set_focused(true);
    }

    fn set_prompt_error(&mut self, message: String) {
        if let Some(prompt) = self.prompt.as_mut() {
            prompt.error = Some(message);
        }
    }

    /// Zatwierdza okno nazwy: tworzy plik/folder albo zmienia nazwę.
    pub fn submit_prompt(&mut self) {
        let Some(prompt) = self.prompt.as_ref() else {
            return;
        };
        let value = prompt.entry.text().trim().to_owned();
        if value.is_empty() {
            self.set_prompt_error(tr("Enter a name"));
            return;
        }
        if value == "." || value == ".." || value.contains('/') || value.contains('\\') {
            self.set_prompt_error(tr("Name cannot contain / or \\"));
            return;
        }
        let kind = prompt.kind;
        let target = prompt.target.clone();
        let result = match kind {
            PromptKind::NewFile => {
                let path = target.join(&value);
                if path.exists() {
                    Err(tr("Already exists"))
                } else {
                    std::fs::write(&path, "")
                        .map(|()| path)
                        .map_err(|error| error.to_string())
                }
            }
            PromptKind::NewFolder => {
                let path = target.join(&value);
                if path.exists() {
                    Err(tr("Already exists"))
                } else {
                    std::fs::create_dir(&path)
                        .map(|()| path)
                        .map_err(|error| error.to_string())
                }
            }
            PromptKind::Rename => {
                let parent = target
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| self.workspace.root.clone());
                let path = parent.join(&value);
                if path == target {
                    Ok(target.clone())
                } else if path.exists() {
                    Err(tr("Already exists"))
                } else {
                    std::fs::rename(&target, &path)
                        .map(|()| path)
                        .map_err(|error| error.to_string())
                }
            }
        };
        match result {
            Ok(path) => {
                self.prompt = None;
                self.workspace.refresh();
                match kind {
                    PromptKind::NewFile => {
                        self.open_path(path);
                        self.workspace_focused = false;
                        self.status = Some(tr("File created"));
                    }
                    PromptKind::NewFolder => {
                        self.workspace.focus_dir(&path);
                        self.status = Some(tr("Folder created"));
                    }
                    PromptKind::Rename => {
                        self.retarget_documents(&target, &path);
                        self.workspace.reveal(&path);
                        self.status = Some(tr("Renamed"));
                    }
                }
            }
            Err(error) => self.set_prompt_error(error),
        }
    }

    /// Przenosi otwarte dokumenty po zmianie nazwy pliku lub katalogu.
    fn retarget_documents(&mut self, old: &Path, new: &Path) {
        for doc in &mut self.docs {
            let Some(path) = doc.path.as_ref() else {
                continue;
            };
            let updated = if path == old {
                Some(new.to_path_buf())
            } else if path.starts_with(old) {
                path.strip_prefix(old).ok().map(|rest| new.join(rest))
            } else {
                None
            };
            if let Some(updated) = updated {
                doc.path = Some(updated.clone());
                doc.language = Language::from_path(&updated);
                doc.rehighlight();
            }
        }
        self.sync_tabs();
    }

    pub fn rename_selected(&mut self) {
        if let Some(path) = self.workspace.selected.clone() {
            if path != self.workspace.root {
                self.begin_prompt(PromptKind::Rename, path);
            }
        }
    }

    pub fn request_delete(&mut self, path: PathBuf) {
        if path == self.workspace.root {
            return;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        let body = format!(
            "{}\n\n{}",
            name,
            tr("The item and any open tabs will be closed.")
        );
        self.pending_delete = Some((
            path,
            AlertDialog::new(tr("Delete"), body)
                .button(tr("Delete"))
                .button(tr("Cancel")),
        ));
    }

    pub fn delete_selected(&mut self) {
        if let Some(path) = self.workspace.selected.clone() {
            self.request_delete(path);
        }
    }

    pub fn answer_delete(&mut self, delete: bool) {
        let Some((path, _)) = self.pending_delete.take() else {
            return;
        };
        if !delete {
            return;
        }
        let result = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        match result {
            Ok(()) => {
                self.docs.retain(|doc| {
                    !doc.path
                        .as_deref()
                        .is_some_and(|open| open.starts_with(&path))
                });
                if self.docs.is_empty() {
                    self.docs.push(Document::new());
                }
                self.active = self.active.min(self.docs.len() - 1);
                self.activate_active();
                self.workspace.selected = None;
                self.workspace.preview = None;
                self.workspace.refresh();
                self.workspace.scroll = 0.0;
                self.status = Some(tr("Deleted"));
            }
            Err(error) => self.show_error(error.to_string()),
        }
    }

    pub fn workspace_row_clicked(&mut self, index: usize) {
        let Some(row) = self.workspace.rows.get(index).cloned() else {
            return;
        };
        if row.is_dir {
            self.workspace.toggle_dir(&row.path);
            self.last_workspace_click = None;
            return;
        }
        let now = Instant::now();
        let double = self
            .last_workspace_click
            .as_ref()
            .is_some_and(|(at, path)| {
                path == &row.path && now.duration_since(*at) < Duration::from_millis(400)
            });
        self.workspace.select(row.path.clone());
        if double {
            self.last_workspace_click = None;
            self.open_path(row.path);
            self.workspace_focused = false;
        } else {
            self.last_workspace_click = Some((now, row.path));
        }
    }

    pub fn workspace_open_selected(&mut self) {
        if let Some(path) = self.workspace.selected.clone() {
            if !path.is_dir() {
                self.open_path(path);
                self.workspace_focused = false;
            }
        }
    }

    /// Przesuwa zaznaczenie w drzewie o `delta` wierszy.
    pub fn workspace_move(&mut self, delta: isize) {
        if self.workspace.rows.is_empty() {
            return;
        }
        let last = self.workspace.rows.len() as isize - 1;
        let current = self
            .workspace
            .selected_index()
            .map(|index| index as isize)
            .unwrap_or(if delta > 0 { -1 } else { last + 1 });
        let next = (current + delta).clamp(0, last) as usize;
        let path = self.workspace.rows[next].path.clone();
        self.workspace.select(path);
    }

    /// Zaznacza pierwszy albo ostatni wiersz drzewa.
    pub fn workspace_select_edge(&mut self, end: bool) {
        let index = if end {
            self.workspace.rows.len().checked_sub(1)
        } else if self.workspace.rows.is_empty() {
            None
        } else {
            Some(0)
        };
        if let Some(index) = index {
            let path = self.workspace.rows[index].path.clone();
            self.workspace.select(path);
        }
    }

    /// `←`: zwija katalog albo przechodzi do katalogu nadrzędnego.
    pub fn workspace_collapse_or_parent(&mut self) {
        let Some(index) = self.workspace.selected_index() else {
            return;
        };
        let row = self.workspace.rows[index].clone();
        if row.is_dir && row.open {
            self.workspace.toggle_dir(&row.path);
            return;
        }
        if let Some(parent) = row.path.parent() {
            if parent.starts_with(&self.workspace.root) && parent != self.workspace.root {
                self.workspace.select(parent.to_path_buf());
            }
        }
    }

    /// `→`: rozwija katalog albo przechodzi do jego pierwszego dziecka.
    pub fn workspace_expand_or_child(&mut self) {
        let Some(index) = self.workspace.selected_index() else {
            return;
        };
        let row = self.workspace.rows[index].clone();
        if !row.is_dir {
            return;
        }
        if !row.open {
            self.workspace.toggle_dir(&row.path);
            return;
        }
        if let Some(child) = self.workspace.rows.get(index + 1) {
            if child.depth > row.depth {
                let path = child.path.clone();
                self.workspace.select(path);
            }
        }
    }

    /// `Enter`: otwiera plik albo rozwija/zwija katalog.
    pub fn workspace_activate_selected(&mut self) {
        let Some(index) = self.workspace.selected_index() else {
            return;
        };
        let row = self.workspace.rows[index].clone();
        if row.is_dir {
            self.workspace.toggle_dir(&row.path);
        } else {
            self.open_path(row.path);
            self.workspace_focused = false;
        }
    }

    // ── Wyszukiwanie ───────────────────────────────────────────────────────

    pub fn toggle_find(&mut self) {
        if self.find.is_some() {
            self.close_find();
            return;
        }
        let mut entry = SearchEntry::new(tr("Find…"));
        if let Some(text) = self.view().selected_text() {
            if !text.contains('\n') {
                entry.set_text(text);
            }
        }
        entry.set_focused(true);
        self.find = Some(FindState {
            entry,
            matches: Vec::new(),
            index: 0,
        });
        self.refresh_find();
    }

    pub fn close_find(&mut self) {
        self.find = None;
        self.focus_editor();
    }

    pub fn refresh_find(&mut self) {
        let Some(find) = self.find.as_ref() else {
            return;
        };
        let query = find.entry.text().to_owned();
        let matches = if query.is_empty() {
            Vec::new()
        } else {
            find_all(self.view().text(), &query)
        };
        let first = matches.first().copied();
        if let Some(find) = self.find.as_mut() {
            find.matches = matches;
            find.index = 0;
        }
        if let Some((start, end)) = first {
            self.view_mut().set_selection(start, end);
            self.view_mut().set_focused(false);
            self.reveal_cursor = true;
        }
    }

    pub fn find_step(&mut self, forward: bool) {
        let Some(find) = self.find.as_ref() else {
            return;
        };
        if find.matches.is_empty() {
            return;
        }
        let len = find.matches.len();
        let index = if forward {
            (find.index + 1) % len
        } else if find.index == 0 {
            len - 1
        } else {
            find.index - 1
        };
        let (start, end) = find.matches[index];
        if let Some(find) = self.find.as_mut() {
            find.index = index;
        }
        self.view_mut().set_selection(start, end);
        self.view_mut().set_focused(false);
        self.reveal_cursor = true;
    }

    pub fn find_focused(&self) -> bool {
        self.find
            .as_ref()
            .is_some_and(|find| find.entry.is_focused())
    }

    pub fn set_find_focus(&mut self, focused: bool) {
        if let Some(find) = self.find.as_mut() {
            find.entry.set_focused(focused);
        }
    }

    // ── Dialogi i menu ─────────────────────────────────────────────────────

    pub fn open_goto_dialog(&mut self) {
        let mut entry = Entry::new(tr("Line number"));
        entry.set_focused(true);
        self.goto = Some(GoToDialog { entry, error: None });
    }

    pub fn submit_goto(&mut self) {
        let Some(dialog) = self.goto.as_ref() else {
            return;
        };
        let value = dialog.entry.text().trim().to_owned();
        let Ok(line) = value.parse::<usize>() else {
            if let Some(dialog) = self.goto.as_mut() {
                dialog.error = Some(tr("Enter a number"));
            }
            return;
        };
        self.goto = None;
        self.view_mut().goto_line(line);
        self.focus_editor();
        self.reveal_cursor = true;
    }

    pub fn show_error(&mut self, message: String) {
        self.browser = None;
        self.error = Some(AlertDialog::new(tr("Error"), message).button(tr("OK")));
    }

    pub fn show_about(&mut self) {
        let body = format!(
            "Luna Editor {}\n{}\n\n{}\n\n© 2026 Luna Project · GPL-3.0-or-later",
            env!("CARGO_PKG_VERSION"),
            tr("Edit text and source code with syntax highlighting."),
            tr("Rust, TOML, JSON, Markdown, Shell, C/C++, Python, HTML, CSS, JavaScript, PHP, YAML.")
        );
        self.error = Some(AlertDialog::new(tr("About"), body).button(tr("OK")));
    }

    // ── Ustawienia ─────────────────────────────────────────────────────────

    pub fn open_settings(&mut self) {
        self.menu_open = false;
        self.tree_menu = None;
        self.settings_open = true;
    }

    pub fn close_settings(&mut self) {
        self.settings_open = false;
        self.focus_editor();
    }

    pub fn set_tab_width(&mut self, width: usize) {
        self.prefs.tab_width = Prefs::clamp_tab_width(width);
        for doc in &mut self.docs {
            doc.view.set_tab_width(self.prefs.tab_width);
        }
        self.prefs_dirty = true;
    }

    pub fn toggle_auto_indent(&mut self) {
        self.prefs.auto_indent = !self.prefs.auto_indent;
        self.prefs_dirty = true;
    }

    pub fn toggle_show_hidden(&mut self) {
        self.prefs.show_hidden = !self.prefs.show_hidden;
        self.workspace
            .configure(self.prefs.show_hidden, self.prefs.skip_build_dirs);
        self.prefs_dirty = true;
    }

    pub fn toggle_skip_build_dirs(&mut self) {
        self.prefs.skip_build_dirs = !self.prefs.skip_build_dirs;
        self.workspace
            .configure(self.prefs.show_hidden, self.prefs.skip_build_dirs);
        self.prefs_dirty = true;
    }

    pub fn toggle_show_workspace(&mut self) {
        self.prefs.show_workspace = !self.prefs.show_workspace;
        self.workspace.visible = self.prefs.show_workspace;
        self.prefs_dirty = true;
    }

    /// Zapisuje preferencje, jeśli użytkownik coś zmienił.
    pub fn save_prefs_if_dirty(&mut self) {
        if self.prefs_dirty {
            self.prefs_dirty = false;
            self.prefs.save();
        }
    }

    pub fn menu_action(&mut self, index: usize) {
        if let Some(Some((plugin, command))) = self.menu_map.get(index).copied() {
            crate::plugin_ui::start(self, plugin, command);
            return;
        }
        match index {
            0 => self.new_file(),
            1 => self.open_open_browser(),
            2 => self.save(),
            3 => self.open_save_browser(),
            4 => self.close_tab(self.active),
            6 => self.toggle_find(),
            7 => self.open_goto_dialog(),
            9 => self.undo(),
            10 => self.redo(),
            12 => self.toggle_workspace(),
            13 => self.open_folder_browser(),
            15 => self.open_plugins(),
            16 => self.open_settings(),
            17 => self.show_about(),
            _ => {}
        }
    }

    /// Odbudowuje menu: pozycje bazowe + polecenia zainstalowanych wtyczek.
    pub fn rebuild_menu(&mut self) {
        let (entries, map) = menu_entries_with_plugins(&self.plugins.plugins);
        self.menu = Menu::new(entries);
        self.menu_map = map;
    }

    // ── Wtyczki ────────────────────────────────────────────────────────────

    /// Otwiera menedżer wtyczek (odświeża listę z dysku).
    pub fn open_plugins(&mut self) {
        self.menu_open = false;
        self.hover_menu = false;
        self.hover_workspace_toggle = false;
        self.hover_workspace_row = None;
        self.hover_workspace_open = false;
        self.plugins.reload();
        self.rebuild_menu();
        self.plugin_ui.open = true;
        self.plugin_ui.form = None;
        self.plugin_ui.prompt = None;
        self.plugin_ui.status = None;
        if self.plugin_ui.selected >= self.plugins.plugins.len() {
            self.plugin_ui.selected = 0;
        }
    }

    /// Uruchamia polecenie wtyczki w tle i wstawia wynik do dokumentu.
    pub fn start_plugin(&mut self, plugin_index: usize, command_index: usize, instruction: String) {
        let Some(plugin) = self.plugins.plugins.get(plugin_index).cloned() else {
            return;
        };
        let Some(command) = plugin.commands.get(command_index).cloned() else {
            return;
        };
        let settings = plugin.values();
        let (path, language, payload) = crate::plugin_ui::payload_for(self, command.context);
        let receiver = plugins::spawn_run(RunRequest {
            plugin: plugin.clone(),
            command: command.clone(),
            settings,
            instruction,
            path,
            language,
            payload,
        });
        self.plugins.job = Some(Job::Run {
            receiver,
            doc: self.active,
            output: command.output,
            title: command.title.clone(),
        });
        self.plugin_ui.open = false;
        self.plugin_ui.prompt = None;
        self.status = Some(format!("{}: {}", plugin.name, tr("Working…")));
    }

    /// Sprawdza, czy praca w tle (instalacja/uruchomienie) się zakończyła.
    pub fn poll_plugins(&mut self) {
        use std::sync::mpsc::TryRecvError;
        let Some(job) = self.plugins.job.take() else {
            return;
        };
        match job {
            Job::Install(receiver) => match receiver.try_recv() {
                Ok(Ok(summary)) => {
                    self.plugins.reload();
                    self.rebuild_menu();
                    self.plugin_ui.status = Some(summary.clone());
                    self.status = Some(summary);
                    if self.plugin_ui.selected >= self.plugins.plugins.len() {
                        self.plugin_ui.selected = self.plugins.plugins.len().saturating_sub(1);
                    }
                }
                Ok(Err(error)) => {
                    self.plugin_ui.status = Some(error.clone());
                    self.status = None;
                    self.show_error(error);
                }
                Err(TryRecvError::Empty) => {
                    self.plugins.job = Some(Job::Install(receiver));
                }
                Err(TryRecvError::Disconnected) => {
                    self.status = None;
                    self.show_error(tr("Installation ended without a result"));
                }
            },
            Job::Run {
                receiver,
                doc,
                output,
                title,
            } => match receiver.try_recv() {
                Ok(Ok(text)) => {
                    self.apply_plugin_output(doc, output, &text);
                    self.status = Some(format!("{title}: {}", tr("Done")));
                }
                Ok(Err(error)) => {
                    self.status = None;
                    self.show_error(error);
                }
                Err(TryRecvError::Empty) => {
                    self.plugins.job = Some(Job::Run {
                        receiver,
                        doc,
                        output,
                        title,
                    });
                }
                Err(TryRecvError::Disconnected) => {
                    self.status = None;
                    self.show_error(tr("Plugin ended without a result"));
                }
            },
            Job::Models {
                receiver,
                plugin,
                setting,
            } => match receiver.try_recv() {
                Ok(Ok(models)) => {
                    if let Some(form) = self.plugin_ui.form.as_mut() {
                        if form.plugin == plugin && setting < form.options.len() {
                            let previous = form
                                .options
                                .get(setting)
                                .and_then(|options| {
                                    options.get(*form.selections.get(setting).unwrap_or(&0))
                                })
                                .cloned();
                            form.options[setting] = models;
                            form.selections[setting] = previous
                                .and_then(|value| {
                                    form.options[setting]
                                        .iter()
                                        .position(|model| *model == value)
                                })
                                .unwrap_or(0);
                            form.loading[setting] = false;
                            form.needs_refresh[setting] = false;
                            form.pending_open = Some(setting);
                            form.error = None;
                        }
                    }
                    self.status = Some(tr("Models loaded"));
                }
                Ok(Err(error)) => {
                    if let Some(form) = self.plugin_ui.form.as_mut() {
                        if form.plugin == plugin && setting < form.loading.len() {
                            form.loading[setting] = false;
                            form.pending_open = None;
                            form.error = Some(error);
                        }
                    }
                    self.status = None;
                }
                Err(TryRecvError::Empty) => {
                    self.plugins.job = Some(Job::Models {
                        receiver,
                        plugin,
                        setting,
                    });
                }
                Err(TryRecvError::Disconnected) => {
                    self.status = None;
                    if let Some(form) = self.plugin_ui.form.as_mut() {
                        if form.plugin == plugin && setting < form.loading.len() {
                            form.loading[setting] = false;
                            form.pending_open = None;
                            form.error = Some(tr("Models ended without a result"));
                        }
                    }
                }
            },
        }
    }

    fn apply_plugin_output(&mut self, index: usize, output: Output, text: &str) {
        if index >= self.docs.len() {
            return;
        }
        match output {
            Output::Insert | Output::ReplaceSelection => {
                let clean = text.trim_end_matches(|ch| ch == '\r' || ch == '\n');
                self.docs[index].insert_text(clean);
            }
            Output::ReplaceDocument => self.docs[index].replace_all(text),
            Output::Append => self.docs[index].append(text),
        }
        if index == self.active {
            self.after_edit();
        } else {
            self.sync_tabs();
        }
    }

    pub fn apply_pending(&mut self, frame: &mut Frame) {
        if self.pending.close {
            self.pending.close = false;
            frame.close();
        }
        if self.pending.minimize {
            self.pending.minimize = false;
            frame.minimize();
        }
        if self.pending.maximize {
            self.pending.maximize = false;
            frame.toggle_maximize();
        }
        if self.pending.begin_move {
            self.pending.begin_move = false;
            frame.begin_move();
        }
    }

    pub fn handle_window_event(&mut self, event: &WindowEvent) -> bool {
        if let WindowEvent::CloseRequested = event {
            self.request_close();
            return true;
        }
        false
    }
}

fn save_confirm(body: String) -> AlertDialog {
    AlertDialog::new(tr("Unsaved changes"), body)
        .button(tr("Save"))
        .button(tr("Discard"))
        .button(tr("Cancel"))
}

fn workspace_menu(row: &Row) -> Menu {
    let open = if row.is_dir {
        if row.open {
            tr("Collapse")
        } else {
            tr("Expand")
        }
    } else {
        tr("Open")
    };
    Menu::new(vec![
        MenuEntry::action(open),
        MenuEntry::separator(),
        MenuEntry::action(tr("New file")),
        MenuEntry::action(tr("New folder")),
        MenuEntry::separator(),
        MenuEntry::action(tr("Rename")),
        MenuEntry::action(tr("Delete")),
        MenuEntry::separator(),
        MenuEntry::action(tr("Refresh")),
    ])
}

fn menu_entries() -> Vec<MenuEntry> {
    vec![
        MenuEntry::action(tr("New file")).shortcut("Ctrl+N"),
        MenuEntry::action(tr("Open file")).shortcut("Ctrl+O"),
        MenuEntry::action(tr("Save")).shortcut("Ctrl+S"),
        MenuEntry::action(tr("Save as…")).shortcut("Ctrl+Shift+S"),
        MenuEntry::action(tr("Close tab")).shortcut("Ctrl+W"),
        MenuEntry::separator(),
        MenuEntry::action(tr("Find")).shortcut("Ctrl+F"),
        MenuEntry::action(tr("Go to line")).shortcut("Ctrl+G"),
        MenuEntry::separator(),
        MenuEntry::action(tr("Undo")).shortcut("Ctrl+Z"),
        MenuEntry::action(tr("Redo")).shortcut("Ctrl+Shift+Z"),
        MenuEntry::separator(),
        MenuEntry::action(tr("Toggle workspace")).shortcut("Ctrl+B"),
        MenuEntry::action(tr("Open folder")),
        MenuEntry::separator(),
        MenuEntry::action(tr("Plugins…")).shortcut("Ctrl+Shift+P"),
        MenuEntry::action(tr("Settings")).shortcut("Ctrl+,"),
        MenuEntry::action(tr("About")),
    ]
}

/// Menu bazowe + polecenia wtyczek (na końcu, po separatorze).
fn menu_entries_with_plugins(
    plugins: &[crate::plugins::Plugin],
) -> (Vec<MenuEntry>, Vec<Option<(usize, usize)>>) {
    let mut entries = menu_entries();
    let mut map: Vec<Option<(usize, usize)>> = vec![None; entries.len()];
    let with_commands = plugins.iter().any(|plugin| !plugin.commands.is_empty());
    if with_commands {
        entries.push(MenuEntry::separator());
        map.push(None);
    }
    for (plugin_index, plugin) in plugins.iter().enumerate() {
        if plugin.commands.is_empty() {
            continue;
        }
        entries.push(MenuEntry::section(plugin.name.clone()));
        map.push(None);
        for (command_index, command) in plugin.commands.iter().enumerate() {
            entries.push(MenuEntry::action(command.title.clone()));
            map.push(Some((plugin_index, command_index)));
        }
    }
    (entries, map)
}

/// Indeks „Plugins” w menu.
pub const MENU_PLUGINS: usize = 15;

/// Indeks „Settings” w menu.
pub const MENU_SETTINGS: usize = 16;

/// Indeks „About” w menu.
pub const MENU_ABOUT: usize = 17;

// ── Start ───────────────────────────────────────────────────────────────────

/// Uruchamia Luna Editor.
pub fn run() -> Result<(), Box<dyn Error>> {
    set_locale(libluna_i18n::preferred_locale());
    libluna_i18n::load_catalogs_from_dirs();

    let mut app = App::with_prefs(Prefs::load());
    apply_cli_args(&mut app);
    let mut ui_language = libluna_i18n::locale().language().to_owned();
    let settings = ThemeSettings::load();
    let system_dark = std::env::var("LUNA_SYSTEM_DARK")
        .map(|value| !value.is_empty() && value != "0")
        .unwrap_or(false);

    let config = AppConfig::new("Luna Editor")
        .app_id("eu.luna.Editor")
        .initial_size(1120, 760)
        .monospace(true)
        .settings(settings)
        .system_dark(system_dark)
        .escape_to_exit(false);

    run_app(config, move |frame: &mut Frame| {
        let language = libluna_i18n::locale().language().to_owned();
        if language != ui_language {
            ui_language = language;
            app.rebuild_menu();
        }
        frame.set_title(app.window_title());
        crate::ui::handle_events(&mut app, frame.events, frame.size, &frame.theme, frame.text);
        if let Some(text) = app.clipboard_text.take() {
            frame.copy_to_clipboard(text);
        }
        if app.paste_requested {
            app.paste_requested = false;
            frame.paste_from_clipboard();
        }
        app.apply_pending(frame);
        app.poll_plugins();
        crate::plugin_ui::tick(&mut app, frame.size, &frame.theme);
        app.save_prefs_if_dirty();
        crate::ui::paint(&mut app, frame.scene, frame.size, &frame.theme, frame.text);
        frame.request_frame_after(Duration::from_millis(300));
    })
}

fn apply_cli_args(app: &mut App) {
    let mut path: Option<PathBuf> = None;
    let mut line: Option<usize> = None;
    let mut folder: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--line" | "+" => line = args.next().and_then(|value| value.parse().ok()),
            "--folder" => folder = args.next().map(PathBuf::from),
            "--version" | "-v" => {
                println!("Luna Editor {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            other => {
                if let Some(value) = other.strip_prefix("+") {
                    line = value.parse().ok();
                } else if let Some(value) = other.strip_prefix("--line=") {
                    line = value.parse().ok();
                } else if let Some(value) = other.strip_prefix("--folder=") {
                    folder = Some(PathBuf::from(value));
                } else if !other.starts_with('-') {
                    path = Some(crate::document::expand_path(other));
                }
            }
        }
    }
    if let Some(folder) = folder {
        if folder.is_dir() {
            app.workspace.set_root(folder);
        }
    }
    if let Some(path) = path {
        app.open_path(path);
        if let Some(line) = line {
            app.view_mut().goto_line(line);
            app.reveal_cursor = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_indices_match_entries() {
        assert_eq!(menu_entries().len(), MENU_ABOUT + 1);
    }

    #[test]
    fn new_file_adds_a_tab_and_activation_switches() {
        let mut app = App::new();
        assert_eq!(app.docs.len(), 1);
        app.new_file();
        assert_eq!(app.docs.len(), 2);
        assert_eq!(app.active, 1);
        app.select_tab(0);
        assert_eq!(app.active, 0);
        assert_eq!(app.tabs.len(), 2);
    }

    #[test]
    fn closing_a_dirty_tab_asks_before_discarding() {
        let mut app = App::new();
        app.insert_text("x");
        assert!(app.dirty());
        app.close_tab(0);
        assert!(app.confirm.is_some());
        app.answer_confirm(1);
        assert!(app.pending.close);
    }

    #[test]
    fn discarding_pathless_files_closes_without_saving() {
        let mut app = App::new();
        app.insert_text("x");
        assert!(app.doc().path.is_none());
        app.request_close();
        let confirm = app.confirm.as_ref().unwrap();
        assert!(!confirm.can_save);
        // W oknie są tylko „Odrzuć” i „Anuluj” — pierwszy przycisk zamyka,
        // nie próbuje zapisywać pliku bez ścieżki.
        app.answer_confirm(0);
        assert!(app.pending.close);
        assert!(app.error.is_none());
    }

    #[test]
    fn closing_a_clean_tab_removes_it() {
        let mut app = App::new();
        app.new_file();
        assert_eq!(app.docs.len(), 2);
        app.close_tab(1);
        assert_eq!(app.docs.len(), 1);
        assert_eq!(app.active, 0);
    }

    #[test]
    fn opening_the_same_path_selects_existing_tab() {
        let dir = std::env::temp_dir().join(format!("luna-editor-tabs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.rs");
        std::fs::write(&path, "fn main() {}").unwrap();
        let mut app = App::new();
        app.open_path(path.clone());
        app.new_file();
        app.open_path(path);
        assert_eq!(app.docs.len(), 2);
        assert_eq!(app.active, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn about_body_has_separate_lines() {
        let mut app = App::new();
        app.show_about();
        let dialog = app.error.as_ref().unwrap();
        let _ = dialog;
    }

    #[test]
    fn auto_indent_can_be_disabled() {
        let mut app = App::new();
        app.insert_text("    x");
        app.newline();
        assert_eq!(app.view().text(), "    x\n    ");

        let mut plain = App::new();
        plain.toggle_auto_indent();
        plain.insert_text("    x");
        plain.newline();
        assert_eq!(plain.view().text(), "    x\n");
    }

    #[test]
    fn prefs_tab_width_applies_to_open_and_new_documents() {
        let mut app = App::with_prefs(Prefs {
            tab_width: 2,
            ..Prefs::default()
        });
        assert_eq!(app.doc().view.tab_width(), 2);
        app.new_file();
        assert_eq!(app.doc().view.tab_width(), 2);
        app.set_tab_width(8);
        assert!(app.docs.iter().all(|doc| doc.view.tab_width() == 8));
    }

    fn temp_project() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "luna-editor-project-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.join("README.md"), "# Test\n").unwrap();
        dir.canonicalize().unwrap()
    }

    fn write_prompt(app: &mut App, value: &str) {
        if let Some(prompt) = app.prompt.as_mut() {
            prompt.entry.set_text(value);
        }
    }

    #[test]
    fn prompt_creates_a_file_and_opens_it() {
        let dir = temp_project();
        let mut app = App::new();
        app.workspace.set_root(dir.clone());
        app.begin_prompt(PromptKind::NewFile, app.workspace.target_dir());
        write_prompt(&mut app, "notes.txt");
        app.submit_prompt();
        assert!(app.prompt.is_none());
        let path = dir.join("notes.txt");
        assert!(path.is_file());
        assert_eq!(app.doc().path.as_deref(), Some(path.as_path()));
        assert!(app.workspace.rows.iter().any(|row| row.path == path));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prompt_creates_a_folder_and_selects_it() {
        let dir = temp_project();
        let mut app = App::new();
        app.workspace.set_root(dir.clone());
        app.begin_prompt(PromptKind::NewFolder, dir.clone());
        write_prompt(&mut app, "module");
        app.submit_prompt();
        assert!(dir.join("module").is_dir());
        assert_eq!(app.workspace.selected, Some(dir.join("module")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prompt_rejects_separators_and_existing_names() {
        let dir = temp_project();
        let mut app = App::new();
        app.workspace.set_root(dir.clone());
        app.begin_prompt(PromptKind::NewFile, dir.clone());
        write_prompt(&mut app, "a/b.txt");
        app.submit_prompt();
        assert!(app.prompt.is_some());
        assert!(app.prompt.as_ref().unwrap().error.is_some());
        assert!(!dir.join("a").exists());

        write_prompt(&mut app, "README.md");
        app.submit_prompt();
        assert!(app.prompt.is_some());
        assert!(app.prompt.as_ref().unwrap().error.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn renaming_moves_the_file_and_retargets_the_tab() {
        let dir = temp_project();
        let old = dir.join("src/main.rs");
        let mut app = App::new();
        app.workspace.set_root(dir.clone());
        app.open_path(old.clone());
        assert_eq!(app.doc().path.as_deref(), Some(old.as_path()));

        app.begin_prompt(PromptKind::Rename, old.clone());
        assert_eq!(app.prompt.as_ref().unwrap().entry.text(), "main.rs");
        write_prompt(&mut app, "lib.rs");
        app.submit_prompt();

        let new = dir.join("src/lib.rs");
        assert!(!old.exists());
        assert!(new.is_file());
        assert_eq!(app.doc().path.as_deref(), Some(new.as_path()));
        assert_eq!(app.doc().language, Language::Rust);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opening_a_file_outside_the_project_shows_its_folder() {
        let project = temp_project();
        let other = temp_project();
        let file = other.join("README.md");
        let mut app = App::new();
        app.workspace.set_root(project.clone());
        app.open_path(file.clone());
        assert_eq!(
            std::fs::canonicalize(&app.workspace.root).unwrap(),
            std::fs::canonicalize(&other).unwrap()
        );
        assert!(app.workspace.rows.iter().any(|row| row.path == file));
        assert!(!app
            .workspace
            .rows
            .iter()
            .any(|row| row.path == project.join("src")));
        let _ = std::fs::remove_dir_all(&project);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn deleting_closes_open_tabs_and_refreshes_the_tree() {
        let dir = temp_project();
        let path = dir.join("README.md");
        let mut app = App::new();
        app.workspace.set_root(dir.clone());
        app.open_path(path.clone());
        app.request_delete(path.clone());
        assert!(app.pending_delete.is_some());
        app.answer_delete(true);
        assert!(!path.exists());
        assert_eq!(app.docs.len(), 1);
        assert!(app.docs[0].path.is_none());
        assert!(!app.workspace.rows.iter().any(|row| row.path == path));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancelling_a_delete_keeps_the_file() {
        let dir = temp_project();
        let path = dir.join("README.md");
        let mut app = App::new();
        app.workspace.set_root(dir.clone());
        app.request_delete(path.clone());
        app.answer_delete(false);
        assert!(path.is_file());
        assert!(app.pending_delete.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tree_menu_labels_follow_the_row() {
        let dir = temp_project();
        let mut app = App::new();
        app.workspace.set_root(dir.clone());
        let src = app
            .workspace
            .rows
            .iter()
            .position(|row| row.path == dir.join("src"))
            .unwrap();
        let menu = workspace_menu(&app.workspace.rows[src]);
        assert!(matches!(
            &menu.entries()[TREE_OPEN],
            MenuEntry::Action { label, .. } if label == &tr("Expand")
        ));
        app.workspace.toggle_dir(&dir.join("src"));
        let src = app
            .workspace
            .rows
            .iter()
            .position(|row| row.path == dir.join("src"))
            .unwrap();
        let menu = workspace_menu(&app.workspace.rows[src]);
        assert!(matches!(
            &menu.entries()[TREE_OPEN],
            MenuEntry::Action { label, .. } if label == &tr("Collapse")
        ));
        let file = app
            .workspace
            .rows
            .iter()
            .find(|row| row.path == dir.join("README.md"))
            .unwrap();
        let menu = workspace_menu(file);
        assert!(matches!(
            &menu.entries()[TREE_OPEN],
            MenuEntry::Action { label, .. } if label == &tr("Open")
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keyboard_navigation_expands_and_walks_the_tree() {
        let dir = temp_project();
        let mut app = App::new();
        app.workspace.set_root(dir.clone());
        app.workspace.select(dir.join("src"));
        app.workspace_expand_or_child();
        assert!(app
            .workspace
            .rows
            .iter()
            .any(|row| row.path == dir.join("src/main.rs")));
        app.workspace_move(1);
        assert_eq!(app.workspace.selected, Some(dir.join("src/main.rs")));
        app.workspace_collapse_or_parent();
        assert_eq!(app.workspace.selected, Some(dir.join("src")));
        app.workspace_collapse_or_parent();
        assert!(!app
            .workspace
            .rows
            .iter()
            .any(|row| row.path == dir.join("src/main.rs")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plugin_output_is_inserted_into_the_active_document() {
        let dir =
            std::env::temp_dir().join(format!("luna-editor-plugin-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("run.sh"), "#!/bin/sh\nprintf 'X'\n").unwrap();

        let mut app = App::new();
        app.insert_text("abc");
        app.plugins.plugins = vec![plugins::Plugin {
            dir: dir.clone(),
            id: "test".into(),
            name: "Test".into(),
            version: "1".into(),
            author: String::new(),
            description: String::new(),
            entry: "run.sh".into(),
            interpreter: Some("sh".into()),
            commands: vec![plugins::PluginCommand {
                id: "insert".into(),
                title: "Insert".into(),
                prompt: false,
                context: plugins::Context::Document,
                output: Output::Insert,
            }],
            settings: Vec::new(),
        }];
        app.start_plugin(0, 0, String::new());
        for _ in 0..300 {
            app.poll_plugins();
            if app.plugins.job.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(app.plugins.job.is_none(), "plugin job did not finish");
        assert_eq!(app.doc().view.text(), "abcX");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
