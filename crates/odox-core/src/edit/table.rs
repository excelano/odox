//! Tables in a text document: putting one in, adding and taking away its rows
//! and columns, taking it out, and stepping from cell to cell. DESIGN.md §11.
//!
//! A table is written the way `LibreOffice` writes one: a `table:table` with an
//! automatic table style as wide as the text, a `table:table-column` per column
//! with a column style giving its width, and cells with a thin border and the
//! document's table-contents paragraph style where it has one. The window draws
//! columns at the widths their styles give, so a column put in takes half the
//! width of the one it goes beside, and a column taken out gives its width to
//! its neighbour: the table stays as wide as it was.
//!
//! Rows and columns are changed only in a table whose cells are one to a grid
//! position: no merged cells and no repeated rows or cells. Anything else is
//! refused as [`Refused::Structure`] and left as it is.
//!
//! Paths are from the content root. A table style, a column style or a cell
//! style may be written into `office:automatic-styles`, which a document that
//! has none is given before its body, moving the body one place along; a path
//! answered here is from before that move, as the other edits' are.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::block::{declares, name_for};
use super::format::{automatic_styles, element};
use super::{Refused, is_paragraph};
use crate::style::{Family, Styles};
use crate::xml::{Element, Node, Ns};

/// A change to the table a paragraph is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableEdit {
    /// A row above the paragraph's.
    RowAbove,
    /// A row below the paragraph's.
    RowBelow,
    /// A column to the left of the paragraph's.
    ColumnLeft,
    /// A column to the right of the paragraph's.
    ColumnRight,
    /// Take out the paragraph's row, and the table with its last row.
    DeleteRow,
    /// Take out the paragraph's column, and the table with its last column.
    DeleteColumn,
    /// Take out the table.
    DeleteTable,
}

/// Where a paragraph stands in the nearest table around it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InTable {
    /// The `table:table`, from the content root.
    pub table: Vec<usize>,
    /// Its row, counting from zero among the table's rows.
    pub row: usize,
    /// Its column, counting from zero.
    pub column: usize,
}

/// The table a paragraph is in, and where in it, or `None` outside any.
pub fn table_at(content: &Element, paragraph: &[usize]) -> Option<InTable> {
    let (table, row_path, cell_index) = enclosing(content, paragraph)?;
    let element = content.at(&table)?;
    let rows = rows(element);
    let row = rows.iter().position(|r| *r == row_path)?;
    let column = column_of(element.at(&row_path)?, cell_index);
    Some(InTable { table, row, column })
}

