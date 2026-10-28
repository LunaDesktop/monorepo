//! Klient protokołu **greetd** (ramki: 4-bajtowa długość + JSON).
//!
//! Greetd rozdziela uprzywilejowany backend (PAM, logind, uruchomienie sesji) od
//! nieuprzywilejowanego greetera. Frontend wysyła żądania i odbiera odpowiedzi
//! w formacie z `greetd-ipc(7)`: `u32` długości payloadu w kolejności native,
//! a zaraz po nim UTF-8 JSON. **Nie** jest to JSON linia-po-linii — taki błąd
//! powodował, że greetd zamykał połączenie przy pierwszym żądaniu.
//!
//! Komunikacja odbywa się w wątku roboczym, żeby UI nigdy nie blokowało się na
//! odczycie z gniazda. Wątek budzi pętlę Waylanda przez [`WakeHandle`].

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use libluna_platform::WakeHandle;
use serde::Serialize;
use serde_json::Value;

/// Standardowa ścieżka gniazda greetd (używana, gdy brak `GREETD_SOCK`).
pub const DEFAULT_SOCKET: &str = "/run/greetd-ipc";

/// Typ komunikatu autoryzacji PAM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthKind {
    /// Prompt widoczny (np. nazwa użytkownika); odpowiedź ma być jawna.
    Visible,
    /// Prompt ukryty (hasło, PIN); odpowiedź nie może być pokazywana.
    Secret,
    /// Komunikat informacyjny — nie wymaga odpowiedzi.
    Info,
    /// Komunikat błędu PAM — nie wymaga odpowiedzi.
    Error,
}

/// Żądanie wysyłane przez greeter do daemona.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Rozpoczyna sesję PAM dla użytkownika.
    Begin { username: String },
    /// Odpowiada na ostatni prompt (`auth_message`).
    Respond { response: String },
    /// Uruchamia sesję po udanym uwierzytelnieniu.
    Start { cmd: Vec<String>, env: Vec<String> },
    /// Anuluje bieżącą próbę logowania.
    Cancel,
}

/// Zdarzenie zwracane do UI przez wątek roboczy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Połączono z gniazdem greetd.
    Connected,
    /// Daemon prosi o odpowiedź albo przekazuje komunikat.
    Prompt { kind: AuthKind, message: String },
    /// Uwierzytelnienie się powiodło; można wysłać [`Command::Start`].
    AuthOk,
    /// Sesja została uruchomiona — greeter może się zakończyć.
    Started,
    /// Błąd zgłoszony przez greetd (np. `auth_error`).
    ServerError {
        error_type: String,
        description: String,
    },
    /// Gniazdo zostało zamknięte albo połączenie się nie udało.
    Disconnected { message: String },
}

/// Ścieżka gniazda: `GREETD_SOCK` (ustawiane przez greetd) albo standardowa.
pub fn socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("GREETD_SOCK") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    for candidate in [DEFAULT_SOCKET, "/run/greetd/socket"] {
        if Path::new(candidate).exists() {
            return PathBuf::from(candidate);
        }
    }
    PathBuf::from(DEFAULT_SOCKET)
}

/// Polecenie sesji: `LUNA_GREETER_SESSION` (podzielone po białych znakach) albo
/// `luna-session`. Domyślną ścieżkę rozwiązujemy do absolutnej z typowych
/// katalogów — greetd bywa uruchamiany z minimalnym `PATH`, a instalator Luny
/// domyślnie kładzie binarki w `/usr/local/bin`.
pub fn session_command() -> Vec<String> {
    if let Ok(value) = std::env::var("LUNA_GREETER_SESSION") {
        let parts: Vec<String> = value.split_whitespace().map(str::to_owned).collect();
        if !parts.is_empty() {
            return parts;
        }
    }
    for dir in [
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/local/sbin",
        "/usr/sbin",
        "/sbin",
    ] {
        let path = Path::new(dir).join("luna-session");
        if path.is_file() {
            return vec![path.display().to_string()];
        }
    }
    vec!["luna-session".to_owned()]
}

// ── Protokół ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Request {
    CreateSession { username: String },
    PostAuthMessageResponse { response: Option<String> },
    StartSession { cmd: Vec<String>, env: Vec<String> },
    CancelSession,
}

#[derive(Debug)]
enum Response {
    AuthMessage {
        kind: AuthKind,
        message: String,
    },
    Success,
    Error {
        error_type: String,
        description: String,
    },
}

struct Client {
    stream: UnixStream,
}

/// Maksymalny rozmiar jednej ramki — chroni przed alokacją "na ślepo" po
/// uszkodzonym nagłówku długości.
const MAX_FRAME: usize = 1024 * 1024;

