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

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
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
}

impl PresentQueue {
    pub fn new(start: Instant) -> Arc<Self> {
        Arc::new(Self {
            start,
            submitted: AtomicU64::new(0),
            presented: AtomicU64::new(0),
            pending: Mutex::new(Vec::new()),
        })
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

    /// Wątek wyjścia: klatka została pokazana — wyślij zaległe zdarzenia.
    pub fn presented(&self) {
        let seq = self.presented.fetch_add(1, Ordering::AcqRel) + 1;
        // Zdarzenie może czekać tylko wtedy, gdy po jego zakolejkowaniu powstała
        // i została pokazana co najmniej jedna klatka. Sam `presented` bez nowego
        // `submitted` to wyścig z commitem — wtedy czekamy do następnego obiegu.
        let rendered = self.submitted.load(Ordering::Acquire);
        let due: Vec<Item> = {
            let mut pending = self.pending.lock_or_recover();
            let mut due = Vec::new();
            let mut index = 0;
            while index < pending.len() {
                let at = pending[index].0;
                if at < seq && at < rendered {
                    due.push(pending.swap_remove(index).1);
                } else {
                    index += 1;
                }
            }
            due
        };
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
    }

    /// Czy coś czeka na zwolnienie/wysłanie (wątek wyjścia może wymusić klatkę).
    pub fn has_pending(&self) -> bool {
        !self.pending.lock_or_recover().is_empty()
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
        assert!(!queue.has_pending());
        queue.mark_submitted();
        queue.presented();
        assert_eq!(queue.presented.load(Ordering::Acquire), 1);
    }
}
