//! Inserting and deleting rows and columns of a sheet.
//!
//! A row or a column is not always one element: `table:number-rows-repeated`
//! and `table:number-columns-repeated` stand for many, so an insert may split a
//! run and a delete may shorten one. An inserted row or column is a copy of its
//! neighbour with what it held cleared, as spreadsheets do, so it carries the
//! neighbour's formats and nothing a formula could reach.
//!
//! What names a cell has to move with the cells. The references in formulas are
//! shifted by [`super::super::refs`], and a change is refused, before anything
//! is touched, when a formula holds a reference that cannot be read or points
//! at a cell that is deleted, when the line cuts through a merged region, or
//! when the document holds something else that names a range of cells (a
//! conditional format, a validation, a chart, a print range), which this does
//! not move.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::{MAX_COLUMNS, SheetDocument, with_count};
use crate::doc::refs::{self, Axis, Change, Kind, Refusal};
use crate::xml::{Element, Name, Node, Ns};

/// What is asked of a sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restructure {
    /// Put `count` empty rows in before the row at `at`.
    InsertRows {
        /// The row the new ones go before, counting from zero.
        at: usize,
        /// How many.
        count: usize,
    },
    /// Take `count` rows out, from the row at `at`.
    DeleteRows {
        /// The first row, counting from zero.
        at: usize,
        /// How many.
        count: usize,
    },
    /// Put `count` empty columns in before the column at `at`.
    InsertColumns {
        /// The column the new ones go before, counting from zero.
        at: usize,
        /// How many.
        count: usize,
    },
    /// Take `count` columns out, from the column at `at`.
    DeleteColumns {
        /// The first column, counting from zero.
        at: usize,
        /// How many.
        count: usize,
    },
}

impl Restructure {
    fn axis_at_kind(self) -> (Axis, usize, Kind) {
        match self {
            Self::InsertRows { at, count } => (Axis::Row, at, Kind::Insert(count)),
            Self::DeleteRows { at, count } => (Axis::Row, at, Kind::Delete(count)),
            Self::InsertColumns { at, count } => (Axis::Column, at, Kind::Insert(count)),
            Self::DeleteColumns { at, count } => (Axis::Column, at, Kind::Delete(count)),
        }
    }
}

/// Why a sheet was not changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocked {
    /// There is no such sheet, or the count is nothing.
    NotFound,
    /// The line cuts through a merged region.
    Merged,
    /// A formula, or a named range, holds a reference that cannot be read.
    Unreadable(String),
    /// A formula, or a named range, points at what would be deleted.
    Deleted(String),
    /// The document holds something that names a range of cells and is not
    /// moved: what, in the words a window can put in a sentence.
    Names(&'static str),
}

impl SheetDocument {
    /// Insert or delete rows or columns of a sheet, with the formulas that
    /// refer to the cells moved to follow.
    ///
    /// Nothing is changed unless all of it can be: the change is made to a
    /// copy of the content, which replaces the content only when it is done.
    ///
    /// # Errors
    ///
    /// [`Blocked`] says what is in the way.
    pub fn restructure(&mut self, sheet: usize, change: Restructure) -> Result<(), Blocked> {
        let (axis, at, kind) = change.axis_at_kind();
        let (Kind::Insert(count) | Kind::Delete(count)) = kind;
        let table_at = self.sheets.get(sheet).ok_or(Blocked::NotFound)?.table;
        let name = self.sheets[sheet].name.clone();
        if count == 0 {
            return Err(Blocked::NotFound);
        }
        let names = RepeatNames {
            rows: self.document.name(&Ns::Table, "number-rows-repeated"),
            columns: self.document.name(&Ns::Table, "number-columns-repeated"),
        };

        let mut content = self.document.content.clone();
        let spreadsheet = content
            .child_mut(&Ns::Office, "body")
            .and_then(|body| body.child_mut(&Ns::Office, "spreadsheet"))
            .ok_or(Blocked::NotFound)?;
        if let Some(what) = unmovable(spreadsheet) {
            return Err(Blocked::Names(what));
        }
        let Some(Node::Element(table)) = spreadsheet.children.get(table_at) else {
            return Err(Blocked::NotFound);
        };
        if cuts_a_merge(table, axis, at, kind) {
            return Err(Blocked::Merged);
        }
        let moved = Change {
            sheet: name,
            axis,
            at,
            kind,
        };
        // The rows or columns first, so that a formula in a cell that is
        // deleted with them is not asked about what it pointed at.
        let Some(Node::Element(table)) = spreadsheet.children.get_mut(table_at) else {
            return Err(Blocked::NotFound);
        };
        match change {
            Restructure::InsertRows { at, count } => insert_rows(table, at, count, &names),
            Restructure::DeleteRows { at, count } => {
                delete_run(table, Run::Rows, at, count, &names);
            }
            Restructure::InsertColumns { at, count } => insert_columns(table, at, count, &names),
            Restructure::DeleteColumns { at, count } => {
                delete_run(table, Run::Columns, at, count, &names);
            }
        }
        shift_references(spreadsheet, None, &moved)?;
        self.document.content = content;
        self.reindex();
        Ok(())
    }
}

