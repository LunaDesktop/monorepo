//! Stan przeglądarki i pętla klatki.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use libluna_core::{Point, Rect, Size, Theme, ThemePreference, ThemeSettings};
use libluna_i18n::{load_catalogs_from_dirs, set_locale, tr};
use libluna_platform::{run as run_app, AppConfig, Frame};
use libluna_render::{ImageData, TextEngine};
use libluna_widgets::{AlertDialog, MediaControls, Menu, MenuEntry, Toast, WindowControl};

use crate::decode;
use crate::model::{count_kinds, format_duration, human_size, MediaItem, MediaKind};
use crate::places::Place;
use crate::prefs::Prefs;
use crate::thumb::Thumbnails;
use crate::video::{VideoPlayer, VideoSpec};

/// Widok aplikacji.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Gallery,
    Viewer,
}

/// Co wybrać z menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCmd {
    Open,
    BackToGallery,
    StopSlideshow,
    OpenExternal,
    OpenFolder,
    CopyPath,
    CopyPdfText,
    DevelopRaw,
    Details,
    About,
}

/// Otwarte menu (panel liczy UI, bo zna motyw i silnik tekstu).
pub struct MenuState {
    pub menu: Menu,
    pub panel: Rect,
    pub cmds: Vec<Option<MenuCmd>>,
}

pub struct LoadMsg {
    generation: u64,
    image: Option<Arc<ImageData>>,
    text: Option<Arc<decode::PdfText>>,
}

/// Stan dokumentu PDF w podglądzie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfState {
    /// Bieżąca strona (indeks od 0).
    pub page: usize,
    pub pages: usize,
}

/// Stan podglądu jednego pliku.
pub struct Viewer {
    pub index: usize,
    pub generation: u64,
    pub image: Option<Arc<ImageData>>,
    pub loading: bool,
    pub load_rx: Option<Receiver<LoadMsg>>,
    /// Klucz i18n błędu wczytywania bieżącego rodzaju pliku.
    pub load_error_key: &'static str,
    pub error: Option<String>,
    pub pdf: Option<PdfState>,
    /// Warstwa tekstowa bieżącej strony PDF.
    pub pdf_text: Option<Arc<decode::PdfText>>,
    /// Zaznaczenie tekstu PDF jako zakres indeksów słów (`anchor`, `focus`).
    pub text_sel: Option<(usize, usize)>,
    pub text_selecting: bool,
    /// Powiększenie względem „dopasowania do okna” (1.0 = fit).
    pub zoom: f64,
    pub pan: Point,
    /// Przeciąganie obrazu: pozycja startu i przesunięcie bazowe.
    pub drag: Option<(Point, Point)>,
    pub slideshow: bool,
    pub next_slide: Option<Instant>,
    pub video: Option<VideoPlayer>,
    pub video_error: Option<String>,
    pub controls: MediaControls,
}

impl Default for Viewer {
    fn default() -> Self {
        Self {
            index: 0,
            generation: 0,
            image: None,
            loading: false,
            load_rx: None,
            load_error_key: "Cannot load this image",
            error: None,
            pdf: None,
            pdf_text: None,
            text_sel: None,
            text_selecting: false,
            zoom: 1.0,
            pan: Point::ZERO,
            drag: None,
            slideshow: false,
            next_slide: None,
            video: None,
            video_error: None,
            controls: MediaControls::default(),
        }
    }
}

impl Viewer {
    fn open(&mut self, index: usize, item: &MediaItem, muted: bool) {
        self.index = index;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.image = None;
        self.loading = false;
        self.load_rx = None;
        self.load_error_key = "Cannot load this image";
        self.pdf = None;
        self.pdf_text = None;
        self.text_sel = None;
        self.text_selecting = false;
        self.error = None;
        self.video = None;
        self.video_error = None;
        self.controls = MediaControls::default();
        self.zoom = 1.0;
        self.pan = Point::ZERO;
        self.drag = None;

        match item.kind {
            MediaKind::Image => {
                self.loading = true;
                if crate::model::is_raw_path(&item.path) {
                    self.load_error_key = "Cannot read this RAW";
                }
                let (tx, rx) = mpsc::channel();
                self.load_rx = Some(rx);
                let path = item.path.clone();
                thread::Builder::new()
                    .name("luna-media-image".into())
                    .spawn(move || {
                        let image = decode::load(&path, 4096);
                        let _ = tx.send(LoadMsg {
                            generation,
                            image,
                            text: None,
                        });
                    })
                    .ok();
            }
            MediaKind::Video => match VideoPlayer::open(&item.path, muted) {
                Ok(mut player) => {
                    player.play();
                    self.controls = MediaControls::new(item.name.clone(), player.spec().summary());
                    self.controls
                        .set_times("0:00", format_duration(player.duration()));
                    self.video = Some(player);
                }
                Err(error) => self.video_error = Some(error),
            },
            MediaKind::Pdf => {
                self.load_error_key = "Cannot read this PDF";
                if !decode::have_pdf() {
                    self.error = Some(tr("PDF support requires poppler"));
                } else {
                    match decode::pdf_page_count(&item.path) {
                        Some(pages) if pages > 0 => {
                            self.pdf = Some(PdfState { page: 0, pages });
                            self.spawn_pdf_page(&item.path, 0);
                        }
                        _ => self.error = Some(tr("Cannot read this PDF")),
                    }
                }
            }
        }
    }

