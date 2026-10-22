//! A text document laid out onto pages: its body read into blocks the way the
//! window reads it, laid out at the width of its first page style and filled
//! into pages of that style's size. DESIGN.md §12.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::VecDeque;

use odox_core::Length;
use odox_core::doc::TextDocument;

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
    model::undescribed(&blocks, &mut missing);
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

    let mut headings = Vec::new();
    let pages = paginate(pieces, body_height)
        .into_iter()
        .enumerate()
        .map(|(index, placed)| {
            let mut page = Page::new(page_width, page_height);
            for (y, piece) in placed {
                let y = top + y;
                if let Some((level, title)) = piece.heading {
                    headings.push((level, title, index, y));
                }
                page.items
                    .extend(piece.items.into_iter().map(|item| (y, item)));
            }
            page
        })
        .collect();

    crate::write(
        crate::Laid {
            pages,
            tree,
            headings,
        },
        faces,
        &crate::About {
            title: crate::title(&document.document, options),
            language: crate::language(&document.document, "paragraph")
                .unwrap_or_else(|| options.language.clone()),
            creator: "Odox Text",
        },
    )
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
