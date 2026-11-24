//! Comments in a text document: read from what LibreOffice writes, and added,
//! changed and taken out again.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use odox_core::doc::TextDocument;
use odox_core::edit::{
    Annotation, add_annotation, add_slide_annotation, annotations_in, remove_annotation,
    remove_slide_annotation, set_annotation_text, set_slide_annotation_text, slide_annotations,
    text,
};
use odox_core::{Element, Ns, xml};

fn fixture() -> TextDocument {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/comments.odt"),
    )
    .expect("the comments fixture");
    TextDocument::read(&bytes).expect("it reads")
}

fn paragraph(document: &TextDocument, index: usize) -> &Element {
    document
        .body()
        .expect("a body")
        .elements()
        .filter(|e| e.is(&Ns::Text, "p"))
        .nth(index)
        .expect("the paragraph")
}

#[test]
fn reads_author_date_text() {
    let document = fixture();
    assert_eq!(
        annotations_in(paragraph(&document, 0)),
        [(
            18,
            Annotation {
                author: Some("Ada Reviewer".to_owned()),
                date: Some("2026-03-14T09:30:00".to_owned()),
                text: "Check this figure.".to_owned(),
                end: None,
            }
        )]
    );
    assert_eq!(annotations_in(paragraph(&document, 2)), []);
}

#[test]
fn range_annotation_spans_to_end_mark() {
    let document = fixture();
    let found = annotations_in(paragraph(&document, 1));
    assert_eq!(found.len(), 1);
    let (at, note) = &found[0];
    assert_eq!((*at, note.end), (6, Some(22)));
    assert_eq!(note.text, "Too long.\nCut it?");
}

#[test]
fn annotation_contributes_nothing_to_flat_text() {
    let document = fixture();
    assert_eq!(
        text(paragraph(&document, 0)),
        "A remark sits here in the first paragraph."
    );
}

const NAMESPACES: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0""#;

fn content(paragraph: &str) -> Element {
    let source = format!(
        "<office:document-content {NAMESPACES}><office:body><office:text>{paragraph}</office:text></office:body></office:document-content>"
    );
    xml::parse(source.as_bytes(), "test").expect("a content root")
}

const PATH: [usize; 3] = [0, 0, 0];

#[test]
fn add_edit_remove_round_trip() {
    let original = content("<text:p>One two three</text:p>");
    let mut root = original.clone();
    add_annotation(
        &mut root,
        &PATH,
        4..7,
        "Why two?",
        Some("Cy"),
        "2026-11-23T10:00:00",
    )
    .expect("added");
    let paragraph = root.at(&PATH).expect("the paragraph");
    assert_eq!(text(paragraph), "One two three");
    let found = annotations_in(paragraph);
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].0, found[0].1.end), (4, Some(7)));
    assert_eq!(found[0].1.author.as_deref(), Some("Cy"));
    set_annotation_text(
        &mut root,
        &PATH,
        0,
        "Why not three?",
        Some("Di"),
        "2026-11-24T10:00:00",
    )
    .expect("changed");
    let found = annotations_in(root.at(&PATH).expect("the paragraph"));
    assert_eq!(found[0].1.text, "Why not three?");
    assert_eq!(found[0].1.author.as_deref(), Some("Di"));
    assert_eq!(
        (found[0].0, found[0].1.end),
        (4, Some(7)),
        "still on its range"
    );
    remove_annotation(&mut root, &PATH, 0).expect("removed");
    assert!(
        root.at(&PATH) == original.at(&PATH),
        "{}",
        String::from_utf8_lossy(&xml::serialize(&root))
    );
}

#[test]
fn remove_range_annotation_removes_end_mark() {
    let mut document = fixture();
    let body = document.body_path().expect("a body");
    let index = document
        .body()
        .expect("a body")
        .elements_indexed()
        .filter(|(_, e)| e.is(&Ns::Text, "p"))
        .nth(1)
        .map(|(i, _)| i)
        .expect("the second paragraph");
    let path = [body.as_slice(), &[index]].concat();
    let content = &mut document.document.content;
    remove_annotation(content, &path, 0).expect("removed");
    let written = String::from_utf8(xml::serialize(content.at(&path).expect("it"))).expect("UTF-8");
    assert!(!written.contains("annotation"), "{written}");
    assert_eq!(
        text(content.at(&path).expect("it")),
        "These words are marked by a range."
    );
}

