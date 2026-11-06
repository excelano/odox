//! Sorting a block's rows, on the blank spreadsheet File > New makes.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::doc::{Block, SheetDocument, Value};

/// Item and Qty with a header row, and in column C a formula doubling the row's
/// Qty: pear 2, apple 10, Banana 5, plum with no Qty.
fn table() -> SheetDocument {
    let mut document =
        SheetDocument::read(&odox_core::blank::spreadsheet("en-US", "Sheet1")).expect("blank");
    let text = |s: &str| Value::Text(s.to_owned());
    let rows = [
        (text("Item"), text("Qty")),
        (text("pear"), Value::Number(2.0)),
        (text("apple"), Value::Number(10.0)),
        (text("Banana"), Value::Number(5.0)),
        (text("plum"), Value::Empty),
    ];
    for (row, (item, qty)) in rows.iter().enumerate() {
        document.set_cell(0, row, 0, item).expect("item");
        document.set_cell(0, row, 1, qty).expect("qty");
    }
    for row in 1..5 {
        let formula = format!("of:=[.B{}]*2", row + 1);
        document.set_formula(0, row, 2, &formula).expect("formula");
    }
    let all: Vec<_> = (1..5).map(|row| (0, row, 2)).collect();
    document.recalculate(&all);
    document
}

fn block() -> Block {
    Block {
        top: 0,
        left: 0,
        bottom: 4,
        right: 2,
    }
}

fn column(document: &SheetDocument, c: usize) -> Vec<Value> {
    let sheet = &document.sheets()[0];
    (1..5)
        .map(|row| {
            document
                .cell(sheet, row, c)
                .map_or(Value::Empty, |cell| cell.value())
        })
        .collect()
}

fn items(document: &SheetDocument) -> Vec<String> {
    column(document, 0)
        .into_iter()
        .map(|v| match v {
            Value::Text(t) => t,
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn a_header_is_seen_and_kept_in_place() {
    let mut document = table();
    assert!(document.has_header(0, block()));
    let written = document.sort(0, block(), 1, false, true).expect("sorted");
    document.recalculate(&written);
    let sheet = &document.sheets()[0];
    assert_eq!(document.cell(sheet, 0, 0).expect("header").text(), "Item");
    assert_eq!(items(&document), ["pear", "Banana", "apple", "plum"]);
}

#[test]
fn text_sorts_without_its_case_and_empty_cells_go_last_either_way() {
    let mut document = table();
    document.sort(0, block(), 0, false, true).expect("sorted");
    assert_eq!(items(&document), ["apple", "Banana", "pear", "plum"]);
    let mut document = table();
    document.sort(0, block(), 1, true, true).expect("sorted");
    assert_eq!(items(&document), ["apple", "Banana", "pear", "plum"]);
}

#[test]
fn a_formula_moves_with_its_row() {
    let mut document = table();
    let written = document.sort(0, block(), 1, true, true).expect("sorted");
    document.recalculate(&written);
    assert_eq!(
        column(&document, 2),
        [
            Value::Number(20.0),
            Value::Number(10.0),
            Value::Number(4.0),
            Value::Number(0.0)
        ]
    );
}

#[test]
fn the_block_around_a_cell_is_found() {
    let document = table();
    assert_eq!(document.region(0, 2, 1), block());
}
