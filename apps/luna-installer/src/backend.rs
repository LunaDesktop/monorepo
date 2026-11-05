//! Uruchamianie backendu instalacji.
//!
//! Aplikacja nie formatuje dysków sama. Przygotowuje plan i uruchamia
//! `luna-install-backend` — program dostarczany przez obraz live (albo wskazany
//! przez `LUNA_INSTALL_BACKEND`). Backend dostaje parametry planu w `argv`,
//! a hasło użytkownika przez **stdin**, żeby nie było widoczne w `ps` ani w
//! dziennikach.
//!
//! Gdy proces nie działa jako root, używamy `pkexec` (agent polkit Luny).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use crate::plan::InstallPlan;

/// Zdarzenie z procesu instalacji.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobEvent {
    Line(String),
    Finished(Result<(), String>),
}

/// Uchwyt do działającej instalacji.
pub struct Job {
    rx: Receiver<JobEvent>,
    pub pid: Option<u32>,
}

impl Job {
    /// Nieblokujący odbiór zdarzeń.
    pub fn drain(&self) -> Vec<JobEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.rx.try_recv() {
            events.push(event);
        }
        events
    }
}

/// Gotowe polecenie do uruchomienia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl SpawnPlan {
    pub fn display(&self) -> String {
        let mut parts = vec![self.program.display().to_string()];
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }
}

/// Szuka backendu: zmienna środowiskowa, `PATH`, katalogi systemowe, katalog
/// obok binarki i (w budowie z repozytorium) `data/` aplikacji.
pub fn locate_backend() -> Option<PathBuf> {
    if let Some(custom) = std::env::var_os("LUNA_INSTALL_BACKEND").filter(|v| !v.is_empty()) {
        let path = PathBuf::from(custom);
        if path.is_file() {
            return Some(path);
        }
    }

    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            candidates.push(dir.join("luna-install-backend"));
        }
    }
    candidates.push(PathBuf::from("/usr/libexec/luna/luna-install-backend"));
    candidates.push(PathBuf::from("/usr/lib/luna/luna-install-backend"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("luna-install-backend"));
            candidates.push(dir.join("../libexec/luna/luna-install-backend"));
            candidates.push(dir.join("../app/luna-installer/luna-install-backend"));
        }
    }
    // Ścieżka deweloperska (działa przy `cargo run` z repozytorium).
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("data/luna-install-backend"));

    candidates.into_iter().find(|path| path.is_file())
}

fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn in_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .any(|candidate| is_executable(&candidate))
}

/// Buduje polecenie bez uruchamiania go (testowalne).
pub fn build_command(
    backend: &Path,
    plan: &InstallPlan,
    root: bool,
    pkexec_available: bool,
    backend_executable: bool,
) -> SpawnPlan {
    let mut args = plan.backend_args();
    let program = if backend_executable {
        backend.to_path_buf()
    } else {
        // Skrypt z repozytorium może nie mieć bitu `x` — uruchamiamy przez `sh`.
        args.insert(0, backend.display().to_string());
        PathBuf::from("/bin/sh")
    };

    if root {
        SpawnPlan { program, args }
    } else if pkexec_available {
        let mut wrapped = Vec::with_capacity(args.len() + 1);
        wrapped.push(program.display().to_string());
        wrapped.extend(args);
        SpawnPlan {
            program: PathBuf::from("pkexec"),
            args: wrapped,
        }
    } else {
        SpawnPlan { program, args }
    }
}

