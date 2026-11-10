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
use crate::edit::Refused;
use crate::style::{Family, Fill, PageLayout};
use crate::value::Length;
use crate::xml::{Element, Node, Ns};
use crate::{Error, media_type};

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
    /// Where the `draw:page` sits among the body's children, so that the
    /// slide can be reached again for changing without holding a reference.
    pub position: usize,
}

impl Slide<'_> {
    /// The drawing-page style the slide names, which carries its background and
    /// the switches over what its master gives it.
    pub fn style_name(&self) -> Option<&str> {
        self.element.attr(&Ns::Draw, "style-name")
    }

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
        self.shapes_indexed().map(|(_, shape)| shape)
    }

    /// The shapes with their index among the page's children, which is what
    /// [`Presentation::set_geometry`] takes.
    pub fn shapes_indexed(&self) -> impl Iterator<Item = (usize, &Element)> {
        self.element
            .elements_indexed()
            .filter(|(_, e)| !e.is(&Ns::Presentation, "notes") && !e.is(&Ns::Office, "forms"))
    }
}

impl From<Document> for Presentation {
    fn from(document: Document) -> Self {
        Self { document }
    }
}

impl Presentation {
    /// Read a `.odp` package.
    ///
    /// # Errors
    ///
    /// The bytes are not a presentation, or its `content.xml` cannot be read.
    pub fn read(bytes: &[u8]) -> Result<Self, Error> {
        Ok(Document::read(bytes, media_type::PRESENTATION_ANY)?.into())
    }

