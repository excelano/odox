//! A spreadsheet laid out onto pages: each sheet that is shown cut into
//! pages of cells.
//!
//! A sheet prints its print range as far as the cells in use, or every cell
//! up to the last that holds anything, leaving out the rows and columns it
//! hides. Columns that do not
//! fit across a page go on pages of their own, down the sheet and then across
//! it unless its page style says across first, and the page style's scale or
//! its number of pages to fit decides how large the cells are drawn. A cell
//! is drawn as the window draws it: its fill and borders, and its text on one
//! line from the top, cut at the cell's edge. There are no grid lines. Each
//! sheet begins a page with its name as a heading and is one table to a
//! reader however many pages it takes; the rows the sheet marks as its header
//! are the table's header and are drawn again at the top of every page, and
//! a sheet that marks none has its first row read as the header. DESIGN.md
//! §12.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::num::NonZeroU32;
use std::ops::Range;

use krilla::tagging::{TableHeaderScope, Tag, TagKind};
use odox_core::doc::{DEFAULT_COLUMN_WIDTH, DEFAULT_ROW_HEIGHT, Sheet, SheetDocument};
use odox_core::{Fit, Length, Properties, TextAlign};

use crate::fonts::Faces;
use crate::layout::{Item, Layout};
use crate::model::{self, Block, Paragraph};
use crate::tags::NodeId;
use crate::text;
use crate::{Exported, Laid, Options, Page, Refusal};

/// The space a cell keeps between its edges and its text, as in the window.
const PADDING: f32 = 3.0;
/// The size a sheet's name is drawn at above its first page of cells.
const HEADING_SIZE: f32 = 14.0;
/// The space between the sheet's name and its cells.
const HEADING_GAP: f32 = 6.0;
/// The smallest a fitted sheet is drawn, as a share of its size.
const SMALLEST: f32 = 0.1;

/// A spreadsheet's cells are not pictures: the window draws none of the
/// pictures and charts a sheet can hold, and neither does the export.
pub(crate) fn undescribed(_: &SheetDocument) -> Vec<Vec<usize>> {
    Vec::new()
}

/// Export a spreadsheet as PDF/UA-1.
pub(crate) fn export(document: &SheetDocument, options: &Options) -> Result<Exported, Refusal> {
    let mut faces = Faces::new();
    let laid = lay_out(document, &mut faces)?;
    crate::write(
        laid,
        faces,
        &crate::About {
            title: crate::title(&document.document, options),
            language: crate::language(&document.document, "table-cell")
                .unwrap_or_else(|| options.language.clone()),
            creator: "Odox Grid",
        },
    )
}

fn lay_out(document: &SheetDocument, faces: &mut Faces) -> Result<Laid, Refusal> {
    let mut layout = Layout::new(faces, 0.0);
    let mut pages = Vec::new();
    let mut headings = Vec::new();
    for sheet in document.sheets().iter().filter(|sheet| sheet.visible) {
        let mut sheets = Sheets {
            layout: &mut layout,
            document,
            sheet,
        };
        let Some(laid) = sheets.sheet()? else {
            continue;
        };
        headings.push((1, sheet.name.clone(), pages.len(), laid.top));
        pages.extend(laid.pages);
    }
    if pages.is_empty() {
        return Err(Refusal::Empty);
    }
    Ok(Laid {
        pages,
        tree: layout.tree,
        headings,
    })
}

/// One sheet's pages, and where its heading is on the first of them.
struct SheetPages {
    pages: Vec<Page>,
    top: f32,
}

/// Cut a run of sizes into bands no larger than a capacity, the first band
/// with a capacity of its own. A size larger than the capacity is a band by
/// itself.
fn bands(sizes: &[f32], first: f32, rest: f32) -> Vec<Range<usize>> {
    let mut bands = Vec::new();
    let mut start = 0;
    let mut used = 0.0;
    for (index, size) in sizes.iter().enumerate() {
        let capacity = if bands.is_empty() { first } else { rest };
        if index > start && used + size > capacity {
            bands.push(start..index);
            start = index;
            used = 0.0;
        }
        used += size;
    }
    if start < sizes.len() {
        bands.push(start..sizes.len());
    }
    bands
}

/// The band each position falls in.
fn band_of(bands: &[Range<usize>], count: usize) -> Vec<usize> {
    let mut of = vec![0; count];
    for (band, range) in bands.iter().enumerate() {
        for at in range.clone() {
            of[at] = band;
        }
    }
    of
}

