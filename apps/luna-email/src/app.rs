//! Stan aplikacji i pętla klatki.
//!
//! UI jest immediate-mode: lista jest rysowana co klatkę, a kliknięcia trafiają
//! w prostokąty zebrane w [`App::hits`]. Dopóki nie ma konta, aplikacja pracuje
//! na wbudowanych danych demonstracyjnych — dzięki temu da się ją uruchomić
//! i obejrzeć bez serwera poczty.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::time::{Duration, Instant};

use libluna_core::{Point, Rect, ThemeSettings};
use libluna_i18n::{load_catalogs_from_dirs, preferred_locale, set_locale, tr, tr_args};
use libluna_platform::{run as run_app, AppConfig, Frame, WakeHandle};
use libluna_widgets::{
    Entry, Menu, MenuButton, MenuEntry, PasswordEntry, SearchEntry, TextView, WindowControl,
};

use crate::account::{Account, AccountStore, Encryption, Preset};
use crate::backend::{Command, MailWorker, Reply};
use crate::cli::ComposeRequest;
use crate::model::{
    forward_body, forward_subject, initials, now_secs, reply_body, reply_subject, validate_draft,
    AttachmentInfo, Draft, FolderInfo, FolderKind, MessageBody, MessageSummary,
};
use crate::notify;

/// Jak długo widać potwierdzenie akcji w stopce.
pub const TOAST_FOR: Duration = Duration::from_secs(3);

/// Co ile sprawdzamy skrzynkę odbiorczą, gdy aplikacja jest otwarta.
pub const POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Odstęp między kliknięciami liczony jako dwuklik.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// Górny limit łącznego rozmiaru załączników (25 MiB).
///
/// Większe wiadomości i tak odrzuci większość serwerów, a trzymanie ich
/// w pamięci procesu GUI tylko zwiększa ryzyko.
pub const MAX_ATTACHMENTS_BYTES: usize = 25 * 1024 * 1024;

/// Co jest aktualnie na wierzchu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Mail,
    Compose,
    Account,
    About,
}

/// Pole formularza konta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountField {
    Email,
    Display,
    Login,
    Password,
    ImapHost,
    ImapPort,
    SmtpHost,
    SmtpPort,
}

