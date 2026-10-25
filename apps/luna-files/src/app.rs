//! Stan aplikacji i pętla klatki (platforma libLuny).

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use libluna_core::ThemeSettings;
use libluna_i18n::{set_locale, tr, tr_args, Locale};
use libluna_platform::{run as run_app, AppConfig, Frame};
use libluna_render::ImageData;
use libluna_widgets::{
    AlertDialog, Entry as TextEntry, ItemView, Menu, NavEntry, NavList, PaintCtx, ScrollView,
    SearchEntry, Toast,
};

use crate::model::{human_size, now_secs, DirView, Entry, Kind, SortKey};
use crate::ops::{Job, Ops, OpsEvent};
use crate::places::{self, Mount, Place};
use crate::{trash, ui};

/// Odstęp między panelami w trybie dzielonym.
pub const SPLIT_GAP: f64 = 4.0;

/// Edycja ścieżki albo nazwy (pole tekstowe w pasku adresu).
pub struct EditState {
    pub entry: TextEntry,
    pub rename: Option<PathBuf>,
    pub new_folder: bool,
}

impl EditState {
    pub fn path(path: &Path) -> Self {
        let mut entry = TextEntry::new(tr("Path"));
        entry.set_text(path.display().to_string());
        entry.set_focused(true);
        entry.select_all();
        Self {
            entry,
            rename: None,
            new_folder: false,
        }
    }

    pub fn rename(path: &Path) -> Self {
        let mut entry = TextEntry::new(tr("Name"));
        entry.set_text(
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        entry.set_focused(true);
        entry.select_all();
        Self {
            entry,
            rename: Some(path.to_path_buf()),
            new_folder: false,
        }
    }
}

/// Podgląd pliku (Space).
pub enum Preview {
    Text(String),
    Image(Arc<ImageData>),
    /// Brak podglądu (np. JPG bez dekodera) — pokazujemy metadane.
    None,
}

/// Jedna zakładka/panel z własną ścieżką i stanem widoku.
pub struct Pane {
    pub dir: DirView,
    pub view: ItemView,
    pub scroll: ScrollView,
    pub tabs: Vec<PathBuf>,
    pub tab: usize,
    pub back: Vec<PathBuf>,
    pub forward: Vec<PathBuf>,
    pub edit: Option<EditState>,
    pub search: SearchEntry,
    pub search_active: bool,
    pub preview: bool,
    pub preview_cache: Option<(PathBuf, Preview)>,
}

impl Pane {
    fn new(path: PathBuf) -> Self {
        let dir = DirView::load(&path).unwrap_or_else(|_| {
            DirView::load(places::home()).unwrap_or_else(|_| DirView::load("/").expect("/"))
        });
        let mut view = ItemView::new(dir.items());
        view.set_mode(libluna_widgets::ViewMode::List);
        Self {
            tabs: vec![dir.path.clone()],
            tab: 0,
            back: Vec::new(),
            forward: Vec::new(),
            edit: None,
            search: SearchEntry::new(tr("Search")),
            search_active: false,
            preview: false,
            preview_cache: None,
            scroll: ScrollView::new(),
            dir,
            view,
        }
    }

    /// Indeksy zaznaczonych wpisów (widoczne).
    pub fn selection(&self) -> Vec<PathBuf> {
        let visible = self.dir.visible_entries();
        self.view
            .selected_indices()
            .iter()
            .filter_map(|i| visible.get(*i))
            .map(|e| e.path.clone())
            .collect()
    }

    pub fn primary(&self) -> Option<&Entry> {
        self.view.selected().and_then(|i| self.dir.entry(i))
    }

    /// Po zmianie katalogu/porządku odświeża `ItemView`, zachowując zaznaczenie
    /// po ścieżce (jeśli element nadal istnieje).
    pub fn sync_view(&mut self) {
        let keep = self.primary().map(|e| e.path.clone());
        let items = self.dir.items();
        self.view.set_items(items);
        if let Some(path) = keep {
            if let Some(pos) = (0..self.view.len())
                .find(|i| self.dir.entry(*i).map(|e| e.path == path).unwrap_or(false))
            {
                self.view.set_selected(Some(pos));
            }
        }
        self.scroll = ScrollView::new();
        self.preview_cache = None;
    }

