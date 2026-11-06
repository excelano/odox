//! Number formats, measured against what LibreOffice writes.
//!
//! `corpus/libreoffice/formats.ods` holds a cell in each kind of data style
//! Calc writes, each with the text Calc showed for it. Formatting a cell's
//! value in its own style has to give that text back, and so does typing it
//! again and formatting what was read.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use odox_core::doc::{SheetDocument, Value};

fn formats() -> SheetDocument {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/formats.ods"),
    )
    .expect("the formats fixture");
    SheetDocument::read(&bytes).expect("it reads")
}

/// Every cell in the fixture that has a data style, with the text Calc wrote.
fn styled(document: &SheetDocument) -> Vec<(usize, usize, Value, String)> {
    let sheet = &document.sheets()[0];
    let mut out = Vec::new();
    for row in 1..16 {
        for column in 1..3 {
            let Some(cell) = document.cell(sheet, row, column) else {
                continue;
            };
            if document.data_style(0, row, column).is_some() && cell.value() != Value::Empty {
                out.push((row, column, cell.value(), cell.text()));
            }
        }
    }
    out
}

/// What a cell shows once a value is put in it, on a fresh copy of the fixture.
fn shown(row: usize, column: usize, value: &Value) -> Option<String> {
    let mut document = formats();
    document.set_cell(0, row, column, value).expect("set");
    let sheet = &document.sheets()[0];
    document.cell(sheet, row, column).map(|cell| cell.text())
}

#[test]
fn every_value_is_shown_as_calc_showed_it() {
    let document = formats();
    let cells = styled(&document);
    assert!(
        cells.len() >= 16,
        "the fixture has its cells: {}",
        cells.len()
    );
    let wrong: Vec<String> = cells
        .iter()
        .filter_map(|(row, column, value, calc)| {
            let ours = shown(*row, *column, value)?;
            (ours != *calc).then(|| format!("row {row} column {column}: {ours:?}, Calc {calc:?}"))
        })
        .collect();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn what_a_cell_shows_reads_back_as_its_value() {
    let document = formats();
    let wrong: Vec<String> = styled(&document)
        .iter()
        .filter(|(_, _, value, _)| !matches!(value, Value::Number(_)))
        .filter_map(|(row, column, value, calc)| {
            let read = document.read_input(0, *row, *column, calc);
            let same = match (&read, value) {
                (Value::Currency(a, _), Value::Currency(b, _)) => (a - b).abs() < 1e-9,
                (Value::Percentage(a), Value::Percentage(b)) => (a - b).abs() < 1e-9,
                (Value::Time(a), Value::Time(b)) => a.replace("00S", "") == b.replace("00S", ""),
                (a, b) => a == b,
            };
            (!same).then(|| {
                format!("row {row} column {column}: {calc:?} read as {read:?}, was {value:?}")
            })
        })
        .collect();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn a_typed_amount_takes_the_cells_currency() {
    let document = formats();
    let read = document.read_input(0, 3, 1, "1234.5");
    assert_eq!(read, Value::Currency(1234.5, None));
    assert_eq!(shown(3, 1, &read).as_deref(), Some("$1,234.50"));
    assert_eq!(
        shown(3, 1, &Value::Currency(-7.0, None)).as_deref(),
        Some("-$7.00")
    );

    let mut written = formats();
    written
        .set_cell(0, 5, 1, &document.read_input(0, 5, 1, "1234,5"))
        .expect("set");
    let sheet = &written.sheets()[0];
    let cell = written.cell(sheet, 5, 1).expect("the euro cell");
    assert_eq!(cell.text(), "1.234,50 €");
    assert_eq!(
        cell.value(),
        Value::Currency(1234.5, Some("EUR".to_owned()))
    );
}

#[test]
fn input_reads_in_the_cells_format() {
    let document = formats();
    assert_eq!(document.read_input(0, 6, 1, "15"), Value::Percentage(0.15));
    assert_eq!(document.read_input(0, 6, 1, "15%"), Value::Percentage(0.15));
    assert_eq!(
        document.read_input(0, 8, 1, "3/9/26"),
        Value::Date("2026-03-09".to_owned())
    );
    assert_eq!(
        document.read_input(0, 8, 1, "2026-03-09"),
        Value::Date("2026-03-09".to_owned())
    );
    assert_eq!(
        document.read_input(0, 8, 1, "2/30/2026"),
        Value::Text("2/30/2026".to_owned())
    );
    assert_eq!(
        document.read_input(0, 12, 1, "9:30 pm"),
        Value::Time("PT21H30M00S".to_owned())
    );
    assert_eq!(
        document.read_input(0, 1, 1, "1,234.5"),
        Value::Number(1234.5)
    );
    assert_eq!(
        document.read_input(0, 1, 1, "words"),
        Value::Text("words".to_owned())
    );
}

#[test]
fn a_cell_with_no_format_reads_a_percentage() {
    assert_eq!(Value::from_input("15%"), Value::Percentage(0.15));
    assert_eq!(Value::Percentage(0.1234).input_text(), "12.34%");
    let Value::Percentage(read) = Value::from_input("12.34%") else {
        panic!("not a percentage");
    };
    assert!((read - 0.1234).abs() < 1e-12);
}

#[test]
fn a_negative_amount_takes_the_colour_its_format_gives_it() {
    let document = formats();
    let sheet = &document.sheets()[0];
    let red = odox_core::Color { r: 255, g: 0, b: 0 };
    assert_eq!(
        document.value_colour(sheet, 4, 1),
        Some(red),
        "dollars negative"
    );
    assert_eq!(document.value_colour(sheet, 3, 1), None, "dollars positive");
    assert_eq!(
        document.value_colour(sheet, 0, 1),
        None,
        "text in no format"
    );
}
