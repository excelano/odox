//! The spreadsheet index, measured on documents built here.
//!
//! Written from XML rather than from a corpus because the machine this was
//! written on has no Calc to produce one, and because the cases that matter are
//! the ones a corpus makes hard to see: a row written once and repeated a
//! thousand times, a column run covering the rest of the sheet, rows inside a
//! header band, a cell spanned over by its neighbour.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::io::Write;

use odox_core::doc::{SheetDocument, Value};
use odox_core::media_type;

/// Build a spreadsheet package around a body.
fn spreadsheet(body: &str) -> Vec<u8> {
    let content = format!(
        r##"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content
 xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
 xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
 xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
 xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"
 xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"
 office:version="1.3">
<office:automatic-styles>
 <style:style style:name="co1" style:family="table-column">
  <style:table-column-properties style:column-width="2.5cm"/>
 </style:style>
 <style:style style:name="ro1" style:family="table-row">
  <style:table-row-properties style:row-height="0.5cm"/>
 </style:style>
 <style:style style:name="ce1" style:family="table-cell">
  <style:table-cell-properties fo:background-color="#ffff00"/>
 </style:style>
</office:automatic-styles>
<office:body><office:spreadsheet>{body}</office:spreadsheet></office:body>
</office:document-content>"##
    );

    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("mimetype", stored).unwrap();
    writer
        .write_all(media_type::SPREADSHEET.as_bytes())
        .unwrap();
    writer
        .start_file(
            "content.xml",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
    writer.write_all(content.as_bytes()).unwrap();
    writer.finish().unwrap().into_inner()
}

#[test]
fn a_repeated_row_is_not_expanded_and_still_answers_for_every_row_it_covers() {
    let bytes = spreadsheet(
        r#"<table:table table:name="Sheet1">
 <table:table-column table:style-name="co1" table:number-columns-repeated="16384"/>
 <table:table-row table:style-name="ro1">
  <table:table-cell office:value-type="string"><text:p>top</text:p></table:table-cell>
 </table:table-row>
 <table:table-row table:number-rows-repeated="1000">
  <table:table-cell office:value-type="float" office:value="7"><text:p>7</text:p></table:table-cell>
 </table:table-row>
</table:table>"#,
    );
    let document = SheetDocument::read(&bytes).expect("a readable spreadsheet");
    let sheet = &document.sheets()[0];

    assert_eq!(sheet.name, "Sheet1");
    assert_eq!(sheet.used_rows, 1001);
    assert_eq!(sheet.used_columns, 1);

    // Any row in the run gives the same cell, and the run was stored once.
    for row in [1usize, 2, 500, 1000] {
        let cell = document
            .cell(sheet, row, 0)
            .expect("a cell in the repeated run");
        assert_eq!(cell.value(), Value::Number(7.0), "row {row}");
    }
    assert!(
        document.cell(sheet, 1001, 0).is_none(),
        "a row past the end"
    );
    assert_eq!(
        document.cell(sheet, 0, 0).expect("the first row").value(),
        Value::Text("top".to_owned())
    );
}

#[test]
fn a_column_run_covering_the_sheet_is_capped_and_carries_its_width() {
    let bytes = spreadsheet(
        r#"<table:table table:name="Wide">
 <table:table-column table:style-name="co1" table:number-columns-repeated="16384"/>
 <table:table-row><table:table-cell office:value-type="string"><text:p>a</text:p></table:table-cell></table:table-row>
</table:table>"#,
    );
    let document = SheetDocument::read(&bytes).expect("a readable spreadsheet");
    let sheet = &document.sheets()[0];

    assert_eq!(
        sheet.columns.len(),
        16_384,
        "the column run was not capped at ODF's limit"
    );
    let width = sheet.column_width(0).expect("a column width");
    assert!(
        (width.points() - 70.87).abs() < 0.1,
        "2.5cm is {} points",
        width.points()
    );
    assert_eq!(
        sheet.column_width(16_383).map(|w| w.points()),
        Some(width.points())
    );
    assert_eq!(sheet.column_width(16_384), None, "past ODF's last column");
}

#[test]
fn repeated_cells_are_stepped_over_and_spans_are_reported() {
    let bytes = spreadsheet(
        r#"<table:table table:name="Sheet1">
 <table:table-row>
  <table:table-cell table:number-columns-repeated="3"/>
  <table:table-cell table:number-columns-spanned="2" table:number-rows-spanned="1"
   office:value-type="string"><text:p>wide</text:p></table:table-cell>
  <table:covered-table-cell/>
  <table:table-cell office:value-type="float" office:value="1.5"><text:p>1.5</text:p></table:table-cell>
 </table:table-row>
</table:table>"#,
    );
    let document = SheetDocument::read(&bytes).expect("a readable spreadsheet");
    let sheet = &document.sheets()[0];

    // Three empty cells, then the spanning one at column three.
    for column in 0..3 {
        let cell = document.cell(sheet, 0, column).expect("an empty cell");
        assert_eq!(cell.value(), Value::Empty, "column {column}");
    }
    let wide = document.cell(sheet, 0, 3).expect("the spanning cell");
    assert_eq!(wide.value(), Value::Text("wide".to_owned()));
    assert_eq!(wide.columns_spanned(), 2);
    assert_eq!(wide.rows_spanned(), 1);
    assert!(!wide.covered);

    let covered = document.cell(sheet, 0, 4).expect("the covered cell");
    assert!(
        covered.covered,
        "the cell under a span is not marked covered"
    );

    assert_eq!(
        document.cell(sheet, 0, 5).expect("the last cell").value(),
        Value::Number(1.5)
    );
    assert_eq!(sheet.used_columns, 6);
}

#[test]
fn rows_inside_a_header_band_or_a_group_are_part_of_the_sheet() {
    let bytes = spreadsheet(
        r#"<table:table table:name="Grouped">
 <table:table-header-rows>
  <table:table-row><table:table-cell office:value-type="string"><text:p>header</text:p></table:table-cell></table:table-row>
 </table:table-header-rows>
 <table:table-row-group>
  <table:table-row><table:table-cell office:value-type="string"><text:p>grouped</text:p></table:table-cell></table:table-row>
  <table:table-row-group>
   <table:table-row><table:table-cell office:value-type="string"><text:p>deeper</text:p></table:table-cell></table:table-row>
  </table:table-row-group>
 </table:table-row-group>
 <table:table-row><table:table-cell office:value-type="string"><text:p>plain</text:p></table:table-cell></table:table-row>
</table:table>"#,
    );
    let document = SheetDocument::read(&bytes).expect("a readable spreadsheet");
    let sheet = &document.sheets()[0];

    assert_eq!(sheet.used_rows, 4, "a nested row was not counted");
    let text = |row| {
        document
            .cell(sheet, row, 0)
            .map(|c| c.text())
            .unwrap_or_default()
    };
    assert_eq!(text(0), "header");
    assert_eq!(text(1), "grouped");
    assert_eq!(text(2), "deeper", "a row two groups deep was not reachable");
    assert_eq!(text(3), "plain");
}

#[test]
fn a_formula_loses_its_language_prefix_and_keeps_its_cached_value() {
    let bytes = spreadsheet(
        r#"<table:table table:name="Sheet1">
 <table:table-row>
  <table:table-cell table:formula="of:=SUM([.A1:.A9])" office:value-type="float"
   office:value="42.5"><text:p>42.50 &#8364;</text:p></table:table-cell>
 </table:table-row>
</table:table>"#,
    );
    let document = SheetDocument::read(&bytes).expect("a readable spreadsheet");
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 0, 0).expect("the cell");

    assert_eq!(cell.formula(), Some("SUM([.A1:.A9])"));
    assert_eq!(cell.value(), Value::Number(42.5));
    // The formatted text the producer cached is what a viewer shows, which is
    // why no number format is resolved and no formula is evaluated.
    assert_eq!(cell.text(), "42.50 \u{20ac}");
    assert!(cell.value().is_numeric());
}