/// Where each position begins within its band, given the sizes.
fn offsets(bands: &[Range<usize>], sizes: &[f32]) -> Vec<f32> {
    let mut at = vec![0.0; sizes.len()];
    for range in bands {
        let mut y = 0.0;
        for index in range.clone() {
            at[index] = y;
            y += sizes[index];
        }
    }
    at
}

/// The largest share, no larger than one, at which a sheet's bands fit a
/// rule, trying in steps of a hundredth down to the smallest.
fn largest(fits: impl Fn(f32) -> bool) -> f32 {
    (0..=90)
        .map(|step| {
            #[allow(clippy::cast_precision_loss)]
            let share = 1.0 - step as f32 / 100.0;
            share
        })
        .find(|&share| fits(share))
        .unwrap_or(SMALLEST)
}

/// The cells of one sheet turned into pages.
struct Sheets<'l, 'f, 'd> {
    layout: &'l mut Layout<'f>,
    document: &'d SheetDocument,
    sheet: &'d Sheet,
}

impl Sheets<'_, '_, '_> {
    /// One sheet, or nothing where it shows no cells.
    #[allow(clippy::too_many_lines)]
    fn sheet(&mut self) -> Result<Option<SheetPages>, Refusal> {
        let (document, sheet) = (self.document, self.sheet);
        // A print range goes no further than the cells in use: one written as
        // whole columns, which is how Excel writes a print area, reaches the
        // last row a sheet can have.
        let used = (0..sheet.used_rows, 0..sheet.used_columns);
        let (row_range, column_range) = match document.print_range(sheet) {
            Some((rows, columns)) => (
                rows.start..rows.end.min(used.0.end),
                columns.start..columns.end.min(used.1.end),
            ),
            None => used,
        };
        let rows: Vec<usize> = row_range.filter(|&row| sheet.row_shown(row)).collect();
        let columns: Vec<usize> = column_range
            .filter(|&column| sheet.column_shown(column))
            .collect();
        if rows.is_empty() || columns.is_empty() {
            return Ok(None);
        }

        let page = document.page_layout(sheet);
        let margin = |length: Option<Length>| length.map_or(0.0, Length::points);
        let (top, left) = (margin(page.margin.top), margin(page.margin.left));
        let (width, height) = (page.width.points(), page.height.points());
        let across = (width - left - margin(page.margin.right)).max(72.0);
        let down = (height - top - margin(page.margin.bottom)).max(72.0);

        let heights: Vec<f32> = rows
            .iter()
            .map(|&row| {
                document
                    .row_height(sheet, row)
                    .map_or(DEFAULT_ROW_HEIGHT, Length::points)
            })
            .collect();
        let widths: Vec<f32> = columns
            .iter()
            .map(|&column| {
                sheet
                    .column_width(column)
                    .map_or(DEFAULT_COLUMN_WIDTH, Length::points)
            })
            .collect();
        let repeated: Vec<usize> = (0..rows.len())
            .filter(|&at| sheet.is_header_row(rows[at]))
            .collect();
        let repeated_height: f32 = repeated.iter().map(|&at| heights[at]).sum();
        let header: Vec<usize> = if repeated.is_empty() {
            vec![0]
        } else {
            repeated.clone()
        };

        // The sheet's name, which begins its first page and is read first.
        let section = self.layout.tree.group(None, Tag::Section);
        let base = document.cell_style(sheet, None, columns[0]);
        let mut style = model::style(&base.text, HEADING_SIZE, None);
        style.variant.bold = true;
        let mut name = Paragraph::plain(&sheet.name, &style);
        name.heading = Some(1);
        let mut pieces = Vec::new();
        self.layout.blocks(
            &[Block::Paragraph(name)],
            left,
            across,
            Some(section),
            &mut pieces,
        )?;
        let mut heading = Vec::new();
        let mut heading_height = 0.0;
        for piece in pieces {
            heading.extend(
                piece
                    .items
                    .into_iter()
                    .map(|item| item.shifted(0.0, heading_height)),
            );
            heading_height += piece.height;
        }
        heading_height += HEADING_GAP;

        let column_bands = |share: f32| bands(&widths, across / share, across / share);
        let row_bands = |share: f32| {
            bands(
                &heights,
                (down - heading_height) / share,
                down / share - repeated_height,
            )
        };
        let share = match page.fit {
            Fit::Scale(share) => share.clamp(SMALLEST, 4.0),
            Fit::Pages(pages) => largest(|share| {
                column_bands(share).len() * row_bands(share).len() <= pages as usize
            }),
            Fit::Within { across, down } => largest(|share| {
                across.is_none_or(|n| column_bands(share).len() <= n as usize)
                    && down.is_none_or(|n| row_bands(share).len() <= n as usize)
            }),
        };
        let column_bands = column_bands(share);
        let row_bands = row_bands(share);
        let column_band = band_of(&column_bands, columns.len());
        let row_band = band_of(&row_bands, rows.len());
        let column_at = offsets(&column_bands, &widths);
        let row_at = offsets(&row_bands, &heights);

        let count = column_bands.len() * row_bands.len();
        let page_of = |rows_band: usize, columns_band: usize| {
            if page.down_first {
                columns_band * row_bands.len() + rows_band
            } else {
                rows_band * column_bands.len() + columns_band
            }
        };
        let mut pages: Vec<Page> = (0..count).map(|_| Page::new(width, height)).collect();
        pages[page_of(0, 0)]
            .items
            .extend(heading.into_iter().map(|item| (top, item)));

        // Where a row band's cells begin below the page's top margin.
        let band_top = |band: usize| {
            if band == 0 {
                heading_height
            } else {
                repeated_height * share
            }
        };
        let grid = Grid {
            rows: &rows,
            columns: &columns,
            heights: &heights,
            widths: &widths,
            share,
        };
        // What a cell is cut to: the band of columns it is on, the page's
        // height below the top margin.
        let clip = |column_index: usize| {
            let band = column_bands[column_band[column_index]].clone();
            (
                left,
                0.0,
                band.map(|c| widths[c] * share).sum::<f32>(),
                down,
            )
        };

        let table = self.layout.tree.group(Some(section), Tag::Table);
        for at in 0..rows.len() {
            let band = row_band[at];
            let y = band_top(band) + row_at[at] * share;
            let read = Read::Cell {
                row: self.layout.tree.group(Some(table), Tag::TR),
                header: header.contains(&at),
            };
            for column_index in 0..columns.len() {
                let x = left + column_at[column_index] * share;
                let items =
                    self.cell(&grid, (at, column_index), (x, y), clip(column_index), read)?;
                pages[page_of(band, column_band[column_index])]
                    .items
                    .extend(items.into_iter().map(|item| (top, item)));
            }
        }

        // The marked header rows again at the top of every page below the
        // first, as decoration: a reader has read them once. They are laid out
        // under a group nothing drawn is read under, which the tree drops.
        let again = Read::Again(self.layout.tree.group(None, Tag::Div));
        for band in 1..row_bands.len() {
            let mut y = 0.0;
            for &at in &repeated {
                for column_index in 0..columns.len() {
                    let x = left + column_at[column_index] * share;
                    let items =
                        self.cell(&grid, (at, column_index), (x, y), clip(column_index), again)?;
                    pages[page_of(band, column_band[column_index])]
                        .items
                        .extend(items.into_iter().map(|item| (top, item.decoration())));
                }
                y += heights[at] * share;
            }
        }

        Ok(Some(SheetPages { pages, top }))
    }

    /// One cell's fill, borders and text, at its place on the page relative
    /// to the top margin, cut to the band of columns it is on.
    fn cell(
        &mut self,
        grid: &Grid<'_>,
        (at, column_index): (usize, usize),
        (x, y): (f32, f32),
        clip: (f32, f32, f32, f32),
        read: Read,
    ) -> Result<Vec<Item>, Refusal> {
        let (document, sheet) = (self.document, self.sheet);
        let (row, column) = (grid.rows[at], grid.columns[column_index]);
        let cell = document.cell(sheet, row, column);
        if cell.as_ref().is_some_and(|cell| cell.covered) {
            return Ok(Vec::new());
        }
        let style = document.cell_style(sheet, cell.as_ref(), column);

        // A merged cell covers the shown rows and columns its span reaches.
        let across = cell
            .as_ref()
            .map_or(1, odox_core::doc::Cell::columns_spanned);
        let down = cell.as_ref().map_or(1, odox_core::doc::Cell::rows_spanned);
        let spanned_columns: Vec<usize> = (column_index..grid.columns.len())
            .take_while(|&c| grid.columns[c] < column + across)
            .collect();
        let spanned_rows: Vec<usize> = (at..grid.rows.len())
            .take_while(|&r| grid.rows[r] < row + down)
            .collect();
        let width: f32 = spanned_columns.iter().map(|&c| grid.widths[c]).sum::<f32>() * grid.share;
        let height: f32 = spanned_rows.iter().map(|&r| grid.heights[r]).sum::<f32>() * grid.share;

        let parent = match read {
            Read::Again(node) => node,
            Read::Cell { row, header } => {
                let spans = |count: usize| {
                    NonZeroU32::new(u32::try_from(count).unwrap_or(1)).filter(|n| n.get() > 1)
                };
                let kind: TagKind = if header {
                    Tag::TH(TableHeaderScope::Column)
                        .with_col_span(spans(spanned_columns.len()))
                        .with_row_span(spans(spanned_rows.len()))
                        .into()
                } else {
                    Tag::TD
                        .with_col_span(spans(spanned_columns.len()))
                        .with_row_span(spans(spanned_rows.len()))
                        .into()
                };
                self.layout.tree.group(Some(row), kind)
            }
        };

        let mut items = Vec::new();
        if let Some(fill) = style.cell.background {
            items.push(Item::Rect {
                x,
                y,
                width,
                height,
                color: fill,
            });
        }
        for (from, to, border) in [
            ((x, y), (x, y + height), style.cell.border.left),
            (
                (x + width, y),
                (x + width, y + height),
                style.cell.border.right,
            ),
            ((x, y), (x + width, y), style.cell.border.top),
            (
                (x, y + height),
                (x + width, y + height),
                style.cell.border.bottom,
            ),
        ] {
            if let Some(border) = border {
                items.push(Item::Rule {
                    from,
                    to,
                    width: (border.width.points() * grid.share).max(0.25),
                    color: border.color,
                });
            }
        }

        let text = cell
            .as_ref()
            .map(odox_core::doc::Cell::text)
            .unwrap_or_default();
        let cut = intersect((x, y, width, height), clip);
        if text.is_empty() {
            // An empty cell is still a cell to a reader.
            if matches!(read, Read::Cell { .. }) {
                let leaf = self.layout.tree.leaf(parent);
                items.push(Item::Empty { x, y, leaf });
            }
        } else {
            let numeric = cell.as_ref().is_some_and(|cell| cell.value().is_numeric());
            for item in self.text(&text, &style, numeric, (x, y, width), grid.share, parent)? {
                items.push(Item::Clipped {
                    rect: cut,
                    item: Box::new(item),
                });
            }
        }
        Ok(items
            .into_iter()
            .map(|item| match item {
                Item::Rect { .. } | Item::Rule { .. } => Item::Clipped {
                    rect: clip,
                    item: Box::new(item),
                },
                other => other,
            })
            .collect())
    }

    /// A cell's text on one line from its top, a line to each of its
    /// paragraphs: text against the left edge and numbers against the right,
    /// unless its paragraph style says otherwise, as ODF and the window have
    /// it.
    fn text(
        &mut self,
        text: &str,
        style: &Properties,
        numeric: bool,
        (x, y, width): (f32, f32, f32),
        share: f32,
        parent: NodeId,
    ) -> Result<Vec<Item>, Refusal> {
        let size = model::size_of(&style.text, model::DEFAULT_SIZE) * share;
        let run = model::style(&style.text, size, None);
        let paragraph = Paragraph::plain(text, &run);
        let glyphs = text::shape(&paragraph, self.layout.faces)?;
        let natural = text::lines(text, &glyphs, f32::MAX, f32::MAX)
            .into_iter()
            .map(|line| text::width(text, &glyphs, line))
            .fold(0.0_f32, f32::max);
        let padding = PADDING * share;
        let start = match style.paragraph.align {
            Some(TextAlign::Center) => x + (width - natural) / 2.0,
            Some(TextAlign::End) => x + width - padding - natural,
            None if numeric => x + width - padding - natural,
            _ => x + padding,
        };
        let mut pieces = Vec::new();
        self.layout.blocks(
            &[Block::Paragraph(paragraph)],
            start,
            natural + 1.0,
            Some(parent),
            &mut pieces,
        )?;
        let mut items = Vec::new();
        let mut below = y;
        for piece in pieces {
            items.extend(piece.items.into_iter().map(|item| item.shifted(0.0, below)));
            below += piece.height;
        }
        Ok(items)
    }
}

