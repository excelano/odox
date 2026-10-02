//! A text document exported as a tagged PDF that conforms to PDF/UA-1.
//!
//! The window does not paginate; an export does. The document is read into
//! blocks the way the window reads it, laid out at the width of its first
//! page style, filled into pages of that style's size, and written by krilla
//! with its PDF/UA-1 validation on: a figure without alternative text, a
//! missing title or a glyph that stands for no character fails the export
//! rather than producing a file that only claims to conform. DESIGN.md §12.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

#![forbid(unsafe_code)]

mod fonts;
mod layout;
mod model;
mod tags;
mod text;

use std::collections::{HashMap, VecDeque};
use std::fmt;

use krilla::action::{Action, LinkAction};
use krilla::annotation::{Annotation, LinkAnnotation, Target};
use krilla::configure::ConfigurationBuilder;
use krilla::configure::validate::Accessibility;
use krilla::destination::XyzDestination;
use krilla::geom::{PathBuilder, Point, Rect, Size, Transform};
use krilla::metadata::Metadata;
use krilla::outline::{Outline, OutlineNode};
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::surface::Surface;
use krilla::tagging::{Artifact, ArtifactType, ContentTag, Identifier, SpanTag};
use krilla::{Document as Pdf, SerializeSettings};
use odox_core::doc::TextDocument;
use odox_core::edit::Description;
use odox_core::{Color, Element, Length, Ns};

use fonts::Faces;
pub use fonts::Substitution;
use layout::{Item, Layout, Piece};
use model::Block;

/// What an export is told that the document may not say itself.
pub struct Options {
    /// The title to give a document that has none of its own, which a PDF/UA
    /// file must have: the file's name, as a window shows it.
    pub title: String,
    /// The language to declare where the document declares none: the one the
    /// person exporting it reads.
    pub language: String,
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
            Self::Writer(reason) => write!(f, "the PDF could not be written: {reason}"),
        }
    }
}

impl std::error::Error for Refusal {}

/// The figures an export would draw that say nothing in place of their
/// picture, as paths from the content root: what has to be described, or
/// marked decorative, before the document can be exported.
pub fn undescribed(document: &TextDocument) -> Vec<Vec<usize>> {
    let mut found = Vec::new();
    collect_undescribed(&blocks(document), &mut found);
    found
}

fn collect_undescribed(blocks: &[Block], into: &mut Vec<Vec<usize>>) {
    for block in blocks {
        match block {
            Block::Figure(figure) if figure.description == Description::Missing => {
                into.push(figure.path.clone());
            }
            Block::List(list) => {
                for item in &list.items {
                    collect_undescribed(&item.blocks, into);
                }
            }
            Block::Table(table) => {
                for cell in table.rows.iter().flatten() {
                    collect_undescribed(&cell.blocks, into);
                }
            }
            Block::TextBox(text_box) => collect_undescribed(&text_box.blocks, into),
            Block::Note(note) => collect_undescribed(&note.blocks, into),
            _ => {}
        }
    }
}

fn blocks(document: &TextDocument) -> Vec<Block> {
    match (document.body(), document.body_path()) {
        (Some(body), Some(path)) => model::read(&document.document, body, path),
        _ => Vec::new(),
    }
}