/// Uruchamia instalację. Hasło trafia do `stdin` procesu.
pub fn start(plan: &InstallPlan) -> Result<Job, String> {
    let backend = locate_backend().ok_or_else(|| {
        libluna_i18n::tr(
            "Installation backend not found (luna-install-backend). Run this on the live image.",
        )
    })?;
    // `pkexec` wymaga ścieżki bezwzględnej do programu.
    let backend = std::fs::canonicalize(&backend).unwrap_or(backend);
    let root = unsafe { libc::geteuid() } == 0;
    let pkexec = in_path("pkexec");
    if !root && !pkexec {
        return Err(libluna_i18n::tr(
            "pkexec was not found — the installer needs authorization to write to the disk.",
        ));
    }

    let spawn = build_command(&backend, plan, root, pkexec, is_executable(&backend));
    let mut child = Command::new(&spawn.program)
        .args(&spawn.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            libluna_i18n::tr_args(
                "Could not start the installation backend: {error}",
                &[("error", &err.to_string())],
            )
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        let secrets = plan.secrets();
        thread::spawn(move || {
            for secret in secrets {
                let _ = stdin.write_all(secret.as_bytes());
                let _ = stdin.write_all(b"\n");
            }
            let _ = stdin.flush();
        });
    }

    let (tx, rx) = mpsc::channel();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_tx = tx.clone();
    let out_reader = stdout.map(|stream| {
        thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let _ = out_tx.send(JobEvent::Line(line));
            }
        })
    });
    let err_tx = tx.clone();
    let err_reader = stderr.map(|stream| {
        thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let _ = err_tx.send(JobEvent::Line(line));
            }
        })
    });

    let pid = Some(child.id());
    thread::spawn(move || {
        let status = child.wait();
        if let Some(handle) = out_reader {
            let _ = handle.join();
        }
        if let Some(handle) = err_reader {
            let _ = handle.join();
        }
        let result = match status {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(libluna_i18n::tr_args(
                "The installer finished with an error (exit code {code}).",
                &[("code", &status.code().unwrap_or(-1).to_string())],
            )),
            Err(err) => Err(libluna_i18n::tr_args(
                "Lost contact with the installer: {error}",
                &[("error", &err.to_string())],
            )),
        };
        let _ = tx.send(JobEvent::Finished(result));
    });

    Ok(Job { rx, pid })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk::Disk;
    use crate::plan::{Account, InstallOptions, InstallPlan, RootMode};

    fn plan() -> InstallPlan {
        let mut disk = Disk {
            name: "sda".to_owned(),
            path: "/dev/sda".to_owned(),
            size_bytes: 64_000_000_000,
            model: "Test".to_owned(),
            removable: false,
            free_bytes: 30 * 1024 * 1024 * 1024,
            protection: None,
        };
        disk.free_bytes = 30 * 1024 * 1024 * 1024;
        let account = Account {
            full_name: "Luna Tester".to_owned(),
            username: "luna".to_owned(),
            hostname: "luna-pc".to_owned(),
            password: "tajne-haslo".to_owned(),
            locale: "pl_PL.UTF-8".to_owned(),
            timezone: "Europe/Warsaw".to_owned(),
            keyboard_layout: "pl".to_owned(),
            keyboard_variant: String::new(),
            keyboard_model: "pc105".to_owned(),
            autologin: false,
            root_mode: RootMode::Custom,
            root_password: "root-haslo".to_owned(),
        };
        InstallPlan::new(disk, account, InstallOptions::default())
    }

    #[test]
    fn direct_execution_as_root_keeps_backend_first() {
        let spawn = build_command(Path::new("/usr/bin/backend"), &plan(), true, true, true);
        assert_eq!(spawn.program, PathBuf::from("/usr/bin/backend"));
        assert_eq!(spawn.args.first().map(String::as_str), Some("--mode"));
    }

    #[test]
    fn pkexec_wraps_the_backend() {
        let spawn = build_command(Path::new("/usr/bin/backend"), &plan(), false, true, true);
        assert_eq!(spawn.program, PathBuf::from("pkexec"));
        assert_eq!(
            spawn.args.first().map(String::as_str),
            Some("/usr/bin/backend")
        );
    }

    #[test]
    fn non_executable_script_runs_through_sh() {
        let spawn = build_command(Path::new("/repo/data/backend"), &plan(), true, true, false);
        assert_eq!(spawn.program, PathBuf::from("/bin/sh"));
        assert_eq!(
            spawn.args.first().map(String::as_str),
            Some("/repo/data/backend")
        );
    }

    #[test]
    fn password_is_never_an_argument() {
        let spawn = build_command(Path::new("/usr/bin/backend"), &plan(), true, true, true);
        assert!(!spawn.args.join(" ").contains("tajne-haslo"));
    }
}
