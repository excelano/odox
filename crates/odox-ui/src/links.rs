//! Following a hyperlink in a document.
//!
//! A document is not trusted with the machine: a link may name any scheme the
//! desktop has a handler for, and some handlers run programs. Only the three a
//! person means by a link are opened, in whatever the desktop uses for them,
//! and nothing else is, whatever the document says.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::Context;

/// Where a link goes, as far as the window is concerned.
#[derive(Debug, PartialEq, Eq)]
pub enum Target<'a> {
    /// A web page or a mail address, handed to the desktop.
    External(&'a str),
    /// A place in this document: a bookmark, or a heading as `Name|outline`.
    Within(&'a str),
}

/// What a link's `xlink:href` means here, or `None` for one that is not
/// followed.
pub fn target(href: &str) -> Option<Target<'_>> {
    if let Some(place) = href.strip_prefix('#') {
        return (!place.is_empty()).then_some(Target::Within(place));
    }
    let scheme = href.split_once(':')?.0;
    ["http", "https", "mailto"]
        .iter()
        .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
        .then_some(Target::External(href))
}

/// Hand an address to the desktop.
pub fn open(ctx: &Context, address: &str) {
    ctx.open_url(eframe::egui::OpenUrl::new_tab(address));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_web_and_mail_are_followed() {
        assert_eq!(
            target("https://example.com/a"),
            Some(Target::External("https://example.com/a"))
        );
        assert_eq!(
            target("HTTP://example.com"),
            Some(Target::External("HTTP://example.com"))
        );
        assert_eq!(
            target("mailto:me@example.com"),
            Some(Target::External("mailto:me@example.com"))
        );
    }

    #[test]
    fn a_place_in_the_document_is_named_without_its_hash() {
        assert_eq!(
            target("#Chapter|outline"),
            Some(Target::Within("Chapter|outline"))
        );
        assert_eq!(target("#"), None);
    }

    #[test]
    fn nothing_else_is_followed() {
        for href in [
            "file:///etc/passwd",
            "ms-msdt:/id PCWDiagnostic",
            "javascript:alert(1)",
            "smb://host/share",
            "./other.odt",
            "example.com",
            "",
        ] {
            assert_eq!(target(href), None, "{href}");
        }
    }
}