impl AccountField {
    pub const ALL: [AccountField; 8] = [
        AccountField::Email,
        AccountField::Display,
        AccountField::Login,
        AccountField::Password,
        AccountField::ImapHost,
        AccountField::ImapPort,
        AccountField::SmtpHost,
        AccountField::SmtpPort,
    ];

    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    pub fn previous(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        Self::ALL[(index + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Pole edytora wiadomości.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeField {
    To,
    Cc,
    Subject,
    Body,
}

impl ComposeField {
    pub fn next(self) -> Self {
        match self {
            Self::To => Self::Cc,
            Self::Cc => Self::Subject,
            Self::Subject => Self::Body,
            Self::Body => Self::To,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::To => Self::Body,
            Self::Cc => Self::To,
            Self::Subject => Self::Cc,
            Self::Body => Self::Subject,
        }
    }
}

/// Szkic trzymany w UI (pola są widgetami, więc mają własny kursor i zaznaczenie).
pub struct ComposeState {
    pub to: Entry,
    pub cc: Entry,
    pub subject: Entry,
    pub body: TextView,
    pub field: ComposeField,
    pub attachments: Vec<AttachmentInfo>,
    pub in_reply_to: Option<String>,
    /// Błąd walidacji pokazywany pod polami.
    pub error: Option<&'static str>,
    /// Trwa wysyłka — przycisk jest nieaktywny, żeby nie wysłać dwa razy.
    pub sending: bool,
}

impl Default for ComposeState {
    fn default() -> Self {
        Self {
            to: Entry::new(tr("Recipient address")),
            cc: Entry::new(tr("Copy to (optional)")),
            subject: Entry::new(tr("Subject")),
            body: TextView::new(tr("Write your message…")),
            field: ComposeField::To,
            attachments: Vec::new(),
            in_reply_to: None,
            error: None,
            sending: false,
        }
    }
}

impl ComposeState {
    /// Czyści wszystko po wysłaniu albo anulowaniu.
    pub fn reset(&mut self) {
        self.to.clear();
        self.cc.clear();
        self.subject.clear();
        self.body.clear();
        self.attachments.clear();
        self.in_reply_to = None;
        self.error = None;
        self.sending = false;
        self.field = ComposeField::To;
        self.focus();
    }

    /// Ustawia fokus widgetów zgodnie z [`ComposeState::field`].
    pub fn focus(&mut self) {
        self.to.set_focused(self.field == ComposeField::To);
        self.cc.set_focused(self.field == ComposeField::Cc);
        self.subject
            .set_focused(self.field == ComposeField::Subject);
        self.body.set_focused(self.field == ComposeField::Body);
    }

    /// Czy szkic ma jakąkolwiek treść (do decyzji „porzucić?”).
    pub fn is_empty(&self) -> bool {
        self.to.is_empty()
            && self.cc.is_empty()
            && self.subject.is_empty()
            && self.body.is_empty()
            && self.attachments.is_empty()
    }

    /// Łączny rozmiar załączników.
    pub fn attachments_size(&self) -> usize {
        self.attachments.iter().map(|item| item.size).sum()
    }
}

/// Formularz konta (Ustawienia poczty).
pub struct AccountForm {
    pub email: Entry,
    pub display: Entry,
    pub login: Entry,
    pub password: PasswordEntry,
    pub imap_host: Entry,
    pub imap_port: Entry,
    pub smtp_host: Entry,
    pub smtp_port: Entry,
    pub field: AccountField,
    /// Komunikat pod formularzem (błąd albo podpowiedź presetu).
    pub message: Option<String>,
    /// Drugie kliknięcie „Usuń konto” potwierdza operację.
    pub confirm_remove: bool,
}

impl Default for AccountForm {
    fn default() -> Self {
        Self {
            email: Entry::new("ala@example.org"),
            display: Entry::new(tr("Your name")),
            login: Entry::new(tr("Login (usually the address)")),
            password: PasswordEntry::new(tr("Password or app password")),
            imap_host: Entry::new("imap.example.org"),
            imap_port: Entry::new("993"),
            smtp_host: Entry::new("smtp.example.org"),
            smtp_port: Entry::new("465"),
            field: AccountField::Email,
            message: None,
            confirm_remove: false,
        }
    }
}

impl AccountForm {
    pub fn focus(&mut self) {
        self.email.set_focused(self.field == AccountField::Email);
        self.display
            .set_focused(self.field == AccountField::Display);
        self.login.set_focused(self.field == AccountField::Login);
        self.password
            .set_focused(self.field == AccountField::Password);
        self.imap_host
            .set_focused(self.field == AccountField::ImapHost);
        self.imap_port
            .set_focused(self.field == AccountField::ImapPort);
        self.smtp_host
            .set_focused(self.field == AccountField::SmtpHost);
        self.smtp_port
            .set_focused(self.field == AccountField::SmtpPort);
    }

    /// Wypełnia formularz z konta (albo czyści, gdy konto jest puste).
    pub fn load(&mut self, account: Option<&Account>) {
        match account {
            Some(account) => {
                self.email.set_text(account.email.clone());
                self.display.set_text(account.display_name.clone());
                self.login.set_text(account.login.clone());
                self.password.set_text(account.password.clone());
                self.imap_host.set_text(account.imap_host.clone());
                self.imap_port.set_text(account.imap_port.to_string());
                self.smtp_host.set_text(account.smtp_host.clone());
                self.smtp_port.set_text(account.smtp_port.to_string());
            }
            None => {
                self.email.clear();
                self.display.clear();
                self.login.clear();
                self.password.clear();
                self.imap_host.clear();
                self.imap_port.set_text("993");
                self.smtp_host.clear();
                self.smtp_port.set_text("465");
            }
        }
        self.field = AccountField::Email;
        self.message = None;
        self.confirm_remove = false;
        self.focus();
    }

    /// Uzupełnia serwery na podstawie domeny adresu (przycisk „Wykryj”).
    pub fn detect(&mut self) {
        let email = self.email.text().trim();
        if email.is_empty() {
            self.message = Some(tr("Enter your address first").to_owned());
            return;
        }
        let preset = Preset::for_email(email);
        self.imap_host.set_text(preset.imap_host);
        self.imap_port.set_text(preset.imap_port.to_string());
        self.smtp_host.set_text(preset.smtp_host);
        self.smtp_port.set_text(preset.smtp_port.to_string());
        if self.login.is_empty() {
            self.login.set_text(email.to_owned());
        }
        self.message = Some(tr("Server settings detected").to_owned());
    }

    /// Buduje konto z formularza (albo błąd do pokazania).
    pub fn to_account(&self) -> Result<Account, &'static str> {
        let email = self.email.text().trim().to_owned();
        if email.is_empty() || !crate::model::looks_like_address(&email) {
            return Err("Enter a valid e-mail address");
        }
        let imap_host = self.imap_host.text().trim().to_owned();
        let smtp_host = self.smtp_host.text().trim().to_owned();
        if imap_host.is_empty() || smtp_host.is_empty() {
            return Err("Server addresses cannot be empty");
        }
        let imap_port = self
            .imap_port
            .text()
            .trim()
            .parse::<u16>()
            .map_err(|_| "IMAP port must be a number")?;
        let smtp_port = self
            .smtp_port
            .text()
            .trim()
            .parse::<u16>()
            .map_err(|_| "SMTP port must be a number")?;
        let preset = Preset::for_email(&email);
        // Jeśli użytkownik nie zmienił portu względem presetu, zachowujemy
        // jego szyfrowanie (np. 587 = STARTTLS dla Outlooka).
        let imap_encryption = if imap_port == preset.imap_port {
            preset.imap_encryption
        } else if imap_port == 143 {
            Encryption::StartTls
        } else {
            Encryption::Tls
        };
        let smtp_encryption = if smtp_port == preset.smtp_port {
            preset.smtp_encryption
        } else if smtp_port == 587 || smtp_port == 25 {
            Encryption::StartTls
        } else {
            Encryption::Tls
        };
        let login = if self.login.is_empty() {
            email.clone()
        } else {
            self.login.text().trim().to_owned()
        };
        Ok(Account {
            name: email.clone(),
            email,
            display_name: self.display.text().trim().to_owned(),
            imap_host,
            imap_port,
            imap_encryption,
            smtp_host,
            smtp_port,
            smtp_encryption,
            login,
            password: self.password.text().to_owned(),
        })
    }
}

/// Co robi kliknięcie w zarejestrowany obszar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Folder(usize),
    Message(usize),
    Attachment(usize),
    RemoveAttachment(usize),
    ComposeField(ComposeField),
    ComposeSend,
    ComposeCancel,
    ComposeAttach,
    AccountField(AccountField),
    AccountMenu,
    AccountRemove,
    AccountSave,
    AccountCancel,
    AccountDetect,
    PasswordPeek,
    AboutClose,
    SidebarToggle,
    RetryConnect,
    Reply,
    Forward,
    Configure,
}

/// Stan aplikacji.
pub struct App {
    pub store: AccountStore,
    pub worker: Option<MailWorker>,
    /// Brak kont = dane demonstracyjne.
    pub demo: bool,
    pub connecting: bool,
    pub error: Option<String>,

    pub view: View,
    pub menu_button: MenuButton,
    pub menu: Menu,
    pub menu_panel: Option<Rect>,
    /// Menu kont w pasku bocznym.
    pub account_menu_open: bool,
    pub account_menu: Menu,

    /// Id wiadomości z początku skrzynki — punkt odniesienia dla powiadomień.
    pub inbox_baseline: Option<HashSet<String>>,
    /// Kiedy ostatnio sprawdzaliśmy nową pocztę.
    pub last_poll: Option<Instant>,

    pub folders: Vec<FolderInfo>,
    pub folder_index: usize,
    pub messages: Vec<MessageSummary>,
    /// Indeksy wiadomości widocznych po filtrze.
    pub visible: Vec<usize>,
    pub selected: usize,
    pub list_scroll: f64,
    pub list_viewport: f64,

    pub open_id: Option<String>,
    pub open_body: Option<MessageBody>,
    pub body_loading: bool,
    pub body_scroll: f64,
    pub body_viewport: f64,
    /// Szerokość treści edytora (bez paddingu) — do ruchu po wierszach wizualnych.
    pub compose_body_width: f64,

    pub loading: bool,
    pub search: SearchEntry,
    pub search_active: bool,
    pub compose: ComposeState,
    pub account_form: AccountForm,

    pub toast: Option<(String, Instant)>,
    pub hits: Vec<(Hit, Rect)>,
    pub pointer: Option<Point>,
    pub controls_hover: Option<WindowControl>,
    pub sidebar_visible: bool,

    pub pending_close: bool,
    pub pending_minimize: bool,
    pub pending_maximize: bool,
    pub pending_move: bool,
    pub pending_paste: bool,
    pub pending_paste_primary: bool,
    pub pending_copy: Option<String>,

    /// Wczytane treści (klucz `folder/id`) — nie pytamy serwera dwa razy.
    pub cache: HashMap<String, MessageBody>,
    last_click: Option<(usize, Instant)>,
    /// Uchwyt budzenia — ustawiany po pierwszej klatce.
    waker: Option<WakeHandle>,
}

impl App {
    /// Start: konta z pliku albo dane demo.
    pub fn launch() -> Self {
        Self::with_store(AccountStore::load(), None)
    }

    /// Start z gotowym szkicem (mailto / argumenty wiersza poleceń).
    pub fn launch_with(request: ComposeRequest) -> Self {
        Self::with_store(AccountStore::load(), Some(request))
    }

    /// Aplikacja bez czytania konfiguracji — podglądy i testy.
    pub fn demo() -> Self {
        Self::with_store(AccountStore::default(), None)
    }

    fn with_store(store: AccountStore, request: Option<ComposeRequest>) -> Self {
        let demo = store.accounts.is_empty();
        let mut app = Self {
            store,
            worker: None,
            demo,
            connecting: false,
            error: None,
            view: View::Mail,
            menu_button: MenuButton::burger(),
            menu: build_menu(),
            menu_panel: None,
            account_menu_open: false,
            account_menu: Menu::new(Vec::new()),
            inbox_baseline: None,
            last_poll: None,
            folders: Vec::new(),
            folder_index: 0,
            messages: Vec::new(),
            visible: Vec::new(),
            selected: 0,
            list_scroll: 0.0,
            list_viewport: 0.0,
            open_id: None,
            open_body: None,
            body_loading: false,
            body_scroll: 0.0,
            body_viewport: 0.0,
            compose_body_width: 0.0,
            loading: false,
            search: SearchEntry::new(tr("Search messages")),
            search_active: false,
            compose: ComposeState::default(),
            account_form: AccountForm::default(),
            toast: None,
            hits: Vec::new(),
            pointer: None,
            controls_hover: None,
            sidebar_visible: true,
            pending_close: false,
            pending_minimize: false,
            pending_maximize: false,
            pending_move: false,
            pending_paste: false,
            pending_paste_primary: false,
            pending_copy: None,
            cache: HashMap::new(),
            last_click: None,
            waker: None,
        };
        if demo {
            app.load_demo();
        } else {
            app.connect();
        }
        if let Some(request) = request {
            app.apply_compose_request(request);
        }
        app
    }

    /// Aktywne konto.
    pub fn account(&self) -> Option<&Account> {
        self.store.active_account()
    }

    /// Nazwa folderu, który jest otwarty.
    pub fn folder_name(&self) -> String {
        self.folders
            .get(self.folder_index)
            .map(|folder| folder.name.clone())
            .unwrap_or_else(|| tr("Inbox").to_owned())
    }

    pub fn folder_kind(&self) -> FolderKind {
        self.folders
            .get(self.folder_index)
            .map(|folder| folder.kind)
            .unwrap_or(FolderKind::Inbox)
    }

    /// Nazwa folderu „Wysłane” (do zapisu kopii po wysyłce).
    pub fn sent_folder(&self) -> Option<String> {
        self.folders
            .iter()
            .find(|folder| folder.kind == FolderKind::Sent)
            .map(|folder| folder.name.clone())
    }

    /// Nazwa folderu „Kosz” (do usuwania).
    pub fn trash_folder(&self) -> Option<String> {
        self.folders
            .iter()
            .find(|folder| folder.kind == FolderKind::Trash)
            .map(|folder| folder.name.clone())
    }

    /// Nazwa skrzynki odbiorczej (do sprawdzania nowej poczty).
    pub fn inbox_folder(&self) -> String {
        self.folders
            .iter()
            .find(|folder| folder.kind == FolderKind::Inbox)
            .map(|folder| folder.name.clone())
            .unwrap_or_else(|| "INBOX".to_owned())
    }

    /// Lista kont pokazywana po kliknięciu wiersza konta w pasku bocznym.
    pub fn rebuild_account_menu(&mut self) {
        let mut entries = vec![MenuEntry::section(tr("Accounts"))];
        if self.store.accounts.is_empty() {
            entries.push(MenuEntry::action(tr("No accounts yet")).disabled());
        } else {
            for (index, account) in self.store.accounts.iter().enumerate() {
                let label = if index == self.store.active {
                    format!("{}  ({})", account.email, tr("active"))
                } else {
                    account.email.clone()
                };
                entries.push(MenuEntry::action(label));
            }
        }
        entries.push(MenuEntry::separator());
        entries.push(MenuEntry::action(tr("Add account…")));
        entries.push(MenuEntry::action(tr("Account settings")));
        self.account_menu = Menu::new(entries);
    }

    /// Otwiera menu kont (klik w wiersz konta w pasku bocznym).
    pub fn open_account_menu(&mut self) {
        self.rebuild_account_menu();
        // W wąskim oknie pasek jest szufladą — bez tego menu nie miałoby
        // do czego się przyczepić.
        self.sidebar_visible = true;
        self.account_menu_open = true;
    }

    /// Przełącza aktywne konto i podłącza je od nowa.
    pub fn switch_account(&mut self, index: usize) {
        if index >= self.store.accounts.len() || index == self.store.active {
            self.account_menu_open = false;
            return;
        }
        self.store.set_active(index);
        if let Err(error) = self.store.save() {
            eprintln!("luna-email: nie można zapisać aktywnego konta: {error}");
        }
        self.worker = None;
        self.reset_mail_state();
        self.demo = false;
        self.connect();
        self.notify(tr("Account switched"));
        self.account_menu_open = false;
    }

    /// Nowe konto: pusty formularz (zapis doda je do listy).
    pub fn open_account_settings_new(&mut self) {
        self.account_form.load(None);
        self.view = View::Account;
        self.account_menu_open = false;
    }

    /// Usuwa aktywne konto (po potwierdzeniu w UI).
    pub fn remove_active_account(&mut self) {
        if self.store.accounts.is_empty() {
            return;
        }
        self.store.remove(self.store.active);
        if let Err(error) = self.store.save() {
            eprintln!("luna-email: nie można zapisać kont: {error}");
        }
        self.worker = None;
        self.reset_mail_state();
        self.account_menu_open = false;
        self.view = View::Mail;
        self.notify(tr("Account removed"));
        if self.store.accounts.is_empty() {
            self.demo = true;
            self.load_demo();
        } else {
            self.demo = false;
            self.connect();
        }
    }

    /// Czyści wszystko, co zależy od konta (foldery, wiadomości, cache).
    fn reset_mail_state(&mut self) {
        self.folders.clear();
        self.messages.clear();
        self.visible.clear();
        self.cache.clear();
        self.folder_index = 0;
        self.selected = 0;
        self.list_scroll = 0.0;
        self.open_id = None;
        self.open_body = None;
        self.inbox_baseline = None;
        self.last_poll = None;
    }

    /// Wysyła zapytanie o nową pocztę (bez zmiany widoku).
    pub fn request_poll(&mut self) {
        if self.demo || self.worker.is_none() || self.folders.is_empty() {
            return;
        }
        let folder = self.inbox_folder();
        if let Some(worker) = &self.worker {
            worker.send(Command::CheckNew { folder });
        }
        self.last_poll = Some(Instant::now());
    }

    /// Wywoływane raz na klatkę przez pętlę: planuje kolejne sprawdzenie.
    ///
    /// Zwraca czas do następnego sprawdzenia (do `request_frame_after`),
    /// albo `None`, gdy nie ma czego sprawdzać (demo / brak konta).
    pub fn tick_poll(&mut self) -> Option<Duration> {
        if self.demo || self.worker.is_none() || self.folders.is_empty() {
            return None;
        }
        let due = self
            .last_poll
            .map_or(true, |at| at.elapsed() >= POLL_INTERVAL);
        if due {
            self.request_poll();
        }
        Some(
            self.last_poll
                .map(|at| POLL_INTERVAL.saturating_sub(at.elapsed()))
                .unwrap_or(POLL_INTERVAL),
        )
    }

    /// Aktualizuje licznik nieprzeczytanych w pasku folderów.
    fn update_folder_unread(&mut self, folder: &str, items: &[MessageSummary]) {
        let unread = items.iter().filter(|item| !item.seen).count();
        if let Some(entry) = self.folders.iter_mut().find(|entry| entry.name == folder) {
            entry.unread = unread;
        }
    }

    /// Zapisuje nowy punkt odniesienia dla powiadomień (skrzynka odbiorcza).
    fn set_inbox_baseline(&mut self, items: &[MessageSummary]) {
        self.inbox_baseline = Some(items.iter().map(|item| item.id.clone()).collect());
    }

    /// Podłącza wątek sieciowy do aktywnego konta.
    pub fn connect(&mut self) {
        let Some(account) = self.store.active_account().cloned() else {
            return;
        };
        if !account.is_complete() {
            self.error = Some(tr("Account configuration is incomplete").to_owned());
            return;
        }
        self.demo = false;
        self.worker = Some(MailWorker::spawn(account, self.waker.clone()));
        if let Some(worker) = &self.worker {
            worker.send(Command::Connect);
        }
        self.connecting = true;
        self.error = None;
    }

    /// Ponawia połączenie po błędzie.
    pub fn retry(&mut self) {
        self.error = None;
        if self.demo {
            self.view = View::Account;
            self.account_form.load(None);
            self.account_form.focus();
        } else {
            self.connect();
        }
    }

    /// Odbiera odpowiedzi z sieci i wstawia je do stanu.
    pub fn poll(&mut self) {
        let Some(worker) = &self.worker else {
            return;
        };
        for reply in worker.drain() {
            self.apply_reply(reply);
        }
    }

    pub fn apply_reply(&mut self, reply: Reply) {
        match reply {
            Reply::Folders(folders) => {
                self.connecting = false;
                self.loading = false;
                self.folders = folders;
                // Preferujemy skrzynkę odbiorczą, ale nie wymuszamy jej istnienia.
                self.folder_index = self
                    .folders
                    .iter()
                    .position(|folder| folder.kind == FolderKind::Inbox)
                    .unwrap_or(0);
                self.selected = 0;
                self.messages.clear();
                self.visible.clear();
                self.open_id = None;
                self.open_body = None;
                // Nowe konto/sesja: punkt odniesienia ustali pierwsza lista.
                self.inbox_baseline = None;
                self.last_poll = Some(Instant::now());
                self.request_folder();
            }
            Reply::Envelopes {
                folder,
                page,
                items,
            } => {
                self.loading = false;
                if folder != self.folder_name() || page != 0 {
                    return;
                }
                // Każde wczytanie skrzynki odbiorczej jest nowym punktem
                // odniesienia — inaczej po ręcznym odświeżeniu dostalibyśmy
                // powiadomienie o wiadomościach, które już widzieliśmy.
                if folder == self.inbox_folder() {
                    self.set_inbox_baseline(&items);
                    self.update_folder_unread(&folder, &items);
                }
                self.messages = items;
                self.recompute();
                self.selected = 0;
                self.list_scroll = 0.0;
                self.request_body();
            }
            Reply::NewMail { folder, items } => {
                let ids: HashSet<String> = items.iter().map(|item| item.id.clone()).collect();
                let previous = self.inbox_baseline.replace(ids);
                self.update_folder_unread(&folder, &items);
                // Pierwsze sprawdzenie tylko ustala bazę — bez toasta.
                let Some(previous) = previous else {
                    return;
                };
                let new_messages: Vec<&MessageSummary> = items
                    .iter()
                    .filter(|message| !previous.contains(&message.id))
                    .collect();
                if new_messages.is_empty() {
                    return;
                }
                if let Some(newest) = new_messages.iter().max_by_key(|message| message.date) {
                    notify::new_mail(
                        new_messages.len(),
                        Some((newest.sender(), newest.subject.as_str())),
                    );
                }
                let text = if new_messages.len() == 1 {
                    tr("New message")
                } else {
                    tr_args(
                        "{n} new messages",
                        &[("n", &new_messages.len().to_string())],
                    )
                };
                self.notify(text);
                // Gdy patrzymy na skrzynkę, dopisujemy nowe wiadomości bez
                // resetowania zaznaczenia i podglądu.
                if folder == self.folder_name() {
                    let selected_id = self.selected_message_id();
                    let snippets: HashMap<String, String> = self
                        .messages
                        .iter()
                        .filter(|message| !message.snippet.is_empty())
                        .map(|message| (message.id.clone(), message.snippet.clone()))
                        .collect();
                    let mut items = items;
                    for item in &mut items {
                        if item.snippet.is_empty() {
                            if let Some(snippet) = snippets.get(&item.id) {
                                item.snippet = snippet.clone();
                            }
                        }
                    }
                    self.messages = items;
                    self.recompute();
                    if let Some(id) = selected_id {
                        if let Some(position) = self
                            .visible
                            .iter()
                            .position(|index| self.messages[*index].id == id)
                        {
                            self.selected = position;
                        }
                    }
                    self.ensure_visible();
                }
            }
            Reply::Body { id, body } => {
                self.body_loading = false;
                let key = format!("{}/{}", self.folder_name(), id);
                if let Some(summary) = self.messages.iter_mut().find(|item| item.id == id) {
                    summary.snippet = crate::model::snippet(&body.text, 120);
                }
                if self.open_id.as_deref() == Some(id.as_str()) {
                    self.open_body = Some(body.clone());
                    self.body_scroll = 0.0;
                }
                self.cache.insert(key, body);
            }
            Reply::Flags {
                folder,
                id,
                seen,
                flagged,
            } => {
                if folder != self.folder_name() {
                    return;
                }
                if let Some(summary) = self.messages.iter_mut().find(|item| item.id == id) {
                    if let Some(seen) = seen {
                        summary.seen = seen;
                    }
                    if let Some(flagged) = flagged {
                        summary.flagged = flagged;
                    }
                }
            }
            Reply::Moved { folder } => {
                if folder == self.folder_name() {
                    self.open_id = None;
                    self.open_body = None;
                    self.request_folder();
                }
            }
            Reply::Sent { saved_copy } => {
                self.compose.reset();
                self.view = View::Mail;
                self.notify(if saved_copy {
                    tr("Message sent and saved to Sent")
                } else {
                    tr("Message sent")
                });
                self.request_folder();
            }
            Reply::Error { context, message } => {
                self.connecting = false;
                self.loading = false;
                self.body_loading = false;
                self.compose.sending = false;
                let text = match context {
                    "send" => format!("{}: {message}", tr("Could not send")),
                    "message" => format!("{}: {message}", tr("Could not load message")),
                    "folders" | "connect" => format!("{}: {message}", tr("Could not connect")),
                    _ => message.clone(),
                };
                self.notify(text);
                self.error = Some(message);
            }
        }
    }

    /// Ponownie filtruje listę wiadomości.
    pub fn recompute(&mut self) {
        let needle = self.search.text().trim().to_lowercase();
        self.visible = self
            .messages
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                needle.is_empty()
                    || item.subject.to_lowercase().contains(&needle)
                    || item.from_name.to_lowercase().contains(&needle)
                    || item.from_addr.to_lowercase().contains(&needle)
                    || item.snippet.to_lowercase().contains(&needle)
            })
            .map(|(index, _)| index)
            .collect();
        if self.visible.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.visible.len() {
            self.selected = self.visible.len() - 1;
        }
    }

