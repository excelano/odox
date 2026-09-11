//! odox: hand a document to whichever of the three viewers reads it.
//!
//! One command for any `OpenDocument` file, for a shell and for a script. It
//! draws nothing itself — it works out which application reads the file it was
//! given and becomes that application.
//!
//! **The applications are the product and this is a convenience over them.** A
//! desktop offers `xodt`, `xods` and `xodp` directly, one per media type, which
//! is how a file manager works and why the packages exist; nothing here is
//! needed for that and nothing there depends on this.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use odox_core::{Package, media_type};

/// The applications this can become, and what each of them reads.
const READS: [(&str, &[&str], &[&str]); 3] = [
    ("xodt", &["odt", "ott", "oth", "fodt"], media_type::TEXT_ANY),
    ("xods", &["ods", "ots", "fods"], media_type::SPREADSHEET_ANY),
    (
        "xodp",
        &["odp", "otp", "fodp"],
        media_type::PRESENTATION_ANY,
    ),
];

fn main() -> ExitCode {
    let mut arguments: Vec<OsString> = std::env::args_os().skip(1).collect();

    if arguments.is_empty() {
        usage();
        return ExitCode::from(2);
    }
    if let Some(first) = arguments[0].to_str() {
        match first {
            "-h" | "--help" => {
                usage();
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("odox {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            // Answering without launching, which is what a script wants and what
            // makes the dispatch checkable by something other than a person
            // watching a window appear.
            "--which" => {
                let Some(path) = arguments.get(1).map(PathBuf::from) else {
                    eprintln!("odox: --which needs a file");
                    return ExitCode::from(2);
                };
                let Some(application) = application_for(&path) else {
                    eprintln!("odox: {}: nothing here reads that", path.display());
                    return ExitCode::from(2);
                };
                println!("{application}");
                return ExitCode::SUCCESS;
            }
            _ => {}
        }
    }

    let path = PathBuf::from(arguments.remove(0));
    if !path.exists() {
        eprintln!("odox: {}: no such file", path.display());
        return ExitCode::from(2);
    }

    let Some(application) = application_for(&path) else {
        eprintln!(
            "odox: {}: not a text document, a spreadsheet or a presentation",
            path.display()
        );
        return ExitCode::from(2);
    };

    launch(application, &path, &arguments)
}

fn usage() {
    println!(
        "\
odox {version} — open an OpenDocument file with the viewer that reads it

    odox FILE [ARGUMENT...]

The file decides: a text document opens in xodt, a spreadsheet in xods, a
presentation in xodp. The extension answers first and the package's own media
type answers where the extension does not, so a file named wrongly, or not
named at all, still opens in the right place.

The viewer is looked for beside this command and then on PATH. Any further
arguments are passed to it.

    --which FILE     name the viewer and launch nothing
    -h, --help       this
    -V, --version    the version",
        version = env!("CARGO_PKG_VERSION")
    );
}

/// Which application reads this file.
///
/// The extension answers first because it is free, it is right for every file an
/// office application wrote, and it is the same answer the desktop's own
/// association gives. Reading the package is the fallback for the file the
/// extension cannot speak for: one named `.zip`, one named nothing, one
/// downloaded under a name a browser invented.
fn application_for(path: &Path) -> Option<&'static str> {
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        let extension = extension.to_ascii_lowercase();
        for (application, extensions, _) in READS {
            if extensions.contains(&extension.as_str()) {
                return Some(application);
            }
        }
    }

    // Reading the whole file to answer one question is more than it takes, and
    // it is the rare path: a file whose name already said what it is never
    // reaches here.
    let bytes = std::fs::read(path).ok()?;
    let package = Package::read(&bytes).ok()?;
    let declared = package.media_type()?;
    READS
        .iter()
        .find(|(_, _, types)| types.contains(&declared))
        .map(|(application, _, _)| *application)
}

/// Become the application, or say why not.
fn launch(application: &str, path: &Path, rest: &[OsString]) -> ExitCode {
    let mut command = Command::new(program(application));
    command.arg(path).args(rest);

    // On Unix this replaces the process, so nothing is left holding a window
    // open on its behalf and the application inherits the terminal, the signals
    // and the exit status directly. `exec` returns only on failure.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let failure = command.exec();
        refuse(application, &failure)
    }

    #[cfg(not(unix))]
    match command.status() {
        Ok(status) => {
            // A status with no code is a signal, which Windows does not have and
            // this arm is Windows alone; the fallback keeps the type honest.
            u8::try_from(status.code().unwrap_or(1)).map_or(ExitCode::FAILURE, ExitCode::from)
        }
        Err(failure) => refuse(application, &failure),
    }
}

fn refuse(application: &str, failure: &std::io::Error) -> ExitCode {
    if failure.kind() == std::io::ErrorKind::NotFound {
        eprintln!(
            "odox: {application} is not installed — it is a package of its own,\n\
             \x20      `apt install {application}` or `cargo install {application}`"
        );
    } else {
        eprintln!("odox: {application} could not be started: {failure}");
    }
    ExitCode::from(127)
}

/// Where to look for the application.
///
///
/// Beside this command first, so that a set installed together — by `cargo
/// install` into one directory, or built into one `target/release` — finds its
/// own siblings rather than an older copy somewhere earlier on PATH. Falling
/// back to the bare name is what finds a system install.
fn program(application: &str) -> PathBuf {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(application)));
    match beside {
        Some(path) if path.is_file() => path,
        _ => PathBuf::from(application),
    }
}

#[cfg(test)]
mod tests {
    use super::application_for;
    use std::path::{Path, PathBuf};

    fn corpus(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus")
            .join(name)
    }

    #[test]
    fn the_extension_decides_where_it_can() {
        // No file needs to exist for this: the name is the whole answer, which
        // is the property that makes the common path free.
        assert_eq!(
            application_for(Path::new("/nowhere/report.odt")),
            Some("xodt")
        );
        assert_eq!(
            application_for(Path::new("/nowhere/accounts.ODS")),
            Some("xods")
        );
        assert_eq!(
            application_for(Path::new("/nowhere/deck.odp")),
            Some("xodp")
        );
        assert_eq!(
            application_for(Path::new("/nowhere/template.ott")),
            Some("xodt")
        );
    }

    #[test]
    fn a_file_named_nothing_useful_is_read_instead() {
        let source = corpus("libreoffice/calc.ods");
        if !source.is_file() {
            return;
        }
        let directory = std::env::temp_dir().join("odox-launcher-test");
        std::fs::create_dir_all(&directory).expect("a directory to work in");

        // A spreadsheet under a name that says nothing, and one under a name
        // that lies. Both are answered by what the package declares.
        for name in ["download", "attachment.bin"] {
            let disguised = directory.join(name);
            std::fs::copy(&source, &disguised).expect("the copy");
            assert_eq!(application_for(&disguised), Some("xods"), "{name}");
        }

        let not_a_package = directory.join("notes.txt");
        std::fs::write(&not_a_package, b"not a package at all").expect("the file");
        assert_eq!(application_for(&not_a_package), None);

        std::fs::remove_dir_all(&directory).ok();
    }
}