impl Client {
    fn connect(path: &Path) -> std::io::Result<Self> {
        let stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(Duration::from_secs(120)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        Ok(Self { stream })
    }

    /// Wysyła ramkę `greetd-ipc`: `u32` długości (native endian) + JSON.
    fn send(&mut self, request: &Request) -> std::io::Result<()> {
        let payload = serde_json::to_vec(request)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let len = u32::try_from(payload.len()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "ramka greetd za duża")
        })?;
        self.stream.write_all(&len.to_ne_bytes())?;
        self.stream.write_all(&payload)?;
        self.stream.flush()
    }

    /// Czyta ramkę `greetd-ipc` i parsuje JSON.
    fn read(&mut self) -> std::io::Result<Response> {
        let mut header = [0u8; 4];
        if let Err(error) = self.stream.read_exact(&mut header) {
            return Err(frame_eof(error));
        }
        let len = u32::from_ne_bytes(header) as usize;
        if len == 0 || len > MAX_FRAME {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("nieprawidłowa długość ramki greetd: {len}"),
            ));
        }
        let mut payload = vec![0u8; len];
        if let Err(error) = self.stream.read_exact(&mut payload) {
            return Err(frame_eof(error));
        }
        let text = String::from_utf8(payload).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("greetd wysłał niepoprawny UTF-8: {e}"),
            )
        })?;
        parse_response(&text)
    }
}

fn frame_eof(error: std::io::Error) -> std::io::Error {
    if error.kind() == std::io::ErrorKind::UnexpectedEof {
        std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "greetd zamknął połączenie",
        )
    } else {
        error
    }
}

fn parse_response(line: &str) -> std::io::Result<Response> {
    let value: Value = serde_json::from_str(line.trim()).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("niepoprawny JSON od greetd: {e}"),
        )
    })?;
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "brak pola `type`"))?;
    match kind {
        "auth_message" => {
            let kind = match value
                .get("auth_message_type")
                .and_then(Value::as_str)
                .unwrap_or("info")
            {
                "visible" => AuthKind::Visible,
                "secret" => AuthKind::Secret,
                "error" => AuthKind::Error,
                _ => AuthKind::Info,
            };
            let message = value
                .get("auth_message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            Ok(Response::AuthMessage { kind, message })
        }
        "success" => Ok(Response::Success),
        "error" => Ok(Response::Error {
            error_type: value
                .get("error_type")
                .and_then(Value::as_str)
                .unwrap_or("error")
                .to_owned(),
            description: value
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
        }),
        other => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("nieznany komunikat greetd: {other}"),
        )),
    }
}

// ── Wątek roboczy ───────────────────────────────────────────────────────────

/// Uchwyt do wątku rozmawiającego z greetd.
pub struct Worker {
    tx: Sender<Command>,
    rx: Receiver<Event>,
}

impl Worker {
    /// Łączy się z greetd w tle i czeka na polecenia.
    pub fn spawn(socket: PathBuf, waker: Option<WakeHandle>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (evt_tx, evt_rx) = mpsc::channel();
        let emitter = Emitter { tx: evt_tx, waker };
        let backup = emitter.clone();
        let spawn = std::thread::Builder::new()
            .name("luna-greetd".to_owned())
            .spawn(move || run(&socket, cmd_rx, emitter));
        if let Err(error) = spawn {
            backup.emit(Event::Disconnected {
                message: format!("nie można uruchomić wątku greetd: {error}"),
            });
        }
        Self {
            tx: cmd_tx,
            rx: evt_rx,
        }
    }

    /// Wysyła polecenie do wątku roboczego.
    pub fn send(&self, command: Command) -> bool {
        self.tx.send(command).is_ok()
    }

    /// Odbiera zdarzenie bez blokowania.
    pub fn try_recv(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }

    /// Odbiera zdarzenie z limitem czasu (używane w testach i diagnostyce).
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Event> {
        self.rx.recv_timeout(timeout).ok()
    }
}

#[derive(Clone)]
struct Emitter {
    tx: Sender<Event>,
    waker: Option<WakeHandle>,
}

impl Emitter {
    fn emit(&self, event: Event) {
        let _ = self.tx.send(event);
        if let Some(waker) = &self.waker {
            waker.wake();
        }
    }
}