struct RepeatNames {
    rows: Name,
    columns: Name,
}

/// Something in the spreadsheet that names a range of cells and is not moved.
fn unmovable(element: &Element) -> Option<&'static str> {
    let has_content = element.elements().next().is_some();
    let named = |local: &str| element.name.local.as_ref() == local;
    if has_content {
        if named("conditional-formats") {
            return Some("conditional formats");
        }
        if named("content-validations") {
            return Some("validations");
        }
        if named("database-ranges") {
            return Some("database ranges");
        }
        if named("data-pilot-tables") {
            return Some("pivot tables");
        }
        if named("consolidation") {
            return Some("consolidations");
        }
    }
    if element.is(&Ns::Draw, "object") || element.is(&Ns::Draw, "object-ole") {
        return Some("charts and objects");
    }
    if element.attr(&Ns::Table, "print-ranges").is_some() {
        return Some("print ranges");
    }
    if element.attr(&Ns::Table, "end-cell-address").is_some() {
        return Some("shapes anchored to cells");
    }
    element.elements().find_map(unmovable)
}

/// The cells of a row, with the column each begins at and how many it stands
/// for.
fn cells_of(row: &Element) -> impl Iterator<Item = (usize, usize, &Element)> {
    let mut at = 0;
    row.elements()
        .filter(|e| e.is(&Ns::Table, "table-cell") || e.is(&Ns::Table, "covered-table-cell"))
        .map(move |cell| {
            let repeat = cell
                .attr_usize(&Ns::Table, "number-columns-repeated")
                .unwrap_or(1)
                .max(1);
            let first = at;
            at += repeat;
            (first, repeat, cell)
        })
}

fn is_row_container(element: &Element) -> bool {
    element.is(&Ns::Table, "table-rows")
        || element.is(&Ns::Table, "table-header-rows")
        || element.is(&Ns::Table, "table-row-group")
}

fn is_column_container(element: &Element) -> bool {
    element.is(&Ns::Table, "table-columns")
        || element.is(&Ns::Table, "table-header-columns")
        || element.is(&Ns::Table, "table-column-group")
}

/// Each row element of a table in order, with the row it begins at, how many
/// it stands for, and the route to it.
fn rows_of(table: &Element) -> Vec<(Vec<usize>, usize, usize)> {
    fn walk(
        parent: &Element,
        path: &mut Vec<usize>,
        at: &mut usize,
        out: &mut Vec<(Vec<usize>, usize, usize)>,
    ) {
        for (index, child) in parent.elements_indexed() {
            path.push(index);
            if child.is(&Ns::Table, "table-row") {
                let repeat = child
                    .attr_usize(&Ns::Table, "number-rows-repeated")
                    .unwrap_or(1)
                    .max(1);
                out.push((path.clone(), *at, repeat));
                *at += repeat;
            } else if is_row_container(child) {
                walk(child, path, at, out);
            }
            path.pop();
        }
    }
    let mut out = Vec::new();
    walk(table, &mut Vec::new(), &mut 0, &mut out);
    out
}

