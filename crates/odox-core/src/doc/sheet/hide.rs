//! Rows and columns hidden from view, and shown again.
//!
//! A hidden row or column is one whose element says `table:visibility`
//! "collapse"; shown is the attribute's absence, which also shows one a filter
//! hid. The rows or columns at each end of what is hidden or shown are split
//! out of their runs first, so that the attribute is written on whole elements
//! and on nothing past either end.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::ops::RangeInclusive;

use super::restructure::{columns_of, rows_of};
use super::size::{reach_column, table_mut};
use super::{CellNames, SheetDocument, reach_row};
use crate::edit::Refused;
use crate::xml::{Element, Ns};

/// Which of the two is hidden or shown.
#[derive(Clone, Copy)]
enum Line {
    Row,
    Column,
}

impl Line {
    /// Each element of the kind under a table, with the first line it stands
    /// for and how many.
    fn runs(self, table: &Element) -> Vec<(Vec<usize>, usize, usize)> {
        match self {
            Self::Row => rows_of(table),
            Self::Column => columns_of(table),
        }
    }

    /// Make the line stand alone in its own element.
    fn reach(self, table: &mut Element, at: usize, names: &CellNames) -> Result<(), Refused> {
        match self {
            Self::Row => {
                let runs = rows_of(table);
                let range = runs
                    .iter()
                    .find(|(_, first, count)| (*first..first + count).contains(&at));
                let written = runs.last().map_or(0, |(_, first, count)| first + count);
                reach_row(table, range, at, written, names).map(|_| ())
            }
            Self::Column => reach_column(table, at, names).map(|_| ()),
        }
    }
}

impl SheetDocument {
    /// Hide rows, or show them.
    ///
    /// # Errors
    ///
    /// The sheet does not exist. Nothing is changed.
    pub fn set_rows_shown(
        &mut self,
        sheet: usize,
        rows: RangeInclusive<usize>,
        shown: bool,
    ) -> Result<(), Refused> {
        self.set_shown(sheet, Line::Row, rows, shown)
    }

    /// Hide columns, or show them.
    ///
    /// # Errors
    ///
    /// The sheet does not exist. Nothing is changed.
    pub fn set_columns_shown(
        &mut self,
        sheet: usize,
        columns: RangeInclusive<usize>,
        shown: bool,
    ) -> Result<(), Refused> {
        self.set_shown(sheet, Line::Column, columns, shown)
    }

    fn set_shown(
        &mut self,
        sheet: usize,
        line: Line,
        lines: RangeInclusive<usize>,
        shown: bool,
    ) -> Result<(), Refused> {
        let table_at = self.sheets.get(sheet).ok_or(Refused::NotFound)?.table;
        let names = CellNames::of(&self.document);
        let visibility = self.document.name(&Ns::Table, "visibility");
        let table = table_mut(&mut self.document, table_at)?;
        let written = line
            .runs(table)
            .last()
            .map_or(0, |(_, first, count)| first + count);
        // Past what the table writes every line is shown already.
        let (first, last) = if shown {
            (
                *lines.start(),
                (*lines.end()).min(written.saturating_sub(1)),
            )
        } else {
            (*lines.start(), *lines.end())
        };
        if first > last || (shown && written == 0) {
            return Ok(());
        }
        line.reach(table, first, &names)?;
        line.reach(table, last, &names)?;
        for (path, start, count) in line.runs(table) {
            if start < first || start + count - 1 > last {
                continue;
            }
            let element = table.at_mut(&path).ok_or(Refused::NotFound)?;
            if shown {
                element.remove_attr(&Ns::Table, "visibility");
            } else {
                element.set_attr(visibility.clone(), "collapse");
            }
        }
        self.reindex();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> SheetDocument {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/libreoffice/calc.ods"),
        )
        .expect("the corpus sheet");
        SheetDocument::read(&bytes).expect("it reads")
    }

    #[test]
    fn hidden_rows_are_hidden_alone_and_shown_again() {
        let mut document = sheet();
        document.set_rows_shown(0, 1..=2, false).expect("hidden");
        let index = &document.sheets()[0];
        assert!(index.row_shown(0));
        assert!(!index.row_shown(1));
        assert!(!index.row_shown(2));
        assert!(index.row_shown(3));
        let bytes = document.document.write().expect("written");
        let mut again = SheetDocument::read(&bytes).expect("read again");
        assert!(!again.sheets()[0].row_shown(2), "kept by a save");
        again.set_rows_shown(0, 0..=5, true).expect("shown");
        assert!((0..=5).all(|row| again.sheets()[0].row_shown(row)));
    }

    #[test]
    fn columns_hide_past_the_ones_the_table_defines() {
        let mut document = sheet();
        let past = document.sheets()[0].used_columns + 5;
        document
            .set_columns_shown(0, 2..=past, false)
            .expect("hidden");
        let index = &document.sheets()[0];
        assert!(index.column_shown(1));
        assert!(!index.column_shown(2));
        assert!(!index.column_shown(past));
        assert!(index.column_shown(past + 1));
        document.set_columns_shown(0, 3..=3, true).expect("shown");
        let index = &document.sheets()[0];
        assert!(!index.column_shown(2));
        assert!(index.column_shown(3));
        assert!(!index.column_shown(4));
        assert!(document.document.write_verified().is_ok());
    }
}