/// Put a table of `rows` by `columns` empty cells beside a paragraph, as wide
/// as `width` points, and answer the path of its first cell's paragraph.
///
/// The table goes after the paragraph, or after the list it is an item of; an
/// empty paragraph has it go before it instead, so that the paragraph is left
/// to type in below the table. A table with nothing after it in its parent is
/// given an empty paragraph to follow it, so the text can go on.
///
/// # Errors
///
/// The path does not lead to a paragraph, either count is zero, or the content
/// root does not declare a namespace the table is written in.
pub fn insert_table(
    content: &mut Element,
    paragraph: &[usize],
    rows: usize,
    columns: usize,
    width: f32,
    styles: &mut Styles,
) -> Result<Vec<usize>, Refused> {
    let found = content
        .at(paragraph)
        .filter(|e| is_paragraph(e))
        .ok_or(Refused::NotFound)?;
    if rows == 0 || columns == 0 {
        return Err(Refused::NotFound);
    }
    declares(content, &[Ns::Table, Ns::Style, Ns::Fo, Ns::Text])?;
    let empty = super::text(found).is_empty();
    let place = outside_lists(content, paragraph);
    let (parent_path, index) = place.split_at(place.len() - 1);
    let at = if empty && place == paragraph {
        index[0]
    } else {
        index[0] + 1
    };

    let set = |element: &mut Element, ns: Ns, local: &str, value: &str| {
        element.set_attr(content.name_for(&ns, local), value);
    };
    let mut table_style = element(content.name_for(&Ns::Style, "style"));
    set(&mut table_style, Ns::Style, "family", "table");
    let mut properties = element(content.name_for(&Ns::Style, "table-properties"));
    set(&mut properties, Ns::Style, "width", &length(width));
    set(&mut properties, Ns::Table, "align", "margins");
    table_style.children.push(Node::Element(properties));
    table_style.self_closing = false;
    let (table_style_name, table_minted) =
        name_for(content, styles, table_style, &Family::Table, "Table");

    #[allow(clippy::cast_precision_loss)]
    let (column_style_name, column_minted) = column_style(content, styles, width / columns as f32);

    let mut cell_style = element(content.name_for(&Ns::Style, "style"));
    set(&mut cell_style, Ns::Style, "family", "table-cell");
    let mut cell_properties = element(content.name_for(&Ns::Style, "table-cell-properties"));
    set(&mut cell_properties, Ns::Fo, "padding", "0.097cm");
    set(
        &mut cell_properties,
        Ns::Fo,
        "border",
        "0.5pt solid #000000",
    );
    cell_style.children.push(Node::Element(cell_properties));
    cell_style.self_closing = false;
    let (cell_style_name, cell_minted) =
        name_for(content, styles, cell_style, &Family::TableCell, "TableCell");

    let contents = styles
        .has_style(&Family::Paragraph, "Table_20_Contents")
        .then_some("Table_20_Contents");
    let mut number = 1;
    while has_table_named(content, &format!("Table{number}")) {
        number += 1;
    }

    let mut table = element(content.name_for(&Ns::Table, "table"));
    set(&mut table, Ns::Table, "name", &format!("Table{number}"));
    set(&mut table, Ns::Table, "style-name", &table_style_name);
    for _ in 0..columns {
        let mut column = element(content.name_for(&Ns::Table, "table-column"));
        set(&mut column, Ns::Table, "style-name", &column_style_name);
        table.children.push(Node::Element(column));
    }
    for _ in 0..rows {
        let mut row = element(content.name_for(&Ns::Table, "table-row"));
        for _ in 0..columns {
            let mut cell = element(content.name_for(&Ns::Table, "table-cell"));
            set(&mut cell, Ns::Table, "style-name", &cell_style_name);
            let mut paragraph = element(content.name_for(&Ns::Text, "p"));
            if let Some(style) = contents {
                set(&mut paragraph, Ns::Text, "style-name", style);
            }
            cell.children.push(Node::Element(paragraph));
            cell.self_closing = false;
            row.children.push(Node::Element(cell));
        }
        row.self_closing = false;
        table.children.push(Node::Element(row));
    }
    table.self_closing = false;
    let follower = element(content.name_for(&Ns::Text, "p"));

    let parent = content.at_mut(parent_path).ok_or(Refused::NotFound)?;
    parent.children.insert(at, Node::Element(table));
    parent.self_closing = false;
    let last = !parent.children[at + 1..]
        .iter()
        .any(|node| matches!(node, Node::Element(_)));
    if last {
        parent.children.push(Node::Element(follower));
    }
    for minted in [table_minted, column_minted, cell_minted]
        .into_iter()
        .flatten()
    {
        let container = automatic_styles(content);
        container.children.push(Node::Element(minted));
        container.self_closing = false;
    }
    Ok([parent_path, &[at, columns, 0, 0]].concat())
}

/// Change the table a paragraph is in, and answer the path of the paragraph
/// the caret goes to: the same column in the new row, the same row in the new
/// column, a neighbour of what was taken out, or what follows a table taken
/// out.
///
/// # Errors
///
/// The paragraph is in no table, or the edit is to rows or columns of a table
/// with merged or repeated cells.
pub fn edit_table(
    content: &mut Element,
    paragraph: &[usize],
    edit: TableEdit,
    styles: &mut Styles,
) -> Result<Vec<usize>, Refused> {
    let place = table_at(content, paragraph).ok_or(Refused::NotFound)?;
    let table = content.at(&place.table).ok_or(Refused::NotFound)?;
    let row_count = rows(table).len();
    let column_count = columns(table);
    match edit {
        TableEdit::DeleteTable => delete_table(content, &place.table),
        TableEdit::DeleteRow if row_count == 1 => delete_table(content, &place.table),
        TableEdit::DeleteColumn if column_count == 1 => delete_table(content, &place.table),
        _ if !plain(table) => Err(Refused::Structure),
        TableEdit::RowAbove | TableEdit::RowBelow => {
            let at = place.row + usize::from(edit == TableEdit::RowBelow);
            insert_row(content, &place.table, place.row, at)?;
            cell_paragraph(content, &place.table, at, place.column)
        }
        TableEdit::DeleteRow => {
            let row_path = rows(table)[place.row].clone();
            remove_at(content, &[place.table.as_slice(), &row_path].concat())?;
            cell_paragraph(
                content,
                &place.table,
                place.row.min(row_count - 2),
                place.column,
            )
        }
        TableEdit::ColumnLeft | TableEdit::ColumnRight => {
            let at = place.column + usize::from(edit == TableEdit::ColumnRight);
            insert_column(content, &place.table, place.column, at, styles)?;
            cell_paragraph(content, &place.table, place.row, at)
        }
        TableEdit::DeleteColumn => {
            delete_column(content, &place.table, place.column, styles)?;
            cell_paragraph(
                content,
                &place.table,
                place.row,
                place.column.min(column_count - 2),
            )
        }
    }
}

