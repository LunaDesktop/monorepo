//! Stan menedżera zadań i pętla klatki.
//!
//! Aplikacja jest „immediate mode”: wiersze listy rysujemy co klatkę, a kliknięcia
//! trafiają w prostokąty zebrane w [`App::hits`]. Dzięki temu obsługa zdarzeń nie
//! powtarza całego układu, a podglądy headless mogą narysować dowolny stan.

use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::time::{Duration, Instant};

use libluna_core::{Point, Rect, ThemeSettings};
use libluna_i18n::{load_catalogs_from_dirs, preferred_locale, set_locale, tr, tr_args};
use libluna_platform::{run as run_app, AppConfig, Frame};
use libluna_widgets::{
    AlertDialog, Menu, MenuEntry, ScrollArea, SearchEntry, Toast, WindowControl,
};

use crate::prefs::Prefs;
use crate::procs::{self, Process, Snapshot};

/// Odstęp między próbkami `/proc`.
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(1);

/// Odstęp próbek w oknie szczegółów procesu (wątki potrzebują dwóch próbek,
/// żeby policzyć CPU; 400 ms to kompromis między reakcją a szumem).
pub const DETAILS_INTERVAL: Duration = Duration::from_millis(400);

/// Ile próbek trzyma historia wykresów (przy 1 s = ostatnia minuta).
pub const CHART_SAMPLES: usize = 60;

/// Pierścień próbek 0.0–1.0 dla jednego wykresu.
#[derive(Debug, Clone, Default)]
pub struct ChartHistory {
    pub samples: VecDeque<f32>,
}

impl ChartHistory {
    pub fn push(&mut self, sample: f32) {
        self.samples.push_back(sample.clamp(0.0, 1.0));
        while self.samples.len() > CHART_SAMPLES {
            self.samples.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn last(&self) -> Option<f32> {
        self.samples.back().copied()
    }
}

/// Historia wykresów: system, pamięć i zaznaczony proces.
#[derive(Debug, Clone, Default)]
pub struct Charts {
    pub system_cpu: ChartHistory,
    pub system_memory: ChartHistory,
    pub selected_cpu: ChartHistory,
    /// PID, którego dotyczy `selected_cpu` (zmiana zaznaczenia czyści historię).
    pub pid: Option<u32>,
}

impl Charts {
    /// Dopisuje jedną próbkę z bieżącej migawki.
    pub fn record(&mut self, system: &crate::procs::SystemInfo, selected: Option<(u32, f32)>) {
        self.system_cpu.push(system.cpu_percent / 100.0);
        let memory = if system.memory_total_kib == 0 {
            0.0
        } else {
            system.memory_used_kib() as f32 / system.memory_total_kib as f32
        };
        self.system_memory.push(memory);
        match selected {
            Some((pid, cpu)) => {
                if self.pid != Some(pid) {
                    self.selected_cpu.clear();
                    self.pid = Some(pid);
                }
                self.selected_cpu.push(cpu / 100.0);
            }
            None => {
                if self.pid.is_some() {
                    self.selected_cpu.clear();
                    self.pid = None;
                }
            }
        }
    }
}

/// Klucz sortowania listy procesów.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Cpu,
    Memory,
    Pid,
    Name,
    User,
}

impl SortKey {
    pub const ALL: [SortKey; 5] = [
        SortKey::Cpu,
        SortKey::Memory,
        SortKey::Pid,
        SortKey::Name,
        SortKey::User,
    ];

    /// Etykieta kolumny (klucz tłumaczenia).
    pub const fn title_key(self) -> &'static str {
        match self {
            SortKey::Cpu => "CPU",
            SortKey::Memory => "Memory",
            SortKey::Pid => "PID",
            SortKey::Name => "Name",
            SortKey::User => "User",
        }
    }

    /// Wartość zapisywana w `tasks.conf`.
    pub const fn name(self) -> &'static str {
        match self {
            SortKey::Cpu => "cpu",
            SortKey::Memory => "memory",
            SortKey::Pid => "pid",
            SortKey::Name => "name",
            SortKey::User => "user",
        }
    }

    pub fn from_name(name: &str) -> Option<SortKey> {
        match name.trim().to_ascii_lowercase().as_str() {
            "cpu" => Some(SortKey::Cpu),
            "memory" | "ram" => Some(SortKey::Memory),
            "pid" => Some(SortKey::Pid),
            "name" => Some(SortKey::Name),
            "user" => Some(SortKey::User),
            _ => None,
        }
    }

    /// Domyślny kierunek przy pierwszym kliknięciu kolumny.
    pub const fn default_ascending(self) -> bool {
        matches!(self, SortKey::Pid | SortKey::Name | SortKey::User)
    }
}

/// Akcja interfejsu zebrana podczas rysowania klatki.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Sort(SortKey),
    Row(u32),
    ToggleOnlyMine,
    EndSelected,
    ForceKillSelected,
}

/// Pozycja menu głównego.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Refresh,
    ToggleCharts,
    ToggleScope,
    About,
    Quit,
}

/// Otwarte menu pod przyciskiem „burger”.
pub struct MenuState {
    pub menu: Menu,
    pub anchor: Rect,
    pub actions: Vec<Option<MenuAction>>,
}

