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

// `deny` rather than `forbid`, and `opened_document` is the exception it leaves
// room for: receiving a document from macOS needs one Objective-C method that
// cannot be written without `unsafe`, and `forbid` cannot be lifted beneath it.
// That module is the only `unsafe` in the workspace and it is compiled on one
// platform. `odox-core` keeps `forbid`, and so do the three applications.
#![deny(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::must_use_candidate)]

pub mod edit;
pub mod flow;
pub mod fonts;
pub mod format;
pub mod i18n;
/// Receiving a document from macOS, which does not arrive as an argument.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub mod opened_document;
pub mod settings;
pub mod shapes;
pub mod shell;
pub mod system_theme;

pub use edit::Editing;
pub use flow::{Flow, Pictures};
pub use i18n::mark;
pub use settings::Settings;
pub use shapes::Canvas;
pub use shell::{Product, Shell, View, run};
