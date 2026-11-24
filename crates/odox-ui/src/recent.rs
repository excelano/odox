//! The documents an application last opened or saved, newest first, kept one
//! path to a line in `recent-<application>` beside the settings file.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::{Path, PathBuf};
use std::time::SystemTime;

const KEPT: usize = 10;

/// An application's list of recent documents, and the file it is kept in.
///
/// Every window of the application shares the file, so each change reads it
/// again first and writes it whole, and the menu reads it again when it has
/// changed since.
pub struct Recent {
    file: Option<PathBuf>,
    paths: Vec<PathBuf>,
    read_at: Option<SystemTime>,
}

impl Recent {
    /// The list kept for an application, or an empty one where there is none
    /// or it cannot be read. On macOS it is never kept: the sandbox refuses a
    /// path the person did not pick in this session (DESIGN.md §7).
    pub fn load(app: &str) -> Self {
        if cfg!(target_os = "macos") {
            return Self::at(None);
        }
        Self::at(
            crate::settings::path()
                .and_then(|settings| Some(settings.parent()?.join(format!("recent-{app}")))),
        )
    }

    fn at(file: Option<PathBuf>) -> Self {
        let mut recent = Self {
            file,
            paths: Vec::new(),
            read_at: None,
        };
        recent.read();
        recent
    }

    /// A document was opened or saved. A path that cannot be written as one
    /// line of text is not kept.
    pub fn opened(&mut self, path: &Path) {
        let path = std::fs::canonicalize(path)
            .or_else(|_| std::path::absolute(path))
            .unwrap_or_else(|_| path.to_path_buf());
        if path.to_str().is_none_or(|text| text.contains('\n')) {
            return;
        }
        self.read();
        self.paths.retain(|kept| *kept != path);
        self.paths.insert(0, path);
        self.paths.truncate(KEPT);
        self.store();
    }

    /// The documents, newest first.
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Take a document off the list.
    pub fn forget(&mut self, path: &Path) {
        self.read();
        self.paths.retain(|kept| kept != path);
        self.store();
    }

    /// Whether a picked document is still there; one that is not is forgotten.
    pub fn still_there(&mut self, path: &Path) -> bool {
        let there = path.is_file();
        if !there {
            self.forget(path);
        }
        there
    }

    /// Read the list again if another window of the application has changed
    /// it since.
    pub fn refresh(&mut self) {
        if self.modified() != self.read_at {
            self.read();
        }
    }

    /// Empty the list.
    pub fn clear(&mut self) {
        self.paths.clear();
        self.store();
    }

    fn modified(&self) -> Option<SystemTime> {
        std::fs::metadata(self.file.as_ref()?).ok()?.modified().ok()
    }

    fn read(&mut self) {
        self.read_at = self.modified();
        self.paths = self
            .file
            .as_ref()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .map(|text| {
                text.lines()
                    .filter(|line| !line.is_empty())
                    .map(PathBuf::from)
                    .take(KEPT)
                    .collect()
            })
            .unwrap_or_default();
    }

    fn store(&mut self) {
        let Some(file) = &self.file else {
            return;
        };
        let mut text = String::new();
        for path in &self.paths {
            text.push_str(&path.to_string_lossy());
            text.push('\n');
        }
        if let Some(folder) = file.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let _ = std::fs::write(file, text);
        self.read_at = self.modified();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Folder(PathBuf);

    impl std::ops::Deref for Folder {
        type Target = Path;
        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn folder(name: &str) -> Folder {
        let folder =
            std::env::temp_dir().join(format!("odox-recent-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("a folder");
        Folder(std::fs::canonicalize(folder).expect("the folder's own path"))
    }

    fn document(folder: &Path, name: &str) -> PathBuf {
        let path = folder.join(name);
        std::fs::write(&path, b"").expect("a document");
        path
    }

    #[test]
    fn newest_first_at_most_ten() {
        let folder = folder("ten");
        let mut recent = Recent::at(Some(folder.join("recent-xodt")));
        let documents: Vec<PathBuf> = (0..12)
            .map(|n| document(&folder, &format!("{n}.odt")))
            .collect();
        for path in &documents {
            recent.opened(path);
        }
        let expected: Vec<PathBuf> = documents.iter().rev().take(10).cloned().collect();
        assert_eq!(recent.paths(), expected.as_slice());
        let read_again = Recent::at(Some(folder.join("recent-xodt")));
        assert_eq!(read_again.paths(), expected.as_slice());
    }

    #[test]
    fn reopening_moves_to_top() {
        let folder = folder("top");
        let mut recent = Recent::at(Some(folder.join("recent-xodt")));
        let (a, b) = (document(&folder, "a.odt"), document(&folder, "b.odt"));
        recent.opened(&a);
        recent.opened(&b);
        recent.opened(&a);
        assert_eq!(recent.paths(), [a, b].as_slice());
    }

    #[test]
    fn missing_file_is_forgotten_when_picked() {
        let folder = folder("missing");
        let mut recent = Recent::at(Some(folder.join("recent-xodt")));
        let (a, b) = (document(&folder, "a.odt"), document(&folder, "b.odt"));
        recent.opened(&a);
        recent.opened(&b);
        std::fs::remove_file(&a).expect("removed");
        assert!(recent.still_there(&b));
        assert!(!recent.still_there(&a));
        assert_eq!(recent.paths(), [b].as_slice());
    }

    #[test]
    fn unreadable_file_reads_as_empty() {
        let folder = folder("unreadable");
        std::fs::write(folder.join("recent-xodt"), [0xff, 0xfe, 0x00]).expect("written");
        let recent = Recent::at(Some(folder.join("recent-xodt")));
        assert_eq!(recent.paths(), &[] as &[PathBuf]);
        let nowhere = Recent::at(None);
        assert_eq!(nowhere.paths(), &[] as &[PathBuf]);
    }

    #[test]
    fn two_windows_keep_each_others_documents() {
        let folder = folder("two");
        let file = folder.join("recent-xodt");
        let (mut one, mut other) = (
            Recent::at(Some(file.clone())),
            Recent::at(Some(file.clone())),
        );
        let [a, b, c] = ["a.odt", "b.odt", "c.odt"].map(|name| document(&folder, name));
        one.opened(&a);
        other.opened(&b);
        one.opened(&c);
        assert_eq!(Recent::at(Some(file)).paths(), [c, b, a].as_slice());
    }

    #[test]
    fn refresh_sees_another_windows_list() {
        let folder = folder("refresh");
        let file = folder.join("recent-xodt");
        let (mut one, mut other) = (Recent::at(Some(file.clone())), Recent::at(Some(file)));
        let b = document(&folder, "b.odt");
        other.opened(&b);
        one.refresh();
        assert_eq!(one.paths(), [b].as_slice());
    }

    #[test]
    fn a_path_through_dot_dot_is_kept_as_where_it_leads() {
        let folder = folder("dots");
        std::fs::create_dir_all(folder.join("sub")).expect("a folder");
        let a = document(&folder, "a.odt");
        let mut recent = Recent::at(Some(folder.join("recent-xodt")));
        recent.opened(&folder.join("sub/../a.odt"));
        assert_eq!(recent.paths(), [a].as_slice());
    }
}
