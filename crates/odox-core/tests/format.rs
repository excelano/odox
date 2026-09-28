//! Formatting a range of a paragraph: what changes, and what does not.
//!
//! The claim is the one §3 of DESIGN.md asks of every edit: formatting a range
//! changes that paragraph and the automatic styles and nothing else, the text
//! is the same text, the document reads back, and the range then reads as
//! formatted. Taking a mark off text it was put on gives back the paragraph it
//! was. Measured over every paragraph of every corpus document, and on
//! paragraphs built here to hold what the corpus is short of.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::{Path, PathBuf};

use odox_core::edit::{Mark, format, marked, text};
use odox_core::{Element, Family, Node, Ns, Package, Refused, Styles, xml};

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

/// The path of every `text:p` and `text:h` under an element, wherever it sits.
fn paragraph_paths(element: &Element, path: &mut Vec<usize>, into: &mut Vec<Vec<usize>>) {
    for (index, child) in element.elements_indexed() {
        path.push(index);
        if child.is(&Ns::Text, "p") || child.is(&Ns::Text, "h") {
            into.push(path.clone());
        }
        paragraph_paths(child, path, into);
        path.pop();
    }
}

/// The styles container's children, which is all a format may add to.
fn automatic_count(content: &Element) -> Option<usize> {
    content
        .child(&Ns::Office, "automatic-styles")
        .map(|c| c.children.len())
}

/// The content as it would be had nothing but the paragraph at a path and
/// the automatic styles changed: the paragraph put back, and whatever was
/// added to the styles taken off.
fn restored(edited: &Element, original: &Element, path: &[usize]) -> Element {
    let mut restored = edited.clone();
    let mut original_path = path.to_vec();
    if automatic_count(original).is_none() && automatic_count(edited).is_some() {
        original_path[0] -= 1;
    }
    *restored.at_mut(path).expect("the paragraph") =
        original.at(&original_path).expect("it").clone();
    match automatic_count(original) {
        Some(count) => {
            let container = restored
                .child_mut(&Ns::Office, "automatic-styles")
                .expect("kept");
            container.children.truncate(count);
        }
        None => restored
            .children
            .retain(|n| !matches!(n, Node::Element(e) if e.is(&Ns::Office, "automatic-styles"))),
    }
    restored
}

fn reads_back(content: &Element) {
    let written = xml::serialize(content);
    let again = xml::parse(&written, "content.xml").expect("well-formed after the format");
    assert!(*content == again, "the format does not read back");
}

/// Whether an offset falls between two spaces, where the text may be a
/// `text:s` that a format cuts in two and taking the mark off does not join.
fn between_spaces(chars: &[char], at: usize) -> bool {
    at > 0 && at < chars.len() && chars[at - 1] == ' ' && chars[at] == ' '
}

#[test]
fn the_corpus_formats_one_paragraph_and_nothing_else() {
    let mut formatted = 0;
    let mut given_back = 0;
    for document in corpus() {
        let bytes = std::fs::read(&document).expect("the document");
        let package = Package::read(&bytes).expect("a readable package");
        let content = package.xml("content.xml").expect("content");
        let styles_part = package.optional_xml("styles.xml").expect("styles");
        let mut paths = Vec::new();
        paragraph_paths(&content, &mut Vec::new(), &mut paths);

        for path in paths {
            let paragraph = content.at(&path).expect("the paragraph");
            let chars: Vec<char> = text(paragraph).chars().collect();
            let range = chars.len() / 3..chars.len() * 2 / 3;
            if range.is_empty() {
                continue;
            }
            let mut styles = Styles::collect(Some(&content), styles_part.as_ref());
            let before = marked(paragraph, range.clone(), Mark::Bold, &styles);

            let mut edited = content.clone();
            format(
                &mut edited,
                &path,
                range.clone(),
                Mark::Bold,
                true,
                &mut styles,
            )
            .expect("a corpus paragraph formats");
            // Styles written into a document that had no container for them
            // go before the body, and move it along.
            let mut path = path.clone();
            if automatic_count(&content).is_none() && automatic_count(&edited).is_some() {
                path[0] += 1;
            }
            let after = edited.at(&path).expect("the paragraph");
            let where_ = format!("{} at {path:?}", document.display());
            assert_eq!(text(after), text(paragraph), "{where_}");
            assert_eq!(
                marked(after, range.clone(), Mark::Bold, &styles),
                Some(true),
                "{where_}"
            );
            assert!(restored(&edited, &content, &path) == content, "{where_}");
            reads_back(&edited);
            formatted += 1;

            format(
                &mut edited,
                &path,
                range.clone(),
                Mark::Bold,
                false,
                &mut styles,
            )
            .expect("and takes the mark off");
            let after = edited.at(&path).expect("the paragraph");
            assert_eq!(text(after), text(paragraph), "{where_}");
            assert_eq!(
                marked(after, range.clone(), Mark::Bold, &styles),
                Some(false),
                "{where_}"
            );
            if before == Some(false)
                && !between_spaces(&chars, range.start)
                && !between_spaces(&chars, range.end)
            {
                assert!(
                    after == paragraph,
                    "{where_}: on and off is not the paragraph it was:\n{}\n{}",
                    String::from_utf8_lossy(&xml::serialize(paragraph)),
                    String::from_utf8_lossy(&xml::serialize(after)),
                );
                given_back += 1;
            }
        }
    }
    assert!(formatted > 100, "only {formatted} paragraphs formatted");
    assert!(given_back > 50, "only {given_back} paragraphs given back");
}

