//! Automatyczne wygaszanie, blokada i usypianie po bezczynności (`~/.config/luna/idle.conf`).
//!
//! Ustawienia czyta **Selene** (kompozytor wie, kiedy ostatnio było wejście) i centrum
//! ustawień (zapis + `luna-control idle-reload`, żeby zmiana działała bez wylogowania).
//! Wszystkie progi liczone są od **jednego zegara bezczynności**: czasu od ostatniego
//! zdarzenia klawiatury/myszy/dotyku.
//!
//! Wartość `0` oznacza „nigdy”. Kolejność progów nie ma znaczenia: każdy działa
//! niezależnie (np. blokada po 5 min i wygaszenie po 10 min).

use crate::config::{config_file, parse_pairs, save_atomic};

/// Plik ustawień w `~/.config/luna`.
pub const CONFIG_FILE: &str = "idle.conf";

/// Progi oferowane w UI (sekundy): nigdy, 1, 5, 10, 20, 30, 60 minut.
pub const CHOICES: [u32; 7] = [0, 60, 300, 600, 1200, 1800, 3600];

/// Ustawienia bezczynności.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdleSettings {
    /// Po ilu sekundach wygasić ekran (czarna klatka, kursor schowany); 0 = nigdy.
    pub screen_off_secs: u32,
    /// Po ilu sekundach zablokować sesję (`luna-lock`); 0 = nigdy.
    pub lock_secs: u32,
    /// Po ilu sekundach uśpić komputer; 0 = nigdy.
    pub suspend_secs: u32,
    /// Czy wygaszenie ekranu ma od razu blokować sesję.
    pub lock_on_blank: bool,
}

impl Default for IdleSettings {
    fn default() -> Self {
        Self {
            // Ekran gaśnie po 5 min, ale sesja **nie** blokuje się sama —
            // dotychczasowe zachowanie (brak blokady) zostaje domyślne.
            screen_off_secs: 300,
            lock_secs: 0,
            suspend_secs: 0,
            lock_on_blank: false,
        }
    }
}

impl IdleSettings {
    pub fn config_path() -> Option<std::path::PathBuf> {
        config_file(CONFIG_FILE)
    }

    pub fn load() -> Self {
        Self::config_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        for (key, value) in parse_pairs(text) {
            match key.as_str() {
                "screen_off" => out.screen_off_secs = parse_secs(&value),
                "lock" => out.lock_secs = parse_secs(&value),
                "suspend" => out.suspend_secs = parse_secs(&value),
                "lock_on_blank" => {
                    out.lock_on_blank = matches!(value.as_str(), "true" | "1" | "yes" | "on")
                }
                _ => {}
            }
        }
        out
    }

    pub fn render(&self) -> String {
        format!(
            "# Luna — wygaszanie, blokada i usypianie po bezczynności (0 = nigdy)\n\
             screen_off = {}\nlock = {}\nsuspend = {}\nlock_on_blank = {}\n",
            self.screen_off_secs, self.lock_secs, self.suspend_secs, self.lock_on_blank
        )
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::config_path().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "brak katalogu konfiguracji")
        })?;
        save_atomic(&path, &self.render())
    }

    /// Najbliższy próg, który jeszcze nie zadziałał (sekundy od teraz) — dla pętli Selene.
    pub fn next_deadline_secs(
        &self,
        idle_secs: u64,
        blanked: bool,
        lock_fired: bool,
        suspend_fired: bool,
    ) -> Option<f64> {
        let mut next: Option<f64> = None;
        let mut push = |secs: u32| {
            let remaining = f64::from(secs) - idle_secs as f64;
            next = Some(next.map_or(remaining, |n: f64| n.min(remaining)));
        };
        if self.screen_off_secs > 0 && !blanked {
            push(self.screen_off_secs);
        }
        if self.lock_secs > 0 && !lock_fired {
            push(self.lock_secs);
        }
        if self.suspend_secs > 0 && !suspend_fired {
            push(self.suspend_secs);
        }
        next
    }
}

fn parse_secs(value: &str) -> u32 {
    value
        .trim()
        .parse::<u32>()
        .map(|secs| secs.min(24 * 60 * 60))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_blank_after_five_minutes_without_locking() {
        let idle = IdleSettings::default();
        assert_eq!(idle.screen_off_secs, 300);
        assert_eq!(idle.lock_secs, 0);
        assert!(!idle.lock_on_blank);
    }

    #[test]
    fn round_trip_through_text() {
        let idle = IdleSettings {
            screen_off_secs: 600,
            lock_secs: 300,
            suspend_secs: 3600,
            lock_on_blank: true,
        };
        assert_eq!(IdleSettings::parse(&idle.render()), idle);
    }

    #[test]
    fn zeros_disable_everything() {
        let idle = IdleSettings::parse("screen_off = 0\nlock = 0\nsuspend = 0\n");
        assert_eq!(idle.screen_off_secs, 0);
        assert_eq!(idle.lock_secs, 0);
        assert_eq!(idle.suspend_secs, 0);
    }

    #[test]
    fn garbage_is_ignored_and_clamped() {
        let idle = IdleSettings::parse("screen_off = abc\nlock = 999999999\n");
        assert_eq!(idle.screen_off_secs, 0);
        assert_eq!(idle.lock_secs, 24 * 60 * 60);
    }

    #[test]
    fn next_deadline_points_at_the_closest_pending_action() {
        let idle = IdleSettings {
            screen_off_secs: 600,
            lock_secs: 300,
            suspend_secs: 0,
            lock_on_blank: false,
        };
        // Po 100 s bezczynności najbliżej jest blokada (za 200 s).
        assert_eq!(
            idle.next_deadline_secs(100, false, false, false),
            Some(200.0)
        );
        // Po wykonanej blokadzie zostaje wygaszenie (za 300 s).
        assert_eq!(
            idle.next_deadline_secs(400, false, true, false),
            Some(200.0)
        );
        // Gdy ekran już zgasł i blokada zadziałała — nic nie czeka.
        assert_eq!(idle.next_deadline_secs(400, true, true, false), None);
    }
}
