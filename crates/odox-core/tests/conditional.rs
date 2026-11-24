//! Conditional formats as LibreOffice writes them, in
//! `corpus/libreoffice/conditional.ods`: on Sheet1, A1:A4 is Good over 10 and
//! Bad over 4; B1:B3 is Neutral between 0 and 1 and Good where it is more than
//! the cell to its left; C1:C3 has a duplicate test this does not read, then
//! Good where the text begins with "b" and Neutral where it holds "pie".

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use odox_core::Ns;
use odox_core::doc::{SheetDocument, Value};

fn fixture() -> SheetDocument {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/conditional.ods"),
    )
    .expect("the fixture");
    SheetDocument::read(&bytes).expect("it reads")
}

fn applied(document: &SheetDocument, row: usize, column: usize) -> Option<&str> {
    document.conditional_style(0, row, column)
}

#[test]
fn greater_than_applies_style() {
    let document = fixture();
    assert_eq!(applied(&document, 0, 0), Some("Good"));
    assert_eq!(applied(&document, 1, 0), None);
}

#[test]
fn first_matching_condition_wins() {
    let document = fixture();
    assert_eq!(applied(&document, 2, 0), Some("Bad"));
    assert_eq!(applied(&document, 3, 0), Some("Good"));
}

#[test]
fn formula_condition_relative_to_base_cell() {
    let document = fixture();
    assert_eq!(applied(&document, 2, 1), Some("Good"));
}

#[test]
fn between_inclusive() {
    let document = fixture();
    assert_eq!(applied(&document, 0, 1), Some("Neutral"));
    assert_eq!(applied(&document, 1, 1), Some("Neutral"));
}

#[test]
fn unsupported_condition_never_matches() {
    let document = fixture();
    assert_eq!(applied(&document, 0, 2), None);
    assert_eq!(applied(&document, 1, 2), Some("Good"));
    assert_eq!(applied(&document, 2, 2), Some("Neutral"));
}

#[test]
fn falls_back_to_style_map() {
    let mut document = fixture();
    let spreadsheet = document
        .document
        .content
        .child_mut(&Ns::Office, "body")
        .and_then(|body| body.child_mut(&Ns::Office, "spreadsheet"))
        .expect("a spreadsheet");
    let table = spreadsheet
        .children
        .iter_mut()
        .find_map(|node| match node {
            odox_core::xml::Node::Element(e) if e.is(&Ns::Table, "table") => Some(e),
            _ => None,
        })
        .expect("a table");
    table
        .children
        .retain(|node| !matches!(node, odox_core::xml::Node::Element(e) if e.name.local.as_ref() == "conditional-formats"));
    document.reindex();
    assert_eq!(applied(&document, 0, 0), Some("Good"));
    assert_eq!(applied(&document, 2, 0), Some("Bad"));
    assert_eq!(applied(&document, 1, 1), Some("Neutral"));
    assert_eq!(applied(&document, 2, 1), Some("Good"));
    assert_eq!(applied(&document, 1, 2), None);
}

#[test]
fn result_recomputed_after_edit() {
    let mut document = fixture();
    assert_eq!(applied(&document, 3, 0), Some("Good"));
    document
        .set_cell(0, 0, 0, &Value::Number(2.0))
        .expect("written");
    document.recalculate(&[(0, 0, 0)]);
    assert_eq!(applied(&document, 0, 0), None);
    assert_eq!(applied(&document, 3, 0), None);
}

#[test]
fn the_applied_style_is_layered_over_the_cells_own() {
    let document = fixture();
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 0, 0);
    let style = document.shown_style(sheet, cell.as_ref(), 0, 0);
    assert_eq!(
        style.cell.background.map(|c| (c.r, c.g, c.b)),
        Some((0xcc, 0xff, 0xcc))
    );
}

#[test]
fn document_is_unchanged_after_open() {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/conditional.ods"),
    )
    .expect("the fixture");
    let document = SheetDocument::read(&bytes).expect("it reads");
    let _ = applied(&document, 0, 0);
    let plain = odox_core::doc::Document::read(&bytes, odox_core::media_type::SPREADSHEET_ANY)
        .expect("it reads");
    assert!(document.document.content == plain.content);
}