/// Proces czekający na potwierdzenie zakończenia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub pid: u32,
    /// Czas startu chroni przed wysłaniem sygnału do procesu po recyklingu PID.
    pub start_jiffies: u64,
    pub name: String,
    pub force: bool,
}

/// Pozycja menu kontekstowego (prawy przycisk myszy na procesie).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextAction {
    End,
    ForceKill,
    CopyPid,
    CopyCommand,
    Details,
    Refresh,
    ToggleScope,
}

/// Okno „Szczegóły procesu”: podsumowanie + rozbicie na wątki i I/O.
///
/// Wątki mają sens tylko względem dwóch próbek — dlatego trzymamy poprzednie
/// `tid → jiffies` i liczymy CPU dokładnie tak samo jak dla procesów.
#[derive(Debug, Clone)]
pub struct Details {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub user: String,
    pub state: char,
    /// Liczba wątków z `/proc/<pid>/status` (może być większa niż lista próbki).
    pub threads_total: u32,
    pub threads: Vec<procs::ThreadInfo>,
    pub io: Option<procs::IoStats>,
    /// CPU całego procesu z ostatniej migawki (dla nagłówka).
    pub cpu_percent: f32,
    pub rss_kib: u64,
    pub sampled: Instant,
    last: HashMap<u32, u64>,
}

impl Details {
    pub fn open(
        pid: u32,
        ppid: u32,
        name: String,
        user: String,
        state: char,
        threads_total: u32,
        cpu_percent: f32,
        rss_kib: u64,
    ) -> Self {
        Self {
            pid,
            ppid,
            name,
            user,
            state,
            threads_total,
            threads: Vec::new(),
            io: None,
            cpu_percent,
            rss_kib,
            sampled: Instant::now(),
            last: HashMap::new(),
        }
    }

    /// Odświeża wątki i I/O. Pierwsza próbka ma CPU wątków zerowe (brak delty).
    pub fn sample(&mut self) {
        let elapsed = self.sampled.elapsed();
        self.threads = procs::read_threads(self.pid, &self.last, elapsed, procs::clock_ticks());
        self.last = self
            .threads
            .iter()
            .map(|thread| (thread.tid, thread.cpu_jiffies))
            .collect();
        self.io = procs::read_io(self.pid);
        self.sampled = Instant::now();
    }

    /// Suma CPU wątków (pozwala złapać proces wielowątkowy, gdy `top` pokazuje mniej).
    pub fn threads_cpu(&self) -> f32 {
        self.threads.iter().map(|thread| thread.cpu_percent).sum()
    }
}

/// Otwarte menu kontekstowe przy wskazanym procesie.
pub struct ContextState {
    pub menu: Menu,
    pub anchor: Rect,
    pub pid: Option<u32>,
    pub actions: Vec<Option<ContextAction>>,
}

/// Stan odświeżania po klatce.
pub struct Update {
    pub animating: bool,
    pub delay: Option<Duration>,
}

/// Stan aplikacji.
pub struct App {
    pub snapshot: Snapshot,
    /// Czy odświeżać z prawdziwego `/proc` (podglądy i testy mają to wyłączone).
    pub live: bool,
    /// Czy zapisywać preferencje (w testach wyłączone).
    pub persist_prefs: bool,
    pub uid: u32,
    pub search: SearchEntry,
    /// Czy pole wyszukiwania jest widoczne w pasku tytułu.
    pub search_active: bool,
    /// Czy nad listą pokazywać wykresy CPU/RAM/procesu.
    pub show_charts: bool,
    pub charts: Charts,
    pub sort: SortKey,
    pub sort_asc: bool,
    pub only_mine: bool,
    /// PID zaznaczonego procesu (stabilny między odświeżeniami).
    pub selected: Option<u32>,
    /// Indeksy procesów po filtrze i sortowaniu.
    pub filtered: Vec<usize>,
    pub scroll: ScrollArea,
    pub menu: Option<MenuState>,
    pub context: Option<ContextState>,
    pub about: Option<AlertDialog>,
    pub confirm: Option<Confirm>,
    /// Otwarte okno szczegółów procesu (PPM → „Szczegóły procesu”).
    pub details: Option<Details>,
    pub toast: Toast,
    pub toast_until: Option<Instant>,
    pub pointer: Option<Point>,
    pub controls_hover: Option<WindowControl>,
    pub pending_close: bool,
    pub pending_minimize: bool,
    pub pending_maximize: bool,
    pub pending_move: bool,
    /// Prostokąty i akcje z ostatniej klatki.
    pub hits: Vec<(Action, Rect)>,
    /// Czy okno ma fokus (bez fokusu nie odświeżamy `/proc`).
    pub focused: bool,
    /// Obszar wierszy z ostatniej klatki — potrzebny do przewijania do wyboru.
    pub list_viewport: Rect,
    pub clipboard_text: Option<String>,
    pub pending_paste: bool,
    next_refresh: Instant,
    next_details: Instant,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Pusta aplikacja do testów (bez dotykania `/proc`).
    pub fn new() -> Self {
        Self::with_snapshot(Snapshot::default(), Prefs::default())
    }

