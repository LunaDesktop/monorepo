//! Operacje na plikach w tle: kolejka, postęp, anulowanie.
//!
//! UI nigdy nie kopiuje ani nie usuwa synchronicznie — wysyła [`Job`] do wątku
//! roboczego i dostaje [`OpsEvent`] (postęp, wynik). Dzięki temu duże pliki nie
//! zamrażają okna.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use libluna_i18n::tr;

use crate::trash;

/// Zadanie do wykonania w tle.
#[derive(Debug, Clone)]
pub enum Job {
    Copy {
        srcs: Vec<PathBuf>,
        dst: PathBuf,
    },
    Move {
        srcs: Vec<PathBuf>,
        dst: PathBuf,
    },
    Trash {
        paths: Vec<PathBuf>,
    },
    Delete {
        paths: Vec<PathBuf>,
    },
    Rename {
        from: PathBuf,
        to: PathBuf,
    },
    NewFolder {
        parent: PathBuf,
    },
    Mkdir {
        path: PathBuf,
    },
    Restore {
        path: PathBuf,
    },
    Purge {
        path: PathBuf,
    },
    /// Pakuje `items` do archiwum `dest` (format wynika z rozszerzenia).
    Compress {
        items: Vec<PathBuf>,
        dest: PathBuf,
    },
    /// Wypakowuje `archive` do katalogu `dest`.
    Extract {
        archive: PathBuf,
        dest: PathBuf,
    },
}

/// Decyzja przy konflikcie nazw (dialog w UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Overwrite,
    Skip,
    Rename,
    OverwriteAll,
    SkipAll,
    RenameAll,
}

/// Zdarzenia z wątku operacji.
#[derive(Debug, Clone)]
pub enum OpsEvent {
    Started {
        label: String,
    },
    Progress {
        done: u64,
        total: u64,
        current: String,
    },
    /// Cel już istnieje — UI pokazuje dialog i wysyła [`Decision`].
    Conflict {
        src: PathBuf,
        dst: PathBuf,
        is_dir: bool,
    },
    Finished {
        label: String,
        ok: usize,
        skipped: usize,
        failed: Vec<String>,
        created: Option<PathBuf>,
    },
}

/// Uchwyt do kolejki operacji (wątek roboczy żyje do końca procesu).
pub struct Ops {
    tx: Sender<Job>,
    rx: Receiver<OpsEvent>,
    decisions: Sender<Decision>,
    cancel: Arc<AtomicBool>,
    busy: bool,
}

impl Ops {
    pub fn new() -> Self {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (ev_tx, ev_rx) = mpsc::channel::<OpsEvent>();
        let (decision_tx, decision_rx) = mpsc::channel::<Decision>();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        std::thread::Builder::new()
            .name("luna-files-ops".into())
            .spawn(move || {
                while let Ok(job) = job_rx.recv() {
                    flag.store(false, Ordering::SeqCst);
                    run(job, &ev_tx, &flag, &decision_rx);
                }
            })
            .expect("wątek operacji");
        Self {
            tx: job_tx,
            rx: ev_rx,
            decisions: decision_tx,
            cancel,
            busy: false,
        }
    }

    /// Dodaje zadanie; `false`, gdy poprzednie jeszcze trwa.
    pub fn submit(&mut self, job: Job) -> bool {
        if self.busy {
            return false;
        }
        if self.tx.send(job).is_ok() {
            self.busy = true;
            true
        } else {
            false
        }
    }

    /// Odbiera zdarzenia (do wywołania raz na klatkę).
    pub fn poll(&mut self) -> Vec<OpsEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = self.rx.try_recv() {
            if matches!(ev, OpsEvent::Finished { .. }) {
                self.busy = false;
            }
            out.push(ev);
        }
        out
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// Odpowiedź na dialog konfliktu.
    pub fn decide(&self, decision: Decision) {
        let _ = self.decisions.send(decision);
    }

    pub fn busy(&self) -> bool {
        self.busy
    }
}

impl Default for Ops {
    fn default() -> Self {
        Self::new()
    }
}

struct Ctx<'a> {
    tx: &'a Sender<OpsEvent>,
    cancel: &'a AtomicBool,
    done: u64,
    total: u64,
    last: Instant,
}

