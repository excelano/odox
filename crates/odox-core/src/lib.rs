//! `OpenDocument`, read and written: the package, its XML, its styles, and the
//! three bodies that an ODF document can have.
//!
//! # Bytes in, bytes out
//!
//! Nothing here opens a file. A [`Package`] is made from a byte slice and
//! turned back into a `Vec<u8>`, which is what lets the same code serve a
//! desktop window, a sandboxed macOS application that may only replace the
//! file it was handed, a browser, and a test that never touches a disk.
//!
//! # What it does not understand, it keeps
//!
//! The document is held as the XML tree it was parsed from, not as a summary of
//! the parts this crate has opinions about. An element nobody here has heard of
//! keeps its attributes, its children and its position, and is written back
//! where it was found. A viewer does not need that; an editor built on a model
//! that discarded the unread nine tenths of ODF would destroy every document it
//! saved, and by then the model is load-bearing everywhere.
//!
//! Typed reading is therefore a *view* over the tree rather than a replacement
//! for it: [`Styles`] resolves a style name through its inheritance chain, and
//! the three document types in [`doc`] index the body for the application that
//! draws it.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::must_use_candidate)]

pub mod doc;
pub mod draw;
mod error;
mod meta;
mod package;
mod style;
mod value;
pub mod xml;

pub use doc::Document;
pub use error::Error;
pub use meta::Meta;
pub use package::{Package, Part};
pub use style::{
    Anchor, Border, Break, CellProperties, Edges, Family, Fill, Gradient, GradientStyle,
    GraphicProperties, PageLayout, ParagraphProperties, Position, Properties, Style, Styles,
    TextAlign, TextProperties, VerticalAlign,
};
pub use value::{Color, Length, Measure, Percent};
pub use xml::{Element, Name, Node, Ns};

/// The media type of each format this crate reads, as written in a package's
/// `mimetype` entry and in the desktop's media-type database.
pub mod media_type {
    /// Text document: `.odt`.
    pub const TEXT: &str = "application/vnd.oasis.opendocument.text";
    /// A text document written as a web page: also `.odt`, and a media type of
    /// its own rather than a variant of the one above. It is what `LibreOffice`
    /// writes for a document converted from HTML, and it is read the same way.
    pub const TEXT_WEB: &str = "application/vnd.oasis.opendocument.text-web";
    /// Spreadsheet: `.ods`.
    pub const SPREADSHEET: &str = "application/vnd.oasis.opendocument.spreadsheet";
    /// Presentation: `.odp`.
    pub const PRESENTATION: &str = "application/vnd.oasis.opendocument.presentation";

    /// Every media type a text document may declare.
    pub const TEXT_ANY: &[&str] = &[TEXT, TEXT_WEB];
    /// Every media type a spreadsheet may declare.
    pub const SPREADSHEET_ANY: &[&str] = &[SPREADSHEET];
    /// Every media type a presentation may declare.
    pub const PRESENTATION_ANY: &[&str] = &[PRESENTATION];
}
