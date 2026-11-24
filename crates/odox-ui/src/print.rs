//! File > Print: the document made into a PDF on a thread of its own and
//! handed to whatever prints on the platform. On Linux that is the desktop
//! portal's print dialog; where there is none, and on Windows and macOS, the
//! PDF is opened in the default viewer, where it is printed. DESIGN.md §12.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::{Path, PathBuf};

/// How a PDF was handed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Printed {
    /// The print dialog took it.
    Sent,
    /// The person closed the print dialog without printing.
    Cancelled,
    /// It was opened in the default viewer.
    Opened,
}

/// What hands a written PDF over, with the title to print it under.
pub type HandOver = fn(&Path, &str) -> std::io::Result<Printed>;

/// Where the PDF to print is written: the temporary directory, in a folder of
/// this process's own, under the document's name.
pub fn temporary_pdf(stem: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!("odox-print-{}", std::process::id()))
        .join(format!("{stem}.pdf"))
}

/// Hand a PDF to the platform to print.
///
/// # Errors
///
/// Neither the portal nor the default viewer took it.
pub fn hand_over(pdf: &Path, title: &str) -> std::io::Result<Printed> {
    #[cfg(target_os = "linux")]
    if let Ok(printed) = portal::print(pdf, title) {
        return Ok(printed);
    }
    let _ = title;
    open_in_viewer(pdf).map(|()| Printed::Opened)
}

/// Open a file in the application the desktop opens it with.
fn open_in_viewer(pdf: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::{NSString, NSURL};
        let url = NSURL::fileURLWithPath(&NSString::from_str(&pdf.to_string_lossy()));
        if NSWorkspace::sharedWorkspace().openURL(&url) {
            Ok(())
        } else {
            Err(std::io::Error::other("no application opens PDF files"))
        }
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg(pdf)
            .spawn()
            .map(drop)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(pdf)
            .spawn()
            .map(drop)
    }
}

#[cfg(target_os = "linux")]
mod portal {
    use std::collections::HashMap;
    use std::os::fd::AsFd;
    use std::path::Path;

    use zbus::blocking::{Connection, Proxy};
    use zbus::zvariant::{Fd, OwnedObjectPath, OwnedValue, Value};

    use super::Printed;

    const PORTAL: &str = "org.freedesktop.portal.Desktop";
    const PATH: &str = "/org/freedesktop/portal/desktop";
    const PRINT: &str = "org.freedesktop.portal.Print";
    const REQUEST: &str = "org.freedesktop.portal.Request";

    type Results = HashMap<String, OwnedValue>;

    /// Put a request to the portal and wait for its answer. The answer is
    /// listened for before the request is put, at the path the token makes,
    /// so it cannot arrive first.
    fn ask(
        connection: &Connection,
        token: &str,
        put: impl FnOnce(&Proxy<'_>) -> zbus::Result<OwnedObjectPath>,
    ) -> zbus::Result<(u32, Results)> {
        let sender = connection
            .unique_name()
            .map(|name| name.trim_start_matches(':').replace('.', "_"))
            .unwrap_or_default();
        let path = format!("{PATH}/request/{sender}/{token}");
        let request = Proxy::new(connection, PORTAL, path, REQUEST)?;
        let mut answers = request.receive_signal("Response")?;
        let print = Proxy::new(connection, PORTAL, PATH, PRINT)?;
        put(&print)?;
        let answer = answers
            .next()
            .ok_or_else(|| zbus::Error::Failure("the portal did not answer".to_owned()))?;
        answer.body().deserialize::<(u32, Results)>()
    }

    /// Ask the portal's print dialog how to print, and print the PDF there.
    pub(super) fn print(pdf: &Path, title: &str) -> zbus::Result<Printed> {
        let connection = Connection::session()?;
        let stamp = std::process::id();
        let prepare = format!("odox_prepare_{stamp}");
        let options: HashMap<&str, Value<'_>> =
            HashMap::from([("handle_token", Value::from(prepare.as_str()))]);
        let empty: HashMap<&str, Value<'_>> = HashMap::new();
        let (response, results) = ask(&connection, &prepare, |print| {
            print.call("PreparePrint", &("", title, &empty, &empty, &options))
        })?;
        if response != 0 {
            return Ok(Printed::Cancelled);
        }
        let token: u32 = results
            .get("token")
            .and_then(|token| u32::try_from(token).ok())
            .ok_or_else(|| zbus::Error::Failure("the portal gave no token".to_owned()))?;
        let file = std::fs::File::open(pdf).map_err(|e| zbus::Error::Failure(e.to_string()))?;
        let send = format!("odox_print_{stamp}");
        let options: HashMap<&str, Value<'_>> = HashMap::from([
            ("handle_token", Value::from(send.as_str())),
            ("token", Value::from(token)),
        ]);
        let (response, _) = ask(&connection, &send, |print| {
            print.call("Print", &("", title, Fd::from(file.as_fd()), &options))
        })?;
        Ok(if response == 0 {
            Printed::Sent
        } else {
            Printed::Cancelled
        })
    }
}

#[cfg(test)]
mod tests {
    use super::temporary_pdf;

    #[test]
    fn temporary_pdf_named_after_document() {
        let pdf = temporary_pdf("Quarterly report");
        assert!(pdf.starts_with(std::env::temp_dir()));
        assert_eq!(
            pdf.file_name().and_then(|name| name.to_str()),
            Some("Quarterly report.pdf")
        );
    }
}