#[test]
fn typed_values_read_as_their_types() {
    let bytes = spreadsheet(
        r#"<table:table table:name="Types">
 <table:table-row>
  <table:table-cell office:value-type="percentage" office:value="0.15"><text:p>15%</text:p></table:table-cell>
  <table:table-cell office:value-type="currency" office:currency="EUR" office:value="9.99"><text:p>9,99 EUR</text:p></table:table-cell>
  <table:table-cell office:value-type="date" office:date-value="2026-09-11"><text:p>11/09/2026</text:p></table:table-cell>
  <table:table-cell office:value-type="time" office:time-value="PT01H30M00S"><text:p>01:30</text:p></table:table-cell>
  <table:table-cell office:value-type="boolean" office:boolean-value="true"><text:p>TRUE</text:p></table:table-cell>
  <table:table-cell office:value-type="string"><text:p>plain</text:p></table:table-cell>
 </table:table-row>
</table:table>"#,
    );
    let document = SheetDocument::read(&bytes).expect("a readable spreadsheet");
    let sheet = &document.sheets()[0];
    let value = |column| document.cell(sheet, 0, column).expect("a cell").value();

    assert_eq!(value(0), Value::Percentage(0.15));
    assert_eq!(value(1), Value::Currency(9.99, Some("EUR".to_owned())));
    assert_eq!(value(2), Value::Date("2026-09-11".to_owned()));
    assert_eq!(value(3), Value::Time("PT01H30M00S".to_owned()));
    assert_eq!(value(4), Value::Boolean(true));
    assert_eq!(value(5), Value::Text("plain".to_owned()));
    assert!(
        !value(5).is_numeric(),
        "text is the one type that is not right-aligned"
    );
}

