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
/// this process's own and one for each print, so that a viewer still holding
/// the last one open does not stand in the way, under the document's name.
pub fn temporary_pdf(stem: &str, print: u32) -> PathBuf {
    std::env::temp_dir()
        .join(format!("odox-print-{}", std::process::id()))
        .join(print.to_string())
        .join(format!("{stem}.pdf"))
}

/// Hand a PDF to the platform to print.
///
/// # Errors
///
/// The print dialog failed to print it, or nothing opens PDF files.
pub fn hand_over(pdf: &Path, title: &str) -> std::io::Result<Printed> {
    #[cfg(target_os = "linux")]
    if let Ok(printed) = portal::print(pdf, title) {
        return printed;
    }
    let _ = title;
    open_in_viewer(pdf).map(|()| Printed::Opened)
}

/// What a program that could not be started is told as.
fn not_started(e: std::io::Error) -> std::io::Error {
    if e.kind() == std::io::ErrorKind::NotFound {
        std::io::Error::other(crate::i18n::t("no program here opens PDF files"))
    } else {
        e
    }
}

/// Start a program that opens the PDF, and wait for it apart, so it is not
/// left behind when it ends.
#[cfg(not(target_os = "macos"))]
fn start(program: &str, pdf: &Path) -> std::io::Result<()> {
    let mut child = std::process::Command::new(program)
        .arg(pdf)
        .spawn()
        .map_err(not_started)?;
    std::thread::spawn(move || child.wait());
    Ok(())
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
            Err(not_started(std::io::ErrorKind::NotFound.into()))
        }
    }
    #[cfg(target_os = "windows")]
    {
        start("explorer.exe", pdf)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        start("xdg-open", pdf)
    }
}

#[cfg(target_os = "linux")]
mod portal {
    use std::collections::HashMap;
    use std::os::fd::AsFd;
    use std::path::Path;

    use futures_lite::StreamExt as _;
    use zbus::Connection;
    use zbus::export::serde::Serialize;
    use zbus::zvariant::{DynamicType, Fd, OwnedObjectPath, OwnedValue, Value};

    use super::Printed;

    const PORTAL: &str = "org.freedesktop.portal.Desktop";
    const PATH: &str = "/org/freedesktop/portal/desktop";
    const PRINT: &str = "org.freedesktop.portal.Print";
    const REQUEST: &str = "org.freedesktop.portal.Request";

    type Results = HashMap<String, OwnedValue>;

    fn failed(said: &str) -> zbus::Error {
        zbus::Error::Failure(said.to_owned())
    }

    /// Put a request to the portal and wait for its answer, or for the
    /// portal to go away, which answers nothing. The answer is listened for
    /// before the request is put, at the path the token makes, so it cannot
    /// arrive first; a portal that answers at another path is listened to
    /// there.
    async fn ask<B: Serialize + DynamicType>(
        connection: &Connection,
        token: &str,
        method: &str,
        body: &B,
    ) -> zbus::Result<(u32, Results)> {
        let sender = connection
            .unique_name()
            .map(|name| name.trim_start_matches(':').replace('.', "_"))
            .unwrap_or_default();
        let expected = format!("{PATH}/request/{sender}/{token}");
        let listen = |path: String| async move {
            zbus::Proxy::new(connection, PORTAL, path, REQUEST)
                .await?
                .receive_signal("Response")
                .await
        };
        let mut answers = listen(expected.clone()).await?;
        let mut owners = zbus::fdo::DBusProxy::new(connection)
            .await?
            .receive_name_owner_changed_with_args(&[(0, PORTAL)])
            .await?;
        let print = zbus::Proxy::new(connection, PORTAL, PATH, PRINT).await?;
        let handle: OwnedObjectPath = print.call(method, body).await?;
        if handle.as_str() != expected {
            answers = listen(handle.as_str().to_owned()).await?;
        }
        let answered = async {
            let answer = answers
                .next()
                .await
                .ok_or_else(|| failed("the portal did not answer"))?;
            answer.body().deserialize::<(u32, Results)>()
        };
        let gone = async {
            while let Some(change) = owners.next().await {
                if change.args().is_ok_and(|args| args.new_owner().is_none()) {
                    break;
                }
            }
            Err(failed("the portal went away"))
        };
        futures_lite::future::or(answered, gone).await
    }

    /// Ask the portal's print dialog how to print, and print the PDF there.
    /// The outer error is a portal that could not be asked; the inner one, a
    /// print it failed.
    pub(super) fn print(pdf: &Path, title: &str) -> zbus::Result<std::io::Result<Printed>> {
        zbus::block_on(async {
            let connection = Connection::session().await?;
            let stamp = std::process::id();
            let prepare = format!("odox_prepare_{stamp}");
            let options: HashMap<&str, Value<'_>> =
                HashMap::from([("handle_token", Value::from(prepare.as_str()))]);
            let empty: HashMap<&str, Value<'_>> = HashMap::new();
            let (response, results) = ask(
                &connection,
                &prepare,
                "PreparePrint",
                &("", title, &empty, &empty, &options),
            )
            .await?;
            if response != 0 {
                return Ok(Ok(Printed::Cancelled));
            }
            let token: u32 = results
                .get("token")
                .and_then(|token| u32::try_from(token).ok())
                .ok_or_else(|| failed("the portal gave no token"))?;
            let file = std::fs::File::open(pdf).map_err(|e| failed(&e.to_string()))?;
            let send = format!("odox_print_{stamp}");
            let options: HashMap<&str, Value<'_>> = HashMap::from([
                ("handle_token", Value::from(send.as_str())),
                ("token", Value::from(token)),
            ]);
            let (response, _) = ask(
                &connection,
                &send,
                "Print",
                &("", title, Fd::from(file.as_fd()), &options),
            )
            .await?;
            Ok(match response {
                0 => Ok(Printed::Sent),
                1 => Ok(Printed::Cancelled),
                _ => Err(std::io::Error::other(crate::i18n::t(
                    "the print dialog did not print it",
                ))),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::temporary_pdf;

    #[test]
    fn temporary_pdf_named_after_document() {
        let pdf = temporary_pdf("Quarterly report", 1);
        assert!(pdf.starts_with(std::env::temp_dir()));
        assert_eq!(
            pdf.file_name().and_then(|name| name.to_str()),
            Some("Quarterly report.pdf")
        );
    }

    #[test]
    fn each_print_has_a_file_of_its_own() {
        assert_ne!(temporary_pdf("Report", 1), temporary_pdf("Report", 2));
    }
}
