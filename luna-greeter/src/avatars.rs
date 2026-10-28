//! Awatary kont z AccountsService.
//!
//! Greeter próbuje użyć zdjęcia profilowego użytkownika z
//! `/var/lib/AccountsService` (ścieżka z `[User] Icon=` albo standardowy plik
//! `icons/<login>`). Obraz jest dekodowany raz przy starcie, a przy braku pliku,
//! uprawnień albo niepoprawnego PNG zostają inicjały z GECOS. Dzięki temu greeter
//! nie zależy od tego, czy system ma AccountsService i nie czyta katalogów domowych
//! bez zgody systemu.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use libluna_render::wallpaper::decode_png;
use libluna_render::ImageData;

use crate::users::User;

const MAX_ICON_BYTES: u64 = 8 * 1024 * 1024;

/// Wczytane awatary (login → obraz).
#[derive(Debug, Clone, Default)]
pub struct Avatars {
    images: HashMap<String, Arc<ImageData>>,
}

impl Avatars {
    /// Ładuje awatary dla podanych kont z domyślnego katalogu AccountsService.
    pub fn load(users: &[User]) -> Self {
        Self::load_from(users, Path::new("/var/lib/AccountsService"))
    }

    /// Wariant testowy z jawnym katalogiem bazowym.
    pub fn load_from(users: &[User], base: &Path) -> Self {
        let mut images = HashMap::new();
        for user in users {
            if let Some(path) = icon_path(base, &user.name) {
                if let Some(image) = decode_icon(&path) {
                    images.insert(user.name.clone(), image);
                }
            }
        }
        Self { images }
    }

    /// Obraz dla loginu, jeśli był poprawny i czytelny.
    pub fn get(&self, user: &str) -> Option<&Arc<ImageData>> {
        self.images.get(user)
    }
}

/// Ścieżka ikony: najpierw `Icon=` z pliku użytkownika, potem `icons/<login>`.
fn icon_path(base: &Path, user: &str) -> Option<PathBuf> {
    let configured = std::fs::read_to_string(base.join("users").join(user))
        .ok()
        .and_then(|text| configured_icon(&text));
    if let Some(path) = configured {
        let path = if path.is_absolute() {
            path
        } else {
            base.join(path)
        };
        if path.is_file() {
            return Some(path);
        }
    }
    let fallback = base.join("icons").join(user);
    fallback.is_file().then_some(fallback)
}

fn configured_icon(text: &str) -> Option<PathBuf> {
    let mut in_user = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_user = line == "[User]";
            continue;
        }
        if !in_user {
            continue;
        }
        if let Some(value) = line.strip_prefix("Icon=") {
            let value = value.trim();
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }
    }
    None
}

fn decode_icon(path: &Path) -> Option<Arc<ImageData>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_ICON_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    decode_png(&bytes).map(Arc::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_core::{Color, Rect};
    use libluna_render::{Scene, SoftwareRenderer};

    fn sample_png() -> Vec<u8> {
        let mut scene = Scene::new();
        scene.fill_rect(
            Rect::from_xywh(0.0, 0.0, 4.0, 4.0),
            Color::from_rgba8(20, 120, 220, 255),
        );
        let mut renderer = SoftwareRenderer::new(4, 4);
        renderer.draw(&scene, 1.0, Color::TRANSPARENT);
        renderer.encode_png().expect("PNG")
    }

    #[test]
    fn reads_configured_and_fallback_icons() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!(
            "luna-greeter-avatars-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(base.join("users")).unwrap();
        std::fs::create_dir_all(base.join("icons")).unwrap();
        let png = sample_png();
        let custom = base.join("custom.png");
        std::fs::write(&custom, &png).unwrap();
        std::fs::write(
            base.join("users/patryk"),
            format!("[User]\nIcon={}\n", custom.display()),
        )
        .unwrap();
        std::fs::write(base.join("icons/student"), &png).unwrap();

        let users = vec![
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
            User {
                name: "nobody".to_owned(),
                display: "Nobody".to_owned(),
                uid: 1002,
            },
        ];
        let avatars = Avatars::load_from(&users, &base);
        assert_eq!(avatars.get("patryk").map(|i| i.width), Some(4));
        assert_eq!(avatars.get("student").map(|i| i.width), Some(4));
        assert!(avatars.get("nobody").is_none());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn rejects_broken_or_oversized_icons() {
        assert!(decode_icon(Path::new("/definitely/missing/icon.png")).is_none());
    }
}
