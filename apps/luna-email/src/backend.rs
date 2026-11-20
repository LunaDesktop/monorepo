//! Warstwa sieciowa: IMAP/SMTP w osobnym wątku.
//!
//! Rysowanie nie może czekać na serwer, a `email-lib` jest asynchroniczny.
//! Dlatego UI wysyła [`Command`] kanałem MPSC, wątek roboczy trzyma runtime
//! Tokio i odsyła gotowe [`Reply`]. Każde odświeżenie robi tylko
//! `try_recv()` — nigdy `recv()`.

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use email::backend::{Backend, BackendBuilder};
use email::envelope::flag::add::AddFlags;
use email::envelope::flag::remove::RemoveFlags;
use email::envelope::flag::Flag;
use email::envelope::list::{ListEnvelopes, ListEnvelopesOptions};
use email::envelope::Id;
use email::folder::list::ListFolders;
use email::imap::config::{ImapAuthConfig, ImapConfig};
use email::imap::{ImapContext, ImapContextBuilder};
use email::message::add::AddMessage;
use email::message::peek::PeekMessages;
use email::message::r#move::MoveMessages;
use email::message::send::SendMessage;
use email::smtp::config::{SmtpAuthConfig, SmtpConfig};
use email::smtp::SmtpContextBuilder;
use email::tls::{Encryption as TlsEncryption, Tls};
use email::{account::config::passwd::PasswordConfig, account::config::AccountConfig};
use libluna_platform::WakeHandle;
use secret::Secret;

use crate::account::{Account, Encryption};
use crate::model::{snippet, AttachmentInfo, FolderInfo, FolderKind, MessageBody, MessageSummary};

/// Ile wiadomości pobieramy na stronę listy.
pub const PAGE_SIZE: usize = 50;

/// Co UI każe zrobić sieci.
#[derive(Debug, Clone)]
pub enum Command {
    /// Połącz się i pobierz foldery (+ pierwszą stronę skrzynki odbiorczej).
    Connect,
    ListEnvelopes {
        folder: String,
        page: usize,
    },
    /// Sprawdza tylko początek skrzynki (bez zmiany widoku) — do powiadomień.
    CheckNew {
        folder: String,
    },
    FetchBody {
        folder: String,
        id: String,
    },
    SetSeen {
        folder: String,
        id: String,
        seen: bool,
    },
    SetFlagged {
        folder: String,
        id: String,
        flagged: bool,
    },
    /// Przenosi wiadomość; `dest` puste = usuń trwale.
    Move {
        folder: String,
        id: String,
        dest: String,
    },
    /// Wysyła gotowy RFC 822 i (jeśli znamy folder) zapisuje kopię.
    Send {
        raw: Vec<u8>,
        save_to: Option<String>,
    },
}

/// Co sieć odsyła do UI.
#[derive(Debug, Clone)]
pub enum Reply {
    Folders(Vec<FolderInfo>),
    Envelopes {
        folder: String,
        page: usize,
        items: Vec<MessageSummary>,
    },
    /// Wynik okresowego sprawdzania nowej poczty (bez podmiany listy).
    NewMail {
        folder: String,
        items: Vec<MessageSummary>,
    },
    Body {
        id: String,
        body: MessageBody,
    },
    Flags {
        folder: String,
        id: String,
        seen: Option<bool>,
        flagged: Option<bool>,
    },
    /// Wiadomość przeniesiona/usunięta — lista wymaga odświeżenia.
    Moved {
        folder: String,
    },
    Sent {
        saved_copy: bool,
    },
    Error {
        context: &'static str,
        message: String,
    },
}

/// Uchwyt UI do wątku roboczego.
pub struct MailWorker {
    commands: Sender<Command>,
    replies: Receiver<Reply>,
    /// Budzik pętli — ustawiany po pierwszej klatce, gdy znamy `WakeHandle`.
    wake_slot: Arc<Mutex<Option<WakeHandle>>>,
    thread: Option<JoinHandle<()>>,
}

