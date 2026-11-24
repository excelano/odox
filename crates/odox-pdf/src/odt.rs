//! A text document laid out onto pages: its body read into blocks the way the
//! window reads it, laid out at the width of its first page style and filled
//! into pages of that style's size. DESIGN.md §12.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::VecDeque;

use odox_core::Length;
use odox_core::doc::TextDocument;

use odox_core::doc::PageFields;

use crate::bands::{Bands, Regions};
use crate::fonts::Faces;
use crate::layout::{self, Layout, Piece};
use crate::model::{self, Block};
use crate::{Exported, Options, Page, Refusal};

impl std::error::Error for Refusal {}

/// The figures an export would draw that say nothing in place of their
/// picture, as paths from the content root: what has to be described, or
/// marked decorative, before the document can be exported.
pub fn undescribed(document: &TextDocument) -> Vec<Vec<usize>> {
    let mut found = Vec::new();
    model::undescribed(&blocks(document), &mut found);
    found
}

fn blocks(document: &TextDocument) -> Vec<Block> {
    match (document.body(), document.body_path()) {
        (Some(body), Some(path)) => model::read(&document.document, body, path, None),
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
    let mut faces = Faces::new();
    let laid = lay_out(document, options, &mut faces)?;
    crate::write(
        laid,
        faces,
        &crate::About {
            title: crate::title(&document.document, options),
            language: crate::language(&document.document, "paragraph")
                .unwrap_or_else(|| options.language.clone()),
            creator: "Odox Text",
        },
    )
}

/// The document on pages, each item where it is drawn, and the headings the
/// outline lists.
fn lay_out(
    document: &TextDocument,
    options: &Options,
    faces: &mut Faces,
) -> Result<crate::Laid, Refusal> {
    let blocks = blocks(document);
    let mut missing = Vec::new();
    model::undescribed(&blocks, &mut missing);
    if !missing.is_empty() {
        return Err(Refusal::Undescribed(missing));
    }

    let page = document.page_layout();
    let margin = |length: Option<Length>| length.map_or(0.0, Length::points);
    let (left, right) = (margin(page.margin.left), margin(page.margin.right));
    let (page_width, page_height) = (page.width.points(), page.height.points());
    let body_width = (page_width - left - right).max(72.0);
    let title = crate::title(&document.document, options);
    let fields = |page: usize, pages: usize| PageFields {
        page,
        pages,
        sheet: None,
        title: &title,
        today: options.today,
    };

    let mut layout = Layout::new(faces, 0.0);
    let bands = Bands::new(
        &mut layout,
        &document.document,
        document.master_page(),
        &page,
        Regions::Own,
        &fields(1, 1),
    )?;
    let top = margin(page.margin.top) + bands.above();
    let bottom = page_height - margin(page.margin.bottom) - bands.below();
    let body_height = (bottom - top).max(72.0);
    layout.page_height = body_height;
    let mut pieces = Vec::new();
    layout.blocks(&blocks, left, body_width, None, &mut pieces)?;

    let mut headings = Vec::new();
    let placed = paginate(pieces, body_height, &layout::foot_rule(left, body_width));
    let count = placed.len();
    let mut pages = Vec::with_capacity(count);
    for (index, placed) in placed.into_iter().enumerate() {
        let mut page = Page::new(page_width, page_height);
        for (y, piece) in placed {
            let y = top + y;
            if let Some((level, title)) = piece.heading {
                headings.push((level, title, index, y));
            }
            page.items
                .extend(piece.items.into_iter().map(|item| (y, item)));
        }
        page.items
            .extend(bands.draw(&mut layout, &fields(index + 1, count), (top, bottom))?);
        pages.push(page);
    }
    let tree = layout.tree;
    Ok(crate::Laid {
        pages,
        tree,
        headings,
    })
}

/// Fill pages with pieces, each placed at its top within the page's body.
///
/// A piece goes on the page it fits on. One that asks for a new page, one
/// that does not fit, and one that has to stay with the piece after it where
/// the two do not fit together, begins the next page, below the header rows
/// of the table it is a row of. Only a piece taller than a whole page is cut.
fn paginate(pieces: Vec<Piece>, body: f32, rule: &Piece) -> Vec<Vec<(f32, Piece)>> {
    let mut pages = Vec::new();
    let mut page = Filling::default();
    let mut queue: VecDeque<Piece> = pieces.into();
    while let Some(mut piece) = queue.pop_front() {
        let notes: f32 = piece.footnotes.iter().map(|note| note.height).sum();
        let ruled = if notes > 0.0 && page.foot.is_empty() {
            rule.height
        } else {
            0.0
        };
        if !page.pieces.is_empty() {
            let room = body - page.foot_height() - notes - ruled;
            let fits = page.y + piece.before + piece.height <= room;
            let together = match queue.front().filter(|_| piece.keep_with_next) {
                Some(next) => piece.before + piece.height + piece.after + next.before + next.height,
                None => 0.0,
            };
            // A run that has to stay together and is taller than a page is
            // let go, since no page could hold it.
            let kept = together > body || page.y + together <= room;
            if piece.page_before || !fits || !kept {
                let carried = page.finish(&mut pages, body, rule);
                page.start(carried, piece.repeat.as_deref(), body, rule);
            }
        }
        let gap = if page.pieces.is_empty() {
            0.0
        } else {
            piece.before
        };
        let room = body - page.foot_height();
        if page.y + gap + piece.height > room {
            // Taller than what an empty page has room for: cut it there and
            // carry the rest on.
            let (above, below) = layout::split(piece, (room - page.y - gap).max(1.0));
            queue.push_front(below);
            piece = above;
        }
        page.y += gap;
        let next = page.y + piece.height + piece.after;
        let footnotes = std::mem::take(&mut piece.footnotes);
        page.pieces.push((page.y, piece));
        page.y = next;
        for note in footnotes {
            page.add_note(note, body, rule);
        }
        if page.y > body - page.foot_height() && !queue.is_empty() {
            let carried = page.finish(&mut pages, body, rule);
            page.start(carried, None, body, rule);
        }
    }
    let mut carried = page.finish(&mut pages, body, rule);
    while !carried.is_empty() {
        let mut page = Filling::default();
        page.start(carried, None, body, rule);
        carried = page.finish(&mut pages, body, rule);
    }
    pages
}

/// A page being filled: the pieces placed down it, how far they reach, the
/// footnotes gathered at its foot, and those that did not fit there.
#[derive(Default)]
struct Filling {
    pieces: Vec<(f32, Piece)>,
    y: f32,
    foot: Vec<Piece>,
    carried: VecDeque<Piece>,
    /// The rule's height, once there are footnotes for it to stand over.
    ruled: f32,
}

impl Filling {
    /// How much of the page's foot its footnotes take, the rule above them
    /// included.
    fn foot_height(&self) -> f32 {
        if self.foot.is_empty() {
            return 0.0;
        }
        self.foot.iter().map(|note| note.height).sum::<f32>() + self.ruled
    }

    /// Take a footnote into the foot where there is room for it under what is
    /// on the page, and carry it on otherwise.
    fn add_note(&mut self, note: Piece, body: f32, rule: &Piece) {
        let ruled = if self.foot.is_empty() {
            rule.height
        } else {
            0.0
        };
        if self.carried.is_empty() && self.y + self.foot_height() + ruled + note.height <= body {
            self.ruled = rule.height;
            self.foot.push(note);
        } else {
            self.carried.push_back(note);
        }
    }

    /// Begin a page: a table's header rows to repeat, and the footnotes
    /// carried on from the page before at its foot, as far as they fit.
    fn start(
        &mut self,
        carried: VecDeque<Piece>,
        repeat: Option<&Vec<Piece>>,
        body: f32,
        rule: &Piece,
    ) {
        *self = Self::default();
        for row in repeat.into_iter().flatten() {
            self.pieces.push((self.y, row.clone()));
            self.y += row.height;
        }
        for note in carried {
            self.add_note(note, body, rule);
        }
    }

    /// Put the page's footnotes at its foot, below the rule, add the page to
    /// the pages, and answer the footnotes still to come.
    fn finish(
        &mut self,
        pages: &mut Vec<Vec<(f32, Piece)>>,
        body: f32,
        rule: &Piece,
    ) -> VecDeque<Piece> {
        let mut page = std::mem::take(self);
        if !page.foot.is_empty() {
            let mut y = body - page.foot_height();
            page.pieces.push((y, rule.clone()));
            y += rule.height;
            for note in page.foot.drain(..) {
                let height = note.height;
                page.pieces.push((y, note));
                y += height;
            }
        }
        pages.push(page.pieces);
        page.carried
    }
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

    const RULE: f32 = 10.0;

    fn rule() -> Piece {
        piece(RULE)
    }

    fn cited(height: f32, notes: &[f32]) -> Piece {
        Piece {
            height,
            footnotes: notes.iter().map(|&h| piece(h)).collect(),
            ..Piece::default()
        }
    }

    /// Each page's pieces as (top, height).
    fn placed(pages: &[Vec<(f32, Piece)>]) -> Vec<Vec<(f32, f32)>> {
        pages
            .iter()
            .map(|page| page.iter().map(|(y, p)| (*y, p.height)).collect())
            .collect()
    }

    #[test]
    fn pdf_footnote_on_same_page_as_citation() {
        let pages = paginate(vec![cited(40.0, &[15.0]), piece(20.0)], 100.0, &rule());
        assert_eq!(
            placed(&pages),
            [vec![(0.0, 40.0), (40.0, 20.0), (75.0, RULE), (85.0, 15.0)]]
        );
    }

    #[test]
    fn a_footnote_takes_its_room_from_the_page() {
        let pages = paginate(vec![piece(60.0), cited(20.0, &[15.0])], 100.0, &rule());
        assert_eq!(heights(&pages), [vec![60.0], vec![20.0, RULE, 15.0]]);
    }

    #[test]
    fn pdf_long_footnote_continues_on_next_page() {
        let pages = paginate(vec![cited(40.0, &[30.0, 30.0, 30.0])], 100.0, &rule());
        assert_eq!(
            heights(&pages),
            [vec![40.0, RULE, 30.0], vec![RULE, 30.0, 30.0]]
        );
    }

    #[test]
    fn pieces_fill_a_page_and_the_next_begins_where_one_does_not_fit() {
        let pages = paginate(vec![piece(40.0), piece(40.0), piece(30.0)], 100.0, &rule());
        assert_eq!(heights(&pages), [vec![40.0, 40.0], vec![30.0]]);
    }

    #[test]
    fn space_above_a_piece_is_dropped_at_the_top_of_a_page() {
        let spaced = Piece {
            before: 50.0,
            ..piece(60.0)
        };
        let pages = paginate(vec![piece(60.0), spaced], 100.0, &rule());
        assert_eq!(pages[1][0].0, 0.0);
    }

    #[test]
    fn a_heading_does_not_end_a_page_without_the_line_after_it() {
        let heading = Piece {
            keep_with_next: true,
            ..piece(20.0)
        };
        let pages = paginate(vec![piece(70.0), heading, piece(20.0)], 100.0, &rule());
        assert_eq!(heights(&pages), [vec![70.0], vec![20.0, 20.0]]);
    }

    #[test]
    fn a_page_break_asked_for_is_taken_but_never_makes_a_blank_page() {
        let breaking = Piece {
            page_before: true,
            ..piece(10.0)
        };
        let pages = paginate(
            vec![breaking.clone(), piece(10.0), breaking],
            100.0,
            &rule(),
        );
        assert_eq!(heights(&pages), [vec![10.0, 10.0], vec![10.0]]);
    }

    #[test]
    fn a_table_that_runs_onto_a_new_page_draws_its_header_again() {
        let header = Rc::new(vec![piece(5.0)]);
        let row = Piece {
            repeat: Some(Rc::clone(&header)),
            ..piece(30.0)
        };
        let pages = paginate(
            vec![piece(5.0), row.clone(), row.clone(), row],
            70.0,
            &rule(),
        );
        assert_eq!(heights(&pages), [vec![5.0, 30.0, 30.0], vec![5.0, 30.0]]);
    }

    #[test]
    fn a_piece_taller_than_a_page_is_cut_across_pages() {
        let pages = paginate(vec![piece(250.0)], 100.0, &rule());
        let total: f32 = heights(&pages).iter().flatten().sum();
        assert_eq!(pages.len(), 3);
        assert!((total - 250.0).abs() < 0.01);
    }

    #[test]
    fn pdf_starts_new_page_at_break() {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/libreoffice/text.odt"),
        )
        .expect("the text fixture");
        let mut document = TextDocument::read(&bytes).expect("it reads");
        let options = Options {
            title: "Untitled".to_owned(),
            language: "en-US".to_owned(),
            untitled_slide: "Slide {number}".to_owned(),
            today: (2026, 1, 1),
        };
        let pages = |document: &TextDocument| {
            lay_out(document, &options, &mut Faces::new())
                .expect("laid out")
                .pages
                .len()
        };
        assert_eq!(pages(&document), 1);
        let body = document.body_path().expect("a body");
        let heading = document
            .body()
            .expect("a body")
            .elements_indexed()
            .filter(|(_, e)| e.is(&odox_core::Ns::Text, "h"))
            .nth(1)
            .map(|(index, _)| index)
            .expect("a second heading");
        let path = [body.as_slice(), &[heading]].concat();
        let odox_core::Document {
            content, styles, ..
        } = &mut document.document;
        odox_core::edit::insert_page_break(content, &path, 0, styles).expect("broken");
        assert_eq!(pages(&document), 2);
    }

    #[test]
    fn pdf_endnotes_after_body_and_footnotes_at_the_foot() {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/libreoffice/text.odt"),
        )
        .expect("the text fixture");
        let mut document = TextDocument::read(&bytes).expect("it reads");
        let body = document.body_path().expect("a body");
        let first = document
            .body()
            .expect("a body")
            .elements_indexed()
            .find(|(_, e)| e.is(&odox_core::Ns::Text, "p"))
            .map(|(index, _)| index)
            .expect("a paragraph");
        let path = [body.as_slice(), &[first]].concat();
        let odox_core::Document {
            content, styles, ..
        } = &mut document.document;
        for (class, said) in [
            (odox_core::edit::NoteClass::Footnote, "At the foot"),
            (odox_core::edit::NoteClass::Endnote, "At the end"),
        ] {
            let at = odox_core::edit::insert_note(content, &path, 1, class, styles).expect("in");
            let note = at[..at.len() - 2].to_vec();
            odox_core::edit::set_note_text(content, &note, said).expect("written");
        }
        let options = Options {
            title: "Untitled".to_owned(),
            language: "en-US".to_owned(),
            untitled_slide: "Slide {number}".to_owned(),
            today: (2026, 1, 1),
        };
        let laid = lay_out(&document, &options, &mut Faces::new()).expect("laid out");
        let found = |said: &str| {
            laid.pages.iter().enumerate().find_map(|(page, items)| {
                items.items.iter().find_map(|(y, item)| match item {
                    layout::Item::Text { text, .. } if text.contains(said) => Some((page, *y)),
                    _ => None,
                })
            })
        };
        let (foot_page, foot_y) = found("At the foot").expect("the footnote");
        let (end_page, end_y) = found("At the end").expect("the endnote");
        let (body_page, body_y) = found("Whitespace").expect("the body's last heading");
        assert_eq!(foot_page, 0);
        assert!(
            foot_y > body_y,
            "the footnote is below the body on its page"
        );
        assert!(
            (end_page, end_y) > (body_page, body_y),
            "the endnote is after the body"
        );
    }
}
