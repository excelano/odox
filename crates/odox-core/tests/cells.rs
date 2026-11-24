//! Merging and splitting a table's cells, and its columns' widths.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::edit::{equal_widths, merge_cells, set_column_width, split_cell, text};
use odox_core::{Element, Family, Length, Ns, Refused, Styles, xml};

const NAMESPACES: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0""#;

const TABLE: [usize; 3] = [1, 0, 0];

/// A table of rows of cells, each cell holding a paragraph of its text, and
/// three columns of 2 cm, 3 cm and 5 cm.
fn table(rows: &[&[&str]]) -> Element {
    let body: String = rows
        .iter()
        .map(|row| {
            let cells: String = row
                .iter()
                .map(|cell| format!("<table:table-cell><text:p>{cell}</text:p></table:table-cell>"))
                .collect();
            format!("<table:table-row>{cells}</table:table-row>")
        })
        .collect();
    let source = format!(
        r#"<office:document-content {NAMESPACES}><office:automatic-styles><style:style style:name="C2" style:family="table-column"><style:table-column-properties style:column-width="2cm"/></style:style><style:style style:name="C3" style:family="table-column"><style:table-column-properties style:column-width="3cm"/></style:style><style:style style:name="C5" style:family="table-column"><style:table-column-properties style:column-width="5cm"/></style:style></office:automatic-styles><office:body><office:text><table:table table:name="T"><table:table-column table:style-name="C2"/><table:table-column table:style-name="C3"/><table:table-column table:style-name="C5"/>{body}</table:table></office:text></office:body></office:document-content>"#
    );
    xml::parse(source.as_bytes(), "test").expect("a content root")
}

/// Each row as its cells: the text of a cell, `^` for a covered one, and a
/// cell's spans after it as `[rows x columns]` where it has any.
fn grid(content: &Element) -> Vec<String> {
    content
        .at(&TABLE)
        .expect("the table")
        .elements()
        .filter(|e| e.is(&Ns::Table, "table-row"))
        .map(|row| {
            row.elements()
                .map(|cell| {
                    if cell.is(&Ns::Table, "covered-table-cell") {
                        return "^".to_owned();
                    }
                    let paragraphs: Vec<String> = cell.elements().map(text).collect();
                    let spans = match (
                        cell.attr_usize(&Ns::Table, "number-rows-spanned"),
                        cell.attr_usize(&Ns::Table, "number-columns-spanned"),
                    ) {
                        (None, None) => String::new(),
                        (r, c) => format!("[{}x{}]", r.unwrap_or(1), c.unwrap_or(1)),
                    };
                    format!("{}{spans}", paragraphs.join("/"))
                })
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .collect()
}

#[test]
fn merge_rectangle_joins_paragraphs_into_first() {
    let mut root = table(&[&["a", "b", "c"], &["d", "e", ""], &["g", "h", "i"]]);
    merge_cells(&mut root, &TABLE, 0..2, 1..3).expect("merged");
    assert_eq!(
        grid(&root),
        ["a | b/c/e[2x2] | ^", "d | ^ | ^", "g | h | i"]
    );
}

#[test]
fn merge_refuses_partial_overlap_with_merged_cell() {
    let mut root = table(&[&["a", "b", "c"], &["d", "e", "f"]]);
    merge_cells(&mut root, &TABLE, 0..2, 0..2).expect("merged");
    let before = root.clone();
    assert_eq!(
        merge_cells(&mut root, &TABLE, 0..1, 1..3),
        Err(Refused::Structure)
    );
    assert!(root == before);
}

#[test]
fn split_restores_empty_covered_cells() {
    let mut root = table(&[&["a", "b", "c"], &["d", "e", "f"]]);
    merge_cells(&mut root, &TABLE, 0..2, 0..2).expect("merged");
    split_cell(&mut root, &[1, 0, 0, 3, 0]).expect("split");
    assert_eq!(grid(&root), ["a/b/d/e |  | c", " |  | f"]);
}

fn widths(content: &Element, styles: &Styles) -> Vec<f32> {
    content
        .at(&TABLE)
        .expect("the table")
        .elements()
        .filter(|e| e.is(&Ns::Table, "table-column"))
        .map(|column| {
            let name = column.attr(&Ns::Table, "style-name").expect("a style");
            styles
                .resolve(&Family::TableColumn, name)
                .column_width
                .map_or(0.0, Length::points)
        })
        .collect()
}

fn cm(cm: f32) -> f32 {
    cm / 2.54 * 72.0
}

#[test]
fn width_written_to_column_style_and_rel_width() {
    let mut root = table(&[&["a", "b", "c"]]);
    let mut styles = Styles::collect(Some(&root), None);
    set_column_width(&mut root, &TABLE, 0, cm(4.0), &mut styles).expect("wider");
    let got = widths(&root, &styles);
    assert!(
        (got[0] - cm(4.0)).abs() < 0.1 && (got[1] - cm(1.0)).abs() < 0.1,
        "{got:?}"
    );
    assert!((got[2] - cm(5.0)).abs() < 0.1, "{got:?}");
    let written = String::from_utf8(xml::serialize(&root)).expect("UTF-8");
    assert!(written.contains("style:rel-column-width"), "{written}");
}

#[test]
fn equal_widths_sum_to_table_width() {
    let mut root = table(&[&["a", "b", "c"]]);
    let mut styles = Styles::collect(Some(&root), None);
    equal_widths(&mut root, &TABLE, &mut styles).expect("even");
    let got = widths(&root, &styles);
    assert!(
        got.iter().all(|w| (w - cm(10.0 / 3.0)).abs() < 0.1),
        "{got:?}"
    );
    assert!((got.iter().sum::<f32>() - cm(10.0)).abs() < 0.1);
}