const NAMESPACES: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0""#;

/// A content root holding automatic styles and one paragraph, and the path to
/// the paragraph.
fn document(automatic: &str, paragraph: &str) -> (Element, Vec<usize>) {
    let source = format!(
        r#"<office:document-content {NAMESPACES}><office:automatic-styles>{automatic}</office:automatic-styles><office:body><office:text><text:p text:style-name="P1">{paragraph}</text:p></office:text></office:body></office:document-content>"#
    );
    let content = xml::parse(source.as_bytes(), "content.xml").expect("a document");
    (content, vec![1, 0, 0])
}

/// A paragraph's contents, written back without the paragraph's own tags.
fn inside(content: &Element, path: &[usize]) -> String {
    let written =
        String::from_utf8(xml::serialize(content.at(path).expect("the paragraph"))).expect("UTF-8");
    let tag = written.find("<text:p").expect("the paragraph's start tag");
    let open = tag + written[tag..].find('>').expect("a start tag") + 1;
    let close = written.rfind("</").expect("an end tag");
    written[open..close].to_owned()
}

/// A text style as the document holds it, by name.
fn style<'a>(content: &'a Element, name: &str) -> &'a Element {
    content
        .child(&Ns::Office, "automatic-styles")
        .expect("automatic styles")
        .elements()
        .find(|e| e.attr(&Ns::Style, "name") == Some(name))
        .unwrap_or_else(|| panic!("no style {name}"))
}

fn text_properties<'a>(content: &'a Element, name: &str) -> &'a Element {
    style(content, name)
        .child(&Ns::Style, "text-properties")
        .expect("text properties")
}

const ITALIC: &str = r#"<style:style style:name="T1" style:family="text"><style:text-properties fo:font-style="italic"/></style:style>"#;
const BOLD: &str = r#"<style:style style:name="T1" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style>"#;

fn styles_of(content: &Element) -> Styles {
    Styles::collect(Some(content), None)
}

#[test]
fn a_word_is_wrapped_and_unwrapped() {
    let (original, path) = document("", "one two three");
    let mut content = original.clone();
    let mut styles = styles_of(&content);
    format(&mut content, &path, 4..7, Mark::Bold, true, &mut styles).expect("bold");
    assert_eq!(
        inside(&content, &path),
        r#"one <text:span text:style-name="T1">two</text:span> three"#
    );
    let properties = text_properties(&content, "T1");
    assert_eq!(properties.attr(&Ns::Fo, "font-weight"), Some("bold"));
    assert_eq!(
        properties.attr(&Ns::Style, "font-weight-asian"),
        Some("bold")
    );
    assert_eq!(
        properties.attr(&Ns::Style, "font-weight-complex"),
        Some("bold")
    );
    assert!(styles.resolve(&Family::Text, "T1").text.bold == Some(true));

    format(&mut content, &path, 4..7, Mark::Bold, false, &mut styles).expect("not bold");
    assert!(
        content.at(&path) == original.at(&path),
        "{}",
        inside(&content, &path)
    );
}

