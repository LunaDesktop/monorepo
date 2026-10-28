//! Stan i rysowanie ekranu logowania Luna.
//!
//! UI jest oddzielone od protokołu: [`crate::greetd`] rozmawia z daemonem w tle,
//! a ten moduł dostaje gotowe zdarzenia. Dzięki temu wygląd i logikę pól można
//! testować bez greetd, Waylanda i PAM.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use libluna_core::style::{weight, Appearance};
use libluna_core::{
    ButtonState, Color, Event, GreeterSettings, IconTheme, Insets, KeyEvent, Point, PointerButton,
    Rect, Size, Theme,
};
use libluna_i18n::tr;
use libluna_platform::{Frame, WakeHandle};
use libluna_render::{wallpaper, ImageData, Scene, TextEngine};
use libluna_widgets::display::ellipsize;
use libluna_widgets::{
    icon, paint_icon_themed, Avatar, Button, ButtonVariant, ContentFit, Entry, IconName, Menu,
    MenuEntry, PaintCtx, PasswordEntry, Picture, TextInput,
};

use crate::avatars::Avatars;
use crate::greetd::{AuthKind, Command, Event as GreeterEvent, Worker};
use crate::sessions::{self, Session};
use crate::state::{self, Prefs};
use crate::users::{initials, User};

/// Szerokość karty logowania (jak w makiecie — wąska, lekka karta).
const CARD_W: f64 = 380.0;
/// Odstęp poziomy wewnątrz karty.
const CARD_PAD: f64 = 26.0;
/// Promień karty — `theme.radius.large` + 4 px.
const CARD_RADIUS: f64 = 16.0;
/// Wysokość przycisku logowania.
const LOGIN_H: f64 = 42.0;
/// Wysokość stopki karty (Opcje systemu / Sesja / Wyłącz).
const FOOTER_H: f64 = 24.0;
/// Wysokość linii komunikatu (rezerwacja, żeby układ nie skakał).
const MESSAGE_H: f64 = 18.0;
/// Średnica awatara (normalna / kompaktowa).
const AVATAR_NORMAL: f64 = 90.0;
const AVATAR_COMPACT: f64 = 76.0;
/// Kafelek konta w trybie „Inne konto”.
const USER_CHIP: f64 = 40.0;
const USER_CHIP_COMPACT: f64 = 36.0;
/// Domyślna tapeta greetera instalowana przez pakiet (`luna-greeter`).
const DEFAULT_WALLPAPER: &str = "/usr/share/wallpapers/luna/luna-desktop.png";

/// Które pole ma fokus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Username,
    Password,
}

/// Bieżący ekran greetera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Login,
    System,
}

/// Przyciski w górnym pasku.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopButton {
    Power,
    Settings,
    Wifi,
}

/// Akcje w stopce karty logowania.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FooterAction {
    System,
    Session,
    Power,
}

/// Pozycje panelu „Opcje systemu”.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemAction {
    PowerOff,
    Restart,
    Rescue,
    Console,
    Settings,
}

/// Kolejność pozycji w panelu systemowym.
const SYSTEM_ACTIONS: [SystemAction; 5] = [
    SystemAction::PowerOff,
    SystemAction::Restart,
    SystemAction::Rescue,
    SystemAction::Console,
    SystemAction::Settings,
];

/// Faza rozmowy z greetd.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Próba połączenia z gniazdem.
    Connecting,
    /// Połączono, gotowe na `create_session`.
    Idle,
    /// Wysłano żądanie i czekamy na odpowiedź.
    Busy,
    /// PAM czeka na hasło (odpowiedź ukryta).
    AwaitingSecret,
    /// PAM czeka na odpowiedź widoczną (np. nazwa użytkownika).
    AwaitingVisible,
    /// Uwierzytelniono; trwa uruchamianie sesji.
    Starting,
    /// Sesja wystartowała — greeter się zamyka.
    Started,
    /// Brak połączenia z greetd.
    Offline,
}

impl Phase {
    fn is_busy(self) -> bool {
        matches!(self, Phase::Busy | Phase::Starting | Phase::Started)
    }
}

/// Rodzaj komunikatu pod formularzem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Error,
    Success,
}

/// Komunikat widoczny pod przyciskiem logowania.
#[derive(Debug, Clone)]
pub struct Message {
    pub text: String,
    pub kind: MessageKind,
}

/// Stan greetera.
pub struct GreeterApp {
    pub users: Vec<User>,
    pub avatars: Avatars,
    pub sessions: Vec<Session>,
    pub session_index: usize,
    pub session_menu_open: bool,
    pub hover_session: Option<usize>,
    pub session_hover: bool,
    pub username: Entry,
    pub password: PasswordEntry,
    pub focus: Focus,
    pub phase: Phase,
    pub message: Option<Message>,
    pub hover_user: Option<usize>,
    pub login_hover: bool,
    pub preview: bool,
    pub hostname: String,
    pub socket: PathBuf,
    /// Ustawienia ekranu logowania (`/etc/luna/greeter.conf`).
    pub settings: GreeterSettings,
    /// Zdekodowana tapeta (raz na proces); `None` = gradient akcentu.
    pub wallpaper: Option<Arc<ImageData>>,
    /// Bieżący ekran: logowanie albo opcje systemowe.
    pub screen: Screen,
    /// Czy pokazywać pole nazwy użytkownika (tryb „Inne konto”).
    pub show_username: bool,
    pub hover_top: Option<TopButton>,
    pub hover_footer: Option<FooterAction>,
    pub hover_account: bool,
    pub hover_system_row: Option<usize>,
    pub hover_system_back: bool,
    pub hover_system_footer: bool,
    /// Komunikat panelu systemowego (np. brak uprawnień do wyłączenia).
    pub system_message: Option<Message>,
    /// Hasło wpisane przed pierwszym `Enter` — wysyłamy je automatycznie,
    /// gdy greetd poprosi o sekret (jedno `Enter` zamiast dwóch).
    pending_secret: Option<String>,
    worker: Option<Worker>,
    booted: bool,
}

impl GreeterApp {
    /// Tworzy greeter na podstawie lokalnych kont i systemowych sesji.
    pub fn new(socket: PathBuf, preview: bool) -> Self {
        let mut app = Self::with_settings(
            crate::users::local_users(),
            sessions::system_sessions(),
            socket,
            preview,
            GreeterSettings::load(),
        );
        app.apply_prefs(&state::load());
        app
    }

    /// Wariant z jawną listą użytkowników (systemowe sesje).
    pub fn from_users(users: Vec<User>, socket: PathBuf, preview: bool) -> Self {
        Self::with_sessions(users, sessions::system_sessions(), socket, preview)
    }

    /// Wariant testowy: jawni użytkownicy i jawne sesje, ustawienia systemowe.
    pub fn with_sessions(
        users: Vec<User>,
        sessions: Vec<Session>,
        socket: PathBuf,
        preview: bool,
    ) -> Self {
        Self::with_settings(users, sessions, socket, preview, GreeterSettings::load())
    }

    /// Wariant z jawnymi ustawieniami ekranu logowania (tapeta, motyw, zegar).
    pub fn with_settings(
        users: Vec<User>,
        mut sessions: Vec<Session>,
        socket: PathBuf,
        preview: bool,
        settings: GreeterSettings,
    ) -> Self {
        if sessions.is_empty() {
            sessions.push(sessions::fallback_session());
        }
        let session_index = sessions::default_index(&sessions);
        let avatars = Avatars::load(&users);
        // Brak tapety w greeter.conf → domyślna grafika Luny; jawna, ale
        // nieczytelna ścieżka nie jest podmieniana (diagnostyka zostaje czytelna).
        let wallpaper = match settings.wallpaper.as_deref() {
            Some(path) => wallpaper::load_path(path).map(Arc::new),
            None => wallpaper::load_path(Path::new(DEFAULT_WALLPAPER)).map(Arc::new),
        };
        let first = users.first().map(|u| u.name.clone()).unwrap_or_default();
        let show_username = first.trim().is_empty();
        let mut username = Entry::new(tr("Username"));
        username.set_text(first);
        let password = PasswordEntry::new(tr("Password"));
        let mut app = Self {
            users,
            avatars,
            sessions,
            session_index,
            session_menu_open: false,
            hover_session: None,
            session_hover: false,
            username,
            password,
            focus: Focus::Password,
            phase: if preview {
                Phase::Idle
            } else {
                Phase::Connecting
            },
            message: None,
            hover_user: None,
            login_hover: false,
            preview,
            hostname: hostname(),
            socket,
            settings,
            wallpaper,
            screen: Screen::Login,
            show_username,
            hover_top: None,
            hover_footer: None,
            hover_account: false,
            hover_system_row: None,
            hover_system_back: false,
            hover_system_footer: false,
            system_message: None,
            pending_secret: None,
            worker: None,
            booted: false,
        };
        if app.username.text().trim().is_empty() {
            app.set_focus(Focus::Username);
        } else {
            app.set_focus(Focus::Password);
        }
        if preview {
            app.set_message(tr("Preview mode — login disabled"), MessageKind::Info);
        }
        app
    }

