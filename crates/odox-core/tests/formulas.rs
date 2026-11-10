//! Formulas worked out here against what LibreOffice worked out.
//!
//! `corpus/libreoffice/formulas.ods` was written by LibreOffice from formulas
//! with no results, so every result in it is LibreOffice's own. Working each
//! formula out again has to give the same value of the same kind.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use odox_core::Ns;
use odox_core::doc::{SheetDocument, Value};

fn fixture() -> SheetDocument {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/formulas.ods"),
    )
    .expect("the formulas fixture");
    SheetDocument::read(&bytes).expect("it reads")
}

const CALC: usize = 1;

/// Each formula on the Calc sheet: its label, its value, whether it is an
/// error, and the text it shows.
fn results(document: &SheetDocument) -> Vec<(String, Value, bool, String)> {
    let sheet = &document.sheets()[CALC];
    (0..80)
        .filter_map(|row| {
            let label = document.cell(sheet, row, 0)?.text();
            let cell = document.cell(sheet, row, 1)?;
            cell.formula()?;
            let error = cell.element.attr(&Ns::Calcext, "value-type") == Some("error");
            Some((label, cell.value(), error, cell.text()))
        })
        .collect()
}

fn same(a: &Value, b: &Value) -> bool {
    let near = |x: f64, y: f64| (x - y).abs() <= 1e-9 * x.abs().max(1.0);
    match (a, b) {
        (Value::Number(x), Value::Number(y)) | (Value::Percentage(x), Value::Percentage(y)) => {
            near(*x, *y)
        }
        (Value::Currency(x, c), Value::Currency(y, d)) => near(*x, *y) && c == d,
        _ => a == b,
    }
}

fn at(document: &SheetDocument, label: &str) -> Value {
    results(document)
        .into_iter()
        .find(|(l, ..)| l == label)
        .map(|(_, value, ..)| value)
        .expect("a formula with that label")
}

