//! The grid as a screen reader is told it.
//!
//! The grid is painted cell by cell and has no widgets in it, so what a screen
//! reader can walk is built here beside the painting: a grid of the rows in
//! view, each a row of the cells in view, each cell saying its text, where it
//! is and whether it is selected. Each is a `Ui` of its own whose node is given
//! its role, which is how egui nests one node under another. The grid never
//! takes the keyboard's focus, which the keys that move the picked cell would
//! then lose, so the picked cell is also said aloud whenever it changes, from a
//! node a screen reader reads out as it changes. Nothing is built unless a
//! screen reader is listening.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{Rect, Ui, UiBuilder, accesskit};
use odox_core::doc::{Sheet, SheetDocument};
use odox_ui::i18n::{fill, t};

use crate::grid::{address, column_name};

/// What there is to tell of the grid on screen.
pub struct Told<'a> {
    pub document: &'a SheetDocument,
    pub sheet: &'a Sheet,
    /// The rows in view and shown, the frozen ones first.
    pub rows: Vec<usize>,
    /// The columns in view and shown, the frozen ones first.
    pub columns: Vec<usize>,
    /// How many rows and columns the grid has.
    pub extent: (usize, usize),
    pub picked: (usize, usize),
    /// The selection's first and last row and column.
    pub range: ((usize, usize), (usize, usize)),
}

/// Tell the grid on screen to a screen reader, if one is listening. `place`
/// is where a cell is on the screen.
pub fn tell(ui: &mut Ui, told: &Told<'_>, place: impl Fn(usize, usize) -> Rect) {
    let mut grid = ui.new_child(UiBuilder::new().id_salt("grid-told"));
    let ctx = grid.ctx().clone();
    let listening = ctx
        .accesskit_node_builder(grid.unique_id(), |node| {
            node.set_role(accesskit::Role::Grid);
            node.set_label(told.sheet.name.clone());
            node.set_row_count(told.extent.0);
            node.set_column_count(told.extent.1);
            node.set_multiselectable();
        })
        .is_some();
    if !listening {
        return;
    }
    let ((top, left), (bottom, right)) = told.range;
    let mut picked = None;
    for &row in &told.rows {
        let mut line = grid.new_child(UiBuilder::new().id_salt(("row", row)));
        let mut bounds = Rect::NOTHING;
        for &column in &told.columns {
            let cell = told.document.cell(told.sheet, row, column);
            if cell.as_ref().is_some_and(|cell| cell.covered) {
                continue;
            }
            let text = cell.map(|cell| cell.text()).unwrap_or_default();
            let rect = place(row, column);
            bounds = bounds.union(rect);
            let one = line.new_child(UiBuilder::new().id_salt(column));
            ctx.accesskit_node_builder(one.unique_id(), |node| {
                node.set_role(accesskit::Role::Cell);
                node.set_label(text);
                node.set_row_index(row);
                node.set_column_index(column);
                node.set_row_index_text((row + 1).to_string());
                node.set_column_index_text(column_name(column));
                if (top..=bottom).contains(&row) && (left..=right).contains(&column) {
                    node.set_selected(true);
                }
                node.set_bounds(bounds_of(rect));
            });
            if (row, column) == told.picked {
                picked = Some(one.unique_id());
            }
        }
        ctx.accesskit_node_builder(line.unique_id(), |node| {
            node.set_role(accesskit::Role::Row);
            node.set_row_index(row);
            node.set_row_index_text((row + 1).to_string());
            if bounds.is_positive() {
                node.set_bounds(bounds_of(bounds));
            }
        });
    }
    if let Some(picked) = picked {
        ctx.accesskit_node_builder(grid.unique_id(), |node| {
            node.set_active_descendant(picked.accesskit_id());
        });
    }

    let (row, column) = told.picked;
    let text = told
        .document
        .cell(told.sheet, row, column)
        .map(|cell| cell.text())
        .unwrap_or_default();
    let said = if text.is_empty() {
        fill(t("{cell}, empty"), &[("cell", &address(row, column))])
    } else {
        fill(
            t("{cell}, {text}"),
            &[("cell", &address(row, column)), ("text", &text)],
        )
    };
    let aloud = grid.new_child(UiBuilder::new().id_salt("picked-told"));
    ctx.accesskit_node_builder(aloud.unique_id(), |node| {
        node.set_role(accesskit::Role::Label);
        node.set_value(said);
        node.set_live(accesskit::Live::Polite);
    });
}

fn bounds_of(rect: Rect) -> accesskit::Rect {
    accesskit::Rect {
        x0: rect.min.x.into(),
        y0: rect.min.y.into(),
        x1: rect.max.x.into(),
        y1: rect.max.y.into(),
    }
}