/// The same for the column definitions.
fn columns_of(table: &Element) -> Vec<(Vec<usize>, usize, usize)> {
    fn walk(
        parent: &Element,
        path: &mut Vec<usize>,
        at: &mut usize,
        out: &mut Vec<(Vec<usize>, usize, usize)>,
    ) {
        for (index, child) in parent.elements_indexed() {
            path.push(index);
            if child.is(&Ns::Table, "table-column") {
                let repeat = child
                    .attr_usize(&Ns::Table, "number-columns-repeated")
                    .unwrap_or(1)
                    .max(1);
                out.push((path.clone(), *at, repeat));
                *at += repeat;
            } else if is_column_container(child) {
                walk(child, path, at, out);
            }
            path.pop();
        }
    }
    let mut out = Vec::new();
    walk(table, &mut Vec::new(), &mut 0, &mut out);
    out
}

/// Whether a line of an insert or a delete goes through a merged region.
fn cuts_a_merge(table: &Element, axis: Axis, at: usize, kind: Kind) -> bool {
    let cuts = |low: usize, high: usize| match kind {
        Kind::Insert(_) => low < at && at <= high,
        Kind::Delete(count) => {
            let last = at + count - 1;
            let overlaps = low <= last && high >= at;
            let inside = low >= at && high <= last;
            overlaps && !inside
        }
    };
    for (path, first_row, _) in rows_of(table) {
        let Some(row) = table.at(&path) else {
            continue;
        };
        for (first_column, _, cell) in cells_of(row) {
            let across = cell
                .attr_usize(&Ns::Table, "number-columns-spanned")
                .unwrap_or(1)
                .max(1);
            let down = cell
                .attr_usize(&Ns::Table, "number-rows-spanned")
                .unwrap_or(1)
                .max(1);
            if across == 1 && down == 1 {
                continue;
            }
            let (low, high) = match axis {
                Axis::Row => (first_row, first_row + down - 1),
                Axis::Column => (first_column, first_column + across - 1),
            };
            if cuts(low, high) {
                return true;
            }
        }
    }
    false
}

/// Move the references in everything under an element: the formulas in the
/// cells, and the named ranges and expressions, each against the sheet it sits
/// on or names.
fn shift_references(
    element: &mut Element,
    own_sheet: Option<&str>,
    change: &Change,
) -> Result<(), Blocked> {
    let blocked = |refusal: Refusal| match refusal {
        Refusal::Unreadable(text) => Blocked::Unreadable(text),
        Refusal::Deleted(text) => Blocked::Deleted(text),
    };
    let own = if element.is(&Ns::Table, "table") {
        element.attr(&Ns::Table, "name").map(ToOwned::to_owned)
    } else {
        own_sheet.map(ToOwned::to_owned)
    };
    for attribute in &mut element.attrs {
        if attribute.name.is(&Ns::Table, "formula") {
            attribute.value =
                refs::shift_formula(&attribute.value, own.as_deref(), change).map_err(blocked)?;
        }
    }
    if element.is(&Ns::Table, "named-range") {
        for attribute in &mut element.attrs {
            if attribute.name.is(&Ns::Table, "cell-range-address")
                || attribute.name.is(&Ns::Table, "base-cell-address")
            {
                attribute.value =
                    refs::shift_address(&attribute.value, None, change).map_err(blocked)?;
            }
        }
    }
    if element.is(&Ns::Table, "named-expression") {
        let context = element
            .attr(&Ns::Table, "base-cell-address")
            .and_then(refs::address_sheet);
        for attribute in &mut element.attrs {
            if attribute.name.is(&Ns::Table, "expression") {
                attribute.value = refs::shift_formula(&attribute.value, context.as_deref(), change)
                    .map_err(blocked)?;
            }
            if attribute.name.is(&Ns::Table, "base-cell-address") {
                attribute.value =
                    refs::shift_address(&attribute.value, None, change).map_err(blocked)?;
            }
        }
    }
    for child in &mut element.children {
        if let Node::Element(child) = child {
            shift_references(child, own.as_deref(), change)?;
        }
    }
    Ok(())
}

