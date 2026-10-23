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

use odox_core::doc::{
    DEFAULT_COLUMN_WIDTH as DEFAULT_COLUMN, DEFAULT_ROW_HEIGHT as DEFAULT_ROW, Merged, Sheet,
    SheetDocument,
};

/// The smallest a row or column may be drawn and still be found by a pointer.
/// A hidden one is drawn at no size, and a pointer finds the one after it.
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
    /// The sheet's merged cells.
    pub merges: Vec<Merged>,
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
            if sheet.row_shown(row) {
                let height = document
                    .row_height(sheet, row)
                    .map_or(DEFAULT_ROW, odox_core::Length::points);
                at += (height * zoom).max(MINIMUM);
            }
        }
        rows.push(at);

        let mut columns = Vec::with_capacity(column_count + 1);
        let mut at = 0.0;
        for column in 0..column_count {
            columns.push(at);
            if sheet.column_shown(column) {
                let width = sheet
                    .column_width(column)
                    .map_or(DEFAULT_COLUMN, odox_core::Length::points);
                at += (width * zoom).max(MINIMUM);
            }
        }
        columns.push(at);

        Self {
            rows,
            columns,
            merges: document.merges(sheet),
        }
    }

    /// The merged cell a cell is part of, or the cell alone.
    pub fn extent(&self, row: usize, column: usize) -> Merged {
        self.merges
            .iter()
            .find(|(rows, columns)| rows.contains(&row) && columns.contains(&column))
            .cloned()
            .unwrap_or((row..=row, column..=column))
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

    /// Draw a column at another width, moving every column after it.
    pub fn set_column(&mut self, index: usize, width: f32) {
        set_size(&mut self.columns, index, width);
    }

    /// Draw a row at another height, moving every row below it.
    pub fn set_row(&mut self, index: usize, height: f32) {
        set_size(&mut self.rows, index, height);
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

fn set_size(offsets: &mut [f32], index: usize, size: f32) {
    if index + 1 >= offsets.len() {
        return;
    }
    let change = size.max(MINIMUM) - (offsets[index + 1] - offsets[index]);
    for offset in &mut offsets[index + 1..] {
        *offset += change;
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
    use super::{Metrics, address, column_name, index_at};

    fn metrics() -> Metrics {
        Metrics {
            rows: vec![0.0, 10.0, 25.0, 45.0],
            columns: vec![0.0, 20.0, 40.0],
            merges: vec![(1..=2, 0..=1)],
        }
    }

    #[test]
    fn resizing_one_column_moves_the_ones_after_it() {
        let mut metrics = metrics();
        let (left, _) = metrics.column(1);
        let total = metrics.size().0;
        let (_, width) = metrics.column(0);
        metrics.set_column(0, width + 10.0);
        assert_eq!(metrics.column(1).0, left + 10.0);
        assert_eq!(metrics.size().0, total + 10.0);
    }

    #[test]
    fn a_hidden_row_is_never_the_one_a_pointer_finds() {
        let metrics = Metrics {
            rows: vec![0.0, 10.0, 10.0, 20.0],
            columns: vec![0.0, 20.0],
            merges: Vec::new(),
        };
        assert_eq!(metrics.row(1), (10.0, 0.0));
        assert_eq!(metrics.row_at(10.0), 2);
        assert_eq!(metrics.row_at(9.9), 0);
    }

    #[test]
    fn a_cell_in_a_merge_extends_to_the_whole_of_it() {
        let metrics = metrics();
        assert_eq!(metrics.extent(2, 1), (1..=2, 0..=1));
        assert_eq!(metrics.extent(0, 1), (0..=0, 1..=1));
    }

    #[test]
    fn size_is_the_last_offset_in_each_table() {
        assert_eq!(metrics().size(), (40.0, 45.0));
    }

    #[test]
    fn size_of_an_empty_table_is_zero() {
        let empty = Metrics {
            rows: Vec::new(),
            columns: Vec::new(),
            merges: Vec::new(),
        };
        assert_eq!(empty.size(), (0.0, 0.0));
    }

    #[test]
    fn row_and_column_give_a_start_and_a_length() {
        let metrics = metrics();
        assert_eq!(metrics.row(0), (0.0, 10.0));
        assert_eq!(metrics.row(2), (25.0, 20.0));
        assert_eq!(metrics.column(1), (20.0, 20.0));
        // Past the last real entry is the closing total, with nothing after it:
        // a length of zero rather than a panic.
        assert_eq!(metrics.row(3), (45.0, 0.0));
        assert_eq!(metrics.row(50), (0.0, 0.0));
    }

    #[test]
    fn row_at_and_column_at_answer_through_the_same_search_as_index_at() {
        let metrics = metrics();
        assert_eq!(metrics.row_at(24.9), 1);
        assert_eq!(metrics.column_at(20.0), 1);
    }

    #[test]
    fn rows_between_covers_every_row_a_span_touches() {
        let metrics = metrics();
        // 5 is in row 0, 30 is in row 2: both ends, and what is between.
        assert_eq!(metrics.rows_between(5.0, 30.0), 0..3);
        // A span entirely inside one row still names that row, not an empty range.
        assert_eq!(metrics.rows_between(1.0, 2.0), 0..1);
    }

    #[test]
    fn columns_between_stops_at_the_last_real_column() {
        let metrics = metrics();
        // Run off the end of the sheet: the range stops at the last column,
        // not at the closing total past it.
        assert_eq!(metrics.columns_between(0.0, 1000.0), 0..2);
    }

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
