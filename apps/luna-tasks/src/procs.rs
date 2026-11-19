//! Odczyt procesów i statystyk systemu z `/proc`.
//!
//! Wszystko jest lokalne i tylko do odczytu. Parsery są rozdzielone od wejścia
//! (przyjmują tekst), więc testy nie potrzebują prawdziwego `/proc` ani żadnych
//! uprawnień. Aplikacja nigdy nie uruchamia zewnętrznych poleceń.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::time::{Duration, Instant};

/// Sygnał „zakończ się” (domyślna akcja w menedżerze zadań).
pub const SIGTERM: i32 = libc::SIGTERM;
/// Sygnał wymuszony — proces nie może go przechwycić.
pub const SIGKILL: i32 = libc::SIGKILL;

/// Takty zegara na sekundę (`sysconf(_SC_CLK_TCK)`, zwykle 100).
pub fn clock_ticks() -> u64 {
    // SAFETY: `sysconf` tylko czyta stałą systemową i nie wymaga wskaźników.
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks > 0 {
        ticks as u64
    } else {
        100
    }
}

/// Rozmiar strony pamięci w KiB (dla `statm`).
pub fn page_size_kib() -> u64 {
    // SAFETY: jak wyżej — odczyt stałej systemowej.
    let bytes = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if bytes > 0 {
        (bytes as u64 / 1024).max(1)
    } else {
        4
    }
}

/// UID bieżącego użytkownika.
pub fn current_uid() -> u32 {
    // SAFETY: `getuid` nie ma argumentów i nie może zawieść.
    unsafe { libc::getuid() }
}

/// Jeden proces odczytany z `/proc/<pid>`.
#[derive(Debug, Clone, PartialEq)]
pub struct Process {
    pub pid: u32,
    pub ppid: u32,
    /// Nazwa jądra (`comm`) — krótka, bez argumentów.
    pub name: String,
    /// Pełna linia poleceń (pusta dla wątków jądra).
    pub cmdline: String,
    /// Stan procesu (`R`, `S`, `D`, `Z`, `T`, …).
    pub state: char,
    pub uid: u32,
    /// Nazwa użytkownika z `/etc/passwd` (albo numer UID).
    pub user: String,
    /// Suma `utime + stime` w taktach — podstawa wyliczenia CPU.
    pub cpu_jiffies: u64,
    /// Czas startu w taktach od bootu — odróżnia procesy o tym samym PID.
    pub start_jiffies: u64,
    /// Pamięć rezydentna w KiB.
    pub rss_kib: u64,
    pub threads: u32,
    /// Zużycie CPU od poprzedniej próbki (może przekraczać 100%).
    pub cpu_percent: f32,
}

impl Process {
    /// Wiek procesu (od startu) na podstawie uptime systemu.
    pub fn age(&self, uptime: Duration, ticks: u64) -> Duration {
        let started = self.start_jiffies as f64 / ticks.max(1) as f64;
        let age = uptime.as_secs_f64() - started;
        Duration::from_secs_f64(age.max(0.0))
    }
}

/// Klucz tłumaczenia dla stanu procesu.
pub fn state_key(state: char) -> &'static str {
    match state {
        'R' => "Running",
        'S' => "Sleeping",
        'D' => "Disk sleep",
        'Z' => "Zombie",
        'T' | 't' => "Stopped",
        'I' => "Idle",
        _ => "Unknown",
    }
}

/// Pola `/proc/<pid>/stat`, których potrzebujemy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatInfo {
    pub pid: u32,
    pub comm: String,
    pub state: char,
    pub ppid: u32,
    pub utime: u64,
    pub stime: u64,
    pub threads: u32,
    pub starttime: u64,
}