    /// Aplikacja z gotową migawką (podglądy headless, testy).
    pub fn with_snapshot(snapshot: Snapshot, prefs: Prefs) -> Self {
        let mut app = Self {
            snapshot,
            live: false,
            persist_prefs: false,
            uid: procs::current_uid(),
            search: SearchEntry::new(tr("Search processes…")),
            search_active: false,
            show_charts: prefs.show_charts,
            charts: Charts::default(),
            sort: prefs.sort,
            sort_asc: prefs.sort_asc,
            only_mine: prefs.only_mine,
            selected: None,
            filtered: Vec::new(),
            scroll: ScrollArea::new(),
            menu: None,
            context: None,
            about: None,
            confirm: None,
            details: None,
            toast: Toast::default(),
            toast_until: None,
            pointer: None,
            controls_hover: None,
            pending_close: false,
            pending_minimize: false,
            pending_maximize: false,
            pending_move: false,
            hits: Vec::new(),
            focused: true,
            list_viewport: Rect::from_xywh(0.0, 0.0, 0.0, 0.0),
            clipboard_text: None,
            pending_paste: false,
            next_refresh: Instant::now() + REFRESH_INTERVAL,
            next_details: Instant::now(),
        };
        app.recompute();
        app
    }

    /// Aplikacja produkcyjna: czyta `/proc` i zapisuje preferencje.
    pub fn launch() -> Self {
        let mut app = Self::with_snapshot(Snapshot::read(None), Prefs::load());
        app.live = true;
        app.persist_prefs = true;
        app
    }

    pub fn preferred_size(&self) -> (u32, u32) {
        (980, 700)
    }

    pub fn window_title(&self) -> String {
        tr("Task Manager")
    }

    /// Przelicza filtr i kolejność po zmianie danych albo ustawień listy.
    pub fn recompute(&mut self) {
        let query = self.search.text().trim().to_owned();
        let uid = self.uid;
        let only_mine = self.only_mine;
        let sort = self.sort;
        let ascending = self.sort_asc;
        let processes = &self.snapshot.processes;

        let mut filtered: Vec<usize> = processes
            .iter()
            .enumerate()
            .filter(|(_, process)| {
                (!only_mine || process.uid == uid) && matches_query(process, &query)
            })
            .map(|(index, _)| index)
            .collect();
        filtered.sort_by(|left, right| {
            let order = compare(&processes[*left], &processes[*right], sort);
            if ascending {
                order
            } else {
                order.reverse()
            }
        });

        self.filtered = filtered;
        if self.selected.is_some_and(|pid| {
            !self
                .filtered
                .iter()
                .any(|index| self.snapshot.processes[*index].pid == pid)
        }) {
            // Wybrany proces zniknął albo wypadł z filtra — nie trzymaj martwego wyboru.
            self.selected = None;
        }
    }

    pub fn selected_process(&self) -> Option<&Process> {
        let pid = self.selected?;
        self.snapshot
            .processes
            .iter()
            .find(|process| process.pid == pid)
    }

    /// Pozycja zaznaczenia na liście po filtrze.
    pub fn selected_row(&self) -> Option<usize> {
        let pid = self.selected?;
        self.filtered
            .iter()
            .position(|index| self.snapshot.processes[*index].pid == pid)
    }

    pub fn set_sort(&mut self, key: SortKey) {
        if self.sort == key {
            self.sort_asc = !self.sort_asc;
        } else {
            self.sort = key;
            self.sort_asc = key.default_ascending();
        }
        self.recompute();
        self.save_prefs();
    }

    pub fn set_only_mine(&mut self, only_mine: bool) {
        self.only_mine = only_mine;
        self.recompute();
        self.save_prefs();
    }

    pub fn toggle_only_mine(&mut self) {
        self.set_only_mine(!self.only_mine);
    }

    pub fn save_prefs(&self) {
        if self.persist_prefs {
            Prefs {
                sort: self.sort,
                sort_asc: self.sort_asc,
                only_mine: self.only_mine,
                show_charts: self.show_charts,
            }
            .save();
        }
    }

    /// Pokazuje pole wyszukiwania w pasku tytułu i ustawia na nim fokus.
    pub fn activate_search(&mut self) {
        self.search_active = true;
        self.search.set_focused(true);
    }

    /// Chowa pole wyszukiwania i czyści filtr.
    pub fn close_search(&mut self) {
        self.search_active = false;
        self.search.set_focused(false);
        self.search.clear();
        self.recompute();
    }

    /// Lupka w pasku tytułu: pokaż albo schowaj wyszukiwanie.
    pub fn toggle_search(&mut self) {
        if self.search_active {
            self.close_search();
        } else {
            self.activate_search();
        }
    }

    /// Włącza/wyłącza wykresy nad listą (zapis w `tasks.conf`).
    pub fn set_show_charts(&mut self, show: bool) {
        self.show_charts = show;
        self.save_prefs();
    }

    pub fn toggle_charts(&mut self) {
        self.set_show_charts(!self.show_charts);
    }

    pub fn show_toast(&mut self, message: impl Into<String>) {
        self.toast.set_message(message);
        self.toast.set_open_animated(true);
        self.toast_until = Some(Instant::now() + Duration::from_secs(3));
    }

