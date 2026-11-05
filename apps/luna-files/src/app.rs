//! Stan aplikacji i pętla klatki (platforma libLuny).

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use libluna_core::{Point, Rect, ThemePreference, ThemeSettings};
use libluna_i18n::{set_locale, tr, tr_args, Locale};
use libluna_platform::{run as run_app, AppConfig, Frame};
use libluna_render::ImageData;
use libluna_widgets::{
    AlertDialog, Entry as TextEntry, IconName, ItemView, Menu, NavEntry, NavList, PaintCtx,
    ScrollView, SearchEntry, Toast, ViewMode, WindowControl,
};

use crate::model::{format_mtime, human_size, now_secs, DirView, Entry, Kind, SortKey};
use crate::open_with::OpenWithDialog;
use crate::ops::{Job, Ops, OpsEvent};
use crate::places::{self, Mount, Place};
use crate::prefs::Prefs;
use crate::{archive, trash, ui};

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
    None,
}

/// Informacje do dymka po najechaniu na kafelek/wiersz.
pub struct HoverTip {
    pub entry: Entry,
    pub since: Instant,
}

/// Rozmiar katalogu liczony w tle (nie blokuje UI).
#[derive(Debug, Clone, Copy, Default)]
pub struct DirSize {
    pub bytes: u64,
    pub files: u64,
    pub dirs: u64,
}

/// Obliczanie rozmiaru katalogu w toku.
pub struct PendingProperties {
    pub name: String,
    pub modified: Option<i64>,
    pub result: Arc<Mutex<Option<DirSize>>>,
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
    /// Otwiera okno wyboru aplikacji dla pliku.
    OpenWith,
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
    Mount(usize),
    OpenDevice(usize),
    /// Włącza/wyłącza panel podglądu (obraz lub tekst).
    TogglePreview,
    /// Otwiera okno Ustawień.
    Settings,
    /// Otwiera okno „O aplikacji”.
    About,
    /// Pakuje zaznaczone elementy do archiwum ZIP.
    Compress,
    /// Wypakowuje archiwum do jego katalogu.
    ExtractHere,
    /// Wypakowuje archiwum do podkatalogu o nazwie archiwum.
    ExtractTo,
}

pub struct Clipboard {
    pub paths: Vec<PathBuf>,
    pub cut: bool,
}

/// Pole tekstowe, do którego trafi tekst z systemowego schowka.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextPasteTarget {
    Edit,
    Search,
}

/// Cel upuszczenia przeciąganych plików.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropTarget {
    /// Katalog, do którego przenosimy/kopiujemy.
    Directory(PathBuf),
    /// Kosz (przeniesienie do XDG Trash).
    Trash,
}