    /// The slides, in the order they are presented.
    pub fn slides(&self) -> Vec<Slide<'_>> {
        let Some(body) = self.document.body_of("presentation") else {
            return Vec::new();
        };
        body.elements_indexed()
            .filter(|(_, e)| e.is(&Ns::Draw, "page"))
            .map(|(position, element)| Slide {
                element,
                name: element.attr(&Ns::Draw, "name"),
                master_page: element.attr(&Ns::Draw, "master-page-name"),
                position,
            })
            .collect()
    }

    /// Move and size a shape: `svg:x`, `svg:y`, `svg:width` and `svg:height`,
    /// each written in the unit it was read in, in centimetres where it was
    /// absent. The shape is named by its slide's position among the body's
    /// children and its own among the page's, as [`Slide::shapes_indexed`]
    /// gives them.
    ///
    /// A shape placed by `draw:transform` states no corner and is not moved
    /// this way; asking is refused rather than answered wrongly.
    ///
    /// # Errors
    ///
    /// There is no such slide or shape, or the shape is placed by a transform.
    pub fn set_geometry(
        &mut self,
        slide: usize,
        shape: usize,
        x: Length,
        y: Length,
        width: Length,
        height: Length,
    ) -> Result<(), Refused> {
        let names = [
            (self.document.name(&Ns::Svg, "x"), x),
            (self.document.name(&Ns::Svg, "y"), y),
            (self.document.name(&Ns::Svg, "width"), width),
            (self.document.name(&Ns::Svg, "height"), height),
        ];
        let element = self
            .document
            .content
            .child_mut(&Ns::Office, "body")
            .and_then(|body| body.child_mut(&Ns::Office, "presentation"))
            .and_then(|body| body.at_mut(&[slide, shape]))
            .ok_or(Refused::NotFound)?;
        if element.attr(&Ns::Draw, "transform").is_some() {
            return Err(Refused::NotFound);
        }
        for (name, length) in names {
            let unit = element
                .attr(&name.ns, &name.local)
                .map_or("cm", Length::unit_of)
                .to_owned();
            element.set_attr(name, length.write(&unit));
        }
        Ok(())
    }

    /// The `draw:page` element at a position among the body's children, for
    /// changing what is on it.
    pub fn page_mut(&mut self, position: usize) -> Option<&mut Element> {
        self.document
            .content
            .child_mut(&Ns::Office, "body")?
            .child_mut(&Ns::Office, "presentation")?
            .at_mut(&[position])
    }

    /// Where the slide at a position among the body's children is under the
    /// content root, as a path of child indices: what an editor addresses its
    /// labels' paragraphs from.
    pub fn page_path(&self, position: usize) -> Option<Vec<usize>> {
        let mut path = self.document.body_path("presentation")?;
        path.push(position);
        Some(path)
    }

    /// The master page a slide names.
    pub fn master(&self, slide: &Slide<'_>) -> Option<&Element> {
        self.document.styles.master_page(slide.master_page?)
    }

    /// What fills the ground behind a slide.
    ///
    /// The slide's own drawing-page style first, then its master's, which is
    /// where a template puts the colour or gradient every slide shares. A slide
    /// that turns `presentation:background-visible` off gets neither.
    pub fn background(&self, slide: &Slide<'_>) -> Fill {
        let own = slide
            .style_name()
            .map(|name| self.document.styles.resolve(&Family::DrawingPage, name));
        if own
            .as_ref()
            .and_then(|properties| properties.background_visible)
            == Some(false)
        {
            return Fill::None;
        }
        if let Some(fill) = own.map(|properties| properties.graphic.fill())
            && fill != Fill::None
        {
            return fill;
        }
        self.master(slide)
            .and_then(|master| master.attr(&Ns::Draw, "style-name"))
            .map(|name| self.document.styles.resolve(&Family::DrawingPage, name))
            .map_or(Fill::None, |properties| properties.graphic.fill())
    }

    /// Whether a slide is left out of a show, which its drawing-page style
    /// says.
    pub fn is_hidden(&self, slide: &Slide<'_>) -> bool {
        slide
            .style_name()
            .map(|name| self.document.styles.resolve(&Family::DrawingPage, name))
            .and_then(|properties| properties.hidden)
            .unwrap_or(false)
    }

    /// The master page's decorations: what is drawn behind a slide before
    /// anything on the slide itself.
    ///
    /// **A child carrying a `presentation:class` is left out**, because that
    /// attribute is what makes a frame a slot rather than a decoration: the
    /// slide's own frame of that class takes its place, and what the master
    /// holds is either a prompt or a field. Left in, a slide gains the words
    /// *Click to edit Master title style* and a literal `<number>`.
    ///
    /// The class and not `presentation:placeholder`, which would be the obvious
    /// test and is not written reliably: the title frame on `deck.odp`'s master
    /// carries the prompt text and no such attribute. Measured, not read.
    ///
    /// Empty for a slide whose style turns `presentation:background-objects-visible`
    /// off, which is how a template offers a plain slide.
    pub fn background_objects(&self, slide: &Slide<'_>) -> Vec<&Element> {
        let shown = slide
            .style_name()
            .map(|name| self.document.styles.resolve(&Family::DrawingPage, name))
            .and_then(|properties| properties.background_objects_visible);
        if shown == Some(false) {
            return Vec::new();
        }
        let Some(master) = self.master(slide) else {
            return Vec::new();
        };
        master
            .elements()
            .filter(|e| e.attr(&Ns::Draw, "layer") == Some("backgroundobjects"))
            .filter(|e| e.attr(&Ns::Presentation, "class").is_none())
            .collect()
    }

    /// The master page's footer, date and slide-number frames a slide shows,
    /// each a copy with its field put in as the text it stands for: the footer
    /// the slide names, its date, and its number among the slides.
    ///
    /// A footer or a date is shown where the slide names one with something in
    /// it and its style does not turn it off; a slide number only where its
    /// style turns it on. That is how `LibreOffice` draws them, which writes the
    /// footer and date switches on for nearly every slide whether or not there
    /// is anything to show. A date that follows the clock shows `today`, as
    /// year, month and day, in the date's data style.
    pub fn master_fields(&self, slide: &Slide<'_>, today: (i64, i64, i64)) -> Vec<Element> {
        let Some(master) = self.master(slide) else {
            return Vec::new();
        };
        let properties = slide
            .style_name()
            .map(|name| self.document.styles.resolve(&Family::DrawingPage, name));
        let switch = |get: fn(&crate::style::Properties) -> Option<bool>| {
            properties.as_deref().and_then(get)
        };
        let declared = |kind: &str, naming: &str| {
            let name = slide.element.attr(&Ns::Presentation, naming)?;
            self.document.body_of("presentation")?.elements().find(|e| {
                e.is(&Ns::Presentation, kind) && e.attr(&Ns::Presentation, "name") == Some(name)
            })
        };
        let something = |text: &String| !text.trim().is_empty();

        let footer = declared("footer-decl", "use-footer-name")
            .filter(|_| switch(|p| p.display_footer) != Some(false))
            .map(Element::plain_text)
            .filter(something);
        let date = declared("date-time-decl", "use-date-time-name")
            .filter(|_| switch(|p| p.display_date_time) != Some(false))
            .map(|declaration| self.date_shown(declaration, today))
            .filter(something);
        let number = (switch(|p| p.display_page_number) == Some(true))
            .then(|| {
                self.slides()
                    .iter()
                    .position(|other| other.position == slide.position)
            })
            .flatten()
            .map(|index| (index + 1).to_string());

        master
            .elements()
            .filter_map(|frame| {
                let text = match frame.attr(&Ns::Presentation, "class")? {
                    "footer" => footer.as_ref()?,
                    "date-time" => date.as_ref()?,
                    "page-number" => number.as_ref()?,
                    _ => return None,
                };
                let mut frame = frame.clone();
                put_fields(&mut frame, text);
                Some(frame)
            })
            .collect()
    }

    /// What a `presentation:date-time-decl` shows: its own text where it is
    /// fixed, and `today` in its data style where it follows the clock.
    fn date_shown(&self, declaration: &Element, today: (i64, i64, i64)) -> String {
        if declaration.attr(&Ns::Presentation, "source") != Some("current-date") {
            return declaration.plain_text();
        }
        let (year, month, day) = today;
        let iso = format!("{year:04}-{month:02}-{day:02}");
        declaration
            .attr(&Ns::Style, "data-style-name")
            .and_then(|name| {
                self.document
                    .styles
                    .data_styles()
                    .format(name, &super::Value::Date(iso.clone()))
            })
            .unwrap_or(iso)
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
        ..portrait
    }
}

/// Put text in place of the footer, date and slide-number fields under an
/// element.
fn put_fields(element: &mut Element, text: &str) {
    for node in &mut element.children {
        let Node::Element(child) = node else {
            continue;
        };
        if child.is(&Ns::Presentation, "footer")
            || child.is(&Ns::Presentation, "date-time")
            || child.is(&Ns::Text, "page-number")
        {
            *node = Node::Text(text.to_owned());
        } else {
            put_fields(child, text);
        }
    }
}
