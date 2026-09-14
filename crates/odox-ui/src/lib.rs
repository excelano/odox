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

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::must_use_candidate)]

pub mod flow;
pub mod fonts;
pub mod format;
pub mod i18n;
pub mod shapes;
pub mod shell;
pub mod system_theme;

pub use flow::{Flow, Pictures};
pub use i18n::mark;
pub use shapes::Canvas;
pub use shell::{Product, Shell, Viewer, run};
