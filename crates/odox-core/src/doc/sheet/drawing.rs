//! Pictures and shapes on a sheet: those in `table:shapes`, anchored to the
//! sheet, and those in a cell, anchored to it and, with
//! `table:end-cell-address`, stretched to another. DESIGN.md §11.
//!
//! A drawing is placed where its `svg:x` and `svg:y` say, from the sheet's
//! top left corner with hidden rows and columns taking no room, which is how
//! `LibreOffice` writes them. One anchored to a cell is not shown while that
//! cell's row or column is hidden, and moves with the cell when a row or
//! column before it is resized or hidden here; one stretched to an end cell
//! also takes the end cell's move.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::size::table_mut;
use super::{DEFAULT_COLUMN_WIDTH, DEFAULT_ROW_HEIGHT, Sheet, SheetDocument};
use crate::doc::refs::read_area;
use crate::edit::Refused;
use crate::value::Length;
use crate::xml::{Element, Ns};

/// Where a drawing is anchored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    /// To the sheet.
    Sheet,
    /// To a cell, and maybe stretched to another.
    Cell {
        /// The cell's row.
        row: usize,
        /// The cell's column.
        column: usize,
        /// The cell it is stretched to, as row, column and the distance into
        /// it in points across and down.
        end: Option<(usize, usize, f32, f32)>,
    },
}

/// A picture or a shape on a sheet.
pub struct SheetDrawing<'a> {
    /// The element itself.
    pub element: &'a Element,
    /// Where it is anchored.
    pub anchor: Anchor,
    /// Its top left corner from its anchor's, in points.
    pub offset: (f32, f32),
    /// Where the element is under its table.
    path: Vec<usize>,
}

/// A place on a sheet in points, across and down.
type Point = (f32, f32);

/// Where a drawing's anchor cell and end cell stand.
pub(super) type Corners = (Point, Option<Point>);

/// The last column and row a sheet can have.
const LAST_COLUMN: usize = 16_383;
const LAST_ROW: usize = 1_048_575;

/// The line, of those `size` measures from the first, that a distance from
/// the first's start falls in, and how far into it: one taking no room is
/// passed over.
fn within(distance: f32, last: usize, size: impl Fn(usize) -> f32) -> (usize, f32) {
    let mut start = 0.0;
    for line in 0..last {
        let next = start + size(line);
        if distance < next {
            return (line, distance - start);
        }
        start = next;
    }
    (last, distance - start)
}

/// A length attribute of a drawing in points, nought where it is missing.
fn length(element: &Element, ns: &Ns, local: &str) -> f32 {
    element
        .attr(ns, local)
        .and_then(Length::parse)
        .map_or(0.0, Length::points)
}