/// Parsuje `/proc/<pid>/stat`.
///
/// Nazwa procesu (`comm`) może zawierać spacje i nawiasy, dlatego dzielimy
/// linię po **ostatnim** `)`, a nie po spacji.
pub fn parse_stat(text: &str) -> Option<StatInfo> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    if close <= open + 1 {
        return None;
    }
    let pid = text.get(..open)?.trim().parse().ok()?;
    let comm = text.get(open + 1..close)?.to_owned();
    let rest: Vec<&str> = text.get(close + 1..)?.split_whitespace().collect();
    Some(StatInfo {
        pid,
        comm,
        state: rest.first()?.chars().next()?,
        ppid: rest.get(1)?.parse().ok()?,
        // Pola 14 i 15 (utime, stime) — po `comm` zaczyna się pole 3.
        utime: rest.get(11)?.parse().ok()?,
        stime: rest.get(12)?.parse().ok()?,
        // Pole 20 (`num_threads`) i 22 (`starttime`).
        threads: rest.get(17)?.parse().ok()?,
        starttime: rest.get(19)?.parse().ok()?,
    })
}

/// Wybrane pola `/proc/<pid>/status`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusInfo {
    /// Rzeczywisty UID (pierwsza wartość z `Uid:`).
    pub uid: Option<u32>,
    pub rss_kib: Option<u64>,
    pub threads: Option<u32>,
}

/// Parsuje `/proc/<pid>/status`.
pub fn parse_status(text: &str) -> StatusInfo {
    let mut info = StatusInfo::default();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("Uid:") {
            info.uid = value.split_whitespace().next().and_then(|v| v.parse().ok());
        } else if let Some(value) = line.strip_prefix("VmRSS:") {
            info.rss_kib = value.split_whitespace().next().and_then(|v| v.parse().ok());
        } else if let Some(value) = line.strip_prefix("Threads:") {
            info.threads = value.split_whitespace().next().and_then(|v| v.parse().ok());
        }
    }
    info
}

/// Liczba stron rezydentnych z `/proc/<pid>/statm`.
pub fn parse_statm_pages(text: &str) -> Option<u64> {
    text.split_whitespace().nth(1)?.parse().ok()
}

/// Linia poleceń z `/proc/<pid>/cmdline` (argumenty rozdzielone NUL).
pub fn parse_cmdline(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace('\0', " ")
        .trim()
        .to_owned()
}

/// Suma i dostępna pamięć w KiB z `/proc/meminfo`.
pub fn parse_meminfo(text: &str) -> (u64, u64) {
    let mut total = 0;
    let mut available = 0;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let amount = value
            .split_whitespace()
            .next()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        match key.trim() {
            "MemTotal" => total = amount,
            "MemAvailable" => available = amount,
            _ => {}
        }
    }
    (total, available)
}

/// Trzy średnie obciążenia z `/proc/loadavg`.
pub fn parse_loadavg(text: &str) -> [f64; 3] {
    let mut load = [0.0; 3];
    for (slot, value) in load.iter_mut().zip(text.split_whitespace()) {
        *slot = value.parse().unwrap_or(0.0);
    }
    load
}

/// Czas działania systemu z `/proc/uptime`.
pub fn parse_uptime(text: &str) -> Duration {
    let secs = text
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    Duration::from_secs_f64(secs.max(0.0))
}

/// Suma taktów wszystkich CPU z pierwszej linii `/proc/stat`.
pub fn parse_cpu_total(text: &str) -> Option<u64> {
    let line = text.lines().next()?;
    let mut fields = line.split_whitespace();
    if fields.next()? != "cpu" {
        return None;
    }
    Some(fields.filter_map(|value| value.parse::<u64>().ok()).sum())
}

/// Mapa UID → nazwa użytkownika z `/etc/passwd`.
pub fn parse_passwd(text: &str) -> HashMap<u32, String> {
    let mut users = HashMap::new();
    for line in text.lines() {
        let mut fields = line.split(':');
        let Some(name) = fields.next().filter(|name| !name.is_empty()) else {
            continue;
        };
        let _password = fields.next();
        let uid = fields.next().and_then(|value| value.parse::<u32>().ok());
        // Pierwsze konto o danym UID wygrywa (tak jak w libc).
        if let Some(uid) = uid {
            users.entry(uid).or_insert_with(|| name.to_owned());
        }
    }
    users
}