impl Ctx<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    fn progress(&mut self, current: &Path, force: bool) {
        if force || self.last.elapsed() >= Duration::from_millis(100) {
            self.last = Instant::now();
            let _ = self.tx.send(OpsEvent::Progress {
                done: self.done,
                total: self.total,
                current: current
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            });
        }
    }
}

fn run(job: Job, tx: &Sender<OpsEvent>, cancel: &AtomicBool, decisions: &Receiver<Decision>) {
    let label = tr(match &job {
        Job::Copy { .. } => "Copying",
        Job::Move { .. } => "Moving",
        Job::Trash { .. } => "Moving to Trash",
        Job::Delete { .. } => "Deleting",
        Job::Rename { .. } => "Renaming",
        Job::NewFolder { .. } => "Creating folder",
        Job::Mkdir { .. } => "Creating folder",
        Job::Restore { .. } => "Restoring",
        Job::Purge { .. } => "Deleting",
        Job::Compress { .. } => "Creating archive",
        Job::Extract { .. } => "Extracting",
    });
    let _ = tx.send(OpsEvent::Started {
        label: label.clone(),
    });

    let mut ok = 0usize;
    let mut skipped = 0usize;
    let mut failed = Vec::new();
    let mut created = None;

    match job {
        Job::Copy { srcs, dst } => {
            let total = srcs.iter().map(|p| path_size(p)).sum();
            let mut ctx = Ctx {
                tx,
                cancel,
                done: 0,
                total,
                last: Instant::now(),
            };
            let mut policy = Policy::Ask;
            for src in &srcs {
                if ctx.cancelled() {
                    failed.push(tr("Cancelled"));
                    break;
                }
                let Some(name) = src.file_name() else {
                    continue;
                };
                let Some(dest) = resolve_dest(name, &dst, &mut policy, tx, decisions, cancel)
                else {
                    skipped += 1;
                    continue;
                };
                match copy_path(src, &dest, &mut ctx) {
                    Ok(()) => ok += 1,
                    Err(e) => failed.push(format!("{}: {e}", src.display())),
                }
            }
        }
        Job::Move { srcs, dst } => {
            let mut policy = Policy::Ask;
            for src in &srcs {
                if cancel.load(Ordering::SeqCst) {
                    failed.push(tr("Cancelled"));
                    break;
                }
                let Some(name) = src.file_name() else {
                    continue;
                };
                let Some(dest) = resolve_dest(name, &dst, &mut policy, tx, decisions, cancel)
                else {
                    skipped += 1;
                    continue;
                };
                // Przy „nadpisz” stary cel musi zniknąć (rename nie nadpisze katalogu).
                if dest.symlink_metadata().is_ok() {
                    let _ = remove_path(&dest);
                }
                match move_path(src, &dest) {
                    Ok(()) => ok += 1,
                    Err(e) => failed.push(format!("{}: {e}", src.display())),
                }
            }
        }
        Job::Trash { paths } => {
            for path in paths {
                match trash::move_to_trash(&path) {
                    Ok(_) => ok += 1,
                    Err(e) => failed.push(format!("{}: {e}", path.display())),
                }
            }
        }
        Job::Delete { paths } => {
            for path in paths {
                match remove_path(&path) {
                    Ok(()) => ok += 1,
                    Err(e) => failed.push(format!("{}: {e}", path.display())),
                }
            }
        }
        Job::Rename { from, to } => {
            if to.exists() {
                failed.push(tr("A file with that name already exists"));
            } else {
                match fs::rename(&from, &to) {
                    Ok(()) => ok = 1,
                    Err(e) => failed.push(format!("{}: {e}", from.display())),
                }
            }
        }
        Job::NewFolder { parent } => {
            let dest = unique_dest(&parent, std::ffi::OsStr::new(&tr("New Folder")));
            match fs::create_dir(&dest) {
                Ok(()) => {
                    ok = 1;
                    created = Some(dest);
                }
                Err(e) => failed.push(format!("{}: {e}", dest.display())),
            }
        }
        Job::Mkdir { path } => match fs::create_dir(&path) {
            Ok(()) => {
                ok = 1;
                created = Some(path);
            }
            Err(e) => failed.push(format!("{}: {e}", path.display())),
        },
        Job::Restore { path } => match trash::restore(&path) {
            Ok(dest) => {
                ok = 1;
                created = Some(dest);
            }
            Err(e) => failed.push(format!("{}: {e}", path.display())),
        },
        Job::Purge { path } => match trash::purge(&path) {
            Ok(()) => ok = 1,
            Err(e) => failed.push(format!("{}: {e}", path.display())),
        },
        Job::Compress { items, dest } => {
            let _ = tx.send(OpsEvent::Progress {
                done: 0,
                total: 0,
                current: dest
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            });
            match crate::archive::compress(&items, &dest, cancel) {
                Ok(()) => {
                    ok = 1;
                    created = Some(dest);
                }
                Err(e) => failed.push(format!("{}: {e}", dest.display())),
            }
        }
        Job::Extract { archive, dest } => {
            let _ = tx.send(OpsEvent::Progress {
                done: 0,
                total: 0,
                current: archive
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            });
            match crate::archive::extract(&archive, &dest, cancel) {
                Ok(()) => {
                    ok = 1;
                    created = Some(dest);
                }
                Err(e) => failed.push(format!("{}: {e}", archive.display())),
            }
        }
    }

    let _ = tx.send(OpsEvent::Finished {
        label,
        ok,
        skipped,
        failed,
        created,
    });
}

