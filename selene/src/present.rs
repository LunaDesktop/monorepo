//! Zwalnianie buforów klientów **po pokazaniu klatki**.
//!
//! Wayland pozwala użyć bufora ponownie dopiero po `wl_buffer.release`. Wysłanie
//! go przy `attach` następnej klatki jest zbyt wczesne: gdy kompozytor nie zdąży
//! zaprezentować nowej klatki (np. 37 fps przy animacji 60 fps), stary bufor wciąż
//! jest skanowany, a klient już do niego rysuje — stąd migotanie i „duchy”.
//!
//! Wątek protokołu odkłada więc zwolnienie do kolejki, a wątek wyjścia opróżnia ją
//! **po** faktycznej prezentacji (po `wait_flip` w DRM albo po klatce nested).
//! Numer sekwencji chroni przed wyścigiem: zwalniamy tylko bufory zakolejkowane
//! przed ostatnią prezentacją.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use wayland_server::protocol::wl_buffer::WlBuffer;
use wayland_server::Resource;

use crate::sync::LockExt;
use crate::syncobj;

/// Kolejka zwolnień `wl_buffer.release` czekających na prezentację.
pub struct PresentQueue {
    /// Numer ostatniej klatki przekazanej do skanoutu.
    submitted: AtomicU64,
    /// Numer ostatniej klatki faktycznie pokazanej (po `wait_flip`).
    presented: AtomicU64,
    pending: Mutex<Vec<(u64, WlBuffer, Option<syncobj::Point>)>>,
}

impl PresentQueue {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            submitted: AtomicU64::new(0),
            presented: AtomicU64::new(0),
            pending: Mutex::new(Vec::new()),
        })
    }

    /// Odkłada zwolnienie bufora do najbliższej prezentacji klatki, która go zastąpiła.
    pub fn release_after_present(&self, buffer: WlBuffer, point: Option<syncobj::Point>) {
        let seq = self.submitted.load(Ordering::Acquire);
        self.pending.lock_or_recover().push((seq, buffer, point));
    }

    /// Wątek wyjścia: klatka przekazana do skanoutu (przed `wait_flip`).
    pub fn mark_submitted(&self) {
        self.submitted.fetch_add(1, Ordering::AcqRel);
    }

    /// Wątek wyjścia: klatka została pokazana — oddaj bufory z wcześniejszych klatek.
    pub fn presented(&self) {
        let seq = self.presented.fetch_add(1, Ordering::AcqRel) + 1;
        let due: Vec<(WlBuffer, Option<syncobj::Point>)> = {
            let mut pending = self.pending.lock_or_recover();
            let mut due = Vec::new();
            let mut index = 0;
            while index < pending.len() {
                if pending[index].0 < seq {
                    let (_, buffer, point) = pending.swap_remove(index);
                    due.push((buffer, point));
                } else {
                    index += 1;
                }
            }
            due
        };
        // Poza blokadą: `send_event` może obudzić klienta i wejść w backend.
        for (buffer, point) in due {
            let _ = buffer.send_event(wayland_server::protocol::wl_buffer::Event::Release);
            if let Some(point) = point {
                syncobj::signal_release(point);
            }
        }
    }

    /// Czy jakiś bufor czeka na zwolnienie (wątek wyjścia może wymusić klatkę).
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
        let queue = PresentQueue::new();
        assert_eq!(queue.pending_len(), 0);
        assert!(!queue.has_pending());
        queue.mark_submitted();
        queue.presented();
        assert_eq!(queue.presented.load(Ordering::Acquire), 1);
    }
}
