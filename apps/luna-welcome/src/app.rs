//! Stan i akcje aplikacji Luna Welcome.
//!
//! Powitanie nie wykonuje żadnych operacji systemowych poza uruchomieniem
//! wybranej aplikacji. Na obrazie live główną akcją jest przekierowanie do
//! `luna-installer`; na zainstalowanym systemie zostają skróty do Sklepu,
//! Ustawień i dokumentacji.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use libluna_core::Point;
use libluna_widgets::ScrollView;

use crate::live::LiveEnvironment;
use crate::news::{self, Notes};

/// Akcja dostępna z poziomu powitania.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Install,
    Store,
    Settings,
    Docs,
    Close,
}

impl Action {
    pub fn label(self) -> String {
        match self {
            Self::Install => libluna_i18n::tr("Install Luna"),
            Self::Store => libluna_i18n::tr("Open the Store"),
            Self::Settings => libluna_i18n::tr("Open Settings"),
            Self::Docs => libluna_i18n::tr("Read the documentation"),
            Self::Close => libluna_i18n::tr("Close"),
        }
    }
}

/// Żądania okna, wykonane po zakończeniu klatki.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pending {
    pub close: bool,
    pub minimize: bool,
    pub maximize: bool,
    pub begin_move: bool,
}

/// Stan aplikacji.
pub struct App {
    pub live: LiveEnvironment,
    pub notes: Notes,
    pub version: String,
    pub scroll: ScrollView,
    pub pointer: Option<Point>,
    pub hover: Option<Action>,
    pub pending: Pending,
    pub error: Option<String>,
    error_until: Option<Instant>,
    started: Instant,
}

impl App {
    pub fn new() -> Self {
        let live = LiveEnvironment::detect();
        let notes = news::load();
        let version = news::system_version(&notes);
        Self::with_environment(live, notes, version)
    }

    pub fn with_environment(live: LiveEnvironment, notes: Notes, version: String) -> Self {
        Self {
            live,
            notes,
            version,
            scroll: ScrollView::new(),
            pointer: None,
            hover: None,
            pending: Pending::default(),
            error: None,
            error_until: None,
            started: Instant::now(),
        }
    }

    /// Akcja sugerowana (Enter / największy przycisk).
    pub fn primary_action(&self) -> Option<Action> {
        self.live.live.then_some(Action::Install)
    }

    /// Wykonuje akcję; błąd trafia do paska komunikatu, nie zamyka aplikacji.
    pub fn launch(&mut self, action: Action) {
        if action == Action::Close {
            self.pending.close = true;
            return;
        }
        let result = match action {
            Action::Install => self.spawn(
                std::env::var_os("LUNA_INSTALLER")
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
                    .as_deref()
                    .unwrap_or_else(|| std::path::Path::new("luna-installer")),
            ),
            Action::Store => self.spawn(std::path::Path::new("luna-store")),
            Action::Settings => self.spawn(std::path::Path::new("luna-settings")),
            Action::Docs => open_uri("https://luna.paganlinux.eu/docs/"),
            Action::Close => Ok(()),
        };
        if let Err(error) = result {
            self.show_error(error);
        }
    }

    pub fn show_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
        self.error_until = Some(Instant::now() + Duration::from_secs(12));
    }

    fn spawn(&self, program: &std::path::Path) -> Result<(), String> {
        let mut command = Command::new(program);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command.spawn().map(|_| ()).map_err(|err| {
            libluna_i18n::tr_args(
                "Could not start {program}: {error}",
                &[
                    ("program", &program.display().to_string()),
                    ("error", &err.to_string()),
                ],
            )
        })
    }

    /// Aktualizuje licznik czasu (wygaszanie komunikatu błędu).
    pub fn tick(&mut self, dt: f64) {
        let _ = dt;
        if self
            .error_until
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.error = None;
            self.error_until = None;
        }
    }

    pub fn elapsed(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    pub fn needs_frames(&self) -> bool {
        self.error.is_some()
    }

    /// Zapisuje żądania okna do [`crate::ui`] — aplikacja wywołuje to raz na klatkę.
    pub fn apply_pending(&mut self, frame: &mut libluna_platform::Frame<'_>) {
        if self.pending.close {
            frame.close();
        }
        if self.pending.minimize {
            frame.minimize();
        }
        if self.pending.maximize {
            frame.toggle_maximize();
        }
        if self.pending.begin_move {
            frame.begin_move();
        }
        self.pending = Pending::default();
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

fn open_uri(uri: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(uri)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|err| {
            libluna_i18n::tr_args(
                "Could not open {uri}: {error}",
                &[("uri", uri), ("error", &err.to_string())],
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(live: bool) -> App {
        let notes = Notes::parse("## 0.0.8 — 2026-11-05\n- Nowość\n");
        App::with_environment(
            LiveEnvironment {
                live,
                medium: None,
                reason: String::new(),
            },
            notes,
            "0.0.8".to_owned(),
        )
    }

    #[test]
    fn live_image_offers_installer_as_primary_action() {
        assert_eq!(app(true).primary_action(), Some(Action::Install));
    }

    #[test]
    fn installed_system_has_no_installer_action() {
        assert_eq!(app(false).primary_action(), None);
    }

    #[test]
    fn close_sets_window_request() {
        let mut app = app(true);
        app.launch(Action::Close);
        assert!(app.pending.close);
    }
}