    /// Renderuje stronę PDF w tle (generacja odsiewa stare wyniki).
    fn spawn_pdf_page(&mut self, path: &Path, page: usize) {
        self.loading = true;
        let generation = self.generation;
        let (tx, rx) = mpsc::channel();
        self.load_rx = Some(rx);
        let path = path.to_path_buf();
        thread::Builder::new()
            .name("luna-media-pdf".into())
            .spawn(move || {
                let image = decode::pdf_page(&path, page, 2200).map(Arc::new);
                let text = decode::pdf_text(&path, page).map(Arc::new);
                let _ = tx.send(LoadMsg {
                    generation,
                    image,
                    text,
                });
            })
            .ok();
    }

    /// Krok podglądu; `true`, gdy warto narysować kolejną klatkę.
    fn tick(&mut self) -> bool {
        let mut busy = false;
        if let Some(rx) = self.load_rx.take() {
            let mut keep = true;
            loop {
                match rx.try_recv() {
                    Ok(message) => {
                        if message.generation == self.generation {
                            let image = message.image;
                            self.error = image.is_none().then(|| tr(self.load_error_key));
                            self.image = image;
                            self.pdf_text = message.text;
                            self.loading = false;
                            keep = false;
                            busy = true;
                            break;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.loading = false;
                        self.error = Some(tr(self.load_error_key));
                        keep = false;
                        busy = true;
                        break;
                    }
                }
            }
            if keep {
                self.load_rx = Some(rx);
                busy = true;
            }
        }
        if let Some(video) = &mut self.video {
            busy |= video.tick();
            self.controls.set_playing(video.playing());
            let duration = video.duration();
            let position = video.position();
            self.controls.set_position(if duration > 0.0 {
                (position / duration).clamp(0.0, 1.0)
            } else {
                0.0
            });
            self.controls
                .set_times(format_duration(position), format_duration(duration));
        }
        busy
    }

    /// Czy bieżący plik to obraz (a nie film/błąd).
    pub fn is_image(&self) -> bool {
        self.video.is_none() && self.video_error.is_none()
    }
}

/// Jedno odświeżenie stanu (po obsłudze zdarzeń).
pub struct Update {
    /// Trwa animacja/odtwarzanie — potrzebna kolejna klatka natychmiast.
    pub animating: bool,
    /// Za ile odświeżyć stan (miniatury, pokaz slajdów).
    pub delay: Option<Duration>,
}

pub struct App {
    pub prefs: Prefs,
    pub dir: PathBuf,
    pub items: Vec<MediaItem>,
    /// Podkatalogi bieżącego katalogu (pasek folderów nad siatką).
    pub folders: Vec<(String, PathBuf)>,
    pub selected: usize,
    pub scroll: libluna_widgets::ScrollView,
    pub mode: Mode,
    pub viewer: Viewer,
    pub thumbs: Thumbnails,
    pub places: Vec<Place>,
    pub sidebar: bool,
    pub details: bool,
    pub menu: Option<MenuState>,
    pub about: Option<AlertDialog>,
    pub toast: Toast,
    pub toast_until: Option<Instant>,
    pub pointer: Option<Point>,
    pub controls_hover: Option<WindowControl>,
    pub pending_close: bool,
    pub pending_minimize: bool,
    pub pending_maximize: bool,
    pub pending_move: bool,
    pub text_clipboard: Option<String>,
    pub last_click: Option<(usize, Instant)>,
    pub last_canvas_click: Option<Instant>,
    pub theme_pref: ThemePreference,
    pub nav_error: Option<String>,
}

impl App {
    pub fn new(dir: PathBuf, open: Option<PathBuf>, prefs: Prefs) -> Self {
        let theme_pref = ThemeSettings::load().preference();
        let places = crate::places::list();
        let mut app = Self {
            prefs,
            dir: dir.clone(),
            items: Vec::new(),
            folders: Vec::new(),
            selected: 0,
            scroll: libluna_widgets::ScrollView::new(),
            mode: Mode::Gallery,
            viewer: Viewer::default(),
            thumbs: Thumbnails::new(),
            places,
            sidebar: true,
            details: false,
            menu: None,
            about: None,
            toast: Toast::new(""),
            toast_until: None,
            pointer: None,
            controls_hover: None,
            pending_close: false,
            pending_minimize: false,
            pending_maximize: false,
            pending_move: false,
            text_clipboard: None,
            last_click: None,
            last_canvas_click: None,
            theme_pref,
            nav_error: None,
        };
        app.load_dir(dir, None);
        if let Some(path) = open {
            match app.items.iter().position(|item| item.path == path) {
                Some(index) => app.open_viewer(index),
                None => app.show_toast(tr("Unsupported file")),
            }
        }
        app
    }

