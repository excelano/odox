//! An OpenDocument file exported as a tagged PDF that conforms to PDF/UA-1.
//!
//! The windows do not paginate; an export does. Each kind of document lays
//! itself out onto pages its own way — a text document's body filled into its
//! page style, a spreadsheet's sheets cut into pages of cells, a presentation
//! a page per slide — and all three are written here by krilla with its
//! PDF/UA-1 validation on: a figure without alternative text, a missing title
//! or a glyph that stands for no character fails the export rather than
//! producing a file that only claims to conform. DESIGN.md §12.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

#![forbid(unsafe_code)]

mod fonts;
mod layout;
mod model;
mod odp;
mod odt;
mod tags;
mod text;

use std::collections::HashMap;
use std::fmt;

use krilla::action::{Action, LinkAction};
use krilla::annotation::{Annotation, LinkAnnotation, Target};
use krilla::configure::ConfigurationBuilder;
use krilla::configure::validate::Accessibility;
use krilla::destination::XyzDestination;
use krilla::geom::{PathBuilder, Point, Rect, Size, Transform};
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::outline::{Outline, OutlineNode};
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, Stroke};
use krilla::surface::Surface;
use krilla::tagging::{Artifact, ArtifactType, ContentTag, Identifier, SpanTag};
use krilla::{Document as Pdf, SerializeSettings};
use odox_core::doc::{Presentation, TextDocument};
use odox_core::{Color, Document, Element, Ns};

use fonts::Faces;
pub use fonts::Substitution;
use layout::{Ground, Item, Segment};
use tags::Tree;

/// What an export is told that the document may not say itself.
pub struct Options {
    /// The title to give a document that has none of its own, which a PDF/UA
    /// file must have: the file's name, as a window shows it.
    pub title: String,
    /// The language to declare where the document declares none: the one the
    /// person exporting it reads.
    pub language: String,
    /// What a slide without a title is called in the outline, with
    /// `{number}` where its number goes.
    pub untitled_slide: String,
}

/// A finished export.
pub struct Exported {
    /// The PDF.
    pub pdf: Vec<u8>,
    /// Families drawn in another face because theirs may not be embedded.
    pub substituted: Vec<Substitution>,
}

/// Why an export was not made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Figures that have neither alternative text nor a mark that they are
    /// decorative, as paths from the content root.
    Undescribed(Vec<Vec<usize>>),
    /// A font family with no face on the machine that may be embedded.
    Font(String),
    /// A character no face on the machine has.
    Glyph(char),
    /// Nothing would be drawn: every slide is left out of the show.
    Empty,
    /// The writer could not produce a conforming file, with its reason.
    Writer(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Undescribed(paths) => {
                write!(f, "{} figures have no alternative text", paths.len())
            }
            Self::Font(family) => write!(f, "no face of {family} may be embedded"),
            Self::Glyph(character) => {
                write!(
                    f,
                    "no font has the character {character} (U+{:04X})",
                    u32::from(*character)
                )
            }
            Self::Empty => write!(f, "there is nothing to export"),
            Self::Writer(reason) => write!(f, "the PDF could not be written: {reason}"),
        }
    }
}

/// A document that can be exported.
pub trait Exportable {
    /// The package and everything the three formats share.
    fn document(&self) -> &Document;

    /// The same, for writing what the export asks for into it.
    fn document_mut(&mut self) -> &mut Document;

    /// The figures an export would draw that say nothing in place of their
    /// picture, as paths from the content root: what has to be described, or
    /// marked decorative, before the document can be exported.
    fn undescribed(&self) -> Vec<Vec<usize>>;

    /// Export the document as PDF/UA-1.
    ///
    /// # Errors
    ///
    /// A figure says nothing in place of its picture, a font or a character
    /// cannot be embedded, or the writer refused what it was given.
    fn export(&self, options: &Options) -> Result<Exported, Refusal>;
}

impl Exportable for TextDocument {
    fn document(&self) -> &Document {
        &self.document
    }

    fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    fn undescribed(&self) -> Vec<Vec<usize>> {
        odt::undescribed(self)
    }

    fn export(&self, options: &Options) -> Result<Exported, Refusal> {
        odt::export(self, options)
    }
}

impl Exportable for Presentation {
    fn document(&self) -> &Document {
        &self.document
    }

    fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    fn undescribed(&self) -> Vec<Vec<usize>> {
        odp::undescribed(self)
    }

    fn export(&self, options: &Options) -> Result<Exported, Refusal> {
        odp::export(self, options)
    }
}

/// One page as it is written: its size, and everything drawn on it, each
/// item placed below the top it is given.
pub(crate) struct Page {
    pub width: f32,
    pub height: f32,
    pub items: Vec<(f32, Item)>,
}

impl Page {
    pub(crate) fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            items: Vec::new(),
        }
    }
}

/// What a PDF says about itself.
pub(crate) struct About {
    pub title: String,
    pub language: String,
    pub creator: &'static str,
}

/// A document laid out onto pages.
pub(crate) struct Laid {
    pub pages: Vec<Page>,
    /// What is read on them, in reading order.
    pub tree: Tree,
    /// The outline: `(level, title, page, top)`.
    pub headings: Vec<(u8, String, usize, f32)>,
}