/// The first paragraph of the cell after the one a paragraph is in, or before
/// it, row by row; `None` past either end of the table.
pub fn step_cell(content: &Element, paragraph: &[usize], forward: bool) -> Option<Vec<usize>> {
    let (table, row_path, cell_index) = enclosing(content, paragraph)?;
    let element = content.at(&table)?;
    let cells: Vec<Vec<usize>> = rows(element)
        .into_iter()
        .flat_map(|row| {
            element
                .at(&row)
                .into_iter()
                .flat_map(Element::elements_indexed)
                .filter(|(_, cell)| cell.is(&Ns::Table, "table-cell"))
                .map(move |(index, _)| [row.as_slice(), &[index]].concat())
                .collect::<Vec<_>>()
        })
        .collect();
    let here = [row_path.as_slice(), &[cell_index]].concat();
    let index = cells.iter().position(|cell| *cell == here)?;
    let next = if forward {
        cells.get(index + 1)?
    } else {
        cells.get(index.checked_sub(1)?)?
    };
    let cell = [table.as_slice(), next].concat();
    let within = first_paragraph(content.at(&cell)?)?;
    Some([cell, within].concat())
}

/// The nearest `table:table` around a paragraph, the path of its row under the
/// table, and the index of its cell among the row's children.
fn enclosing(content: &Element, paragraph: &[usize]) -> Option<(Vec<usize>, Vec<usize>, usize)> {
    for depth in (1..paragraph.len()).rev() {
        let cell = &paragraph[..depth];
        if !content.at(cell)?.is(&Ns::Table, "table-cell") {
            continue;
        }
        for top in (0..depth - 1).rev() {
            if content.at(&paragraph[..top])?.is(&Ns::Table, "table") {
                let row = paragraph[top..depth - 1].to_vec();
                return Some((paragraph[..top].to_vec(), row, paragraph[depth - 1]));
            }
        }
    }
    None
}

/// The paths of a table's rows under it, through header rows and row groups.
fn rows(table: &Element) -> Vec<Vec<usize>> {
    fn walk(parent: &Element, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        for (index, child) in parent.elements_indexed() {
            path.push(index);
            if child.is(&Ns::Table, "table-row") {
                out.push(path.clone());
            } else if child.is(&Ns::Table, "table-header-rows")
                || child.is(&Ns::Table, "table-rows")
                || child.is(&Ns::Table, "table-row-group")
            {
                walk(child, path, out);
            }
            path.pop();
        }
    }
    let mut out = Vec::new();
    walk(table, &mut Vec::new(), &mut out);
    out
}

/// The paths of a table's `table:table-column` elements under it.
fn column_elements(table: &Element) -> Vec<Vec<usize>> {
    fn walk(parent: &Element, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        for (index, child) in parent.elements_indexed() {
            path.push(index);
            if child.is(&Ns::Table, "table-column") {
                out.push(path.clone());
            } else if child.is(&Ns::Table, "table-columns")
                || child.is(&Ns::Table, "table-header-columns")
                || child.is(&Ns::Table, "table-column-group")
            {
                walk(child, path, out);
            }
            path.pop();
        }
    }
    let mut out = Vec::new();
    walk(table, &mut Vec::new(), &mut out);
    out
}

/// How many columns a table has, as its widest row counts them.
fn columns(table: &Element) -> usize {
    rows(table)
        .iter()
        .filter_map(|row| table.at(row))
        .map(|row| column_of(row, usize::MAX))
        .max()
        .unwrap_or(0)
}