/// A cell with what it held taken out and its style kept: what an inserted
/// cell is. A covered cell becomes an ordinary one, because the merged region
/// that covered it does not grow.
fn cleared_cell(cell: &Element, repeat: usize, names: &RepeatNames) -> Element {
    let mut fresh = Element {
        name: cell.name.clone(),
        attrs: cell
            .attrs
            .iter()
            .filter(|a| a.name.is(&Ns::Table, "style-name"))
            .cloned()
            .collect(),
        children: Vec::new(),
        self_closing: true,
    };
    if cell.is(&Ns::Table, "covered-table-cell") {
        fresh.name.local = "table-cell".into();
    }
    if repeat > 1 {
        fresh.set_attr(names.columns.clone(), repeat.to_string());
    }
    fresh
}

/// A row of the neighbour's shape with nothing in it: its own formats, its
/// cells' formats and their repeats, and none of what they held.
fn cleared_row(row: &Element, count: usize, names: &RepeatNames) -> Element {
    let mut fresh = row.clone();
    fresh.remove_attr(&Ns::Table, "number-rows-repeated");
    fresh.remove_attr(&Ns::Table, "visibility");
    fresh.remove_attr(&Ns::Table, "filter");
    fresh.children = row
        .children
        .iter()
        .filter_map(|node| {
            let Node::Element(cell) = node else {
                return None;
            };
            if !(cell.is(&Ns::Table, "table-cell") || cell.is(&Ns::Table, "covered-table-cell")) {
                return None;
            }
            let repeat = cell
                .attr_usize(&Ns::Table, "number-columns-repeated")
                .unwrap_or(1)
                .max(1);
            Some(Node::Element(cleared_cell(cell, repeat, names)))
        })
        .collect();
    if count > 1 {
        fresh.set_attr(names.rows.clone(), count.to_string());
    }
    fresh
}

/// Put an element into a run of repeated ones before the position `offset`
/// into the run at `index`, splitting the run where that is inside it.
fn put_in(
    parent: &mut Element,
    index: usize,
    offset: usize,
    repeat: usize,
    new: Element,
    repeated: &Name,
) {
    if offset == 0 {
        parent.children.insert(index, Node::Element(new));
        return;
    }
    let Some(Node::Element(original)) = parent.children.get(index) else {
        return;
    };
    let before = with_count(original.clone(), offset, repeated);
    let after = with_count(original.clone(), repeat - offset, repeated);
    parent.children.splice(
        index..=index,
        [
            Node::Element(before),
            Node::Element(new),
            Node::Element(after),
        ],
    );
}

fn insert_rows(table: &mut Element, at: usize, count: usize, names: &RepeatNames) {
    let rows = rows_of(table);
    let find = |row: usize| {
        rows.iter()
            .find(|(_, first, n)| (*first..first + n).contains(&row))
    };
    let Some((path, first, repeat)) = find(at) else {
        return;
    };
    let reference = find(at.saturating_sub(1)).map_or(path, |(path, ..)| path);
    let Some(neighbour) = table.at(reference) else {
        return;
    };
    let new = cleared_row(neighbour, count, names);
    let Some((index, above)) = path.split_last() else {
        return;
    };
    if let Some(parent) = table.at_mut(above) {
        put_in(parent, *index, at - first, *repeat, new, &names.rows);
    }
}

