//! Stan menedżera schowka i pętla klatki.
//!
//! Aplikacja jest immediate-mode: lista jest rysowana co klatkę, a kliknięcia
//! trafiają w prostokąty zebrane w [`App::hits`]. Historia żyje w
//! `~/.config/luna/clipboard.conf` (prawa `0600`) i nie opuszcza komputera.

use std::error::Error;
use std::time::{Duration, Instant};

use libluna_core::{Event, Point, Rect, ThemeSettings, WindowEvent};
use libluna_i18n::{load_catalogs_from_dirs, preferred_locale, set_locale, tr};
use libluna_platform::{run as run_app, AppConfig, Frame};
use libluna_widgets::{SearchEntry, WindowControl};

use crate::history::History;

/// Jak długo widać potwierdzenie akcji w stopce.
pub const TOAST_FOR: Duration = Duration::from_secs(2);

/// Odstęp między kliknięciami, które liczą się jako dwuklik.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// Stan aplikacji.
pub struct App {
    pub history: History,
    /// Indeksy historii widoczne po filtrze (kolejność jak w historii).
    pub visible: Vec<usize>,
    /// Pozycja w [`App::visible`].
    pub selected: usize,
    /// Przewinięcie listy w pikselach.
    pub scroll: f64,
    /// Wysokość widocznej części listy (ustawiana przez układ).
    pub viewport: f64,
    pub hover: Option<usize>,
    pub search: SearchEntry,
    pub search_active: bool,
    pub about: bool,
    pub toast: Option<(String, Instant)>,
    /// Poprzedni klik (indeks widoczny) — do rozpoznania dwukliku.
    last_click: Option<(usize, Instant)>,
    pub pointer: Option<Point>,
    pub controls_hover: Option<WindowControl>,
    /// Prostokąty klikalne zebrane podczas rysowania: (akcja, obszar).
    pub hits: Vec<(Hit, Rect)>,
    /// Trzeba poprosić o odczyt schowka (zmiana selekcji).
    pub pending_read: bool,
    /// Tekst do wstawienia do schowka po zakończeniu klatki.
    pub clipboard_text: Option<String>,
    /// Historia czeka na zapis.
    pub dirty: bool,
    pub pending_close: bool,
    pub pending_minimize: bool,
    pub pending_maximize: bool,
    pub pending_move: bool,
}

/// Co robi kliknięcie w wiersz listy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// Przypina/odpina wpis.
    Pin(usize),
    /// Usuwa wpis.
    Remove(usize),
    /// Zamyka okno „O aplikacji”.
    CloseAbout,
}

impl App {
    pub fn launch() -> Self {
        let mut app = Self {
            history: History::load(),
            visible: Vec::new(),
            selected: 0,
            scroll: 0.0,
            viewport: 0.0,
            hover: None,
            search: SearchEntry::new(tr("Search clipboard")),
            search_active: false,
            about: false,
            toast: None,
            last_click: None,
            pointer: None,
            controls_hover: None,
            hits: Vec::new(),
            pending_read: false,
            clipboard_text: None,
            dirty: false,
            pending_close: false,
            pending_minimize: false,
            pending_maximize: false,
            pending_move: false,
        };
        app.recompute();
        app
    }

    /// Przelicza filtr i pilnuje, żeby zaznaczenie było w jego granicach.
    pub fn recompute(&mut self) {
        self.visible = self.history.matches(self.search.text());
        if self.visible.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.visible.len() {
            self.selected = self.visible.len() - 1;
        }
    }

    /// Indeks wpisu w historii dla bieżącego zaznaczenia.
    pub fn selected_entry(&self) -> Option<usize> {
        self.visible.get(self.selected).copied()
    }

    pub fn count(&self) -> usize {
        self.visible.len()
    }

    /// Przenosi zaznaczenie o `delta`, pilnując granic listy.
    pub fn move_selection(&mut self, delta: i32) {
        if self.visible.is_empty() {
            return;
        }
        let last = self.visible.len() - 1;
        let next = (self.selected as i32 + delta).clamp(0, last as i32);
        self.selected = next as usize;
        self.ensure_visible();
    }