#[test]
fn reads_a_slides_comment_and_where_it_is() {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/comments.odp"),
    )
    .expect("the slide comments fixture");
    let deck = odox_core::doc::Presentation::read(&bytes).expect("it reads");
    let slides = deck.slides();
    let found = slide_annotations(slides[0].element);
    assert_eq!(found.len(), 1);
    let (_, (x, y), note) = &found[0];
    let cm = |cm: f32| cm / 2.54 * 72.0;
    assert!((x - cm(20.0)).abs() < 0.1 && (y - cm(3.0)).abs() < 0.1);
    assert_eq!(note.author.as_deref(), Some("Ada Reviewer"));
    assert_eq!(note.text, "Is this the right title?");
}

#[test]
fn a_slides_comment_is_added_changed_and_removed() {
    let source = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:presentation><draw:page draw:name="One"/></office:presentation></office:body></office:document-content>"#;
    let original = xml::parse(source.as_bytes(), "test").expect("a content root");
    let mut root = original.clone();
    let page = [0, 0, 0];
    let child = add_slide_annotation(
        &mut root,
        &page,
        (100.0, 50.0),
        "First",
        Some("Cy"),
        "2026-11-23T10:00:00",
    )
    .expect("added");
    set_slide_annotation_text(
        &mut root,
        &page,
        child,
        "Second",
        Some("Di"),
        "2026-11-24T10:00:00",
    )
    .expect("changed");
    let found = slide_annotations(root.at(&page).expect("the page"));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].2.text, "Second");
    assert!((found[0].1.0 - 100.0).abs() < 0.1);
    remove_slide_annotation(&mut root, &page, child).expect("removed");
    assert!(root.at(&page) == original.at(&page));
}

#[test]
fn removing_a_comment_takes_its_end_mark_from_a_later_paragraph() {
    let mut root = content(
        r#"<text:p>one<office:annotation office:name="n1"><text:p>x</text:p></office:annotation> two</text:p><text:p>three<office:annotation-end office:name="n1"/> four</text:p>"#,
    );
    remove_annotation(&mut root, &PATH, 0).expect("removed");
    let written = String::from_utf8(xml::serialize(&root)).expect("UTF-8");
    assert!(!written.contains("annotation"), "{written}");
}

#[test]
fn a_new_range_never_takes_a_name_an_end_mark_has() {
    let mut root = content(
        r#"<text:p>One two three<office:annotation-end office:name="__Annotation__1"/></text:p>"#,
    );
    add_annotation(&mut root, &PATH, 0..3, "x", None, "2026-11-23T10:00:00").expect("added");
    let written = String::from_utf8(xml::serialize(&root)).expect("UTF-8");
    assert!(
        written.contains(r#"office:name="__Annotation__2""#),
        "{written}"
    );
}

#[test]
fn a_comment_inside_a_run_of_spaces_stays_where_it_was_put() {
    let mut root = content(r#"<text:p>a<text:s text:c="4"/>b</text:p>"#);
    add_annotation(&mut root, &PATH, 2..4, "x", None, "2026-11-23T10:00:00").expect("added");
    let paragraph = root.at(&PATH).expect("the paragraph");
    assert_eq!(text(paragraph), "a    b");
    let found = annotations_in(paragraph);
    assert_eq!((found[0].0, found[0].1.end), (2, Some(4)));
}

#[test]
fn a_comment_is_written_in_the_prefixes_the_document_declares() {
    let source = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:purl="http://purl.org/dc/elements/1.1/"><office:body><office:text><text:p>One</text:p></office:text></office:body></office:document-content>"#;
    let mut root = xml::parse(source.as_bytes(), "test").expect("a content root");
    add_annotation(
        &mut root,
        &PATH,
        0..0,
        "x",
        Some("Cy"),
        "2026-11-23T10:00:00",
    )
    .expect("added");
    let written = String::from_utf8(xml::serialize(&root)).expect("UTF-8");
    assert!(
        written.contains("<purl:creator>Cy</purl:creator>"),
        "{written}"
    );
    xml::parse(written.as_bytes(), "again").expect("well formed");
}

#[test]
fn a_heading_with_a_comment_reads_as_its_own_words() {
    let root = content(
        r#"<text:h>Intro<office:annotation><dc:creator xmlns:dc="http://purl.org/dc/elements/1.1/">Cy</dc:creator><text:p>rewrite</text:p></office:annotation>duction</text:h>"#,
    );
    assert_eq!(
        root.at(&PATH).expect("the heading").plain_text(),
        "Introduction"
    );
}
