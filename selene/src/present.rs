//! Zwalnianie buforów klientów **po pokazaniu klatki**.
//!
//! Wayland pozwala użyć bufora ponownie dopiero po `wl_buffer.release`, a frame
//! callback (`wl_callback.done`) powinien przyjść, gdy kompozytor jest gotowy
//! narysować następną klatkę — w praktyce po prezentacji. Wysyłanie obu zdarzeń
//! od razu przy `commit` pozwala klientowi biec szybciej niż ekran: Zen/Firefox
//! animują wtedy sidebar i kontrolki z nieograniczoną częstotliwością, co przy
//! wolnym kompozytorze wygląda jak migotanie.
//!
//! Wątek protokołu odkłada więc zwolnienia i callbacki do kolejki, a wątek
//! wyjścia opróżnia ją **po** faktycznej prezentacji (po `wait_flip` w DRM albo
//! po klatce nested). Numer sekwencji chroni przed wyścigiem: wysyłamy tylko
//! zdarzenia zakolejkowane przed ostatnią prezentacją.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use wayland_server::protocol::{wl_buffer, wl_callback};
use wayland_server::Resource;

use crate::sync::LockExt;
use crate::syncobj;

enum Item {
    Release(wl_buffer::WlBuffer, Option<syncobj::Point>),
    Frame(wl_callback::WlCallback),
}

/// Kolejka zdarzeń czekających na prezentację.
pub struct PresentQueue {
    start: Instant,
    /// Numer ostatniej klatki przekazanej do skanoutu.
    submitted: AtomicU64,
    /// Numer ostatniej klatki faktycznie pokazanej (po `wait_flip`).
    presented: AtomicU64,
    pending: Mutex<Vec<(u64, Item)>>,
    /// Opcjonalny deskryptor wybudzania pętli głównej (`main.rs`), żeby
    /// `display.flush_clients()` wysłało `wl_buffer.release` / `wl_callback.done`
    /// natychmiast po flipie zamiast czekać na ruch myszy lub timeout `poll`.
    wake: OnceLock<UnixStream>,
}

impl PresentQueue {
    pub fn new(start: Instant) -> Arc<Self> {
        Arc::new(Self {
            start,
            submitted: AtomicU64::new(0),
            presented: AtomicU64::new(0),
            pending: Mutex::new(Vec::new()),
            wake: OnceLock::new(),
        })
    }

    /// Rejestruje nieblokujący zapis wybudzający pętlę główną po wysłaniu zdarzeń.
    pub fn set_wake(&self, wake: UnixStream) {
        let _ = wake.set_nonblocking(true);
        let _ = self.wake.set(wake);
    }

    fn notify_wake(&self) {
        if let Some(mut w) = self.wake.get() {
            let _ = w.write(&[1u8]);
        }
    }

    /// Odkłada zwolnienie bufora do najbliższej prezentacji klatki, która go zastąpiła.
    pub fn release_after_present(
        &self,
        buffer: wl_buffer::WlBuffer,
        point: Option<syncobj::Point>,
    ) {
        let seq = self.submitted.load(Ordering::Acquire);
        self.pending
            .lock_or_recover()
            .push((seq, Item::Release(buffer, point)));
    }

    /// Odkłada frame-callback do najbliższej prezentacji.
    pub fn frame_after_present(&self, callback: wl_callback::WlCallback) {
        let seq = self.submitted.load(Ordering::Acquire);
        self.pending
            .lock_or_recover()
            .push((seq, Item::Frame(callback)));
    }

    /// Wątek wyjścia: klatka przekazana do skanoutu (przed `wait_flip`).
    pub fn mark_submitted(&self) {
        self.submitted.fetch_add(1, Ordering::AcqRel);
    }

    /// Numer ostatniej klatki przekazanej do skanoutu.
    pub fn submitted(&self) -> u64 {
        self.submitted.load(Ordering::Acquire)
    }

    /// Wątek wyjścia: klatka została pokazana — wyślij zaległe zdarzenia.
    pub fn presented(&self) {
        self.presented_upto(self.submitted());
    }

