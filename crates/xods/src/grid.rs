//! The grid: where every row and column of a sheet sits, and which of them are
//! on the screen.
//!
//! A sheet is drawn by painting the cells the viewport covers and no others, so
//! the cost of a frame is the size of the window rather than the size of the
//! document. Finding which cells those are means knowing where every row begins,
//! which is what the cumulative tables here are: built once when a sheet is
//! shown, searched twice per frame.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::doc::{Sheet, SheetDocument};

/// A row's height when neither the row nor its style gives one, in ODF points.
///
/// The height a spreadsheet gives a row of text at its default size, which is
/// what an application shows for a sheet nobody has resized.
const DEFAULT_ROW: f32 = 13.0;

/// A column's width when nothing gives one, in ODF points.
const DEFAULT_COLUMN: f32 = 64.0;

/// The smallest a row or column may be drawn and still be found by a pointer.
const MINIMUM: f32 = 4.0;

/// How many empty rows and columns are shown past the end of a sheet's content,
/// so that the grid looks like a grid rather than like a table.
const MARGIN_CELLS: usize = 40;

/// Where every row and column of one sheet begins, in screen points.
///
/// Each table holds one more entry than there are rows or columns: the last is
/// the total, which is what the scroll area is told the content measures.
pub struct Metrics {
    /// The top of each row, and finally the bottom of the last.
    pub rows: Vec<f32>,
    /// The left of each column, and finally the right of the last.
    pub columns: Vec<f32>,
}

impl Metrics {
    /// Measure a sheet at a zoom.
    pub fn of(document: &SheetDocument, sheet: &Sheet, zoom: f32) -> Self {
        let row_count = sheet.used_rows + MARGIN_CELLS;
        let column_count = sheet.used_columns.max(1) + MARGIN_CELLS;

        let mut rows = Vec::with_capacity(row_count + 1);
        let mut at = 0.0;
        for row in 0..row_count {
            rows.push(at);
            let height = document
                .row_height(sheet, row)
                .map_or(DEFAULT_ROW, odox_core::Length::points);
            at += (height * zoom).max(MINIMUM);
        }
        rows.push(at);

        let mut columns = Vec::with_capacity(column_count + 1);
        let mut at = 0.0;
        for column in 0..column_count {
            columns.push(at);
            let width = sheet
                .column_width(column)
                .map_or(DEFAULT_COLUMN, odox_core::Length::points);
            at += (width * zoom).max(MINIMUM);
        }
        columns.push(at);

        Self { rows, columns }
    }

    /// The whole grid's size.
    pub fn size(&self) -> (f32, f32) {
        (
            self.columns.last().copied().unwrap_or(0.0),
            self.rows.last().copied().unwrap_or(0.0),
        )
    }

    /// The rows that any part of a vertical span covers.
    pub fn rows_between(&self, top: f32, bottom: f32) -> std::ops::Range<usize> {
        span(&self.rows, top, bottom)
    }

    /// The columns that any part of a horizontal span covers.
    pub fn columns_between(&self, left: f32, right: f32) -> std::ops::Range<usize> {
        span(&self.columns, left, right)
    }

    /// A row's top and height.
    pub fn row(&self, index: usize) -> (f32, f32) {
        at(&self.rows, index)
    }

    /// A column's left and width.
    pub fn column(&self, index: usize) -> (f32, f32) {
        at(&self.columns, index)
    }

    /// The row an offset falls in, for a pointer.
    pub fn row_at(&self, offset: f32) -> usize {
        index_at(&self.rows, offset)
    }

    /// The column an offset falls in.
    pub fn column_at(&self, offset: f32) -> usize {
        index_at(&self.columns, offset)
    }
}

fn at(offsets: &[f32], index: usize) -> (f32, f32) {
    let start = offsets.get(index).copied().unwrap_or(0.0);
    let end = offsets.get(index + 1).copied().unwrap_or(start);
    (start, end - start)
}

fn span(offsets: &[f32], from: f32, to: f32) -> std::ops::Range<usize> {
    let first = index_at(offsets, from);
    let last = index_at(offsets, to);
    first..(last + 1).min(offsets.len().saturating_sub(1))
}

/// Which cell an offset falls in.
///
/// The offsets rise, so this is a binary search; `partition_point` gives the
/// number of entries at or before the offset, and the entry before that is the
/// one it falls inside.
fn index_at(offsets: &[f32], offset: f32) -> usize {
    let offset = offset.max(0.0);
    offsets
        .partition_point(|start| *start <= offset)
        .saturating_sub(1)
        .min(offsets.len().saturating_sub(2))
}

/// A column's name: `A`, `B`, … `Z`, `AA`, which is how a spreadsheet addresses
/// one and how a formula refers to it.
pub fn column_name(mut column: usize) -> String {
    let mut name = Vec::new();
    loop {
        name.push(b'A' + u8::try_from(column % 26).unwrap_or(0));
        if column < 26 {
            break;
        }
        column = column / 26 - 1;
    }
    name.reverse();
    String::from_utf8(name).unwrap_or_default()
}

/// A cell's address as a person writes it: `B7`.
pub fn address(row: usize, column: usize) -> String {
    format!("{}{}", column_name(column), row + 1)
}

#[cfg(test)]
mod tests {
    use super::{address, column_name, index_at};

    #[test]
    fn column_names_carry_the_way_a_spreadsheet_does() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(27), "AB");
        assert_eq!(column_name(51), "AZ");
        assert_eq!(column_name(52), "BA");
        assert_eq!(column_name(701), "ZZ");
        assert_eq!(column_name(702), "AAA");
        // ODF's last column, which is where a repeat run of 16384 ends.
        assert_eq!(column_name(16_383), "XFD");
        assert_eq!(address(6, 1), "B7");
    }

    #[test]
    fn an_offset_finds_the_cell_it_is_inside() {
        let offsets = [0.0, 10.0, 25.0, 45.0];
        assert_eq!(index_at(&offsets, 0.0), 0);
        assert_eq!(index_at(&offsets, 9.9), 0);
        assert_eq!(index_at(&offsets, 10.0), 1);
        assert_eq!(index_at(&offsets, 24.9), 1);
        assert_eq!(index_at(&offsets, 25.0), 2);
        // Past the end is the last cell, not a panic and not a cell that is not
        // there: a pointer dragged off the bottom of the grid selects the last row.
        assert_eq!(index_at(&offsets, 1000.0), 2);
        assert_eq!(index_at(&offsets, -5.0), 0);
    }
}