fn insert_columns(table: &mut Element, at: usize, count: usize, names: &RepeatNames) {
    // The column definitions first.
    let defs = columns_of(table);
    let find = |column: usize| {
        defs.iter()
            .find(|(_, first, n)| (*first..first + n).contains(&column))
    };
    if let Some((path, first, repeat)) = find(at) {
        let reference = find(at.saturating_sub(1)).map_or(path, |(path, ..)| path);
        if let Some(neighbour) = table.at(reference) {
            let mut new = neighbour.clone();
            new.remove_attr(&Ns::Table, "number-columns-repeated");
            if count > 1 {
                new.set_attr(names.columns.clone(), count.to_string());
            }
            if let Some((index, above)) = path.split_last()
                && let Some(parent) = table.at_mut(above)
            {
                put_in(parent, *index, at - first, *repeat, new, &names.columns);
            }
        }
    }
    // Then a cell in every row.
    for (path, ..) in rows_of(table).into_iter().rev() {
        let Some(row) = table.at_mut(&path) else {
            continue;
        };
        let cells: Vec<(usize, usize, usize)> = cells_of(row)
            .enumerate()
            .map(|(n, (first, repeat, _))| (n, first, repeat))
            .collect();
        let Some(&(which, first, repeat)) = cells
            .iter()
            .find(|(_, first, repeat)| (*first..first + repeat).contains(&at))
        else {
            continue;
        };
        let reference = cells
            .iter()
            .find(|(_, first, repeat)| (*first..first + repeat).contains(&at.saturating_sub(1)))
            .map_or(which, |(n, ..)| *n);
        let indices: Vec<usize> = row
            .elements_indexed()
            .filter(|(_, e)| {
                e.is(&Ns::Table, "table-cell") || e.is(&Ns::Table, "covered-table-cell")
            })
            .map(|(i, _)| i)
            .collect();
        let Some(Node::Element(neighbour)) = row.children.get(indices[reference]) else {
            continue;
        };
        let new = cleared_cell(neighbour, count, names);
        put_in(row, indices[which], at - first, repeat, new, &names.columns);
        trim_overflow(row, names);
    }
}

/// A row that has grown past the last column ODF allows loses the excess from
/// the empty run that fills out its end.
fn trim_overflow(row: &mut Element, names: &RepeatNames) {
    let total: usize = cells_of(row).map(|(_, repeat, _)| repeat).sum();
    if total <= MAX_COLUMNS {
        return;
    }
    let excess = total - MAX_COLUMNS;
    let last = row
        .children
        .iter()
        .rposition(|n| matches!(n, Node::Element(e) if e.is(&Ns::Table, "table-cell")));
    if let Some(last) = last
        && let Some(Node::Element(cell)) = row.children.get(last)
    {
        let repeat = cell
            .attr_usize(&Ns::Table, "number-columns-repeated")
            .unwrap_or(1);
        if repeat > excess && cell.children.is_empty() {
            let trimmed = with_count(cell.clone(), repeat - excess, &names.columns);
            row.children[last] = Node::Element(trimmed);
        }
    }
}

/// Which kind of line a delete goes along.
#[derive(Clone, Copy)]
enum Run {
    Rows,
    Columns,
}

/// Take `count` lines out, from `at`: whole elements are removed and the runs
/// that only partly fall in the gap are shortened.
fn delete_run(table: &mut Element, run: Run, at: usize, count: usize, names: &RepeatNames) {
    let end = at + count;
    match run {
        Run::Rows => delete_in(
            table,
            &mut 0,
            at,
            end,
            &names.rows,
            is_row_container,
            "table-row",
        ),
        Run::Columns => {
            delete_in(
                table,
                &mut 0,
                at,
                end,
                &names.columns,
                is_column_container,
                "table-column",
            );
            let rows: Vec<Vec<usize>> = rows_of(table).into_iter().map(|(path, ..)| path).collect();
            for path in rows {
                if let Some(row) = table.at_mut(&path) {
                    delete_cells(row, at, end, &names.columns);
                }
            }
        }
    }
}

/// Delete from a parent's children the elements named `local` that fall in
/// `at..end` of the line they count along, descending into the containers that
/// hold some.
fn delete_in(
    parent: &mut Element,
    cursor: &mut usize,
    at: usize,
    end: usize,
    repeated: &Name,
    is_container: fn(&Element) -> bool,
    local: &str,
) {
    let mut index = 0;
    while index < parent.children.len() {
        let Node::Element(child) = &mut parent.children[index] else {
            index += 1;
            continue;
        };
        if child.is(&Ns::Table, local) {
            let repeat = child
                .attr_usize(&repeated.ns, &repeated.local)
                .unwrap_or(1)
                .max(1);
            let (start, stop) = (*cursor, *cursor + repeat);
            *cursor = stop;
            let overlap = stop.min(end).saturating_sub(start.max(at));
            if overlap == 0 {
                index += 1;
            } else if overlap == repeat {
                parent.children.remove(index);
            } else {
                *child = with_count(child.clone(), repeat - overlap, repeated);
                index += 1;
            }
        } else if is_container(child) {
            delete_in(child, cursor, at, end, repeated, is_container, local);
            let empty = child.elements().next().is_none();
            if empty {
                parent.children.remove(index);
            } else {
                index += 1;
            }
        } else {
            index += 1;
        }
    }
}