/// Statystyki całego systemu pokazywane pod listą.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemInfo {
    pub cpu_percent: f32,
    pub cpu_count: usize,
    pub memory_total_kib: u64,
    pub memory_available_kib: u64,
    pub uptime: Duration,
    pub load: [f64; 3],
}

impl Default for SystemInfo {
    fn default() -> Self {
        Self {
            cpu_percent: 0.0,
            cpu_count: std::thread::available_parallelism()
                .map(|count| count.get())
                .unwrap_or(1),
            memory_total_kib: 0,
            memory_available_kib: 0,
            uptime: Duration::ZERO,
            load: [0.0; 3],
        }
    }
}

impl SystemInfo {
    /// Użyta pamięć w KiB (zabezpieczona przed niedoszacowaniem).
    pub fn memory_used_kib(&self) -> u64 {
        self.memory_total_kib
            .saturating_sub(self.memory_available_kib)
    }
}

/// Migawka `/proc` w jednej chwili.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub processes: Vec<Process>,
    pub system: SystemInfo,
    pub taken: Instant,
    /// Suma taktów wszystkich CPU (do liczenia obciążenia systemu).
    pub total_jiffies: u64,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            processes: Vec::new(),
            system: SystemInfo::default(),
            taken: Instant::now(),
            total_jiffies: 0,
        }
    }
}

impl Snapshot {
    /// Czyta bieżący stan; `prev` służy do wyliczenia procentów CPU.
    pub fn read(prev: Option<&Snapshot>) -> Snapshot {
        let taken = Instant::now();
        let elapsed = prev
            .map(|snapshot| {
                taken
                    .saturating_duration_since(snapshot.taken)
                    .as_secs_f64()
            })
            .filter(|secs| *secs >= 0.05);
        let ticks = clock_ticks() as f64;
        let page_kib = page_size_kib();
        let cpu_count = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1);
        let users = fs::read_to_string("/etc/passwd")
            .map(|text| parse_passwd(&text))
            .unwrap_or_default();

        let previous: HashMap<(u32, u64), u64> = prev
            .map(|snapshot| {
                snapshot
                    .processes
                    .iter()
                    .map(|process| ((process.pid, process.start_jiffies), process.cpu_jiffies))
                    .collect()
            })
            .unwrap_or_default();

        let mut processes = Vec::new();
        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                let Some(pid) = entry
                    .file_name()
                    .to_str()
                    .and_then(|name| name.parse::<u32>().ok())
                else {
                    continue;
                };
                let dir = entry.path();
                let Some(stat) = fs::read_to_string(dir.join("stat"))
                    .ok()
                    .and_then(|text| parse_stat(&text))
                else {
                    // Proces mógł zniknąć między `read_dir` a odczytem.
                    continue;
                };
                let status = fs::read_to_string(dir.join("status"))
                    .map(|text| parse_status(&text))
                    .unwrap_or_default();
                let cmdline = fs::read(dir.join("cmdline"))
                    .map(|bytes| parse_cmdline(&bytes))
                    .unwrap_or_default();
                let uid = status
                    .uid
                    .or_else(|| fs::metadata(&dir).ok().map(|meta| meta.uid()))
                    .unwrap_or(0);
                let rss_kib = status
                    .rss_kib
                    .or_else(|| {
                        fs::read_to_string(dir.join("statm"))
                            .ok()
                            .and_then(|text| parse_statm_pages(&text))
                            .map(|pages| pages.saturating_mul(page_kib))
                    })
                    .unwrap_or(0);

                let jiffies = stat.utime.saturating_add(stat.stime);
                let cpu_percent = previous
                    .get(&(stat.pid, stat.starttime))
                    .zip(elapsed)
                    .map(|(old, seconds)| {
                        let delta = jiffies.saturating_sub(*old) as f64;
                        (delta / ticks / seconds * 100.0).clamp(0.0, 100.0 * cpu_count as f64)
                            as f32
                    })
                    .unwrap_or(0.0);
                let user = users.get(&uid).cloned().unwrap_or_else(|| uid.to_string());
                let name = if stat.comm.is_empty() {
                    cmdline
                        .split(' ')
                        .next()
                        .and_then(|arg| arg.rsplit('/').next())
                        .filter(|arg| !arg.is_empty())
                        .unwrap_or("?")
                        .to_owned()
                } else {
                    stat.comm.clone()
                };