    pub fn selected_title(&self) -> Option<String> {
        self.primary().map(|e| e.name.clone())
    }
}

/// Pozycja panelu bocznego.
pub enum SideAction {
    Go(PathBuf),
    Unmount(usize),
    Mount(usize),
}

/// Co otworzyło menu kontekstowe.
pub enum MenuTarget {
    Item(PathBuf, bool),
    TrashItem(PathBuf),
    Background,
    Device(usize),
    AppMenu,
}

pub struct ContextMenu {
    pub menu: Menu,
    pub panel: libluna_core::Rect,
    pub target: MenuTarget,
    /// Komenda dla każdego wpisu menu (`None` = sekcja/separator).
    pub cmds: Vec<Option<MenuCmd>>,
}

/// Akcja wybrana z menu (indeks wpisu → komenda).
#[derive(Debug, Clone, Copy)]
pub enum MenuCmd {
    Open,
    OpenTerminal,
    Copy,
    Cut,
    Paste,
    SelectAll,
    Rename,
    Trash,
    Delete,
    Properties,
    Restore,
    Purge,
    NewFolder,
    NewTab,
    Split,
    ToggleHidden,
    Sort(SortKey),
    ToggleAsc,
    Refresh,
    EmptyTrash,
    Unmount(usize),
    OpenDevice(usize),
}

pub struct Clipboard {
    pub paths: Vec<PathBuf>,
    pub cut: bool,
}

impl Clone for Clipboard {
    fn clone(&self) -> Self {
        Self {
            paths: self.paths.clone(),
            cut: self.cut,
        }
    }
}

/// Dialog konfliktu nazw (blokuje operację do decyzji użytkownika).
pub struct ConflictPrompt {
    pub dialog: AlertDialog,
    pub src: PathBuf,
    pub dst: PathBuf,
}

impl ConflictPrompt {
    pub fn new(src: PathBuf, dst: PathBuf) -> Self {
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let body = tr_args(
            "{name} already exists in the destination.",
            &[("name", &name)],
        );
        let dialog = AlertDialog::new(tr("File exists"), body)
            .width(520.0)
            .button(tr("Replace"))
            .button(tr("Skip"))
            .button(tr("Rename"))
            .button(tr("Replace All"));
        Self { dialog, src, dst }
    }
}

/// Cały stan aplikacji.
pub struct App {
    pub panes: [Pane; 2],
    pub active: usize,
    pub split: bool,
    pub sidebar_visible: bool,
    pub places: Vec<Place>,
    pub mounts: Vec<Mount>,
    pub sidebar: NavList,
    pub sidebar_actions: Vec<SideAction>,
    /// Dla każdego wpisu `sidebar.entries()` — indeks akcji (None = sekcja).
    pub sidebar_entry_action: Vec<Option<usize>>,
    pub context: Option<ContextMenu>,
    pub conflict: Option<ConflictPrompt>,
    /// Potwierdzenie opróżnienia kosza (modal).
    pub confirm_empty: Option<AlertDialog>,
    pub ops: Ops,
    pub op_label: Option<String>,
    pub op_progress: Option<(u64, u64)>,
    pub op_current: String,
    pub op_speed: f64,
    op_sample: Option<(Instant, u64)>,
    op_started: Option<Instant>,
    pub toast: Toast,
    pub clipboard: Option<Clipboard>,
    pub last_click: Option<(usize, Instant)>,
    pub last_mount_refresh: Instant,
    pub status: String,
    /// Zakładka pod kursorem (podświetlenie w pasku tytułu).
    pub tab_hover: Option<usize>,
    pub pending_close: bool,
    pub pending_minimize: bool,
    pub pending_maximize: bool,
    pub pending_move: bool,
}

impl App {
    pub fn new(start: PathBuf) -> Self {
        let home = places::home();
        let start = if start.is_dir() { start } else { home };
        let mut app = Self {
            panes: [Pane::new(start.clone()), Pane::new(start)],
            active: 0,
            split: false,
            sidebar_visible: true,
            places: Vec::new(),
            mounts: Vec::new(),
            sidebar: NavList::new(Vec::new()),
            sidebar_actions: Vec::new(),
            sidebar_entry_action: Vec::new(),
            context: None,
            conflict: None,
            confirm_empty: None,
            ops: Ops::new(),
            op_label: None,
            op_progress: None,
            op_current: String::new(),
            op_speed: 0.0,
            op_sample: None,
            op_started: None,
            toast: Toast::new(""),
            clipboard: None,
            last_click: None,
            last_mount_refresh: Instant::now() - Duration::from_secs(10),
            status: String::new(),
            tab_hover: None,
            pending_close: false,
            pending_minimize: false,
            pending_maximize: false,
            pending_move: false,
        };
        app.rebuild_sidebar();
        app
    }