    /// Przechodzi o `delta` wierszy po widocznej liście.
    pub fn move_selection(&mut self, delta: i64) {
        let len = self.filtered.len();
        if len == 0 {
            self.selected = None;
            return;
        }
        let index = match self.selected_row() {
            Some(current) => (current as i64 + delta).clamp(0, len as i64 - 1) as usize,
            None if delta > 0 => 0,
            None => len - 1,
        };
        self.selected = Some(self.snapshot.processes[self.filtered[index]].pid);
        self.ensure_visible(index);
    }

    pub fn select_first(&mut self) {
        self.move_selection(i64::MIN / 2);
    }

    pub fn select_last(&mut self) {
        self.move_selection(i64::MAX / 2);
    }

    /// Przewija listę tak, żeby wybrany wiersz był cały widoczny.
    fn ensure_visible(&mut self, index: usize) {
        let viewport = self.list_viewport;
        if viewport.height() <= 0.0 {
            return;
        }
        let row_height = crate::ui::ROW_HEIGHT;
        let top = index as f64 * row_height;
        let offset = self.scroll.offset();
        if top < offset {
            self.scroll.set_offset(top, viewport);
        } else if top + row_height > offset + viewport.height() {
            self.scroll
                .set_offset(top + row_height - viewport.height(), viewport);
        }
    }

    /// Otwiera menu kontekstowe (PPM) przy procesie `pid`.
    pub fn open_context(&mut self, pid: u32, anchor: Rect) {
        let Some(process) = self
            .snapshot
            .processes
            .iter()
            .find(|process| process.pid == pid)
        else {
            return;
        };
        let name = process.name.clone();
        let has_command = !process.cmdline.trim().is_empty();
        let can_signal = process.uid == self.uid || self.uid == 0;
        let can_kill = can_signal && pid > 1;
        let scope = if self.only_mine {
            tr("Show all processes")
        } else {
            tr("Only my processes")
        };

        let end = MenuEntry::action(tr("End process"));
        let force = MenuEntry::action(tr("Force kill"));
        let copy_command = MenuEntry::action(tr("Copy command line"));
        let entries = vec![
            MenuEntry::section(format!("{name} · PID {pid}")),
            if can_signal { end } else { end.disabled() },
            if can_kill { force } else { force.disabled() },
            MenuEntry::separator(),
            MenuEntry::action(tr("Copy PID")),
            if has_command {
                copy_command
            } else {
                copy_command.disabled()
            },
            MenuEntry::separator(),
            // Rozbicie na wątki i I/O — „co w tym procesie zjada zasoby”.
            MenuEntry::action(tr("Process details")),
            MenuEntry::separator(),
            MenuEntry::action(tr("Refresh")).shortcut("Ctrl+R"),
            MenuEntry::action(scope),
        ];

        self.menu = None;
        self.search.set_focused(false);
        self.selected = Some(pid);
        self.context = Some(ContextState {
            menu: Menu::new(entries),
            anchor,
            pid: Some(pid),
            actions: vec![
                None,
                Some(ContextAction::End),
                Some(ContextAction::ForceKill),
                None,
                Some(ContextAction::CopyPid),
                Some(ContextAction::CopyCommand),
                None,
                Some(ContextAction::Details),
                None,
                Some(ContextAction::Refresh),
                Some(ContextAction::ToggleScope),
            ],
        });
    }

    /// Menu kontekstowe dla aktualnie zaznaczonego procesu (np. `Shift+F10`).
    pub fn open_context_for_selected(&mut self, anchor: Rect) {
        if let Some(pid) = self.selected {
            self.open_context(pid, anchor);
        }
    }

    /// Otwiera okno szczegółów procesu (PPM → „Szczegóły procesu”).
    pub fn open_details(&mut self, pid: u32) {
        let Some(process) = self
            .snapshot
            .processes
            .iter()
            .find(|process| process.pid == pid)
        else {
            return;
        };
        let mut details = Details::open(
            pid,
            process.ppid,
            process.name.clone(),
            process.user.clone(),
            process.state,
            process.threads,
            process.cpu_percent,
            process.rss_kib,
        );
        details.sample();
        self.details = Some(details);
        self.next_details = Instant::now() + DETAILS_INTERVAL;
        self.context = None;
        self.menu = None;
    }

    /// Wykonuje pozycję menu kontekstowego.
    pub fn run_context(&mut self, index: usize) {
        let Some(state) = &self.context else {
            return;
        };
        let action = state.actions.get(index).copied().flatten();
        let pid = state.pid;
        self.context = None;
        let Some(action) = action else {
            return;
        };
        if let Some(pid) = pid {
            self.selected = Some(pid);
        }
        match action {
            ContextAction::End => self.ask_end(false),
            ContextAction::ForceKill => self.ask_end(true),
            ContextAction::CopyPid => {
                if let Some(pid) = pid {
                    self.clipboard_text = Some(pid.to_string());
                    self.show_toast(tr("PID copied"));
                }
            }
            ContextAction::CopyCommand => {
                let command = pid
                    .and_then(|pid| {
                        self.snapshot
                            .processes
                            .iter()
                            .find(|process| process.pid == pid)
                    })
                    .map(|process| process.cmdline.clone())
                    .filter(|command| !command.trim().is_empty());
                if let Some(command) = command {
                    self.clipboard_text = Some(command);
                    self.show_toast(tr("Command line copied"));
                }
            }
            ContextAction::Refresh => {
                self.refresh();
                self.show_toast(tr("Process list refreshed"));
            }
            ContextAction::Details => {
                if let Some(pid) = pid {
                    self.open_details(pid);
                }
            }
            ContextAction::ToggleScope => self.toggle_only_mine(),
        }
    }

