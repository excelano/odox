//! What can go wrong reading or writing a package.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::fmt;

/// A package that could not be read, or a document that could not be written.
#[derive(Debug)]
pub enum Error {
    /// The bytes are not a zip archive, or an entry in one could not be read.
    Package(String),
    /// An entry the format requires is absent.
    MissingPart(&'static str),
    /// The XML in a named part is not well formed.
    Xml {
        /// Which entry of the package.
        part: String,
        /// What the parser said, with the position it said it at.
        detail: String,
    },
    /// The package is a valid `OpenDocument` package of a kind this application
    /// does not open. Carries the media type found, so the window can say which
    /// of its siblings to reach for.
    WrongFormat {
        /// The media type the package declares.
        found: String,
        /// The media type this reader wanted.
        wanted: &'static str,
    },
    /// The document was serialized and read back, and a part did not come back
    /// as the tree it was written from. Nothing has been written to disk: this
    /// is the check that stands between an editor and a document it would have
    /// damaged.
    Unfaithful(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Package(detail) => write!(f, "not a readable package: {detail}"),
            Self::MissingPart(name) => write!(f, "the package has no {name}"),
            Self::Xml { part, detail } => write!(f, "{part} is not well-formed XML: {detail}"),
            Self::WrongFormat { found, wanted } => {
                write!(f, "this is a {found} package, not a {wanted} one")
            }
            Self::Unfaithful(part) => write!(
                f,
                "{part} would not read back as it was written, so the document was not saved; please report this"
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Self::Package(e.to_string())
    }
}
