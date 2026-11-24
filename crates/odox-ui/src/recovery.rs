//! What a window has not saved, kept on disk while it runs so that a window
//! that stops without closing leaves it behind to be offered again. DESIGN.md
//! §7.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::fmt::Write as _;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

/// The files one window keeps: the lock it holds while it runs, the
/// document's bytes, and the note of where they came from and when.
struct Files {
    lock: PathBuf,
    bytes: PathBuf,
    note: PathBuf,
}

impl Files {
    fn of(lock: PathBuf) -> Self {
        Self {
            bytes: lock.with_extension("bytes"),
            note: lock.with_extension("note"),
            lock,
        }
    }

    /// Take the document's bytes and its note away, leaving the lock.
    fn clear(&self) {
        let _ = std::fs::remove_file(&self.bytes);
        let _ = std::fs::remove_file(&self.note);
    }
}

/// What the worker is asked to do.
enum Job {
    Save(Vec<u8>, Option<PathBuf>),
    Clear,
    Done(Sender<()>),
}

/// This window's recovery files, which it holds a lock on for as long as it
/// runs; the bytes are written by a thread of their own.
pub struct Recovery {
    _lock: File,
    files: std::sync::Arc<Files>,
    jobs: Option<Sender<Job>>,
    worker: Option<JoinHandle<()>>,
}

impl Recovery {
    /// Begin keeping an application's unsaved changes, in the platform's data
    /// directory.
    ///
    /// # Errors
    ///
    /// The directory cannot be made, or the lock cannot be taken.
    pub fn start(app: &str) -> std::io::Result<Self> {
        Self::start_in(&directory().ok_or(std::io::ErrorKind::NotFound)?, app)
    }

    /// Begin keeping an application's unsaved changes in a directory.
    ///
    /// # Errors
    ///
    /// The directory cannot be made, or the lock cannot be taken.
    pub fn start_in(directory: &Path, app: &str) -> std::io::Result<Self> {
        std::fs::create_dir_all(directory)?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let lock = directory.join(format!("{app}-{}-{stamp}.lock", std::process::id()));
        let file = File::create(&lock)?;
        file.try_lock().map_err(std::io::Error::other)?;
        let files = std::sync::Arc::new(Files::of(lock));
        let (jobs, received) = channel::<Job>();
        let written = std::sync::Arc::clone(&files);
        let worker = std::thread::spawn(move || {
            for job in received {
                match job {
                    Job::Save(bytes, original) => {
                        let _ = write(&written, &bytes, original.as_deref());
                    }
                    Job::Clear => written.clear(),
                    Job::Done(answer) => {
                        let _ = answer.send(());
                    }
                }
            }
        });
        Ok(Self {
            _lock: file,
            files,
            jobs: Some(jobs),
            worker: Some(worker),
        })
    }

    /// Keep the document's bytes, and the file they were read from, which
    /// the worker writes while the window goes on.
    pub fn save(&self, bytes: Vec<u8>, original: Option<&Path>) {
        self.send(Job::Save(bytes, original.map(Path::to_path_buf)));
    }

    /// Forget what was kept: the document was saved, or closed.
    pub fn clear(&self) {
        self.send(Job::Clear);
    }

    /// Wait until the worker has done what it was asked so far.
    pub fn settle(&self) {
        let (answer, done) = channel();
        self.send(Job::Done(answer));
        let _ = done.recv();
    }

    fn send(&self, job: Job) {
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(job);
        }
    }

    /// The window is closing as it should: wait for the worker, and take the
    /// files away.
    pub fn finish(mut self) {
        self.jobs = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.files.clear();
        let _ = std::fs::remove_file(&self.files.lock);
    }
}

/// Write the bytes beside where they go and move them there, then the note,
/// so that a window stopped halfway leaves the last whole copy.
fn write(files: &Files, bytes: &[u8], original: Option<&Path>) -> std::io::Result<()> {
    let part = files.bytes.with_extension("part");
    std::fs::write(&part, bytes)?;
    std::fs::rename(&part, &files.bytes)?;
    let saved = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let mut note = format!("saved = {saved}\n");
    if let Some(original) = original {
        let _ = writeln!(note, "original = {}", original.display());
    }
    std::fs::write(&files.note, note)
}

