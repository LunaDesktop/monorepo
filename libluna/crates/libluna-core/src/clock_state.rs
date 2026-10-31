//! Stan aktywnego minutnika/stopera współdzielony przez `luna-clock` i powłokę.
//!
//! Gdy aplikacja jest zamknięta, panel może pokazać kompaktowy widget pod paskiem
//! i dalej odliczać czas. Plik jest lokalny, zapisywany atomowo z prawami `0600`;
//! nie zawiera niczego poza stanem liczników.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{parse_pairs, save_atomic_private};

/// `$XDG_DATA_HOME/luna/clock/active.conf`.
pub fn active_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
        })?;
    Some(base.join("luna").join("clock").join("active.conf"))
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

/// Znacznik czasu pliku — powłoka przeładowuje stan tylko po zmianie.
pub fn stamp() -> Option<SystemTime> {
    active_path()
        .and_then(|path| std::fs::metadata(path).ok())
        .and_then(|meta| meta.modified().ok())
}

/// Minutnik zapisany na dysku.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimerActivity {
    pub total_secs: u64,
    pub running: bool,
    /// Koniec odliczania (Unix), ważny gdy `running`.
    pub end_unix: i64,
    /// Pozostały czas, gdy minutnik jest wstrzymany.
    pub remaining_secs: u64,
    pub finished: bool,
}

impl TimerActivity {
    pub fn remaining_at(&self, now: i64) -> u64 {
        if self.running {
            (self.end_unix - now).max(0) as u64
        } else {
            self.remaining_secs
        }
    }

    /// Czy minutnik jest zakończony (flaga albo minął zapisany koniec).
    pub fn is_finished(&self, now: i64) -> bool {
        self.finished || (self.running && self.end_unix <= now)
    }

    /// Czy warto pokazywać minutnik w widgecie (aktywny lub wstrzymany, ale nie zakończony).
    ///
    /// Zakończony minutnik trafia do powiadomień i znika spod paska.
    pub fn is_active(&self, now: i64) -> bool {
        !self.is_finished(now) && (self.running || self.remaining_secs > 0)
    }

    pub fn progress(&self, now: i64) -> f64 {
        if self.total_secs == 0 {
            return 0.0;
        }
        1.0 - (self.remaining_at(now) as f64 / self.total_secs as f64).clamp(0.0, 1.0)
    }
}

/// Stoper zapisany na dysku.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopwatchActivity {
    /// Czas naliczony do `updated_unix` (milisekundy).
    pub elapsed_millis: u64,
    pub running: bool,
    pub updated_unix: i64,
    /// Czasy okrążeń (łącznie, milisekundy) — do odtworzenia listy.
    pub laps: Vec<u64>,
}

impl StopwatchActivity {
    pub fn elapsed_at(&self, now: i64) -> u64 {
        if self.running {
            self.elapsed_millis
                .saturating_add((now - self.updated_unix).max(0) as u64 * 1_000)
        } else {
            self.elapsed_millis
        }
    }

    pub fn is_active(&self, _now: i64) -> bool {
        self.running || self.elapsed_millis > 0 || !self.laps.is_empty()
    }
}

/// Pełny stan aktywności zegara.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClockActivity {
    /// PID działającej aplikacji (`None` = zamknięta).
    pub pid: Option<u32>,
    pub updated_unix: i64,
    pub timer: Option<TimerActivity>,
    pub stopwatch: Option<StopwatchActivity>,
}

impl ClockActivity {
    pub fn is_app_running(&self) -> bool {
        let Some(pid) = self.pid else {
            return false;
        };
        // `comm` jest obcinane do 15 znaków — „luna-clock” się mieści.
        std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .map(|name| name.trim() == "luna-clock")
            .unwrap_or(false)
    }

    pub fn has_activity(&self, now: i64) -> bool {
        self.timer
            .as_ref()
            .is_some_and(|timer| timer.is_active(now))
            || self
                .stopwatch
                .as_ref()
                .is_some_and(|stopwatch| stopwatch.is_active(now))
    }

    /// Czy panel ma pokazać widget (aplikacja zamknięta, coś odlicza).
    pub fn should_show_widget(&self, now: i64) -> bool {
        !self.is_app_running() && self.has_activity(now)
    }