    pub fn pane(&self) -> &Pane {
        &self.panes[self.active]
    }

    pub fn pane_mut(&mut self) -> &mut Pane {
        let i = self.active;
        &mut self.panes[i]
    }

    pub fn other_pane_mut(&mut self) -> &mut Pane {
        let i = 1 - self.active;
        &mut self.panes[i]
    }

    /// Nawigacja z zapisem historii.
    pub fn navigate(&mut self, pane: usize, path: PathBuf) {
        if !path.is_dir() {
            return;
        }
        if debug() {
            eprintln!("luna-files: navigate {}", path.display());
        }
        let previous = self.panes[pane].dir.path.clone();
        match DirView::load(&path) {
            Ok(dir) => {
                let p = &mut self.panes[pane];
                p.dir = dir;
                p.back.push(previous);
                p.forward.clear();
                p.tabs[p.tab] = p.dir.path.clone();
                p.search.clear();
                p.search_active = false;
                p.dir.set_filter("");
                p.edit = None;
                p.sync_view();
                self.select_sidebar_for(&path);
            }
            Err(e) => self.show_toast(format!("{}: {e}", path.display())),
        }
    }

    pub fn reload_pane(&mut self, pane: usize) {
        if self.panes[pane].dir.reload().is_ok() {
            self.panes[pane].sync_view();
        }
    }

    pub fn reload_all(&mut self) {
        for i in 0..2 {
            if self.panes[i].dir.reload().is_ok() {
                self.panes[i].sync_view();
            }
        }
    }

    pub fn back(&mut self) {
        let p = &mut self.panes[self.active];
        let Some(prev) = p.back.pop() else { return };
        p.forward.push(p.dir.path.clone());
        if let Ok(dir) = DirView::load(&prev) {
            p.dir = dir;
            p.tabs[p.tab] = p.dir.path.clone();
            p.sync_view();
            let path = p.dir.path.clone();
            self.select_sidebar_for(&path);
        }
    }

    pub fn forward(&mut self) {
        let p = &mut self.panes[self.active];
        let Some(next) = p.forward.pop() else {
            return;
        };
        p.back.push(p.dir.path.clone());
        if let Ok(dir) = DirView::load(&next) {
            p.dir = dir;
            p.tabs[p.tab] = p.dir.path.clone();
            p.sync_view();
            let path = p.dir.path.clone();
            self.select_sidebar_for(&path);
        }
    }

    pub fn up(&mut self) {
        let path = self.panes[self.active].dir.parent();
        if let Some(path) = path {
            self.navigate(self.active, path);
        }
    }

    /// Otwiera zaznaczony wpis (katalog → nawigacja, plik → `xdg-open`).
    pub fn open_selected(&mut self) {
        let Some(entry) = self.pane().primary().cloned() else {
            return;
        };
        if entry.is_dir {
            self.navigate(self.active, entry.path);
        } else {
            let _ = std::process::Command::new("xdg-open")
                .arg(&entry.path)
                .spawn();
        }
    }

    /// Otwiera terminal w bieżącym katalogu.
    pub fn open_terminal(&mut self) {
        let dir = self.pane().dir.path.clone();
        self.open_terminal_at(&dir);
    }

    /// Otwiera terminal w podanym katalogu.
    pub fn open_terminal_at(&mut self, dir: &Path) {
        let dir_arg = dir.display().to_string();
        let terminal = std::env::var("TERMINAL").ok();
        let spawned = match terminal.as_deref() {
            Some(t) if !t.is_empty() => std::process::Command::new(t)
                .current_dir(dir)
                .spawn()
                .is_ok(),
            _ => ["xfce4-terminal", "foot", "alacritty", "kitty", "wezterm"]
                .iter()
                .any(|t| spawn_terminal(t, &dir_arg)),
        };
        if !spawned {
            self.show_toast(tr("No terminal emulator found"));
        }
    }

