//! Miniatury kafelków galerii — leniwie, w tle, z pamięcią podręczną.
//!
//! UI nigdy nie dekoduje obrazu w wątku klatki: zgłasza tylko „potrzebuję
//! miniatury tego pliku”, a dwa wątki robocze dekodują je równolegle
//! (`ffmpeg` dla filmów i egzotycznych formatów).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use libluna_render::ImageData;

use crate::model::MediaKind;

/// Dłuższy bok miniatury (kafelek jest znacznie mniejszy, ale zostaje zapas
/// na ekrany HiDPI i powiększenie kafelka).
pub const THUMB_MAX: u32 = 320;

struct Request {
    path: PathBuf,
    kind: MediaKind,
}

struct Response {
    path: PathBuf,
    image: Option<Arc<ImageData>>,
}

enum Cached {
    Pending,
    Ready(Arc<ImageData>),
    Failed,
}

/// Wspólna kolejka miniatur.
pub struct Thumbnails {
    tx: Option<Sender<Request>>,
    rx: Receiver<Response>,
    cache: HashMap<PathBuf, Cached>,
}

impl Thumbnails {
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<Request>();
        let (response_tx, response_rx) = mpsc::channel::<Response>();
        let queue = Arc::new(Mutex::new(request_rx));
        for i in 0..2 {
            let queue = Arc::clone(&queue);
            let responses = response_tx.clone();
            let _ = thread::Builder::new()
                .name(format!("luna-media-thumb-{i}"))
                .spawn(move || loop {
                    let request = {
                        let Ok(queue) = queue.lock() else { break };
                        queue.recv()
                    };
                    let Ok(request) = request else { break };
                    let image = match request.kind {
                        MediaKind::Image => crate::decode::load(&request.path, THUMB_MAX),
                        MediaKind::Video => {
                            crate::video::thumbnail(&request.path, THUMB_MAX).map(Arc::new)
                        }
                        MediaKind::Pdf => {
                            crate::decode::pdf_page(&request.path, 0, THUMB_MAX).map(Arc::new)
                        }
                    };
                    if responses
                        .send(Response {
                            path: request.path,
                            image,
                        })
                        .is_err()
                    {
                        break;
                    }
                });
        }
        Self {
            tx: Some(request_tx),
            rx: response_rx,
            cache: HashMap::new(),
        }
    }

    /// Zgłasza zapotrzebowanie na miniaturę (jeśli jeszcze jej nie ma).
    pub fn ensure(&mut self, path: &Path, kind: MediaKind) {
        if self.cache.contains_key(path) {
            return;
        }
        self.cache.insert(path.to_path_buf(), Cached::Pending);
        if let Some(tx) = &self.tx {
            if tx
                .send(Request {
                    path: path.to_path_buf(),
                    kind,
                })
                .is_err()
            {
                self.cache.insert(path.to_path_buf(), Cached::Failed);
            }
        }
    }

    /// Gotowa miniatura (albo `None`, gdy jeszcze się liczy / nie udało się).
    pub fn get(&self, path: &Path) -> Option<&Arc<ImageData>> {
        match self.cache.get(path) {
            Some(Cached::Ready(image)) => Some(image),
            _ => None,
        }
    }

    /// Czy miniatura jest w trakcie liczenia.
    pub fn is_pending(&self, path: &Path) -> bool {
        matches!(self.cache.get(path), Some(Cached::Pending))
    }

    /// Odbiera wyniki z wątków; zwraca `true`, jeśli coś przyszło.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(response) = self.rx.try_recv() {
            changed = true;
            match response.image {
                Some(image) => {
                    self.cache.insert(response.path, Cached::Ready(image));
                }
                None => {
                    self.cache.insert(response.path, Cached::Failed);
                }
            }
        }
        // Pamięć podręczna nie może rosnąć bez końca przy przeglądaniu dużych katalogów.
        if self.cache.len() > 240 {
            self.cache
                .retain(|_, value| matches!(value, Cached::Pending));
        }
        changed
    }

    /// Czy są miniatury, na które jeszcze czekamy.
    pub fn pending(&self) -> bool {
        self.cache
            .values()
            .any(|value| matches!(value, Cached::Pending))
    }
}

impl Default for Thumbnails {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn fails_gracefully_for_a_missing_file() {
        let mut thumbs = Thumbnails::new();
        let path = std::env::temp_dir().join("luna-media-nie-ma-takiego.png");
        let _ = std::fs::remove_file(&path);
        thumbs.ensure(&path, MediaKind::Image);
        assert!(thumbs.is_pending(&path));
        let deadline = Instant::now() + Duration::from_secs(5);
        while thumbs.is_pending(&path) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
            thumbs.poll();
        }
        assert!(!thumbs.is_pending(&path));
        assert!(thumbs.get(&path).is_none());
    }
}