impl SheetDocument {
    /// The drawings on a sheet, in the order the sheet has them: those
    /// anchored to the sheet, then those in cells, row by row.
    pub fn drawings(&self, sheet: usize) -> Vec<SheetDrawing<'_>> {
        let mut found = Vec::new();
        let (Some(index), Some(table)) = (
            self.sheets.get(sheet),
            self.sheets.get(sheet).and_then(|index| self.table(index)),
        ) else {
            return found;
        };
        for (at, shapes) in table.elements_indexed() {
            if !shapes.is(&Ns::Table, "shapes") {
                continue;
            }
            for (inner, element) in shapes.elements_indexed() {
                if element.name.ns == Ns::Draw {
                    found.push(SheetDrawing {
                        element,
                        anchor: Anchor::Sheet,
                        offset: (
                            length(element, &Ns::Svg, "x"),
                            length(element, &Ns::Svg, "y"),
                        ),
                        path: vec![at, inner],
                    });
                }
            }
        }
        for range in &index.rows {
            let Some(row) = table.at(range.path.steps()) else {
                continue;
            };
            let mut column = 0usize;
            for (at, cell) in row.elements_indexed() {
                if !cell.is(&Ns::Table, "table-cell") && !cell.is(&Ns::Table, "covered-table-cell")
                {
                    continue;
                }
                for (inner, element) in cell.elements_indexed() {
                    if element.name.ns != Ns::Draw {
                        continue;
                    }
                    let end = element
                        .attr(&Ns::Table, "end-cell-address")
                        .and_then(read_area)
                        .map(|((row, column), _)| {
                            (
                                row,
                                column,
                                length(element, &Ns::Table, "end-x"),
                                length(element, &Ns::Table, "end-y"),
                            )
                        });
                    let (x, y) = self.origin(index, range.first, column);
                    found.push(SheetDrawing {
                        element,
                        anchor: Anchor::Cell {
                            row: range.first,
                            column,
                            end,
                        },
                        offset: (
                            length(element, &Ns::Svg, "x") - x,
                            length(element, &Ns::Svg, "y") - y,
                        ),
                        path: [range.path.steps(), &[at, inner]].concat(),
                    });
                }
                column += cell
                    .attr_usize(&Ns::Table, "number-columns-repeated")
                    .unwrap_or(1)
                    .max(1);
            }
        }
        found
    }

    /// Where a drawing stands on its sheet as `[x, y, width, height]` in points,
    /// or `None` while the row or column of the cell it is anchored to is
    /// hidden.
    pub fn drawing_place(&self, sheet: &Sheet, drawing: &SheetDrawing<'_>) -> Option<[f32; 4]> {
        let (x, y) = match drawing.anchor {
            Anchor::Sheet => (0.0, 0.0),
            Anchor::Cell { row, column, .. } => {
                if !sheet.row_shown(row) || !sheet.column_shown(column) {
                    return None;
                }
                self.origin(sheet, row, column)
            }
        };
        Some([
            x + drawing.offset.0,
            y + drawing.offset.1,
            length(drawing.element, &Ns::Svg, "width"),
            length(drawing.element, &Ns::Svg, "height"),
        ])
    }

    /// The cell a drawing stands in and how far into it its corner is, in
    /// points: the one it is anchored to, or for one anchored to the sheet
    /// the one its corner falls in. `None` where it is not shown.
    pub fn drawing_cell(
        &self,
        sheet: &Sheet,
        drawing: &SheetDrawing<'_>,
    ) -> Option<(usize, usize, (f32, f32))> {
        let [x, y, ..] = self.drawing_place(sheet, drawing)?;
        match drawing.anchor {
            Anchor::Cell { row, column, .. } => Some((row, column, drawing.offset)),
            Anchor::Sheet => {
                let width = |c: usize| {
                    if sheet.column_shown(c) {
                        sheet
                            .column_width(c)
                            .map_or(DEFAULT_COLUMN_WIDTH, Length::points)
                    } else {
                        0.0
                    }
                };
                let height = |r: usize| {
                    if sheet.row_shown(r) {
                        self.row_height(sheet, r)
                            .map_or(DEFAULT_ROW_HEIGHT, Length::points)
                    } else {
                        0.0
                    }
                };
                let (column, across) = within(x, LAST_COLUMN, width);
                let (row, down) = within(y, LAST_ROW, height);
                Some((row, column, (across, down)))
            }
        }
    }

    /// The top left corner of a cell from the sheet's, in points, hidden rows
    /// and columns taking no room.
    fn origin(&self, sheet: &Sheet, row: usize, column: usize) -> (f32, f32) {
        let x = (0..column)
            .filter(|&c| sheet.column_shown(c))
            .map(|c| {
                sheet
                    .column_width(c)
                    .map_or(DEFAULT_COLUMN_WIDTH, Length::points)
            })
            .sum();
        let mut y = 0.0;
        let mut reached = 0;
        for range in &sheet.rows {
            if range.first >= row {
                break;
            }
            let count = range.count.min(row - range.first);
            if range.visible {
                let height = self
                    .row_height(sheet, range.first)
                    .map_or(DEFAULT_ROW_HEIGHT, Length::points);
                #[allow(clippy::cast_precision_loss)]
                {
                    y += height * count as f32;
                }
            }
            reached = range.first + count;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            y += DEFAULT_ROW_HEIGHT * row.saturating_sub(reached) as f32;
        }
        (x, y)
    }

    /// Where each drawing anchored to a cell of a sheet stands before a
    /// change to its rows or columns: its anchor's corner and its end
    /// cell's, so that [`Self::move_drawings`] can move it with them.
    pub(super) fn anchored(&self, sheet: usize) -> Vec<Corners> {
        let Some(index) = self.sheets.get(sheet) else {
            return Vec::new();
        };
        self.drawings(sheet)
            .iter()
            .filter_map(|drawing| match drawing.anchor {
                Anchor::Sheet => None,
                Anchor::Cell { row, column, end } => Some((
                    self.origin(index, row, column),
                    end.map(|(row, column, ..)| self.origin(index, row, column)),
                )),
            })
            .collect()
    }

    /// Move each drawing anchored to a cell of a sheet by as much as its
    /// anchor moved since [`Self::anchored`], and stretch one with an end
    /// cell by as much again as the end cell moved past that.
    pub(super) fn move_drawings(
        &mut self,
        sheet: usize,
        before: &[Corners],
    ) -> Result<(), Refused> {
        let Some(index) = self.sheets.get(sheet) else {
            return Ok(());
        };
        let moves: Vec<(Vec<usize>, Point, Point)> = self
            .drawings(sheet)
            .iter()
            .filter_map(|drawing| match drawing.anchor {
                Anchor::Sheet => None,
                Anchor::Cell { row, column, end } => Some((drawing, row, column, end)),
            })
            .zip(before)
            .map(|((drawing, row, column, end), (start, end_before))| {
                let now = self.origin(index, row, column);
                let moved = (now.0 - start.0, now.1 - start.1);
                let stretched = match (end, end_before) {
                    (Some((row, column, ..)), Some(end_before)) => {
                        let end_now = self.origin(index, row, column);
                        (
                            end_now.0 - end_before.0 - moved.0,
                            end_now.1 - end_before.1 - moved.1,
                        )
                    }
                    _ => (0.0, 0.0),
                };
                (drawing.path.clone(), moved, stretched)
            })
            .filter(|(_, moved, stretched)| *moved != (0.0, 0.0) || *stretched != (0.0, 0.0))
            .collect();
        if moves.is_empty() {
            return Ok(());
        }
        let table_at = index.table;
        let names = ["x", "y", "width", "height"].map(|local| self.document.name(&Ns::Svg, local));
        let table = table_mut(&mut self.document, table_at)?;
        for (path, moved, stretched) in moves {
            let element = table.at_mut(&path).ok_or(Refused::NotFound)?;
            for (name, by) in names
                .iter()
                .zip([moved.0, moved.1, stretched.0, stretched.1])
            {
                if by == 0.0 {
                    continue;
                }
                let local = name.local.clone();
                let written = element.attr(&Ns::Svg, &local).unwrap_or("0cm");
                let unit = written
                    .get(written.len().saturating_sub(2)..)
                    .unwrap_or("cm");
                let value = Length::parse(written).map_or(0.0, Length::points) + by;
                element.set_attr(name.clone(), Length(value).write(unit));
            }
        }
        self.reindex();
        Ok(())
    }
}
