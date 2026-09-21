//! Editing a paragraph's text: what survives, and what the string becomes.
//!
//! The claim is the one an editor rests on: after replacing a range of the
//! flat text, the flat text is what string arithmetic says it should be, every
//! element that contributed no characters is still there, and the paragraph
//! still reads back through a serialize and a parse. Measured over every
//! paragraph of every corpus document, and on paragraphs built here to hold
//! what the corpus is short of.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::{Path, PathBuf};

use odox_core::edit::{apply, join, join_with_previous, replace, rewrite, split, text};
use odox_core::{Element, Node, Ns, Package, xml};

fn corpus() -> Vec<PathBuf> {
    let mut documents = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"),
        &mut documents,
    );
    documents.sort();
    documents
}

fn walk(directory: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, into);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("odt" | "ods" | "odp")
        ) {
            into.push(path);
        }
    }
}

/// Every `text:p` and `text:h` in a tree, wherever it sits.
fn paragraphs(element: &Element, into: &mut Vec<Element>) {
    for child in element.elements() {
        if child.is(&Ns::Text, "p") || child.is(&Ns::Text, "h") {
            into.push(child.clone());
        } else {
            paragraphs(child, into);
        }
    }
}

fn corpus_paragraphs() -> Vec<Element> {
    let mut all = Vec::new();
    for path in corpus() {
        let bytes = std::fs::read(&path).expect("the document");
        let package = Package::read(&bytes).expect("a readable package");
        let content = package.xml("content.xml").expect("content");
        paragraphs(&content, &mut all);
    }
    assert!(
        all.len() > 100,
        "only {} paragraphs in the corpus",
        all.len()
    );
    all
}

/// How many elements under a paragraph contribute no characters.
fn markers(element: &Element) -> usize {
    element
        .elements()
        .map(|e| {
            let own = usize::from(
                !(e.is(&Ns::Text, "s")
                    || e.is(&Ns::Text, "tab")
                    || e.is(&Ns::Text, "line-break")
                    || e.is(&Ns::Text, "span")
                    || e.is(&Ns::Text, "a")),
            );
            own + markers(e)
        })
        .sum()
}

fn string_replace(s: &str, range: std::ops::Range<usize>, with: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out: String = chars[..range.start].iter().collect();
    out.push_str(with);
    out.extend(chars[range.end..].iter());
    out
}

fn reads_back(paragraph: &Element) {
    let mut root = Element::new("office", "document-content", Ns::Office);
    root.attrs.push(odox_core::xml::Attribute {
        name: odox_core::Name::new("xmlns", "text", Ns::Xmlns),
        value: "urn:oasis:names:tc:opendocument:xmlns:text:1.0".to_owned(),
    });
    root.attrs.push(odox_core::xml::Attribute {
        name: odox_core::Name::new("xmlns", "office", Ns::Xmlns),
        value: "urn:oasis:names:tc:opendocument:xmlns:office:1.0".to_owned(),
    });
    root.attrs.push(odox_core::xml::Attribute {
        name: odox_core::Name::new("xmlns", "draw", Ns::Xmlns),
        value: "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0".to_owned(),
    });
    root.attrs.push(odox_core::xml::Attribute {
        name: odox_core::Name::new("xmlns", "xlink", Ns::Xmlns),
        value: "http://www.w3.org/1999/xlink".to_owned(),
    });
    root.children.push(Node::Element(paragraph.clone()));
    root.self_closing = false;
    let written = xml::serialize(&root);
    let again = xml::parse(&written, "test").expect("well-formed after the edit");
    if root != again {
        panic!(
            "the edit does not read back:\n{}\n{}",
            String::from_utf8_lossy(&written),
            String::from_utf8_lossy(&xml::serialize(&again))
        );
    }
}

#[test]
fn the_corpus_edits_the_way_a_string_does() {
    let mut edited = 0;
    for original in corpus_paragraphs() {
        let before = text(&original);
        let len = before.chars().count();
        let marker_count = markers(&original);
        // A few offsets across the paragraph, both ends included.
        let offsets: Vec<usize> = [0, 1, len / 3, len / 2, len.saturating_sub(1), len]
            .into_iter()
            .filter(|&o| o <= len)
            .collect();
        for &a in &offsets {
            for &b in &offsets {
                if b < a {
                    continue;
                }
                for with in ["", "X", "two words", " ", "a\tb", "l1\nl2"] {
                    let mut paragraph = original.clone();
                    replace(&mut paragraph, a..b, with);
                    let expected = string_replace(&before, a..b, with);
                    assert_eq!(
                        text(&paragraph),
                        expected,
                        "replacing {a}..{b} with {with:?} in {before:?}"
                    );
                    assert_eq!(
                        markers(&paragraph),
                        marker_count,
                        "a marker was lost replacing {a}..{b} in {before:?}"
                    );
                    reads_back(&paragraph);
                    edited += 1;
                }
            }
        }
    }
    assert!(edited > 1000, "{edited} edits is too few to mean anything");
}