#[test]
fn marked_answers_for_a_range_and_for_a_caret() {
    let (content, path) = document(
        BOLD,
        r#"ab<text:span text:style-name="T1">cd</text:span>ef"#,
    );
    let styles = styles_of(&content);
    let p = content.at(&path).expect("the paragraph");
    assert_eq!(marked(p, 2..4, Mark::Bold, &styles), Some(true));
    assert_eq!(marked(p, 1..3, Mark::Bold, &styles), None);
    assert_eq!(marked(p, 0..2, Mark::Bold, &styles), Some(false));
    // A caret reads the character before it, which is what typing there takes.
    assert_eq!(marked(p, 4..4, Mark::Bold, &styles), Some(true));
    assert_eq!(marked(p, 2..2, Mark::Bold, &styles), Some(false));
    assert_eq!(marked(p, 0..0, Mark::Bold, &styles), Some(false));
    assert_eq!(marked(p, 0..6, Mark::Italic, &styles), Some(false));
}

#[test]
fn a_span_held_whole_is_restyled_rather_than_wrapped() {
    let (mut content, path) = document(
        ITALIC,
        r#"a<text:span text:style-name="T1">bc</text:span>d"#,
    );
    let mut styles = styles_of(&content);
    format(&mut content, &path, 1..3, Mark::Bold, true, &mut styles).expect("bold");
    assert_eq!(
        inside(&content, &path),
        r#"a<text:span text:style-name="T2">bc</text:span>d"#
    );
    let properties = text_properties(&content, "T2");
    assert_eq!(properties.attr(&Ns::Fo, "font-style"), Some("italic"));
    assert_eq!(properties.attr(&Ns::Fo, "font-weight"), Some("bold"));
    // T1 is the document's own and is not changed; something else may use it.
    assert_eq!(
        text_properties(&content, "T1").attr(&Ns::Fo, "font-weight"),
        None
    );
}

