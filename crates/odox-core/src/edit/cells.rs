//! A table's shape in a text document: cells merged into one and split back,
//! and its columns' widths. DESIGN.md §11.
//!
//! A merged cell is the `table:table-cell` at the top left of what it covers,
//! with `table:number-columns-spanned` and `table:number-rows-spanned`, and a
//! `table:covered-table-cell` at every other place it covers, as `LibreOffice`
//! and xods write one. A table with repeated rows or cells is refused, as the
//! other table edits refuse it.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::HashMap;
use std::ops::Range;

use super::block::declares;
use super::format::automatic_styles;
use super::table::{column_style, column_width, emptied, rows, spread_columns};
use super::{Refused, is_paragraph};
use crate::style::Styles;
use crate::xml::{Element, Node, Ns};

/// A cell that is not covered: where it is, from the table, and the grid
/// places it covers.
struct Anchor {
    path: Vec<usize>,
    row: usize,
    column: usize,
    rows: usize,
    columns: usize,
}

/// A table's cells on its grid: each place's cell, from the table, and the
/// anchor covering it.
struct Grid {
    anchors: Vec<Anchor>,
    places: HashMap<(usize, usize), (Vec<usize>, Option<usize>)>,
}

impl Grid {
    fn read(table: &Element) -> Result<Self, Refused> {
        let more_than_one =
            |e: &Element, local: &str| e.attr_usize(&Ns::Table, local).is_some_and(|n| n > 1);
        let mut anchors = Vec::new();
        let mut places = HashMap::new();
        for (row, row_path) in rows(table).into_iter().enumerate() {
            let element = table.at(&row_path).ok_or(Refused::NotFound)?;
            if more_than_one(element, "number-rows-repeated") {
                return Err(Refused::Structure);
            }
            let mut column = 0;
            for (index, cell) in element.elements_indexed() {
                let covered = cell.is(&Ns::Table, "covered-table-cell");
                if !covered && !cell.is(&Ns::Table, "table-cell") {
                    continue;
                }
                if more_than_one(cell, "number-columns-repeated") {
                    return Err(Refused::Structure);
                }
                let path = [row_path.as_slice(), &[index]].concat();
                if !covered {
                    let span = |local: &str| cell.attr_usize(&Ns::Table, local).unwrap_or(1).max(1);
                    anchors.push(Anchor {
                        path: path.clone(),
                        row,
                        column,
                        rows: span("number-rows-spanned"),
                        columns: span("number-columns-spanned"),
                    });
                }
                places.insert((row, column), (path, None));
                column += 1;
            }
        }
        for (index, anchor) in anchors.iter().enumerate() {
            for row in anchor.row..anchor.row + anchor.rows {
                for column in anchor.column..anchor.column + anchor.columns {
                    if let Some((_, covering)) = places.get_mut(&(row, column)) {
                        *covering = Some(index);
                    }
                }
            }
        }
        Ok(Self { anchors, places })
    }
}

/// Whether a cell holds nothing but one empty paragraph.
fn is_empty(cell: &Element) -> bool {
    let mut children = cell.elements();
    matches!((children.next(), children.next()), (Some(p), None)
        if is_paragraph(p) && p.children.is_empty())
        || cell.elements().next().is_none()
}

/// Merge the cells of a rectangle of a table into its top left one, the
/// paragraphs of the others after its own, and answer the path of its first
/// paragraph.
///
/// # Errors
///
/// The rectangle cuts across a cell merged already, reaches past the table,
/// or the table has repeated rows or cells.
pub fn merge_cells(
    content: &mut Element,
    table: &[usize],
    rows: Range<usize>,
    columns: Range<usize>,
) -> Result<Vec<usize>, Refused> {
    declares(content, &[Ns::Table, Ns::Text])?;
    let grid = Grid::read(content.at(table).ok_or(Refused::NotFound)?)?;
    if rows.is_empty() || columns.is_empty() {
        return Err(Refused::Structure);
    }
    let mut inside = Vec::new();
    for row in rows.clone() {
        for column in columns.clone() {
            let (_, covering) = grid.places.get(&(row, column)).ok_or(Refused::Structure)?;
            let anchor = covering.ok_or(Refused::Structure)?;
            if !inside.contains(&anchor) {
                inside.push(anchor);
            }
        }
    }
    let within = |a: &Anchor| {
        rows.contains(&a.row)
            && a.row + a.rows <= rows.end
            && columns.contains(&a.column)
            && a.column + a.columns <= columns.end
    };
    if inside.iter().any(|&index| !within(&grid.anchors[index])) {
        return Err(Refused::Structure);
    }
    inside.sort_by_key(|&index| (grid.anchors[index].row, grid.anchors[index].column));
    let Some((&first, others)) = inside.split_first() else {
        return Err(Refused::Structure);
    };
    let path = [table, grid.anchors[first].path.as_slice()].concat();
    let first_paragraph = |content: &Element| {
        content
            .at(&path)
            .and_then(|cell| cell.elements_indexed().find(|(_, e)| is_paragraph(e)))
            .map(|(index, _)| [path.as_slice(), &[index]].concat())
            .ok_or(Refused::NotFound)
    };
    if others.is_empty() {
        return first_paragraph(content);
    }
    let mut moved = Vec::new();
    for &index in others {
        let path = [table, grid.anchors[index].path.as_slice()].concat();
        let cell = content.at_mut(&path).ok_or(Refused::NotFound)?;
        if !is_empty(cell) {
            moved.extend(
                cell.children
                    .drain(..)
                    .filter(|n| matches!(n, Node::Element(_))),
            );
        }
        cell.name = cell.name_for(&Ns::Table, "covered-table-cell");
        cell.attrs
            .retain(|a| a.name.ns == Ns::Table && &*a.name.local == "style-name");
        cell.children.clear();
        cell.self_closing = true;
    }
    let rows_spanned = content.name_for(&Ns::Table, "number-rows-spanned");
    let columns_spanned = content.name_for(&Ns::Table, "number-columns-spanned");
    let cell = content.at_mut(&path).ok_or(Refused::NotFound)?;
    if is_empty(cell) && !moved.is_empty() {
        cell.children.clear();
    }
    cell.children.extend(moved);
    cell.self_closing = false;
    for (name, span) in [(rows_spanned, rows.len()), (columns_spanned, columns.len())] {
        if span > 1 {
            cell.set_attr(name, span.to_string());
        } else {
            cell.remove_attr(&Ns::Table, &name.local.clone());
        }
    }
    first_paragraph(content)
}