#[test]
fn every_formula_comes_to_what_libreoffice_worked_out() {
    let mut document = fixture();
    let before = results(&document);
    assert!(
        before.len() >= 60,
        "the fixture has its formulas: {}",
        before.len()
    );
    let done = document.recalculate_all();
    assert!(
        done.stale.is_empty(),
        "nothing is beyond the core: {:?}",
        done.stale
    );
    let after = results(&document);
    let wrong: Vec<String> = before
        .iter()
        .zip(&after)
        .filter(|((_, a, ae, at), (_, b, be, bt))| !same(a, b) || ae != be || (*ae && at != bt))
        .map(|((label, a, _, at), (_, b, _, bt))| {
            format!("{label}: {b:?} {bt:?}, LibreOffice {a:?} {at:?}")
        })
        .collect();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn a_changed_value_reaches_what_reads_it() {
    let mut document = fixture();
    document
        .set_cell(0, 1, 1, &Value::Number(10.0))
        .expect("set");
    let done = document.recalculate(&[(0, 1, 1)]);
    assert!(done.stale.is_empty());
    assert_eq!(at(&document, "arithmetic"), Value::Number(13.0));
    assert_eq!(at(&document, "sum of a column"), Value::Number(18.0));
    assert_eq!(at(&document, "parentheses"), Value::Number(23.0));
    assert_eq!(at(&document, "depends on a formula"), Value::Number(46.0));
    assert_eq!(at(&document, "if true"), Value::Text("big".to_owned()));
    assert_eq!(
        at(&document, "max"),
        Value::Number(10.0),
        "a column that was not read keeps its result"
    );
    let untouched = done.updated.iter().all(|place| place.0 == CALC);
    assert!(untouched, "only formulas were rewritten");
}

/// The fixture with one formula on the Calc sheet replaced.
fn with_formula(row: usize, written: &str) -> SheetDocument {
    let mut document = fixture();
    let table = document
        .document
        .body_of("spreadsheet")
        .and_then(|body| {
            body.elements_indexed()
                .filter(|(_, e)| e.is(&Ns::Table, "table"))
                .nth(CALC)
                .map(|(index, _)| index)
        })
        .expect("the Calc table");
    let body = document.document.body_path("spreadsheet").expect("a body");
    let content = &mut document.document.content;
    let calc = content
        .at_mut(&[body, vec![table]].concat())
        .expect("table");
    let (row_index, _) = calc
        .elements_indexed()
        .filter(|(_, e)| e.is(&Ns::Table, "table-row"))
        .nth(row)
        .expect("the row");
    let row_element = calc.at_mut(&[row_index]).expect("row");
    let (cell_index, _) = row_element
        .elements_indexed()
        .filter(|(_, e)| e.is(&Ns::Table, "table-cell"))
        .nth(1)
        .expect("the formula cell");
    let cell = row_element.at_mut(&[cell_index]).expect("cell");
    cell.set_attr(cell.name_for(&Ns::Table, "formula"), written);
    document.reindex();
    document
}

#[test]
fn a_formula_outside_the_core_and_what_reads_it_go_stale() {
    let mut document = with_formula(1, "of:=NPV(0.1;[$Data.B2:.B4])");
    let done = document.recalculate(&[(0, 1, 1)]);
    let calc_rows: Vec<usize> = done.stale.iter().map(|p| p.1).collect();
    assert!(
        calc_rows.contains(&1),
        "the unsupported formula: {:?}",
        done.stale
    );
    let depends = results(&document)
        .iter()
        .position(|(label, ..)| label == "depends on a formula")
        .expect("the dependent row");
    assert!(
        calc_rows.contains(&depends),
        "and the formula that reads it"
    );
}

#[test]
fn a_formula_that_reads_itself_is_circular() {
    let mut document = with_formula(0, "of:=[.B1]+1");
    document.recalculate_all();
    let sheet = &document.sheets()[CALC];
    assert_eq!(document.cell(sheet, 0, 1).expect("cell").text(), "Err:522");
}

#[test]
fn a_typed_formula_is_stored_and_worked_out_at_once() {
    let mut document = fixture();
    let stored = odox_core::formula::to_stored("=Data!B2+SUM(Data.C2:C3)").expect("readable");
    document.set_formula(CALC, 0, 2, &stored).expect("set");
    let done = document.recalculate(&[(CALC, 0, 2)]);
    assert!(done.updated.contains(&(CALC, 0, 2)));
    let sheet = &document.sheets()[CALC];
    let cell = document.cell(sheet, 0, 2).expect("the cell");
    assert_eq!(cell.value(), Value::Number(8.5));
    assert_eq!(cell.text(), "8.5");
    assert_eq!(
        cell.element
            .attr(&Ns::Table, "formula")
            .map(odox_core::formula::to_typed)
            .as_deref(),
        Some("=$Data.B2+SUM($Data.C2:C3)")
    );
}

#[test]
fn a_typed_formula_outside_the_core_is_kept_and_stale() {
    let mut document = fixture();
    let stored = odox_core::formula::to_stored("=NPV(0.1;Data.B2:B4)").expect("readable");
    document.set_formula(CALC, 0, 2, &stored).expect("set");
    let done = document.recalculate(&[(CALC, 0, 2)]);
    assert_eq!(done.stale, vec![(CALC, 0, 2)]);
    let sheet = &document.sheets()[CALC];
    assert_eq!(
        document
            .cell(sheet, 0, 2)
            .expect("cell")
            .element
            .attr(&Ns::Table, "formula"),
        Some("of:=NPV(0.1;[$Data.B2:.B4])")
    );
}

#[test]
fn a_value_typed_over_a_formula_reaches_what_read_it() {
    let mut document = fixture();
    document
        .set_cell(CALC, 1, 1, &Value::Number(100.0))
        .expect("set");
    document.recalculate(&[(CALC, 1, 1)]);
    assert_eq!(at(&document, "depends on a formula"), Value::Number(200.0));
}

#[test]
fn an_error_result_reads_as_the_error_it_shows_and_sits_right() {
    let document = fixture();
    let errors: Vec<Value> = results(&document)
        .into_iter()
        .filter(|(.., error, _)| *error)
        .map(|(_, value, ..)| value)
        .collect();
    assert!(
        errors.contains(&Value::Error("#DIV/0!".to_owned())),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .all(|value| matches!(value, Value::Error(shown) if shown.starts_with(['#', 'E'])))
    );
    assert!(errors.iter().all(Value::aligns_right));
}