    /// Indeks wiadomości w [`App::messages`] dla bieżącego zaznaczenia.
    pub fn selected_message(&self) -> Option<&MessageSummary> {
        self.visible
            .get(self.selected)
            .and_then(|index| self.messages.get(*index))
    }

    pub fn selected_message_id(&self) -> Option<String> {
        self.selected_message().map(|item| item.id.clone())
    }

    /// Przenosi zaznaczenie i prosi o treść.
    pub fn select(&mut self, visible_index: usize) {
        if self.visible.is_empty() {
            self.selected = 0;
            return;
        }
        self.selected = visible_index.min(self.visible.len() - 1);
        self.ensure_visible();
        self.request_body();
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.visible.is_empty() {
            return;
        }
        let last = self.visible.len() - 1;
        let next = (self.selected as i32 + delta).clamp(0, last as i32);
        self.select(next as usize);
    }

    pub fn select_first(&mut self) {
        self.select(0);
    }

    pub fn select_last(&mut self) {
        if !self.visible.is_empty() {
            let last = self.visible.len() - 1;
            self.select(last);
        }
    }

    /// Pilnuje, żeby zaznaczony wiersz był widoczny.
    pub fn ensure_visible(&mut self) {
        let row = crate::ui::MESSAGE_ROW_HEIGHT;
        let top = self.selected as f64 * row;
        let bottom = top + row;
        if top < self.list_scroll {
            self.list_scroll = top;
        } else if bottom > self.list_scroll + self.list_viewport {
            self.list_scroll = bottom - self.list_viewport;
        }
        self.clamp_list_scroll();
    }