/// Split a merged cell back into the cells it covers, each new one empty.
///
/// # Errors
///
/// The path does not lead to a cell, or the table has repeated rows or cells.
pub fn split_cell(content: &mut Element, cell: &[usize]) -> Result<(), Refused> {
    let (table, within) = (1..cell.len())
        .rev()
        .find(|&depth| {
            content
                .at(&cell[..depth])
                .is_some_and(|e| e.is(&Ns::Table, "table"))
        })
        .map(|depth| (&cell[..depth], &cell[depth..]))
        .ok_or(Refused::NotFound)?;
    let grid = Grid::read(content.at(table).ok_or(Refused::NotFound)?)?;
    let anchor = grid
        .anchors
        .iter()
        .find(|a| a.path == within)
        .ok_or(Refused::NotFound)?;
    let template = emptied(content.at(cell).ok_or(Refused::NotFound)?);
    for row in anchor.row..anchor.row + anchor.rows {
        for column in anchor.column..anchor.column + anchor.columns {
            if (row, column) == (anchor.row, anchor.column) {
                continue;
            }
            let Some((path, _)) = grid.places.get(&(row, column)) else {
                continue;
            };
            let mut fresh = template.clone();
            fresh.remove_attr(&Ns::Table, "number-rows-spanned");
            fresh.remove_attr(&Ns::Table, "number-columns-spanned");
            *content
                .at_mut(&[table, path.as_slice()].concat())
                .ok_or(Refused::NotFound)? = fresh;
        }
    }
    let element = content.at_mut(cell).ok_or(Refused::NotFound)?;
    element.remove_attr(&Ns::Table, "number-rows-spanned");
    element.remove_attr(&Ns::Table, "number-columns-spanned");
    Ok(())
}

/// Make a column a width in points, the column after it giving or taking the
/// difference so the table keeps its width.
///
/// # Errors
///
/// The column is the last, a width is not stated, or either column would be
/// narrower than half a centimetre.
pub fn set_column_width(
    content: &mut Element,
    table: &[usize],
    column: usize,
    width: f32,
    styles: &mut Styles,
) -> Result<(), Refused> {
    let mut widths = stated_widths(content, table, styles)?;
    if column + 1 >= widths.len() {
        return Err(Refused::Structure);
    }
    let narrowest = 0.5 / 2.54 * 72.0;
    let pair = widths[column] + widths[column + 1];
    let width = width.clamp(narrowest, pair - narrowest);
    if pair < narrowest * 2.0 {
        return Err(Refused::Structure);
    }
    widths[column] = width;
    widths[column + 1] = pair - width;
    write_widths(content, table, &widths, styles)
}

/// Every column's width in points, one to a column.
fn stated_widths(
    content: &mut Element,
    table: &[usize],
    styles: &Styles,
) -> Result<Vec<f32>, Refused> {
    declares(content, &[Ns::Table, Ns::Style])?;
    let columns = spread_columns(content, table)?;
    columns
        .iter()
        .map(|column| column_width(content, table, column, styles).ok_or(Refused::Structure))
        .collect()
}

/// Give each column a style of its width.
fn write_widths(
    content: &mut Element,
    table: &[usize],
    widths: &[f32],
    styles: &mut Styles,
) -> Result<(), Refused> {
    let columns = spread_columns(content, table)?;
    let style_name = content.name_for(&Ns::Table, "style-name");
    let mut minted = Vec::new();
    for (column, &width) in columns.iter().zip(widths) {
        let (name, fresh) = column_style(content, styles, width);
        minted.extend(fresh);
        content
            .at_mut(&[table, column.as_slice()].concat())
            .ok_or(Refused::NotFound)?
            .set_attr(style_name.clone(), name);
    }
    if !minted.is_empty() {
        let container = automatic_styles(content);
        container
            .children
            .extend(minted.into_iter().map(Node::Element));
        container.self_closing = false;
    }
    Ok(())
}

/// Make every column of a table the same width, together as wide as they were.
///
/// # Errors
///
/// A column's width is not stated.
pub fn equal_widths(
    content: &mut Element,
    table: &[usize],
    styles: &mut Styles,
) -> Result<(), Refused> {
    let widths = stated_widths(content, table, styles)?;
    #[allow(clippy::cast_precision_loss)]
    let each = widths.iter().sum::<f32>() / widths.len().max(1) as f32;
    write_widths(content, table, &vec![each; widths.len()], styles)
}