/// Write laid-out pages as PDF/UA-1.
pub(crate) fn write(laid: Laid, faces: Faces, about: &About) -> Result<Exported, Refusal> {
    let Laid {
        pages,
        tree,
        headings,
    } = laid;
    let settings = SerializeSettings {
        configuration: ConfigurationBuilder::new()
            .with_accessibility_validator(Accessibility::UA1)
            .finish()
            .map_err(|e| Refusal::Writer(format!("{e:?}")))?,
        ..SerializeSettings::default()
    };
    let mut pdf = Pdf::new_with(settings);
    let mut drawn: HashMap<usize, Identifier> = HashMap::new();
    for placed in pages {
        let mut page = pdf.start_page_with(
            PageSettings::from_wh(placed.width, placed.height)
                .ok_or_else(|| Refusal::Writer("the page has no size".to_owned()))?,
        );
        let mut links = Vec::new();
        {
            let mut surface = page.surface();
            for (top, item) in &placed.items {
                draw(&mut surface, item, *top, &faces, &mut drawn, &mut links);
            }
            surface.finish();
        }
        for (rect, uri, leaf) in links {
            let annotation = Annotation::new_link(
                LinkAnnotation::new(
                    rect,
                    Target::Action(Action::Link(LinkAction::new(uri.clone()))),
                ),
                Some(uri),
            );
            drawn.insert(leaf, page.add_tagged_annotation(annotation));
        }
        page.finish();
    }

    pdf.set_tag_tree(tree.finish(&drawn, &about.language));
    pdf.set_outline(outline_of(&headings));
    pdf.set_metadata(
        Metadata::new()
            .title(about.title.clone())
            .language(about.language.clone())
            .creator(about.creator.to_owned()),
    );
    let bytes = pdf
        .finish()
        .map_err(|e| Refusal::Writer(format!("{e:?}")))?;
    Ok(Exported {
        pdf: bytes,
        substituted: faces.substituted,
    })
}

/// The document's own title, or the one the export was given.
pub(crate) fn title(document: &Document, options: &Options) -> String {
    document
        .meta
        .title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| options.title.clone())
}