    /// Otwiera potwierdzenie zakończenia zaznaczonego procesu.
    pub fn ask_end(&mut self, force: bool) {
        let Some(process) = self.selected_process() else {
            self.show_toast(tr("Select a process first"));
            return;
        };
        if process.pid <= 1 {
            self.show_toast(tr("PID 1 cannot be ended"));
            return;
        }
        if process.uid != self.uid && self.uid != 0 {
            self.show_toast(tr("Not permitted for this process"));
            return;
        }
        self.confirm = Some(Confirm {
            pid: process.pid,
            start_jiffies: process.start_jiffies,
            name: process.name.clone(),
            force,
        });
    }

    /// Wysyła sygnał po potwierdzeniu. Sprawdza, czy PID nie został w międzyczasie
    /// przydzielony innemu procesowi.
    pub fn execute_confirm(&mut self) {
        let Some(confirm) = self.confirm.take() else {
            return;
        };
        let still_same = self.snapshot.processes.iter().any(|process| {
            process.pid == confirm.pid && process.start_jiffies == confirm.start_jiffies
        });
        if !still_same {
            self.show_toast(tr("Process already ended"));
            self.next_refresh = Instant::now();
            return;
        }
        let signal = if confirm.force {
            procs::SIGKILL
        } else {
            procs::SIGTERM
        };
        match procs::send_signal(confirm.pid, signal) {
            Ok(()) => self.show_toast(if confirm.force {
                tr_args("Sent kill signal to {name}", &[("name", &confirm.name)])
            } else {
                tr_args("Sent end signal to {name}", &[("name", &confirm.name)])
            }),
            Err(error) => {
                let message = if error.kind() == std::io::ErrorKind::PermissionDenied {
                    tr("Not permitted for this process")
                } else if error.raw_os_error() == Some(libc::ESRCH) {
                    tr("Process already ended")
                } else {
                    tr("Could not signal the process")
                };
                self.show_toast(message);
            }
        }
        // Daj procesowi chwilę na zniknięcie z `/proc`.
        self.next_refresh = Instant::now() + Duration::from_millis(250);
    }

    /// Ponowna próbka `/proc` (albo przeliczenie filtra w podglądach).
    pub fn refresh(&mut self) {
        if self.live {
            self.snapshot = Snapshot::read(Some(&self.snapshot));
        }
        self.recompute();
        // Historia wykresów rośnie przy każdej próbce (1 s = ostatnia minuta).
        let selected = self
            .selected_process()
            .map(|process| (process.pid, process.cpu_percent));
        self.charts.record(&self.snapshot.system, selected);
        self.next_refresh = Instant::now() + REFRESH_INTERVAL;
        // Nagłówek otwartych szczegółów ma świeże CPU/RAM z migawki procesów
        // (wątki mają własny rytm próbkowania — patrz `update`).
        if let Some(details) = self.details.as_mut() {
            if let Some(process) = self
                .snapshot
                .processes
                .iter()
                .find(|process| process.pid == details.pid)
            {
                details.cpu_percent = process.cpu_percent;
                details.rss_kib = process.rss_kib;
                details.name = process.name.clone();
                details.user = process.user.clone();
                details.state = process.state;
                details.threads_total = process.threads;
                details.ppid = process.ppid;
            }
        }
    }

    pub fn update(&mut self, dt: f64) -> Update {
        let mut animating = self.toast.advance(dt, 0.18);
        if self
            .toast_until
            .is_some_and(|until| Instant::now() >= until)
        {
            self.toast.set_open_animated(false);
            self.toast_until = None;
        }

        // Bez fokusu nie ma sensu czytać `/proc` co sekundę — oszczędzamy CPU.
        if self.focused && Instant::now() >= self.next_refresh {
            self.refresh();
        }

        // Otwarte szczegóły próbkujemy częściej niż listę — dopiero wtedy widać,
        // który wątek naprawdę pracuje, a nie tylko średnią całego procesu.
        if self.details.is_some() && self.focused && Instant::now() >= self.next_details {
            if let Some(details) = self.details.as_mut() {
                details.sample();
            }
            self.next_details = Instant::now() + DETAILS_INTERVAL;
        }

        let mut delay = if self.toast.is_open() || self.toast.is_animating() {
            Some(Duration::from_millis(100))
        } else {
            Some(
                self.next_refresh
                    .saturating_duration_since(Instant::now())
                    .max(Duration::from_millis(50)),
            )
        };
        if self.details.is_some() {
            let details_delay = self
                .next_details
                .saturating_duration_since(Instant::now())
                .max(Duration::from_millis(50));
            delay = Some(delay.map_or(details_delay, |current| current.min(details_delay)));
        }
        animating |= self.toast.is_animating();
        Update { animating, delay }
    }