    pub fn new_tab(&mut self) {
        let p = self.pane_mut();
        p.tabs.push(p.dir.path.clone());
        p.tab = p.tabs.len() - 1;
    }

    pub fn close_tab(&mut self) {
        let p = self.pane_mut();
        if p.tabs.len() <= 1 {
            return;
        }
        p.tabs.remove(p.tab);
        p.tab = p.tab.min(p.tabs.len() - 1);
        let path = p.tabs[p.tab].clone();
        if let Ok(dir) = DirView::load(&path) {
            p.dir = dir;
            p.sync_view();
        }
    }

    pub fn select_tab(&mut self, index: usize) {
        let p = self.pane_mut();
        if index < p.tabs.len() {
            p.tab = index;
            let path = p.tabs[index].clone();
            if let Ok(dir) = DirView::load(&path) {
                p.dir = dir;
                p.sync_view();
            }
        }
    }

    pub fn submit(&mut self, job: Job) {
        if !self.ops.submit(job) {
            self.show_toast(tr("Another operation is still running"));
        }
    }

    pub fn drain_ops(&mut self) {
        for ev in self.ops.poll() {
            match ev {
                OpsEvent::Started { label } => {
                    self.op_label = Some(label);
                    self.op_progress = Some((0, 0));
                    self.op_current.clear();
                    self.op_speed = 0.0;
                    self.op_sample = None;
                    self.op_started = Some(Instant::now());
                }
                OpsEvent::Progress {
                    done,
                    total,
                    current,
                } => {
                    let now = Instant::now();
                    if let Some((t0, d0)) = self.op_sample {
                        let dt = now.duration_since(t0).as_secs_f64();
                        if dt >= 0.3 {
                            let inst = done.saturating_sub(d0) as f64 / dt;
                            self.op_speed = if self.op_speed <= 0.0 {
                                inst
                            } else {
                                self.op_speed * 0.7 + inst * 0.3
                            };
                            self.op_sample = Some((now, done));
                        }
                    } else {
                        self.op_sample = Some((now, done));
                    }
                    self.op_progress = Some((done, total));
                    self.op_current = current;
                }
                OpsEvent::Conflict { src, dst, .. } => {
                    self.conflict = Some(ConflictPrompt::new(src, dst));
                }
                OpsEvent::Finished {
                    label,
                    ok,
                    skipped,
                    failed,
                    created,
                } => {
                    if debug() {
                        eprintln!(
                            "luna-files: ops '{label}' ok={ok} skipped={skipped} failed={failed:?} created={created:?}"
                        );
                    }
                    self.op_label = None;
                    self.op_progress = None;
                    self.op_current.clear();
                    self.op_speed = 0.0;
                    self.op_sample = None;
                    self.op_started = None;
                    self.conflict = None;
                    self.reload_all();
                    self.refresh_devices();
                    if let Some(path) = created {
                        self.select_path(&path);
                    }
                    let msg = if skipped > 0 {
                        tr_args(
                            "{label}: {n} done, {skipped} skipped",
                            &[
                                ("label", &label),
                                ("n", &ok.to_string()),
                                ("skipped", &skipped.to_string()),
                            ],
                        )
                    } else {
                        tr_args(
                            "{label}: {n} done",
                            &[("label", &label), ("n", &ok.to_string())],
                        )
                    };
                    if let Some(first) = failed.first() {
                        self.show_toast(format!("{msg} — {first}"));
                    } else {
                        self.show_toast(msg);
                    }
                }
            }
        }
    }

    /// Odpowiedź na dialog konfliktu (indeks przycisku).
    pub fn answer_conflict(&mut self, index: usize) {
        use crate::ops::Decision;
        let decision = match index {
            0 => Decision::Overwrite,
            1 => Decision::Skip,
            2 => Decision::Rename,
            3 => Decision::OverwriteAll,
            _ => Decision::Skip,
        };
        if debug() {
            eprintln!("luna-files: conflict -> {decision:?}");
        }
        self.ops.decide(decision);
        self.conflict = None;
    }

    pub fn cancel_op(&mut self) {
        self.ops.cancel();
    }