    /// Wczytuje katalog; `keep` to ścieżka, którą trzeba zaznaczyć (jeśli nadal istnieje).
    pub fn load_dir(&mut self, dir: PathBuf, keep: Option<PathBuf>) {
        match crate::model::scan(&dir) {
            Ok(items) => {
                self.items = items;
                self.folders = crate::model::scan_folders(&dir);
                self.nav_error = None;
                self.selected = keep
                    .as_ref()
                    .and_then(|path| self.items.iter().position(|item| &item.path == path))
                    .unwrap_or(0);
                self.dir = dir;
                self.scroll = libluna_widgets::ScrollView::new();
                self.last_click = None;
                self.prefs.last_dir = self.dir.display().to_string();
            }
            Err(error) => {
                self.nav_error = Some(format!("{}: {error}", dir.display()));
                self.items.clear();
                self.folders.clear();
                self.selected = 0;
                self.dir = dir;
            }
        }
    }

    /// Wraca do galerii (zatrzymuje film).
    pub fn back_to_gallery(&mut self) {
        self.viewer.video = None;
        self.viewer.load_rx = None;
        self.viewer.pdf = None;
        self.viewer.pdf_text = None;
        self.viewer.text_sel = None;
        self.viewer.text_selecting = false;
        self.viewer.loading = false;
        self.viewer.slideshow = false;
        self.viewer.next_slide = None;
        self.viewer.drag = None;
        self.last_canvas_click = None;
        self.mode = Mode::Gallery;
    }

    /// Otwiera katalog (zawsze w trybie galerii).
    pub fn navigate_to(&mut self, path: PathBuf) {
        if !path.is_dir() {
            return;
        }
        self.back_to_gallery();
        self.load_dir(path, None);
        self.prefs.save();
        self.menu = None;
    }

    pub fn up(&mut self) {
        if let Some(parent) = self.dir.parent().map(Path::to_path_buf) {
            let keep = Some(self.dir.clone());
            self.back_to_gallery();
            self.load_dir(parent, keep);
            self.prefs.save();
        }
    }

    pub fn reload(&mut self) {
        let keep = self.shown_item().map(|item| item.path.clone());
        self.load_dir(self.dir.clone(), keep.clone());
        if self.mode == Mode::Viewer {
            let index = keep.and_then(|path| self.items.iter().position(|item| item.path == path));
            match index {
                Some(index) => {
                    let slideshow = self.viewer.slideshow;
                    let item = self.items[index].clone();
                    self.viewer.open(index, &item, self.prefs.muted);
                    self.viewer.slideshow = slideshow;
                }
                None => self.back_to_gallery(),
            }
        }
    }

    pub fn selected_item(&self) -> Option<&MediaItem> {
        self.items.get(self.selected)
    }

    pub fn current_item(&self) -> Option<&MediaItem> {
        match self.mode {
            Mode::Viewer => self.items.get(self.viewer.index),
            Mode::Gallery => self.items.get(self.selected),
        }
    }

    /// Element, którego dotyczą akcje menu i panel szczegółów.
    pub fn shown_item(&self) -> Option<&MediaItem> {
        self.current_item()
    }

    pub fn select(&mut self, index: usize) {
        if index < self.items.len() {
            self.selected = index;
        }
    }

    /// Ruch zaznaczenia w siatce (kolumny liczy UI, bo zależą od szerokości okna).
    pub fn move_selection(&mut self, dx: i64, dy: i64, cols: usize) {
        if self.items.is_empty() {
            return;
        }
        let cols = cols.max(1) as i64;
        let next = self.selected as i64 + dx + dy * cols;
        self.selected = next.clamp(0, self.items.len() as i64 - 1) as usize;
    }