impl MailWorker {
    /// Uruchamia wątek. `waker` budzi pętlę rysowania, gdy przyjdzie odpowiedź.
    pub fn spawn(account: Account, waker: Option<WakeHandle>) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (reply_tx, reply_rx) = mpsc::channel();
        let wake_slot = Arc::new(Mutex::new(waker));
        let thread_wake_slot = wake_slot.clone();
        let thread = thread::Builder::new()
            .name("luna-email-net".to_owned())
            .spawn(move || {
                let wake = |slot: &Arc<Mutex<Option<WakeHandle>>>| {
                    if let Some(waker) = slot.lock().ok().and_then(|guard| guard.clone()) {
                        waker.wake();
                    }
                };
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = reply_tx.send(Reply::Error {
                            context: "connect",
                            message: error.to_string(),
                        });
                        wake(&thread_wake_slot);
                        return;
                    }
                };
                let mut session = Session::new(account);
                while let Ok(command) = command_rx.recv() {
                    let reply = runtime.block_on(session.handle(command));
                    if let Some(reply) = reply {
                        let _ = reply_tx.send(reply);
                    }
                    // Budzimy nawet po błędzie — UI ma szansę pokazać komunikat.
                    wake(&thread_wake_slot);
                }
            })
            .ok();
        Self {
            commands: command_tx,
            replies: reply_rx,
            wake_slot,
            thread,
        }
    }

    /// Wysyła polecenie; porzucone UI nie jest błędem.
    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Podaje wątekowi budzik pętli (po pierwszej klatce).
    pub fn set_waker(&self, waker: WakeHandle) {
        if let Ok(mut slot) = self.wake_slot.lock() {
            *slot = Some(waker);
        }
    }

    /// Odbiera wszystkie gotowe odpowiedzi (bez blokowania).
    pub fn drain(&self) -> Vec<Reply> {
        let mut replies = Vec::new();
        loop {
            match self.replies.try_recv() {
                Ok(reply) => replies.push(reply),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        replies
    }
}

impl Drop for MailWorker {
    fn drop(&mut self) {
        // Zamknięcie kanału (upuszczenie `commands`) kończy pętlę `recv()`.
        if let Some(thread) = self.thread.take() {
            // Nie czekamy na sieć — wątek zakończy się po zamknięciu kanału.
            drop(thread);
        }
    }
}

/// Stan połączenia w wątku roboczym.
struct Session {
    account: Account,
    imap: Option<Backend<ImapContext>>,
    smtp: Option<Backend<email::smtp::SmtpContextSync>>,
}

impl Session {
    fn new(account: Account) -> Self {
        Self {
            account,
            imap: None,
            smtp: None,
        }
    }

    async fn handle(&mut self, command: Command) -> Option<Reply> {
        match command {
            Command::Connect => self.connect().await,
            Command::ListEnvelopes { folder, page } => {
                Some(self.list_envelopes(&folder, page).await)
            }
            Command::CheckNew { folder } => self.check_new(&folder).await,
            Command::FetchBody { folder, id } => Some(self.fetch_body(&folder, &id).await),
            Command::SetSeen { folder, id, seen } => Some(self.set_seen(&folder, &id, seen).await),
            Command::SetFlagged {
                folder,
                id,
                flagged,
            } => Some(self.set_flagged(&folder, &id, flagged).await),
            Command::Move { folder, id, dest } => {
                Some(self.move_message(&folder, &id, &dest).await)
            }
            Command::Send { raw, save_to } => Some(self.send(&raw, save_to.as_deref()).await),
        }
    }

    /// Buduje backend IMAP raz; kolejne komendy używają tego samego połączenia.
    async fn imap(&mut self) -> Result<&Backend<ImapContext>, String> {
        if self.imap.is_none() {
            let account_config = Arc::new(account_config(&self.account));
            let imap_config = Arc::new(imap_config(&self.account));
            let context = ImapContextBuilder::new(account_config.clone(), imap_config);
            let backend = BackendBuilder::new(account_config, context)
                .build()
                .await
                .map_err(|error| error.to_string())?;
            self.imap = Some(backend);
        }
        // `is_none()` wykluczyliśmy powyżej, więc `as_ref` jest bezpieczne.
        self.imap
            .as_ref()
            .ok_or_else(|| "IMAP nie został zbudowany".to_owned())
    }

    async fn smtp(&mut self) -> Result<&Backend<email::smtp::SmtpContextSync>, String> {
        if self.smtp.is_none() {
            let account_config = Arc::new(account_config(&self.account));
            let smtp_config = Arc::new(smtp_config(&self.account));
            let context = SmtpContextBuilder::new(account_config.clone(), smtp_config);
            let backend = BackendBuilder::new(account_config, context)
                .build()
                .await
                .map_err(|error| error.to_string())?;
            self.smtp = Some(backend);
        }
        self.smtp
            .as_ref()
            .ok_or_else(|| "SMTP nie został zbudowany".to_owned())
    }

    async fn connect(&mut self) -> Option<Reply> {
        let folders = match self.imap().await {
            Ok(backend) => match backend.list_folders().await {
                Ok(folders) => folders
                    .into_iter()
                    .map(|folder| FolderInfo {
                        kind: FolderKind::detect(&folder.name),
                        name: folder.name,
                        unread: 0,
                        total: 0,
                    })
                    .collect::<Vec<_>>(),
                Err(error) => {
                    return Some(Reply::Error {
                        context: "folders",
                        message: error.to_string(),
                    })
                }
            },
            Err(message) => {
                return Some(Reply::Error {
                    context: "connect",
                    message,
                })
            }
        };

        Some(Reply::Folders(folders))
    }

    async fn list_envelopes(&mut self, folder: &str, page: usize) -> Reply {
        let backend = match self.imap().await {
            Ok(backend) => backend,
            Err(message) => {
                return Reply::Error {
                    context: "list",
                    message,
                }
            }
        };
        let options = ListEnvelopesOptions {
            page_size: PAGE_SIZE,
            page,
            query: None,
        };
        match backend.list_envelopes(folder, options).await {
            Ok(envelopes) => {
                let items = summaries(envelopes.into_iter());
                Reply::Envelopes {
                    folder: folder.to_owned(),
                    page,
                    items,
                }
            }
            Err(error) => Reply::Error {
                context: "list",
                message: error.to_string(),
            },
        }
    }

    /// Sprawdza początek skrzynki bez zmiany tego, co użytkownik widzi.
    ///
    /// Błąd jest tylko logowany — gdy sieć chwilowo padnie, nie chcemy co minutę
    /// pokazywać toasta o błędzie połączenia.
    async fn check_new(&mut self, folder: &str) -> Option<Reply> {
        let backend = match self.imap().await {
            Ok(backend) => backend,
            Err(message) => {
                eprintln!("luna-email: sprawdzanie poczty: {message}");
                return None;
            }
        };
        let options = ListEnvelopesOptions {
            page_size: PAGE_SIZE,
            page: 0,
            query: None,
        };
        match backend.list_envelopes(folder, options).await {
            Ok(envelopes) => Some(Reply::NewMail {
                folder: folder.to_owned(),
                items: summaries(envelopes.into_iter()),
            }),
            Err(error) => {
                eprintln!("luna-email: sprawdzanie poczty: {error}");
                None
            }
        }
    }

    async fn fetch_body(&mut self, folder: &str, id: &str) -> Reply {
        let backend = match self.imap().await {
            Ok(backend) => backend,
            Err(message) => {
                return Reply::Error {
                    context: "message",
                    message,
                }
            }
        };
        let message_id = Id::single(id.to_owned());
        let messages = match backend.peek_messages(folder, &message_id).await {
            Ok(messages) => messages,
            Err(error) => {
                return Reply::Error {
                    context: "message",
                    message: error.to_string(),
                }
            }
        };
        let Some(message) = messages.first() else {
            return Reply::Error {
                context: "message",
                message: "Wiadomość jest pusta".to_owned(),
            };
        };

        let text = match message.parsed() {
            Ok(parsed) => parsed
                .body_text(0)
                .map(|text| text.into_owned())
                .or_else(|| parsed.body_html(0).map(|html| html_to_text(&html)))
                .unwrap_or_default(),
            Err(error) => {
                return Reply::Error {
                    context: "message",
                    message: error.to_string(),
                }
            }
        };

        let attachments = match message.attachments() {
            Ok(attachments) => attachments
                .into_iter()
                .map(|attachment| AttachmentInfo {
                    name: attachment
                        .filename
                        .unwrap_or_else(|| "załącznik".to_owned()),
                    mime: attachment.mime,
                    size: attachment.body.len(),
                    data: attachment.body,
                })
                .collect(),
            Err(_) => Vec::new(),
        };

        Reply::Body {
            id: id.to_owned(),
            body: MessageBody { text, attachments },
        }
    }

    async fn set_seen(&mut self, folder: &str, id: &str, seen: bool) -> Reply {
        let backend = match self.imap().await {
            Ok(backend) => backend,
            Err(message) => {
                return Reply::Error {
                    context: "flags",
                    message,
                }
            }
        };
        let message_id = Id::single(id.to_owned());
        let result = if seen {
            backend.add_flag(folder, &message_id, Flag::Seen).await
        } else {
            backend.remove_flag(folder, &message_id, Flag::Seen).await
        };
        match result {
            Ok(()) => Reply::Flags {
                folder: folder.to_owned(),
                id: id.to_owned(),
                seen: Some(seen),
                flagged: None,
            },
            Err(error) => Reply::Error {
                context: "flags",
                message: error.to_string(),
            },
        }
    }

    async fn set_flagged(&mut self, folder: &str, id: &str, flagged: bool) -> Reply {
        let backend = match self.imap().await {
            Ok(backend) => backend,
            Err(message) => {
                return Reply::Error {
                    context: "flags",
                    message,
                }
            }
        };
        let message_id = Id::single(id.to_owned());
        let result = if flagged {
            backend.add_flag(folder, &message_id, Flag::Flagged).await
        } else {
            backend
                .remove_flag(folder, &message_id, Flag::Flagged)
                .await
        };
        match result {
            Ok(()) => Reply::Flags {
                folder: folder.to_owned(),
                id: id.to_owned(),
                seen: None,
                flagged: Some(flagged),
            },
            Err(error) => Reply::Error {
                context: "flags",
                message: error.to_string(),
            },
        }
    }

    async fn move_message(&mut self, folder: &str, id: &str, dest: &str) -> Reply {
        let backend = match self.imap().await {
            Ok(backend) => backend,
            Err(message) => {
                return Reply::Error {
                    context: "move",
                    message,
                }
            }
        };
        let message_id = Id::single(id.to_owned());
        let result = if dest.is_empty() {
            backend.add_flag(folder, &message_id, Flag::Deleted).await
        } else {
            backend.move_messages(folder, dest, &message_id).await
        };
        match result {
            // Po operacji odświeżamy bieżący folder — UI zrobi to samo.
            Ok(()) => Reply::Moved {
                folder: folder.to_owned(),
            },
            Err(error) => Reply::Error {
                context: "move",
                message: error.to_string(),
            },
        }
    }

    async fn send(&mut self, raw: &[u8], save_to: Option<&str>) -> Reply {
        let smtp = match self.smtp().await {
            Ok(smtp) => smtp,
            Err(message) => {
                return Reply::Error {
                    context: "send",
                    message,
                }
            }
        };
        if let Err(error) = smtp.send_message(raw).await {
            return Reply::Error {
                context: "send",
                message: error.to_string(),
            };
        }
        // Kopia w „Wysłanych” jest wygodą, nie warunkiem wysłania.
        let mut saved_copy = false;
        if let Some(folder) = save_to.filter(|name| !name.is_empty()) {
            if let Ok(imap) = self.imap().await {
                if imap
                    .add_message_with_flag(folder, raw, Flag::Seen)
                    .await
                    .is_ok()
                {
                    saved_copy = true;
                }
            }
        }
        Reply::Sent { saved_copy }
    }
}

/// Wspólny `AccountConfig` dla IMAP i SMTP.
fn account_config(account: &Account) -> AccountConfig {
    AccountConfig {
        name: account.name.clone(),
        email: account.email.clone(),
        display_name: if account.display_name.trim().is_empty() {
            None
        } else {
            Some(account.display_name.clone())
        },
        ..Default::default()
    }
}

fn tls_encryption(encryption: Encryption) -> Option<TlsEncryption> {
    match encryption {
        Encryption::Tls => Some(TlsEncryption::Tls(Tls::default())),
        Encryption::StartTls => Some(TlsEncryption::StartTls(Tls::default())),
        Encryption::None => Some(TlsEncryption::None),
    }
}

fn imap_config(account: &Account) -> ImapConfig {
    ImapConfig {
        host: account.imap_host.clone(),
        port: account.imap_port,
        encryption: tls_encryption(account.imap_encryption),
        login: account.login.clone(),
        auth: ImapAuthConfig::Password(PasswordConfig(Secret::new_raw(
            account.resolved_password(),
        ))),
        ..Default::default()
    }
}

fn smtp_config(account: &Account) -> SmtpConfig {
    SmtpConfig {
        host: account.smtp_host.clone(),
        port: account.smtp_port,
        encryption: tls_encryption(account.smtp_encryption),
        login: account.login.clone(),
        auth: SmtpAuthConfig::Password(PasswordConfig(Secret::new_raw(
            account.resolved_password(),
        ))),
    }
}

/// Zamienia koperty IMAP na własne podsumowania (najnowsze na górze).
fn summaries(envelopes: impl Iterator<Item = email::envelope::Envelope>) -> Vec<MessageSummary> {
    let mut items: Vec<MessageSummary> = envelopes
        .map(|envelope| MessageSummary {
            id: envelope.id,
            from_name: envelope.from.name.unwrap_or_default(),
            from_addr: envelope.from.addr,
            to_name: envelope.to.name.unwrap_or_default(),
            to_addr: envelope.to.addr,
            subject: envelope.subject,
            date: envelope.date.timestamp(),
            seen: envelope.flags.contains(&Flag::Seen),
            flagged: envelope.flags.contains(&Flag::Flagged),
            answered: envelope.flags.contains(&Flag::Answered),
            has_attachment: envelope.has_attachment,
            snippet: String::new(),
        })
        .collect();
    // IMAP domyślnie zwraca od najstarszych; w kliencie chcemy najnowsze
    // na górze, niezależnie od sortowania serwera.
    items.sort_by(|a, b| b.date.cmp(&a.date));
    items
}

/// Zamienia HTML na czytelny tekst, bez zewnętrznego parsera.
///
/// To nie jest renderer: usuwamy znaczniki, zostawiamy łamanie akapitów.
/// Zdalne obrazy i skrypty i tak nie są pobierane — HTML nigdy nie idzie do UI.
fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let lower = html.to_lowercase();
    let mut index = 0;
    // Znaczniki, których zawartość całkowicie pomijamy (skrypty, style, nagłówek).
    let mut skip_until: Option<&'static str> = None;

    while index < html.len() {
        if let Some(end_tag) = skip_until {
            match lower[index..].find(end_tag) {
                Some(offset) => {
                    index += offset + end_tag.len();
                    skip_until = None;
                }
                None => break,
            }
            continue;
        }

        let rest = &html[index..];
        if rest.starts_with('<') {
            let end = rest
                .find('>')
                .map(|offset| offset + 1)
                .unwrap_or(rest.len());
            let tag = &lower[index..index + end];
            if tag.starts_with("<script") {
                skip_until = Some("</script>");
            } else if tag.starts_with("<style") {
                skip_until = Some("</style>");
            } else if tag.starts_with("<head") {
                skip_until = Some("</head>");
            } else if tag.starts_with("<br")
                || tag.starts_with("</p")
                || tag.starts_with("</div")
                || tag.starts_with("</li")
                || tag.starts_with("</tr")
                || tag.starts_with("</h1")
                || tag.starts_with("</h2")
                || tag.starts_with("</h3")
            {
                out.push('\n');
            }
            index += end;
            continue;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        if ch == '&' {
            // Encje zamieniamy tylko najczęstsze; resztę zostawiamy jak jest.
            let mut replaced = false;
            for (entity, replacement) in [
                ("&nbsp;", ' '),
                ("&amp;", '&'),
                ("&lt;", '<'),
                ("&gt;", '>'),
                ("&quot;", '"'),
                ("&#39;", '\''),
            ] {
                if rest.starts_with(entity) {
                    out.push(replacement);
                    index += entity.len();
                    replaced = true;
                    break;
                }
            }
            if replaced {
                continue;
            }
        }
        out.push(ch);
        index += ch.len_utf8();
    }

    // Sprzątanie: maksymalnie dwie puste linie pod rząd.
    let mut cleaned = String::with_capacity(out.len());
    let mut blank_run = 0;
    for line in out.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 2 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        cleaned.push_str(line);
        cleaned.push('\n');
    }
    cleaned.trim().to_owned()
}

/// Skraca treść do jednej linii listy (dla UI po pobraniu wiadomości).
pub fn summary_snippet(body: &str) -> String {
    snippet(body, 120)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_to_text_keeps_paragraphs_and_drops_scripts() {
        let html = "<html><head><style>p{color:red}</style></head><body>\
            <p>Cześć <b>Aniu</b>,</p><p>to jest test.</p>\
            <script>alert(1)</script></body></html>";
        let text = html_to_text(html);
        assert!(text.contains("Cześć Aniu,"));
        assert!(text.contains("to jest test."));
        assert!(!text.contains("alert"));
        assert!(!text.contains("color"));
        assert!(!text.contains('<'));
    }

    #[test]
    fn html_entities_are_decoded() {
        let text = html_to_text("<p>a &amp; b &lt; c &nbsp; d</p>");
        assert!(text.contains("a & b < c"));
    }

    #[test]
    fn summary_snippet_stays_on_one_line() {
        let text = "pierwsza linia\ndruga linia";
        assert_eq!(summary_snippet(text), "pierwsza linia druga linia");
    }
}
