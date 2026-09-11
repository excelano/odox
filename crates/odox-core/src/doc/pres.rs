//! A presentation: `.odp`.
//!
//! A presentation's body is a sequence of `draw:page` elements, each naming the
//! master page it draws its background and its placeholder geometry from. The
//! shapes on a page carry their own position and size in `svg:` attributes, in
//! the page's coordinate space, which is what makes a slide drawable without
//! laying anything out: a renderer scales the page to the space it has and puts
//! each shape where the document says.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::Document;
use crate::Error;
use crate::media_type;
use crate::style::PageLayout;
use crate::xml::{Element, Ns};

/// An `OpenDocument` presentation.
pub struct Presentation {
    /// The package and everything shared with the other two formats.
    pub document: Document,
}

/// One slide.
pub struct Slide<'a> {
    /// The `draw:page` element and everything on it.
    pub element: &'a Element,
    /// The slide's name, where it has one. An application generates `page1` and
    /// so on for slides nobody has named.
    pub name: Option<&'a str>,
    /// The master page it takes its background and placeholders from.
    pub master_page: Option<&'a str>,
}

impl Slide<'_> {
    /// The speaker's notes, which ODF keeps in a `presentation:notes` element on
    /// the page rather than in a part of its own.
    pub fn notes(&self) -> Option<&Element> {
        self.element.child(&Ns::Presentation, "notes")
    }

    /// The shapes on the slide: every child that is not the notes.
    ///
    /// Order is drawing order, back to front, which is the order ODF writes them
    /// in and the order they have to be drawn in.
    pub fn shapes(&self) -> impl Iterator<Item = &Element> {
        self.element
            .elements()
            .filter(|e| !e.is(&Ns::Presentation, "notes") && !e.is(&Ns::Office, "forms"))
    }
}

impl Presentation {
    /// Read a `.odp` package.
    ///
    /// # Errors
    ///
    /// The bytes are not a presentation, or its `content.xml` cannot be read.
    pub fn read(bytes: &[u8]) -> Result<Self, Error> {
        let document = Document::read(bytes, media_type::PRESENTATION_ANY)?;
        Ok(Self { document })
    }

    /// The slides, in the order they are presented.
    pub fn slides(&self) -> Vec<Slide<'_>> {
        let Some(body) = self.document.body_of("presentation") else {
            return Vec::new();
        };
        body.elements()
            .filter(|e| e.is(&Ns::Draw, "page"))
            .map(|element| Slide {
                element,
                name: element.attr(&Ns::Draw, "name"),
                master_page: element.attr(&Ns::Draw, "master-page-name"),
            })
            .collect()
    }

    /// The page geometry a slide is drawn in: the size of its master page's
    /// layout.
    ///
    /// Every shape's position is in this space, so a renderer needs it before it
    /// can place anything.
    pub fn page_layout(&self, slide: &Slide<'_>) -> PageLayout {
        slide
            .master_page
            .and_then(|name| self.document.styles.page_layout_of(name))
            .cloned()
            .unwrap_or_else(presentation_default)
    }
}

/// The slide size of a presentation that declares no page layout.
///
/// A presentation's default is landscape where a text document's is portrait, and
/// ODF's own default for one is the same paper turned on its side.
fn presentation_default() -> PageLayout {
    let portrait = PageLayout::default();
    PageLayout {
        width: portrait.height,
        height: portrait.width,
        margin: portrait.margin,
    }
}