#[test]
fn an_unchanged_paragraph_is_unchanged() {
    for original in corpus_paragraphs() {
        let mut paragraph = original.clone();
        let len = text(&original).chars().count();
        replace(&mut paragraph, len..len, "");
        assert_eq!(paragraph, original, "{}", text(&original));
    }
}

fn paragraph(inner: &str) -> Element {
    let source = format!(
        r#"<text:p xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" text:style-name="P1">{inner}</text:p>"#
    );
    xml::parse(source.as_bytes(), "test").expect("a paragraph")
}

#[test]
fn text_is_the_characters_the_tree_holds() {
    let p = paragraph(
        r#"One<text:s text:c="3"/>two<text:tab/>three<text:line-break/><text:span text:style-name="T1">four</text:span><text:bookmark text:name="b"/>five<text:note text:id="n1"><text:note-citation>1</text:note-citation><text:note-body><text:p>The note.</text:p></text:note-body></text:note>."#,
    );
    assert_eq!(text(&p), "One   two\tthree\nfourfive.");
}

#[test]
fn deleting_across_a_span_and_a_bookmark_keeps_the_bookmark() {
    let mut p = paragraph(
        r#"ab<text:span text:style-name="T1">cd</text:span><text:bookmark text:name="b"/>ef"#,
    );
    replace(&mut p, 1..5, "");
    assert_eq!(text(&p), "af");
    assert!(p.descendant(&Ns::Text, "bookmark").is_some(), "{p:?}");
    assert!(
        p.descendant(&Ns::Text, "span").is_none(),
        "an emptied span is dropped: {p:?}"
    );
}

