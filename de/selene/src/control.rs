//! Sterowanie kompozytorem z powłoki (lokalny gniazdo Unix).
//!
//! Protokoły Wayland nie mają żądania „przenieś to okno na przestrzeń”, więc powłoka
//! wysyła krótkie polecenie tekstowe do `$XDG_RUNTIME_DIR/luna-control`. Gniazdo jest
//! odpytywane w głównej pętli razem z wejściem — bez dodatkowych wątków i bez
//! blokowania na kliencie (timeout odczytu).

use std::io::Read;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::time::Duration;

use crate::state::State;

pub struct Control {
    listener: UnixListener,
}

impl Control {
    /// Tworzy gniazdo `luna-control` (usuwa pozostałość po poprzedniej sesji).
    pub fn spawn() -> Option<Self> {
        let rt = std::env::var_os("XDG_RUNTIME_DIR")?;
        let path = PathBuf::from(rt).join("luna-control");
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).ok()?;
        listener.set_nonblocking(true).ok()?;
        Some(Self { listener })
    }

    pub fn fd(&self) -> RawFd {
        self.listener.as_raw_fd()
    }

    /// Przyjmuje i wykonuje wszystkie oczekujące polecenia.
    pub fn accept(&self, state: &mut State) {
        while let Ok((mut stream, _)) = self.listener.accept() {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
            let mut buf = [0u8; 1024];
            if let Ok(n) = stream.read(&mut buf) {
                let text = String::from_utf8_lossy(&buf[..n]);
                for line in text.lines() {
                    handle(state, line.trim());
                }
            }
        }
    }
}

fn handle(state: &mut State, line: &str) {
    if line.is_empty() {
        return;
    }
    let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
    match cmd {
        // `move-to-workspace <indeks> <tytuł okna>`
        "move-to-workspace" => {
            let (ws, title) = rest.split_once(' ').unwrap_or((rest, ""));
            if let Ok(ws) = ws.parse::<usize>() {
                crate::seat::move_window_by_title(state, title, ws);
            }
        }
        // `screenshot` — zapis klatek wszystkich monitorów (Super+F12).
        "screenshot" => {
            if std::env::var("SELENE_STATS").is_ok() {
                eprintln!("selene/control: zrzut ekranu");
            }
            state
                .screenshot
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        // `night-light <0..100>` — siła ciepłej korekty gamma.
        "night-light" => {
            if let Ok(percent) = rest.parse::<u8>() {
                state
                    .night_light
                    .store(percent.min(100), std::sync::atomic::Ordering::Relaxed);
                if std::env::var("SELENE_STATS").is_ok() {
                    eprintln!("selene/control: night light {percent}%");
                }
            }
        }
        // `power-saver <0|1>` — ograniczenie tempa prezentacji.
        "power-saver" => {
            let on = matches!(rest.trim(), "1" | "on" | "true");
            state
                .power_saver
                .store(on, std::sync::atomic::Ordering::Relaxed);
            if std::env::var("SELENE_STATS").is_ok() {
                eprintln!(
                    "selene/control: power saver {}",
                    if on { "on" } else { "off" }
                );
            }
        }
        _ => {
            if std::env::var("SELENE_STATS").is_ok() {
                eprintln!("selene/control: nieznane polecenie: {line}");
            }
        }
    }
}
