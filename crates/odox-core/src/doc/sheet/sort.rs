//! Sorting the rows of a block by one of its columns. DESIGN.md §11.
//!
//! The order is a spreadsheet's: numbers, dates and times first, then text by
//! its letters whatever their case, the whole reversed for a descending sort,
//! and empty cells last whichever way it goes; rows that tie keep their order. A row's cells move
//! together, each with its cell style and its comment, and a formula moves as a copy does, its
//! relative references going with it, which is what `LibreOffice` and Excel do
//! unless asked otherwise. Formulas elsewhere that read the sorted cells go on
//! reading the same cells.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::cmp::Ordering;

use super::fill::{Block, Source, offset};
use super::recalc::{date_serial, duration_days};
use super::{Place, SheetDocument, Value};
use crate::edit::Refused;
use crate::formula;

impl SheetDocument {
    /// Sort a block's rows by the cells in one of its columns, leaving its
    /// first row where it is when that is a header. Answers the cells written,
    /// for [`Self::recalculate`].
    ///
    /// # Errors
    ///
    /// The sheet does not exist, the column is outside the block, or a cell in
    /// the block is merged or under a merge. Nothing is changed in any case.
    pub fn sort(
        &mut self,
        sheet: usize,
        block: Block,
        column: usize,
        descending: bool,
        header: bool,
    ) -> Result<Vec<Place>, Refused> {
        let index = self.sheets.get(sheet).ok_or(Refused::NotFound)?;
        if !(block.left..=block.right).contains(&column) {
            return Err(Refused::NotFound);
        }
        for row in block.top..=block.bottom {
            for c in block.left..=block.right {
                let merged = self.cell(index, row, c).is_some_and(|cell| {
                    cell.covered || cell.columns_spanned() > 1 || cell.rows_spanned() > 1
                });
                if merged {
                    return Err(Refused::Structure);
                }
            }
        }
        let first = block.top + usize::from(header);
        if first >= block.bottom {
            return Ok(Vec::new());
        }
        let rows: Vec<Vec<Source>> = (first..=block.bottom)
            .map(|row| {
                (block.left..=block.right)
                    .map(|c| self.source(sheet, row, c))
                    .collect()
            })
            .collect();
        let notes: Vec<Vec<_>> = (first..=block.bottom)
            .map(|row| {
                (block.left..=block.right)
                    .map(|c| self.note(sheet, row, c))
                    .collect()
            })
            .collect();
        let key = column - block.left;
        let mut order: Vec<usize> = (0..rows.len()).collect();
        order.sort_by(|&a, &b| ordered(&rows[a][key].value, &rows[b][key].value, descending));
        if order.iter().enumerate().all(|(at, from)| at == *from) {
            return Ok(Vec::new());
        }
        let mut written = Vec::new();
        for (at, from) in order.into_iter().enumerate() {
            let row = first + at;
            for (source, note) in rows[from].iter().zip(&notes[from]) {
                let c = source.column;
                self.set_style_name(sheet, row, c, source.style.as_deref())?;
                self.put_note(sheet, row, c, note.clone())?;
                match &source.formula {
                    Some(stored) => {
                        let moved = formula::shifted(stored, offset(row, source.row), 0);
                        self.set_formula(sheet, row, c, &moved)?;
                    }
                    None => self.set_cell(sheet, row, c, &source.value)?,
                }
                written.push((sheet, row, c));
            }
        }
        Ok(written)
    }

    /// Whether a block's first row reads as a header: every cell in it is
    /// text, or empty, and the row below holds a number or a date.
    pub fn has_header(&self, sheet: usize, block: Block) -> bool {
        let Some(index) = self.sheets.get(sheet) else {
            return false;
        };
        if block.bottom <= block.top {
            return false;
        }
        let value = |row, column| {
            self.cell(index, row, column)
                .map_or(Value::Empty, |cell| cell.value())
        };
        let all_text = (block.left..=block.right)
            .all(|c| matches!(value(block.top, c), Value::Text(_) | Value::Empty));
        let some_text =
            (block.left..=block.right).any(|c| matches!(value(block.top, c), Value::Text(_)));
        let figures_below = (block.left..=block.right)
            .any(|c| !matches!(value(block.top + 1, c), Value::Text(_) | Value::Empty));
        all_text && some_text && figures_below
    }

    /// The block of filled cells around one, as far as filled cells reach
    /// across and down from it without a wholly empty row or column between.
    pub fn region(&self, sheet: usize, row: usize, column: usize) -> Block {
        let mut block = Block {
            top: row,
            left: column,
            bottom: row,
            right: column,
        };
        let Some(index) = self.sheets.get(sheet) else {
            return block;
        };
        let filled = |r: usize, c: usize| {
            self.cell(index, r, c)
                .is_some_and(|cell| cell.value() != Value::Empty || cell.covered)
        };
        let any_in_row = |r: usize, b: &Block| (b.left..=b.right).any(|c| filled(r, c));
        let any_in_column = |c: usize, b: &Block| (b.top..=b.bottom).any(|r| filled(r, c));
        loop {
            let before = block;
            if block.top > 0 && any_in_row(block.top - 1, &block) {
                block.top -= 1;
            }
            if block.bottom + 1 < index.used_rows && any_in_row(block.bottom + 1, &block) {
                block.bottom += 1;
            }
            if block.left > 0 && any_in_column(block.left - 1, &block) {
                block.left -= 1;
            }
            if block.right + 1 < index.used_columns && any_in_column(block.right + 1, &block) {
                block.right += 1;
            }
            if block == before {
                return block;
            }
        }
    }
}

/// The order of two key cells: numbers before text, reversed for a descending
/// sort, empty cells last either way, and text compared without its case.
fn ordered(a: &Value, b: &Value, descending: bool) -> Ordering {
    let rank = |value: &Value| match value {
        Value::Number(_)
        | Value::Percentage(_)
        | Value::Currency(..)
        | Value::Date(_)
        | Value::Time(_)
        | Value::Boolean(_) => 0,
        Value::Text(_) => 1,
        Value::Empty => 2,
    };
    let (ra, rb) = (rank(a), rank(b));
    if ra == 2 || rb == 2 {
        return ra.cmp(&rb);
    }
    let order = match (a, b) {
        _ if ra != rb => ra.cmp(&rb),
        (Value::Text(x), Value::Text(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
        _ => number(a).total_cmp(&number(b)),
    };
    if descending { order.reverse() } else { order }
}

fn number(value: &Value) -> f64 {
    match value {
        Value::Number(n) | Value::Percentage(n) | Value::Currency(n, _) => *n,
        Value::Boolean(b) => f64::from(u8::from(*b)),
        Value::Date(text) => date_serial(text).unwrap_or(0.0),
        Value::Time(text) => duration_days(text).unwrap_or(0.0),
        Value::Text(_) | Value::Empty => 0.0,
    }
}