/// Export a text document as PDF/UA-1.
///
/// # Errors
///
/// A figure says nothing in place of its picture, a font or a character
/// cannot be embedded, or the writer refused what it was given.
pub fn export(document: &TextDocument, options: &Options) -> Result<Exported, Refusal> {
    let blocks = blocks(document);
    let mut missing = Vec::new();
    collect_undescribed(&blocks, &mut missing);
    if !missing.is_empty() {
        return Err(Refusal::Undescribed(missing));
    }

    let page = document.page_layout();
    let margin = |length: Option<Length>| length.map_or(0.0, Length::points);
    let (top, bottom) = (margin(page.margin.top), margin(page.margin.bottom));
    let (left, right) = (margin(page.margin.left), margin(page.margin.right));
    let (page_width, page_height) = (page.width.points(), page.height.points());
    let body_height = (page_height - top - bottom).max(72.0);
    let body_width = (page_width - left - right).max(72.0);

    let mut faces = Faces::new();
    let mut layout = Layout::new(&mut faces, body_height);
    let mut pieces = Vec::new();
    layout.blocks(&blocks, left, body_width, None, &mut pieces)?;
    let tree = layout.tree;

    let pages = paginate(pieces, body_height);

    let settings = SerializeSettings {
        configuration: ConfigurationBuilder::new()
            .with_accessibility_validator(Accessibility::UA1)
            .finish()
            .map_err(|e| Refusal::Writer(format!("{e:?}")))?,
        ..SerializeSettings::default()
    };
    let mut pdf = Pdf::new_with(settings);
    let mut drawn: HashMap<usize, Identifier> = HashMap::new();
    let mut outline = Vec::new();
    for (index, placed) in pages.iter().enumerate() {
        let mut page = pdf.start_page_with(
            PageSettings::from_wh(page_width, page_height)
                .ok_or_else(|| Refusal::Writer("the page has no size".to_owned()))?,
        );
        let mut links = Vec::new();
        {
            let mut surface = page.surface();
            for (y, piece) in placed {
                let y = top + y;
                if let Some((level, title)) = &piece.heading {
                    outline.push((*level, title.clone(), index, y));
                }
                for item in &piece.items {
                    draw(&mut surface, item, y, &faces, &mut drawn, &mut links);
                }
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

    let language = language(document).unwrap_or_else(|| options.language.clone());
    pdf.set_tag_tree(tree.finish(&drawn, &language));
    pdf.set_outline(outline_of(&outline));
    let title = document
        .document
        .meta
        .title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| options.title.clone());
    pdf.set_metadata(
        Metadata::new()
            .title(title)
            .language(language)
            .creator("Odox Text".to_owned()),
    );
    let bytes = pdf
        .finish()
        .map_err(|e| Refusal::Writer(format!("{e:?}")))?;
    Ok(Exported {
        pdf: bytes,
        substituted: faces.substituted,
    })
}

/// Fill pages with pieces, each placed at its top within the page's body.
///
/// A piece goes on the page it fits on. One that asks for a new page, one
/// that does not fit, and one that has to stay with the piece after it where
/// the two do not fit together, begins the next page, below the header rows
/// of the table it is a row of. Only a piece taller than a whole page is cut.
fn paginate(pieces: Vec<Piece>, body: f32) -> Vec<Vec<(f32, Piece)>> {
    let mut pages = Vec::new();
    let mut page: Vec<(f32, Piece)> = Vec::new();
    let mut y = 0.0;
    let mut queue: VecDeque<Piece> = pieces.into();
    while let Some(piece) = queue.pop_front() {
        if !page.is_empty() {
            let fits = y + piece.before + piece.height <= body;
            let together = match queue.front().filter(|_| piece.keep_with_next) {
                Some(next) => piece.before + piece.height + piece.after + next.before + next.height,
                None => 0.0,
            };
            // A run that has to stay together and is taller than a page is
            // let go, since no page could hold it.
            let kept = together > body || y + together <= body;
            if piece.page_before || !fits || !kept {
                pages.push(std::mem::take(&mut page));
                y = 0.0;
                for row in piece.repeat.iter().flat_map(|header| header.iter()) {
                    page.push((y, row.clone()));
                    y += row.height;
                }
            }
        }
        let gap = if page.is_empty() { 0.0 } else { piece.before };
        if y + gap + piece.height > body {
            // Taller than what an empty page has room for: cut it there and
            // carry the rest on.
            let (above, below) = layout::split(piece, (body - y - gap).max(1.0));
            page.push((y + gap, above));
            pages.push(std::mem::take(&mut page));
            y = 0.0;
            queue.push_front(below);
            continue;
        }
        y += gap;
        let next = y + piece.height + piece.after;
        page.push((y, piece));
        y = next;
    }
    pages.push(page);
    pages
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

fn rgb(color: Color) -> krilla::color::rgb::Color {
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

/// The language the document declares: its default paragraph style's, as
/// `language-COUNTRY`, and failing that its metadata's.
fn language(document: &TextDocument) -> Option<String> {
    let styles = document.document.styles_part.as_ref()?;
    let from_style = styles
        .child(&Ns::Office, "styles")
        .into_iter()
        .flat_map(Element::elements)
        .find(|e| {
            e.is(&Ns::Style, "default-style") && e.attr(&Ns::Style, "family") == Some("paragraph")
        })
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
    from_style.or_else(|| document.document.meta.language.clone())
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;

    fn piece(height: f32) -> Piece {
        Piece {
            height,
            ..Piece::default()
        }
    }

    /// Each page's pieces by height, which is how these tests tell them
    /// apart.
    fn heights(pages: &[Vec<(f32, Piece)>]) -> Vec<Vec<f32>> {
        pages
            .iter()
            .map(|page| page.iter().map(|(_, p)| p.height).collect())
            .collect()
    }

    #[test]
    fn pieces_fill_a_page_and_the_next_begins_where_one_does_not_fit() {
        let pages = paginate(vec![piece(40.0), piece(40.0), piece(30.0)], 100.0);
        assert_eq!(heights(&pages), [vec![40.0, 40.0], vec![30.0]]);
    }

    #[test]
    fn space_above_a_piece_is_dropped_at_the_top_of_a_page() {
        let spaced = Piece {
            before: 50.0,
            ..piece(60.0)
        };
        let pages = paginate(vec![piece(60.0), spaced], 100.0);
        assert_eq!(pages[1][0].0, 0.0);
    }

    #[test]
    fn a_heading_does_not_end_a_page_without_the_line_after_it() {
        let heading = Piece {
            keep_with_next: true,
            ..piece(20.0)
        };
        let pages = paginate(vec![piece(70.0), heading, piece(20.0)], 100.0);
        assert_eq!(heights(&pages), [vec![70.0], vec![20.0, 20.0]]);
    }

    #[test]
    fn a_page_break_asked_for_is_taken_but_never_makes_a_blank_page() {
        let breaking = Piece {
            page_before: true,
            ..piece(10.0)
        };
        let pages = paginate(vec![breaking.clone(), piece(10.0), breaking], 100.0);
        assert_eq!(heights(&pages), [vec![10.0, 10.0], vec![10.0]]);
    }

    #[test]
    fn a_table_that_runs_onto_a_new_page_draws_its_header_again() {
        let header = Rc::new(vec![piece(5.0)]);
        let row = Piece {
            repeat: Some(Rc::clone(&header)),
            ..piece(30.0)
        };
        let pages = paginate(vec![piece(5.0), row.clone(), row.clone(), row], 70.0);
        assert_eq!(heights(&pages), [vec![5.0, 30.0, 30.0], vec![5.0, 30.0]]);
    }

    #[test]
    fn a_piece_taller_than_a_page_is_cut_across_pages() {
        let pages = paginate(vec![piece(250.0)], 100.0);
        let total: f32 = heights(&pages).iter().flatten().sum();
        assert_eq!(pages.len(), 3);
        assert!((total - 250.0).abs() < 0.01);
    }
}