/// The fixture's first table, to change in place.
fn table(document: &mut SheetDocument) -> &mut odox_core::Element {
    document
        .document
        .content
        .child_mut(&Ns::Office, "body")
        .and_then(|body| body.child_mut(&Ns::Office, "spreadsheet"))
        .and_then(|sheet| sheet.child_mut(&Ns::Table, "table"))
        .expect("a table")
}

/// The fixture with its conditional formats replaced.
fn with_formats(formats: &str) -> SheetDocument {
    let mut document = fixture();
    let source = format!(
        r#"<calcext:conditional-formats xmlns:calcext="urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0">{formats}</calcext:conditional-formats>"#
    );
    let parsed = odox_core::xml::parse(source.as_bytes(), "test").expect("well formed");
    let table = table(&mut document);
    table.children.retain(|node| {
        !matches!(node, odox_core::xml::Node::Element(e) if e.name.local.as_ref() == "conditional-formats")
    });
    table.children.push(odox_core::xml::Node::Element(parsed));
    document.reindex();
    document
}

fn format(range: &str, style: &str, value: &str) -> String {
    format!(
        r#"<calcext:conditional-format calcext:target-range-address="{range}"><calcext:condition calcext:apply-style-name="{style}" calcext:value="{value}" calcext:base-cell-address="Sheet1.A1"/></calcext:conditional-format>"#
    )
}

#[test]
fn a_sheet_name_with_a_space_keeps_its_whole_range() {
    let mut document = fixture();
    fn rename(element: &mut odox_core::Element) {
        for attribute in &mut element.attrs {
            attribute.value = attribute
                .value
                .replace("Sheet1.", "'My Sheet'.")
                .replace("Sheet1", "My Sheet");
        }
        for child in &mut element.children {
            if let odox_core::xml::Node::Element(child) = child {
                rename(child);
            }
        }
    }
    rename(table(&mut document));
    document.reindex();
    assert_eq!(document.sheets()[0].name, "My Sheet");
    assert_eq!(applied(&document, 0, 0), Some("Good"));
    assert_eq!(applied(&document, 1, 2), Some("Good"));
}

#[test]
fn cells_past_the_last_used_one_are_styled_too() {
    let document = with_formats(&format(
        "Sheet1.A1:Sheet1.E10",
        "Neutral",
        "formula-is([.$A$1]&gt;0)",
    ));
    assert_eq!(applied(&document, 9, 4), Some("Neutral"));
}

#[test]
fn text_is_not_equal_to_a_number() {
    let document = with_formats(&format("Sheet1.C1:Sheet1.C3", "Bad", "!=5"));
    assert_eq!(applied(&document, 0, 2), Some("Bad"));
    assert_eq!(applied(&document, 2, 2), Some("Bad"));
}

#[test]
fn an_empty_cell_is_nought_or_no_text() {
    let document = with_formats(
        &[
            format("Sheet1.B4:Sheet1.B4", "Bad", "&lt;50"),
            format("Sheet1.C4:Sheet1.C4", "Good", "=&quot;&quot;"),
            format("Sheet1.D1:Sheet1.D1", "Neutral", "between(0,1)"),
        ]
        .concat(),
    );
    assert_eq!(applied(&document, 3, 1), Some("Bad"));
    assert_eq!(applied(&document, 3, 2), Some("Good"));
    assert_eq!(applied(&document, 0, 3), Some("Neutral"));
}

#[test]
fn equal_allows_for_rounding() {
    let document = with_formats(&format(
        "Sheet1.A4:Sheet1.A4",
        "Good",
        "=24.000000000000004",
    ));
    assert_eq!(applied(&document, 3, 0), Some("Good"));
}

#[test]
fn a_column_too_long_to_count_is_no_range() {
    let document = with_formats(&format(
        "Sheet1.AAAAAAAAAAAAAAAAAAAAAAAAAAAAA1:Sheet1.A2",
        "Good",
        "&gt;0",
    ));
    assert_eq!(applied(&document, 0, 0), None);
}