    pub fn refresh_devices(&mut self) {
        let mounts = places::mounts();
        let changed = mounts.len() != self.mounts.len()
            || mounts
                .iter()
                .zip(self.mounts.iter())
                .any(|(a, b)| a.dev != b.dev || a.mount != b.mount);
        self.mounts = mounts;
        if changed {
            self.rebuild_sidebar();
            let path = self.pane().dir.path.clone();
            self.select_sidebar_for(&path);
        }
        self.last_mount_refresh = Instant::now();
    }

    pub fn refresh_devices_if_needed(&mut self) {
        if self.last_mount_refresh.elapsed() >= Duration::from_secs(2) {
            self.refresh_devices();
        }
    }

    pub fn rebuild_sidebar(&mut self) {
        self.places = places::places();
        let mut entries = Vec::new();
        let mut actions = Vec::new();
        let mut mapping = Vec::new();

        entries.push(NavEntry::Section(tr("Places")));
        mapping.push(None);
        for place in &self.places {
            entries.push(NavEntry::Item(place.label.clone()));
            actions.push(SideAction::Go(place.path.clone()));
            mapping.push(Some(actions.len() - 1));
        }
        entries.push(NavEntry::Item(tr("Trash")));
        actions.push(SideAction::Go(trash::trash_root().join("files")));
        mapping.push(Some(actions.len() - 1));

        if !self.mounts.is_empty() {
            entries.push(NavEntry::Section(tr("Devices")));
            mapping.push(None);
            for (i, m) in self.mounts.iter().enumerate() {
                let free = human_size(m.free);
                let label = tr_args(
                    "{name} — {free} free",
                    &[("name", &m.label), ("free", &free)],
                );
                entries.push(NavEntry::Item(label));
                actions.push(SideAction::Go(m.mount.clone()));
                mapping.push(Some(actions.len() - 1));
                // Lewy klik nawiguje; odmontowanie jest w menu kontekstowym.
                let _ = (i, SideAction::Unmount(i));
            }
        }

        self.sidebar = NavList::new(entries);
        self.sidebar_actions = actions;
        self.sidebar_entry_action = mapping;
    }

    fn select_sidebar_for(&mut self, path: &Path) {
        let mut best: Option<(usize, usize)> = None;
        for (ai, act) in self.sidebar_actions.iter().enumerate() {
            if let SideAction::Go(p) = act {
                if path.starts_with(p) {
                    let depth = p.components().count();
                    if best.map(|(_, d)| depth > d).unwrap_or(true) {
                        best = Some((ai, depth));
                    }
                }
            }
        }
        if let Some((ai, _)) = best {
            if let Some(entry) = self
                .sidebar_entry_action
                .iter()
                .position(|a| *a == Some(ai))
            {
                self.sidebar.set_selected(Some(entry));
            }
        }
    }

    /// Zaznacza plik w bieżącym panelu (np. po utworzeniu folderu).
    pub fn select_path(&mut self, path: &Path) {
        let p = self.pane_mut();
        let visible = p.dir.visible_entries();
        if let Some(i) = visible.iter().position(|e| e.path == *path) {
            p.view.set_selected(Some(i));
        }
    }

    pub fn show_toast(&mut self, message: impl Into<String>) {
        self.toast = Toast::new(message);
        self.toast.show_animated();
    }

    /// Status w dolnej belce.
    pub fn status_text(&self) -> String {
        let p = self.pane();
        let visible = p.view.len();
        let selected = p.view.selected_indices().len();
        let mut text = tr_args("{n} items", &[("n", &visible.to_string())]);
        if selected > 0 {
            let n = selected.to_string();
            let visible_entries = p.dir.visible_entries();
            let size: u64 = p
                .view
                .selected_indices()
                .iter()
                .filter_map(|i| visible_entries.get(*i))
                .map(|e| if e.is_dir { 0 } else { e.size })
                .sum();
            text = format!(
                "{text} · {}",
                tr_args(
                    "{n} selected ({size})",
                    &[("n", &n), ("size", &human_size(size))]
                )
            );
        }
        if let Some((total, free)) = places::statvfs(&p.dir.path) {
            let pct = if total > 0 {
                (total.saturating_sub(free)) as f64 / total as f64 * 100.0
            } else {
                0.0
            };
            text = format!(
                "{text} · {} {} ({}%)",
                human_size(free),
                tr("free"),
                pct.round() as i64
            );
        }
        text
    }

