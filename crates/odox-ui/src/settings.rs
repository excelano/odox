//! The one preference the suite keeps, and the file it keeps it in.
//!
//! The file is written only when a person changes a preference in the menu,
//! so an installation nobody has configured has no file, and the privacy
//! statement can say what is there and when. It is shared by the three
//! applications, because a preference about editing is about the suite.
//!
//! The format is one `key = value` line per setting, which is also valid TOML,
//! read and written here rather than through a parser: there is one key.
//! DESIGN.md §11.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::PathBuf;

/// What a person has chosen.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Open every document in edit mode rather than reading.
    pub open_in_edit_mode: bool,
}

const OPEN_IN_EDIT_MODE: &str = "open-in-edit-mode";

impl Settings {
    /// Read the file, or the defaults where there is none or it cannot be
    /// read. A settings file that cannot be read is not a reason to refuse to
    /// open a window.
    pub fn load() -> Self {
        path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map_or_else(Self::default, |text| Self::parse(&text))
    }

    /// Write the file, creating its directory.
    ///
    /// # Errors
    ///
    /// The directory could not be made or the file could not be written, or
    /// there is no place to put it, which is a machine with no home directory.
    pub fn save(&self) -> std::io::Result<()> {
        let path = path().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "no configuration directory")
        })?;
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory)?;
        }
        std::fs::write(path, self.to_string())
    }

    fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim() == OPEN_IN_EDIT_MODE {
                settings.open_in_edit_mode = value.trim() == "true";
            }
        }
        settings
    }
}

impl std::fmt::Display for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{OPEN_IN_EDIT_MODE} = {}", self.open_in_edit_mode)
    }
}

/// Where the file is: the platform's configuration directory, then `odox`.
///
/// Linux follows the XDG base directory specification, macOS puts an
/// application's files under `Library/Application Support` inside the sandbox
/// container the bundle identifier keys, and Windows uses roaming
/// `AppData`. Each is read from the environment and the home directory rather
/// than through a crate, because there is one file and three answers.
pub fn path() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)?
    } else if cfg!(target_os = "macos") {
        std::env::home_dir()?.join("Library/Application Support")
    } else {
        match std::env::var_os("XDG_CONFIG_HOME") {
            Some(config) if !config.is_empty() => PathBuf::from(config),
            _ => std::env::home_dir()?.join(".config"),
        }
    };
    Some(base.join("odox").join("settings.toml"))
}

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn what_is_written_is_what_is_read() {
        let on = Settings {
            open_in_edit_mode: true,
        };
        assert_eq!(Settings::parse(&on.to_string()), on);
        assert_eq!(
            Settings::parse(&Settings::default().to_string()),
            Settings::default()
        );
    }

    #[test]
    fn a_file_from_a_later_version_is_read_for_what_this_one_knows() {
        let text = "# a comment\nopen-in-edit-mode = true\nsomething-newer = 3\n\n";
        assert!(Settings::parse(text).open_in_edit_mode);
        assert!(!Settings::parse("").open_in_edit_mode);
        assert!(!Settings::parse("open-in-edit-mode = yes").open_in_edit_mode);
    }
}