                processes.push(Process {
                    pid,
                    ppid: stat.ppid,
                    name,
                    cmdline,
                    state: stat.state,
                    uid,
                    user,
                    cpu_jiffies: jiffies,
                    start_jiffies: stat.starttime,
                    rss_kib,
                    threads: status.threads.unwrap_or(stat.threads),
                    cpu_percent,
                });
            }
        }
        // Stała kolejność ułatwia testy i stabilny wybór wiersza.
        processes.sort_by_key(|process| process.pid);

        let (memory_total_kib, memory_available_kib) = fs::read_to_string("/proc/meminfo")
            .map(|text| parse_meminfo(&text))
            .unwrap_or((0, 0));
        let total_jiffies = fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|text| parse_cpu_total(&text))
            .unwrap_or(0);
        let system_cpu = prev
            .zip(elapsed)
            .map(|(snapshot, seconds)| {
                if snapshot.total_jiffies == 0 {
                    return 0.0;
                }
                let delta = total_jiffies.saturating_sub(snapshot.total_jiffies) as f64;
                (delta / ticks / seconds / cpu_count as f64 * 100.0).clamp(0.0, 100.0) as f32
            })
            .unwrap_or(0.0);

        Snapshot {
            processes,
            system: SystemInfo {
                cpu_percent: system_cpu,
                cpu_count,
                memory_total_kib,
                memory_available_kib,
                uptime: fs::read_to_string("/proc/uptime")
                    .map(|text| parse_uptime(&text))
                    .unwrap_or_default(),
                load: fs::read_to_string("/proc/loadavg")
                    .map(|text| parse_loadavg(&text))
                    .unwrap_or([0.0; 3]),
            },
            taken,
            total_jiffies,
        }
    }
}