/// Delete the cells of a row that fall in `at..end`, covered cells with them.
fn delete_cells(row: &mut Element, at: usize, end: usize, repeated: &Name) {
    let mut cursor = 0;
    let mut index = 0;
    while index < row.children.len() {
        let Node::Element(cell) = &mut row.children[index] else {
            index += 1;
            continue;
        };
        if !(cell.is(&Ns::Table, "table-cell") || cell.is(&Ns::Table, "covered-table-cell")) {
            index += 1;
            continue;
        }
        let repeat = cell
            .attr_usize(&Ns::Table, "number-columns-repeated")
            .unwrap_or(1)
            .max(1);
        let (start, stop) = (cursor, cursor + repeat);
        cursor = stop;
        let overlap = stop.min(end).saturating_sub(start.max(at));
        if overlap == 0 {
            index += 1;
        } else if overlap == repeat {
            row.children.remove(index);
        } else {
            *cell = with_count(cell.clone(), repeat - overlap, repeated);
            index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spreadsheet(inner: &str) -> Element {
        let source = format!(
            r#"<office:spreadsheet xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:calcext="urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0">{inner}</office:spreadsheet>"#
        );
        crate::xml::parse(source.as_bytes(), "test").expect("a spreadsheet")
    }

    #[test]
    fn what_names_a_range_of_cells_is_found_and_empty_containers_are_not() {
        let found = |inner: &str| unmovable(&spreadsheet(inner));
        assert_eq!(found("<table:table table:name=\"a\"/>"), None);
        assert_eq!(found("<table:named-expressions/>"), None);
        assert_eq!(found("<table:database-ranges/>"), None);
        assert_eq!(
            found(
                "<table:table table:name=\"a\"><calcext:conditional-formats><calcext:conditional-format/></calcext:conditional-formats></table:table>"
            ),
            Some("conditional formats")
        );
        assert_eq!(
            found(
                "<table:content-validations><table:content-validation/></table:content-validations>"
            ),
            Some("validations")
        );
        assert_eq!(
            found("<table:table table:name=\"a\" table:print-ranges=\"a.A1:a.B2\"/>"),
            Some("print ranges")
        );
        assert_eq!(
            found(
                "<table:table table:name=\"a\"><table:shapes><draw:frame table:end-cell-address=\"a.C3\"/></table:shapes></table:table>"
            ),
            Some("shapes anchored to cells")
        );
        assert_eq!(
            found(
                "<table:table table:name=\"a\"><draw:frame><draw:object/></draw:frame></table:table>"
            ),
            Some("charts and objects")
        );
    }

    #[test]
    fn a_repeated_run_is_split_for_an_insert_and_shortened_for_a_delete() {
        let names = RepeatNames {
            rows: Name::new("table", "number-rows-repeated", Ns::Table),
            columns: Name::new("table", "number-columns-repeated", Ns::Table),
        };
        let mut table = spreadsheet(
            "<table:table table:name=\"a\"><table:table-row table:number-rows-repeated=\"10\"><table:table-cell/></table:table-row></table:table>",
        );
        let table = table.child_mut(&Ns::Table, "table").expect("a table");
        insert_rows(table, 4, 2, &names);
        let counts: Vec<usize> = table
            .elements()
            .map(|row| {
                row.attr_usize(&Ns::Table, "number-rows-repeated")
                    .unwrap_or(1)
            })
            .collect();
        assert_eq!(counts, [4, 2, 6], "four before, two new, six after");
        delete_run(table, Run::Rows, 3, 5, &names);
        let counts: Vec<usize> = table
            .elements()
            .map(|row| {
                row.attr_usize(&Ns::Table, "number-rows-repeated")
                    .unwrap_or(1)
            })
            .collect();
        assert_eq!(counts.iter().sum::<usize>(), 12 - 5, "five rows fewer");
    }
}