/// Trwające przeciąganie plików z panelu.
#[derive(Debug, Clone)]
pub struct DragState {
    pub paths: Vec<PathBuf>,
    /// Punkt wciśnięcia (do progu rozpoczęcia przeciągania).
    pub start: Point,
    /// Czy przekroczono próg ruchu (dopiero wtedy rysujemy „ducha”).
    pub active: bool,
    /// Ctrl = kopiuj, bez Ctrl = przenieś.
    pub copy: bool,
    /// Aktualny cel pod kursorem.
    pub target: Option<DropTarget>,
    /// Prostokąt podświetlenia celu.
    pub target_rect: Option<Rect>,
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
    /// Kiedy schować toast (po operacji); `None` = toast nie jest widoczny.
    pub toast_until: Option<Instant>,
    /// Dymek z informacjami o elemencie pod kursorem (po chwili najechania).
    pub tooltip: Option<HoverTip>,
    /// Rozmiar katalogu liczony w tle dla okna Właściwości.
    pub pending_properties: Option<PendingProperties>,
    /// Przeciąganie plików (portal wewnętrzny, nie Wayland DnD).
    pub drag: Option<DragState>,
    pub clipboard: Option<Clipboard>,
    /// Tekst do systemowego schowka (odbierany przez pętlę klatki).
    pub text_clipboard: Option<String>,
    /// Prośba o odczyt systemowego schowka; wynik wróci jako `Event::Paste`.
    pub text_paste_requested: bool,
    /// Pole, do którego trafi wklejony tekst.
    pub text_paste_target: Option<TextPasteTarget>,
    pub last_click: Option<(usize, Instant)>,
    pub last_mount_refresh: Instant,
    pub status: String,
    /// Zakładka pod kursorem (podświetlenie w pasku tytułu).
    pub tab_hover: Option<usize>,
    /// Segment paska ścieżki pod kursorem.
    pub path_hover: Option<usize>,
    /// Ostatnia pozycja kursora w oknie (do podświetlania przycisków).
    pub pointer: Option<Point>,
    /// Kontrolka okna pod kursorem (— □ ✕).
    pub controls_hover: Option<WindowControl>,
    /// Preferencje (język, domyślny widok, ukryte pliki).
    pub prefs: Prefs,
    /// Okno Ustawień (modal).
    pub settings_open: bool,
    /// Okno „O aplikacji” (modal).
    pub about: Option<AlertDialog>,
    /// Okno „Otwórz za pomocą…” (modal).
    pub open_with: Option<OpenWithDialog>,
    /// Motyw wybrany w Ustawieniach — aplikowany przez pętlę klatki.
    pub pending_theme: Option<ThemePreference>,
    /// Bieżąca preferencja motywu (do podświetlenia w Ustawieniach).
    pub theme_pref: ThemePreference,
    pub pending_close: bool,
    pub pending_minimize: bool,
    pub pending_maximize: bool,
    pub pending_move: bool,
}

impl App {
    pub fn new(start: PathBuf) -> Self {
        Self::with_prefs(start, Prefs::load())
    }

    pub fn with_prefs(start: PathBuf, prefs: Prefs) -> Self {
        let home = places::home();
        let start = if start.is_dir() { start } else { home };
        let mut panes = [Pane::new(start.clone()), Pane::new(start)];
        for pane in &mut panes {
            pane.view.set_mode(prefs.view);
            pane.dir.set_show_hidden(prefs.show_hidden);
            pane.sync_view();
        }
        let mut app = Self {
            panes,
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
            toast_until: None,
            tooltip: None,
            pending_properties: None,
            drag: None,
            clipboard: None,
            text_clipboard: None,
            text_paste_requested: false,
            text_paste_target: None,
            last_click: None,
            last_mount_refresh: Instant::now() - Duration::from_secs(10),
            status: String::new(),
            tab_hover: None,
            path_hover: None,
            pointer: None,
            controls_hover: None,
            prefs,
            settings_open: false,
            about: None,
            open_with: None,
            pending_theme: None,
            theme_pref: ThemePreference::System,
            pending_close: false,
            pending_minimize: false,
            pending_maximize: false,
            pending_move: false,
        };
        app.rebuild_sidebar();
        app
    }

    /// Zapisuje bieżące preferencje (wywoływane po zmianie w Ustawieniach).
    pub fn save_prefs(&self) {
        self.prefs.save();
    }

    /// Ustawia język i odświeża wszystkie napisy zależne od katalogu tłumaczeń.
    pub fn set_language(&mut self, code: &str) {
        self.prefs.language = code.to_owned();
        let locale = Locale::new(code);
        set_locale(locale.clone());
        // Język jest wspólny dla całego środowiska — zapisujemy go też globalnie.
        let _ = libluna_i18n::save_preferred_locale(&locale);
        self.rebuild_sidebar();
        self.status = self.status_text();
        self.save_prefs();
    }

    /// Ustawia domyślny widok we wszystkich panelach.
    pub fn set_view(&mut self, mode: ViewMode) {
        self.prefs.view = mode;
        for pane in &mut self.panes {
            pane.view.set_mode(mode);
            pane.sync_view();
        }
        self.save_prefs();
    }