#[test]
fn a_span_that_was_already_empty_is_left_alone() {
    let mut p = paragraph(r#"ab<text:span text:style-name="T1"/>cd"#);
    replace(&mut p, 0..1, "");
    assert_eq!(text(&p), "bcd");
    assert!(p.descendant(&Ns::Text, "span").is_some());
}

#[test]
fn spaces_are_written_the_way_odf_requires() {
    let mut p = paragraph("ab");
    replace(&mut p, 1..1, "  x  ");
    assert_eq!(text(&p), "a  x  b");
    // One literal space, then `text:s` for the second, on both sides.
    let written = String::from_utf8(xml::serialize(&p)).expect("utf-8");
    assert!(written.contains("a <text:s/>x <text:s/>b"), "{written}");

    let mut p = paragraph("ab");
    replace(&mut p, 0..0, "   ");
    let written = String::from_utf8(xml::serialize(&p)).expect("utf-8");
    assert!(
        written.contains(r#"<text:s text:c="3"/>ab"#),
        "leading spaces are all text:s: {written}"
    );

    let mut p = paragraph("a<text:tab/>b");
    replace(&mut p, 1..2, "\t\n");
    assert_eq!(text(&p), "a\t\nb");
    let written = String::from_utf8(xml::serialize(&p)).expect("utf-8");
    assert!(
        written.contains("a<text:tab/><text:line-break/>b"),
        "{written}"
    );
}

#[test]
fn a_run_of_spaces_shrinks_and_grows() {
    let mut p = paragraph(r#"a<text:s text:c="4"/>b"#);
    replace(&mut p, 2..4, "");
    assert_eq!(text(&p), "a  b");
    let written = String::from_utf8(xml::serialize(&p)).expect("utf-8");
    assert!(written.contains(r#"a<text:s text:c="2"/>b"#), "{written}");
    replace(&mut p, 1..3, "");
    assert_eq!(text(&p), "ab");
    assert!(p.descendant(&Ns::Text, "s").is_none());
}

#[test]
fn an_empty_paragraph_takes_text() {
    let mut p = paragraph("");
    replace(&mut p, 0..0, "hello");
    assert_eq!(text(&p), "hello");
    assert!(!p.self_closing);
    reads_back(&p);
}

#[test]
fn text_after_a_frame_goes_after_the_frame() {
    let mut p = paragraph(
        r#"<draw:frame draw:name="f"><draw:image xlink:href="Pictures/x.png" xmlns:xlink="http://www.w3.org/1999/xlink"/></draw:frame>tail"#,
    );
    replace(&mut p, 0..0, "head ");
    assert_eq!(text(&p), "head tail");
    // The frame is still first: new text at offset 0 goes after whatever
    // zero-length element ends there.
    let first = p.elements().next().expect("an element");
    assert!(first.is(&Ns::Draw, "frame"), "{p:?}");
}

#[test]
fn a_paragraph_splits_and_joins() {
    let p = paragraph(
        r#"one <text:span text:style-name="T1">two</text:span><text:bookmark text:name="b"/> three"#,
    );
    let (first, second) = split(&p, 7);
    assert_eq!(text(&first), "one two");
    assert_eq!(text(&second), " three");
    assert_eq!(first.attr(&Ns::Text, "style-name"), Some("P1"));
    assert_eq!(second.attr(&Ns::Text, "style-name"), Some("P1"));
    assert!(
        first.descendant(&Ns::Text, "bookmark").is_some(),
        "a marker at the split stays with the first half"
    );
    assert!(second.descendant(&Ns::Text, "bookmark").is_none());
    // A leading space in the second half is one ODF keeps.
    let written = String::from_utf8(xml::serialize(&second)).expect("utf-8");
    assert!(written.contains("<text:s/>three"), "{written}");

    let mut joined = first;
    join(&mut joined, second);
    assert_eq!(text(&joined), "one two three");
    assert_eq!(
        joined
            .elements()
            .filter(|e| e.is(&Ns::Text, "bookmark"))
            .count(),
        1
    );
}

#[test]
fn a_split_at_either_end_gives_an_empty_half() {
    let p = paragraph("abc");
    let (first, second) = split(&p, 0);
    assert_eq!(text(&first), "");
    assert_eq!(text(&second), "abc");
    let (first, second) = split(&p, 3);
    assert_eq!(text(&first), "abc");
    assert_eq!(text(&second), "");
    let (_, second) = split(&paragraph(r#"<text:bookmark text:name="b"/>abc"#), 0);
    assert!(second.descendant(&Ns::Text, "bookmark").is_none());
}

#[test]
fn a_rewrite_touches_only_the_middle_that_changed() {
    let p = paragraph(
        r#"one <text:span text:style-name="T1">two</text:span><text:bookmark text:name="b"/> three"#,
    );
    let out = rewrite(&p, "one TWO three");
    assert_eq!(out.len(), 1);
    assert_eq!(text(&out[0]), "one TWO three");
    assert!(out[0].descendant(&Ns::Text, "bookmark").is_some());
    // The change fell inside the span, so the span still holds it.
    let span = out[0].descendant(&Ns::Text, "span").expect("the span");
    assert_eq!(text(span), "TWO");

    // Unchanged is unchanged.
    assert_eq!(rewrite(&p, &text(&p)), vec![p.clone()]);
}

#[test]
fn a_newline_typed_into_a_paragraph_splits_it() {
    let p = paragraph(r#"first<text:line-break/>second third"#);
    let out = rewrite(&p, "first\nsecond\nthird");
    assert_eq!(out.len(), 2, "the old line break stays, the new one splits");
    assert_eq!(text(&out[0]), "first\nsecond");
    assert_eq!(text(&out[1]), "third");
    assert!(out[0].descendant(&Ns::Text, "line-break").is_some());
    assert_eq!(out[1].attr(&Ns::Text, "style-name"), Some("P1"));

    let out = rewrite(&paragraph("abc"), "a\nb\nc");
    assert_eq!(
        out.iter().map(text).collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
    let out = rewrite(&paragraph("abc"), "abc\n");
    assert_eq!(out.iter().map(text).collect::<Vec<_>>(), vec!["abc", ""]);
}

#[test]
fn apply_and_join_work_by_path_under_a_root() {
    let mut root = paragraph("");
    root.name = odox_core::Name::new("office", "text", Ns::Office);
    root.children.clear();
    root.children.push(Node::Element(paragraph("one")));
    root.children.push(Node::Text("\n  ".to_owned()));
    root.children.push(Node::Element(paragraph("two")));

    assert_eq!(apply(&mut root, &[2], "two\nthree"), Ok(2));
    assert_eq!(root.children.len(), 4);
    let texts: Vec<String> = root.elements().map(text).collect();
    assert_eq!(texts, vec!["one", "two", "three"]);

    let at = join_with_previous(&mut root, &[3]).expect("joins");
    assert_eq!(at, vec![2]);
    let texts: Vec<String> = root.elements().map(text).collect();
    assert_eq!(texts, vec!["one", "twothree"]);

    // The first paragraph has nothing before it; the whitespace node is not a
    // paragraph.
    assert_eq!(
        join_with_previous(&mut root, &[0]),
        Err(odox_core::Refused::NotFound)
    );
    assert_eq!(
        apply(&mut root, &[1], "x"),
        Err(odox_core::Refused::NotFound)
    );
}