#[test]
fn a_cell_takes_its_column_default_style_when_it_names_none() {
    let bytes = spreadsheet(
        r#"<table:table table:name="Sheet1">
 <table:table-column table:style-name="co1" table:default-cell-style-name="ce1"/>
 <table:table-row>
  <table:table-cell office:value-type="string"><text:p>inherits</text:p></table:table-cell>
 </table:table-row>
</table:table>"#,
    );
    let document = SheetDocument::read(&bytes).expect("a readable spreadsheet");
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 0, 0).expect("the cell");

    assert!(
        cell.style_name().is_none(),
        "the cell named a style of its own"
    );
    let style = document.cell_style(sheet, Some(&cell), 0);
    assert_eq!(
        style.cell.background,
        Some(odox_core::Color {
            r: 0xff,
            g: 0xff,
            b: 0x00
        }),
        "the column's default cell style was not applied"
    );
}

#[test]
fn the_wrong_format_is_named_rather_than_refused_silently() {
    let bytes = spreadsheet(r#"<table:table table:name="Sheet1"/>"#);
    let error = odox_core::doc::TextDocument::read(&bytes)
        .err()
        .expect("a spreadsheet is not a text document");
    let message = error.to_string();
    assert!(message.contains("spreadsheet"), "{message}");
    assert!(message.contains("text"), "{message}");
}

/// Every cell of a sheet's used range as (row, column, value), for saying that
/// an edit changed one cell and no other.
fn snapshot(document: &SheetDocument, sheet: usize) -> Vec<(usize, usize, Value)> {
    let sheet = &document.sheets()[sheet];
    let mut out = Vec::new();
    for row in 0..sheet.used_rows {
        for column in 0..sheet.used_columns {
            let value = document
                .cell(sheet, row, column)
                .map_or(Value::Empty, |c| c.value());
            out.push((row, column, value));
        }
    }
    out
}

#[test]
fn a_cell_in_a_repeated_row_changes_alone() {
    let bytes = spreadsheet(
        r#"<table:table table:name="Sheet1">
 <table:table-column table:style-name="co1" table:number-columns-repeated="16384"/>
 <table:table-row table:style-name="ro1">
  <table:table-cell office:value-type="string"><text:p>top</text:p></table:table-cell>
 </table:table-row>
 <table:table-row table:number-rows-repeated="1000" table:style-name="ro1">
  <table:table-cell office:value-type="float" office:value="7" table:style-name="ce1"><text:p>7</text:p></table:table-cell>
 </table:table-row>
</table:table>"#,
    );
    let mut document = SheetDocument::read(&bytes).expect("a readable spreadsheet");
    let before = snapshot(&document, 0);

    document
        .set_cell(0, 500, 0, &Value::Number(99.0))
        .expect("a writable cell");

    let after = snapshot(&document, 0);
    assert_eq!(before.len(), after.len(), "the used range changed");
    for ((row, column, was), (_, _, is)) in before.iter().zip(&after) {
        if (*row, *column) == (500, 0) {
            assert_eq!(*is, Value::Number(99.0));
        } else {
            assert_eq!(was, is, "row {row} column {column} changed");
        }
    }
    // The run's style survives on the cell that was split out of it, and the
    // rows either side are still one run each.
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 500, 0).expect("the cell");
    assert_eq!(cell.style_name(), Some("ce1"));
    assert_eq!(cell.text(), "99");
    let rows = document
        .document
        .body_of("spreadsheet")
        .and_then(|b| b.child(&odox_core::Ns::Table, "table"))
        .map(|t| {
            t.elements()
                .filter(|e| e.is(&odox_core::Ns::Table, "table-row"))
                .count()
        });
    assert_eq!(rows, Some(4), "top, 499 before, the one, 500 after");

    // And it all reads back.
    let written = document.document.write_verified().expect("saves");
    let again = SheetDocument::read(&written).expect("readable");
    assert_eq!(snapshot(&again, 0), after);
}

