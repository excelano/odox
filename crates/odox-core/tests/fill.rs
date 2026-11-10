//! Filling cells down and across, on the blank spreadsheet File > New makes.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::Ns;
use odox_core::doc::{Block, SheetDocument, Value};

fn blank() -> SheetDocument {
    SheetDocument::read(&odox_core::blank::spreadsheet("en-US", "Sheet1")).expect("a blank sheet")
}

fn put(document: &mut SheetDocument, cells: &[((usize, usize), Value)]) {
    for ((row, column), value) in cells {
        document.set_cell(0, *row, *column, value).expect("set");
    }
}

fn value(document: &SheetDocument, row: usize, column: usize) -> Value {
    let sheet = &document.sheets()[0];
    document
        .cell(sheet, row, column)
        .map_or(Value::Empty, |cell| cell.value())
}

fn text(s: &str) -> Value {
    Value::Text(s.to_owned())
}

fn column(top: usize, bottom: usize) -> Block {
    Block {
        top,
        left: 0,
        bottom,
        right: 0,
    }
}

#[test]
fn a_fill_down_copies_values_and_moves_formulas() {
    let mut document = blank();
    put(&mut document, &[((0, 0), Value::Number(4.0))]);
    document
        .set_formula(0, 0, 1, "of:=[.A1]*[.$A$1]")
        .expect("formula");
    let source = Block {
        top: 0,
        left: 0,
        bottom: 0,
        right: 1,
    };
    let written = document.fill(0, source, true, 2, false).expect("filled");
    assert_eq!(written.len(), 4);
    document.recalculate(&written);
    assert_eq!(value(&document, 2, 0), Value::Number(4.0));
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 2, 1).expect("the filled formula");
    assert_eq!(
        cell.element.attr(&Ns::Table, "formula"),
        Some("of:=[.A3]*[.$A$1]")
    );
    assert_eq!(cell.value(), Value::Number(16.0));
}

#[test]
fn a_fill_right_moves_a_formula_across() {
    let mut document = blank();
    put(
        &mut document,
        &[((0, 0), Value::Number(2.0)), ((0, 1), Value::Number(5.0))],
    );
    document
        .set_formula(0, 1, 0, "of:=[.A1]+1")
        .expect("formula");
    let source = Block {
        top: 1,
        left: 0,
        bottom: 1,
        right: 0,
    };
    let written = document.fill(0, source, false, 1, false).expect("filled");
    document.recalculate(&written);
    assert_eq!(value(&document, 1, 1), Value::Number(6.0));
}

#[test]
fn a_series_goes_on_where_the_cells_start_one() {
    let cases: Vec<(Vec<Value>, Vec<Value>)> = vec![
        (
            vec![Value::Number(1.0), Value::Number(3.0)],
            vec![Value::Number(5.0), Value::Number(7.0)],
        ),
        (
            vec![
                Value::Date("2026-01-30".to_owned()),
                Value::Date("2026-01-31".to_owned()),
            ],
            vec![
                Value::Date("2026-02-01".to_owned()),
                Value::Date("2026-02-02".to_owned()),
            ],
        ),
        (
            vec![
                Value::Date("2024-01-15".to_owned()),
                Value::Date("2024-02-15".to_owned()),
            ],
            vec![
                Value::Date("2024-03-15".to_owned()),
                Value::Date("2024-04-15".to_owned()),
            ],
        ),
        (
            vec![
                Value::Date("2023-10-31".to_owned()),
                Value::Date("2023-12-31".to_owned()),
            ],
            vec![
                Value::Date("2024-02-29".to_owned()),
                Value::Date("2024-04-30".to_owned()),
            ],
        ),
        (
            vec![
                Value::Date("2024-03-01".to_owned()),
                Value::Date("2025-03-01".to_owned()),
            ],
            vec![Value::Date("2026-03-01".to_owned())],
        ),
        (vec![text("Mon")], vec![text("Tue"), text("Wed")]),
        (vec![text("SATURDAY")], vec![text("SUNDAY"), text("MONDAY")]),
        (
            vec![text("January"), text("March")],
            vec![text("May"), text("July")],
        ),
        (vec![text("Item 1")], vec![text("Item 2"), text("Item 3")]),
        (
            vec![text("Q01"), text("Q03")],
            vec![text("Q05"), text("Q07")],
        ),
        (
            vec![Value::Number(7.0)],
            vec![Value::Number(7.0), Value::Number(7.0)],
        ),
        (vec![text("a"), text("b")], vec![text("a"), text("b")]),
    ];
    for (start, expected) in cases {
        let mut document = blank();
        let cells: Vec<_> = start
            .iter()
            .enumerate()
            .map(|(row, v)| ((row, 0), v.clone()))
            .collect();
        put(&mut document, &cells);
        document
            .fill(0, column(0, start.len() - 1), true, expected.len(), true)
            .expect("filled");
        let got: Vec<Value> = (start.len()..start.len() + expected.len())
            .map(|row| value(&document, row, 0))
            .collect();
        assert_eq!(got, expected, "from {start:?}");
    }
}

#[test]
fn a_fill_over_a_merge_is_refused() {
    let mut document = blank();
    put(&mut document, &[((0, 0), Value::Number(1.0))]);
    document.merge(0, 2..=3, 0..=0).expect("merged");
    let before = document.document.content.clone();
    assert_eq!(
        document.fill(0, column(0, 0), true, 3, false),
        Err(odox_core::Refused::Structure)
    );
    assert_eq!(document.document.content, before);
}

#[test]
fn a_formula_put_past_the_written_rows_lands_in_its_own_row() {
    let mut document = blank();
    document.set_formula(0, 5, 2, "of:=1+1").expect("formula");
    document.recalculate(&[(0, 5, 2)]);
    assert_eq!(value(&document, 5, 2), Value::Number(2.0));
    assert_eq!(value(&document, 6, 2), Value::Empty);
}