    /// Zwalnia zdarzenia zakolejkowane **przed** klatką numer `upto`.
    ///
    /// Monitory mają różne odświeżanie (75 Hz + 60 Hz), więc „pokazane” liczy
    /// się per wyjście. Wywołujący podaje numer najstarszej klatki, która
    /// **nadal jest skanowana** (minus jeden), zamiast czekać na kompletny
    /// zestaw flipów — inaczej 60 Hz dyktowało tempo obu monitorom.
    pub fn presented_upto(&self, upto: u64) {
        self.presented.fetch_add(1, Ordering::AcqRel);
        // Zdarzenie może wyjść dopiero wtedy, gdy po jego zakolejkowaniu
        // została przekazana i pokazana co najmniej jedna klatka (`at < upto`).
        // Porównanie wyłącznie z `submitted` chroni przed rozjazdem liczników
        // po `resume()` lub `set_mode()`.
        let due: Vec<Item> = {
            let mut pending = self.pending.lock_or_recover();
            let mut due = Vec::new();
            let mut index = 0;
            while index < pending.len() {
                let at = pending[index].0;
                if at < upto {
                    due.push(pending.swap_remove(index).1);
                } else {
                    index += 1;
                }
            }
            due
        };
        if due.is_empty() {
            return;
        }
        // Poza blokadą: `send_event` może obudzić klienta i wejść w backend.
        let now = self.start.elapsed().as_millis() as u32;
        for item in due {
            match item {
                Item::Release(buffer, point) => {
                    let _ = buffer.send_event(wl_buffer::Event::Release);
                    if let Some(point) = point {
                        syncobj::signal_release(point);
                    }
                }
                Item::Frame(callback) => {
                    let _ = callback.send_event(wl_callback::Event::Done { callback_data: now });
                }
            }
        }
        self.notify_wake();
    }

    /// Czy czeka bufor do zwolnienia (wymaga prezentacji nowej klatki).
    pub fn has_pending_releases(&self) -> bool {
        self.pending
            .lock_or_recover()
            .iter()
            .any(|(_, item)| matches!(item, Item::Release(..)))
    }

    /// Czy czeka frame-callback (można go oddać w tempie odświeżania).
    pub fn has_pending_frames(&self) -> bool {
        self.pending
            .lock_or_recover()
            .iter()
            .any(|(_, item)| matches!(item, Item::Frame(_)))
    }

    /// Oddaje frame-callbacki bez wymuszania nowej klatki.
    ///
    /// Używane, gdy scena się nie zmieniła: klient potrzebuje tylko sygnału
    /// „możesz rysować następną klatkę”. Wymuszanie wtedy pełnego renderu obu
    /// monitorów zrzucało kompozytor do kilkunastu fps (widoczne migotanie).
    pub fn tick_frames(&self) {
        let due: Vec<Item> = {
            let mut pending = self.pending.lock_or_recover();
            let mut due = Vec::new();
            let mut index = 0;
            while index < pending.len() {
                if matches!(pending[index].1, Item::Frame(_)) {
                    due.push(pending.swap_remove(index).1);
                } else {
                    index += 1;
                }
            }
            due
        };
        if due.is_empty() {
            return;
        }
        let now = self.start.elapsed().as_millis() as u32;
        for item in due {
            if let Item::Frame(callback) = item {
                let _ = callback.send_event(wl_callback::Event::Done { callback_data: now });
            }
        }
        self.notify_wake();
    }

    #[cfg(test)]
    fn pending_len(&self) -> usize {
        self.pending.lock_or_recover().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_starts_empty_and_counts_presentations() {
        let queue = PresentQueue::new(std::time::Instant::now());
        assert_eq!(queue.pending_len(), 0);
        assert!(!queue.has_pending_releases());
        assert!(!queue.has_pending_frames());
        queue.mark_submitted();
        queue.presented();
        assert_eq!(queue.presented.load(Ordering::Acquire), 1);
    }

    #[test]
    fn presented_wakes_poll_fd_only_when_events_are_due() {
        use std::io::Read;
        let (mut read_end, write_end) = UnixStream::pair().unwrap();
        read_end.set_nonblocking(true).unwrap();

        let queue = PresentQueue::new(std::time::Instant::now());
        queue.set_wake(write_end);

        // Brak oczekujących zdarzeń -> brak bajtu wybudzającego.
        queue.mark_submitted();
        queue.presented();
        let mut buf = [0u8; 8];
        assert!(read_end.read(&mut buf).is_err());
    }
}