/// How a cell is read: as one of a row's cells, header or not, or not at
/// all, being drawn again under a group nothing is read under.
#[derive(Clone, Copy)]
enum Read {
    Cell { row: NodeId, header: bool },
    Again(NodeId),
}

/// The shown rows and columns of a sheet and their sizes, with the share
/// they are drawn at.
struct Grid<'a> {
    rows: &'a [usize],
    columns: &'a [usize],
    heights: &'a [f32],
    widths: &'a [f32],
    share: f32,
}

/// The part of one rectangle inside another, each `(x, y, width, height)`.
fn intersect(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    let left = a.0.max(b.0);
    let top = a.1.max(b.1);
    let right = (a.0 + a.2).min(b.0 + b.2);
    let bottom = (a.1 + a.3).min(b.1 + b.3);
    (left, top, (right - left).max(0.0), (bottom - top).max(0.0))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn fixture() -> SheetDocument {
        let bytes = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/export.ods"),
        )
        .expect("the export fixture");
        SheetDocument::read(&bytes).expect("it reads")
    }

    /// The text drawn on a page, with whether each is read or decoration.
    fn texts(page: &Page) -> Vec<(String, bool)> {
        fn text(item: &Item) -> Option<(String, bool)> {
            match item {
                Item::Text { text, leaf, .. } => Some((text.clone(), leaf.is_some())),
                Item::Clipped { item, .. } => text(item),
                _ => None,
            }
        }
        page.items
            .iter()
            .filter_map(|(_, item)| text(item))
            .collect()
    }

    fn laid() -> Laid {
        let mut faces = Faces::new();
        lay_out(&fixture(), &mut faces).expect("laid out")
    }

    #[test]
    fn hidden_sheets_rows_and_columns_and_an_empty_sheet_are_left_out() {
        let laid = laid();
        let titles: Vec<&str> = laid.headings.iter().map(|h| h.1.as_str()).collect();
        assert_eq!(titles, ["Orders", "Summary"]);
        let all: Vec<String> = laid
            .pages
            .iter()
            .flat_map(texts)
            .map(|(text, _)| text)
            .collect();
        assert!(!all.iter().any(|t| t == "This row is hidden"));
        assert!(!all.iter().any(|t| t == "Column 3"), "column C is hidden");
        assert!(!all.iter().any(|t| t == "Never exported"));
        assert!(all.iter().any(|t| t == "Column 4"));
    }

    #[test]
    fn a_wide_long_sheet_runs_down_then_across_with_its_header_again_on_each_page() {
        let laid = laid();
        // Orders takes two bands of rows and two of columns; Summary one page.
        assert_eq!(laid.pages.len(), 5);
        assert_eq!(laid.headings[1].2, 4, "Summary begins after Orders' four");
        let first = texts(&laid.pages[0]);
        let below = texts(&laid.pages[1]);
        assert!(first.contains(&("Column 1".to_owned(), true)));
        assert!(first.contains(&("Order 1".to_owned(), true)));
        // Down first: the second page is the rest of the first columns, under
        // the header drawn again as decoration.
        assert!(below.contains(&("Column 1".to_owned(), false)));
        assert!(below.iter().any(|(t, read)| t == "Order 70" && *read));
        assert!(!below.iter().any(|(t, _)| t == "Column 12"));
        let across = texts(&laid.pages[2]);
        assert!(across.contains(&("Column 12".to_owned(), true)));
    }

    #[test]
    fn a_print_range_is_all_that_is_printed_and_fit_to_a_page() {
        let laid = laid();
        let summary = texts(&laid.pages[4]);
        let shown: Vec<&str> = summary.iter().map(|(t, _)| t.as_str()).collect();
        assert!(shown.contains(&"Q2"));
        assert!(
            !shown.contains(&"Q3"),
            "column D is outside the print range"
        );
        assert!(!shown.contains(&"Region 4"), "row 5 is outside it");
    }

    #[test]
    fn bands_hold_what_fits_and_a_size_too_large_is_a_band_of_its_own() {
        assert_eq!(bands(&[40.0, 40.0, 40.0], 100.0, 100.0), [0..2, 2..3]);
        assert_eq!(bands(&[40.0, 40.0, 40.0], 50.0, 100.0), [0..1, 1..3]);
        assert_eq!(bands(&[250.0, 10.0], 100.0, 100.0), [0..1, 1..2]);
    }
}