    /// Pokazuje/ukrywa pliki ukryte we wszystkich panelach.
    pub fn set_show_hidden(&mut self, show: bool) {
        self.prefs.show_hidden = show;
        for pane in &mut self.panes {
            pane.dir.set_show_hidden(show);
            pane.sync_view();
        }
        self.save_prefs();
    }

    /// Otwiera okno „O aplikacji”.
    pub fn show_about(&mut self) {
        let body = format!(
            "Luna Files {}\n{}\n\n© 2026 Luna Project · GPL-3.0-or-later",
            env!("CARGO_PKG_VERSION"),
            tr("File manager for the Luna desktop.")
        );
        self.about = Some(
            AlertDialog::new(tr("About Files"), body)
                .width(500.0)
                .button(tr("Done")),
        );
    }

    /// Tworzy archiwum ZIP z podanych elementów w katalogu `dir`.
    pub fn compress(&mut self, items: Vec<PathBuf>, dir: &Path) {
        if items.is_empty() {
            return;
        }
        let base = if items.len() == 1 {
            items[0]
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| tr("Archive"))
        } else {
            tr("Archive")
        };
        let dest = archive::unique_path(dir, &base, "zip");
        self.submit(Job::Compress { items, dest });
    }

    /// Wypakowuje archiwum: `here = true` → do katalogu archiwum, inaczej do podkatalogu.
    pub fn extract(&mut self, archive_path: PathBuf, here: bool) {
        if !archive::is_archive(&archive_path) {
            self.show_toast(tr("Unsupported archive format"));
            return;
        }
        let Some(parent) = archive_path.parent() else {
            return;
        };
        let dest = if here {
            parent.to_path_buf()
        } else {
            parent.join(archive::archive_stem(&archive_path))
        };
        self.submit(Job::Extract {
            archive: archive_path,
            dest,
        });
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

    /// Otwiera zaznaczony wpis (katalog → nawigacja, plik → aplikacja domyślna).
    pub fn open_selected(&mut self) {
        let Some(entry) = self.pane().primary().cloned() else {
            return;
        };
        if entry.is_dir {
            self.navigate(self.active, entry.path);
        } else {
            self.open_file(&entry.path);
        }
    }

    /// Otwiera plik: domyślną aplikacją albo oknem „Otwórz za pomocą…”.
    ///
    /// Okno pojawia się, gdy typ pliku jest nierozpoznany (np. `application/octet-stream`)
    /// albo system nie ma dla niego domyślnej aplikacji.
    pub fn open_file(&mut self, path: &Path) {
        if crate::open_with::recognized_default(path).is_some()
            && crate::open_with::open_default(path).is_ok()
        {
            return;
        }
        self.show_open_with(path.to_path_buf());
    }

    /// Pokazuje okno wyboru aplikacji dla pliku.
    pub fn show_open_with(&mut self, path: PathBuf) {
        self.open_with = Some(OpenWithDialog::load(path));
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
            _ => [
                "luna-terminal",
                "xfce4-terminal",
                "foot",
                "alacritty",
                "kitty",
                "wezterm",
            ]
            .iter()
            .any(|t| spawn_terminal(t, &dir_arg)),
        };
        if !spawned {
            self.show_toast(tr("No terminal emulator found"));
        }
    }

    pub fn new_tab(&mut self) {
        let path = self.pane().dir.path.clone();
        self.new_tab_at(path);
    }

    /// Otwiera `path` w nowej karcie (np. środkowym przyciskiem myszy).
    pub fn new_tab_at(&mut self, path: PathBuf) {
        if !path.is_dir() {
            return;
        }
        {
            let p = self.pane_mut();
            p.tabs.push(path);
            p.tab = p.tabs.len() - 1;
            if let Ok(dir) = DirView::load(&p.tabs[p.tab]) {
                p.dir = dir;
                p.sync_view();
            }
        }
        let path = self.pane().dir.path.clone();
        self.select_sidebar_for(&path);
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
        let mounts = places::devices();
        let changed = mounts.len() != self.mounts.len()
            || mounts
                .iter()
                .zip(self.mounts.iter())
                .any(|(a, b)| a.dev != b.dev || a.mount != b.mount || a.mounted != b.mounted);
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
        let home = places::home();
        let dirs = places::user_dirs();
        let place_icon = |path: &Path| -> IconName {
            if path == home.as_path() {
                IconName::Home
            } else if dirs.get("DESKTOP").is_some_and(|dir| dir.as_path() == path) {
                IconName::Monitor
            } else if dirs
                .get("DOCUMENTS")
                .is_some_and(|dir| dir.as_path() == path)
            {
                IconName::Document
            } else if dirs
                .get("DOWNLOAD")
                .is_some_and(|dir| dir.as_path() == path)
            {
                IconName::Archive
            } else if dirs
                .get("PICTURES")
                .is_some_and(|dir| dir.as_path() == path)
            {
                IconName::Image
            } else if dirs.get("MUSIC").is_some_and(|dir| dir.as_path() == path) {
                IconName::Music
            } else if dirs.get("VIDEOS").is_some_and(|dir| dir.as_path() == path) {
                IconName::Video
            } else {
                IconName::Files
            }
        };

        let mut entries = Vec::new();
        let mut actions = Vec::new();
        let mut mapping = Vec::new();
        let mut icons = Vec::new();

        entries.push(NavEntry::Section(tr("Places")));
        mapping.push(None);
        icons.push(None);
        for place in &self.places {
            entries.push(NavEntry::Item(place.label.clone()));
            actions.push(SideAction::Go(place.path.clone()));
            mapping.push(Some(actions.len() - 1));
            icons.push(Some(place_icon(&place.path)));
        }
        entries.push(NavEntry::Item(tr("Trash")));
        actions.push(SideAction::Go(trash::trash_root().join("files")));
        mapping.push(Some(actions.len() - 1));
        icons.push(Some(IconName::Archive));

        if !self.mounts.is_empty() {
            entries.push(NavEntry::Section(tr("Devices")));
            mapping.push(None);
            icons.push(None);
            for (i, m) in self.mounts.iter().enumerate() {
                let label = if m.mounted {
                    let free = human_size(m.free);
                    tr_args(
                        "{name} — {free} free",
                        &[("name", &m.label), ("free", &free)],
                    )
                } else {
                    let size = human_size(m.size);
                    tr_args("{name} — {size}", &[("name", &m.label), ("size", &size)])
                };
                entries.push(NavEntry::Item(label));
                actions.push(if m.mounted {
                    SideAction::Go(m.mount.clone())
                } else {
                    SideAction::Mount(i)
                });
                mapping.push(Some(actions.len() - 1));
                icons.push(Some(IconName::Archive));
            }
        }

        self.sidebar = NavList::new(entries).monochrome(true);
        self.sidebar.set_icons(icons);
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
        // Toast jest informacyjny (bez akcji) — po chwili sam się chowa.
        self.toast_until = Some(Instant::now() + Duration::from_secs(4));
    }

    /// Pokazuje Właściwości. Rozmiar katalogu liczy wątek w tle — duże drzewa
    /// nie mogą blokować klatki.
    pub fn show_properties(&mut self, path: &Path) {
        let Ok(meta) = std::fs::metadata(path) else {
            return;
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        if !meta.is_dir() {
            self.pending_properties = None;
            self.show_toast(properties_message(
                &name,
                "File",
                meta.len(),
                None,
                modified,
            ));
            self.toast_until = Some(Instant::now() + Duration::from_secs(8));
            return;
        }

        let slot = Arc::new(Mutex::new(None));
        let result = Arc::clone(&slot);
        let dir = path.to_path_buf();
        std::thread::spawn(move || {
            let size = dir_size(&dir);
            if let Ok(mut guard) = result.lock() {
                *guard = Some(size);
            }
        });
        self.pending_properties = Some(PendingProperties {
            name,
            modified,
            result: slot,
        });
        self.show_toast(tr("Calculating folder size…"));
    }

    /// Odbiera wynik obliczeń rozmiaru katalogu (wywoływane co klatkę).
    pub fn poll_properties(&mut self) {
        let Some(pending) = &self.pending_properties else {
            return;
        };
        let Some(size) = pending
            .result
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
        else {
            return;
        };
        let name = pending.name.clone();
        let modified = pending.modified;
        self.pending_properties = None;
        self.show_toast(properties_message(
            &name,
            "Folder",
            size.bytes,
            Some((size.files, size.dirs)),
            modified,
        ));
        self.toast_until = Some(Instant::now() + Duration::from_secs(8));
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

fn properties_message(
    name: &str,
    kind: &str,
    bytes: u64,
    items: Option<(u64, u64)>,
    modified: Option<i64>,
) -> String {
    let kind = tr(kind);
    let size = human_size(bytes);
    let date = format_mtime(modified);
    match items {
        Some((files, dirs)) => tr_args(
            "{name} · {kind} · {size} · {files} files, {dirs} folders · {date}",
            &[
                ("name", name),
                ("kind", &kind),
                ("size", &size),
                ("files", &files.to_string()),
                ("dirs", &dirs.to_string()),
                ("date", &date),
            ],
        ),
        None => tr_args(
            "{name} · {kind} · {size} · {date}",
            &[
                ("name", name),
                ("kind", &kind),
                ("size", &size),
                ("date", &date),
            ],
        ),
    }
}

/// Rekurencyjny rozmiar katalogu; nie podąża za dowiązaniami symbolicznymi.
fn dir_size(path: &Path) -> DirSize {
    let mut out = DirSize::default();
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            let file_type = meta.file_type();
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                out.dirs += 1;
                stack.push(path);
            } else if file_type.is_file() {
                out.files += 1;
                out.bytes += meta.len();
            }
        }
    }
    out
}