    pub fn open_viewer(&mut self, index: usize) {
        let Some(item) = self.items.get(index).cloned() else {
            return;
        };
        self.mode = Mode::Viewer;
        self.viewer.slideshow = false;
        self.viewer.next_slide = None;
        self.last_canvas_click = None;
        self.viewer.open(index, &item, self.prefs.muted);
    }

    /// Przejście do poprzedniego/następnego pliku w podglądzie (z zawijaniem).
    pub fn viewer_step(&mut self, delta: i64) {
        if self.items.is_empty() {
            return;
        }
        let len = self.items.len() as i64;
        let next = (self.viewer.index as i64 + delta).rem_euclid(len) as usize;
        let slideshow = self.viewer.slideshow;
        let item = self.items[next].clone();
        self.viewer.open(next, &item, self.prefs.muted);
        self.viewer.slideshow = slideshow;
        if slideshow {
            self.viewer.next_slide = Some(Instant::now() + self.slide_interval());
        }
        self.mode = Mode::Viewer;
    }

    /// Przejście o `delta` stron w PDF (ograniczone do zakresu).
    pub fn pdf_step(&mut self, delta: i64) {
        let Some(pdf) = self.viewer.pdf else {
            return;
        };
        let target = (pdf.page as i64 + delta).clamp(0, pdf.pages as i64 - 1) as usize;
        if target == pdf.page {
            return;
        }
        let Some(path) = self
            .items
            .get(self.viewer.index)
            .map(|item| item.path.clone())
        else {
            return;
        };
        if let Some(pdf) = self.viewer.pdf.as_mut() {
            pdf.page = target;
        }
        self.viewer.pdf_text = None;
        self.viewer.text_sel = None;
        self.viewer.text_selecting = false;
        self.viewer.generation = self.viewer.generation.wrapping_add(1);
        self.viewer.error = None;
        self.viewer.spawn_pdf_page(&path, target);
    }

    /// Skok do pierwszej/ostatniej strony PDF.
    pub fn pdf_edge(&mut self, last: bool) {
        let Some(pdf) = self.viewer.pdf else {
            return;
        };
        let target = if last { pdf.pages.saturating_sub(1) } else { 0 };
        let delta = target as i64 - pdf.page as i64;
        self.pdf_step(delta);
    }

    /// Rozpoczyna/aktualizuje zaznaczanie tekstu PDF (indeksy słów).
    pub fn pdf_begin_selection(&mut self, word: usize) {
        self.viewer.text_sel = Some((word, word));
        self.viewer.text_selecting = true;
    }

    pub fn pdf_update_selection(&mut self, word: usize) {
        if let Some((anchor, _)) = self.viewer.text_sel {
            self.viewer.text_sel = Some((anchor, word));
        }
    }

    pub fn pdf_end_selection(&mut self) {
        self.viewer.text_selecting = false;
    }

    fn pdf_range(&self) -> Option<(usize, usize)> {
        let text = self.viewer.pdf_text.as_ref()?;
        let (a, b) = self.viewer.text_sel?;
        let (lo, hi) = (a.min(b), a.max(b));
        if text.words.is_empty() {
            return None;
        }
        Some((lo.min(text.words.len() - 1), hi.min(text.words.len() - 1)))
    }

    /// Tekst zaznaczonych słów (albo całej strony, gdy nic nie zaznaczono).
    pub fn pdf_text_string(&self, selected_only: bool) -> Option<String> {
        let text = self.viewer.pdf_text.as_ref()?;
        if text.words.is_empty() {
            return None;
        }
        let range = if selected_only {
            self.pdf_range()?
        } else {
            (0, text.words.len() - 1)
        };
        let mut out = String::new();
        let mut last_y: Option<f32> = None;
        for word in &text.words[range.0..=range.1] {
            if let Some(y) = last_y {
                if (word.y - y).abs() > word.height.max(2.0) * 0.6 {
                    out.push('\n');
                } else {
                    out.push(' ');
                }
            }
            out.push_str(&word.text);
            last_y = Some(word.y);
        }
        (!out.is_empty()).then_some(out)
    }

    /// Kopiuje zaznaczenie tekstu PDF (albo całą bieżącą stronę).
    pub fn copy_pdf_text(&mut self, selected_only: bool) {
        let Some(text) = self.pdf_text_string(selected_only) else {
            return;
        };
        self.text_clipboard = Some(text);
        self.show_toast(tr("Text copied"));
    }

    fn slide_interval(&self) -> Duration {
        Duration::from_secs(self.prefs.slideshow_secs.clamp(1, 120) as u64)
    }

