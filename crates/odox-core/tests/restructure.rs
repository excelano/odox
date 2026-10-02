//! Inserting and deleting rows and columns: where every cell and formula goes,
//! and what is refused.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use odox_core::doc::{Blocked, Restructure, SheetDocument};

fn open(name: &str) -> SheetDocument {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/libreoffice")
            .join(name),
    )
    .expect("the corpus sheet");
    SheetDocument::read(&bytes).expect("it reads")
}

/// What every cell in the first rows and columns says, and the formula in it.
fn grid(
    document: &SheetDocument,
    rows: usize,
    columns: usize,
) -> Vec<Vec<(String, Option<String>)>> {
    let sheet = &document.sheets()[0];
    (0..rows)
        .map(|row| {
            (0..columns)
                .map(|column| {
                    document
                        .cell(sheet, row, column)
                        .map_or((String::new(), None), |cell| {
                            (cell.text(), cell.formula().map(ToOwned::to_owned))
                        })
                })
                .collect()
        })
        .collect()
}

#[test]
fn rows_inserted_into_a_sheet_with_gaps_push_everything_below_and_leave_the_rest() {
    let mut document = open("gaps.ods");
    let before = grid(&document, 12, 6);
    document
        .restructure(0, Restructure::InsertRows { at: 2, count: 3 })
        .expect("inserted");
    let after = grid(&document, 15, 6);
    assert_eq!(&after[..2], &before[..2], "above the line nothing moves");
    for (row, cells) in after.iter().enumerate().take(5).skip(2) {
        assert!(
            cells
                .iter()
                .all(|(text, formula)| text.is_empty() && formula.is_none()),
            "row {row} is new and empty"
        );
    }
    assert_eq!(
        &after[5..15],
        &before[2..12],
        "below it everything is three rows down"
    );
}

#[test]
fn deleting_what_was_inserted_gives_the_sheet_back() {
    let mut document = open("gaps.ods");
    let before = grid(&document, 12, 6);
    document
        .restructure(0, Restructure::InsertRows { at: 1, count: 2 })
        .expect("in");
    document
        .restructure(0, Restructure::DeleteRows { at: 1, count: 2 })
        .expect("out");
    assert_eq!(grid(&document, 12, 6), before);
    document
        .restructure(0, Restructure::InsertColumns { at: 1, count: 2 })
        .expect("in");
    document
        .restructure(0, Restructure::DeleteColumns { at: 1, count: 2 })
        .expect("out");
    assert_eq!(grid(&document, 12, 6), before);
}

#[test]
fn columns_inserted_push_everything_to_their_right() {
    let mut document = open("calc.ods");
    let before = grid(&document, 6, 7);
    document
        .restructure(0, Restructure::InsertColumns { at: 1, count: 1 })
        .expect("inserted");
    let after = grid(&document, 6, 8);
    for row in 0..6 {
        assert_eq!(
            after[row][0].0, before[row][0].0,
            "column A is where it was"
        );
        assert!(after[row][1].0.is_empty(), "the new column B is empty");
        for column in 1..7 {
            assert_eq!(
                after[row][column + 1].0,
                before[row][column].0,
                "row {row} column {column}"
            );
        }
    }
}

#[test]
fn formulas_follow_the_cells_they_name() {
    let mut document = open("calc.ods");
    let before = grid(&document, 6, 7);
    let formula = |g: &Vec<Vec<(String, Option<String>)>>, row: usize, column: usize| {
        g[row][column].1.clone().unwrap_or_default()
    };
    assert_eq!(formula(&before, 1, 3), "[.B2]*[.C2]");
    assert_eq!(formula(&before, 4, 3), "SUM([.D2:.D4])");

    document
        .restructure(0, Restructure::InsertRows { at: 1, count: 1 })
        .expect("inserted");
    let after = grid(&document, 7, 7);
    assert_eq!(
        formula(&after, 2, 3),
        "[.B3]*[.C3]",
        "the row moved and its formula with it"
    );
    assert_eq!(
        formula(&after, 5, 3),
        "SUM([.D3:.D5])",
        "an insert at the first row of a range moves the range"
    );

    // An insert inside the range grows it.
    let mut document = open("calc.ods");
    document
        .restructure(0, Restructure::InsertRows { at: 3, count: 1 })
        .expect("inserted");
    let after = grid(&document, 7, 7);
    assert_eq!(formula(&after, 5, 3), "SUM([.D2:.D5])");
}