    pub fn select_first(&mut self) {
        self.selected = 0;
        self.ensure_visible();
    }

    pub fn select_last(&mut self) {
        if !self.visible.is_empty() {
            self.selected = self.visible.len() - 1;
            self.ensure_visible();
        }
    }

    /// Przewija listę tak, żeby zaznaczony wiersz był widoczny.
    pub fn ensure_visible(&mut self) {
        let row = crate::ui::ROW_HEIGHT;
        let top = self.selected as f64 * row;
        let bottom = top + row;
        if top < self.scroll {
            self.scroll = top;
        } else if bottom > self.scroll + self.viewport {
            self.scroll = bottom - self.viewport;
        }
        self.clamp_scroll();
    }

    pub fn clamp_scroll(&mut self) {
        let content = self.visible.len() as f64 * crate::ui::ROW_HEIGHT;
        let max = (content - self.viewport).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max);
    }

    /// Kopiuje zaznaczony wpis z powrotem do schowka systemowego.
    pub fn copy_selected(&mut self) {
        let Some(index) = self.selected_entry() else {
            return;
        };
        let Some(entry) = self.history.entries.get(index) else {
            return;
        };
        self.clipboard_text = Some(entry.text.clone());
        self.notify(tr("Copied to clipboard"));
    }

    /// Dopisuje tekst ze schowka (pomija duplikat ostatniego wpisu).
    pub fn record(&mut self, text: &str) {
        if text.len() > crate::history::MAX_ENTRY_BYTES {
            self.notify(tr("Entry too large to keep in history"));
            return;
        }
        if self.history.push(text) {
            self.dirty = true;
            self.recompute();
        }
    }

    pub fn toggle_pin(&mut self) {
        let Some(index) = self.selected_entry() else {
            return;
        };
        self.history.toggle_pin(index);
        self.dirty = true;
        self.notify(tr("Pin toggled"));
    }

    pub fn remove_selected(&mut self) {
        let Some(index) = self.selected_entry() else {
            return;
        };
        self.history.remove(index);
        self.dirty = true;
        self.recompute();
        self.notify(tr("Entry removed"));
    }

    pub fn notify(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    /// Zapisuje historię, jeśli coś się zmieniło.
    pub fn save_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        if let Err(error) = self.history.save() {
            eprintln!("luna-clipboard: nie można zapisać historii: {error}");
        }
    }

    /// Opis wieku wpisu („teraz”, „5 min”, „2 godz”…).
    pub fn age_label(&self, entry: &crate::history::Entry) -> Option<String> {
        if entry.at == 0 {
            return None;
        }
        Some(crate::ui::format_age(
            History::now().saturating_sub(entry.at),
        ))
    }

    /// Oznacza kliknięcie; zwraca `true`, gdy to dwuklik na tym samym wierszu.
    pub fn register_click(&mut self, visible_index: usize) -> bool {
        let now = Instant::now();
        let double = self.last_click.is_some_and(|(index, at)| {
            index == visible_index && now.duration_since(at) < DOUBLE_CLICK
        });
        self.last_click = if double {
            None
        } else {
            Some((visible_index, now))
        };
        double
    }
}

