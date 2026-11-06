//! Tables put into a text document and changed there, measured on the blank
//! document File > New makes.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::doc::TextDocument;
use odox_core::edit::{self, TableEdit};
use odox_core::{Element, Family, Ns};

fn blank() -> TextDocument {
    TextDocument::read(&odox_core::blank::text("en-US")).expect("a blank document")
}

/// The path of the blank document's one paragraph.
fn first_paragraph(document: &TextDocument) -> Vec<usize> {
    let body = document.body_path().expect("a body");
    let text = document.body().expect("a body");
    let (index, _) = text
        .elements_indexed()
        .find(|(_, e)| edit::is_paragraph(e))
        .expect("a paragraph");
    [body, vec![index]].concat()
}

fn table_of<'a>(document: &'a TextDocument, path: &[usize]) -> &'a Element {
    let place = edit::table_at(&document.document.content, path).expect("in a table");
    document
        .document
        .content
        .at(&place.table)
        .expect("the table")
}

fn shape(table: &Element) -> Vec<usize> {
    table
        .elements()
        .filter(|e| e.is(&Ns::Table, "table-row"))
        .map(|row| {
            row.elements()
                .filter(|c| c.is(&Ns::Table, "table-cell"))
                .count()
        })
        .collect()
}

fn widths(document: &TextDocument, table: &Element) -> Vec<f32> {
    table
        .elements()
        .filter(|e| e.is(&Ns::Table, "table-column"))
        .map(|column| {
            let name = column
                .attr(&Ns::Table, "style-name")
                .expect("a column style");
            document
                .document
                .styles
                .resolve(&Family::TableColumn, name)
                .column_width
                .expect("a width")
                .points()
        })
        .collect()
}

/// A document with a table of two rows and three columns, 300 points wide,
/// and the path of its first cell's paragraph.
fn with_table() -> (TextDocument, Vec<usize>) {
    let mut document = blank();
    let paragraph = first_paragraph(&document);
    let odox_core::Document {
        content, styles, ..
    } = &mut document.document;
    let had = content.child(&Ns::Office, "automatic-styles").is_some();
    let mut first = edit::insert_table(content, &paragraph, 2, 3, 300.0, styles).expect("inserted");
    if !had && content.child(&Ns::Office, "automatic-styles").is_some() {
        first[0] += 1;
    }
    let document =
        TextDocument::read(&document.document.write().expect("written")).expect("read back");
    let place = edit::table_at(&document.document.content, &first);
    assert!(place.is_some(), "the caret's paragraph is in the table");
    (document, first)
}

#[test]
fn a_table_goes_in_with_its_columns_and_a_paragraph_after() {
    let (document, first) = with_table();
    let table = table_of(&document, &first);
    assert_eq!(shape(table), vec![3, 3]);
    let column_widths = widths(&document, table);
    assert_eq!(column_widths.len(), 3);
    assert!(
        column_widths.iter().all(|w| (w - 100.0).abs() < 0.1),
        "{column_widths:?}"
    );
    let place = edit::table_at(&document.document.content, &first).expect("in the table");
    assert_eq!((place.row, place.column), (0, 0));
    let parent = document
        .document
        .content
        .at(&place.table[..place.table.len() - 1])
        .expect("the body");
    let after = parent
        .elements_indexed()
        .find(|(i, _)| *i > *place.table.last().expect("an index"))
        .expect("something after the table");
    assert!(edit::is_paragraph(after.1), "a paragraph follows the table");
}

#[test]
fn tab_walks_the_cells_row_by_row() {
    let (document, first) = with_table();
    let content = &document.document.content;
    let mut at = first;
    let mut seen = vec![];
    while let Some(next) = edit::step_cell(content, &at, true) {
        let place = edit::table_at(content, &next).expect("in the table");
        seen.push((place.row, place.column));
        at = next;
    }
    assert_eq!(seen, vec![(0, 1), (0, 2), (1, 0), (1, 1), (1, 2)]);
    let back = edit::step_cell(content, &at, false).expect("a cell before");
    let place = edit::table_at(content, &back).expect("in the table");
    assert_eq!((place.row, place.column), (1, 1));
}

#[test]
fn rows_and_columns_go_in_and_come_out_keeping_the_width() {
    let (mut document, first) = with_table();
    let odox_core::Document {
        content, styles, ..
    } = &mut document.document;

    let below = edit::edit_table(content, &first, TableEdit::RowBelow, styles).expect("row");
    let place = edit::table_at(content, &below).expect("in the table");
    assert_eq!((place.row, place.column), (1, 0));
    assert_eq!(
        shape(content.at(&place.table).expect("table")),
        vec![3, 3, 3]
    );

    let right = edit::edit_table(content, &first, TableEdit::ColumnRight, styles).expect("column");
    let place = edit::table_at(content, &right).expect("in the table");
    assert_eq!((place.row, place.column), (0, 1));
    let table = content.at(&place.table).expect("table").clone();
    assert_eq!(shape(&table), vec![4, 4, 4]);

    let removed =
        edit::edit_table(content, &right, TableEdit::DeleteColumn, styles).expect("delete");
    let place = edit::table_at(content, &removed).expect("in the table");
    assert_eq!(
        shape(content.at(&place.table).expect("table")),
        vec![3, 3, 3]
    );

    let document =
        TextDocument::read(&document.document.write().expect("written")).expect("read back");
    let table = table_of(&document, &removed);
    let total: f32 = widths(&document, table).iter().sum();
    assert!(
        (total - 300.0).abs() < 0.5,
        "the table stays as wide: {total}"
    );
}

#[test]
fn the_last_row_taken_out_takes_the_table() {
    let (mut document, first) = with_table();
    let odox_core::Document {
        content, styles, ..
    } = &mut document.document;
    let row = edit::edit_table(content, &first, TableEdit::DeleteRow, styles).expect("row");
    assert!(edit::table_at(content, &row).is_some());
    let caret = edit::edit_table(content, &row, TableEdit::DeleteRow, styles).expect("table");
    assert!(edit::table_at(content, &caret).is_none());
    assert!(edit::is_paragraph(content.at(&caret).expect("a paragraph")));
    assert!(!format!("{content:?}").contains("table-row"));
}

#[test]
fn a_merged_table_is_left_as_it_is() {
    let (mut document, first) = with_table();
    let odox_core::Document {
        content, styles, ..
    } = &mut document.document;
    let place = edit::table_at(content, &first).expect("in the table");
    let cell = content
        .at_mut(&first[..first.len() - 1])
        .expect("the first cell");
    cell.set_attr(cell.name_for(&Ns::Table, "number-columns-spanned"), "2");
    let before = content.clone();
    assert_eq!(
        edit::edit_table(content, &first, TableEdit::ColumnLeft, styles),
        Err(odox_core::Refused::Structure)
    );
    assert_eq!(*content, before);
    assert!(edit::edit_table(content, &first, TableEdit::DeleteTable, styles).is_ok());
    assert!(
        content
            .at(&place.table)
            .is_none_or(|e| !e.is(&Ns::Table, "table"))
    );
}