    pub fn clamp_list_scroll(&mut self) {
        let content = self.visible.len() as f64 * crate::ui::MESSAGE_ROW_HEIGHT;
        let max = (content - self.list_viewport).max(0.0);
        self.list_scroll = self.list_scroll.clamp(0.0, max);
    }

    /// Prosi sieć o treść zaznaczonej wiadomości (albo bierze z cache).
    pub fn request_body(&mut self) {
        let Some(id) = self.selected_message_id() else {
            self.open_id = None;
            self.open_body = None;
            return;
        };
        if self.open_id.as_deref() == Some(id.as_str()) {
            return;
        }
        self.open_id = Some(id.clone());
        let key = format!("{}/{}", self.folder_name(), id);
        if let Some(body) = self.cache.get(&key) {
            self.open_body = Some(body.clone());
            self.body_scroll = 0.0;
            self.body_loading = false;
        } else if self.demo {
            self.open_body = None;
            self.body_loading = false;
        } else {
            self.open_body = None;
            self.body_loading = true;
            if let Some(worker) = &self.worker {
                worker.send(Command::FetchBody {
                    folder: self.folder_name(),
                    id,
                });
            }
        }
        // Prezentacja jest równoznaczna z przeczytaniem.
        self.mark_seen(true, false);
    }

    /// Prosi o bieżący folder.
    pub fn request_folder(&mut self) {
        let folder = self.folder_name();
        self.loading = !self.demo;
        if self.demo {
            self.messages.retain(|_| false);
            self.load_demo_messages();
            self.recompute();
            self.selected = 0;
            self.request_body();
            return;
        }
        if let Some(worker) = &self.worker {
            worker.send(Command::ListEnvelopes { folder, page: 0 });
        }
    }

    /// Zmienia folder.
    pub fn select_folder(&mut self, index: usize) {
        if index == self.folder_index || index >= self.folders.len() {
            return;
        }
        self.folder_index = index;
        self.messages.clear();
        self.visible.clear();
        self.selected = 0;
        self.open_id = None;
        self.open_body = None;
        self.list_scroll = 0.0;
        self.request_folder();
    }

    /// Odświeżenie (F5 / przycisk).
    pub fn refresh(&mut self) {
        if self.demo {
            self.request_folder();
            self.notify(tr("Messages refreshed"));
            return;
        }
        if self.worker.is_none() {
            self.connect();
        } else {
            self.request_folder();
            self.notify(tr("Refreshing…"));
        }
    }