    /// Otwiera menu główne pod podanym przyciskiem.
    pub fn open_menu(&mut self, anchor: Rect) {
        self.context = None;
        let scope = if self.only_mine {
            tr("Show all processes")
        } else {
            tr("Only my processes")
        };
        let entries = vec![
            MenuEntry::action(tr("Refresh")).shortcut("Ctrl+R / F5"),
            MenuEntry::action(if self.show_charts {
                tr("Hide charts")
            } else {
                tr("Show charts")
            })
            .shortcut("Ctrl+G"),
            MenuEntry::separator(),
            MenuEntry::action(scope),
            MenuEntry::separator(),
            MenuEntry::action(tr("About Task Manager")).shortcut("F1"),
            MenuEntry::separator(),
            MenuEntry::action(tr("Quit")).shortcut("Ctrl+Q"),
        ];
        self.menu = Some(MenuState {
            menu: Menu::new(entries),
            anchor,
            actions: vec![
                Some(MenuAction::Refresh),
                Some(MenuAction::ToggleCharts),
                None,
                Some(MenuAction::ToggleScope),
                None,
                Some(MenuAction::About),
                None,
                Some(MenuAction::Quit),
            ],
        });
    }

    pub fn run_menu(&mut self, index: usize) {
        let action = self
            .menu
            .as_ref()
            .and_then(|state| state.actions.get(index).copied().flatten());
        self.menu = None;
        match action {
            Some(MenuAction::Refresh) => {
                self.refresh();
                self.show_toast(tr("Process list refreshed"));
            }
            Some(MenuAction::ToggleCharts) => self.toggle_charts(),
            Some(MenuAction::ToggleScope) => self.toggle_only_mine(),
            Some(MenuAction::About) => self.show_about(),
            Some(MenuAction::Quit) => self.pending_close = true,
            None => {}
        }
    }

    pub fn show_about(&mut self) {
        self.menu = None;
        self.context = None;
        let body = format!(
            "Luna Tasks {}\n{}\n\n{}\n\n© 2026 Luna Project · GPL-3.0-or-later",
            env!("CARGO_PKG_VERSION"),
            tr("Process and system monitor for the Luna desktop."),
            tr("Everything is read locally from /proc. No network access and no data leaves this computer."),
        );
        self.about = Some(
            AlertDialog::new(tr("About Task Manager"), body)
                .width(520.0)
                .button(tr("Done")),
        );
    }
}

/// Czy proces pasuje do tekstu wyszukiwania (PID, nazwa, polecenie, użytkownik).
pub fn matches_query(process: &Process, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    process.pid.to_string().contains(&query)
        || process.name.to_lowercase().contains(&query)
        || process.cmdline.to_lowercase().contains(&query)
        || process.user.to_lowercase().contains(&query)
}

fn compare(left: &Process, right: &Process, key: SortKey) -> Ordering {
    match key {
        SortKey::Cpu => left
            .cpu_percent
            .total_cmp(&right.cpu_percent)
            .then_with(|| left.pid.cmp(&right.pid)),
        SortKey::Memory => left
            .rss_kib
            .cmp(&right.rss_kib)
            .then_with(|| left.pid.cmp(&right.pid)),
        SortKey::Pid => left.pid.cmp(&right.pid),
        SortKey::Name => left
            .name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.pid.cmp(&right.pid)),
        SortKey::User => left
            .user
            .to_lowercase()
            .cmp(&right.user.to_lowercase())
            .then_with(|| left.pid.cmp(&right.pid)),
    }
}

