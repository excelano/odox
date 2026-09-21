//! A text document: `.odt`.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::Document;
use crate::media_type;
use crate::style::PageLayout;
use crate::xml::{Element, Ns};
use crate::{Error, Family};

/// An `OpenDocument` text document.
///
/// The body is a flow of block elements — paragraphs, headings, lists, tables,
/// sections, frames — in the order they are read, which is the order they are
/// drawn. Nothing here paginates: a page layout says how wide a line may be, and
/// that is what [`Self::page_layout`] is for.
pub struct TextDocument {
    /// The package and everything shared with the other two formats.
    pub document: Document,
}

impl TextDocument {
    /// Read a `.odt` package.
    ///
    /// # Errors
    ///
    /// The bytes are not a text document, or its `content.xml` cannot be read.
    pub fn read(bytes: &[u8]) -> Result<Self, Error> {
        let document = Document::read(bytes, media_type::TEXT_ANY)?;
        Ok(Self { document })
    }

    /// The `office:text` element: the flow.
    ///
    /// `None` for a package that declares itself a text document and carries no
    /// text body, which is a broken document rather than an empty one.
    pub fn body(&self) -> Option<&Element> {
        self.document.body_of("text")
    }

    /// The same, for changing it.
    pub fn body_mut(&mut self) -> Option<&mut Element> {
        self.document
            .content
            .child_mut(&Ns::Office, "body")?
            .child_mut(&Ns::Office, "text")
    }

    /// The page layout the document's first master page points at.
    ///
    /// A text document has one master page per page style, and the first is the
    /// one the body starts on. Where there is none, or no `styles.xml` at all,
    /// the default of [`PageLayout`] applies.
    pub fn page_layout(&self) -> PageLayout {
        let master = self
            .document
            .styles_part
            .as_ref()
            .and_then(|styles| styles.child(&Ns::Office, "master-styles"))
            .and_then(|masters| masters.child(&Ns::Style, "master-page"))
            .and_then(|master| master.attr(&Ns::Style, "name"));
        master
            .and_then(|name| self.document.styles.page_layout_of(name))
            .cloned()
            .unwrap_or_default()
    }

    /// The width a line of body text may be: the page less its side margins.
    pub fn text_width(&self) -> f32 {
        let layout = self.page_layout();
        let left = layout
            .margin
            .left
            .map_or(0.0, super::super::value::Length::points);
        let right = layout
            .margin
            .right
            .map_or(0.0, super::super::value::Length::points);
        (layout.width.points() - left - right).max(1.0)
    }

    /// The document's headings, as an outline for navigating it: each heading's
    /// level, its text, and its position among the body's children.
    ///
    /// Only the top level of the body is walked. A heading inside a table cell or
    /// a frame is not a heading in the outline — ODF permits one and no
    /// application lists it — and a heading inside a section is, which is why
    /// sections are descended into.
    pub fn outline(&self) -> Vec<Heading> {
        let mut headings = Vec::new();
        if let Some(body) = self.body() {
            collect_headings(body, &mut headings);
        }
        headings
    }

    /// The style a block element asks for, resolved.
    ///
    /// The family is the one the element belongs to, which for everything in a
    /// text flow is [`Family::Paragraph`] except a table and its parts.
    pub fn paragraph_style(&self, element: &Element) -> std::rc::Rc<crate::Properties> {
        let name = element.attr(&Ns::Text, "style-name").unwrap_or("Standard");
        self.document.styles.resolve(&Family::Paragraph, name)
    }
}

/// One entry of a document's outline.
pub struct Heading {
    /// `text:outline-level`, where 1 is the top. A heading with no level
    /// declared is level 1, which is what ODF says.
    pub level: u8,
    /// The heading's text, with ODF's whitespace elements resolved.
    pub text: String,
}

fn collect_headings(parent: &Element, into: &mut Vec<Heading>) {
    for element in parent.elements() {
        if element.is(&Ns::Text, "h") {
            let level = element
                .attr_usize(&Ns::Text, "outline-level")
                .unwrap_or(1)
                .clamp(1, 10);
            #[allow(clippy::cast_possible_truncation)]
            into.push(Heading {
                level: level as u8,
                text: element.plain_text(),
            });
        } else if element.is(&Ns::Text, "section") {
            collect_headings(element, into);
        }
    }
}