    /// Włącza/wyłącza pokaz slajdów (tylko dla obrazów).
    pub fn toggle_slideshow(&mut self) {
        let Some(item) = self.current_item() else {
            return;
        };
        if item.kind != MediaKind::Image {
            self.show_toast(tr("Slideshow is available for images"));
            return;
        }
        self.viewer.slideshow = !self.viewer.slideshow;
        self.viewer.next_slide = self
            .viewer
            .slideshow
            .then(|| Instant::now() + self.slide_interval());
    }

    fn slideshow_advance(&mut self) {
        let len = self.items.len();
        if len == 0 {
            return;
        }
        let mut index = self.viewer.index;
        for _ in 0..len {
            index = (index + 1) % len;
            if self.items[index].kind == MediaKind::Image {
                let item = self.items[index].clone();
                self.viewer.open(index, &item, self.prefs.muted);
                self.viewer.slideshow = true;
                self.viewer.next_slide = Some(Instant::now() + self.slide_interval());
                return;
            }
        }
        // Same filmy — pokaz nie ma czego pokazywać.
        self.viewer.slideshow = false;
        self.viewer.next_slide = None;
    }

    pub fn zoom_in(&mut self) {
        self.viewer.zoom = (self.viewer.zoom * 1.25).min(32.0);
    }

    pub fn zoom_out(&mut self) {
        self.viewer.zoom = (self.viewer.zoom / 1.25).max(0.1);
        if self.viewer.zoom <= 1.0 {
            self.viewer.zoom = 1.0;
            self.viewer.pan = Point::ZERO;
        }
    }

    pub fn zoom_fit(&mut self) {
        self.viewer.zoom = 1.0;
        self.viewer.pan = Point::ZERO;
        self.viewer.drag = None;
    }

    pub fn zoom_actual(&mut self, canvas: Rect) {
        let Some(image) = &self.viewer.image else {
            return;
        };
        let (iw, ih) = (image.width as f64, image.height as f64);
        if iw <= 0.0 || ih <= 0.0 || canvas.width() <= 0.0 || canvas.height() <= 0.0 {
            return;
        }
        let base = (canvas.width() / iw).min(canvas.height() / ih);
        if base > 0.0 {
            self.viewer.zoom = (1.0 / base).clamp(0.1, 32.0);
            self.viewer.pan = Point::ZERO;
        }
    }

    pub fn toggle_mute(&mut self) {
        self.prefs.muted = !self.prefs.muted;
        self.prefs.save();
        if let Some(video) = &mut self.viewer.video {
            video.set_muted(self.prefs.muted);
        }
    }

    pub fn toggle_details(&mut self) {
        self.details = !self.details;
    }

    /// Tekst paska stanu galerii.
    pub fn status_text(&self) -> String {
        let (images, videos, documents) = count_kinds(&self.items);
        let size: u64 = self.items.iter().map(|item| item.size).sum();
        format!(
            "{}: {} · {}: {} · {}: {} · {}",
            tr("Images"),
            images,
            tr("Videos"),
            videos,
            tr("Documents"),
            documents,
            human_size(size)
        )
    }

    pub fn image_dimensions(&self) -> Option<(u32, u32)> {
        self.viewer
            .image
            .as_ref()
            .map(|image| (image.width, image.height))
    }

    pub fn video_spec(&self) -> Option<&VideoSpec> {
        self.viewer.video.as_ref().map(|video| video.spec())
    }

    pub fn show_toast(&mut self, message: impl Into<String>) {
        self.toast.set_message(message);
        self.toast.set_action(None);
        self.toast.show_animated();
        self.toast_until = Some(Instant::now() + Duration::from_secs(4));
    }

    pub fn show_about(&mut self) {
        let body = format!(
            "Luna Media {}\n{}\n\n{}\n\n© 2026 Luna Project · GPL-3.0-or-later",
            env!("CARGO_PKG_VERSION"),
            tr("Media viewer for the Luna desktop."),
            tr("Images (including RAW), videos and PDFs. PNG, JPEG and SVG are decoded natively; ffmpeg, poppler and libraw handle the rest."),
        );
        self.about = Some(
            AlertDialog::new(tr("About Media"), body)
                .width(500.0)
                .button(tr("Done")),
        );
    }

    pub fn copy_path(&mut self) {
        let path = self
            .shown_item()
            .map(|item| item.path.clone())
            .unwrap_or_else(|| self.dir.clone());
        self.text_clipboard = Some(path.display().to_string());
        self.show_toast(tr("Path copied"));
    }

