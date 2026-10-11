//! Podgląd głośności — źródło zdarzeń dla OSD.
//!
//! Dominujący panel pokazuje OSD po zmianie głośności/wyciszenia (np. klawisze
//! multimedialne). Nie mamy tu magistrali PipeWire, więc stan odczytujemy w tle
//! przez `wpctl` (patrz [`crate::system::read_volume`]) i zgłaszamy zmianę licznikiem
//! [`VolumeWatcher::current`] — UI porównuje licznik i pokazuje OSD.

use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Odstęp odpytywania głośności (s).
const POLL: Duration = Duration::from_millis(400);

#[derive(Default)]
struct Shared {
    seq: u64,
    volume: Option<(u8, bool)>,
}

/// Obserwator głośności (własny wątek odpytywania).
pub struct VolumeWatcher {
    shared: Arc<Mutex<Shared>>,
}

impl VolumeWatcher {
    /// Startuje wątek i zwraca uchwyt.
    pub fn spawn() -> Self {
        let shared: Arc<Mutex<Shared>> = Arc::new(Mutex::new(Shared::default()));
        let writer = Arc::clone(&shared);
        std::thread::spawn(move || {
            let mut last = crate::system::read_volume();
            loop {
                std::thread::sleep(POLL);
                let now = crate::system::read_volume();
                if now != last {
                    last = now;
                    if let Ok(mut s) = writer.lock() {
                        s.seq += 1;
                        s.volume = now;
                    }
                }
            }
        });
        Self { shared }
    }

    /// Bieżący `(licznik zmian, głośność)`. Licznik rośnie przy każdej zmianie.
    pub fn current(&self) -> (u64, Option<(u8, bool)>) {
        let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        (s.seq, s.volume)
    }
}

/// Etykieta OSD dla stanu głośności.
pub fn label(muted: bool) -> &'static str {
    if muted {
        "Wyciszono"
    } else {
        "Głośność"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_reflects_mute() {
        assert_eq!(label(false), "Głośność");
        assert_eq!(label(true), "Wyciszono");
    }

    #[test]
    fn fresh_watcher_has_no_change_yet() {
        // Nie startujemy wątku — interesuje nas tylko stan początkowy.
        let w = VolumeWatcher {
            shared: Arc::new(Mutex::new(Shared::default())),
        };
        assert_eq!(w.current(), (0, None));
    }
}