/// Wysyła sygnał do procesu.
///
/// PID 1 jest chroniony — nawet root nie powinien kończyć inita z aplikacji.
pub fn send_signal(pid: u32, signal: i32) -> io::Result<()> {
    if pid <= 1 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to signal pid 1",
        ));
    }
    // SAFETY: `kill` przyjmuje PID i numer sygnału; wynik sprawdzamy poniżej.
    let rc = unsafe { libc::kill(pid as i32, signal) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stat_line() -> &'static str {
        // Kolejne pola po `comm`: state, ppid, pgrp, session, tty, tpgid, flags,
        // minflt, cminflt, majflt, cmajflt, utime(12), stime(34), cutime, cstime,
        // priority, nice, num_threads(8), itrealvalue, starttime(5000), vsize, rss…
        "1234 (Web Content (x)) S 1000 1234 1234 0 -1 4194560 12345 0 0 0 12 34 0 0 20 0 8 0 5000 123456789 4567"
    }

    #[test]
    fn parses_stat_with_spaces_and_parentheses_in_comm() {
        let stat = parse_stat(stat_line()).expect("stat");
        assert_eq!(stat.pid, 1234);
        assert_eq!(stat.comm, "Web Content (x)");
        assert_eq!(stat.state, 'S');
        assert_eq!(stat.ppid, 1000);
        assert_eq!(stat.utime, 12);
        assert_eq!(stat.stime, 34);
        assert_eq!(stat.threads, 8);
        assert_eq!(stat.starttime, 5000);
    }

    #[test]
    fn rejects_truncated_stat() {
        assert!(parse_stat("1234 (firefox) S 1").is_none());
        assert!(parse_stat("").is_none());
    }

    #[test]
    fn parses_status_fields() {
        let status = parse_status(
            "Name:\tfirefox\nUid:\t1000\t1000\t1000\t1000\nVmRSS:\t 524288 kB\nThreads:\t42\n",
        );
        assert_eq!(status.uid, Some(1000));
        assert_eq!(status.rss_kib, Some(524_288));
        assert_eq!(status.threads, Some(42));
    }

    #[test]
    fn parses_statm_and_cmdline() {
        assert_eq!(parse_statm_pages("1000 250 100 20 0 90 0"), Some(250));
        assert_eq!(
            parse_cmdline(b"/usr/bin/firefox\0--new-window\0https://luna.paganlinux.eu\0"),
            "/usr/bin/firefox --new-window https://luna.paganlinux.eu"
        );
        assert_eq!(parse_cmdline(b""), "");
    }

    #[test]
    fn parses_meminfo_load_and_uptime() {
        let (total, available) = parse_meminfo(
            "MemTotal:       16384000 kB\nMemFree:         1234567 kB\nMemAvailable:    8192000 kB\n",
        );
        assert_eq!(total, 16_384_000);
        assert_eq!(available, 8_192_000);

        assert_eq!(
            parse_loadavg("0.52 0.71 0.90 2/345 6789"),
            [0.52, 0.71, 0.90]
        );
        assert_eq!(
            parse_uptime("1234.56 9876.54"),
            Duration::from_secs_f64(1234.56)
        );
    }

    #[test]
    fn parses_total_cpu_jiffies() {
        let total = parse_cpu_total("cpu  100 20 30 400 5 6 7 8 9 10\ncpu0 1 2 3\n");
        assert_eq!(total, Some(100 + 20 + 30 + 400 + 5 + 6 + 7 + 8 + 9 + 10));
        assert_eq!(parse_cpu_total("intr 1 2 3"), None);
    }

    #[test]
    fn parses_passwd_and_keeps_first_entry() {
        let users = parse_passwd(
            "root:x:0:0:root:/root:/bin/bash\npatryk:x:1000:1000::/home/patryk:/bin/sh\nother:x:1000:1000::/:/bin/sh\n",
        );
        assert_eq!(users.get(&0).map(String::as_str), Some("root"));
        assert_eq!(users.get(&1000).map(String::as_str), Some("patryk"));
    }

    #[test]
    fn state_keys_cover_common_states() {
        assert_eq!(state_key('R'), "Running");
        assert_eq!(state_key('S'), "Sleeping");
        assert_eq!(state_key('D'), "Disk sleep");
        assert_eq!(state_key('Z'), "Zombie");
        assert_eq!(state_key('T'), "Stopped");
        assert_eq!(state_key('I'), "Idle");
        assert_eq!(state_key('?'), "Unknown");
    }

    #[test]
    fn process_age_uses_start_time() {
        let process = Process {
            pid: 1,
            ppid: 0,
            name: "init".into(),
            cmdline: String::new(),
            state: 'S',
            uid: 0,
            user: "root".into(),
            cpu_jiffies: 0,
            start_jiffies: 200,
            rss_kib: 0,
            threads: 1,
            cpu_percent: 0.0,
        };
        assert_eq!(
            process.age(Duration::from_secs(100), 100),
            Duration::from_secs(98)
        );
        // Proces „z przyszłości” nie daje ujemnego wieku.
        assert_eq!(process.age(Duration::from_secs(1), 100), Duration::ZERO);
    }

    #[test]
    fn refuses_to_signal_pid_one() {
        assert!(send_signal(1, 0).is_err());
    }

    #[test]
    fn reads_the_live_proc_and_finds_itself() {
        // Test dotyka prawdziwego /proc (aplikacja jest wyłącznie linuksowa):
        // sprawdza, że parser radzi sobie z realnymi danymi i że widzimy własny PID.
        let snapshot = Snapshot::read(None);
        assert!(!snapshot.processes.is_empty());
        assert!(snapshot
            .processes
            .iter()
            .any(|process| process.pid == std::process::id()));
        assert!(snapshot.system.memory_total_kib > 0);
        assert!(snapshot.system.cpu_count >= 1);
        assert!(snapshot.system.uptime > Duration::ZERO);
    }
}