    /// Stosuje zapamiętane konto/sesję (tylko jako podpowiedź; nic nie wykonuje).
    fn apply_prefs(&mut self, prefs: &Prefs) {
        if let Some(user) = &prefs.user {
            if state::valid_username(user) {
                self.username.set_text(user.clone());
                self.show_username = false;
                self.set_focus(Focus::Password);
            }
        }
        if let Some(id) = &prefs.session {
            if let Some(index) = self.sessions.iter().position(|s| &s.id == id) {
                self.session_index = index;
            }
        }
    }

    /// Zapisuje ostatnie konto i sesję po udanym uwierzytelnieniu.
    fn save_prefs(&self) {
        let user = self.username.text().trim().to_owned();
        let prefs = Prefs {
            user: state::valid_username(&user).then_some(user),
            session: self.current_session().map(|s| s.id.clone()),
        };
        state::save(&prefs);
    }

    /// Ustawia fokus i synchronizuje flagi widgetów.
    pub fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
        self.username.set_focused(focus == Focus::Username);
        self.password.set_focused(focus == Focus::Password);
    }

    fn set_message(&mut self, text: impl Into<String>, kind: MessageKind) {
        self.message = Some(Message {
            text: text.into(),
            kind,
        });
    }

    /// Czy pokazywać listę kont w trybie „Inne konto”.
    fn show_users(&self, size: Size) -> bool {
        self.users.len() > 1 && size.height >= 700.0 && size.width >= 380.0
    }

    /// Czy wolno zmienić nazwę użytkownika (lub konto) w bieżącym stanie.
    fn username_editable(&self) -> bool {
        matches!(
            self.phase,
            Phase::Idle | Phase::Offline | Phase::Connecting | Phase::AwaitingVisible
        )
    }

    /// Czy wolno przełączyć konto z paska szybkiego wyboru.
    fn user_switchable(&self) -> bool {
        matches!(self.phase, Phase::Idle | Phase::Offline | Phase::Connecting)
    }

    /// Czy wolno zmienić sesję (przed uruchomieniem sesji).
    fn session_switchable(&self) -> bool {
        !matches!(self.phase, Phase::Busy | Phase::Starting | Phase::Started)
    }

    /// Nazwa wyświetlana wybranego/ wpisanego użytkownika.
    pub fn display_name(&self) -> String {
        let name = self.username.text().trim();
        if name.is_empty() {
            return tr("Welcome");
        }
        self.users
            .iter()
            .find(|u| u.name == name)
            .map(|u| u.display.clone())
            .unwrap_or_else(|| name.to_owned())
    }

    fn avatar_initials(&self) -> String {
        let name = self.username.text().trim();
        if name.is_empty() {
            return "?".to_owned();
        }
        self.users
            .iter()
            .find(|u| u.name == name)
            .map(User::initials)
            .unwrap_or_else(|| initials(name))
    }

    fn login_label(&self) -> String {
        match self.phase {
            Phase::Connecting => tr("Connecting…"),
            Phase::Busy => tr("Signing in…"),
            Phase::Starting => tr("Starting session…"),
            Phase::Started => tr("Logged in"),
            Phase::Offline => tr("Reconnect"),
            _ => tr("Sign In"),
        }
    }

    /// Aktualnie wybrana sesja (zawsze istnieje — konstruktor dokłada fallback).
    pub fn current_session(&self) -> Option<&Session> {
        self.sessions
            .get(self.session_index)
            .or_else(|| self.sessions.first())
    }

    fn session_menu(&self) -> Menu {
        Menu::new(
            self.sessions
                .iter()
                .map(|s| MenuEntry::action(s.name.clone()).shortcut(s.kind.label()))
                .collect(),
        )
    }

    fn session_menu_panel(
        &self,
        size: Size,
        theme: &Theme,
        text: &TextEngine,
        lay: &Layout,
    ) -> Option<Rect> {
        if !self.session_menu_open {
            return None;
        }
        let menu = self.session_menu();
        let width = menu
            .width(text, theme)
            .max(lay.session_button.width())
            .min(size.width - 24.0);
        // Menu otwiera się pod kartą, wyrównane do środkowego pola stopki.
        let anchor = Rect::from_xywh(
            lay.session_button.x(),
            lay.card.bottom(),
            lay.session_button.width(),
            0.0,
        );
        Some(menu.place(anchor, width, Rect::from_size(size), theme))
    }

    /// Wykonuje kroki niezależne od rysowania: odbiera zdarzenia greetd i zegar.
    pub fn update(&mut self, frame: &mut Frame) {
        if !self.booted {
            self.booted = true;
            if !self.preview {
                self.ensure_worker(frame.waker());
            }
        }
        frame.set_motion_redraw(true);

        match self.screen {
            Screen::Login => {
                let lay = layout(
                    frame.size,
                    &frame.theme,
                    self.show_users(frame.size),
                    self.users.len(),
                    self.show_username,
                );
                for event in frame.events {
                    self.on_login_event(event, &lay, frame);
                }
            }
            Screen::System => {
                let lay = system_layout(frame.size, &frame.theme);
                for event in frame.events {
                    self.on_system_event(event, &lay, frame);
                }
            }
        }

        let mut events = Vec::new();
        if let Some(worker) = &self.worker {
            while let Some(event) = worker.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            self.on_greeter_event(event, frame);
        }

        // Zegar w nagłówku.
        frame.request_frame_after(Duration::from_secs(1));
    }

    /// Wysyła hasło wpisane przed pierwszym `Enter`, gdy PAM poprosi o sekret.
    fn respond_pending_secret(&mut self) {
        let Some(secret) = self.pending_secret.take() else {
            return;
        };
        let sent = self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.send(Command::Respond { response: secret }));
        if sent {
            self.password.clear();
            self.phase = Phase::Busy;
            self.set_message(tr("Signing in…"), MessageKind::Info);
        } else {
            self.worker = None;
            self.phase = Phase::Offline;
            self.set_message(tr("Connection to greetd failed"), MessageKind::Error);
        }
    }

    fn ensure_worker(&mut self, waker: WakeHandle) {
        if self.preview {
            return;
        }
        self.worker = Some(Worker::spawn(self.socket.clone(), Some(waker)));
        self.phase = Phase::Connecting;
        self.set_message(tr("Connecting to greetd…"), MessageKind::Info);
    }

    fn on_greeter_event(&mut self, event: GreeterEvent, frame: &mut Frame) {
        match event {
            GreeterEvent::Connected => {
                self.phase = Phase::Idle;
                self.message = None;
            }
            GreeterEvent::Prompt { kind, message } => match kind {
                AuthKind::Secret => {
                    self.phase = Phase::AwaitingSecret;
                    self.set_focus(Focus::Password);
                    self.set_message(message, MessageKind::Info);
                    // Jeśli hasło było wpisane przed pierwszym Enterem, wyślij je
                    // od razu — użytkownik nie musi wciskać Enter drugi raz.
                    self.respond_pending_secret();
                }
                AuthKind::Visible => {
                    self.phase = Phase::AwaitingVisible;
                    self.show_username = true;
                    self.set_focus(Focus::Username);
                    self.set_message(message, MessageKind::Info);
                }
                AuthKind::Info => self.set_message(message, MessageKind::Info),
                AuthKind::Error => self.set_message(message, MessageKind::Error),
            },
            GreeterEvent::AuthOk => {
                self.pending_secret = None;
                self.save_prefs();
                self.phase = Phase::Starting;
                self.set_message(tr("Starting session…"), MessageKind::Info);
                if let (Some(worker), Some(session)) = (&self.worker, self.current_session()) {
                    worker.send(Command::Start {
                        cmd: session.command.clone(),
                        env: session.env(),
                    });
                }
            }
            GreeterEvent::Started => {
                self.save_prefs();
                self.phase = Phase::Started;
                self.set_message(tr("Starting Luna…"), MessageKind::Success);
                frame.close();
            }
            GreeterEvent::ServerError {
                error_type,
                description,
            } => {
                self.worker = None;
                self.pending_secret = None;
                self.phase = Phase::Idle;
                self.password.clear();
                self.set_focus(Focus::Password);
                let text = if description.trim().is_empty() {
                    if error_type == "auth_error" {
                        tr("Incorrect username or password")
                    } else {
                        tr("Login failed")
                    }
                } else {
                    description
                };
                self.set_message(text, MessageKind::Error);
            }
            GreeterEvent::Disconnected { message } => {
                self.worker = None;
                self.pending_secret = None;
                self.phase = Phase::Offline;
                self.set_message(
                    format!("{}: {message}", tr("Connection to greetd failed")),
                    MessageKind::Error,
                );
            }
        }
    }

    fn on_login_event(&mut self, event: &Event, lay: &Layout, frame: &mut Frame) {
        let menu_panel = self.session_menu_panel(frame.size, &frame.theme, frame.text, lay);
        match event {
            Event::PointerMotion { position, .. } => {
                self.hover_top = if lay.top_power.contains(*position) {
                    Some(TopButton::Power)
                } else if lay.top_settings.contains(*position) {
                    Some(TopButton::Settings)
                } else if lay.top_wifi.contains(*position) {
                    Some(TopButton::Wifi)
                } else {
                    None
                };
                self.hover_footer = if lay.footer_system.contains(*position) {
                    Some(FooterAction::System)
                } else if lay.footer_session.contains(*position) {
                    Some(FooterAction::Session)
                } else if lay.footer_power.contains(*position) {
                    Some(FooterAction::Power)
                } else {
                    None
                };
                self.hover_account = lay.account_link.contains(*position);
                self.hover_user = lay.user_rects.iter().position(|r| r.contains(*position));
                self.login_hover = lay.login.contains(*position);
                self.session_hover = lay.session_button.contains(*position);
                self.hover_session = menu_panel
                    .and_then(|panel| self.session_menu().index_at(panel, *position, &frame.theme));
                self.password.set_peek_hover(self.password.peek_at(
                    lay.password,
                    &frame.theme,
                    *position,
                ));
            }
            Event::PointerLeave => {
                self.hover_top = None;
                self.hover_footer = None;
                self.hover_account = false;
                self.hover_user = None;
                self.hover_session = None;
                self.session_hover = false;
                self.login_hover = false;
                self.password.set_peek_hover(false);
            }
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;

                // Otwarte menu sesji ma pierwszeństwo.
                if let Some(panel) = menu_panel {
                    if let Some(index) = self.session_menu().index_at(panel, p, &frame.theme) {
                        self.session_index = index;
                        self.session_menu_open = false;
                        self.hover_session = None;
                    } else if lay.session_button.contains(p) || !panel.contains(p) {
                        self.session_menu_open = false;
                    }
                    return;
                }

                if lay.top_power.contains(p) || lay.top_settings.contains(p) {
                    self.open_system_panel();
                    return;
                }
                if lay.footer_system.contains(p) {
                    self.open_system_panel();
                    return;
                }
                if lay.footer_session.contains(p) {
                    if self.session_switchable() {
                        self.session_menu_open = true;
                        self.hover_session = Some(self.session_index);
                    }
                    return;
                }
                if lay.footer_power.contains(p) {
                    self.execute_system_action(SystemAction::PowerOff, frame);
                    return;
                }
                if lay.account_link.contains(p) {
                    self.toggle_account_mode();
                    return;
                }
                if let Some(index) = lay.user_rects.iter().position(|r| r.contains(p)) {
                    if self.user_switchable() {
                        self.select_user(index);
                    }
                } else if self.password.peek_at(lay.password, &frame.theme, p) {
                    self.password.toggle_peek();
                } else if self.password.contains(lay.password, p) {
                    self.set_focus(Focus::Password);
                } else if self.show_username && self.username.contains(lay.username, p) {
                    if self.username_editable() {
                        self.set_focus(Focus::Username);
                    }
                } else if lay.login.contains(p) {
                    self.submit(frame);
                }
            }
            Event::Key(key) if key.state == ButtonState::Pressed => self.on_key(key, frame),
            _ => {}
        }
    }

    fn on_system_event(&mut self, event: &Event, lay: &SystemLayout, frame: &mut Frame) {
        match event {
            Event::PointerMotion { position, .. } => {
                self.hover_system_row = lay.rows.iter().position(|r| r.contains(*position));
                self.hover_system_back = lay.back.contains(*position);
                self.hover_system_footer = lay.footer.contains(*position);
                self.hover_top = if lay.top_power.contains(*position) {
                    Some(TopButton::Power)
                } else if lay.top_settings.contains(*position) {
                    Some(TopButton::Settings)
                } else if lay.top_wifi.contains(*position) {
                    Some(TopButton::Wifi)
                } else {
                    None
                };
            }
            Event::PointerLeave => {
                self.hover_system_row = None;
                self.hover_system_back = false;
                self.hover_system_footer = false;
                self.hover_top = None;
            }
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;
                if lay.top_power.contains(p) {
                    self.execute_system_action(SystemAction::PowerOff, frame);
                    return;
                }
                if lay.back.contains(p) || lay.footer.contains(p) {
                    self.close_system_panel();
                    return;
                }
                if let Some(index) = lay.rows.iter().position(|r| r.contains(p)) {
                    if let Some(action) = SYSTEM_ACTIONS.get(index) {
                        self.execute_system_action(*action, frame);
                    }
                }
            }
            Event::Key(key) if key.state == ButtonState::Pressed => match key.keysym {
                0xff1b | 0xff51 => self.close_system_panel(),
                0xff52 => self.hover_system_row = Some(self.step_system_row(-1)),
                0xff54 => self.hover_system_row = Some(self.step_system_row(1)),
                0xff0d | 0xff8d => {
                    let index = self.hover_system_row.unwrap_or(0);
                    if let Some(action) = SYSTEM_ACTIONS.get(index) {
                        self.execute_system_action(*action, frame);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn step_system_row(&self, delta: isize) -> usize {
        let len = SYSTEM_ACTIONS.len() as isize;
        let current = self.hover_system_row.unwrap_or(0).min(len as usize - 1) as isize;
        (current + delta).rem_euclid(len) as usize
    }

    fn toggle_account_mode(&mut self) {
        if !self.username_editable() {
            return;
        }
        self.show_username = !self.show_username;
        if self.show_username {
            self.set_focus(Focus::Username);
        } else {
            self.set_focus(Focus::Password);
        }
    }

    fn open_system_panel(&mut self) {
        self.screen = Screen::System;
        self.session_menu_open = false;
        self.hover_session = None;
        self.hover_system_row = Some(0);
        self.system_message = None;
    }

    fn close_system_panel(&mut self) {
        self.screen = Screen::Login;
        self.hover_system_row = None;
        self.hover_system_back = false;
        self.hover_system_footer = false;
        self.system_message = None;
        self.set_focus(Focus::Password);
    }

    fn execute_system_action(&mut self, action: SystemAction, _frame: &mut Frame) {
        let ok = match action {
            SystemAction::PowerOff => {
                run_first(&[("loginctl", &["poweroff"]), ("systemctl", &["poweroff"])])
            }
            SystemAction::Restart => {
                run_first(&[("loginctl", &["reboot"]), ("systemctl", &["reboot"])])
            }
            SystemAction::Rescue => run_first(&[
                ("systemctl", &["rescue"]),
                ("systemctl", &["isolate", "rescue.target"]),
            ]),
            SystemAction::Console => {
                run_first(&[("chvt", &["2"]), ("systemctl", &["start", "getty@tty2"])])
            }
            SystemAction::Settings => std::process::Command::new("luna-settings").spawn().is_ok(),
        };
        if !ok {
            self.system_message = Some(Message {
                text: tr("System action failed"),
                kind: MessageKind::Error,
            });
        } else if action == SystemAction::Settings {
            self.system_message = Some(Message {
                text: tr("Starting settings…"),
                kind: MessageKind::Info,
            });
        }
    }

    fn on_key(&mut self, key: &KeyEvent, frame: &mut Frame) {
        // Otwarte menu sesji przechwytuje nawigację strzałkami i Enter.
        if self.session_menu_open {
            match key.keysym {
                0xff1b | 0xff09 => self.session_menu_open = false,
                0xff52 => self.hover_session = self.step_session(-1),
                0xff54 => self.hover_session = self.step_session(1),
                0xff0d | 0xff8d => {
                    if let Some(index) = self.hover_session {
                        self.session_index = index;
                    }
                    self.session_menu_open = false;
                    self.hover_session = None;
                }
                _ => {}
            }
            return;
        }
        // Escape: najpierw zwija tryb „Inne konto”, potem czyści hasło.
        if key.keysym == 0xff1b {
            if self.show_username {
                self.show_username = false;
                self.set_focus(Focus::Password);
            } else {
                self.password.clear();
            }
            self.message = None;
            return;
        }
        if self.phase.is_busy() {
            return;
        }
        match key.keysym {
            // Tab / Shift+Tab — zmiana pola.
            0xff09 => {
                if self.show_username && self.username_editable() {
                    self.set_focus(match self.focus {
                        Focus::Username => Focus::Password,
                        Focus::Password => Focus::Username,
                    });
                } else {
                    self.set_focus(Focus::Password);
                }
                return;
            }
            // Enter (Return / KP_Enter) — zaloguj.
            0xff0d | 0xff8d => {
                self.submit(frame);
                return;
            }
            _ => {}
        }
        // Gdy PAM czeka na hasło (albo tryb kont jest zwinięty), pole nazwy
        // jest niedostępne.
        if self.focus == Focus::Username && (!self.show_username || !self.username_editable()) {
            self.set_focus(Focus::Password);
        }
        let used = match self.focus {
            Focus::Username => text_key(&mut self.username, key),
            Focus::Password => text_key(&mut self.password, key),
        };
        if used && self.focus == Focus::Username {
            // Zmiana nazwy konta unieważnia hasło wpisane dla poprzedniego.
            self.password.clear();
            self.pending_secret = None;
        }
        if used
            && matches!(
                self.message.as_ref().map(|m| m.kind),
                Some(MessageKind::Error)
            )
        {
            self.message = None;
        }
    }

    fn select_user(&mut self, index: usize) {
        let Some(user) = self.users.get(index) else {
            return;
        };
        if self.username.text() != user.name {
            self.password.clear();
        }
        self.username.set_text(user.name.clone());
        self.show_username = false;
        self.set_focus(Focus::Password);
        self.pending_secret = None;
        self.message = None;
    }

    fn step_session(&self, delta: isize) -> Option<usize> {
        if self.sessions.is_empty() {
            return None;
        }
        let len = self.sessions.len() as isize;
        let current = self
            .hover_session
            .unwrap_or(self.session_index)
            .min(self.sessions.len() - 1) as isize;
        Some((current + delta).rem_euclid(len) as usize)
    }

    /// Wysyła bieżący krok logowania do greetd.
    pub fn submit(&mut self, frame: &mut Frame) {
        if self.preview {
            self.set_message(tr("Preview mode — login disabled"), MessageKind::Info);
            return;
        }
        if self.phase.is_busy() {
            return;
        }

        match self.phase {
            Phase::AwaitingSecret => {
                if self.password.text().is_empty() {
                    self.set_message(tr("Password is required"), MessageKind::Error);
                    self.set_focus(Focus::Password);
                    return;
                }
                let Some(worker) = &self.worker else {
                    self.phase = Phase::Offline;
                    return;
                };
                let response = self.password.text().to_owned();
                worker.send(Command::Respond { response });
                self.pending_secret = None;
                self.password.clear();
                self.phase = Phase::Busy;
                self.set_message(tr("Signing in…"), MessageKind::Info);
            }
            Phase::AwaitingVisible => {
                let Some(worker) = &self.worker else {
                    self.phase = Phase::Offline;
                    return;
                };
                worker.send(Command::Respond {
                    response: self.username.text().trim().to_owned(),
                });
                self.phase = Phase::Busy;
                self.set_message(tr("Signing in…"), MessageKind::Info);
            }
            _ => {
                let username = self.username.text().trim().to_owned();
                if username.is_empty() {
                    self.set_message(tr("Username is required"), MessageKind::Error);
                    self.set_focus(Focus::Username);
                    return;
                }
                if self.worker.is_none() {
                    self.ensure_worker(frame.waker());
                }
                // Hasło wpisane przed Enterem dołączymy do pierwszej odpowiedzi PAM.
                let pending =
                    (!self.password.text().is_empty()).then(|| self.password.text().to_owned());
                let Some(worker) = &self.worker else {
                    self.phase = Phase::Offline;
                    return;
                };
                if !worker.send(Command::Begin { username }) {
                    self.worker = None;
                    self.phase = Phase::Offline;
                    self.set_message(tr("Connection to greetd failed"), MessageKind::Error);
                    return;
                }
                self.pending_secret = pending;
                self.phase = Phase::Busy;
                self.set_focus(Focus::Password);
                self.set_message(tr("Signing in…"), MessageKind::Info);
            }
        }
    }

    /// Rysuje cały ekran logowania.
    pub fn paint(&self, scene: &mut Scene, size: Size, theme: &Theme, text: &TextEngine) {
        let mut ctx = PaintCtx::new(scene, theme, text);
        let compact = size.width < 760.0 || size.height < 700.0;
        paint_background(&mut ctx, size, theme, self.wallpaper.as_ref());

        match self.screen {
            Screen::Login => {
                let lay = layout(
                    size,
                    theme,
                    self.show_users(size),
                    self.users.len(),
                    self.show_username,
                );
                paint_top_bar(
                    &mut ctx,
                    size,
                    theme,
                    self,
                    lay.top_power,
                    lay.top_settings,
                    lay.top_wifi,
                    compact,
                );
                paint_login_card(&mut ctx, &lay, theme, self, compact);
                paint_session_menu(&mut ctx, &lay, size, theme, text, self);
            }
            Screen::System => {
                let lay = system_layout(size, theme);
                paint_top_bar(
                    &mut ctx,
                    size,
                    theme,
                    self,
                    lay.top_power,
                    lay.top_settings,
                    lay.top_wifi,
                    compact,
                );
                paint_system_panel(&mut ctx, &lay, theme, self, compact);
            }
        }
    }
}

// ── Układ ───────────────────────────────────────────────────────────────────

/// Prostokąty interaktywnych elementów ekranu logowania.
#[derive(Debug, Clone)]
pub struct Layout {
    pub card: Rect,
    pub avatar: Rect,
    pub greeting: Point,
    /// Link „Inne konto” / „Gotowe”.
    pub account_link: Rect,
    pub users_label: Point,
    pub user_rects: Vec<Rect>,
    pub username_label: Point,
    pub username: Rect,
    pub password: Rect,
    pub login: Rect,
    pub message: Rect,
    pub footer_system: Rect,
    pub footer_session: Rect,
    pub footer_power: Rect,
    /// Alias stopki sesji (menu i testy).
    pub session_button: Rect,
    pub top_power: Rect,
    pub top_settings: Rect,
    pub top_wifi: Rect,
}

/// Układ panelu „Opcje systemu”.
#[derive(Debug, Clone)]
pub struct SystemLayout {
    pub card: Rect,
    pub back: Rect,
    pub title: Point,
    pub rows: Vec<Rect>,
    pub message: Rect,
    pub footer: Rect,
    pub top_power: Rect,
    pub top_settings: Rect,
    pub top_wifi: Rect,
}

/// Wspólne prostokąty przycisków górnego paska.
fn top_rects(size: Size) -> (Rect, Rect, Rect) {
    let compact = size.height < 760.0 || size.width < 520.0;
    let margin = if compact { 18.0 } else { 24.0 };
    let top_size = if compact { 24.0 } else { 28.0 };
    let top_y = if compact { 14.0 } else { 16.0 };
    let right = size.width - margin;
    (
        // Kolejność jak w makiecie: wyłączanie, ustawienia, sieć.
        Rect::from_xywh(right - 3.0 * top_size - 20.0, top_y, top_size, top_size),
        Rect::from_xywh(right - 2.0 * top_size - 10.0, top_y, top_size, top_size),
        Rect::from_xywh(right - top_size, top_y, top_size, top_size),
    )
}

/// Liczy układ karty logowania dla danego rozmiaru ekranu.
pub fn layout(
    size: Size,
    theme: &Theme,
    show_users: bool,
    user_count: usize,
    show_username: bool,
) -> Layout {
    let compact = size.height < 760.0 || size.width < 520.0;
    let pad = if compact { 22.0 } else { CARD_PAD };
    let avatar_sz = if compact {
        AVATAR_COMPACT
    } else {
        AVATAR_NORMAL
    };
    let name_h = theme.typography.title.line_height;
    let label_h = theme.typography.caption.line_height;
    let entry_h = theme
        .metrics
        .entry
        .height
        .max(if compact { 38.0 } else { 42.0 });
    let button_h = if compact { 40.0 } else { LOGIN_H };
    let chip = if compact {
        USER_CHIP_COMPACT
    } else {
        USER_CHIP
    };
    let chip_name_h = theme.typography.caption.line_height;
    let chip_block = chip + chip_name_h;
    let chip_gap = 8.0;
    let link_h = label_h;

    let card_w = CARD_W.min((size.width - 40.0).max(280.0));
    let inner_w = (card_w - 2.0 * pad).max(0.0);
    let per_row = (((inner_w + chip_gap) / (chip + chip_gap)).floor() as usize).max(1);
    let visible_users = if show_username && show_users {
        user_count.min(12)
    } else {
        0
    };
    let rows = visible_users.div_ceil(per_row);
    let chips_h = if visible_users > 0 {
        label_h
            + 6.0
            + 6.0
            + rows as f64 * chip_block
            + rows.saturating_sub(1) as f64 * chip_gap
            + 12.0
    } else {
        0.0
    };
    let username_block = if show_username {
        label_h + 6.0 + entry_h + 4.0 + link_h
    } else {
        link_h
    };

    let card_h = pad
        + avatar_sz
        + 14.0
        + name_h
        + 8.0
        + username_block
        + chips_h
        + 14.0
        + entry_h
        + 12.0
        + button_h
        + 8.0
        + MESSAGE_H
        + 12.0
        + FOOTER_H
        + pad;
    let card_h = card_h.min((size.height - 8.0).max(280.0));
    let mut card_y = ((size.height - card_h) * 0.40).max(80.0);
    if card_y + card_h > size.height - 4.0 {
        card_y = (size.height - card_h - 4.0).max(12.0);
    }
    let card = Rect::from_xywh((size.width - card_w) / 2.0, card_y, card_w, card_h);

    let cx = card.center().x;
    let inner_x = card.x() + pad;
    let mut y = card.y() + pad;

    let avatar = Rect::from_xywh(cx - avatar_sz / 2.0, y, avatar_sz, avatar_sz);
    y += avatar_sz + 14.0;
    let greeting = Point::new(cx, y + name_h / 2.0);
    y += name_h + 8.0;

    let account_link;
    let mut username_label = Point::new(0.0, 0.0);
    let mut username = Rect::ZERO;
    if show_username {
        username_label = Point::new(inner_x, y + label_h / 2.0);
        y += label_h + 6.0;
        username = Rect::from_xywh(inner_x, y, inner_w, entry_h);
        y += entry_h + 4.0;
        account_link = Rect::from_xywh(cx - 60.0, y, 120.0, link_h);
        y += link_h + 4.0;
    } else {
        account_link = Rect::from_xywh(cx - 70.0, y, 140.0, link_h);
        y += link_h + 14.0;
    }

    let mut users_label = Point::new(0.0, 0.0);
    let mut user_rects = Vec::new();
    if visible_users > 0 {
        y += 6.0;
        users_label = Point::new(cx, y + label_h / 2.0);
        y += label_h + 6.0;
        let mut left = visible_users;
        while left > 0 {
            let count = left.min(per_row);
            let total = count as f64 * chip + (count.saturating_sub(1)) as f64 * chip_gap;
            let mut x = cx - total / 2.0;
            for _ in 0..count {
                user_rects.push(Rect::from_xywh(x, y, chip, chip_block));
                x += chip + chip_gap;
            }
            y += chip_block;
            left -= count;
            if left > 0 {
                y += chip_gap;
            }
        }
        y += 12.0;
    }

    let password = Rect::from_xywh(inner_x, y, inner_w, entry_h);
    y += entry_h + 12.0;
    let login = Rect::from_xywh(inner_x, y, inner_w, button_h);
    y += button_h + 8.0;
    let message = Rect::from_xywh(inner_x, y, inner_w, MESSAGE_H);
    y += MESSAGE_H + 12.0;

    let third = inner_w / 3.0;
    let footer_system = Rect::from_xywh(inner_x, y, third, FOOTER_H);
    let footer_session = Rect::from_xywh(inner_x + third, y, third, FOOTER_H);
    let footer_power = Rect::from_xywh(inner_x + 2.0 * third, y, inner_w - 2.0 * third, FOOTER_H);

    let (top_power, top_settings, top_wifi) = top_rects(size);

    Layout {
        card,
        avatar,
        greeting,
        account_link,
        users_label,
        user_rects,
        username_label,
        username,
        password,
        login,
        message,
        footer_system,
        footer_session,
        footer_power,
        session_button: footer_session,
        top_power,
        top_settings,
        top_wifi,
    }
}

/// Liczy układ panelu „Opcje systemu”.
pub fn system_layout(size: Size, _theme: &Theme) -> SystemLayout {
    let compact = size.height < 760.0 || size.width < 520.0;
    let pad = if compact { 18.0 } else { 22.0 };
    let card_w = CARD_W.min((size.width - 40.0).max(280.0));
    let header_h = 44.0;
    let row_h = 44.0;
    let button_h = 38.0;
    let rows_h = row_h * SYSTEM_ACTIONS.len() as f64;
    let card_h = pad + header_h + 1.0 + rows_h + 8.0 + MESSAGE_H + 12.0 + button_h + pad;
    let mut card_y = ((size.height - card_h) * 0.38).max(80.0);
    if card_y + card_h > size.height - 4.0 {
        card_y = (size.height - card_h - 4.0).max(12.0);
    }
    let card = Rect::from_xywh((size.width - card_w) / 2.0, card_y, card_w, card_h);

    let inner_x = card.x() + pad;
    let inner_w = card.width() - 2.0 * pad;
    let mut y = card.y() + pad;
    let back = Rect::from_xywh(inner_x, y + 6.0, 32.0, 32.0);
    let title = Point::new(inner_x + 42.0, y + header_h / 2.0);
    y += header_h + 1.0;

    let mut rows = Vec::with_capacity(SYSTEM_ACTIONS.len());
    for _ in SYSTEM_ACTIONS {
        rows.push(Rect::from_xywh(inner_x, y, inner_w, row_h));
        y += row_h;
    }
    y += 8.0;
    let message = Rect::from_xywh(inner_x, y, inner_w, MESSAGE_H);
    y += MESSAGE_H + 12.0;
    let footer = Rect::from_xywh(inner_x, y, inner_w, button_h);

    let (top_power, top_settings, top_wifi) = top_rects(size);
    SystemLayout {
        card,
        back,
        title,
        rows,
        message,
        footer,
        top_power,
        top_settings,
        top_wifi,
    }
}

// ── Rysowanie ───────────────────────────────────────────────────────────────

fn on_scrim_text() -> Color {
    Color::from_rgba8(0xf4, 0xf6, 0xfb, 0xff)
}

fn on_scrim_dim() -> Color {
    Color::rgba(1.0, 1.0, 1.0, 0.78)
}

fn paint_background(
    ctx: &mut PaintCtx,
    size: Size,
    theme: &Theme,
    wallpaper: Option<&Arc<ImageData>>,
) {
    let full = Rect::from_size(size);
    if let Some(image) = wallpaper {
        let mut picture = Picture::with_image(image.clone());
        picture.set_fit(ContentFit::Cover);
        picture.paint(ctx, full);
    } else {
        ctx.scene.fill_rect(full, theme.colors.window_bg);
        paint_aurora(ctx, size, theme);
    }

    if matches!(theme.appearance, Appearance::Dark) {
        ctx.scene
            .fill_rect(full, Color::rgba(0.02, 0.03, 0.06, 0.45));
    }

    paint_top_scrim(ctx, size);
}

fn paint_aurora(ctx: &mut PaintCtx, size: Size, theme: &Theme) {
    let accent = theme.colors.accent;
    let w = size.width;
    let h = size.height;
    ctx.scene.fill_circle(
        Point::new(w * 0.82, h * 0.10),
        w * 0.34,
        accent.scale_alpha(0.10),
    );
    ctx.scene.fill_circle(
        Point::new(w * 0.12, h * 0.92),
        w * 0.30,
        accent.scale_alpha(0.08),
    );
    ctx.scene.fill_circle(
        Point::new(w * 0.52, h * 0.56),
        w * 0.24,
        accent.scale_alpha(0.03),
    );
}

fn paint_top_scrim(ctx: &mut PaintCtx, size: Size) {
    const BANDS: usize = 48;
    let height = 92.0_f64.min(size.height * 0.22);
    for i in 0..BANDS {
        let t = i as f32 / (BANDS - 1) as f32;
        let alpha = 0.30 * (1.0 - t);
        let y = height * i as f64 / BANDS as f64;
        let h = height / BANDS as f64 + 1.0;
        ctx.scene.fill_rect(
            Rect::from_xywh(0.0, y, size.width, h),
            Color::rgba(0.01, 0.02, 0.04, alpha),
        );
    }
}

fn paint_top_bar(
    ctx: &mut PaintCtx,
    size: Size,
    theme: &Theme,
    app: &GreeterApp,
    power: Rect,
    settings: Rect,
    wifi: Rect,
    compact: bool,
) {
    let white = on_scrim_text();
    let dim = on_scrim_dim();
    let margin = if compact { 18.0 } else { 24.0 };
    let logo_sz = if compact { 22.0 } else { 24.0 };
    let logo_y = if compact { 14.0 } else { 18.0 };
    let logo = Rect::from_xywh(margin, logo_y, logo_sz, logo_sz);
    paint_icon_themed(ctx, IconName::Moon, logo, white, IconTheme::Monochrome);
    ctx.label_left(
        logo.right() + 10.0,
        logo.center().y,
        "luna-greeter",
        if compact { 14.0 } else { 15.0 },
        weight::MEDIUM,
        white,
    );

    if app.settings.show_clock {
        let text = datetime();
        let icons_left = power.x().min(settings.x()).min(wifi.x());
        ctx.label_right(
            icons_left - 14.0,
            logo.center().y,
            &text,
            if compact { 11.0 } else { 12.0 },
            weight::NORMAL,
            dim,
        );
    }

    for (rect, icon, hover) in [
        (power, IconName::Power, TopButton::Power),
        (settings, IconName::Settings, TopButton::Settings),
        (wifi, IconName::Wifi, TopButton::Wifi),
    ] {
        if app.hover_top == Some(hover) {
            ctx.scene.fill_rounded_rect(
                rect.deflate(Insets::splat(3.0)),
                theme.radius.small,
                Color::WHITE.scale_alpha(0.12),
            );
        }
        match icon {
            IconName::Settings => icon::gear(ctx, white, rect.center(), rect.width() * 0.72),
            _ => paint_icon_themed(
                ctx,
                icon,
                rect.deflate(Insets::splat(6.0)),
                white,
                IconTheme::Monochrome,
            ),
        }
    }

    let _ = size;
}

fn paint_main_avatar(
    ctx: &mut PaintCtx,
    rect: Rect,
    image: Option<&Arc<ImageData>>,
    initials: &str,
) {
    let theme = *ctx.theme;
    let radius = rect.width().min(rect.height()) / 2.0;
    ctx.scene.fill_circle(
        rect.center(),
        radius + 8.0,
        theme.colors.accent.scale_alpha(0.08),
    );
    if let Some(image) = image {
        ctx.scene.push_clip_rounded(rect, radius);
        let mut picture = Picture::with_image(image.clone());
        picture.set_fit(ContentFit::Cover);
        picture.paint(ctx, rect);
        ctx.scene.pop_clip();
    } else {
        Avatar::new(initials).paint(ctx, rect);
    }
    ctx.scene.stroke_circle(
        rect.center(),
        radius + 1.0,
        1.5,
        Color::WHITE.scale_alpha(0.28),
    );
}

fn paint_chip_avatar(
    ctx: &mut PaintCtx,
    rect: Rect,
    initials: &str,
    image: Option<&Arc<ImageData>>,
    selected: bool,
) {
    let theme = *ctx.theme;
    let radius = rect.width().min(rect.height()) / 2.0;
    if let Some(image) = image {
        ctx.scene.push_clip_rounded(rect, radius);
        let mut picture = Picture::with_image(image.clone());
        picture.set_fit(ContentFit::Cover);
        picture.paint(ctx, rect);
        ctx.scene.pop_clip();
    } else {
        Avatar::new(initials).paint(ctx, rect);
    }
    if selected {
        ctx.scene
            .stroke_circle(rect.center(), radius + 2.0, 2.0, theme.colors.accent);
    }
}

fn paint_login_card(
    ctx: &mut PaintCtx,
    lay: &Layout,
    theme: &Theme,
    app: &GreeterApp,
    compact: bool,
) {
    paint_card_shell(ctx, &lay.card, theme);
    paint_main_avatar(
        ctx,
        lay.avatar,
        app.avatars.get(app.username.text().trim()),
        &app.avatar_initials(),
    );
    ctx.label_center(
        lay.greeting.x,
        lay.greeting.y,
        &app.display_name(),
        theme.typography.title.size,
        weight::BOLD,
        theme.colors.text,
    );

    paint_account_link(ctx, lay, theme, app);
    if app.show_username {
        ctx.label_left(
            lay.username_label.x,
            lay.username_label.y,
            &tr("Username"),
            theme.typography.caption.size,
            weight::BOLD,
            theme.colors.dim_text,
        );
        app.username.paint(ctx, lay.username);
        paint_user_chips(ctx, lay, theme, app, compact);
    }

    app.password.paint(ctx, lay.password);

    let mut button = Button::new(app.login_label()).variant(ButtonVariant::Suggested);
    if app.phase.is_busy() || app.phase == Phase::Started {
        button = button.enabled(false);
    }
    button.set_hovered(app.login_hover);
    button.paint(ctx, lay.login);

    paint_message(ctx, lay, theme, app);
    paint_footer(ctx, lay, theme, app);
}

fn paint_account_link(ctx: &mut PaintCtx, lay: &Layout, theme: &Theme, app: &GreeterApp) {
    let label = if app.show_username {
        tr("Done")
    } else {
        tr("Other accounts")
    };
    let color = if app.hover_account {
        theme.colors.accent
    } else {
        theme.colors.dim_text
    };
    if app.hover_account {
        ctx.scene
            .fill_rounded_rect(lay.account_link, theme.radius.small, theme.colors.shade);
    }
    ctx.label_center(
        lay.account_link.center().x,
        lay.account_link.center().y,
        &label,
        theme.typography.caption.size,
        weight::MEDIUM,
        color,
    );
}

fn paint_user_chips(
    ctx: &mut PaintCtx,
    lay: &Layout,
    theme: &Theme,
    app: &GreeterApp,
    compact: bool,
) {
    if lay.user_rects.is_empty() {
        return;
    }
    ctx.label_center(
        lay.users_label.x,
        lay.users_label.y,
        &tr("Select an account"),
        theme.typography.caption.size,
        weight::BOLD,
        theme.colors.dim_text,
    );
    for (i, rect) in lay.user_rects.iter().enumerate() {
        let Some(user) = app.users.get(i) else {
            continue;
        };
        let selected = app.username.text().trim() == user.name;
        let hovered = app.hover_user == Some(i);
        let chip = rect.width();
        if selected || hovered {
            let cell = rect.deflate(Insets::splat(2.0));
            ctx.scene.fill_rounded_rect(
                cell,
                theme.radius.medium,
                if selected {
                    theme.colors.accent.scale_alpha(0.14)
                } else {
                    theme.colors.shade
                },
            );
        }
        let avatar = Rect::from_xywh(rect.center().x - chip / 2.0, rect.y(), chip, chip);
        paint_chip_avatar(
            ctx,
            avatar,
            &user.initials(),
            app.avatars.get(&user.name),
            selected,
        );
        let name = user.display.split_whitespace().next().unwrap_or(&user.name);
        let name = ellipsize(name, chip + 16.0, theme.typography.caption.size, ctx.text);
        ctx.label_center(
            rect.center().x,
            rect.y() + chip + if compact { 7.0 } else { 8.0 },
            &name,
            theme.typography.caption.size,
            if selected {
                weight::BOLD
            } else {
                weight::NORMAL
            },
            if selected {
                theme.colors.text
            } else {
                theme.colors.dim_text
            },
        );
    }
}

fn paint_message(ctx: &mut PaintCtx, lay: &Layout, theme: &Theme, app: &GreeterApp) {
    let Some(message) = &app.message else {
        return;
    };
    let color = match message.kind {
        MessageKind::Info => theme.colors.dim_text,
        MessageKind::Error => theme.colors.error,
        MessageKind::Success => theme.colors.success,
    };
    let text = ellipsize(
        &message.text,
        lay.message.width(),
        theme.typography.caption.size,
        ctx.text,
    );
    ctx.label_center(
        lay.message.center().x,
        lay.message.center().y,
        &text,
        theme.typography.caption.size,
        weight::MEDIUM,
        color,
    );
}

fn paint_footer(ctx: &mut PaintCtx, lay: &Layout, theme: &Theme, app: &GreeterApp) {
    paint_footer_item(
        ctx,
        lay.footer_system,
        Some(IconName::Settings),
        &tr("System options"),
        FooterAlign::Left,
        app.hover_footer == Some(FooterAction::System),
        true,
        theme,
    );

    let session = tr("Session");
    let session = ellipsize(
        &session,
        lay.footer_session.width() - 18.0,
        theme.typography.caption.size,
        ctx.text,
    );
    paint_footer_item(
        ctx,
        lay.footer_session,
        None,
        &session,
        FooterAlign::Center,
        app.hover_footer == Some(FooterAction::Session),
        app.session_switchable(),
        theme,
    );

    paint_footer_item(
        ctx,
        lay.footer_power,
        Some(IconName::Power),
        &tr("Power off"),
        FooterAlign::Right,
        app.hover_footer == Some(FooterAction::Power),
        true,
        theme,
    );
}

#[derive(Clone, Copy)]
enum FooterAlign {
    Left,
    Center,
    Right,
}

fn paint_footer_item(
    ctx: &mut PaintCtx,
    rect: Rect,
    icon: Option<IconName>,
    label: &str,
    align: FooterAlign,
    hovered: bool,
    enabled: bool,
    theme: &Theme,
) {
    if hovered {
        ctx.scene.fill_rounded_rect(
            rect.deflate(Insets::splat(2.0)),
            theme.radius.small,
            theme.colors.shade,
        );
    }
    let color = if enabled {
        theme.colors.dim_text
    } else {
        theme.colors.dim_text.scale_alpha(0.5)
    };
    let icon_sz = 14.0;
    let gap = 6.0;
    let leading = if icon.is_some() { icon_sz + gap } else { 0.0 };
    let text_w = ctx.measure(label, theme.typography.caption.size, weight::MEDIUM);
    let content_w = leading + text_w;
    let start = match align {
        FooterAlign::Left => rect.x() + 2.0,
        FooterAlign::Center => rect.center().x - content_w / 2.0,
        FooterAlign::Right => rect.right() - content_w - 2.0,
    };
    if let Some(icon) = icon {
        let icon_rect = Rect::from_xywh(start, rect.center().y - icon_sz / 2.0, icon_sz, icon_sz);
        match icon {
            IconName::Settings => icon::gear(ctx, color, icon_rect.center(), icon_sz * 0.9),
            _ => paint_icon_themed(ctx, icon, icon_rect, color, IconTheme::Monochrome),
        }
    }
    ctx.label_left(
        start + leading,
        rect.center().y,
        label,
        theme.typography.caption.size,
        weight::MEDIUM,
        color,
    );
}

fn paint_system_panel(
    ctx: &mut PaintCtx,
    lay: &SystemLayout,
    theme: &Theme,
    app: &GreeterApp,
    compact: bool,
) {
    paint_card_shell(ctx, &lay.card, theme);

    if app.hover_system_back {
        ctx.scene
            .fill_rounded_rect(lay.back, theme.radius.small, theme.colors.shade);
    }
    icon::chevron_left(ctx, theme.colors.text, lay.back.center(), 5.0, 1.8);
    ctx.label_left(
        lay.title.x,
        lay.title.y,
        &tr("System options"),
        if compact { 14.0 } else { 15.0 },
        weight::BOLD,
        theme.colors.text,
    );

    let separator_y = lay.rows.first().map(|r| r.y()).unwrap_or(lay.title.y) - 0.5;
    ctx.scene.fill_rect(
        Rect::from_xywh(
            lay.card.x() + 18.0,
            separator_y,
            lay.card.width() - 36.0,
            1.0,
        ),
        theme.colors.border,
    );

    for (index, rect) in lay.rows.iter().enumerate() {
        let Some(action) = SYSTEM_ACTIONS.get(index) else {
            continue;
        };
        let hovered = app.hover_system_row == Some(index);
        if hovered {
            ctx.scene.fill_rounded_rect(
                rect.deflate(Insets::splat(2.0)),
                theme.radius.small,
                theme.colors.shade,
            );
        }
        paint_system_action_icon(
            ctx,
            *action,
            Rect::from_xywh(rect.x() + 10.0, rect.center().y - 10.0, 20.0, 20.0),
            theme.colors.accent,
        );
        ctx.label_left(
            rect.x() + 40.0,
            rect.center().y,
            &system_action_label(*action),
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.text,
        );
        icon::chevron_right(
            ctx,
            theme.colors.dim_text,
            Point::new(rect.right() - 16.0, rect.center().y),
            4.0,
            1.8,
        );
    }

    if let Some(message) = &app.system_message {
        let color = match message.kind {
            MessageKind::Info => theme.colors.dim_text,
            MessageKind::Error => theme.colors.error,
            MessageKind::Success => theme.colors.success,
        };
        let text = ellipsize(
            &message.text,
            lay.message.width(),
            theme.typography.caption.size,
            ctx.text,
        );
        ctx.label_center(
            lay.message.center().x,
            lay.message.center().y,
            &text,
            theme.typography.caption.size,
            weight::MEDIUM,
            color,
        );
    }

    paint_back_button(ctx, lay.footer, theme, app.hover_system_footer);
}

fn paint_system_action_icon(ctx: &mut PaintCtx, action: SystemAction, rect: Rect, color: Color) {
    match action {
        SystemAction::PowerOff => {
            paint_icon_themed(ctx, IconName::Power, rect, color, IconTheme::Monochrome)
        }
        SystemAction::Restart => icon::restart(ctx, color, rect.center(), rect.width()),
        SystemAction::Rescue => icon::shield(ctx, color, rect.center(), rect.width()),
        SystemAction::Console => {
            paint_icon_themed(ctx, IconName::Terminal, rect, color, IconTheme::Monochrome)
        }
        SystemAction::Settings => icon::gear(ctx, color, rect.center(), rect.width() * 0.9),
    }
}

fn system_action_label(action: SystemAction) -> String {
    match action {
        SystemAction::PowerOff => tr("Power off"),
        SystemAction::Restart => tr("Restart"),
        SystemAction::Rescue => tr("Rescue mode"),
        SystemAction::Console => tr("Console"),
        SystemAction::Settings => tr("Settings"),
    }
}

fn paint_back_button(ctx: &mut PaintCtx, rect: Rect, theme: &Theme, hovered: bool) {
    ctx.scene.fill_rounded_rect(
        rect,
        theme.radius.medium,
        if hovered {
            theme.colors.shade.scale_alpha(1.8)
        } else {
            theme.colors.shade
        },
    );
    let label = tr("Back");
    let size = theme.typography.body.size;
    let text_w = ctx.measure(&label, size, weight::MEDIUM);
    let content_w = 16.0 + 6.0 + text_w;
    let start = rect.center().x - content_w / 2.0;
    icon::chevron_left(
        ctx,
        theme.colors.text,
        Point::new(start + 7.0, rect.center().y),
        4.0,
        1.8,
    );
    ctx.label_left(
        start + 16.0 + 6.0,
        rect.center().y,
        &label,
        size,
        weight::MEDIUM,
        theme.colors.text,
    );
}

fn paint_card_shell(ctx: &mut PaintCtx, card: &Rect, theme: &Theme) {
    for (dy, spread, alpha) in [(12.0, 8.0, 0.10), (5.0, 3.0, 0.12)] {
        let shadow = Rect::from_xywh(
            card.x() - spread,
            card.y() + dy - spread,
            card.width() + 2.0 * spread,
            card.height() + 2.0 * spread,
        );
        ctx.scene.fill_rounded_rect(
            shadow,
            CARD_RADIUS + spread,
            Color::BLACK.scale_alpha(alpha),
        );
    }
    ctx.scene
        .fill_rounded_rect(*card, CARD_RADIUS, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(*card, CARD_RADIUS, 1.0, theme.colors.border);
}

fn paint_session_menu(
    ctx: &mut PaintCtx,
    lay: &Layout,
    size: Size,
    theme: &Theme,
    text: &TextEngine,
    app: &GreeterApp,
) {
    let Some(panel) = app.session_menu_panel(size, theme, text, lay) else {
        return;
    };
    let shadow = Rect::from_xywh(
        panel.x() + 2.0,
        panel.y() + 8.0,
        panel.width(),
        panel.height(),
    );
    ctx.scene.fill_rounded_rect(
        shadow,
        theme.radius.medium + 6.0,
        Color::BLACK.scale_alpha(0.32),
    );
    let mut menu = app.session_menu();
    menu.set_hovered(app.hover_session);
    menu.paint(ctx, panel);
}

fn datetime() -> String {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    let ok = unsafe { !libc::localtime_r(&now, tm.as_mut_ptr()).is_null() };
    if !ok {
        return "--:--".to_owned();
    }
    let tm = unsafe { tm.assume_init() };
    let loc = libluna_i18n::locale();
    let (weekday, month) = match loc.language() {
        "pl" => (
            ["Nie", "Pon", "Wt", "Śr", "Czw", "Pt", "Sob"],
            [
                "sty", "lut", "mar", "kwi", "maj", "cze", "lip", "sie", "wrz", "paź", "lis", "gru",
            ],
        ),
        "de" => (
            ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"],
            [
                "Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
            ],
        ),
        _ => (
            ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
            [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ],
        ),
    };
    let wd = weekday[(tm.tm_wday.clamp(0, 6)) as usize];
    let mo = month[(tm.tm_mon.clamp(0, 11)) as usize];
    format!(
        "{wd} {} {mo} {} {:02}:{:02}",
        tm.tm_mday,
        tm.tm_year + 1900,
        tm.tm_hour,
        tm.tm_min
    )
}
fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "luna".to_owned())
}

/// Uruchamia pierwszą dostępną komendę systemową; `true`, gdy zakończyła się sukcesem.
fn run_first(commands: &[(&str, &[&str])]) -> bool {
    for (program, args) in commands {
        match std::process::Command::new(program).args(*args).status() {
            Ok(status) if status.success() => return true,
            _ => {}
        }
    }
    false
}

/// Obsługa pojedynczego klawisza w polu tekstowym (wspólna dla `Entry` i hasła).
fn text_key<T: TextInput>(input: &mut T, key: &KeyEvent) -> bool {
    let shift = key.modifiers.shift;
    let ctrl = key.modifiers.ctrl;
    if ctrl && key.keysym == 0x61 {
        input.select_all();
        return true;
    }
    if !ctrl {
        if let Some(text) = &key.text {
            if !text.is_empty() && !text.chars().all(char::is_control) {
                input.insert_str(text);
                return true;
            }
        }
    }
    match key.keysym {
        0xff08 => input.backspace(),
        0xffff => input.delete_forward(),
        0xff51 => {
            if shift {
                input.move_left_selecting(true)
            } else {
                input.move_left()
            }
        }
        0xff53 => {
            if shift {
                input.move_right_selecting(true)
            } else {
                input.move_right()
            }
        }
        0xff50 => {
            if shift {
                input.home_selecting(true)
            } else {
                input.home()
            }
        }
        0xff57 => {
            if shift {
                input.end_selecting(true)
            } else {
                input.end()
            }
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::SessionKind;
    use libluna_core::{Accent, GreeterSettings, ThemePreference};
    use libluna_render::{SoftwareRenderer, TextEngine};
    use std::io::Read;
    use std::os::unix::net::UnixListener;

    fn sample_users() -> Vec<User> {
        vec![
            User {
                name: "patryk".to_owned(),
                display: "Patryk Szkudlarek".to_owned(),
                uid: 1000,
            },
            User {
                name: "student".to_owned(),
                display: "Student".to_owned(),
                uid: 1001,
            },
        ]
    }

    fn sample_sessions() -> Vec<Session> {
        vec![
            Session {
                id: "selene".to_owned(),
                name: "Luna".to_owned(),
                command: vec!["luna-session".to_owned()],
                desktop: "Luna".to_owned(),
                kind: SessionKind::Wayland,
                path: PathBuf::new(),
            },
            Session {
                id: "sway".to_owned(),
                name: "Sway".to_owned(),
                command: vec!["sway".to_owned()],
                desktop: "sway".to_owned(),
                kind: SessionKind::Wayland,
                path: PathBuf::new(),
            },
            Session {
                id: "gnome-x11".to_owned(),
                name: "GNOME".to_owned(),
                command: vec!["gnome-session".to_owned()],
                desktop: "GNOME".to_owned(),
                kind: SessionKind::X11,
                path: PathBuf::new(),
            },
        ]
    }

    fn sample_app() -> GreeterApp {
        GreeterApp::with_sessions(
            sample_users(),
            sample_sessions(),
            PathBuf::from("/tmp/none.sock"),
            true,
        )
    }

    fn test_socket_path() -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "luna-greeter-app-test-{}-{stamp}.sock",
            std::process::id()
        ))
    }

    fn read_frame(stream: &mut std::os::unix::net::UnixStream) -> String {
        let mut header = [0u8; 4];
        stream.read_exact(&mut header).unwrap();
        let len = u32::from_ne_bytes(header) as usize;
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).unwrap();
        String::from_utf8(payload).unwrap()
    }

    #[test]
    fn pending_password_is_sent_without_second_enter() {
        let path = test_socket_path();
        let listener = UnixListener::bind(&path).expect("bind");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let line = read_frame(&mut stream);
            assert!(line.contains("post_auth_message_response"), "{line}");
            assert!(line.contains("hunter2"), "{line}");
        });

        let mut app =
            GreeterApp::with_sessions(sample_users(), sample_sessions(), path.clone(), false);
        app.worker = Some(Worker::spawn(path.clone(), None));
        app.phase = Phase::Busy;
        app.pending_secret = Some("hunter2".to_owned());
        app.password.set_text("hunter2");

        app.respond_pending_secret();
        server.join().unwrap();

        assert!(app.password.text().is_empty());
        assert_eq!(app.phase, Phase::Busy);
        assert!(app.pending_secret.is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn layout_keeps_fields_inside_card() {
        let theme = Theme::dark();
        let lay = layout(Size::new(1200.0, 800.0), &theme, true, 2, true);
        for rect in [
            lay.username,
            lay.password,
            lay.login,
            lay.avatar,
            lay.user_rects[0],
            lay.session_button,
        ] {
            assert!(lay.card.contains(rect.center()), "{rect:?} poza kartą");
        }
        assert!(lay.username.y() > lay.avatar.bottom());
        assert!(lay.password.y() > lay.username.bottom());
        assert!(lay.login.y() > lay.password.bottom());
        assert!(lay.session_button.y() > lay.login.bottom());
    }

    #[test]
    fn many_accounts_wrap_to_next_rows() {
        let theme = Theme::dark();
        let lay = layout(Size::new(1200.0, 900.0), &theme, true, 12, true);
        assert_eq!(lay.user_rects.len(), 12);
        let first_row_y = lay.user_rects[0].y();
        assert!(
            lay.user_rects.iter().any(|r| r.y() > first_row_y),
            "konta powinny zawijać się do kolejnego rzędu"
        );
        assert!(lay.user_rects.iter().all(|r| lay.card.contains(r.center())));
    }

    #[test]
    fn default_layout_is_password_only() {
        let theme = Theme::dark();
        let lay = layout(Size::new(1200.0, 800.0), &theme, true, 3, false);
        assert!(lay.username.is_empty(), "pole użytkownika ma być ukryte");
        assert!(lay.user_rects.is_empty(), "kafelki kont mają być ukryte");
        for rect in [
            lay.password,
            lay.login,
            lay.avatar,
            lay.account_link,
            lay.footer_system,
            lay.footer_session,
            lay.footer_power,
        ] {
            assert!(lay.card.contains(rect.center()), "{rect:?} poza kartą");
        }
        assert!(lay.password.y() > lay.avatar.bottom());
        assert!(lay.login.y() > lay.password.bottom());
        assert!(lay.footer_session.y() > lay.login.bottom());
    }

    #[test]
    fn display_name_falls_back_to_typed_login() {
        let app = sample_app();
        assert_eq!(app.display_name(), "Patryk Szkudlarek");
        let mut app = app;
        app.username.set_text("nieznany");
        assert_eq!(app.display_name(), "nieznany");
    }

    #[test]
    fn initials_come_from_display_name() {
        assert_eq!(sample_app().avatar_initials(), "PS");
    }

    #[test]
    fn session_choice_drives_start_environment() {
        let mut app = sample_app();
        assert_eq!(app.current_session().unwrap().name, "Luna");
        app.session_index = 2;
        let session = app.current_session().unwrap();
        assert_eq!(session.name, "GNOME");
        assert_eq!(session.kind, SessionKind::X11);
        assert!(session.env().contains(&"XDG_SESSION_TYPE=x11".to_owned()));
    }

    #[test]
    fn prefs_restore_user_and_session() {
        let mut app = sample_app();
        app.apply_prefs(&Prefs {
            user: Some("student".to_owned()),
            session: Some("sway".to_owned()),
        });
        assert_eq!(app.username.text(), "student");
        assert_eq!(app.current_session().unwrap().name, "Sway");
        assert_eq!(app.focus, Focus::Password);
    }

    #[test]
    fn auth_states_lock_the_account_switch() {
        let mut app = sample_app();
        app.phase = Phase::AwaitingSecret;
        assert!(!app.username_editable());
        assert!(!app.user_switchable());
        app.phase = Phase::AwaitingVisible;
        assert!(app.username_editable());
        assert!(!app.user_switchable());
        app.phase = Phase::Idle;
        assert!(app.username_editable());
        assert!(app.user_switchable());
    }

    #[test]
    fn settings_load_wallpaper_and_clock() {
        // PNG wygenerowany rendererem — bez dodatkowych zależności w teście.
        let mut renderer = SoftwareRenderer::new(8, 8);
        renderer.draw(&Scene::new(), 1.0, Color::WHITE);
        let png = renderer.encode_png().expect("PNG");
        let path =
            std::env::temp_dir().join(format!("luna-greeter-wallpaper-{}.png", std::process::id()));
        std::fs::write(&path, png).expect("zapis PNG");

        let settings = GreeterSettings {
            wallpaper: Some(path.clone()),
            theme: ThemePreference::Light,
            accent: Some(Accent::Purple),
            show_clock: false,
        };
        let app = GreeterApp::with_settings(
            sample_users(),
            sample_sessions(),
            PathBuf::from("/tmp/none.sock"),
            true,
            settings.clone(),
        );
        assert!(app.wallpaper.is_some(), "tapeta powinna się zdekodować");
        assert_eq!(app.settings, settings);
        // Rysowanie z tapetą i bez zegara nie może panikować.
        let theme = Theme::dark();
        let mut scene = Scene::new();
        app.paint(
            &mut scene,
            Size::new(1200.0, 800.0),
            &theme,
            &TextEngine::none(),
        );
        assert!(!scene.is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_wallpaper_falls_back_to_gradient() {
        let app = GreeterApp::with_settings(
            sample_users(),
            sample_sessions(),
            PathBuf::from("/tmp/none.sock"),
            true,
            GreeterSettings {
                wallpaper: Some(PathBuf::from("/nie/ma/takiego.png")),
                ..GreeterSettings::default()
            },
        );
        assert!(app.wallpaper.is_none());
    }

    /// Podgląd ekranów greetera do PNG (bez Waylanda i greetd).
    #[test]
    #[ignore]
    fn render_greeter_preview() {
        libluna_i18n::set_locale(libluna_i18n::Locale::new("pl"));
        libluna_i18n::load_catalogs_from_dirs();
        let text = TextEngine::system();
        let size = Size::new(1200.0, 800.0);
        let render = |app: &GreeterApp, theme: &Theme, size: Size, path: &str| {
            let mut scene = Scene::new();
            app.paint(&mut scene, size, theme, &text);
            let mut renderer = SoftwareRenderer::new(size.width as u32, size.height as u32);
            renderer.draw(&scene, 1.0, theme.colors.window_bg);
            let png = renderer.encode_png().expect("PNG");
            std::fs::write(path, png).expect("zapis PNG");
        };
        let wallpaper_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/wallpapers/luna-desktop.png");
        let make = || {
            let mut app = GreeterApp::with_settings(
                sample_users(),
                sample_sessions(),
                PathBuf::from("/tmp/none.sock"),
                true,
                GreeterSettings {
                    wallpaper: Some(wallpaper_path.clone()),
                    ..GreeterSettings::default()
                },
            );
            // Podgląd służy tylko rysowaniu — bez komunikatu o trybie podglądu.
            app.message = None;
            app
        };
        let app = make();
        let dark = Theme::dark();
        let light = Theme::light();
        render(&app, &dark, size, "/tmp/luna-greeter.png");
        render(&app, &light, size, "/tmp/luna-greeter-light.png");
        render(
            &app,
            &dark,
            Size::new(1024.0, 600.0),
            "/tmp/luna-greeter-compact.png",
        );

        let mut menu = make();
        menu.session_menu_open = true;
        menu.hover_session = Some(1);
        render(&menu, &dark, size, "/tmp/luna-greeter-menu.png");

        // Ekran „Opcje systemu”.
        let mut system = make();
        system.screen = Screen::System;
        system.hover_system_row = Some(1);
        render(&system, &dark, size, "/tmp/luna-greeter-system.png");
        render(&system, &light, size, "/tmp/luna-greeter-system-light.png");

        // Wariant „Inne konto” z siatką kont do oceny układu.
        let mut accounts = make();
        accounts.show_username = true;
        render(&accounts, &dark, size, "/tmp/luna-greeter-accounts.png");
    }
}
