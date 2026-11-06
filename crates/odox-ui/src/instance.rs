//! Another copy of the running application, which is how File > New opens a
//! window when this one already holds a document.
//!
//! On Linux and Windows the copy is a child process, and inside an MSIX package
//! the child keeps the package's identity. On macOS it cannot be: a sandboxed
//! process may not start an executable signed with the full sandbox
//! entitlement, and the child is killed with SIGILL in `_libsecinit_appsandbox`
//! before `main` while the parent's `spawn` reports success. A bundle there asks
//! `NSWorkspace` for a new instance of itself instead, which Launch Services
//! starts as Finder would, with a sandbox of its own. Launch Services drops
//! the arguments a sandboxed application gives the instance it asks for, so the
//! request for a new document goes as an empty marker file in the temporary
//! directory, which two instances of one sandboxed application share: the
//! instance claims one by deleting it, and a marker older than half a minute,
//! or one whose launch failed, is not a request. An executable outside a
//! bundle, as `cargo run` starts one, has nothing to ask for and is started as
//! a child.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui;

/// Start another copy of this application, given `argument`.
///
/// # Errors
///
/// The executable could not be found or started. A refusal macOS gives only
/// after this has returned comes from [`failure`] instead.
pub fn start(ctx: &egui::Context, argument: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    if let Some(bundle) = macos::bundle() {
        macos::open(ctx, &bundle);
        return Ok(());
    }
    let _ = ctx;
    let program = std::env::current_exe()?;
    std::process::Command::new(program)
        .arg(argument)
        .spawn()
        .map(drop)
}

/// Whether this process is a copy started for a new document where the
/// platform could not say so in its arguments.
pub fn asked_for() -> bool {
    #[cfg(target_os = "macos")]
    return macos::claim();
    #[cfg(not(target_os = "macos"))]
    false
}

/// Why the copy last asked for did not start, once, where the platform says so
/// only after [`start`] has returned.
pub fn failure() -> Option<String> {
    #[cfg(target_os = "macos")]
    return macos::failure();
    #[cfg(not(target_os = "macos"))]
    None
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos {
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use block2::RcBlock;
    use eframe::egui;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration};
    use objc2_foundation::{NSBundle, NSError, NSURL};

    static FAILURE: Mutex<Option<String>> = Mutex::new(None);

    /// How long a marker stays a request.
    const FRESH: Duration = Duration::from_secs(30);

    /// What this application's markers are named from, which keeps the three
    /// applications from claiming each other's where they share a directory.
    fn prefix() -> String {
        let name = std::env::current_exe()
            .ok()
            .and_then(|program| {
                program
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
            })
            .unwrap_or_default();
        format!("{name}-new-")
    }

    fn marker() -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("{}{now}", prefix()))
    }

    /// Take one fresh marker, and clear away any that are stale.
    pub(super) fn claim() -> bool {
        let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
            return false;
        };
        let prefix = prefix();
        let mut claimed = false;
        for entry in entries.flatten() {
            if !entry.file_name().to_string_lossy().starts_with(&prefix) {
                continue;
            }
            let fresh = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age < FRESH);
            if fresh && claimed {
                continue;
            }
            let removed = std::fs::remove_file(entry.path()).is_ok();
            claimed |= fresh && removed;
        }
        claimed
    }

    /// The bundle this process runs from, if it runs from one.
    pub(super) fn bundle() -> Option<Retained<NSURL>> {
        let bundle = NSBundle::mainBundle();
        std::path::Path::new(&bundle.bundlePath().to_string())
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
            .then(|| bundle.bundleURL())
    }

    /// Ask Launch Services for a new instance of `bundle`, leaving a marker for
    /// it to claim.
    ///
    /// The completion handler is the one `unsafe`: Launch Services hands it
    /// either null or an error that stays alive for the length of the call, so
    /// the pointer is read as an optional reference and nothing is kept of it
    /// but its description.
    pub(super) fn open(ctx: &egui::Context, bundle: &NSURL) {
        let configuration = NSWorkspaceOpenConfiguration::configuration();
        configuration.setCreatesNewApplicationInstance(true);
        let marker = marker();
        let _ = std::fs::write(&marker, b"");
        let ctx = ctx.clone();
        let finished = RcBlock::new(move |_: *mut NSRunningApplication, error: *mut NSError| {
            if let Some(error) = unsafe { error.as_ref() } {
                let _ = std::fs::remove_file(&marker);
                if let Ok(mut failure) = FAILURE.lock() {
                    *failure = Some(error.localizedDescription().to_string());
                }
                ctx.request_repaint();
            }
        });
        NSWorkspace::sharedWorkspace().openApplicationAtURL_configuration_completionHandler(
            bundle,
            &configuration,
            Some(&finished),
        );
    }

    pub(super) fn failure() -> Option<String> {
        FAILURE.lock().ok()?.take()
    }
}