/// Polityka rozwiązywania konfliktów w obrębie jednego zadania.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Policy {
    Ask,
    Overwrite,
    Skip,
    Rename,
}

/// Ustala nazwę docelową; przy istniejącym celu pyta UI ([`OpsEvent::Conflict`]).
fn resolve_dest(
    name: &std::ffi::OsStr,
    dst_dir: &Path,
    policy: &mut Policy,
    tx: &Sender<OpsEvent>,
    decisions: &Receiver<Decision>,
    cancel: &AtomicBool,
) -> Option<PathBuf> {
    let dest = dst_dir.join(name);
    if dest.symlink_metadata().is_err() {
        return Some(dest);
    }
    loop {
        match *policy {
            Policy::Overwrite => return Some(dest),
            Policy::Skip => return None,
            Policy::Rename => return Some(unique_dest(dst_dir, name)),
            Policy::Ask => {
                let is_dir = fs::symlink_metadata(&dest)
                    .map(|m| m.is_dir())
                    .unwrap_or(false);
                let _ = tx.send(OpsEvent::Conflict {
                    src: dst_dir.join(name),
                    dst: dest.clone(),
                    is_dir,
                });
                let decision = loop {
                    if cancel.load(Ordering::SeqCst) {
                        return None;
                    }
                    match decisions.recv_timeout(Duration::from_millis(200)) {
                        Ok(d) => break d,
                        Err(RecvTimeoutError::Timeout) => continue,
                        Err(RecvTimeoutError::Disconnected) => return None,
                    }
                };
                match decision {
                    Decision::Overwrite => return Some(dest),
                    Decision::Skip => return None,
                    Decision::Rename => return Some(unique_dest(dst_dir, name)),
                    Decision::OverwriteAll => {
                        *policy = Policy::Overwrite;
                        return Some(dest);
                    }
                    Decision::SkipAll => {
                        *policy = Policy::Skip;
                        return None;
                    }
                    Decision::RenameAll => {
                        *policy = Policy::Rename;
                        return Some(unique_dest(dst_dir, name));
                    }
                }
            }
        }
    }
}

/// Kopiuje plik/katalog, raportując postęp (256 KiB na chunk).
fn copy_path(src: &Path, dst: &Path, ctx: &mut Ctx<'_>) -> io::Result<()> {
    if ctx.cancelled() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
    }
    let meta = fs::symlink_metadata(src)?;
    if meta.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_path(&entry.path(), &dst.join(entry.file_name()), ctx)?;
        }
        Ok(())
    } else if meta.file_type().is_symlink() {
        let target = fs::read_link(src)?;
        std::os::unix::fs::symlink(target, dst)
    } else {
        let mut input = fs::File::open(src)?;
        let mut output = fs::File::create(dst)?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            if ctx.cancelled() {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            output.write_all(&buf[..n])?;
            ctx.done += n as u64;
            ctx.progress(dst, false);
        }
        Ok(())
    }
}