#[test]
fn part_of_a_bold_span_is_taken_out_inside_it() {
    let (mut content, path) = document(BOLD, r#"<text:span text:style-name="T1">abcd</text:span>"#);
    let mut styles = styles_of(&content);
    format(&mut content, &path, 1..3, Mark::Bold, false, &mut styles).expect("not bold");
    assert_eq!(
        inside(&content, &path),
        r#"<text:span text:style-name="T1">a<text:span text:style-name="T2">bc</text:span>d</text:span>"#
    );
    assert_eq!(
        text_properties(&content, "T2").attr(&Ns::Fo, "font-weight"),
        Some("normal")
    );
    let p = content.at(&path).expect("the paragraph");
    assert_eq!(marked(p, 1..3, Mark::Bold, &styles), Some(false));
    assert_eq!(marked(p, 0..1, Mark::Bold, &styles), Some(true));
}

#[test]
fn a_bold_span_inside_the_range_is_unbolded_where_it_stands() {
    let (mut content, path) = document(
        BOLD,
        r#"ab<text:span text:style-name="T1">cd</text:span>ef"#,
    );
    let mut styles = styles_of(&content);
    format(&mut content, &path, 0..6, Mark::Bold, false, &mut styles).expect("not bold");
    assert_eq!(inside(&content, &path), "abcdef");
}

#[test]
fn a_run_around_a_span_that_says_otherwise_fixes_the_span() {
    // Italic on over a span that says upright: the run is wrapped, and the
    // span inside it stops saying upright.
    let upright = r#"<style:style style:name="T1" style:family="text"><style:text-properties fo:font-style="normal"/></style:style>"#;
    let (mut content, path) = document(
        upright,
        r#"ab<text:span text:style-name="T1">cd</text:span>ef"#,
    );
    let mut styles = styles_of(&content);
    format(&mut content, &path, 1..5, Mark::Italic, true, &mut styles).expect("italic");
    let p = content.at(&path).expect("the paragraph");
    assert_eq!(marked(p, 1..5, Mark::Italic, &styles), Some(true));
    assert_eq!(marked(p, 0..1, Mark::Italic, &styles), Some(false));
    assert_eq!(marked(p, 5..6, Mark::Italic, &styles), Some(false));
}

#[test]
fn a_marker_inside_the_range_goes_in_and_one_at_an_edge_stays_out() {
    let (mut content, path) = document(
        "",
        r#"ab<text:bookmark text:name="edge"/>cd<text:bookmark text:name="inside"/>ef"#,
    );
    let mut styles = styles_of(&content);
    format(&mut content, &path, 2..6, Mark::Bold, true, &mut styles).expect("bold");
    assert_eq!(
        inside(&content, &path),
        r#"ab<text:bookmark text:name="edge"/><text:span text:style-name="T1">cd<text:bookmark text:name="inside"/>ef</text:span>"#
    );
}

#[test]
fn a_link_is_not_split() {
    let (mut content, path) = document("", r#"ab<text:a text:name="x">cd</text:a>"#);
    let mut styles = styles_of(&content);
    format(
        &mut content,
        &path,
        1..3,
        Mark::Underline,
        true,
        &mut styles,
    )
    .expect("underline");
    assert_eq!(
        inside(&content, &path),
        r#"a<text:span text:style-name="T1">b</text:span><text:a text:name="x"><text:span text:style-name="T1">c</text:span>d</text:a>"#
    );
    let properties = text_properties(&content, "T1");
    assert_eq!(
        properties.attr(&Ns::Style, "text-underline-style"),
        Some("solid")
    );
}

#[test]
fn a_style_that_says_the_same_is_used_again() {
    let (mut content, path) = document("", "one two three");
    let mut styles = styles_of(&content);
    format(&mut content, &path, 0..3, Mark::Strike, true, &mut styles).expect("strike");
    format(&mut content, &path, 8..13, Mark::Strike, true, &mut styles).expect("strike");
    let container = content
        .child(&Ns::Office, "automatic-styles")
        .expect("styles");
    assert_eq!(container.elements().count(), 1);
    assert_eq!(
        inside(&content, &path),
        r#"<text:span text:style-name="T1">one</text:span> two <text:span text:style-name="T1">three</text:span>"#
    );
}

#[test]
fn a_new_style_takes_a_name_nobody_has() {
    let (mut content, path) = document(ITALIC, "one two");
    let mut styles = styles_of(&content);
    format(&mut content, &path, 0..3, Mark::Bold, true, &mut styles).expect("bold");
    assert_eq!(
        inside(&content, &path),
        r#"<text:span text:style-name="T2">one</text:span> two"#
    );
}

#[test]
fn a_paragraph_that_is_bold_already_is_left_alone() {
    let heavy = r#"<style:style style:name="P1" style:family="paragraph"><style:text-properties fo:font-weight="bold"/></style:style>"#;
    let (original, path) = document(heavy, "one two");
    let mut content = original.clone();
    let mut styles = styles_of(&content);
    format(&mut content, &path, 0..3, Mark::Bold, true, &mut styles).expect("bold");
    assert!(content == original);
    format(&mut content, &path, 0..3, Mark::Bold, false, &mut styles).expect("not bold");
    assert_eq!(
        inside(&content, &path),
        r#"<text:span text:style-name="T1">one</text:span> two"#
    );
    assert_eq!(
        text_properties(&content, "T1").attr(&Ns::Fo, "font-weight"),
        Some("normal")
    );
}

#[test]
fn an_offset_inside_a_run_of_spaces_cuts_it() {
    let (mut content, path) = document("", r#"a <text:s text:c="3"/>b"#);
    let mut styles = styles_of(&content);
    format(&mut content, &path, 2..6, Mark::Bold, true, &mut styles).expect("bold");
    let p = content.at(&path).expect("the paragraph");
    assert_eq!(text(p), "a    b");
    assert_eq!(marked(p, 2..5, Mark::Bold, &styles), Some(true));
    assert_eq!(marked(p, 1..2, Mark::Bold, &styles), Some(false));
    reads_back(&content);
}

#[test]
fn a_document_without_automatic_styles_is_given_them_before_its_body() {
    let source = format!(
        r#"<office:document-content {NAMESPACES}><office:font-face-decls/><office:body><office:text><text:p>one</text:p></office:text></office:body></office:document-content>"#
    );
    let mut content = xml::parse(source.as_bytes(), "content.xml").expect("a document");
    let mut styles = styles_of(&content);
    format(
        &mut content,
        &[1, 0, 0],
        0..3,
        Mark::Bold,
        true,
        &mut styles,
    )
    .expect("bold");
    let names: Vec<&str> = content.elements().map(|e| &*e.name.local).collect();
    assert_eq!(names, ["font-face-decls", "automatic-styles", "body"]);
    assert_eq!(
        inside(&content, &[2, 0, 0]),
        r#"<text:span text:style-name="T1">one</text:span>"#
    );
    reads_back(&content);
}

#[test]
fn a_document_that_does_not_declare_the_namespace_is_refused() {
    let source = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:text><text:p>one</text:p></office:text></office:body></office:document-content>"#;
    let original = xml::parse(source.as_bytes(), "content.xml").expect("a document");
    let mut content = original.clone();
    let mut styles = styles_of(&content);
    assert_eq!(
        format(
            &mut content,
            &[0, 0, 0],
            0..3,
            Mark::Bold,
            true,
            &mut styles
        ),
        Err(Refused::Namespace)
    );
    assert!(content == original);
}
