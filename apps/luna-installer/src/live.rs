//! Wykrywanie sesji **live ISO**.
//!
//! Welcome potrzebuje jednej decyzji: czy system działa z nośnika live (wtedy
//! pokazujemy zaproszenie do instalatora), czy jest już zainstalowany. Nie
//! zgadujemy po nazwie hosta — sprawdzamy standardowe znaczniki live-boot,
//! casper i rd.live.image, parametr jądra oraz typ systemu plików rootu.
//!
//! Obraz PaganOS (`pagan-mkiso`) uruchamia jądro z parametrem **`pagan.live`**
//! i montuje root jako **overlayfs** (squashfs w `lowerdir`, tmpfs w `upperdir`).
//! Oba sygnały są sprawdzane, bo parametr bywa zmieniany w niestandardowych
//! obrazach, a overlay root zostaje nawet przy innym wpisie w bootloaderze.
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
///
/// `pagan.live` to parametr obrazów PaganOS (`pagan-mkiso`); reszta to
/// konwencje Debian live-boot, Ubuntu casper, dracut i overlayroot.
const CMDLINE_TOKENS: &[&str] = &[
    "pagan.live",
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
        let mounts = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
        Self::detect_with(override_value.as_deref(), &cmdline, &mounts, |path| {
            path.exists()
        })
    }

    /// Wariant testowalny: bez dotykania systemu plików poza `exists`.
    pub fn detect_with(
        override_value: Option<&str>,
        cmdline: &str,
        mounts: &str,
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

        if overlay_root(mounts) {
            return Self {
                live: true,
                medium: None,
                reason: "root to overlayfs (obraz live)".to_owned(),
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

/// Czy root (`/`) jest overlayfsem — tak montuje system obraz PaganOS.
///
/// Linia `/proc/mounts`: `urządzenie punkt_montowania typ opcje …`.
fn overlay_root(mounts: &str) -> bool {
    mounts.lines().any(|line| {
        let mut fields = line.split_whitespace();
        let _device = fields.next();
        let mountpoint = fields.next();
        let fstype = fields.next();
        mountpoint == Some("/") && fstype == Some("overlay")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins_over_markers() {
        let env = LiveEnvironment::detect_with(Some("0"), "boot=live", "", |_| true);
        assert!(!env.live);
    }

    #[test]
    fn kernel_parameter_marks_live() {
        let env = LiveEnvironment::detect_with(None, "quiet splash boot=live", "", |_| false);
        assert!(env.live);
        assert!(env.reason.contains("boot=live"));
    }

    #[test]
    fn pagan_parameter_marks_live() {
        let env = LiveEnvironment::detect_with(None, "pagan.live rw quiet splash", "", |_| false);
        assert!(env.live);
        assert!(env.reason.contains("pagan.live"));
    }

    #[test]
    fn marker_path_is_reported() {
        let env = LiveEnvironment::detect_with(None, "", "", |path| {
            path == Path::new("/run/live/medium")
        });
        assert!(env.live);
        assert_eq!(env.medium.as_deref(), Some(Path::new("/run/live/medium")));
    }

    #[test]
    fn overlay_root_marks_live() {
        let mounts = "\
sysfs /sys sysfs rw 0 0
overlay / overlay rw,lowerdir=/lower,upperdir=/overlay/upper,workdir=/overlay/work 0 0
tmpfs /run tmpfs rw 0 0
";
        let env = LiveEnvironment::detect_with(None, "rw quiet splash", mounts, |_| false);
        assert!(env.live);
        assert!(env.reason.contains("overlay"));
    }

    #[test]
    fn ordinary_system_is_not_live() {
        let mounts = "\
/dev/nvme0n1p3 / ext4 rw,relatime 0 0
tmpfs /run tmpfs rw 0 0
";
        let env =
            LiveEnvironment::detect_with(None, "rw quiet root=/dev/nvme0n1p2", mounts, |_| false);
        assert!(!env.live);
        assert!(env.medium.is_none());
    }
}