fn preview_for(entry: &Entry) -> Preview {
    if entry.is_dir {
        return Preview::None;
    }
    match entry.kind {
        Kind::Image => {
            // Duże pliki pomijamy — podgląd nie może zamrozić klatki.
            let size = std::fs::metadata(&entry.path).map(|m| m.len()).unwrap_or(0);
            if size > 64 * 1024 * 1024 {
                return Preview::None;
            }
            let extension = entry
                .path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let bytes = match std::fs::read(&entry.path) {
                Ok(bytes) => bytes,
                Err(_) => return Preview::None,
            };
            let image = match extension.as_str() {
                "png" => libluna_render::wallpaper::decode_png(&bytes),
                "jpg" | "jpeg" => decode_jpeg(&bytes),
                _ => None,
            };
            match image {
                Some(image) => Preview::Image(Arc::new(fit_preview(image, 1400))),
                None => Preview::None,
            }
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

/// Dekoduje JPEG do RGBA8 (bez EXIF-owej orientacji).
fn decode_jpeg(bytes: &[u8]) -> Option<ImageData> {
    use jpeg_decoder::PixelFormat;
    let mut decoder = jpeg_decoder::Decoder::new(bytes);
    // Najpierw metadane, potem dekodowanie ze skalowaniem DCT — duże zdjęcia
    // (12 Mpx) nie zajmują wtedy kilkudziesięciu megabajtów na podgląd.
    decoder.read_info().ok()?;
    let original = decoder.info()?;
    const MAX_SIDE: u16 = 1400;
    if original.width > MAX_SIDE || original.height > MAX_SIDE {
        let scale = MAX_SIDE as f32 / original.width.max(original.height) as f32;
        let _ = decoder.scale(
            (original.width as f32 * scale).max(1.0) as u16,
            (original.height as f32 * scale).max(1.0) as u16,
        );
    }
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    let (width, height) = (info.width as usize, info.height as usize);
    let mut rgba = Vec::with_capacity(width * height * 4);
    match info.pixel_format {
        PixelFormat::RGB24 => {
            for px in pixels.chunks_exact(3) {
                rgba.extend_from_slice(&[px[0], px[1], px[2], 0xff]);
            }
        }
        PixelFormat::L8 => {
            for value in pixels {
                rgba.extend_from_slice(&[value, value, value, 0xff]);
            }
        }
        PixelFormat::CMYK32 => {
            for px in pixels.chunks_exact(4) {
                let (c, m, y, k) = (px[0] as u16, px[1] as u16, px[2] as u16, px[3] as u16);
                let r = ((255 - c) * (255 - k) / 255) as u8;
                let g = ((255 - m) * (255 - k) / 255) as u8;
                let b = ((255 - y) * (255 - k) / 255) as u8;
                rgba.extend_from_slice(&[r, g, b, 0xff]);
            }
        }
        PixelFormat::L16 => return None,
    }
    Some(ImageData::new(info.width as u32, info.height as u32, rgba))
}

/// Zmniejsza podgląd do `max` pikseli na dłuższym boku (najbliższy sąsiad).
fn fit_preview(image: ImageData, max: u32) -> ImageData {
    let (w, h) = (image.width.max(1), image.height.max(1));
    if w <= max && h <= max {
        return image;
    }
    let scale = max as f64 / w.max(h) as f64;
    let nw = (w as f64 * scale).round().max(1.0) as u32;
    let nh = (h as f64 * scale).round().max(1.0) as u32;
    let mut out = vec![0u8; nw as usize * nh as usize * 4];
    for y in 0..nh {
        let sy = (y as u64 * h as u64 / nh as u64) as usize;
        for x in 0..nw {
            let sx = (x as u64 * w as u64 / nw as u64) as usize;
            let src = (sy * w as usize + sx) * 4;
            let dst = (y as usize * nw as usize + x as usize) * 4;
            out[dst..dst + 4].copy_from_slice(&image.rgba[src..src + 4]);
        }
    }
    ImageData::new(nw, nh, out)
}

fn spawn_terminal(program: &str, dir: &str) -> bool {
    let args: &[&str] = match program {
        // `luna-terminal` startuje w `current_dir` (zob. `pty.rs`), więc katalog
        // przekazujemy przez `current_dir`, a nie argumentem.
        "luna-terminal" => &[],
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
    // Język z konfiguracji aplikacji ma pierwszeństwo nad `LANG` — jego wybór
    // w Ustawieniach ma być trwały między uruchomieniami.
    let prefs = Prefs::load();
    set_locale(libluna_i18n::preferred_locale());
    libluna_i18n::load_catalogs_from_dirs();
    let title_key = title_key.to_owned();
    let title = tr(&title_key);

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
        .system_dark(system_dark)
        // Esc zamyka menu/podglądy, ale nie całe okno.
        .escape_to_exit(false);

    let mut app = App::with_prefs(start, prefs);
    app.status = app.status_text();
    // Język może się zmienić w Ustawieniach w trakcie działania — sidebar ma
    // zapisane etykiety, więc przebudowujemy go po wykryciu nowego katalogu.
    let mut ui_language = libluna_i18n::locale().language().to_owned();

    run_app(config, move |frame: &mut Frame| {
        let language = libluna_i18n::locale().language().to_owned();
        if language != ui_language {
            ui_language = language.clone();
            app.prefs.language = language;
            app.save_prefs();
            app.rebuild_sidebar();
            frame.set_title(tr(&title_key));
        }
        let theme = frame.settings().resolve_themed(frame.system_dark());
        app.theme_pref = frame.settings().preference();
        app.drain_ops();
        app.refresh_devices_if_needed();

        ui::handle_events(&mut app, frame.events, frame.size, &theme, frame.text);

        // Ctrl+C/X w polach tekstowych → systemowy schowek; Ctrl+V → odczyt.
        if let Some(text) = app.text_clipboard.take() {
            frame.copy_to_clipboard(text);
        }
        if app.text_paste_requested {
            app.text_paste_requested = false;
            frame.paste_from_clipboard();
        }

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
        if let Some(pref) = app.pending_theme.take() {
            frame.set_preference(pref);
        }

        app.update_preview();
        app.poll_properties();
        app.status = app.status_text();

        ui::paint(&app, frame.scene, frame.size, &theme, frame.text);

        if app.toast.is_animating() {
            app.toast.advance(frame.delta_time(), 0.3);
            frame.request_frame();
        }
        if let Some(until) = app.toast_until {
            if Instant::now() >= until {
                app.toast.set_open_animated(false);
                app.toast_until = None;
            }
            frame.request_frame();
        }
        if app.ops.busy() {
            frame.request_frame();
        }
        // Budzimy aplikacje cyklicznie: podłączone USB ma się pojawić samo (bez
        // ruchu myszy), a dymek podpowiedzi ma się pokazać po chwili najechania.
        frame.request_frame_after(Duration::from_millis(500));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_gets_an_expiry_deadline() {
        let mut app = App::new(std::env::temp_dir());
        app.show_toast("Skopiowano");
        assert!(app.toast.is_open());
        assert!(app.toast_until.is_some());
    }

    #[test]
    fn dir_size_counts_nested_files() {
        let dir = std::env::temp_dir().join("luna-files-dir-size-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("podkatalog/glebiej")).unwrap();
        std::fs::write(dir.join("a.txt"), b"12345").unwrap();
        std::fs::write(dir.join("podkatalog/b.bin"), vec![0u8; 7]).unwrap();
        std::fs::write(dir.join("podkatalog/glebiej/c"), vec![0u8; 3]).unwrap();
        let size = dir_size(&dir);
        assert_eq!(size.files, 3);
        assert_eq!(size.bytes, 15);
        assert_eq!(size.dirs, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_properties_finish_in_background() {
        let dir = std::env::temp_dir().join("luna-files-properties-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("plik"), vec![0u8; 9]).unwrap();
        let mut app = App::new(std::env::temp_dir());
        app.show_properties(&dir);
        assert!(app.pending_properties.is_some());
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.pending_properties.is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            app.poll_properties();
        }
        assert!(
            app.pending_properties.is_none(),
            "obliczanie nie zakończyło się"
        );
        assert!(app.toast.message().contains("Folder") || app.toast.message().contains("Katalog"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_downscale_keeps_aspect_ratio() {
        let image = ImageData::new(400, 200, vec![255; 400 * 200 * 4]);
        let small = fit_preview(image, 100);
        assert_eq!((small.width, small.height), (100, 50));
    }

    #[test]
    fn decodes_a_small_jpeg() {
        // 2×1, czerwony — minimalny poprawny JPEG (wygenerowany ImageMagickiem).
        const JPEG: &[u8] = &[
            255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0, 255, 219, 0,
            67, 0, 3, 2, 2, 2, 2, 2, 3, 2, 2, 2, 3, 3, 3, 3, 4, 6, 4, 4, 4, 4, 4, 8, 6, 6, 5, 6, 9,
            8, 10, 10, 9, 8, 9, 9, 10, 12, 15, 12, 10, 11, 14, 11, 9, 9, 13, 17, 13, 14, 15, 16,
            16, 17, 16, 10, 12, 18, 19, 18, 16, 19, 15, 16, 16, 16, 255, 219, 0, 67, 1, 3, 3, 3, 4,
            3, 4, 8, 4, 4, 8, 16, 11, 9, 11, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16,
            16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16,
            16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 255, 192, 0, 17, 8, 0,
            1, 0, 2, 3, 1, 17, 0, 2, 17, 1, 3, 17, 1, 255, 196, 0, 20, 0, 1, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 8, 255, 196, 0, 20, 16, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 255, 196, 0, 21, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7, 9,
            255, 196, 0, 20, 17, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 218, 0,
            12, 3, 1, 0, 2, 17, 3, 17, 0, 63, 0, 58, 3, 21, 77, 255, 217,
        ];
        let image = decode_jpeg(JPEG).expect("JPEG powinien się zdekodować");
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.rgba.len(), 2 * 4);
        assert!(image.rgba[3] == 255, "kanał alfa");
    }
}