/// The column a cell at a child index of a row begins in.
fn column_of(row: &Element, cell_index: usize) -> usize {
    row.elements_indexed()
        .take_while(|(index, _)| *index < cell_index)
        .filter(|(_, cell)| {
            cell.is(&Ns::Table, "table-cell") || cell.is(&Ns::Table, "covered-table-cell")
        })
        .map(|(_, cell)| {
            cell.attr_usize(&Ns::Table, "number-columns-repeated")
                .unwrap_or(1)
                .max(1)
        })
        .sum()
}

/// Whether every cell is one grid position: no spans, no covered cells, and
/// nothing repeated.
fn plain(table: &Element) -> bool {
    let more_than_one =
        |e: &Element, ns: Ns, local: &str| e.attr_usize(&ns, local).is_some_and(|n| n > 1);
    rows(table)
        .iter()
        .filter_map(|row| table.at(row))
        .all(|row| {
            !more_than_one(row, Ns::Table, "number-rows-repeated")
                && row.elements().all(|cell| {
                    !cell.is(&Ns::Table, "covered-table-cell")
                        && !more_than_one(cell, Ns::Table, "number-columns-repeated")
                        && !more_than_one(cell, Ns::Table, "number-columns-spanned")
                        && !more_than_one(cell, Ns::Table, "number-rows-spanned")
                })
        })
}

/// The path under an element of the first paragraph in it.
fn first_paragraph(parent: &Element) -> Option<Vec<usize>> {
    for (index, child) in parent.elements_indexed() {
        if is_paragraph(child) {
            return Some(vec![index]);
        }
        if let Some(rest) = first_paragraph(child) {
            return Some([vec![index], rest].concat());
        }
    }
    None
}

/// The first paragraph of the cell at a row and a column of a plain table.
fn cell_paragraph(
    content: &Element,
    table: &[usize],
    row: usize,
    column: usize,
) -> Result<Vec<usize>, Refused> {
    let element = content.at(table).ok_or(Refused::NotFound)?;
    let row_path = rows(element).get(row).cloned().ok_or(Refused::NotFound)?;
    let row_element = element.at(&row_path).ok_or(Refused::NotFound)?;
    let (cell_index, cell) = row_element
        .elements_indexed()
        .filter(|(_, cell)| cell.is(&Ns::Table, "table-cell"))
        .nth(column)
        .ok_or(Refused::NotFound)?;
    let within = first_paragraph(cell).ok_or(Refused::NotFound)?;
    Ok([table, &row_path, &[cell_index], &within].concat())
}

/// A cell like another but empty: one paragraph in the style of the other's
/// first, and none of its value.
fn emptied(cell: &Element) -> Element {
    let mut fresh = cell.clone();
    for local in [
        "value-type",
        "value",
        "date-value",
        "time-value",
        "boolean-value",
        "string-value",
        "currency",
    ] {
        fresh.remove_attr(&Ns::Office, local);
    }
    fresh.remove_attr(&Ns::Table, "formula");
    let mut paragraph = cell
        .elements()
        .find(|e| is_paragraph(e))
        .cloned()
        .unwrap_or_else(|| element(cell.name_for(&Ns::Text, "p")));
    paragraph.children.clear();
    paragraph.self_closing = true;
    fresh.children = vec![Node::Element(paragraph)];
    fresh.self_closing = false;
    fresh
}

/// Put an empty copy of one row at a place among the rows.
fn insert_row(
    content: &mut Element,
    table: &[usize],
    like: usize,
    at: usize,
) -> Result<(), Refused> {
    let element = content.at(table).ok_or(Refused::NotFound)?;
    let all = rows(element);
    let model = element.at(&all[like]).ok_or(Refused::NotFound)?;
    let mut row = model.clone();
    for node in &mut row.children {
        if let Node::Element(cell) = node
            && cell.is(&Ns::Table, "table-cell")
        {
            *cell = emptied(cell);
        }
    }
    let beside = if at < all.len() { &all[at] } else { &all[like] };
    let (parent, index) = beside.split_at(beside.len() - 1);
    let index = if at < all.len() {
        index[0]
    } else {
        index[0] + 1
    };
    let parent = content
        .at_mut(&[table, parent].concat())
        .ok_or(Refused::NotFound)?;
    parent.children.insert(index, Node::Element(row));
    Ok(())
}

