//! Tracked changes as LibreOffice records them: read, accepted and rejected.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use odox_core::doc::TextDocument;
use odox_core::edit::{
    ChangeKind, accept, accept_all, changes, reject, reject_all, text, tracking,
};
use odox_core::{Element, Ns, xml};

fn fixture() -> TextDocument {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/tracked.odt"),
    )
    .expect("the tracked fixture");
    TextDocument::read(&bytes).expect("it reads")
}

/// The body's paragraphs' text, in order.
fn paragraphs(document: &TextDocument) -> Vec<String> {
    document
        .body()
        .expect("a body")
        .elements()
        .filter(|e| e.is(&Ns::Text, "p"))
        .map(text)
        .collect()
}

fn id_of(document: &TextDocument, nth: usize) -> String {
    changes(&document.document.content)[nth].id.clone()
}

fn content(document: &mut TextDocument) -> &mut Element {
    &mut document.document.content
}

#[test]
fn reads_regions_with_author_date() {
    let document = fixture();
    let found = changes(&document.document.content);
    let kinds: Vec<ChangeKind> = found.iter().map(|c| c.kind).collect();
    assert_eq!(
        kinds,
        [
            ChangeKind::Insertion,
            ChangeKind::Deletion,
            ChangeKind::Deletion,
            ChangeKind::Format
        ]
    );
    assert_eq!(found[0].author.as_deref(), Some("Ada Reviewer"));
    assert_eq!(found[1].date.as_deref(), Some("2026-03-15T10:00:00"));
    assert_eq!(found[1].deleted.as_deref(), Some("very "));
}

#[test]
fn accept_insertion_drops_marks_keeps_text() {
    let mut document = fixture();
    let id = id_of(&document, 0);
    accept(content(&mut document), &id).expect("accepted");
    assert_eq!(paragraphs(&document)[0], "Some newly added words.");
    assert_eq!(changes(&document.document.content).len(), 3);
    let written = String::from_utf8(xml::serialize(&document.document.content)).expect("UTF-8");
    assert!(!written.contains(&id), "{written}");
}

#[test]
fn reject_insertion_removes_text() {
    let mut document = fixture();
    let id = id_of(&document, 0);
    reject(content(&mut document), &id).expect("rejected");
    assert_eq!(paragraphs(&document)[0], "Some words.");
}

#[test]
fn accept_deletion_drops_mark() {
    let mut document = fixture();
    let id = id_of(&document, 1);
    accept(content(&mut document), &id).expect("accepted");
    assert_eq!(paragraphs(&document)[1], "A long sentence.");
    let written = String::from_utf8(xml::serialize(&document.document.content)).expect("UTF-8");
    assert!(!written.contains(&id), "{written}");
}

#[test]
fn reject_deletion_restores_text_at_mark() {
    let mut document = fixture();
    let id = id_of(&document, 1);
    reject(content(&mut document), &id).expect("rejected");
    assert_eq!(paragraphs(&document)[1], "A very long sentence.");
}

#[test]
fn reject_deletion_spanning_paragraphs_restores_both() {
    let mut document = fixture();
    let id = id_of(&document, 2);
    reject(content(&mut document), &id).expect("rejected");
    let shown = paragraphs(&document);
    assert_eq!(shown[2], "Before the cutand then some");
    assert_eq!(shown[3], "A whole line once stood here, and after the cut.");
    assert_eq!(shown.len(), 5);
}

#[test]
fn accept_all_leaves_no_tracked_changes_element() {
    let mut document = fixture();
    accept_all(content(&mut document)).expect("accepted");
    let written = String::from_utf8(xml::serialize(&document.document.content)).expect("UTF-8");
    assert!(!written.contains("tracked-changes"), "{written}");
    assert!(!written.contains("change-"), "{written}");
    assert!(changes(&document.document.content).is_empty());
    let mut rejected = fixture();
    reject_all(content(&mut rejected)).expect("rejected");
    assert_eq!(paragraphs(&rejected)[0], "Some words.");
    assert!(changes(&rejected.document.content).is_empty());
}

#[test]
fn document_is_unchanged_after_open() {
    let document = fixture();
    let before = document.document.content.clone();
    let _ = changes(&document.document.content);
    let _ = tracking(&document.document.content);
    assert!(document.document.content == before);
}

#[test]
fn tracking_is_read_from_the_body() {
    let source = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:text text:track-changes="true"><text:p/></office:text></office:body></office:document-content>"#;
    assert!(tracking(
        &xml::parse(source.as_bytes(), "test").expect("parsed")
    ));
    assert!(!tracking(&fixture().document.content));
}

#[test]
fn a_paragraphs_change_marks_are_found_where_they_stand() {
    let document = fixture();
    let body = document.body().expect("a body");
    let first = body
        .elements()
        .find(|e| e.is(&Ns::Text, "p"))
        .expect("a paragraph");
    let id = id_of(&document, 0);
    let found = odox_core::edit::change_marks(first);
    assert_eq!(found, vec![(5, "start", id.clone()), (17, "end", id)]);
}