/// Przenosi plik/katalog (rename, a przy `EXDEV` kopiuje i usuwa oryginał).
fn move_path(src: &Path, dst: &Path) -> io::Result<()> {
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            copy_tree(src, dst)?;
            remove_path(src)
        }
        Err(e) => Err(e),
    }
}

fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(src)?;
    if meta.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_tree(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dst).map(|_| ())
    }
}

pub(crate) fn remove_path(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn path_size(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_dir() {
        let mut sum = 0;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                sum += path_size(&entry.path());
            }
        }
        sum
    } else {
        meta.len()
    }
}

/// Wolna nazwa w katalogu docelowym: `raport.pdf`, `raport (1).pdf`, …
pub(crate) fn unique_dest(dir: &Path, name: &std::ffi::OsStr) -> PathBuf {
    let candidate = dir.join(name);
    if candidate.symlink_metadata().is_err() {
        return candidate;
    }
    let name = name.to_string_lossy();
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_owned(), format!(".{e}")),
        _ => (name.into_owned(), String::new()),
    };
    let mut n = 1u32;
    loop {
        let candidate = dir.join(format!("{stem} ({n}){ext}"));
        if candidate.symlink_metadata().is_err() {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "luna-files-ops-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn wait(ops: &mut Ops) -> OpsEvent {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            for ev in ops.poll() {
                if matches!(ev, OpsEvent::Finished { .. }) {
                    return ev;
                }
            }
            assert!(
                Instant::now() < deadline,
                "operacja nie skończyła się w czasie"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn unique_dest_adds_number() {
        let dir = temp_dir("unique");
        fs::write(dir.join("a.txt"), "x").unwrap();
        let dest = unique_dest(&dir, std::ffi::OsStr::new("a.txt"));
        assert_eq!(dest.file_name().unwrap(), "a (1).txt");
    }

    #[test]
    fn copy_job_copies_file_content() {
        let dir = temp_dir("copy");
        fs::write(dir.join("src.txt"), "abc").unwrap();
        fs::create_dir(dir.join("out")).unwrap();
        let mut ops = Ops::new();
        assert!(ops.submit(Job::Copy {
            srcs: vec![dir.join("src.txt")],
            dst: dir.join("out"),
        }));
        match wait(&mut ops) {
            OpsEvent::Finished { ok, failed, .. } => {
                assert_eq!(ok, 1, "{failed:?}");
                assert!(failed.is_empty());
            }
            _ => unreachable!(),
        }
        assert_eq!(fs::read_to_string(dir.join("out/src.txt")).unwrap(), "abc");
    }

    #[test]
    fn conflict_policy_can_skip_rename_or_overwrite() {
        let dir = temp_dir("conflict-policy");
        let dst = dir.join("out");
        fs::create_dir(&dst).unwrap();
        fs::write(dst.join("a.txt"), "stary").unwrap();
        let (tx, _rx) = mpsc::channel();
        let (_dtx, drx) = mpsc::channel();
        let cancel = AtomicBool::new(false);

        let mut policy = Policy::Skip;
        assert!(resolve_dest(
            std::ffi::OsStr::new("a.txt"),
            &dst,
            &mut policy,
            &tx,
            &drx,
            &cancel
        )
        .is_none());

        policy = Policy::Rename;
        let renamed = resolve_dest(
            std::ffi::OsStr::new("a.txt"),
            &dst,
            &mut policy,
            &tx,
            &drx,
            &cancel,
        )
        .unwrap();
        assert_eq!(renamed.file_name().unwrap(), "a (1).txt");

        policy = Policy::Overwrite;
        let replaced = resolve_dest(
            std::ffi::OsStr::new("a.txt"),
            &dst,
            &mut policy,
            &tx,
            &drx,
            &cancel,
        )
        .unwrap();
        assert_eq!(replaced, dst.join("a.txt"));
    }

    #[test]
    fn new_folder_job_reports_created_path() {
        let dir = temp_dir("mkdir");
        let mut ops = Ops::new();
        assert!(ops.submit(Job::NewFolder {
            parent: dir.clone(),
        }));
        match wait(&mut ops) {
            OpsEvent::Finished { created, .. } => {
                let created = created.expect("ścieżka nowego katalogu");
                assert!(created.is_dir());
            }
            _ => unreachable!(),
        }
    }
}