/// Put an empty column at a place, a copy of the column at `like`, the two
/// sharing what was its width.
fn insert_column(
    content: &mut Element,
    table: &[usize],
    like: usize,
    at: usize,
    styles: &mut Styles,
) -> Result<(), Refused> {
    let columns_at = spread_columns(content, table)?;
    let model_path = columns_at.get(like).cloned().ok_or(Refused::NotFound)?;
    let half = column_width(content, table, &model_path, styles).map(|w| w / 2.0);
    let mut minted = None;
    let mut model = content
        .at(&[table, &model_path].concat())
        .cloned()
        .ok_or(Refused::NotFound)?;
    if let Some(half) = half {
        let (name, fresh) = column_style(content, styles, half);
        minted = fresh;
        let style_name = content.name_for(&Ns::Table, "style-name");
        model.set_attr(style_name.clone(), name.clone());
        content
            .at_mut(&[table, &model_path].concat())
            .ok_or(Refused::NotFound)?
            .set_attr(style_name, name);
    }
    let (parent, index) = model_path.split_at(model_path.len() - 1);
    let index = index[0] + usize::from(at > like);
    let parent_element = content
        .at_mut(&[table, parent].concat())
        .ok_or(Refused::NotFound)?;
    parent_element.children.insert(index, Node::Element(model));

    let element = content.at(table).ok_or(Refused::NotFound)?;
    for row_path in rows(element) {
        let row = content
            .at_mut(&[table, &row_path].concat())
            .ok_or(Refused::NotFound)?;
        let cells: Vec<usize> = row
            .elements_indexed()
            .filter(|(_, cell)| cell.is(&Ns::Table, "table-cell"))
            .map(|(index, _)| index)
            .collect();
        let Some(&source) = cells.get(like) else {
            continue;
        };
        let fresh = match row.at(&[source]) {
            Some(cell) => emptied(cell),
            None => continue,
        };
        let target = if at > like { source + 1 } else { source };
        row.children.insert(target, Node::Element(fresh));
    }
    if let Some(minted) = minted {
        let container = automatic_styles(content);
        container.children.push(Node::Element(minted));
        container.self_closing = false;
    }
    Ok(())
}

/// Take a column out, giving its width to the column beside it.
fn delete_column(
    content: &mut Element,
    table: &[usize],
    column: usize,
    styles: &mut Styles,
) -> Result<(), Refused> {
    let columns_at = spread_columns(content, table)?;
    let gone = columns_at.get(column).cloned().ok_or(Refused::NotFound)?;
    let neighbour_index = if column + 1 < columns_at.len() {
        column + 1
    } else {
        column - 1
    };
    let neighbour = columns_at[neighbour_index].clone();
    let widths = (
        column_width(content, table, &gone, styles),
        column_width(content, table, &neighbour, styles),
    );
    let mut minted = None;
    if let (Some(a), Some(b)) = widths {
        let (name, fresh) = column_style(content, styles, a + b);
        minted = fresh;
        let style_name = content.name_for(&Ns::Table, "style-name");
        content
            .at_mut(&[table, &neighbour].concat())
            .ok_or(Refused::NotFound)?
            .set_attr(style_name, name);
    }
    let element = content.at(table).ok_or(Refused::NotFound)?;
    for row_path in rows(element).into_iter().rev() {
        let row = content
            .at_mut(&[table, &row_path].concat())
            .ok_or(Refused::NotFound)?;
        let found = row
            .elements_indexed()
            .filter(|(_, cell)| cell.is(&Ns::Table, "table-cell"))
            .nth(column)
            .map(|(index, _)| index);
        if let Some(index) = found {
            row.children.remove(index);
        }
    }
    remove_at(content, &[table, &gone].concat())?;
    if let Some(minted) = minted {
        let container = automatic_styles(content);
        container.children.push(Node::Element(minted));
        container.self_closing = false;
    }
    Ok(())
}

/// Write a table's columns one element to a column, so that one can be told
/// from its neighbours, and answer their paths under the table.
fn spread_columns(content: &mut Element, table: &[usize]) -> Result<Vec<Vec<usize>>, Refused> {
    let element = content.at(table).ok_or(Refused::NotFound)?;
    for path in column_elements(element).into_iter().rev() {
        let column = content
            .at_mut(&[table, &path].concat())
            .ok_or(Refused::NotFound)?;
        let repeat = column
            .attr_usize(&Ns::Table, "number-columns-repeated")
            .unwrap_or(1)
            .clamp(1, 1024);
        if repeat == 1 {
            continue;
        }
        column.remove_attr(&Ns::Table, "number-columns-repeated");
        let copy = column.clone();
        let (parent, index) = path.split_at(path.len() - 1);
        let parent = content
            .at_mut(&[table, parent].concat())
            .ok_or(Refused::NotFound)?;
        for _ in 1..repeat {
            parent
                .children
                .insert(index[0] + 1, Node::Element(copy.clone()));
        }
    }
    let element = content.at(table).ok_or(Refused::NotFound)?;
    Ok(column_elements(element))
}

