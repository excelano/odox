//! Cell comments, read from what LibreOffice writes and written back.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use odox_core::Ns;
use odox_core::doc::{Comment, SheetDocument, Value};

fn fixture() -> SheetDocument {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/comments.ods"),
    )
    .expect("the comments fixture");
    SheetDocument::read(&bytes).expect("it reads")
}

fn comment(document: &SheetDocument, row: usize, column: usize) -> Option<Comment> {
    let sheet = &document.sheets()[0];
    document.cell(sheet, row, column)?.comment()
}

#[test]
fn libreoffices_comments_are_read() {
    let document = fixture();
    assert_eq!(
        comment(&document, 0, 0),
        Some(Comment {
            author: Some("Ada Lovelace".to_owned()),
            date: Some("2026-11-05T09:30:00".to_owned()),
            text: "Checked against the invoice.\nSecond line.".to_owned(),
        })
    );
    let shown = comment(&document, 0, 1).expect("a comment");
    assert_eq!(shown.author, None);
    assert_eq!(shown.text, "Shown always");
    assert_eq!(comment(&document, 1, 0), None);
}

#[test]
fn a_comment_added_reads_back_after_a_save() {
    let mut document = fixture();
    document
        .set_comment(
            0,
            2,
            3,
            Some("First\nsecond  spaced"),
            Some("Grace Hopper"),
            "2026-11-06T01:00:00",
        )
        .expect("added");
    let bytes = document.document.write().expect("written");
    let document = SheetDocument::read(&bytes).expect("read back");
    assert_eq!(
        comment(&document, 2, 3),
        Some(Comment {
            author: Some("Grace Hopper".to_owned()),
            date: Some("2026-11-06T01:00:00".to_owned()),
            text: "First\nsecond  spaced".to_owned(),
        })
    );
}

#[test]
fn a_comment_changed_keeps_its_box_and_takes_the_new_author() {
    let mut document = fixture();
    document
        .set_comment(
            0,
            0,
            0,
            Some("Rechecked."),
            Some("Grace Hopper"),
            "2026-11-06T01:00:00",
        )
        .expect("changed");
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 0, 0).expect("the cell");
    let note = cell
        .element
        .child(&Ns::Office, "annotation")
        .expect("a note");
    assert!(note.attr(&Ns::Svg, "width").is_some(), "the box is kept");
    let changed = cell.comment().expect("a comment");
    assert_eq!(changed.author.as_deref(), Some("Grace Hopper"));
    assert_eq!(changed.text, "Rechecked.");
    assert_eq!(cell.value(), Value::Number(12.0), "the value is untouched");
}

#[test]
fn a_comment_taken_away_is_gone_and_a_value_typed_keeps_one() {
    let mut document = fixture();
    document
        .set_comment(0, 0, 1, None, None, "2026-11-06T01:00:00")
        .expect("removed");
    assert_eq!(comment(&document, 0, 1), None);
    document
        .set_cell(0, 0, 0, &Value::Number(13.0))
        .expect("typed");
    assert!(comment(&document, 0, 0).is_some());
}

#[test]
fn a_moment_is_written_as_a_comments_date() {
    assert_eq!(Comment::date_of(0), "1970-01-01T00:00:00");
    assert_eq!(Comment::date_of(1_793_955_600), "2026-11-06T09:00:00");
}