    pub fn open_external(&mut self) {
        let path = self
            .current_item()
            .map(|item| item.path.clone())
            .unwrap_or_else(|| self.dir.clone());
        if Command::new("xdg-open").arg(&path).spawn().is_err() {
            self.show_toast(tr("No application to open this file"));
        }
    }

    /// Wymusza pełną demozajkę bieżącego RAW-a (menu „Rozwiń RAW”).
    pub fn develop_current_raw(&mut self) {
        let index = match self.mode {
            Mode::Viewer => self.viewer.index,
            Mode::Gallery => self.selected,
        };
        let Some(item) = self.items.get(index).cloned() else {
            return;
        };
        if !crate::model::is_raw_path(&item.path) || !decode::can_develop_raw() {
            return;
        }
        if self.mode != Mode::Viewer {
            self.open_viewer(index);
        }
        self.viewer.generation = self.viewer.generation.wrapping_add(1);
        let generation = self.viewer.generation;
        self.viewer.load_error_key = "Cannot develop this RAW";
        self.viewer.error = None;
        self.viewer.loading = true;
        let (tx, rx) = mpsc::channel();
        self.viewer.load_rx = Some(rx);
        let path = item.path.clone();
        self.show_toast(tr("Developing RAW…"));
        thread::Builder::new()
            .name("luna-media-raw".into())
            .spawn(move || {
                let image = decode::develop_raw(&path, 4096, false).map(Arc::new);
                let _ = tx.send(LoadMsg {
                    generation,
                    image,
                    text: None,
                });
            })
            .ok();
    }

    pub fn open_in_files(&mut self) {
        let dir = self.dir.clone();
        if Command::new("luna-files").arg(&dir).spawn().is_err()
            && Command::new("xdg-open").arg(&dir).spawn().is_err()
        {
            self.show_toast(tr("No file manager found"));
        }
    }

    /// Buduje menu pod `anchor`.
    pub fn open_menu(&mut self, anchor: Rect, size: Size, theme: &Theme, text: &TextEngine) {
        let mut entries = Vec::new();
        let mut cmds = Vec::new();
        let has_item = self.current_item().is_some();
        if self.mode == Mode::Viewer {
            entries.push(MenuEntry::action(tr("Back to gallery")).shortcut("Esc"));
            cmds.push(Some(MenuCmd::BackToGallery));
        } else {
            let mut open = MenuEntry::action(tr("Open")).shortcut("Enter");
            if !has_item {
                open = open.disabled();
            }
            entries.push(open);
            cmds.push(Some(MenuCmd::Open));
        }
        entries.push(MenuEntry::separator());
        cmds.push(None);
        let mut external = MenuEntry::action(tr("Open with default app"));
        if !has_item {
            external = external.disabled();
        }
        entries.push(external);
        cmds.push(Some(MenuCmd::OpenExternal));
        entries.push(MenuEntry::action(tr("Open containing folder")));
        cmds.push(Some(MenuCmd::OpenFolder));
        entries.push(MenuEntry::action(tr("Copy path")));
        cmds.push(Some(MenuCmd::CopyPath));
        if self.viewer.pdf_text.is_some() {
            let selected = self.viewer.text_sel.is_some();
            entries.push(MenuEntry::action(if selected {
                tr("Copy selected text")
            } else {
                tr("Copy page text")
            }));
            cmds.push(Some(MenuCmd::CopyPdfText));
        }
        let raw = self
            .current_item()
            .is_some_and(|item| crate::model::is_raw_path(&item.path));
        if raw && decode::can_develop_raw() {
            entries.push(MenuEntry::action(tr("Develop RAW in full quality")));
            cmds.push(Some(MenuCmd::DevelopRaw));
        }
        entries.push(MenuEntry::separator());
        cmds.push(None);
        entries.push(
            MenuEntry::action(if self.details {
                tr("Hide details")
            } else {
                tr("Details")
            })
            .shortcut("I"),
        );
        cmds.push(Some(MenuCmd::Details));
        if self.viewer.slideshow {
            entries.push(MenuEntry::action(tr("Stop slideshow")).shortcut("Space"));
            cmds.push(Some(MenuCmd::StopSlideshow));
        }
        entries.push(MenuEntry::separator());
        cmds.push(None);
        entries.push(MenuEntry::action(tr("About Media")));
        cmds.push(Some(MenuCmd::About));

        let menu = Menu::new(entries);
        let width = menu.width(text, theme);
        let panel = menu.place(anchor, width, Rect::from_size(size), theme);
        self.menu = Some(MenuState { menu, panel, cmds });
    }