/// Pętla aplikacji i podpięcie do schowka systemowego.
pub fn run() -> Result<(), Box<dyn Error>> {
    set_locale(preferred_locale());
    load_catalogs_from_dirs();

    let mut app = App::launch();
    let settings = ThemeSettings::load();
    let system_dark = libluna_core::appearance::system_dark();
    let config = AppConfig::new(tr("Clipboard"))
        .app_id("eu.luna.Clipboard")
        .initial_size(460, 560)
        .settings(settings)
        .system_dark(system_dark)
        .escape_to_exit(false)
        // Menedżer musi wiedzieć, że ktoś coś skopiował.
        .watch_clipboard(true);

    let mut last_title = String::new();
    run_app(config, move |frame: &mut Frame| {
        let theme = frame.settings().resolve_themed(frame.system_dark());
        crate::ui::handle_events(&mut app, frame.events, frame.size, &theme, frame.text);
        app.save_if_dirty();

        let title = tr("Clipboard");
        if title != last_title {
            frame.set_title(title.clone());
            last_title = title;
        }

        crate::ui::paint(&mut app, frame.scene, frame.size, &theme, frame.text);

        if app.pending_read {
            app.pending_read = false;
            frame.paste_from_clipboard();
        }
        if let Some(text) = app.clipboard_text.take() {
            frame.copy_to_clipboard(text);
        }
        if app.pending_close {
            app.pending_close = false;
            app.save_if_dirty();
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

        // Toast znika sam — bez tego stopka zostawałaby z komunikatem na zawsze.
        if let Some((_, since)) = &app.toast {
            if since.elapsed() >= TOAST_FOR {
                app.toast = None;
                frame.request_frame();
            } else {
                frame.request_frame_after(TOAST_FOR - since.elapsed());
            }
        }
    })
}

/// Obsługa zdarzeń, która nie dotyczy układu (czysta logika — do testów).
pub fn handle_event(app: &mut App, event: &Event) -> bool {
    match event {
        // Zmiana schowka: odczyt idzie asynchronicznie, wynik wróci jako `Paste`.
        Event::ClipboardChanged => {
            app.pending_read = true;
            true
        }
        Event::Paste(text) => {
            app.record(text);
            true
        }
        Event::Window(WindowEvent::Focused(false)) => {
            app.search.set_focused(false);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with(entries: &[(&str, bool)]) -> App {
        let mut app = App::launch();
        app.history = History::default();
        for (index, (text, pinned)) in entries.iter().enumerate() {
            app.history.push_at(text, index as u64 + 1);
            if *pinned {
                let last = app.history.entries.len() - 1;
                app.history.entries[last].pinned = true;
            }
        }
        app.recompute();
        app
    }

    #[test]
    fn clipboard_change_requests_a_read_instead_of_reading_inline() {
        let mut app = App::launch();
        assert!(handle_event(&mut app, &Event::ClipboardChanged));
        assert!(app.pending_read);
    }

    #[test]
    fn pasted_text_lands_in_history_newest_first() {
        let mut app = app_with(&[]);
        handle_event(&mut app, &Event::Paste("pierwszy".into()));
        handle_event(&mut app, &Event::Paste("drugi".into()));
        assert_eq!(app.history.entries[0].text, "drugi");
        assert_eq!(app.count(), 2);
        assert!(app.dirty);
    }

    #[test]
    fn duplicate_paste_does_not_grow_the_history() {
        let mut app = app_with(&[]);
        handle_event(&mut app, &Event::Paste("ten sam".into()));
        handle_event(&mut app, &Event::Paste("ten sam".into()));
        assert_eq!(app.count(), 1);
    }

    #[test]
    fn selection_stays_inside_the_filtered_list() {
        let mut app = app_with(&[("alfa", false), ("beta", false), ("gamma", false)]);
        app.search.set_text("a");
        app.recompute();
        app.select_last();
        assert_eq!(app.selected, app.visible.len() - 1);
        app.move_selection(5);
        assert_eq!(app.selected, app.visible.len() - 1);
        app.move_selection(-99);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn removing_the_last_visible_entry_keeps_selection_valid() {
        let mut app = app_with(&[("jeden", false)]);
        app.remove_selected();
        assert!(app.visible.is_empty());
        assert_eq!(app.selected, 0);
        assert_eq!(app.selected_entry(), None);
    }

    #[test]
    fn pinning_survives_filtering_out_and_back() {
        let mut app = app_with(&[("notatka", false)]);
        app.toggle_pin();
        assert!(app.history.entries[0].pinned);
        app.search.set_text("nic");
        app.recompute();
        app.search.clear();
        app.recompute();
        assert!(app.history.entries[0].pinned);
    }
}