/// Draw one item at a page's height, marked as what it is.
fn draw(
    surface: &mut Surface<'_>,
    item: &Item,
    top: f32,
    faces: &Faces,
    drawn: &mut HashMap<usize, Identifier>,
    links: &mut Vec<(Rect, String, usize)>,
) {
    let decoration = || ContentTag::Artifact(Artifact::new(ArtifactType::Layout, None));
    match item {
        Item::Text {
            x,
            baseline,
            face,
            size,
            color,
            glyphs,
            text,
            leaf,
        } => {
            let id = surface.start_tagged(match leaf {
                Some(_) => ContentTag::Span(SpanTag::empty()),
                None => decoration(),
            });
            surface.set_fill(Some(fill(*color)));
            surface.draw_glyphs(
                Point::from_xy(*x, top + baseline),
                glyphs,
                faces.get(*face).font.clone(),
                text,
                *size,
                false,
            );
            surface.end_tagged();
            if let Some(leaf) = leaf {
                drawn.insert(*leaf, id);
            }
        }
        Item::Rect {
            x,
            y,
            width,
            height,
            color,
        } => {
            if let Some(rect) = Rect::from_xywh(*x, top + y, *width, *height) {
                let mut path = PathBuilder::new();
                path.push_rect(rect);
                if let Some(path) = path.finish() {
                    surface.start_tagged(decoration());
                    surface.set_fill(Some(fill(*color)));
                    surface.set_stroke(None);
                    surface.draw_path(&path);
                    surface.end_tagged();
                }
            }
        }
        Item::Rule {
            from,
            to,
            width,
            color,
        } => {
            let mut path = PathBuilder::new();
            path.move_to(from.0, top + from.1);
            path.line_to(to.0, top + to.1);
            if let Some(path) = path.finish() {
                surface.start_tagged(decoration());
                surface.set_fill(None);
                surface.set_stroke(Some(Stroke {
                    paint: rgb(*color).into(),
                    width: *width,
                    ..Stroke::default()
                }));
                surface.draw_path(&path);
                surface.set_stroke(None);
                surface.end_tagged();
            }
        }
        Item::Image {
            x,
            y,
            width,
            height,
            image,
            leaf,
        } => {
            if let Some(size) = Size::from_wh(*width, *height) {
                let id = surface.start_tagged(match leaf {
                    Some(_) => ContentTag::Other,
                    None => decoration(),
                });
                surface.push_transform(&Transform::from_translate(*x, top + y));
                surface.draw_image(image.clone(), size);
                surface.pop();
                surface.end_tagged();
                if let Some(leaf) = leaf {
                    drawn.insert(*leaf, id);
                }
            }
        }
        Item::Shape {
            outline,
            ground,
            stroke,
            leaf,
            ..
        } => {
            let mut builder = PathBuilder::new();
            for segment in outline {
                match *segment {
                    Segment::Move(x, y) => builder.move_to(x, top + y),
                    Segment::Line(x, y) => builder.line_to(x, top + y),
                    Segment::Cubic(a, b, c, d, e, f) => {
                        builder.cubic_to(a, top + b, c, top + d, e, top + f);
                    }
                    Segment::Close => builder.close(),
                }
            }
            let Some(path) = builder.finish() else {
                return;
            };
            let id = surface.start_tagged(match leaf {
                Some(_) => ContentTag::Other,
                None => decoration(),
            });
            match ground {
                Some(Ground::Paint(paint, opacity)) => {
                    surface.set_fill(Some(Fill {
                        paint: paint.clone(),
                        opacity: NormalizedF32::new(*opacity).unwrap_or(NormalizedF32::ONE),
                        rule: FillRule::NonZero,
                    }));
                    surface.set_stroke(None);
                    surface.draw_path(&path);
                }
                Some(Ground::Image(image, (x, y, width, height), opacity)) => {
                    if let Some(size) = Size::from_wh(*width, *height) {
                        surface.push_clip_path(&path, &FillRule::NonZero);
                        surface.push_opacity(
                            NormalizedF32::new(*opacity).unwrap_or(NormalizedF32::ONE),
                        );
                        surface.push_transform(&Transform::from_translate(*x, top + y));
                        surface.draw_image(image.clone(), size);
                        surface.pop();
                        surface.pop();
                        surface.pop();
                    }
                }
                None => {}
            }
            if let Some((color, width)) = stroke {
                surface.set_fill(None);
                surface.set_stroke(Some(Stroke {
                    paint: rgb(*color).into(),
                    width: *width,
                    ..Stroke::default()
                }));
                surface.draw_path(&path);
                surface.set_stroke(None);
            }
            surface.end_tagged();
            if let Some(leaf) = leaf {
                drawn.insert(*leaf, id);
            }
        }
        Item::Placed {
            origin,
            across,
            down,
            image,
            leaf,
        } => {
            let Some(size) = Size::from_wh(1.0, 1.0) else {
                return;
            };
            let id = surface.start_tagged(match leaf {
                Some(_) => ContentTag::Other,
                None => decoration(),
            });
            surface.push_transform(&Transform::from_row(
                across.0,
                across.1,
                down.0,
                down.1,
                origin.0,
                top + origin.1,
            ));
            surface.draw_image(image.clone(), size);
            surface.pop();
            surface.end_tagged();
            if let Some(leaf) = leaf {
                drawn.insert(*leaf, id);
            }
        }
        Item::Empty { leaf, .. } => {
            let id = surface.start_tagged(ContentTag::Other);
            surface.end_tagged();
            drawn.insert(*leaf, id);
        }
        Item::Link {
            x,
            y,
            width,
            height,
            uri,
            leaf,
        } => {
            if let Some(rect) = Rect::from_xywh(*x, top + y, *width, *height) {
                links.push((rect, uri.clone(), *leaf));
            }
        }
    }
}

pub(crate) fn rgb(color: Color) -> krilla::color::rgb::Color {
    krilla::color::rgb::Color::new(color.r, color.g, color.b)
}

fn fill(color: Color) -> Fill {
    Fill {
        paint: rgb(color).into(),
        ..Fill::default()
    }
}

/// The outline from the headings, nested by level, each pointing at the top
/// of its first line.
fn outline_of(headings: &[(u8, String, usize, f32)]) -> Outline {
    fn build(headings: &[(u8, String, usize, f32)], at: &mut usize, level: u8) -> Vec<OutlineNode> {
        let mut nodes = Vec::new();
        while let Some((own, title, page, y)) = headings.get(*at) {
            if *own < level && !nodes.is_empty() || *own < level && level > 1 {
                break;
            }
            *at += 1;
            let mut node = OutlineNode::new(
                title.clone(),
                XyzDestination::new(*page, Point::from_xy(0.0, *y)),
            );
            for child in build(headings, at, own + 1) {
                node.push_child(child);
            }
            nodes.push(node);
        }
        nodes
    }
    let mut outline = Outline::new();
    let mut at = 0;
    while at < headings.len() {
        for node in build(headings, &mut at, 1) {
            outline.push_child(node);
        }
    }
    outline
}

/// The language the document declares: its default style's for a family, as
/// `language-COUNTRY`, and failing that its metadata's.
pub(crate) fn language(document: &Document, family: &str) -> Option<String> {
    let from_style = document
        .styles_part
        .as_ref()
        .and_then(|styles| styles.child(&Ns::Office, "styles"))
        .into_iter()
        .flat_map(Element::elements)
        .find(|e| e.is(&Ns::Style, "default-style") && e.attr(&Ns::Style, "family") == Some(family))
        .and_then(|style| style.child(&Ns::Style, "text-properties"))
        .and_then(|text| {
            let language = text.attr(&Ns::Fo, "language").filter(|l| *l != "zxx")?;
            Some(
                match text.attr(&Ns::Fo, "country").filter(|c| *c != "none") {
                    Some(country) => format!("{language}-{country}"),
                    None => language.to_owned(),
                },
            )
        });
    from_style.or_else(|| document.meta.language.clone())
}
