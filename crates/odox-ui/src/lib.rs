//! The window the three odox applications are each a thin shell over.
//!
//! Everything a person sees belongs here: the chrome, the document renderer, the
//! mapping from ODF styles to what egui draws with, the font resolution and the
//! messages. An application crate contributes its name, the format it opens, and
//! the one view that is particular to its body — a flow, a grid, a slide.
//!
//! Drawing lives here rather than in `odox-core` so that the library that reads
//! `OpenDocument` has no window in it: it is testable headlessly, it compiles for
//! a target with no display, and nothing about the format is decided by what egui
//! can draw.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

// `deny` rather than `forbid`, and the macOS glue is the exception it leaves
// room for: receiving a document and starting another instance each need an
// Objective-C call that cannot be written without `unsafe`, and `forbid` cannot
// be lifted beneath it. `opened_document` and `instance` are the only `unsafe`
// in the workspace, on one platform. `odox-core` keeps `forbid`, and so do the
// three applications.
#![deny(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::must_use_candidate)]

pub mod clipboard;
pub mod color_picker;
pub mod comments;
pub mod count;
pub mod edit;
pub mod export;
pub mod find;
pub mod find_bar;
pub mod flow;
pub mod flow_model;
pub mod fonts;
pub mod format;
pub mod i18n;
pub mod instance;
pub mod link_dialog;
pub mod links;
pub mod marks;
pub mod note_popup;
/// Receiving a document from macOS, which does not arrive as an argument.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub mod opened_document;
pub mod page_setup;
pub mod picture_question;
pub mod recent;
pub mod recovery;
pub mod settings;
pub mod shapes;
pub mod shell;
pub mod system_theme;
pub mod toolbar;

pub use edit::{Caret, Editing};
pub use export::Export;
pub use find::{Found, Highlights, Replaced};
pub use flow::{Flow, Pictures};
pub use flow_model::{Block, FlowModel, LinkHere, PageEditor, Picture, page_editor};
pub use i18n::mark;
pub use settings::Settings;
pub use shapes::Canvas;
pub use shell::{Notice, Product, Shell, View, Zoom, replace_file, run};

/// The moment it is in local time, as a spreadsheet counts days: the days
/// since 1899-12-30 and the fraction of today gone.
pub fn now() -> f64 {
    const UNIX_EPOCH_DAY: f64 = 25_569.0;
    let now = jiff::ZonedDateTime::now();
    let local = now.timestamp().as_second() + i64::from(now.offset().seconds());
    #[allow(clippy::cast_precision_loss)]
    let days = local as f64 / 86_400.0;
    UNIX_EPOCH_DAY + days
}

/// Today, as year, month and day in local time: what a date that follows the
/// clock shows.
pub fn today() -> (i64, i64, i64) {
    let now = jiff::ZonedDateTime::now();
    (
        i64::from(now.year()),
        i64::from(now.month()),
        i64::from(now.day()),
    )
}