/// Changes a window that stopped without closing left behind.
pub struct Orphan {
    files: Files,
    /// The file the changes were made to, where it had one.
    pub original: Option<PathBuf>,
    /// When they were kept, in seconds since 1970.
    pub saved: u64,
}

impl Orphan {
    /// The document's bytes as they were kept.
    ///
    /// # Errors
    ///
    /// The file cannot be read.
    pub fn bytes(&self) -> std::io::Result<Vec<u8>> {
        std::fs::read(&self.files.bytes)
    }

    /// Take the changes away.
    pub fn discard(self) {
        self.files.clear();
        let _ = std::fs::remove_file(&self.files.lock);
    }
}

/// The changes an application's windows left behind, in the platform's data
/// directory.
pub fn orphans(app: &str) -> Vec<Orphan> {
    directory().map_or_else(Vec::new, |directory| orphans_in(&directory, app))
}

/// The changes an application's windows left behind in a directory: the
/// files whose lock no running window holds. One that left no bytes is
/// tidied away.
pub fn orphans_in(directory: &Path, app: &str) -> Vec<Orphan> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let lock = entry.path();
        let ours = lock
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(&format!("{app}-")))
            && lock
                .extension()
                .is_some_and(|extension| extension == "lock");
        if !ours {
            continue;
        }
        let Ok(file) = File::options().write(true).open(&lock) else {
            continue;
        };
        if file.try_lock().is_err() {
            continue;
        }
        let files = Files::of(lock);
        let note = std::fs::read_to_string(&files.note).unwrap_or_default();
        if !files.bytes.exists() {
            drop(file);
            files.clear();
            let _ = std::fs::remove_file(&files.lock);
            continue;
        }
        let value = |key: &str| {
            note.lines()
                .find_map(|line| line.strip_prefix(key)?.trim_start().strip_prefix('='))
                .map(str::trim)
        };
        found.push(Orphan {
            original: value("original").map(PathBuf::from),
            saved: value("saved").and_then(|s| s.parse().ok()).unwrap_or(0),
            files,
        });
    }
    found.sort_by_key(|orphan| orphan.saved);
    found
}

/// Where the files are: the platform's data directory, then `odox/recovery`.
fn directory() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?
    } else if cfg!(target_os = "macos") {
        std::env::home_dir()?.join("Library/Application Support")
    } else {
        match std::env::var_os("XDG_DATA_HOME") {
            Some(data) if !data.is_empty() => PathBuf::from(data),
            _ => std::env::home_dir()?.join(".local/share"),
        }
    };
    Some(base.join("odox").join("recovery"))
}

#[cfg(test)]
mod tests {
    use super::{Recovery, orphans_in};
    use std::path::Path;

    fn scratch(name: &str) -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("odox-recovery-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        directory
    }

    #[test]
    fn orphan_found_when_unlocked() {
        let directory = scratch("orphan");
        let recovery = Recovery::start_in(&directory, "xodt").expect("started");
        recovery.save(
            b"kept".to_vec(),
            Some(Path::new("/home/someone/letter.odt")),
        );
        recovery.settle();
        drop(recovery);
        let found = orphans_in(&directory, "xodt");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].bytes().expect("read"), b"kept");
        assert!(orphans_in(&directory, "xods").is_empty());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn held_file_not_offered() {
        let directory = scratch("held");
        let recovery = Recovery::start_in(&directory, "xodt").expect("started");
        recovery.save(b"kept".to_vec(), None);
        recovery.settle();
        assert!(orphans_in(&directory, "xodt").is_empty());
        recovery.finish();
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn finish_removes_file() {
        let directory = scratch("finish");
        let recovery = Recovery::start_in(&directory, "xodt").expect("started");
        recovery.save(b"kept".to_vec(), None);
        recovery.finish();
        let left = std::fs::read_dir(&directory).expect("listed").count();
        assert_eq!(left, 0);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn sidecar_names_original_and_time() {
        let directory = scratch("sidecar");
        let recovery = Recovery::start_in(&directory, "xodt").expect("started");
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after 1970")
            .as_secs();
        recovery.save(
            b"kept".to_vec(),
            Some(Path::new("/home/someone/letter.odt")),
        );
        recovery.settle();
        drop(recovery);
        let found = orphans_in(&directory, "xodt");
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].original.as_deref(),
            Some(Path::new("/home/someone/letter.odt"))
        );
        assert!(found[0].saved >= before);
        let _ = std::fs::remove_dir_all(&directory);
    }
}