/// Uruchamia aplikację.
pub fn run() -> Result<(), Box<dyn Error>> {
    set_locale(preferred_locale());
    load_catalogs_from_dirs();

    let mut app = App::launch();
    let settings = ThemeSettings::load();
    let system_dark = libluna_core::appearance::system_dark();
    let (width, height) = app.preferred_size();
    let config = AppConfig::new(tr("Task Manager"))
        .app_id("eu.luna.Tasks")
        .initial_size(width, height)
        .settings(settings)
        .system_dark(system_dark)
        .escape_to_exit(false);

    let mut last_title = String::new();
    run_app(config, move |frame: &mut Frame| {
        let theme = frame.settings().resolve_themed(frame.system_dark());
        crate::ui::handle_events(&mut app, frame.events, frame.size, &theme, frame.text);
        let update = app.update(frame.delta_time());

        let title = app.window_title();
        if title != last_title {
            frame.set_title(title.clone());
            last_title = title;
        }

        crate::ui::paint(&mut app, frame.scene, frame.size, &theme, frame.text);

        if let Some(text) = app.clipboard_text.take() {
            frame.copy_to_clipboard(text);
        }
        if app.pending_paste {
            app.pending_paste = false;
            frame.paste_from_clipboard();
        }
        if app.pending_close {
            app.pending_close = false;
            app.save_prefs();
            frame.close();
        }
        if app.pending_minimize {
            app.pending_minimize = false;
            frame.minimize();
        }
        if app.pending_maximize {
            app.pending_maximize = false;
            frame.toggle_maximize();
        }
        if app.pending_move {
            app.pending_move = false;
            frame.begin_move();
        }

        if update.animating {
            frame.request_frame();
        } else if let Some(delay) = update.delay {
            frame.request_frame_after(delay);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procs::SystemInfo;
    use libluna_i18n::Locale;

    fn process(pid: u32, name: &str, uid: u32, cpu: f32, rss: u64) -> Process {
        Process {
            pid,
            ppid: 1,
            name: name.to_owned(),
            cmdline: format!("/usr/bin/{name} --flag"),
            state: 'S',
            uid,
            user: if uid == 1000 {
                "patryk".into()
            } else {
                "root".into()
            },
            cpu_jiffies: 0,
            start_jiffies: 100,
            rss_kib: rss,
            threads: 1,
            cpu_percent: cpu,
        }
    }

    fn app_with(processes: Vec<Process>) -> App {
        let snapshot = Snapshot {
            processes,
            system: SystemInfo::default(),
            ..Snapshot::default()
        };
        App::with_snapshot(snapshot, Prefs::default())
    }

    #[test]
    fn sorts_by_cpu_descending_by_default() {
        let app = app_with(vec![
            process(1, "init", 0, 0.0, 100),
            process(20, "firefox", 1000, 25.0, 500),
            process(30, "bash", 1000, 5.0, 50),
        ]);
        let order: Vec<u32> = app
            .filtered
            .iter()
            .map(|index| app.snapshot.processes[*index].pid)
            .collect();
        assert_eq!(order, vec![20, 30, 1]);
    }

    #[test]
    fn clicking_the_same_column_reverses_the_order() {
        let mut app = app_with(vec![
            process(1, "init", 0, 0.0, 100),
            process(20, "firefox", 1000, 25.0, 500),
        ]);
        assert_eq!(app.sort, SortKey::Cpu);
        app.set_sort(SortKey::Cpu);
        assert!(app.sort_asc);
        let first = app.snapshot.processes[app.filtered[0]].pid;
        assert_eq!(first, 1);
    }

    #[test]
    fn filters_by_name_pid_and_user() {
        let mut app = app_with(vec![
            process(1, "init", 0, 0.0, 100),
            process(20, "firefox", 1000, 25.0, 500),
        ]);

        app.search.set_text("fire");
        app.recompute();
        assert_eq!(app.filtered.len(), 1);

        app.search.set_text("patryk");
        app.recompute();
        assert_eq!(app.filtered.len(), 1);

        app.search.set_text("20");
        app.recompute();
        assert_eq!(app.filtered.len(), 1);

        app.search.set_text("nie ma");
        app.recompute();
        assert!(app.filtered.is_empty());
        assert_eq!(app.selected, None);
    }

    #[test]
    fn only_mine_hides_other_users() {
        let mut app = app_with(vec![
            process(1, "init", 0, 0.0, 100),
            process(20, "firefox", 1000, 25.0, 500),
        ]);
        app.uid = 1000;
        app.set_only_mine(true);
        assert_eq!(app.filtered.len(), 1);
        assert_eq!(app.snapshot.processes[app.filtered[0]].pid, 20);
    }

    #[test]
    fn selection_is_preserved_by_pid_across_refresh() {
        let mut app = app_with(vec![
            process(1, "init", 0, 0.0, 100),
            process(20, "firefox", 1000, 25.0, 500),
        ]);
        app.selected = Some(20);
        app.refresh();
        assert_eq!(app.selected, Some(20));
        assert_eq!(app.selected_process().map(|p| p.pid), Some(20));
    }

    #[test]
    fn navigation_clamps_to_visible_rows() {
        let mut app = app_with(vec![
            process(1, "init", 0, 0.0, 100),
            process(20, "firefox", 1000, 25.0, 500),
            process(30, "bash", 1000, 5.0, 50),
        ]);
        app.move_selection(1);
        assert_eq!(app.selected, Some(20));
        app.move_selection(10);
        assert_eq!(app.selected, Some(1));
        app.move_selection(-10);
        assert_eq!(app.selected, Some(20));
        app.select_last();
        assert_eq!(app.selected, Some(1));
        app.select_first();
        assert_eq!(app.selected, Some(20));
    }

    #[test]
    fn ask_end_requires_a_selection() {
        let mut app = app_with(vec![process(20, "firefox", 1000, 25.0, 500)]);
        app.ask_end(false);
        assert!(app.confirm.is_none());
        assert!(app.toast.is_open());
    }

    #[test]
    fn ask_end_builds_a_confirmation_and_blocks_pid_one() {
        let mut app = app_with(vec![
            process(1, "init", 0, 0.0, 100),
            process(20, "firefox", 1000, 25.0, 500),
        ]);
        app.uid = 1000;
        app.selected = Some(1);
        app.ask_end(false);
        assert!(app.confirm.is_none());

        app.selected = Some(20);
        app.ask_end(true);
        let confirm = app.confirm.clone().expect("confirm");
        assert_eq!(confirm.pid, 20);
        assert!(confirm.force);
    }

    #[test]
    fn toggle_search_clears_the_filter_when_hidden() {
        let mut app = app_with(vec![
            process(1, "init", 0, 0.0, 100),
            process(20, "firefox", 1000, 25.0, 500),
        ]);
        app.toggle_search();
        assert!(app.search_active);
        assert!(app.search.is_focused());

        app.search.set_text("fire");
        app.recompute();
        assert_eq!(app.filtered.len(), 1);

        app.toggle_search();
        assert!(!app.search_active);
        assert!(!app.search.is_focused());
        assert!(app.search.is_empty());
        assert_eq!(app.filtered.len(), 2);
    }

    #[test]
    fn context_menu_lists_actions_and_selects_the_process() {
        let mut app = app_with(vec![process(20, "firefox", 1000, 25.0, 500)]);
        app.uid = 1000;
        app.open_context(20, Rect::from_xywh(10.0, 10.0, 1.0, 1.0));
        assert_eq!(app.selected, Some(20));
        let state = app.context.as_ref().expect("context");
        let actions: Vec<ContextAction> = state.actions.iter().flatten().copied().collect();
        assert_eq!(
            actions,
            vec![
                ContextAction::End,
                ContextAction::ForceKill,
                ContextAction::CopyPid,
                ContextAction::CopyCommand,
                ContextAction::Details,
                ContextAction::Refresh,
                ContextAction::ToggleScope,
            ]
        );
    }

    #[test]
    fn details_action_opens_the_thread_breakdown() {
        let mut app = app_with(vec![process(20, "firefox", 1000, 25.0, 500)]);
        app.uid = 1000;
        app.open_context(20, Rect::from_xywh(10.0, 10.0, 1.0, 1.0));
        let index = app
            .context
            .as_ref()
            .expect("context")
            .actions
            .iter()
            .position(|action| *action == Some(ContextAction::Details))
            .expect("pozycja szczegółów");
        app.run_context(index);
        let details = app.details.as_ref().expect("szczegóły procesu");
        assert_eq!(details.pid, 20);
        assert_eq!(details.name, "firefox");
        assert_eq!(details.user, "patryk");
        assert!(
            app.context.is_none(),
            "menu zamyka się po otwarciu szczegółów"
        );
    }

    #[test]
    fn context_copy_pid_puts_the_pid_on_the_clipboard() {
        let mut app = app_with(vec![process(20, "firefox", 1000, 25.0, 500)]);
        app.uid = 1000;
        app.open_context(20, Rect::from_xywh(10.0, 10.0, 1.0, 1.0));
        let index = app
            .context
            .as_ref()
            .expect("context")
            .actions
            .iter()
            .position(|action| *action == Some(ContextAction::CopyPid))
            .expect("copy pid");
        app.run_context(index);
        assert_eq!(app.clipboard_text.as_deref(), Some("20"));
        assert!(app.context.is_none());
        assert!(app.toast.is_open());
    }

    #[test]
    fn menu_lists_refresh_scope_about_and_quit() {
        let mut app = app_with(vec![]);
        app.open_menu(Rect::from_xywh(0.0, 0.0, 10.0, 10.0));
        let state = app.menu.as_ref().expect("menu");
        assert_eq!(state.actions.len(), state.menu.entries().len());
        assert_eq!(
            state.actions.iter().flatten().copied().collect::<Vec<_>>(),
            vec![
                MenuAction::Refresh,
                MenuAction::ToggleCharts,
                MenuAction::ToggleScope,
                MenuAction::About,
                MenuAction::Quit
            ]
        );
    }

    #[test]
    fn charts_history_follows_the_selected_process() {
        let mut app = app_with(vec![
            process(20, "firefox", 1000, 25.0, 500),
            process(30, "bash", 1000, 5.0, 50),
        ]);
        app.selected = Some(20);
        app.refresh();
        assert_eq!(app.charts.selected_cpu.samples.len(), 1);
        assert_eq!(app.charts.pid, Some(20));

        // Zmiana zaznaczenia czyści historię procesu, ale nie systemu.
        app.selected = Some(30);
        app.refresh();
        assert_eq!(app.charts.pid, Some(30));
        assert_eq!(app.charts.selected_cpu.samples.len(), 1);
        assert_eq!(app.charts.system_cpu.samples.len(), 2);
    }

    #[test]
    fn toggle_charts_flips_the_option() {
        let mut app = app_with(vec![]);
        assert!(!app.show_charts);
        app.toggle_charts();
        assert!(app.show_charts);
        app.toggle_charts();
        assert!(!app.show_charts);
    }

    #[test]
    fn polish_builtin_translations_load() {
        set_locale(Locale::new("pl"));
        assert_eq!(tr("User"), "Użytkownik");
        assert_eq!(tr("Command line"), "Wiersz poleceń");
        assert_eq!(tr("Process"), "Proces");
    }

    #[test]
    fn sort_key_names_roundtrip() {
        for key in SortKey::ALL {
            assert_eq!(SortKey::from_name(key.name()), Some(key));
            assert_eq!(SortKey::from_name(key.title_key()), Some(key));
        }
        assert_eq!(SortKey::from_name("nonsense"), None);
    }
}