    pub fn menu_command(&self, index: usize) -> Option<MenuCmd> {
        self.menu
            .as_ref()
            .and_then(|state| state.cmds.get(index).copied().flatten())
    }

    pub fn run_menu(&mut self, index: usize) {
        let Some(cmd) = self.menu_command(index) else {
            return;
        };
        self.menu = None;
        match cmd {
            MenuCmd::Open => {
                if self.mode == Mode::Gallery {
                    self.open_viewer(self.selected);
                }
            }
            MenuCmd::BackToGallery => self.back_to_gallery(),
            MenuCmd::StopSlideshow => {
                if self.viewer.slideshow {
                    self.toggle_slideshow();
                }
            }
            MenuCmd::OpenExternal => self.open_external(),
            MenuCmd::OpenFolder => self.open_in_files(),
            MenuCmd::CopyPath => self.copy_path(),
            MenuCmd::CopyPdfText => {
                let selected = self.viewer.text_sel.is_some();
                self.copy_pdf_text(selected);
            }
            MenuCmd::DevelopRaw => self.develop_current_raw(),
            MenuCmd::Details => self.toggle_details(),
            MenuCmd::About => self.show_about(),
        }
    }

    /// Aktualizuje stan po zdarzeniach; zwraca informację o kolejnej klatce.
    pub fn update(&mut self, dt: f64) -> Update {
        let mut animating = false;
        self.thumbs.poll();
        if self.mode == Mode::Viewer {
            animating |= self.viewer.tick();
            if self.viewer.slideshow {
                match self.viewer.next_slide {
                    Some(next) if Instant::now() >= next => {
                        self.slideshow_advance();
                        animating = true;
                    }
                    _ => {}
                }
            }
        }
        if self.toast.is_animating() {
            self.toast.advance(dt, 0.3);
            animating = true;
        }
        if let Some(until) = self.toast_until {
            if Instant::now() >= until {
                self.toast.set_open_animated(false);
                self.toast_until = None;
                animating = true;
            }
        }

        let mut delay = None;
        if self.thumbs.pending() {
            delay = Some(Duration::from_millis(150));
        }
        if self.viewer.slideshow && self.mode == Mode::Viewer {
            if let Some(next) = self.viewer.next_slide {
                let remaining = next.saturating_duration_since(Instant::now());
                delay = Some(delay.map_or(remaining, |d: Duration| d.min(remaining)));
            }
        }
        if let Some(until) = self.toast_until {
            // Bez terminu kolejnej klatki toast nie zniknąłby po 4 s — okno
            // odświeża się tylko na zdarzenia, gdy nic nie animujemy.
            let remaining = until.saturating_duration_since(Instant::now());
            delay = Some(delay.map_or(remaining, |d: Duration| d.min(remaining)));
        }
        Update { animating, delay }
    }

    /// Tytuł okna.
    pub fn window_title(&self) -> String {
        match self.mode {
            Mode::Viewer => match self.items.get(self.viewer.index) {
                Some(item) => format!("{} — {}", item.name, tr("Media")),
                None => tr("Media"),
            },
            Mode::Gallery => tr("Media"),
        }
    }
}

/// Pomocnicze argumenty wiersza poleceń: `[katalog | plik]`.
fn parse_start(prefs: &Prefs) -> (PathBuf, Option<PathBuf>) {
    if let Some(arg) = std::env::args().nth(1) {
        let path = PathBuf::from(arg);
        // Ścieżka względna (`luna-media foto.jpg`) musi mieć bezwzględny katalog
        // nadrzędny — inaczej `parent()` daje pusty łańcuch i skan nie działa.
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if path.is_file() {
            if let Some(parent) = path.parent() {
                return (parent.to_path_buf(), Some(path));
            }
        } else if path.is_dir() {
            return (path, None);
        }
    }
    let last = PathBuf::from(&prefs.last_dir);
    if !prefs.last_dir.is_empty() && last.is_dir() {
        return (last, None);
    }
    (crate::places::start_dir(), None)
}

/// Uruchamia aplikację.
pub fn run(title: &str, app_id: &str) -> Result<(), Box<dyn Error>> {
    set_locale(libluna_i18n::preferred_locale());
    load_catalogs_from_dirs();
    let title = tr(title);

    let settings = ThemeSettings::load();
    let system_dark = std::env::var("LUNA_SYSTEM_DARK")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);

    let prefs = Prefs::load();
    let (dir, open) = parse_start(&prefs);
    let mut app = App::new(dir, open, prefs);

