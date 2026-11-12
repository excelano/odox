//! A text document: `.odt`.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::Document;
use crate::edit::Refused;
use crate::media_type;
use crate::style::PageLayout;
use crate::value::Length;
use crate::xml::{Element, Node, Ns};
use crate::{Error, Family};

/// What Format > Page setup… changes: the paper, which way round it is
/// printed, and the margins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSetup {
    /// The page's width, as it is printed.
    pub width: Length,
    /// Its height.
    pub height: Length,
    /// Top, right, bottom and left margins.
    pub margins: [Length; 4],
}

impl PageSetup {
    /// Whether the page is wider than it is tall.
    pub fn landscape(&self) -> bool {
        self.width.points() > self.height.points()
    }
}

/// The attributes of `style:page-layout-properties` a [`PageSetup`] writes,
/// with the length each takes from it.
fn page_attributes(setup: &PageSetup) -> [(Ns, &'static str, Length); 6] {
    let [top, right, bottom, left] = setup.margins;
    [
        (Ns::Fo, "page-width", setup.width),
        (Ns::Fo, "page-height", setup.height),
        (Ns::Fo, "margin-top", top),
        (Ns::Fo, "margin-right", right),
        (Ns::Fo, "margin-bottom", bottom),
        (Ns::Fo, "margin-left", left),
    ]
}

/// A length written in a unit: the one `like` is written in where it names
/// one, and centimetres otherwise.
fn written_like(length: Length, like: Option<&str>) -> String {
    let unit = like
        .map(str::trim)
        .and_then(|value| value.get(value.len().saturating_sub(2)..))
        .filter(|unit| matches!(*unit, "in" | "cm" | "mm" | "pt"))
        .unwrap_or("cm");
    let points = length.points();
    let value = match unit {
        "in" => points / 72.0,
        "mm" => points / 72.0 * 25.4,
        "pt" => points,
        _ => points / 72.0 * 2.54,
    };
    let number = format!("{value:.4}");
    format!(
        "{}{unit}",
        number.trim_end_matches('0').trim_end_matches('.')
    )
}

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

impl From<Document> for TextDocument {
    fn from(document: Document) -> Self {
        Self { document }
    }
}

impl TextDocument {
    /// Read a `.odt` package.
    ///
    /// # Errors
    ///
    /// The bytes are not a text document, or its `content.xml` cannot be read.
    pub fn read(bytes: &[u8]) -> Result<Self, Error> {
        Ok(Document::read(bytes, media_type::TEXT_ANY)?.into())
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

    /// Where [`Self::body`] is under the content root, as a path of child
    /// indices: what an editor addresses the body's paragraphs from.
    pub fn body_path(&self) -> Option<Vec<usize>> {
        self.document.body_path("text")
    }

    /// The page layout the document's first master page points at.
    ///
    /// A text document has one master page per page style, and the first is the
    /// one the body starts on. Where there is none, or no `styles.xml` at all,
    /// the default of [`PageLayout`] applies.
    pub fn page_layout(&self) -> PageLayout {
        self.master_page()
            .and_then(|master| master.attr(&Ns::Style, "name"))
            .and_then(|name| self.document.styles.page_layout_of(name))
            .cloned()
            .unwrap_or_default()
    }

    /// The paper, its orientation and its margins, as the body's page layout
    /// gives them.
    pub fn page_setup(&self) -> PageSetup {
        let layout = self.page_layout();
        let margin = |side: Option<Length>| side.unwrap_or(Length(0.0));
        PageSetup {
            width: layout.width,
            height: layout.height,
            margins: [
                margin(layout.margin.top),
                margin(layout.margin.right),
                margin(layout.margin.bottom),
                margin(layout.margin.left),
            ],
        }
    }

    /// Change the paper, its orientation and its margins, in the page layout
    /// the body's master page names, where it stands: another master page
    /// naming the same layout changes with it. Only what differs is written,
    /// each length in the unit the layout already writes it in. Answers
    /// whether anything changed.
    ///
    /// # Errors
    ///
    /// The document has no `styles.xml`, no master page, or no page layout
    /// of the name its master page gives.
    pub fn set_page_setup(&mut self, setup: &PageSetup) -> Result<bool, Refused> {
        let name = self
            .master_page()
            .and_then(|master| master.attr(&Ns::Style, "page-layout-name"))
            .map(ToOwned::to_owned)
            .ok_or(Refused::NotFound)?;
        let current = self.page_setup();
        let styles = self
            .document
            .styles_part
            .as_mut()
            .ok_or(Refused::NotFound)?;
        let layout = styles
            .children
            .iter_mut()
            .filter_map(|node| match node {
                Node::Element(container)
                    if container.is(&Ns::Office, "automatic-styles")
                        || container.is(&Ns::Office, "styles") =>
                {
                    Some(container)
                }
                _ => None,
            })
            .flat_map(|container| container.children.iter_mut())
            .find_map(|node| match node {
                Node::Element(e)
                    if e.is(&Ns::Style, "page-layout")
                        && e.attr(&Ns::Style, "name") == Some(name.as_str()) =>
                {
                    Some(e)
                }
                _ => None,
            })
            .ok_or(Refused::NotFound)?;
        let properties_name = layout.name_for(&Ns::Style, "page-layout-properties");
        if layout.child(&Ns::Style, "page-layout-properties").is_none() {
            layout.children.insert(
                0,
                Node::Element(Element {
                    name: properties_name,
                    attrs: Vec::new(),
                    children: Vec::new(),
                    self_closing: true,
                }),
            );
            layout.self_closing = false;
        }
        let properties = layout
            .child_mut(&Ns::Style, "page-layout-properties")
            .ok_or(Refused::NotFound)?;
        let mut changed = false;
        for ((ns, local, wanted), (_, _, had)) in page_attributes(setup)
            .into_iter()
            .zip(page_attributes(&current))
        {
            if (wanted.points() - had.points()).abs() < 0.01 {
                continue;
            }
            let value = written_like(wanted, properties.attr(&ns, local));
            let name = properties.name_for(&ns, local);
            properties.set_attr(name, value);
            changed = true;
        }
        let orientation = if setup.landscape() {
            "landscape"
        } else {
            "portrait"
        };
        if setup.landscape() != current.landscape()
            || properties
                .attr(&Ns::Style, "print-orientation")
                .is_some_and(|o| o != orientation)
        {
            let name = properties.name_for(&Ns::Style, "print-orientation");
            properties.set_attr(name, orientation);
            changed = true;
        }
        if changed {
            let styles = self
                .document
                .styles_part
                .as_ref()
                .ok_or(Refused::NotFound)?;
            self.document.styles.reread(styles);
        }
        Ok(changed)
    }

    /// The document's first master page, which holds the body's page header
    /// and footer.
    pub fn master_page(&self) -> Option<&Element> {
        self.document
            .styles_part
            .as_ref()?
            .child(&Ns::Office, "master-styles")?
            .child(&Ns::Style, "master-page")
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

    /// The height of a page's body text: the page less its top and bottom
    /// margins.
    pub fn text_height(&self) -> f32 {
        let layout = self.page_layout();
        let top = layout
            .margin
            .top
            .map_or(0.0, super::super::value::Length::points);
        let bottom = layout
            .margin
            .bottom
            .map_or(0.0, super::super::value::Length::points);
        (layout.height.points() - top - bottom).max(1.0)
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
