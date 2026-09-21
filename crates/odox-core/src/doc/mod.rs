//! The three kinds of document, over one shared reader.
//!
//! Everything a package carries that is not the body — its styles, its
//! metadata, its pictures, its manifest — is read the same way for all three
//! formats, and [`Document`] is that. Each format then adds one thing: the way
//! its body is addressed. A text document is a flow, a spreadsheet is a set of
//! sheets indexed by row and column, a presentation is a sequence of pages.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

mod pres;
mod sheet;
mod text;

pub use pres::{Presentation, Slide};
pub use sheet::{Cell, Column, Sheet, SheetDocument, Value};
pub use text::TextDocument;

use crate::xml::{Element, Name, Ns};
use crate::{Error, Meta, Package, Styles};

/// A document read from a package: everything the three formats share.
pub struct Document {
    /// The package itself, which keeps every entry whether or not it was read
    /// here, so that writing gives back a document and not a subset of one.
    pub package: Package,
    /// The parsed `content.xml`.
    pub content: Element,
    /// The parsed `styles.xml`, if the package has one.
    pub styles_part: Option<Element>,
    /// Every style in the document, from both parts.
    pub styles: Styles,
    /// What the document says about itself.
    pub meta: Meta,
}

impl Document {
    /// Read a package and check its media type.
    ///
    /// # Errors
    ///
    /// The bytes are not a package, the package declares a different media type,
    /// or `content.xml` is absent or not well formed.
    pub fn read(bytes: &[u8], wanted: &'static [&'static str]) -> Result<Self, Error> {
        let package = Package::read(bytes)?;
        package.expect_media_type(wanted)?;
        let content = package.xml("content.xml")?;
        let styles_part = package.optional_xml("styles.xml")?;
        let styles = Styles::collect(Some(&content), styles_part.as_ref());
        let meta = match package.optional_xml("meta.xml")? {
            Some(part) => Meta::read(&part),
            None => Meta::default(),
        };
        Ok(Self {
            package,
            content,
            styles_part,
            styles,
            meta,
        })
    }

    /// The `office:body` element, which every format has exactly one of.
    pub fn body(&self) -> Option<&Element> {
        self.content.child(&Ns::Office, "body")
    }

    /// The part of the body this format keeps its content in: `office:text`,
    /// `office:spreadsheet` or `office:presentation`.
    pub fn body_of(&self, local: &str) -> Option<&Element> {
        self.body()?.child(&Ns::Office, local)
    }

    /// A name for writing into the document, in the prefix the document
    /// declares for the namespace on its content root, or the conventional one
    /// where it declares none.
    pub fn name(&self, ns: &Ns, local: &str) -> Name {
        let declared = self
            .content
            .attrs
            .iter()
            .filter(|a| a.name.ns == Ns::Xmlns)
            .find(|a| Ns::from_uri(&a.value) == *ns)
            .map(|a| &*a.name.local);
        Name::new(
            declared.unwrap_or(ns.conventional_prefix()),
            local,
            ns.clone(),
        )
    }

    /// Whether the content root declares a namespace, which is what decides
    /// whether an attribute in it may be written at all.
    pub fn declares(&self, ns: &Ns) -> bool {
        self.content
            .attrs
            .iter()
            .any(|a| a.name.ns == Ns::Xmlns && Ns::from_uri(&a.value) == *ns)
    }

    /// The version of the format the document declares, as `office:version`.
    pub fn version(&self) -> Option<&str> {
        self.content.attr(&Ns::Office, "version")
    }

    /// The bytes of a picture, by the `xlink:href` that referred to it.
    ///
    /// A href that names an entry of the package gives those bytes. A href that
    /// is a URL to somewhere else gives `None`: reaching it would be a network
    /// request, which nothing in this suite makes.
    pub fn picture(&self, href: &str) -> Option<&[u8]> {
        // A href inside a package is a relative path, sometimes written with a
        // leading `./`.
        let path = href.strip_prefix("./").unwrap_or(href);
        if path.contains("://") {
            return None;
        }
        self.package.part(path).map(|p| p.data.as_slice())
    }

    /// Write the document back out, with the parsed parts re-serialized over the
    /// ones they were read from.
    ///
    /// # Errors
    ///
    /// The package could not be assembled.
    pub fn write(&mut self) -> Result<Vec<u8>, Error> {
        let content = crate::xml::serialize(&self.content);
        self.package.set_part("content.xml", content);
        if let Some(styles) = &self.styles_part {
            let bytes = crate::xml::serialize(styles);
            self.package.set_part("styles.xml", bytes);
        }
        self.package.write()
    }

    /// Write the document and prove the bytes read back as the tree they were
    /// written from, which is what a save has to know before it touches the
    /// file.
    ///
    /// The round-trip tests make the same claim over the corpus, and a person's
    /// document is not in the corpus. One re-parse per save is the price, and
    /// the alternative is an editor that finds out it damaged a document after
    /// it has.
    ///
    /// # Errors
    ///
    /// The package could not be assembled, or a part came back different, in
    /// which case nothing should be written.
    pub fn write_verified(&mut self) -> Result<Vec<u8>, Error> {
        let bytes = self.write()?;
        let written = Package::read(&bytes)?;
        if written.xml("content.xml")? != self.content {
            return Err(Error::Unfaithful("content.xml"));
        }
        if let Some(styles) = &self.styles_part
            && written.optional_xml("styles.xml")?.as_ref() != Some(styles)
        {
            return Err(Error::Unfaithful("styles.xml"));
        }
        Ok(bytes)
    }
}