    pub fn load() -> Option<Self> {
        active_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| Self::parse(&text))
    }

    pub fn save(&self) -> io::Result<()> {
        let Some(path) = active_path() else {
            return Ok(());
        };
        save_atomic_private(&path, &self.render())
    }

    pub fn render(&self) -> String {
        let mut out = String::from("# Luna Clock — aktywny minutnik/stoper (dane lokalne)\n");
        if let Some(pid) = self.pid {
            out.push_str(&format!("app_pid = {pid}\n"));
        }
        out.push_str(&format!("updated = {}\n", self.updated_unix));
        if let Some(timer) = &self.timer {
            out.push_str("timer = 1\n");
            out.push_str(&format!("timer_total = {}\n", timer.total_secs));
            out.push_str(&format!("timer_running = {}\n", u8::from(timer.running)));
            out.push_str(&format!("timer_end = {}\n", timer.end_unix));
            out.push_str(&format!("timer_remaining = {}\n", timer.remaining_secs));
            out.push_str(&format!("timer_finished = {}\n", u8::from(timer.finished)));
        }
        if let Some(stopwatch) = &self.stopwatch {
            out.push_str("stopwatch = 1\n");
            out.push_str(&format!(
                "stopwatch_running = {}\n",
                u8::from(stopwatch.running)
            ));
            out.push_str(&format!(
                "stopwatch_elapsed_ms = {}\n",
                stopwatch.elapsed_millis
            ));
            out.push_str(&format!("stopwatch_updated = {}\n", stopwatch.updated_unix));
            if !stopwatch.laps.is_empty() {
                let laps: Vec<String> = stopwatch.laps.iter().map(u64::to_string).collect();
                out.push_str(&format!("stopwatch_laps = {}\n", laps.join(",")));
            }
        }
        out
    }

    pub fn parse(text: &str) -> Option<Self> {
        let now = now_unix();
        let mut activity = ClockActivity {
            updated_unix: now,
            ..ClockActivity::default()
        };
        let mut timer = false;
        let mut timer_total = 0u64;
        let mut timer_running = false;
        let mut timer_end = 0i64;
        let mut timer_remaining = 0u64;
        let mut timer_finished = false;

        let mut stopwatch = false;
        let mut watch_running = false;
        let mut watch_elapsed = 0u64;
        let mut watch_updated = now;
        let mut watch_laps = Vec::new();

        for (key, value) in parse_pairs(text) {
            match key.as_str() {
                "app_pid" => {
                    activity.pid = value.parse::<u32>().ok().filter(|pid| *pid > 0);
                }
                "updated" => {
                    if let Ok(updated) = value.parse::<i64>() {
                        activity.updated_unix = updated;
                    }
                }
                "timer" => timer = truthy(&value),
                "timer_total" => timer_total = value.parse::<u64>().unwrap_or(0),
                "timer_running" => timer_running = truthy(&value),
                "timer_end" => timer_end = value.parse::<i64>().unwrap_or(0),
                "timer_remaining" => timer_remaining = value.parse::<u64>().unwrap_or(0),
                "timer_finished" => timer_finished = truthy(&value),
                "stopwatch" => stopwatch = truthy(&value),
                "stopwatch_running" => watch_running = truthy(&value),
                "stopwatch_elapsed_ms" => watch_elapsed = value.parse::<u64>().unwrap_or(0),
                "stopwatch_updated" => {
                    if let Ok(updated) = value.parse::<i64>() {
                        watch_updated = updated;
                    }
                }
                "stopwatch_laps" => {
                    watch_laps = value
                        .split(',')
                        .filter_map(|lap| lap.trim().parse::<u64>().ok())
                        .collect();
                }
                _ => {}
            }
        }

        if timer && timer_total > 0 {
            activity.timer = Some(TimerActivity {
                total_secs: timer_total,
                running: timer_running && timer_end > 0,
                end_unix: timer_end,
                remaining_secs: timer_remaining.min(timer_total),
                finished: timer_finished,
            });
        }
        if stopwatch {
            activity.stopwatch = Some(StopwatchActivity {
                elapsed_millis: watch_elapsed,
                running: watch_running,
                updated_unix: watch_updated,
                laps: watch_laps,
            });
        }
        (activity.pid.is_some() || activity.timer.is_some() || activity.stopwatch.is_some())
            .then_some(activity)
    }
}

fn truthy(value: &str) -> bool {
    matches!(value.trim(), "1" | "true" | "yes" | "on")
}

/// Czy podany PID należy do działającego `luna-clock`.
pub fn pid_is_clock(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}/comm")).exists()
        && std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .map(|name| name.trim() == "luna-clock")
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_timer_and_stopwatch() {
        let activity = ClockActivity {
            pid: Some(42),
            updated_unix: 1_700_000_000,
            timer: Some(TimerActivity {
                total_secs: 1_500,
                running: true,
                end_unix: 1_700_000_742,
                remaining_secs: 0,
                finished: false,
            }),
            stopwatch: Some(StopwatchActivity {
                elapsed_millis: 12_345,
                running: false,
                updated_unix: 1_700_000_000,
                laps: vec![12_000, 34_000],
            }),
        };
        let parsed = ClockActivity::parse(&activity.render()).unwrap();
        assert_eq!(parsed, activity);
    }

    #[test]
    fn timer_and_stopwatch_count_forward() {
        let timer = TimerActivity {
            total_secs: 300,
            running: true,
            end_unix: 1_000,
            remaining_secs: 0,
            finished: false,
        };
        assert_eq!(timer.remaining_at(900), 100);
        assert_eq!(timer.remaining_at(1_000), 0);
        assert!((timer.progress(900) - 0.6667).abs() < 0.01);
        assert!(timer.is_active(900));
        assert!(!timer.is_active(1_000));
        assert!(timer.is_finished(1_000));

        let watch = StopwatchActivity {
            elapsed_millis: 5_000,
            running: true,
            updated_unix: 1_000,
            laps: Vec::new(),
        };
        assert_eq!(watch.elapsed_at(1_002), 7_000);
    }

    #[test]
    fn ignores_invalid_or_empty_state() {
        assert!(ClockActivity::parse("").is_none());
        let parsed = ClockActivity::parse("timer=1\ntimer_total=0\n").unwrap_or_default();
        assert!(parsed.timer.is_none());
        assert_eq!(ClockActivity::parse("app_pid=0\n"), None);
    }

    #[test]
    fn nonexistent_pid_is_not_running() {
        assert!(!pid_is_clock(u32::MAX));
        let activity = ClockActivity {
            pid: Some(u32::MAX),
            ..ClockActivity::default()
        };
        assert!(!activity.is_app_running());
    }
}
