//! Cells merged into one, and taken apart again.
//!
//! A merge is its top left cell spanning the others, with
//! `table:number-columns-spanned` and `table:number-rows-spanned`, and each
//! cell under it written `table:covered-table-cell`. A covered cell keeps what
//! it held, as ODF allows, so that unmerging gives it back.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::ops::RangeInclusive;

use super::size::table_mut;
use super::{CellNames, Sheet, SheetDocument, reach_cell, reach_row};
use crate::edit::Refused;
use crate::xml::Ns;

/// The rows and the columns a merged cell covers.
pub type Merged = (RangeInclusive<usize>, RangeInclusive<usize>);

impl SheetDocument {
    /// Every merged cell on a sheet, as the rows and columns it covers.
    pub fn merges(&self, sheet: &Sheet) -> Vec<Merged> {
        let mut merges = Vec::new();
        for range in &sheet.rows {
            let Some(row) = self.row_element(sheet, range.first) else {
                continue;
            };
            let mut column = 0usize;
            for cell in row.elements() {
                let covered = cell.is(&Ns::Table, "covered-table-cell");
                if !covered && !cell.is(&Ns::Table, "table-cell") {
                    continue;
                }
                let repeat = cell
                    .attr_usize(&Ns::Table, "number-columns-repeated")
                    .unwrap_or(1)
                    .max(1);
                let span = |name| cell.attr_usize(&Ns::Table, name).unwrap_or(1).max(1);
                let (across, down) = (span("number-columns-spanned"), span("number-rows-spanned"));
                if !covered && (across > 1 || down > 1) {
                    for first in range.first..range.first + range.count {
                        for left in column..column + repeat {
                            merges.push((first..=first + down - 1, left..=left + across - 1));
                        }
                    }
                }
                column += repeat;
            }
        }
        merges
    }

    /// The merged cell a cell is part of, whether as the one spanning or as
    /// one covered.
    pub fn merge_at(&self, sheet: &Sheet, row: usize, column: usize) -> Option<Merged> {
        self.merges(sheet)
            .into_iter()
            .find(|(rows, columns)| rows.contains(&row) && columns.contains(&column))
    }

    /// Merge a range into its top left cell. A merge wholly inside the range
    /// becomes part of the new one.
    ///
    /// # Errors
    ///
    /// The sheet does not exist, or a merged cell lies partly inside the range
    /// and partly out. Nothing is changed.
    pub fn merge(
        &mut self,
        sheet: usize,
        rows: RangeInclusive<usize>,
        columns: RangeInclusive<usize>,
    ) -> Result<(), Refused> {
        let index = self.sheets.get(sheet).ok_or(Refused::NotFound)?;
        let within = |(r, c): &Merged| {
            r.start() >= rows.start()
                && r.end() <= rows.end()
                && c.start() >= columns.start()
                && c.end() <= columns.end()
        };
        let meets = |(r, c): &Merged| {
            r.start() <= rows.end()
                && rows.start() <= r.end()
                && c.start() <= columns.end()
                && columns.start() <= c.end()
        };
        if self.merges(index).iter().any(|m| meets(m) && !within(m)) {
            return Err(Refused::Structure);
        }
        let (top, left) = (*rows.start(), *columns.start());
        let (down, across) = (rows.clone().count(), columns.clone().count());
        self.each_cell(sheet, rows, columns, |cell, names, row, column| {
            cell.remove_attr(&Ns::Table, "number-columns-spanned");
            cell.remove_attr(&Ns::Table, "number-rows-spanned");
            if (row, column) == (top, left) {
                cell.name = names.cell.clone();
                if across > 1 {
                    cell.set_attr(names.columns_spanned.clone(), across.to_string());
                }
                if down > 1 {
                    cell.set_attr(names.rows_spanned.clone(), down.to_string());
                }
            } else {
                cell.name = names.covered.clone();
            }
        })
    }

    /// Take apart the merged cell a cell is part of, giving each covered cell
    /// back what it held.
    ///
    /// # Errors
    ///
    /// The sheet does not exist, or the cell is not part of a merge. Nothing is
    /// changed.
    pub fn unmerge(&mut self, sheet: usize, row: usize, column: usize) -> Result<(), Refused> {
        let index = self.sheets.get(sheet).ok_or(Refused::NotFound)?;
        let (rows, columns) = self.merge_at(index, row, column).ok_or(Refused::NotFound)?;
        self.each_cell(sheet, rows, columns, |cell, names, _, _| {
            cell.remove_attr(&Ns::Table, "number-columns-spanned");
            cell.remove_attr(&Ns::Table, "number-rows-spanned");
            cell.name = names.cell.clone();
        })
    }

    /// Change each cell of a range, each standing alone in its own element.
    fn each_cell(
        &mut self,
        sheet: usize,
        rows: RangeInclusive<usize>,
        columns: RangeInclusive<usize>,
        mut change: impl FnMut(&mut crate::xml::Element, &CellNames, usize, usize),
    ) -> Result<(), Refused> {
        let names = CellNames::of(&self.document);
        for row in rows {
            let index = self.sheets.get(sheet).ok_or(Refused::NotFound)?;
            let table_at = index.table;
            let range = index
                .row_range(row)
                .map(|r| (r.path.steps().to_vec(), r.first, r.count));
            let written = index.rows.last().map_or(0, |r| r.first + r.count);
            let table = table_mut(&mut self.document, table_at)?;
            let element = reach_row(table, range.as_ref(), row, written, &names)?;
            for column in columns.clone() {
                let at = reach_cell(element, column, &names);
                let cell = element.at_mut(&[at]).ok_or(Refused::NotFound)?;
                change(cell, &names, row, column);
            }
            self.reindex();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Value;

    fn sheet() -> SheetDocument {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/libreoffice/calc.ods"),
        )
        .expect("the corpus sheet");
        SheetDocument::read(&bytes).expect("it reads")
    }

    #[test]
    fn a_merge_covers_its_range_and_unmerging_gives_the_cells_back() {
        let mut document = sheet();
        document
            .set_cell(0, 20, 1, &Value::Text("kept".into()))
            .expect("written");
        document.merge(0, 20..=21, 0..=2).expect("merged");
        let index = &document.sheets()[0];
        assert_eq!(document.merge_at(index, 21, 2), Some((20..=21, 0..=2)));
        let corner = document.cell(index, 20, 0).expect("the corner");
        assert_eq!((corner.rows_spanned(), corner.columns_spanned()), (2, 3));
        assert!(document.cell(index, 20, 1).expect("covered").covered);
        assert!(document.can_edit(0, 21, 1).is_err(), "under the merge");

        let bytes = document.document.write_verified().expect("written");
        let mut again = SheetDocument::read(&bytes).expect("read again");
        again.unmerge(0, 21, 1).expect("unmerged");
        let index = &again.sheets()[0];
        assert!(again.merge_at(index, 20, 0).is_none());
        let cell = again.cell(index, 20, 1).expect("the cell");
        assert!(!cell.covered);
        assert_eq!(cell.text(), "kept");
    }

    #[test]
    fn a_merge_takes_in_one_inside_it_and_refuses_one_across_its_edge() {
        let mut document = sheet();
        document.merge(0, 20..=20, 0..=1).expect("merged");
        assert_eq!(document.merge(0, 20..=22, 1..=2), Err(Refused::Structure));
        document.merge(0, 20..=22, 0..=2).expect("merged around it");
        let index = &document.sheets()[0];
        assert_eq!(document.merges(index), vec![(20..=22, 0..=2)]);
    }
}
