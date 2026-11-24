//! Drawings on a sheet as LibreOffice writes them, in
//! `corpus/libreoffice/drawings.ods`: "Free" anchored to the sheet, "InCell"
//! in B1, "Spanning" from C2 to an end cell, "Hidden" in a hidden row, and a
//! picture in A6.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use odox_core::Length;
use odox_core::doc::{Anchor, SheetDocument};

fn fixture() -> SheetDocument {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/drawings.ods"),
    )
    .expect("the fixture");
    SheetDocument::read(&bytes).expect("it reads")
}

/// A drawing's place on the sheet by its name, in points.
fn placed(document: &SheetDocument, name: &str) -> Option<[f32; 4]> {
    let sheet = &document.sheets()[0];
    let drawing = document
        .drawings(0)
        .into_iter()
        .find(|d| d.element.attr(&odox_core::Ns::Draw, "name") == Some(name))
        .expect("the drawing");
    document.drawing_place(sheet, &drawing)
}

fn inches(n: f32) -> f32 {
    n * 72.0
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.05
}

#[test]
fn every_drawing_is_found_with_its_anchor() {
    let document = fixture();
    let anchors: Vec<(String, Anchor)> = document
        .drawings(0)
        .into_iter()
        .map(|d| {
            (
                d.element
                    .attr(&odox_core::Ns::Draw, "name")
                    .unwrap_or_default()
                    .to_owned(),
                d.anchor,
            )
        })
        .collect();
    let names: Vec<&str> = anchors.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["Free", "InCell", "Spanning", "Hidden", "Picture"]);
    assert_eq!(anchors[0].1, Anchor::Sheet);
    assert!(matches!(
        anchors[2].1,
        Anchor::Cell {
            row: 1,
            column: 2,
            end: Some(_)
        }
    ));
}

#[test]
fn sheet_anchored_shape_at_its_position() {
    let document = fixture();
    let [x, y, w, h] = placed(&document, "Free").expect("shown");
    assert!(
        close(x, inches(2.3622)) && close(y, inches(1.1811)),
        "{x} {y}"
    );
    assert!(
        close(w, inches(0.7874)) && close(h, inches(0.3937)),
        "{w} {h}"
    );
}

#[test]
fn cell_anchored_moves_with_resized_column() {
    let mut document = fixture();
    let [x, y, w, _] = placed(&document, "InCell").expect("shown");
    let free = placed(&document, "Free");
    let width = document.sheets()[0].column_width(0).expect("a width");
    document
        .set_column_width(0, 0, Length(width.points() + 20.0))
        .expect("resized");
    let [x2, y2, w2, _] = placed(&document, "InCell").expect("shown");
    assert!(
        close(x2, x + 20.0) && close(y2, y) && close(w2, w),
        "{x2} {y2} {w2}"
    );
    assert_eq!(placed(&document, "Free"), free);
}

#[test]
fn end_cell_anchor_resizes_with_rows() {
    let mut document = fixture();
    let [x, y, w, h] = placed(&document, "Spanning").expect("shown");
    let sheet = &document.sheets()[0];
    let height = document.row_height(sheet, 3).expect("a height");
    document
        .set_row_height(0, 3, Length(height.points() + 10.0))
        .expect("resized");
    let [x2, y2, w2, h2] = placed(&document, "Spanning").expect("shown");
    assert!(
        close(x2, x) && close(y2, y) && close(w2, w),
        "{x2} {y2} {w2}"
    );
    assert!(close(h2, h + 10.0), "{h} {h2}");
}

#[test]
fn hidden_row_hides_shape_anchored_in_it() {
    let document = fixture();
    assert_eq!(placed(&document, "Hidden"), None);
    assert!(placed(&document, "InCell").is_some());
}

#[test]
fn hiding_a_row_moves_the_shapes_below_it_up() {
    let mut document = fixture();
    let [_, y, ..] = placed(&document, "Picture").expect("shown");
    let sheet = &document.sheets()[0];
    let height = document.row_height(sheet, 1).expect("a height").points();
    document.set_rows_shown(0, 1..=1, false).expect("hidden");
    let [_, y2, ..] = placed(&document, "Picture").expect("shown");
    assert!(close(y2, y - height), "{y} {y2}");
}

#[test]
fn a_drawing_stands_in_a_cell_and_so_far_into_it() {
    let document = fixture();
    let sheet = &document.sheets()[0];
    let drawings = document.drawings(0);
    let cell = |name: &str| {
        let drawing = drawings
            .iter()
            .find(|d| d.element.attr(&odox_core::Ns::Draw, "name") == Some(name))
            .expect("the drawing");
        document.drawing_cell(sheet, drawing)
    };
    let (row, column, (x, y)) = cell("Free").expect("shown");
    let width = sheet.column_width(0).expect("a width").points();
    assert_eq!(column, 2);
    assert!(close(x, inches(2.3622) - 2.0 * width), "{x}");
    assert!(row > 0 && y >= 0.0, "{row} {y}");
    let (row, column, _) = cell("InCell").expect("shown");
    assert_eq!((row, column), (0, 1));
    assert_eq!(cell("Hidden"), None);
}
