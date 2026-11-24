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
fn tracking_is_read_from_the_tracked_changes_element() {
    assert!(tracking(&fixture().document.content));
    assert!(!tracking(&body("<text:p/>")));
    assert!(!tracking(&body(
        r#"<text:tracked-changes text:track-changes="false"/><text:p/>"#
    )));
}

const NAMESPACES: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/""#;

fn body(inner: &str) -> Element {
    let source = format!(
        "<office:document-content {NAMESPACES}><office:body><office:text>{inner}</office:text></office:body></office:document-content>"
    );
    xml::parse(source.as_bytes(), "test").expect("parsed")
}

fn region(id: &str, kind: &str, inner: &str) -> String {
    format!(
        r#"<text:changed-region text:id="{id}"><text:{kind}><office:change-info><dc:creator>A</dc:creator></office:change-info>{inner}</text:{kind}></text:changed-region>"#
    )
}

fn written(content: &Element) -> String {
    String::from_utf8(xml::serialize(content)).expect("UTF-8")
}

#[test]
fn reject_deletion_holding_a_list_restores_the_list() {
    let mut root = body(&format!(
        r#"<text:tracked-changes>{}</text:tracked-changes><text:p>A<text:change text:change-id="d"/>B</text:p>"#,
        region(
            "d",
            "deletion",
            "<text:p>one</text:p><text:list><text:list-item><text:p>item</text:p></text:list-item></text:list><text:p>two</text:p>"
        )
    ));
    reject(&mut root, "d").expect("rejected");
    let out = written(&root);
    assert!(out.contains("<text:p>Aone</text:p><text:list><text:list-item><text:p>item</text:p></text:list-item></text:list><text:p>twoB</text:p>"), "{out}");
}

#[test]
fn reject_deletion_marked_between_paragraphs_restores_them_there() {
    let mut root = body(&format!(
        r#"<text:tracked-changes>{}</text:tracked-changes><text:p>A</text:p><text:change text:change-id="d"/><text:p>B</text:p>"#,
        region("d", "deletion", "<text:p>gone</text:p>")
    ));
    reject(&mut root, "d").expect("rejected");
    let out = written(&root);
    assert!(
        out.contains("<text:p>A</text:p><text:p>gone</text:p><text:p>B</text:p>"),
        "{out}"
    );
}

#[test]
fn reject_insertion_without_its_end_is_refused_and_changes_nothing() {
    let mut root = body(&format!(
        r#"<text:tracked-changes>{}</text:tracked-changes><text:p>A<text:change-start text:change-id="i"/>new</text:p>"#,
        region("i", "insertion", "")
    ));
    let before = root.clone();
    assert!(reject(&mut root, "i").is_err());
    assert!(root == before);
}

#[test]
fn accepting_the_last_change_keeps_the_recording_switch() {
    let mut root = body(&format!(
        r#"<text:tracked-changes text:track-changes="true">{}</text:tracked-changes><text:p>A<text:change-start text:change-id="i"/>new<text:change-end text:change-id="i"/></text:p>"#,
        region("i", "insertion", "")
    ));
    accept_all(&mut root).expect("accepted");
    assert!(tracking(&root));
}

#[test]
fn reject_deletion_keeps_its_last_paragraphs_kind() {
    let mut root = body(&format!(
        r#"<text:tracked-changes>{}</text:tracked-changes><text:p>A<text:change text:change-id="d"/>B</text:p>"#,
        region(
            "d",
            "deletion",
            r#"<text:p>x</text:p><text:h text:outline-level="1">Head</text:h>"#
        )
    ));
    reject(&mut root, "d").expect("rejected");
    let out = written(&root);
    assert!(
        out.contains(r#"<text:h text:outline-level="1">HeadB</text:h>"#),
        "{out}"
    );
}

#[test]
fn rejecting_an_insertion_takes_the_changes_inside_it_too() {
    let mut root = body(&format!(
        r#"<text:tracked-changes>{}{}</text:tracked-changes><text:p>A<text:change-start text:change-id="i"/>new</text:p><text:p>mid<text:change text:change-id="d"/>dle</text:p><text:p>end<text:change-end text:change-id="i"/>B</text:p>"#,
        region("i", "insertion", ""),
        region("d", "deletion", "<text:p>gone</text:p>")
    ));
    reject(&mut root, "i").expect("rejected");
    assert!(changes(&root).is_empty(), "{}", written(&root));
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
