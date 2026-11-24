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

/// A move smaller than this, in points, is the sum of the same sizes taken
/// in another order, not a move.
const NO_MOVE: f32 = 0.001;

/// A place on a sheet in points, across and down.
type Point = (f32, f32);

/// Where a drawing's anchor cell and end cell stand.
pub(super) type Corners = (Point, Option<Point>);

/// Where a drawing is under its table, and the cell it is in, if any.
pub(super) type Found = (Vec<usize>, Option<(usize, usize)>);

/// The line, of those `size` measures from the first, that a distance from
/// the first's start falls in, and how far into it: one taking no room is
/// passed over. Past the `written` lines every line is `usual`.
fn within(distance: f32, written: usize, size: impl Fn(usize) -> f32, usual: f32) -> (usize, f32) {
    let mut start = 0.0;
    for line in 0..written {
        let next = start + size(line);
        if distance < next {
            return (line, distance - start);
        }
        start = next;
    }
    let past = (distance - start).max(0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lines = (past / usual).floor() as usize;
    #[allow(clippy::cast_precision_loss)]
    let into = past - lines as f32 * usual;
    (written + lines, into)
}

impl SheetDrawing<'_> {
    /// Whether it is shown on the screen, or with `screen` false in print,
    /// by its `draw:display`, which `LibreOffice` also writes in its own
    /// namespace: `always`, `none`, `printer` or `screen`.
    pub fn shown(&self, screen: bool) -> bool {
        let display = self
            .element
            .attrs
            .iter()
            .find(|attribute| attribute.name.local.as_ref() == "display")
            .map(|attribute| attribute.value.as_str());
        match display {
            Some("none") => false,
            Some("printer") => !screen,
            Some("screen") => screen,
            _ => true,
        }
    }

    /// Whether it is placed by `svg:x` and `svg:y`, which a move rewrites:
    /// a line, a connector and a dimension are placed by their ends, a group
    /// by what it holds, and a turned shape by its `draw:transform`, and
    /// those are left as written.
    fn movable(&self) -> bool {
        self.element.attr(&Ns::Svg, "x").is_some()
            && self.element.attr(&Ns::Svg, "y").is_some()
            && self.element.attr(&Ns::Draw, "transform").is_none()
    }
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
        let (Some(index), Some(table)) = (
            self.sheets.get(sheet),
            self.sheets.get(sheet).and_then(|index| self.table(index)),
        ) else {
            return Vec::new();
        };
        let found = index.drawn.get_or_init(|| find_drawings(index, table));
        found
            .iter()
            .filter_map(|(path, cell)| {
                let element = table.at(path)?;
                let (x, y) = (
                    length(element, &Ns::Svg, "x"),
                    length(element, &Ns::Svg, "y"),
                );
                let Some((row, column)) = *cell else {
                    return Some(SheetDrawing {
                        element,
                        anchor: Anchor::Sheet,
                        offset: (x, y),
                        path: path.clone(),
                    });
                };
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
                let origin = self.origin(index, row, column);
                Some(SheetDrawing {
                    element,
                    anchor: Anchor::Cell { row, column, end },
                    offset: (x - origin.0, y - origin.1),
                    path: path.clone(),
                })
            })
            .collect()
    }

    /// How many rows and columns from the first the drawings on a sheet
    /// reach, to the cells their far corners fall in.
    pub fn drawings_reach(&self, sheet: &Sheet) -> (usize, usize) {
        let Some(index) = self.sheets.iter().position(|s| std::ptr::eq(s, sheet)) else {
            return (0, 0);
        };
        let written = sheet.rows.last().map_or(0, |r| r.first + r.count);
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
        self.drawings(index)
            .iter()
            .filter_map(|drawing| self.drawing_place(sheet, drawing))
            .fold((0, 0), |(rows, columns), [x, y, w, h]| {
                let (column, _) = within(x + w, sheet.columns.len(), width, DEFAULT_COLUMN_WIDTH);
                let (row, _) = within(y + h, written, height, DEFAULT_ROW_HEIGHT);
                (rows.max(row + 1), columns.max(column + 1))
            })
    }

    /// A drawing's path from the content root.
    pub fn drawing_path(&self, sheet: usize, drawing: &SheetDrawing<'_>) -> Option<Vec<usize>> {
        let content = &self.document.content;
        let (body, element) = content
            .elements_indexed()
            .find(|(_, e)| e.is(&Ns::Office, "body"))?;
        let (spreadsheet, _) = element
            .elements_indexed()
            .find(|(_, e)| e.is(&Ns::Office, "spreadsheet"))?;
        let table = self.sheets.get(sheet)?.table;
        Some([&[body, spreadsheet, table], drawing.path.as_slice()].concat())
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
                let (column, across) = within(x, sheet.columns.len(), width, DEFAULT_COLUMN_WIDTH);
                let written = sheet.rows.last().map_or(0, |r| r.first + r.count);
                let (row, down) = within(y, written, height, DEFAULT_ROW_HEIGHT);
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
            .filter(|drawing| drawing.movable())
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
        let drawings = self.drawings(sheet);
        let anchored: Vec<_> = drawings
            .iter()
            .filter(|drawing| drawing.movable())
            .filter_map(|drawing| match drawing.anchor {
                Anchor::Sheet => None,
                Anchor::Cell { row, column, end } => Some((drawing, row, column, end)),
            })
            .collect();
        if anchored.len() != before.len() {
            return Ok(());
        }
        let moves: Vec<(Vec<usize>, Point, Point)> = anchored
            .into_iter()
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
            .filter(|(_, moved, stretched)| {
                [moved.0, moved.1, stretched.0, stretched.1]
                    .iter()
                    .any(|by| by.abs() > NO_MOVE)
            })
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
                if by.abs() <= NO_MOVE {
                    continue;
                }
                let local = name.local.clone();
                let Some(written) = element.attr(&Ns::Svg, &local) else {
                    continue;
                };
                let unit = Length::unit_of(written).to_owned();
                let value = Length::parse(written).map_or(0.0, Length::points) + by;
                element.set_attr(name.clone(), Length(value).write(&unit));
            }
        }
        self.reindex();
        Ok(())
    }
}

/// Where the drawings on a sheet are under its table: those in
/// `table:shapes`, then those in cells, row by row, with each one's cell.
pub(super) fn find_drawings(index: &Sheet, table: &Element) -> Vec<Found> {
    let mut found = Vec::new();
    for (at, shapes) in table.elements_indexed() {
        if shapes.is(&Ns::Table, "shapes") {
            for (inner, element) in shapes.elements_indexed() {
                if element.name.ns == Ns::Draw {
                    found.push((vec![at, inner], None));
                }
            }
        }
    }
    for range in &index.rows {
        let Some(row) = table.at(range.path.steps()) else {
            continue;
        };
        let mut column = 0usize;
        for (at, cell) in row.elements_indexed() {
            if !cell.is(&Ns::Table, "table-cell") && !cell.is(&Ns::Table, "covered-table-cell") {
                continue;
            }
            for (inner, element) in cell.elements_indexed() {
                if element.name.ns == Ns::Draw {
                    found.push((
                        [range.path.steps(), &[at, inner]].concat(),
                        Some((range.first, column)),
                    ));
                }
            }
            column += cell
                .attr_usize(&Ns::Table, "number-columns-repeated")
                .unwrap_or(1)
                .max(1);
        }
    }
    found
}