/// A column's width in points, where its style gives one.
fn column_width(
    content: &Element,
    table: &[usize],
    column: &[usize],
    styles: &Styles,
) -> Option<f32> {
    let name = content
        .at(&[table, column].concat())?
        .attr(&Ns::Table, "style-name")?;
    styles
        .resolve(&Family::TableColumn, name)
        .column_width
        .map(crate::Length::points)
}

/// A column style of a width in points: one the document has, or one to write.
fn column_style(content: &Element, styles: &mut Styles, width: f32) -> (String, Option<Element>) {
    let mut style = element(content.name_for(&Ns::Style, "style"));
    style.set_attr(content.name_for(&Ns::Style, "family"), "table-column");
    let mut properties = element(content.name_for(&Ns::Style, "table-column-properties"));
    properties.set_attr(content.name_for(&Ns::Style, "column-width"), length(width));
    style.children.push(Node::Element(properties));
    style.self_closing = false;
    name_for(content, styles, style, &Family::TableColumn, "TableColumn")
}

/// Take out a table, and answer the paragraph the caret goes to: the first in
/// what followed the table, or the last before it, or a new empty one where
/// the table stood alone.
fn delete_table(content: &mut Element, table: &[usize]) -> Result<Vec<usize>, Refused> {
    let (parent_path, index) = table.split_at(table.len() - 1);
    let index = index[0];
    let parent = content.at_mut(parent_path).ok_or(Refused::NotFound)?;
    parent.children.remove(index);
    let after = parent
        .elements_indexed()
        .filter(|(i, _)| *i >= index)
        .find_map(|(i, e)| {
            if is_paragraph(e) {
                Some(vec![i])
            } else {
                first_paragraph(e).map(|rest| [vec![i], rest].concat())
            }
        });
    if let Some(found) = after {
        return Ok([parent_path, &found].concat());
    }
    let before = parent
        .elements_indexed()
        .filter(|(i, _)| *i < index)
        .filter_map(|(i, e)| {
            if is_paragraph(e) {
                Some(vec![i])
            } else {
                first_paragraph(e).map(|rest| [vec![i], rest].concat())
            }
        })
        .last();
    if let Some(found) = before {
        return Ok([parent_path, &found].concat());
    }
    let paragraph = element(parent.name_for(&Ns::Text, "p"));
    parent.children.insert(index, Node::Element(paragraph));
    Ok([parent_path, &[index]].concat())
}

/// Take out the element at a path.
fn remove_at(content: &mut Element, path: &[usize]) -> Result<(), Refused> {
    let (parent, index) = path.split_at(path.len() - 1);
    let parent = content.at_mut(parent).ok_or(Refused::NotFound)?;
    if index[0] >= parent.children.len() {
        return Err(Refused::NotFound);
    }
    parent.children.remove(index[0]);
    Ok(())
}

/// The path of what a table beside a paragraph goes beside: the paragraph, or
/// the outermost list it is an item of.
pub(super) fn outside_lists(content: &Element, paragraph: &[usize]) -> Vec<usize> {
    let mut place = paragraph.to_vec();
    for depth in (1..paragraph.len()).rev() {
        match content.at(&paragraph[..depth]) {
            Some(e) if e.is(&Ns::Text, "list") => place = paragraph[..depth].to_vec(),
            Some(e) if e.is(&Ns::Text, "list-item") || e.is(&Ns::Text, "list-header") => {}
            _ => break,
        }
    }
    place
}

fn has_table_named(content: &Element, name: &str) -> bool {
    (content.is(&Ns::Table, "table") && content.attr(&Ns::Table, "name") == Some(name))
        || content.elements().any(|child| has_table_named(child, name))
}

/// A length in points as ODF writes it, in centimetres.
fn length(points: f32) -> String {
    format!("{:.3}cm", points / 72.0 * 2.54)
}