    let config = AppConfig::new(title)
        .app_id(app_id)
        .initial_size(1120, 720)
        .settings(settings)
        .system_dark(system_dark)
        // Escape w podglądzie wraca do galerii, a w galerii zamyka okno.
        .escape_to_exit(false);

    let mut last_title = String::new();
    run_app(config, move |frame: &mut Frame| {
        let theme = frame.settings().resolve_themed(frame.system_dark());
        app.theme_pref = frame.settings().preference();

        crate::ui::handle_events(&mut app, frame.events, frame.size, &theme, frame.text);
        let update = app.update(frame.delta_time());

        let title = app.window_title();
        if title != last_title {
            frame.set_title(title.clone());
            last_title = title;
        }

        crate::ui::paint(&app, frame.scene, frame.size, &theme, frame.text);

        if let Some(text) = app.text_clipboard.take() {
            frame.copy_to_clipboard(text);
        }
        if app.pending_close {
            app.pending_close = false;
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

/// Skraca ścieżkę do katalogu domowego (`~`), na potrzeby statusu.
pub fn display_dir(path: &Path) -> String {
    if crate::places::in_home(path) {
        if let Ok(rest) = path.strip_prefix(crate::places::home()) {
            let rest = rest.display().to_string();
            return if rest.is_empty() {
                "~".to_owned()
            } else {
                format!("~/{rest}")
            };
        }
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with(dir: &Path, names: &[&str]) -> App {
        for name in names {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        App::new(dir.to_path_buf(), None, Prefs::default())
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "luna-media-app-{tag}-{}-{}",
            std::process::id(),
            crate::model::now_secs()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scans_and_moves_selection() {
        let dir = temp_dir("select");
        let mut app = app_with(&dir, &["1.png", "2.png", "3.png", "4.mp4"]);
        assert_eq!(app.items.len(), 4);
        assert_eq!(app.selected, 0);
        app.move_selection(1, 0, 2);
        assert_eq!(app.selected, 1);
        app.move_selection(1, 0, 2);
        assert_eq!(app.selected, 2);
        app.move_selection(0, 1, 2);
        assert_eq!(app.selected, 3);
        app.move_selection(0, 1, 2);
        assert_eq!(app.selected, 3);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn viewer_step_wraps_and_slideshow_keeps_state() {
        let dir = temp_dir("step");
        let mut app = app_with(&dir, &["a.png", "b.mp4", "c.png"]);
        app.open_viewer(0);
        assert_eq!(app.mode, Mode::Viewer);
        app.viewer_step(1);
        assert_eq!(app.viewer.index, 1);
        app.viewer_step(1);
        assert_eq!(app.viewer.index, 2);
        app.viewer_step(1);
        assert_eq!(app.viewer.index, 0);
        assert_eq!(app.mode, Mode::Viewer);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn status_counts_images_and_videos() {
        let dir = temp_dir("status");
        let app = app_with(&dir, &["a.png", "b.jpg", "c.mp4"]);
        let text = app.status_text();
        assert!(text.contains("2"));
        assert!(text.contains("1"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pdf_text_selection_joins_lines_and_copies() {
        let dir = temp_dir("pdf-text-copy");
        let mut app = app_with(&dir, &["a.pdf"]);
        app.viewer.pdf_text = Some(Arc::new(decode::PdfText {
            width: 100.0,
            height: 100.0,
            words: vec![
                decode::PdfWord {
                    text: "Ala".into(),
                    x: 10.0,
                    y: 10.0,
                    width: 10.0,
                    height: 8.0,
                },
                decode::PdfWord {
                    text: "ma".into(),
                    x: 25.0,
                    y: 10.0,
                    width: 10.0,
                    height: 8.0,
                },
                decode::PdfWord {
                    text: "kota".into(),
                    x: 10.0,
                    y: 30.0,
                    width: 15.0,
                    height: 8.0,
                },
            ],
        }));
        app.viewer.text_sel = Some((0, 2));
        assert_eq!(app.pdf_text_string(true).as_deref(), Some("Ala ma\nkota"));
        app.copy_pdf_text(true);
        assert_eq!(app.text_clipboard.as_deref(), Some("Ala ma\nkota"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn toast_schedules_a_frame_to_hide_itself() {
        let dir = temp_dir("toast");
        let mut app = app_with(&dir, &["a.png"]);
        app.show_toast("Gotowe");
        let update = app.update(0.3);
        assert!(update.animating);
        assert!(update.delay.is_some(), "toast musi dostać termin schowania");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn display_dir_shortens_home() {
        let home = crate::places::home();
        assert_eq!(display_dir(&home), "~");
        assert!(display_dir(&home.join("Obrazy")).starts_with("~/"));
    }
}