#[test]
fn a_cell_in_a_repeated_run_and_past_the_end_are_both_reachable() {
    let bytes = spreadsheet(
        r#"<table:table table:name="Sheet1">
 <table:table-column table:style-name="co1" table:number-columns-repeated="4"/>
 <table:table-row>
  <table:table-cell office:value-type="string" table:number-columns-repeated="3"><text:p>same</text:p></table:table-cell>
 </table:table-row>
</table:table>"#,
    );
    let mut document = SheetDocument::read(&bytes).expect("a readable spreadsheet");

    // Inside the run.
    document
        .set_cell(0, 0, 1, &Value::Text("middle".to_owned()))
        .expect("a writable cell");
    let sheet = &document.sheets()[0];
    assert_eq!(document.cell(sheet, 0, 0).expect("left").text(), "same");
    assert_eq!(document.cell(sheet, 0, 1).expect("middle").text(), "middle");
    assert_eq!(document.cell(sheet, 0, 2).expect("right").text(), "same");

    // Past every cell in the row, and past every row in the sheet.
    document
        .set_cell(0, 0, 6, &Value::Boolean(true))
        .expect("a cell past the row's end");
    document
        .set_cell(0, 4, 2, &Value::Number(1.5))
        .expect("a cell past the sheet's end");
    let sheet = &document.sheets()[0];
    assert_eq!(sheet.used_rows, 5);
    assert_eq!(sheet.used_columns, 7);
    assert_eq!(
        document.cell(sheet, 0, 6).expect("the far cell").value(),
        Value::Boolean(true)
    );
    assert_eq!(
        document.cell(sheet, 0, 6).expect("the far cell").text(),
        "TRUE"
    );
    assert!(
        document.cell(sheet, 0, 4).expect("the gap").value() == Value::Empty,
        "the gap is empty"
    );
    assert_eq!(
        document.cell(sheet, 4, 2).expect("the low cell").value(),
        Value::Number(1.5)
    );
    assert!(
        document
            .cell(sheet, 2, 0)
            .is_none_or(|c| c.value() == Value::Empty)
    );
    document.document.write_verified().expect("saves");
}

#[test]
fn a_covered_cell_and_a_formula_are_refused_and_nothing_moves() {
    let bytes = spreadsheet(
        r#"<table:table table:name="Sheet1">
 <table:table-row>
  <table:table-cell office:value-type="string" table:number-columns-spanned="2"><text:p>wide</text:p></table:table-cell>
  <table:covered-table-cell/>
  <table:table-cell table:formula="of:=SUM([.A1:.B1])" office:value-type="float" office:value="0"><text:p>0</text:p></table:table-cell>
 </table:table-row>
</table:table>"#,
    );
    let mut document = SheetDocument::read(&bytes).expect("a readable spreadsheet");
    let before = document.document.content.clone();
    assert_eq!(
        document.set_cell(0, 0, 1, &Value::Number(1.0)),
        Err(odox_core::Refused::Covered)
    );
    assert_eq!(
        document.set_cell(0, 0, 2, &Value::Number(1.0)),
        Err(odox_core::Refused::Formula)
    );
    assert_eq!(
        document.set_cell(3, 0, 0, &Value::Number(1.0)),
        Err(odox_core::Refused::NotFound)
    );
    assert_eq!(document.document.content, before);
}