fn run(socket: &Path, rx: Receiver<Command>, emitter: Emitter) {
    let mut client = match Client::connect(socket) {
        Ok(client) => client,
        Err(error) => {
            emitter.emit(Event::Disconnected {
                message: format!("{}: {error}", socket.display()),
            });
            return;
        }
    };
    emitter.emit(Event::Connected);

    while let Ok(command) = rx.recv() {
        let starting = matches!(&command, Command::Start { .. });
        let request = match command {
            Command::Begin { username } => Request::CreateSession { username },
            Command::Respond { response } => Request::PostAuthMessageResponse {
                response: Some(response),
            },
            Command::Start { cmd, env } => Request::StartSession { cmd, env },
            Command::Cancel => Request::CancelSession,
        };
        if let Err(error) = client.send(&request) {
            emitter.emit(Event::Disconnected {
                message: error.to_string(),
            });
            return;
        }
        if matches!(&request, Request::CancelSession) {
            return;
        }

        loop {
            match client.read() {
                Ok(Response::AuthMessage { kind, message }) => {
                    emitter.emit(Event::Prompt { kind, message });
                    if matches!(kind, AuthKind::Secret | AuthKind::Visible) {
                        // Poczekaj na `Respond` z UI.
                        break;
                    }
                    // info/error nie wymagają odpowiedzi — czytaj dalej.
                }
                Ok(Response::Success) => {
                    emitter.emit(if starting {
                        Event::Started
                    } else {
                        Event::AuthOk
                    });
                    if starting {
                        return;
                    }
                    break;
                }
                Ok(Response::Error {
                    error_type,
                    description,
                }) => {
                    emitter.emit(Event::ServerError {
                        error_type,
                        description,
                    });
                    return;
                }
                Err(error) => {
                    emitter.emit(Event::Disconnected {
                        message: error.to_string(),
                    });
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn read_frame(stream: &mut UnixStream) -> String {
        let mut header = [0u8; 4];
        stream.read_exact(&mut header).unwrap();
        let len = u32::from_ne_bytes(header) as usize;
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).unwrap();
        String::from_utf8(payload).unwrap()
    }

    fn write_frame(stream: &mut UnixStream, json: &str) {
        let payload = json.as_bytes();
        stream
            .write_all(&(payload.len() as u32).to_ne_bytes())
            .unwrap();
        stream.write_all(payload).unwrap();
        stream.flush().unwrap();
    }

    fn socket_path() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "luna-greeter-test-{}-{stamp}.sock",
            std::process::id()
        ))
    }

    #[test]
    fn parses_auth_message_and_errors() {
        let msg =
            r#"{"type":"auth_message","auth_message_type":"secret","auth_message":"Password: "}"#;
        match parse_response(msg).unwrap() {
            Response::AuthMessage { kind, message } => {
                assert_eq!(kind, AuthKind::Secret);
                assert_eq!(message, "Password: ");
            }
            _ => panic!("oczekiwano auth_message"),
        }
        let msg = r#"{"type":"error","error_type":"auth_error","description":"denied"}"#;
        match parse_response(msg).unwrap() {
            Response::Error {
                error_type,
                description,
            } => {
                assert_eq!(error_type, "auth_error");
                assert_eq!(description, "denied");
            }
            _ => panic!("oczekiwano error"),
        }
    }

    #[test]
    fn authenticates_against_fake_greetd() {
        let path = socket_path();
        let listener = UnixListener::bind(&path).expect("bind");
        let server_path = path.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");

            let line = read_frame(&mut stream);
            assert!(line.contains("create_session"));
            assert!(line.contains("\"patryk\""));
            write_frame(
                &mut stream,
                r#"{"type":"auth_message","auth_message_type":"secret","auth_message":"Password: "}"#,
            );

            let line = read_frame(&mut stream);
            assert!(line.contains("post_auth_message_response"));
            assert!(line.contains("hunter2"));
            write_frame(&mut stream, r#"{"type":"success"}"#);

            let line = read_frame(&mut stream);
            assert!(line.contains("start_session"));
            assert!(line.contains("luna-session"));
            write_frame(&mut stream, r#"{"type":"success"}"#);
        });

        let worker = Worker::spawn(server_path.clone(), None);
        assert_eq!(
            worker.recv_timeout(Duration::from_secs(2)),
            Some(Event::Connected)
        );
        assert!(worker.send(Command::Begin {
            username: "patryk".to_owned()
        }));
        match worker.recv_timeout(Duration::from_secs(2)) {
            Some(Event::Prompt { kind, .. }) => assert_eq!(kind, AuthKind::Secret),
            other => panic!("oczekiwano promptu, jest {other:?}"),
        }
        assert!(worker.send(Command::Respond {
            response: "hunter2".to_owned()
        }));
        assert_eq!(
            worker.recv_timeout(Duration::from_secs(2)),
            Some(Event::AuthOk)
        );
        assert!(worker.send(Command::Start {
            cmd: vec!["luna-session".to_owned()],
            env: vec!["XDG_SESSION_TYPE=wayland".to_owned()],
        }));
        assert_eq!(
            worker.recv_timeout(Duration::from_secs(2)),
            Some(Event::Started)
        );
        server.join().expect("serwer testowy");
        let _ = std::fs::remove_file(path);
    }
}
