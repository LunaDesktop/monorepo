//! Wykrywanie sesji **live ISO**.
//!
//! Welcome potrzebuje jednej decyzji: czy system działa z nośnika live (wtedy
//! pokazujemy zaproszenie do instalatora), czy jest już zainstalowany. Nie
//! zgadujemy po nazwie hosta — sprawdzamy standardowe znaczniki live-boot,
//! casper i rd.live.image oraz linię poleceń jądra.
//!
//! Do testów i obrazów niestandardowych można wymusić wynik zmienną
//! `LUNA_LIVE` (`1` = live, `0` = zainstalowany system).

use std::path::{Path, PathBuf};

/// Znaczniki pozostawiane przez popularne mechanizmy live.
const MARKERS: &[&str] = &[
    "/run/live/medium",
    "/lib/live/mount/medium",
    "/live/image",
    "/run/initramfs/live",
    "/cdrom/casper/filesystem.squashfs",
    "/run/live/rootfs",
    "/etc/luna/live",
];

/// Tokeny jądra oznaczające uruchomienie z obrazu live.
const CMDLINE_TOKENS: &[&str] = &[
    "boot=live",
    "casper",
    "rd.live.image",
    "live-media=",
    "overlayroot",
];

/// Wynik wykrywania.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveEnvironment {
    /// `true`, gdy sesja działa z nośnika live.
    pub live: bool,
    /// Zamontowany nośnik live (pierwszy znaleziony znacznik), jeśli znany.
    pub medium: Option<PathBuf>,
    /// Czytelne uzasadnienie — pokazywane w podglądzie i logach.
    pub reason: String,
}

impl LiveEnvironment {
    /// Wykrywa środowisko bieżącego procesu.
    pub fn detect() -> Self {
        let override_value = std::env::var("LUNA_LIVE").ok();
        let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
        Self::detect_with(override_value.as_deref(), &cmdline, |path| path.exists())
    }

    /// Wariant testowalny: bez dotykania systemu plików poza `exists`.
    pub fn detect_with(
        override_value: Option<&str>,
        cmdline: &str,
        exists: impl Fn(&Path) -> bool,
    ) -> Self {
        if let Some(value) = override_value {
            let normalized = value.trim().to_ascii_lowercase();
            if matches!(normalized.as_str(), "1" | "true" | "yes" | "live") {
                return Self {
                    live: true,
                    medium: None,
                    reason: "wymuszone przez LUNA_LIVE".to_owned(),
                };
            }
            if matches!(normalized.as_str(), "0" | "false" | "no" | "installed") {
                return Self {
                    live: false,
                    medium: None,
                    reason: "wyłączone przez LUNA_LIVE".to_owned(),
                };
            }
        }

        for marker in MARKERS {
            let path = Path::new(marker);
            if exists(path) {
                return Self {
                    live: true,
                    medium: Some(path.to_path_buf()),
                    reason: format!("znacznik live: {marker}"),
                };
            }
        }

        if let Some(token) = cmdline_token(cmdline) {
            return Self {
                live: true,
                medium: None,
                reason: format!("parametr jądra: {token}"),
            };
        }

        Self {
            live: false,
            medium: None,
            reason: "brak znaczników live ISO".to_owned(),
        }
    }
}

fn cmdline_token(cmdline: &str) -> Option<&'static str> {
    CMDLINE_TOKENS
        .iter()
        .copied()
        .find(|token| cmdline.split_whitespace().any(|arg| arg.contains(token)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins_over_markers() {
        let env = LiveEnvironment::detect_with(Some("0"), "boot=live", |_| true);
        assert!(!env.live);
    }

    #[test]
    fn kernel_parameter_marks_live() {
        let env = LiveEnvironment::detect_with(None, "quiet splash boot=live", |_| false);
        assert!(env.live);
        assert!(env.reason.contains("boot=live"));
    }

    #[test]
    fn marker_path_is_reported() {
        let env =
            LiveEnvironment::detect_with(None, "", |path| path == Path::new("/run/live/medium"));
        assert!(env.live);
        assert_eq!(env.medium.as_deref(), Some(Path::new("/run/live/medium")));
    }

    #[test]
    fn ordinary_system_is_not_live() {
        let env = LiveEnvironment::detect_with(None, "rw quiet root=/dev/nvme0n1p2", |_| false);
        assert!(!env.live);
        assert!(env.medium.is_none());
    }
}