    /// Odświeża podgląd zaznaczonego pliku.
    pub fn update_preview(&mut self) {
        let p = self.pane_mut();
        if !p.preview {
            p.preview_cache = None;
            return;
        }
        let Some(entry) = p.primary().cloned() else {
            p.preview_cache = None;
            return;
        };
        if p.preview_cache
            .as_ref()
            .map(|(path, _)| *path == entry.path)
            .unwrap_or(false)
        {
            return;
        }
        let preview = preview_for(&entry);
        p.preview_cache = Some((entry.path, preview));
    }
}

/// Diagnostyka `LUNA_FILES_DEBUG=1`.
pub fn debug() -> bool {
    std::env::var("LUNA_FILES_DEBUG").is_ok()
}

fn preview_for(entry: &Entry) -> Preview {
    if entry.is_dir {
        return Preview::None;
    }
    match entry.kind {
        Kind::Image => {
            // Dekoder PNG jest w libluna-render; inne formaty → metadane.
            if entry
                .path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("png"))
                .unwrap_or(false)
            {
                if let Ok(bytes) = std::fs::read(&entry.path) {
                    if let Some(img) = libluna_render::wallpaper::decode_png(&bytes) {
                        return Preview::Image(Arc::new(img));
                    }
                }
            }
            Preview::None
        }
        Kind::Text | Kind::Code => {
            use std::io::Read;
            let mut file = match std::fs::File::open(&entry.path) {
                Ok(f) => f,
                Err(_) => return Preview::None,
            };
            let mut buf = vec![0u8; 64 * 1024];
            let n = file.read(&mut buf).unwrap_or(0);
            buf.truncate(n);
            let text = String::from_utf8_lossy(&buf);
            let lines: Vec<&str> = text.lines().take(200).collect();
            Preview::Text(lines.join("\n"))
        }
        _ => Preview::None,
    }
}

fn spawn_terminal(program: &str, dir: &str) -> bool {
    let args: &[&str] = match program {
        "xfce4-terminal" => &["--working-directory", dir],
        "gnome-terminal" => &["--working-directory", dir],
        "kitty" => &["--directory", dir],
        "alacritty" => &["--working-directory", dir],
        _ => &[],
    };
    std::process::Command::new(program)
        .args(args)
        .current_dir(dir)
        .spawn()
        .is_ok()
}

/// Uruchamia aplikację (binarki `luna-files` i `gaminedes`).
pub fn run(title_key: &str, app_id: &str) -> Result<(), Box<dyn Error>> {
    set_locale(Locale::from_env());
    libluna_i18n::load_catalogs_from_dirs();
    let title = tr(title_key);

    let settings = ThemeSettings::load();
    let system_dark = std::env::var("LUNA_SYSTEM_DARK")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);

    let start = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(places::home);

    let config = AppConfig::new(title)
        .app_id(app_id)
        .initial_size(1120, 720)
        .settings(settings)
        .system_dark(system_dark);

    let mut app = App::new(start);
    app.status = app.status_text();

    run_app(config, move |frame: &mut Frame| {
        let theme = frame.settings().resolve_themed(frame.system_dark());
        app.drain_ops();
        app.refresh_devices_if_needed();

        ui::handle_events(&mut app, frame.events, frame.size, &theme, frame.text);

        if app.pending_close {
            frame.close();
            app.pending_close = false;
        }
        if app.pending_minimize {
            frame.minimize();
            app.pending_minimize = false;
        }
        if app.pending_maximize {
            frame.toggle_maximize();
            app.pending_maximize = false;
        }
        if app.pending_move {
            frame.begin_move();
            app.pending_move = false;
        }

        app.update_preview();
        app.status = app.status_text();

        ui::paint(&app, frame.scene, frame.size, &theme, frame.text);

        if app.toast.is_animating() {
            app.toast.advance(frame.delta_time(), 0.3);
            frame.request_frame();
        }
        if app.ops.busy() {
            frame.request_frame();
        }
    })
}

/// Pomocnik dla UI: `PaintCtx` z klatki.
pub fn paint_ctx<'a>(
    scene: &'a mut libluna_render::Scene,
    theme: &'a libluna_core::Theme,
    text: &'a libluna_render::TextEngine,
) -> PaintCtx<'a> {
    PaintCtx::new(scene, theme, text)
}

/// Na potrzeby UI: bieżący czas do animacji/migań.
pub fn tick() -> i64 {
    now_secs()
}