#[test]
fn a_delete_shrinks_the_range_over_it_and_takes_the_deleted_formulas_with_it() {
    let mut document = open("calc.ods");
    document
        .restructure(0, Restructure::DeleteRows { at: 2, count: 1 })
        .expect("deleted");
    let after = grid(&document, 5, 7);
    assert_eq!(
        after[2][0].0, "Washer",
        "the next row is where the deleted one was"
    );
    assert_eq!(after[3][3].1.as_deref(), Some("SUM([.D2:.D3])"));
}

#[test]
fn a_delete_that_a_surviving_formula_points_straight_at_is_refused_and_changes_nothing() {
    let mut document = open("calc.ods");
    let before = grid(&document, 6, 7);
    // The totals in column D multiply column B, row by row, so deleting B
    // leaves formulas pointing at cells that are gone.
    let result = document.restructure(0, Restructure::DeleteColumns { at: 1, count: 1 });
    assert!(matches!(result, Err(Blocked::Deleted(_))), "{result:?}");
    assert_eq!(grid(&document, 6, 7), before, "and nothing changed");
}

#[test]
fn a_formula_deleted_with_its_row_is_not_asked_about() {
    let mut document = open("calc.ods");
    // Row 3 holds `[.B3]*[.C3]`, which names cells in its own row.
    document
        .restructure(0, Restructure::DeleteRows { at: 2, count: 1 })
        .expect("the formula goes with its row");
}

#[test]
fn a_line_through_a_merged_region_is_refused() {
    let mut document = open("sheet.ods");
    let sheet = &document.sheets()[0];
    let mut found = None;
    for row in 0..sheet.used_rows {
        for column in 0..sheet.used_columns {
            if let Some(cell) = document.cell(sheet, row, column) {
                if cell.rows_spanned() > 1 {
                    found = Some(Restructure::InsertRows {
                        at: row + 1,
                        count: 1,
                    });
                } else if cell.columns_spanned() > 1 {
                    found = Some(Restructure::InsertColumns {
                        at: column + 1,
                        count: 1,
                    });
                }
            }
        }
    }
    let change = found.expect("the sheet has a merged cell");
    assert_eq!(document.restructure(0, change), Err(Blocked::Merged));
}

#[test]
fn named_ranges_move_with_the_cells() {
    let mut document = open("sheet.ods");
    document
        .restructure(0, Restructure::InsertRows { at: 0, count: 2 })
        .expect("inserted");
    let written =
        String::from_utf8(odox_core::xml::serialize(&document.document.content)).expect("UTF-8");
    assert!(
        written.contains(r#"table:cell-range-address="$Sheet1.$A$3:.$F$8""#),
        "{written}"
    );
}

#[test]
fn a_sheet_that_has_had_rows_and_columns_changed_saves_and_reads_back() {
    let mut document = open("calc.ods");
    document
        .restructure(0, Restructure::InsertRows { at: 2, count: 2 })
        .expect("rows");
    document
        .restructure(0, Restructure::InsertColumns { at: 0, count: 1 })
        .expect("columns");
    document
        .restructure(0, Restructure::DeleteRows { at: 2, count: 1 })
        .expect("fewer rows");
    let written = document
        .document
        .write_verified()
        .expect("it comes back the same");
    let again = SheetDocument::read(&written).expect("it reads again");
    assert_eq!(grid(&again, 8, 8), grid(&document, 8, 8));
}

#[test]
fn nothing_is_asked_of_a_sheet_that_is_not_there() {
    let mut document = open("calc.ods");
    assert_eq!(
        document.restructure(9, Restructure::InsertRows { at: 0, count: 1 }),
        Err(Blocked::NotFound)
    );
    assert_eq!(
        document.restructure(0, Restructure::InsertRows { at: 0, count: 0 }),
        Err(Blocked::NotFound)
    );
}