#[test]
fn a_value_replaces_what_the_cell_held_and_keeps_its_style() {
    let bytes = spreadsheet(
        r#"<table:table table:name="Sheet1">
 <table:table-row>
  <table:table-cell office:value-type="date" office:date-value="2026-09-21" table:style-name="ce1"><text:p>21/09/2026</text:p><text:p>second line</text:p></table:table-cell>
 </table:table-row>
</table:table>"#,
    );
    let mut document = SheetDocument::read(&bytes).expect("a readable spreadsheet");
    document
        .set_cell(0, 0, 0, &Value::Text("two\nlines".to_owned()))
        .expect("writable");
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 0, 0).expect("the cell");
    assert_eq!(cell.value(), Value::Text("two\nlines".to_owned()));
    assert_eq!(cell.text(), "two\nlines");
    assert_eq!(cell.style_name(), Some("ce1"));
    assert!(
        cell.element
            .attr(&odox_core::Ns::Office, "date-value")
            .is_none()
    );
    assert_eq!(
        cell.element
            .elements()
            .filter(|e| e.is(&odox_core::Ns::Text, "p"))
            .count(),
        2
    );

    document.set_cell(0, 0, 0, &Value::Empty).expect("writable");
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 0, 0).expect("the cell");
    assert_eq!(cell.value(), Value::Empty);
    assert_eq!(
        cell.style_name(),
        Some("ce1"),
        "an emptied cell keeps its style"
    );
    assert_eq!(sheet.used_rows, 0, "an emptied sheet has no used rows");
}

#[test]
fn input_reads_the_way_a_spreadsheet_reads_it() {
    assert_eq!(Value::from_input("12"), Value::Number(12.0));
    assert_eq!(Value::from_input(" -0.5 "), Value::Number(-0.5));
    assert_eq!(Value::from_input("1e3"), Value::Number(1000.0));
    assert_eq!(Value::from_input("TRUE"), Value::Boolean(true));
    assert_eq!(Value::from_input("false"), Value::Boolean(false));
    assert_eq!(Value::from_input(""), Value::Empty);
    assert_eq!(Value::from_input("   "), Value::Empty);
    assert_eq!(Value::from_input("inf"), Value::Text("inf".to_owned()));
    assert_eq!(Value::from_input("NaN"), Value::Text("NaN".to_owned()));
    assert_eq!(
        Value::from_input("=SUM(A1)"),
        Value::Text("=SUM(A1)".to_owned())
    );
    assert_eq!(
        Value::from_input("12 apples"),
        Value::Text("12 apples".to_owned())
    );
    assert_eq!(Value::Number(12.0).cached_text(), "12");
    assert_eq!(Value::Number(0.1).cached_text(), "0.1");
}

#[test]
fn the_corpus_spreadsheet_takes_a_value_in_the_prefixes_it_declares() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/calc.ods");
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let mut document = SheetDocument::read(&bytes).expect("a readable spreadsheet");
    document
        .set_cell(0, 1, 1, &Value::Number(99.0))
        .expect("writable");
    let written = document.document.write_verified().expect("saves");
    let text = String::from_utf8(
        odox_core::Package::read(&written)
            .expect("package")
            .part("content.xml")
            .expect("content")
            .data
            .clone(),
    )
    .expect("utf-8");
    assert!(
        text.contains(r#"office:value-type="float" calcext:value-type="float" office:value="99""#)
            || text.contains(r#"office:value-type="float" office:value="99""#),
        "{text}"
    );
    assert!(
        text.contains("calcext:value-type=\"float\""),
        "LibreOffice's own type copy is written where the document declares calcext"
    );
}