    pub fn notify(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    /// Oznacza zaznaczoną wiadomość jako przeczytaną/nieprzeczytaną.
    pub fn mark_seen(&mut self, seen: bool, manual: bool) {
        let Some(id) = self.selected_message_id() else {
            return;
        };
        if let Some(summary) = self.messages.iter_mut().find(|item| item.id == id) {
            if summary.seen == seen {
                return;
            }
            summary.seen = seen;
        }
        if manual {
            self.notify(if seen {
                tr("Marked as read")
            } else {
                tr("Marked as unread")
            });
        }
        if !self.demo {
            if let Some(worker) = &self.worker {
                worker.send(Command::SetSeen {
                    folder: self.folder_name(),
                    id,
                    seen,
                });
            }
        }
    }

    /// Przełącza gwiazdkę (flagę) zaznaczonej wiadomości.
    pub fn toggle_flag(&mut self) {
        let Some(id) = self.selected_message_id() else {
            return;
        };
        let mut flagged = false;
        if let Some(summary) = self.messages.iter_mut().find(|item| item.id == id) {
            summary.flagged = !summary.flagged;
            flagged = summary.flagged;
        }
        if !self.demo {
            if let Some(worker) = &self.worker {
                worker.send(Command::SetFlagged {
                    folder: self.folder_name(),
                    id,
                    flagged,
                });
            }
        }
    }

    /// Przenosi zaznaczoną wiadomość do kosza.
    pub fn delete_selected(&mut self) {
        let Some(id) = self.selected_message_id() else {
            return;
        };
        if self.demo {
            self.messages.retain(|item| item.id != id);
            self.recompute();
            self.open_id = None;
            self.open_body = None;
            self.notify(tr("Message moved to trash"));
            return;
        }
        let dest = self.trash_folder().unwrap_or_default();
        // Nie przenosimy kosza do samego siebie — tam usuwamy na dobre.
        let dest = if self.folder_kind() == FolderKind::Trash {
            String::new()
        } else {
            dest
        };
        if let Some(worker) = &self.worker {
            worker.send(Command::Move {
                folder: self.folder_name(),
                id,
                dest,
            });
        }
        self.notify(tr("Moving message…"));
    }

    /// Otwiera edytor z gotowym szkicem (mailto / argumenty startowe).
    pub fn apply_compose_request(&mut self, request: ComposeRequest) {
        self.compose = ComposeState::default();
        if !request.to.is_empty() {
            self.compose.to.set_text(request.to.join(", "));
        }
        if !request.cc.is_empty() {
            self.compose.cc.set_text(request.cc.join(", "));
        }
        self.compose.subject.set_text(request.subject);
        self.compose.body.set_text(request.body);
        if !request.attachments.is_empty() {
            self.attach_paths(&request.attachments);
        }
        // Kursor tam, gdzie zwykle dopisuje się treść — pole odbiorcy jest
        // już wypełnione przez aplikację wywołującą.
        self.compose.field = if self.compose.body.is_empty() {
            ComposeField::Subject
        } else {
            ComposeField::Body
        };
        self.compose.focus();
        self.view = View::Compose;
    }

    /// Nowa wiadomość od zera.
    pub fn compose_new(&mut self) {
        if self.view != View::Compose {
            self.compose = ComposeState::default();
        }
        self.view = View::Compose;
        self.compose.focus();
    }

    /// Odpowiedź na zaznaczoną wiadomość.
    pub fn compose_reply(&mut self) {
        let Some(message) = self.selected_message().cloned() else {
            self.compose_new();
            return;
        };
        let body = self
            .open_body
            .as_ref()
            .map(|body| body.text.clone())
            .unwrap_or_default();
        let mut compose = ComposeState::default();
        compose.to.set_text(message.from_addr.clone());
        compose.subject.set_text(reply_subject(&message.subject));
        compose
            .body
            .set_text(reply_body(message.sender(), message.date, &body));
        compose.in_reply_to = Some(message.id);
        compose.field = ComposeField::Body;
        compose.focus();
        self.compose = compose;
        self.view = View::Compose;
    }

    /// Przekazanie zaznaczonej wiadomości.
    pub fn compose_forward(&mut self) {
        let Some(message) = self.selected_message().cloned() else {
            self.compose_new();
            return;
        };
        let body = self
            .open_body
            .as_ref()
            .map(|body| body.text.clone())
            .unwrap_or_default();
        let mut compose = ComposeState::default();
        compose.subject.set_text(forward_subject(&message.subject));
        compose.body.set_text(forward_body(
            message.sender(),
            message.date,
            &message.subject,
            &body,
        ));
        // Przekazanie zaczyna nowy wątek — bez `In-Reply-To`.
        compose.in_reply_to = None;
        compose.field = ComposeField::To;
        compose.focus();
        self.compose = compose;
        self.view = View::Compose;
    }

    /// Zamyka edytor, zostawiając szkic w pamięci (ponowne otwarcie go odtworzy).
    pub fn close_compose(&mut self) {
        self.view = View::Mail;
        self.compose.error = None;
    }

    /// Czy bieżący widok przyjmuje tekst z klawiatury (dla wklejania PRIMARY).
    pub fn wants_text_input(&self) -> bool {
        match self.view {
            View::Compose | View::Account => true,
            View::Mail => self.search.is_focused(),
            View::About => false,
        }
    }

    /// Dodaje pliki jako załączniki (DnD z menedżera plików).
    pub fn attach_paths(&mut self, paths: &[std::path::PathBuf]) {
        let mut total = self.compose.attachments_size();
        let mut added = 0;
        for path in paths {
            if total >= MAX_ATTACHMENTS_BYTES {
                self.compose.error = Some("Attachment limit is 25 MB");
                break;
            }
            let Ok(metadata) = std::fs::metadata(path) else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            let size = metadata.len() as usize;
            if size > MAX_ATTACHMENTS_BYTES.saturating_sub(total) {
                self.compose.error = Some("Attachment limit is 25 MB");
                break;
            }
            let Ok(data) = std::fs::read(path) else {
                continue;
            };
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("załącznik")
                .to_owned();
            let mime = mime_from_name(&name);
            self.compose.attachments.push(AttachmentInfo {
                name,
                mime,
                size: data.len(),
                data,
            });
            total += size;
            added += 1;
        }
        if added > 0 {
            self.notify(format!("{} {}", added, tr("attachments added")));
        }
    }

    /// Waliduje i wysyła szkic.
    pub fn send_compose(&mut self) {
        if self.compose.sending {
            return;
        }
        let draft = Draft {
            to: self.compose.to.text().to_owned(),
            cc: self.compose.cc.text().to_owned(),
            subject: self.compose.subject.text().to_owned(),
            body: self.compose.body.text().to_owned(),
            attachments: self.compose.attachments.clone(),
            in_reply_to: self.compose.in_reply_to.clone(),
            reply_to: None,
        };
        let recipients = match validate_draft(&draft) {
            Ok(recipients) => recipients,
            Err(error) => {
                self.compose.error = Some(error.message());
                return;
            }
        };
        if self.demo {
            self.compose.error = Some("Configure an account to send mail");
            self.view = View::Account;
            self.account_form.load(None);
            self.account_form.focus();
            return;
        }
        let Some(account) = self.store.active_account() else {
            self.compose.error = Some("Configure an account to send mail");
            return;
        };
        let raw = match build_raw_message(account, &draft, &recipients) {
            Ok(raw) => raw,
            Err(message) => {
                self.compose.error = None;
                self.notify(message);
                return;
            }
        };
        self.compose.sending = true;
        self.compose.error = None;
        if let Some(worker) = &self.worker {
            worker.send(Command::Send {
                raw,
                save_to: self.sent_folder(),
            });
            self.notify(tr("Sending…"));
        }
    }

    /// Zapisuje formularz konta i podłącza się ponownie.
    pub fn save_account(&mut self) {
        let account = match self.account_form.to_account() {
            Ok(account) => account,
            Err(message) => {
                self.account_form.message = Some(tr(message).to_owned());
                return;
            }
        };
        self.store.upsert(account);
        if let Err(error) = self.store.save() {
            self.account_form.message = Some(format!("{}: {error}", tr("Could not save")));
            return;
        }
        self.account_form.message = Some(tr("Account saved").to_owned());
        self.account_form.confirm_remove = false;
        self.demo = false;
        self.view = View::Mail;
        self.worker = None;
        self.reset_mail_state();
        self.connect();
    }

    /// Otwiera ustawienia konta.
    pub fn open_account_settings(&mut self) {
        let account = self.store.active_account().cloned();
        self.account_form.load(account.as_ref());
        self.view = View::Account;
        self.account_menu_open = false;
    }

    /// Wprowadza tekst do aktywnego pola.
    pub fn insert_text(&mut self, text: &str) {
        match self.view {
            View::Compose => match self.compose.field {
                ComposeField::To => self.compose.to.insert_str(text),
                ComposeField::Cc => self.compose.cc.insert_str(text),
                ComposeField::Subject => self.compose.subject.insert_str(text),
                ComposeField::Body => self.compose.body.insert_str(text),
            },
            View::Account => self.account_insert(text),
            View::Mail | View::About => {
                if self.search.is_focused() {
                    self.search.insert_str(text);
                    self.recompute();
                }
            }
        }
    }

    /// Wstawia znak (wariant dla `KeyEvent::text` bez wielolinijkowości).
    pub fn insert_char(&mut self, ch: char) {
        let mut buffer = [0u8; 4];
        self.insert_text(ch.encode_utf8(&mut buffer));
    }

    pub fn backspace(&mut self) {
        match self.view {
            View::Compose => match self.compose.field {
                ComposeField::To => self.compose.to.backspace(),
                ComposeField::Cc => self.compose.cc.backspace(),
                ComposeField::Subject => self.compose.subject.backspace(),
                ComposeField::Body => self.compose.body.backspace(),
            },
            View::Account => self.account_backspace(),
            View::Mail | View::About => {
                if self.search.is_focused() {
                    self.search.backspace();
                    self.recompute();
                }
            }
        }
    }

    pub fn delete_forward(&mut self) {
        match self.view {
            View::Compose => match self.compose.field {
                ComposeField::To => self.compose.to.delete_forward(),
                ComposeField::Cc => self.compose.cc.delete_forward(),
                ComposeField::Subject => self.compose.subject.delete_forward(),
                ComposeField::Body => self.compose.body.delete_forward(),
            },
            View::Account => {
                // Wygodny skrót: Delete w polu działa jak Backspace na końcu.
                self.account_backspace();
            }
            View::Mail | View::About => {
                if self.search.is_focused() {
                    self.search.delete_forward();
                    self.recompute();
                }
            }
        }
    }

    /// Ruch kursora; `vertical` tylko dla treści wiadomości.
    pub fn move_cursor(&mut self, dx: i32, dy: i32, shift: bool) {
        if self.view != View::Compose && !self.search.is_focused() {
            return;
        }
        let (left, right, up, down, home, end) = match self.view {
            View::Compose => match self.compose.field {
                ComposeField::To => (
                    EntryCursor::To,
                    EntryCursor::To,
                    EntryCursor::To,
                    EntryCursor::To,
                    EntryCursor::To,
                    EntryCursor::To,
                ),
                ComposeField::Cc => (
                    EntryCursor::Cc,
                    EntryCursor::Cc,
                    EntryCursor::Cc,
                    EntryCursor::Cc,
                    EntryCursor::Cc,
                    EntryCursor::Cc,
                ),
                ComposeField::Subject => (
                    EntryCursor::Subject,
                    EntryCursor::Subject,
                    EntryCursor::Subject,
                    EntryCursor::Subject,
                    EntryCursor::Subject,
                    EntryCursor::Subject,
                ),
                ComposeField::Body => (
                    EntryCursor::Body,
                    EntryCursor::Body,
                    EntryCursor::Body,
                    EntryCursor::Body,
                    EntryCursor::Body,
                    EntryCursor::Body,
                ),
            },
            _ => (
                EntryCursor::Search,
                EntryCursor::Search,
                EntryCursor::Search,
                EntryCursor::Search,
                EntryCursor::Search,
                EntryCursor::Search,
            ),
        };
        if dx < 0 {
            self.cursor_op(left, CursorOp::Left, shift);
        } else if dx > 0 {
            self.cursor_op(right, CursorOp::Right, shift);
        } else if dy < 0 {
            self.cursor_op(up, CursorOp::Up, shift);
        } else if dy > 0 {
            self.cursor_op(down, CursorOp::Down, shift);
        }
        // `home`/`end` wołane są osobnymi metodami.
        let _ = (home, end);
    }

    pub fn cursor_home(&mut self, shift: bool) {
        self.cursor_op(self.current_cursor(), CursorOp::Home, shift);
    }

    pub fn cursor_end(&mut self, shift: bool) {
        self.cursor_op(self.current_cursor(), CursorOp::End, shift);
    }

    fn current_cursor(&self) -> EntryCursor {
        match self.view {
            View::Compose => match self.compose.field {
                ComposeField::To => EntryCursor::To,
                ComposeField::Cc => EntryCursor::Cc,
                ComposeField::Subject => EntryCursor::Subject,
                ComposeField::Body => EntryCursor::Body,
            },
            _ => EntryCursor::Search,
        }
    }

    fn cursor_op(&mut self, target: EntryCursor, op: CursorOp, shift: bool) {
        match target {
            EntryCursor::Search => match op {
                CursorOp::Left => self.search.move_left_selecting(shift),
                CursorOp::Right => self.search.move_right_selecting(shift),
                CursorOp::Home => self.search.home_selecting(shift),
                CursorOp::End => self.search.end_selecting(shift),
                CursorOp::Up | CursorOp::Down => {}
            },
            EntryCursor::To => cursor_entry(&mut self.compose.to, op),
            EntryCursor::Cc => cursor_entry(&mut self.compose.cc, op),
            EntryCursor::Subject => cursor_entry(&mut self.compose.subject, op),
            EntryCursor::Body => match op {
                CursorOp::Left => self.compose.body.move_left_selecting(shift),
                CursorOp::Right => self.compose.body.move_right_selecting(shift),
                CursorOp::Up => self.compose.body.move_up_selecting(shift),
                CursorOp::Down => self.compose.body.move_down_selecting(shift),
                CursorOp::Home => self.compose.body.home_selecting(shift),
                CursorOp::End => self.compose.body.end_selecting(shift),
            },
        }
    }

    /// Zaznacza wszystko w aktywnym polu.
    pub fn select_all(&mut self) {
        match self.view {
            View::Compose => match self.compose.field {
                ComposeField::To => self.compose.to.select_all(),
                ComposeField::Cc => self.compose.cc.select_all(),
                ComposeField::Subject => self.compose.subject.select_all(),
                ComposeField::Body => self.compose.body.select_all(),
            },
            View::Account => self.account_select_all(),
            View::Mail | View::About => self.search.select_all(),
        }
    }

    /// Kopiuje zaznaczenie do schowka; zwraca tekst do `Frame::copy_to_clipboard`.
    pub fn copy(&mut self) -> Option<String> {
        match self.view {
            View::Compose => match self.compose.field {
                ComposeField::To => self.compose.to.copy(),
                ComposeField::Cc => self.compose.cc.copy(),
                ComposeField::Subject => self.compose.subject.copy(),
                ComposeField::Body => self.compose.body.copy(),
            },
            View::Account => self.account_copy(),
            View::Mail | View::About => self.search.copy(),
        }
    }

    /// Wycina zaznaczenie (tylko pola edycyjne).
    pub fn cut(&mut self) -> Option<String> {
        match self.view {
            View::Compose => match self.compose.field {
                ComposeField::To => self.compose.to.cut(),
                ComposeField::Cc => self.compose.cc.cut(),
                ComposeField::Subject => self.compose.subject.cut(),
                ComposeField::Body => self.compose.body.cut(),
            },
            View::Account => self.account_cut(),
            View::Mail | View::About => self.search.cut().inspect(|_| self.recompute()),
        }
    }

    /// Wkleja tekst do aktywnego pola; `true` = obsłużone lokalnie.
    pub fn paste_text(&mut self, text: &str) -> bool {
        match self.view {
            View::Compose => {
                match self.compose.field {
                    ComposeField::To => self.compose.to.paste(text),
                    ComposeField::Cc => self.compose.cc.paste(text),
                    ComposeField::Subject => self.compose.subject.paste(text),
                    ComposeField::Body => self.compose.body.paste(text),
                }
                true
            }
            View::Account => {
                self.account_paste(text);
                true
            }
            View::Mail | View::About => {
                if self.search.is_focused() {
                    self.search.paste(text);
                    self.recompute();
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Przechodzi do następnego pola formularza.
    pub fn focus_next(&mut self) {
        match self.view {
            View::Compose => {
                self.compose.field = self.compose.field.next();
                self.compose.focus();
            }
            View::Account => {
                self.account_form.field = self.account_form.field.next();
                self.account_form.focus();
            }
            _ => {}
        }
    }

    pub fn focus_previous(&mut self) {
        match self.view {
            View::Compose => {
                self.compose.field = self.compose.field.previous();
                self.compose.focus();
            }
            View::Account => {
                self.account_form.field = self.account_form.field.previous();
                self.account_form.focus();
            }
            _ => {}
        }
    }

    /// Oznacza kliknięcie; `true`, gdy to dwuklik na tym samym wierszu.
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

    /// Ustawia wątek budzenia po pierwszej klatce.
    pub fn set_waker(&mut self, waker: WakeHandle) {
        if self.waker.is_some() {
            return;
        }
        self.waker = Some(waker.clone());
        if let Some(worker) = &self.worker {
            worker.set_waker(waker);
        }
    }

    // ——— formularz konta: skróty ———

    fn account_insert(&mut self, text: &str) {
        match self.account_form.field {
            AccountField::Email => self.account_form.email.insert_str(text),
            AccountField::Display => self.account_form.display.insert_str(text),
            AccountField::Login => self.account_form.login.insert_str(text),
            AccountField::Password => self.account_form.password.insert_str(text),
            AccountField::ImapHost => self.account_form.imap_host.insert_str(text),
            AccountField::ImapPort => self.account_form.imap_port.insert_str(text),
            AccountField::SmtpHost => self.account_form.smtp_host.insert_str(text),
            AccountField::SmtpPort => self.account_form.smtp_port.insert_str(text),
        }
    }

    fn account_backspace(&mut self) {
        match self.account_form.field {
            AccountField::Email => self.account_form.email.backspace(),
            AccountField::Display => self.account_form.display.backspace(),
            AccountField::Login => self.account_form.login.backspace(),
            AccountField::Password => self.account_form.password.backspace(),
            AccountField::ImapHost => self.account_form.imap_host.backspace(),
            AccountField::ImapPort => self.account_form.imap_port.backspace(),
            AccountField::SmtpHost => self.account_form.smtp_host.backspace(),
            AccountField::SmtpPort => self.account_form.smtp_port.backspace(),
        }
    }

    fn account_select_all(&mut self) {
        match self.account_form.field {
            AccountField::Email => self.account_form.email.select_all(),
            AccountField::Display => self.account_form.display.select_all(),
            AccountField::Login => self.account_form.login.select_all(),
            AccountField::Password => self.account_form.password.select_all(),
            AccountField::ImapHost => self.account_form.imap_host.select_all(),
            AccountField::ImapPort => self.account_form.imap_port.select_all(),
            AccountField::SmtpHost => self.account_form.smtp_host.select_all(),
            AccountField::SmtpPort => self.account_form.smtp_port.select_all(),
        }
    }

    fn account_copy(&mut self) -> Option<String> {
        match self.account_form.field {
            AccountField::Email => self.account_form.email.copy(),
            AccountField::Display => self.account_form.display.copy(),
            AccountField::Login => self.account_form.login.copy(),
            AccountField::Password => self.account_form.password.copy(),
            AccountField::ImapHost => self.account_form.imap_host.copy(),
            AccountField::ImapPort => self.account_form.imap_port.copy(),
            AccountField::SmtpHost => self.account_form.smtp_host.copy(),
            AccountField::SmtpPort => self.account_form.smtp_port.copy(),
        }
    }

    fn account_cut(&mut self) -> Option<String> {
        match self.account_form.field {
            AccountField::Email => self.account_form.email.cut(),
            AccountField::Display => self.account_form.display.cut(),
            AccountField::Login => self.account_form.login.cut(),
            AccountField::Password => self.account_form.password.cut(),
            AccountField::ImapHost => self.account_form.imap_host.cut(),
            AccountField::ImapPort => self.account_form.imap_port.cut(),
            AccountField::SmtpHost => self.account_form.smtp_host.cut(),
            AccountField::SmtpPort => self.account_form.smtp_port.cut(),
        }
    }

    fn account_paste(&mut self, text: &str) {
        match self.account_form.field {
            AccountField::Email => self.account_form.email.paste(text),
            AccountField::Display => self.account_form.display.paste(text),
            AccountField::Login => self.account_form.login.paste(text),
            AccountField::Password => self.account_form.password.paste(text),
            AccountField::ImapHost => self.account_form.imap_host.paste(text),
            AccountField::ImapPort => self.account_form.imap_port.paste(text),
            AccountField::SmtpHost => self.account_form.smtp_host.paste(text),
            AccountField::SmtpPort => self.account_form.smtp_port.paste(text),
        }
    }

    // ——— dane demonstracyjne ———

    /// Foldery i wiadomości demo (bez sieci).
    pub fn load_demo(&mut self) {
        self.demo = true;
        self.connecting = false;
        self.folders = vec![
            FolderInfo {
                name: tr("Inbox").to_owned(),
                kind: FolderKind::Inbox,
                unread: 3,
                total: 0,
            },
            FolderInfo {
                name: tr("Starred").to_owned(),
                kind: FolderKind::Custom,
                unread: 0,
                total: 0,
            },
            FolderInfo {
                name: tr("Sent").to_owned(),
                kind: FolderKind::Sent,
                unread: 0,
                total: 0,
            },
            FolderInfo {
                name: tr("Drafts").to_owned(),
                kind: FolderKind::Drafts,
                unread: 0,
                total: 0,
            },
            FolderInfo {
                name: tr("Archive").to_owned(),
                kind: FolderKind::Archive,
                unread: 0,
                total: 0,
            },
            FolderInfo {
                name: tr("Trash").to_owned(),
                kind: FolderKind::Trash,
                unread: 0,
                total: 0,
            },
        ];
        self.folder_index = 0;
        self.request_folder();
    }

    fn load_demo_messages(&mut self) {
        let now = now_secs();
        let mut items: Vec<MessageSummary> = match self.folder_kind() {
            FolderKind::Sent => demo_sent(now),
            FolderKind::Drafts => demo_drafts(now),
            FolderKind::Trash => demo_trash(now),
            FolderKind::Archive => demo_archive(now),
            FolderKind::Custom => demo_starred(now),
            FolderKind::Inbox => demo_inbox(now),
        };
        items.sort_by(|a, b| b.date.cmp(&a.date));
        self.messages = items;
        // Treści demo trzymamy w cache, więc podgląd działa natychmiast.
        for message in self.messages.clone() {
            let key = format!("{}/{}", self.folder_name(), message.id);
            if let Some(body) = demo_body(&message.id, now) {
                self.cache.insert(key, body);
            }
        }
    }
}

/// Gdzie jest kursor klawiatury (do skrótów edycyjnych).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryCursor {
    Search,
    To,
    Cc,
    Subject,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CursorOp {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

fn cursor_entry(entry: &mut Entry, op: CursorOp) {
    match op {
        CursorOp::Left => entry.move_left(),
        CursorOp::Right => entry.move_right(),
        CursorOp::Home => entry.home(),
        CursorOp::End => entry.end(),
        CursorOp::Up | CursorOp::Down => {}
    }
}

/// Buduje surową wiadomość RFC 822.
fn build_raw_message(
    account: &Account,
    draft: &Draft,
    recipients: &[String],
) -> Result<Vec<u8>, String> {
    use mail_builder::headers::address::Address;
    use mail_builder::headers::date::Date;
    use mail_builder::MessageBuilder;

    let display = if account.display_name.trim().is_empty() {
        account.email.clone()
    } else {
        account.display_name.clone()
    };
    let from = Address::new_address(Some(display), account.email.clone());
    let to = Address::new_list(
        recipients
            .iter()
            .map(|address| Address::from(address.clone()))
            .collect(),
    );
    let cc_addresses = crate::model::parse_addresses(&draft.cc);
    let mut builder = MessageBuilder::new()
        .from(from)
        .to(to)
        .subject(draft.subject.clone())
        .date(Date::now())
        .text_body(draft.body.clone());
    if !cc_addresses.is_empty() {
        builder = builder.cc(Address::new_list(
            cc_addresses.into_iter().map(Address::from).collect(),
        ));
    }
    if let Some(parent) = &draft.in_reply_to {
        if !parent.is_empty() {
            builder = builder.in_reply_to(parent.clone());
        }
    }
    for attachment in &draft.attachments {
        builder = builder.attachment(
            attachment.mime.clone(),
            attachment.name.clone(),
            attachment.data.clone(),
        );
    }
    builder
        .write_to_vec()
        .map_err(|error| format!("{}: {error}", tr("Could not build message")))
}

/// Typ MIME z rozszerzenia (wystarczający dla załączników).
fn mime_from_name(name: &str) -> String {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "txt" | "md" => "text/plain",
        "html" | "htm" => "text/html",
        "csv" => "text/csv",
        "json" => "application/json",
        "xml" => "application/xml",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "mp4" => "video/mp4",
        "odt" => "application/vnd.oasis.opendocument.text",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        _ => "application/octet-stream",
    }
    .to_owned()
}

fn build_menu() -> Menu {
    Menu::new(vec![
        MenuEntry::section(tr("Message")),
        MenuEntry::action(tr("New message")).shortcut("Ctrl+N"),
        MenuEntry::action(tr("Reply")).shortcut("Ctrl+R"),
        MenuEntry::action(tr("Forward")).shortcut("Ctrl+Shift+F"),
        MenuEntry::separator(),
        MenuEntry::section(tr("View")),
        MenuEntry::action(tr("Refresh")).shortcut("F5"),
        MenuEntry::action(tr("Mark as read")).shortcut("Ctrl+Shift+R"),
        MenuEntry::action(tr("Mark as unread")).shortcut("Ctrl+Shift+U"),
        MenuEntry::action(tr("Toggle star")).shortcut("S"),
        MenuEntry::action(tr("Delete message")).shortcut("Del"),
        MenuEntry::separator(),
        MenuEntry::section(tr("Luna")),
        MenuEntry::action(tr("Switch account")),
        MenuEntry::action(tr("Add account…")),
        MenuEntry::action(tr("Account settings")),
        MenuEntry::action(tr("About Luna Mail")),
        MenuEntry::action(tr("Quit")).shortcut("Ctrl+Q"),
    ])
}

/// Pętla aplikacji.
pub fn run(request: Option<ComposeRequest>) -> Result<(), Box<dyn Error>> {
    set_locale(preferred_locale());
    load_catalogs_from_dirs();

    let mut app = match request {
        Some(request) => App::launch_with(request),
        None => App::launch(),
    };
    let settings = ThemeSettings::load();
    let system_dark = libluna_core::appearance::system_dark();
    let config = AppConfig::new(tr("Luna Mail"))
        .app_id("eu.luna.Email")
        .initial_size(1080, 720)
        .settings(settings)
        .system_dark(system_dark)
        .escape_to_exit(false);

    let mut last_title = String::new();
    run_app(config, move |frame: &mut Frame| {
        // Budzik sieci działa dopiero, gdy znamy uchwyt pętli.
        app.set_waker(frame.waker());

        let theme = frame.settings().resolve_themed(frame.system_dark());
        app.poll();
        crate::ui::handle_events(&mut app, frame.events, frame.size, &theme, frame.text);

        let title = match app.view {
            View::Compose => tr("New message").to_owned(),
            View::Account => tr("Account settings").to_owned(),
            View::About => tr("About Luna Mail").to_owned(),
            View::Mail => {
                let unread = app.messages.iter().filter(|item| !item.seen).count();
                crate::model::window_title(&app.folder_name(), unread)
            }
        };
        if title != last_title {
            frame.set_title(title.clone());
            last_title = title;
        }

        crate::ui::paint(&mut app, frame.scene, frame.size, &theme, frame.text);

        if app.pending_paste {
            app.pending_paste = false;
            frame.paste_from_clipboard();
        }
        if app.pending_paste_primary {
            app.pending_paste_primary = false;
            frame.paste_from_primary();
        }
        if let Some(text) = app.pending_copy.take() {
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

        // Toast znika sam.
        if let Some((_, since)) = &app.toast {
            if since.elapsed() >= TOAST_FOR {
                app.toast = None;
                frame.request_frame();
            } else {
                frame.request_frame_after(TOAST_FOR - since.elapsed());
            }
        }
        // Gdy czekamy na sieć, odświeżamy się regularnie (wątek może nie mieć
        // jeszcze uchwytu budzenia).
        if app.connecting || app.loading || app.body_loading {
            frame.request_frame_after(Duration::from_millis(120));
        }
        // Nowa poczta: sprawdzamy skrzynkę co minutę, dopóki okno jest otwarte.
        if let Some(remaining) = app.tick_poll() {
            frame.request_frame_after(remaining.max(Duration::from_secs(1)));
        }
    })
}

/// Zapisuje załącznik do katalogu Pobrane (z unikalną nazwą).
pub fn save_attachment(attachment: &AttachmentInfo) -> Result<std::path::PathBuf, String> {
    let dir = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("Downloads");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let mut path = dir.join(&attachment.name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("zalacznik")
        .to_owned();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    let mut counter = 1;
    while path.exists() {
        path = dir.join(format!("{stem}-{counter}{extension}"));
        counter += 1;
    }
    std::fs::write(&path, &attachment.data).map_err(|error| error.to_string())?;
    Ok(path)
}

// ——— dane demo ———

fn demo_message(
    id: &str,
    from_name: &str,
    from_addr: &str,
    subject: &str,
    date: i64,
    seen: bool,
    flagged: bool,
    attachment: bool,
) -> MessageSummary {
    MessageSummary {
        id: id.to_owned(),
        from_name: from_name.to_owned(),
        from_addr: from_addr.to_owned(),
        to_name: String::new(),
        to_addr: "ty@example.org".to_owned(),
        subject: subject.to_owned(),
        date,
        seen,
        flagged,
        answered: false,
        has_attachment: attachment,
        snippet: String::new(),
    }
}

fn demo_inbox(now: i64) -> Vec<MessageSummary> {
    vec![
        demo_message(
            "demo-1",
            "Zespół PaganOS",
            "zespol@paganlinux.org",
            "Wydanie 7.3.4 — co nowego w Lunie",
            now - 12 * 60,
            false,
            true,
            false,
        ),
        demo_message(
            "demo-2",
            "Marta Kowalska",
            "marta@example.org",
            "Projekt graficzny na piątek",
            now - 3 * 3_600,
            false,
            false,
            true,
        ),
        demo_message(
            "demo-3",
            "GitHub",
            "noreply@github.com",
            "[luna] Pull request #128: klient poczty",
            now - 6 * 3_600,
            false,
            false,
            false,
        ),
        demo_message(
            "demo-4",
            "Poczta Polska",
            "powiadomienia@poczta-polska.pl",
            "Twoja przesyłka jest w drodze",
            now - 22 * 3_600,
            true,
            false,
            false,
        ),
        demo_message(
            "demo-5",
            "Tomasz Nowak",
            "tomasz@example.org",
            "Re: Spotkanie zespołu — zmiana godziny",
            now - 2 * 86_400,
            true,
            false,
            false,
        ),
        demo_message(
            "demo-6",
            "Newsletter Rust",
            "hello@rust-lang.org",
            "Rust 1.86: async closures i nowości w cargo",
            now - 4 * 86_400,
            true,
            true,
            false,
        ),
        demo_message(
            "demo-7",
            "Anna Wiśniewska",
            "anna@example.org",
            "Faktury za październik",
            now - 9 * 86_400,
            true,
            false,
            true,
        ),
    ]
}

fn demo_starred(now: i64) -> Vec<MessageSummary> {
    let mut items = demo_inbox(now);
    items.retain(|item| item.flagged);
    items
}

fn demo_sent(now: i64) -> Vec<MessageSummary> {
    vec![
        demo_message(
            "sent-1",
            "Ja",
            "ty@example.org",
            "Re: Projekt graficzny na piątek",
            now - 2 * 3_600,
            true,
            false,
            true,
        ),
        demo_message(
            "sent-2",
            "Ja",
            "ty@example.org",
            "Faktury — potwierdzenie",
            now - 8 * 86_400,
            true,
            false,
            false,
        ),
    ]
}

fn demo_drafts(_now: i64) -> Vec<MessageSummary> {
    Vec::new()
}

fn demo_trash(_now: i64) -> Vec<MessageSummary> {
    Vec::new()
}

fn demo_archive(now: i64) -> Vec<MessageSummary> {
    vec![demo_message(
        "arch-1",
        "Konferencja",
        "info@example.org",
        "Materiały z konferencji",
        now - 40 * 86_400,
        true,
        false,
        true,
    )]
}

fn demo_body(id: &str, now: i64) -> Option<MessageBody> {
    let text = match id {
        "demo-1" => format!(
            "Cześć!\n\nWłaśnie ukazało się wydanie PaganOS 7.3.4. Najważniejsze zmiany:\n\n\
             - Luna Mail — nowy klient poczty z IMAP/SMTP i podglądem wiadomości\n\
             - szybszy panel i poprawki w kompozytorze Selene\n\
             - nowe skróty klawiaturowe w Ustawieniach\n\n\
             Pełna lista zmian jest w dokumentacji. Miłego testowania!\n\n\
             — Zespół PaganOS"
        ),
        "demo-2" => format!(
            "Hej,\n\nw załączniku wrzucam najnowszą wersję projektu. Zmieniłam paletę \
             na chłodniejszą i dodałam wariant ciemny.\n\nDaj znać, czy pasuje.\n\nMarta"
        ),
        "demo-3" => format!(
            "Nowy pull request w repozytorium luna:\n\n\
             #128 — klient poczty na bazie email-lib\n\n\
             - IMAP: foldery, lista wiadomości, podgląd\n\
             - SMTP: wysyłka i kopia w „Wysłanych”\n\
             - obsługa załączników i tryb demo\n\n\
             Zobacz zmiany w przeglądarce GitHuba."
        ),
        "demo-4" => format!(
            "Dzień dobry,\n\ninformujemy, że Twoja przesyłka została przyjęta do doręczenia. \
             Przewidywany termin dostawy: jutro do godziny 18:00.\n\n\
             Numer przesyłki: PL1234567890."
        ),
        "demo-5" => format!(
            "Cześć,\n\nprzesuwamy spotkanie z 14:00 na 15:30 — sala bez zmian.\n\n\
             > Czy everyone pasuje?\n\nTomek"
        ),
        "demo-6" => format!(
            "Rust 1.86 jest już dostępny!\n\n\
             W tym wydaniu: stabilizacja async closures, szybsze kompilacje \
             przyrostowe oraz nowe flagi w cargo.\n\nZespół Rust"
        ),
        "demo-7" => format!(
            "Dzień dobry,\n\nw załączniku faktury za październik. Proszę o potwierdzenie \
             otrzymania.\n\nPozdrawiam,\nAnna Wiśniewska"
        ),
        "sent-1" => format!(
            "Cześć Marto,\n\nprojekt wygląda świetnie — wariant ciemny jest dużo lepszy. \
             Wysyłam uwagi do ikon w załączniku.\n\nDzięki!"
        ),
        "sent-2" => format!("Dzień dobry,\n\npotwierdzam otrzymanie faktur. Wszystko się zgadza."),
        "arch-1" => format!(
            "Dziękujemy za udział w konferencji. W załączniku znajdziesz slajdy \
             i nagrania z sesji."
        ),
        _ => return None,
    };
    let attachments = if id == "demo-2" {
        vec![AttachmentInfo {
            name: "projekt-luna.png".to_owned(),
            mime: "image/png".to_owned(),
            size: 184_320,
            data: Vec::new(),
        }]
    } else if id == "demo-1" {
        vec![AttachmentInfo {
            name: "changelog-7.3.4.txt".to_owned(),
            mime: "text/plain".to_owned(),
            size: 4_096,
            data: Vec::new(),
        }]
    } else {
        Vec::new()
    };
    let _ = now;
    Some(MessageBody { text, attachments })
}

/// Pomoc dla UI: inicjały nadawcy.
pub fn sender_initials(message: &MessageSummary) -> String {
    let source = if message.from_name.trim().is_empty() {
        &message.from_addr
    } else {
        &message.from_name
    };
    initials(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_demo() -> App {
        // `App::demo()` nie czyta `~/.config` i nie uruchamia wątku sieciowego.
        let mut app = App::demo();
        app.load_demo();
        app
    }

    #[test]
    fn demo_mode_fills_inbox_and_preview() {
        let app = app_with_demo();
        assert!(app.demo);
        assert!(!app.visible.is_empty());
        assert!(app.open_body.is_some());
        assert_eq!(app.folder_kind(), FolderKind::Inbox);
    }

    #[test]
    fn search_filters_by_sender_subject_and_snippet() {
        let mut app = app_with_demo();
        app.search.set_text("marta");
        app.recompute();
        assert_eq!(app.visible.len(), 1);
        assert_eq!(app.messages[app.visible[0]].from_name, "Marta Kowalska");
        app.search.set_text("pull request");
        app.recompute();
        assert_eq!(app.visible.len(), 1);
        app.search.clear();
        app.recompute();
        assert!(app.visible.len() >= 5);
    }

    #[test]
    fn selecting_a_message_updates_preview_and_marks_it_read() {
        let mut app = app_with_demo();
        app.select(1);
        assert!(app.open_body.is_some());
        assert!(app.messages[app.visible[1]].seen);
    }

    #[test]
    fn switching_folders_reloads_messages() {
        let mut app = app_with_demo();
        app.select_folder(2); // Wysłane
        assert_eq!(app.folder_kind(), FolderKind::Sent);
        assert_eq!(app.messages.len(), 2);
    }

    #[test]
    fn reply_prefills_recipient_and_quotes_body() {
        let mut app = app_with_demo();
        app.select(0);
        app.compose_reply();
        assert_eq!(app.view, View::Compose);
        assert_eq!(app.compose.to.text(), "zespol@paganlinux.org");
        assert!(app.compose.subject.text().starts_with("Re:"));
        assert!(app.compose.body.text().contains("> "));
    }

    #[test]
    fn forward_prefills_subject_and_original_header() {
        let mut app = app_with_demo();
        app.select(0);
        app.compose_forward();
        assert!(app.compose.subject.text().starts_with("Fwd:"));
        assert!(app.compose.body.text().contains("Przekazana wiadomość"));
    }

    #[test]
    fn send_in_demo_mode_redirects_to_account_form() {
        let mut app = app_with_demo();
        app.compose_new();
        app.compose.to.set_text("ala@example.org");
        app.compose.subject.set_text("Test");
        app.send_compose();
        assert_eq!(app.view, View::Account);
        assert!(app.compose.error.is_some());
    }

    #[test]
    fn invalid_recipient_blocks_sending() {
        let mut app = app_with_demo();
        app.compose_new();
        app.compose.to.set_text("niepoprawny");
        app.send_compose();
        assert_eq!(
            app.compose.error,
            Some("One of the addresses looks invalid")
        );
    }

    #[test]
    fn deleting_in_demo_removes_the_row() {
        let mut app = app_with_demo();
        let before = app.messages.len();
        app.delete_selected();
        assert_eq!(app.messages.len(), before - 1);
    }

    #[test]
    fn tabs_cycle_compose_fields() {
        let mut app = app_with_demo();
        app.compose_new();
        assert_eq!(app.compose.field, ComposeField::To);
        app.focus_next();
        assert_eq!(app.compose.field, ComposeField::Cc);
        app.focus_previous();
        assert_eq!(app.compose.field, ComposeField::To);
    }

    #[test]
    fn account_form_detects_google_settings() {
        let mut form = AccountForm::default();
        form.email.set_text("ktos@gmail.com");
        form.detect();
        assert_eq!(form.imap_host.text(), "imap.gmail.com");
        assert_eq!(form.smtp_port.text(), "465");
    }

    #[test]
    fn account_form_rejects_incomplete_data() {
        let form = AccountForm::default();
        assert!(form.to_account().is_err());
    }

    #[test]
    fn compose_request_opens_a_prefilled_form() {
        let mut app = App::demo();
        app.apply_compose_request(ComposeRequest {
            to: vec!["luna@mijsys.pl".to_owned()],
            cc: vec!["kopia@example.org".to_owned()],
            subject: "[luna-report] Luna".to_owned(),
            body: "Opis problemu".to_owned(),
            attachments: Vec::new(),
        });
        assert_eq!(app.view, View::Compose);
        assert_eq!(app.compose.to.text(), "luna@mijsys.pl");
        assert_eq!(app.compose.cc.text(), "kopia@example.org");
        assert_eq!(app.compose.subject.text(), "[luna-report] Luna");
        assert_eq!(app.compose.body.text(), "Opis problemu");
        assert_eq!(app.compose.field, ComposeField::Body);
    }

    #[test]
    fn new_mail_starts_with_a_silent_baseline_then_notifies() {
        let mut app = app_with_demo();
        app.inbox_baseline = None;
        let first = MessageSummary {
            id: "1".to_owned(),
            subject: "Pierwsza".to_owned(),
            seen: false,
            ..MessageSummary::default()
        };
        // Pierwsze sprawdzenie tylko ustala punkt odniesienia.
        app.apply_reply(Reply::NewMail {
            folder: app.folder_name(),
            items: vec![first.clone()],
        });
        assert!(app.toast.is_none());
        assert_eq!(app.inbox_baseline.as_ref().map(HashSet::len), Some(1));

        let second = MessageSummary {
            id: "2".to_owned(),
            from_name: "Marta".to_owned(),
            subject: "Druga".to_owned(),
            date: first.date + 10,
            seen: false,
            ..MessageSummary::default()
        };
        app.apply_reply(Reply::NewMail {
            folder: app.folder_name(),
            items: vec![second, first],
        });
        assert!(app.toast.is_some());
        assert_eq!(app.inbox_baseline.as_ref().map(HashSet::len), Some(2));
        // Licznik nieprzeczytanych w pasku folderów też się aktualizuje.
        let inbox = app
            .folders
            .iter()
            .find(|folder| folder.kind == FolderKind::Inbox)
            .expect("skrzynka");
        assert_eq!(inbox.unread, 2);
    }

    #[test]
    fn account_menu_lists_accounts_and_actions() {
        let mut app = App::demo();
        app.store
            .accounts
            .push(Account::from_email("ala@example.org"));
        app.store
            .accounts
            .push(Account::from_email("bob@example.org"));
        app.store.set_active(0);
        app.rebuild_account_menu();
        // sekcja + 2 konta + separator + „Dodaj konto” + „Ustawienia konta”
        assert_eq!(app.account_menu.entries().len(), 6);
    }

    #[test]
    fn mime_type_comes_from_extension() {
        assert_eq!(mime_from_name("raport.pdf"), "application/pdf");
        assert_eq!(mime_from_name("zdjecie.PNG"), "image/png");
        assert_eq!(
            mime_from_name("bez-rozszerzenia"),
            "application/octet-stream"
        );
    }

    #[test]
    fn raw_message_contains_headers_and_body() {
        let account = Account::from_email("ala@example.org");
        let draft = Draft {
            to: "bob@example.org".to_owned(),
            subject: "Temat".to_owned(),
            body: "Treść wiadomości".to_owned(),
            ..Draft::default()
        };
        let raw = build_raw_message(&account, &draft, &["bob@example.org".to_owned()])
            .expect("wiadomość");
        let text = String::from_utf8_lossy(&raw);
        assert!(text.contains("bob@example.org"));
        assert!(text.contains("Subject: Temat"));
        assert!(text.contains("Content-Type: text/plain"));
        // Treść jest zakodowana (base64), ale sekcja ciała nie może być pusta.
        assert!(text
            .split("\r\n\r\n")
            .nth(1)
            .is_some_and(|body| !body.trim().is_empty()));
    }
}
