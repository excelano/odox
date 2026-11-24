//! Footnotes and endnotes put in, numbered and their text changed.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::edit::{
    NoteClass, insert_note, note_text, notes_in, remove_note, renumber_notes, set_note_text, text,
};
use odox_core::{Element, Ns, Styles, xml};

const NAMESPACES: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0""#;

fn content(body: &str) -> Element {
    let source = format!(
        "<office:document-content {NAMESPACES}><office:body><office:text>{body}</office:text></office:body></office:document-content>"
    );
    xml::parse(source.as_bytes(), "test").expect("a content root")
}

fn citations(content: &Element) -> Vec<String> {
    fn walk(element: &Element, into: &mut Vec<String>) {
        for child in element.elements() {
            if child.is(&Ns::Text, "note") {
                let class = child.attr(&Ns::Text, "note-class").unwrap_or("footnote");
                let citation = child
                    .child(&Ns::Text, "note-citation")
                    .map(Element::plain_text)
                    .unwrap_or_default();
                into.push(format!("{}{citation}", &class[..1]));
            }
            walk(child, into);
        }
    }
    let mut into = Vec::new();
    walk(content, &mut into);
    into
}

fn note(n: &str, class: &str) -> String {
    format!(
        r#"<text:note text:note-class="{class}"><text:note-citation>{n}</text:note-citation><text:note-body><text:p>body</text:p></text:note-body></text:note>"#
    )
}

#[test]
fn insert_footnote_numbers_after_previous() {
    let mut root = content(&format!(
        "<text:p>One{}</text:p><text:p>Two</text:p>",
        note("1", "footnote")
    ));
    let styles = Styles::collect(Some(&root), None);
    let body = insert_note(&mut root, &[0, 0, 1], 3, NoteClass::Footnote, &styles).expect("put in");
    assert_eq!(citations(&root), ["f1", "f2"]);
    assert!(root.at(&body).is_some_and(|p| p.is(&Ns::Text, "p")));
    assert_eq!(text(root.at(&[0, 0, 1]).expect("the paragraph")), "Two");
}

#[test]
fn renumber_after_insert_in_middle() {
    let mut root = content(&format!(
        "<text:p>One{}</text:p><text:p>Two</text:p><text:p>Three{}{}</text:p>",
        note("1", "footnote"),
        note("2", "footnote"),
        note("1", "endnote")
    ));
    let styles = Styles::collect(Some(&root), None);
    insert_note(&mut root, &[0, 0, 1], 0, NoteClass::Footnote, &styles).expect("put in");
    insert_note(&mut root, &[0, 0, 1], 0, NoteClass::Endnote, &styles).expect("put in");
    assert_eq!(citations(&root), ["f1", "e1", "f2", "f3", "e2"]);
}

#[test]
fn labelled_note_keeps_label() {
    let mut root = content(
        r#"<text:p>A<text:note text:note-class="footnote"><text:note-citation text:label="*">*</text:note-citation><text:note-body><text:p/></text:note-body></text:note>B</text:p>"#,
    );
    let styles = Styles::collect(Some(&root), None);
    insert_note(&mut root, &[0, 0, 0], 0, NoteClass::Footnote, &styles).expect("put in");
    renumber_notes(&mut root);
    assert_eq!(citations(&root), ["f1", "f*"]);
}

#[test]
fn a_notes_text_is_changed_a_line_to_a_paragraph() {
    let mut root = content(&format!("<text:p>One{}</text:p>", note("1", "footnote")));
    let at = [0, 0, 0, 1];
    set_note_text(&mut root, &at, "First\nSecond").expect("changed");
    assert_eq!(note_text(root.at(&at).expect("the note")), "First\nSecond");
}

#[test]
fn notes_are_found_where_they_stand_and_one_is_deleted() {
    let mut root = content(&format!(
        "<text:p>One{}two{}</text:p>",
        note("1", "footnote"),
        note("2", "footnote")
    ));
    let paragraph = root.at(&[0, 0, 0]).expect("the paragraph");
    let found = notes_in(paragraph);
    assert_eq!(found.iter().map(|(at, _)| *at).collect::<Vec<_>>(), [3, 6]);
    let first = [[0, 0, 0].as_slice(), &found[0].1].concat();
    remove_note(&mut root, &first).expect("deleted");
    assert_eq!(citations(&root), ["f1"]);
    assert_eq!(text(root.at(&[0, 0, 0]).expect("it")), "Onetwo");
}

#[test]
fn refuses_text_for_a_note_holding_more_than_paragraphs_and_changes_nothing() {
    let mut root = content(
        r#"<text:p>One<text:note text:note-class="footnote"><text:note-citation>1</text:note-citation><text:note-body><text:p>A</text:p><text:list><text:list-item><text:p>x</text:p></text:list-item></text:list><text:p>B</text:p></text:note-body></text:note></text:p>"#,
    );
    let before = root.clone();
    assert_eq!(
        set_note_text(&mut root, &[0, 0, 0, 1], "A2\nB"),
        Err(odox_core::Refused::Structure)
    );
    assert!(root == before);
}

#[test]
fn notes_in_tracked_deletions_are_not_counted() {
    let mut root = content(&format!(
        r#"<text:tracked-changes><text:changed-region text:id="ct1"><text:deletion><text:p>Gone{}</text:p></text:deletion></text:changed-region></text:tracked-changes><text:p>One{}</text:p>"#,
        note("1", "footnote"),
        note("2", "footnote")
    ));
    renumber_notes(&mut root);
    let shown = root.at(&[0, 0, 1]).expect("the paragraph");
    let citation = shown
        .descendant(&Ns::Text, "note-citation")
        .map(Element::plain_text);
    assert_eq!(citation.as_deref(), Some("1"));
}
