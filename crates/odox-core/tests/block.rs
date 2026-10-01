//! Headings and lists: what a paragraph is made into, and what it is made
//! back into. None of it adds or removes a paragraph, and what is not touched
//! is the tree it was.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::edit::{block_state, heading_level, set_heading, set_list, text};
use odox_core::{Element, ListKind, Ns, Refused, Styles, xml};

const NAMESPACES: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0""#;

/// A content root with the given automatic styles and body text.
fn content(automatic: &str, body: &str) -> Element {
    let source = format!(
        "<office:document-content {NAMESPACES}><office:automatic-styles>{automatic}</office:automatic-styles><office:body><office:text>{body}</office:text></office:body></office:document-content>"
    );
    xml::parse(source.as_bytes(), "test").expect("a content root")
}

/// The same with no automatic styles at all.
fn bare(body: &str) -> Element {
    let source = format!(
        "<office:document-content {NAMESPACES}><office:body><office:text>{body}</office:text></office:body></office:document-content>"
    );
    xml::parse(source.as_bytes(), "test").expect("a content root")
}

fn styles_part(styles: &str) -> Element {
    let source = format!(
        "<office:document-styles {NAMESPACES}><office:styles>{styles}</office:styles></office:document-styles>"
    );
    xml::parse(source.as_bytes(), "test").expect("a styles root")
}

/// Where the body's text sits, which is after the automatic styles when
/// there are some.
fn body_path(content: &Element) -> Vec<usize> {
    let body = content
        .elements_indexed()
        .find(|(_, e)| e.is(&Ns::Office, "body"))
        .map(|(i, _)| i)
        .expect("a body");
    let text = content
        .at(&[body])
        .expect("an element")
        .elements_indexed()
        .find(|(_, e)| e.is(&Ns::Office, "text"))
        .map(|(i, _)| i)
        .expect("a text");
    vec![body, text]
}

fn paragraph_path(content: &Element, index: usize) -> Vec<usize> {
    let mut path = body_path(content);
    path.push(index);
    path
}

fn serialized(content: &Element) -> String {
    String::from_utf8(xml::serialize(content)).expect("UTF-8")
}

#[test]
fn a_heading_in_a_document_with_no_heading_style_gets_an_automatic_one() {
    let mut root = content("", "<text:p>Title</text:p><text:p>Body</text:p>");
    let mut styles = Styles::collect(Some(&root), None);
    let at = paragraph_path(&root, 0);
    set_heading(&mut root, &at, Some(1), &mut styles).expect("it is made");
    let heading = root.at(&at).expect("still there");
    assert!(heading.is(&Ns::Text, "h"));
    assert_eq!(heading.attr(&Ns::Text, "outline-level"), Some("1"));
    assert_eq!(heading.attr(&Ns::Text, "style-name"), Some("P1"));
    assert_eq!(text(heading), "Title");
    let written = serialized(&root);
    assert!(written.contains(r#"<style:style style:name="P1" style:family="paragraph">"#));
    assert!(written.contains(r#"fo:font-size="18pt""#));
    assert!(styles.has_style(&odox_core::Family::Paragraph, "P1"));
}

#[test]
fn a_second_heading_of_the_same_level_shares_the_style() {
    let mut root = content("", "<text:p>One</text:p><text:p>Two</text:p>");
    let mut styles = Styles::collect(Some(&root), None);
    for index in 0..2 {
        let at = paragraph_path(&root, index);
        set_heading(&mut root, &at, Some(2), &mut styles).expect("made");
    }
    assert_eq!(serialized(&root).matches("<style:style ").count(), 1);
    let first = root.at(&paragraph_path(&root, 0)).expect("one");
    let second = root.at(&paragraph_path(&root, 1)).expect("two");
    assert_eq!(
        first.attr(&Ns::Text, "style-name"),
        second.attr(&Ns::Text, "style-name")
    );
}

#[test]
fn a_heading_takes_the_style_the_document_gives_its_level() {
    let named = styles_part(
        r#"<style:style style:name="Heading_20_1" style:family="paragraph"/><style:style style:name="Chapter" style:family="paragraph" style:default-outline-level="2"/>"#,
    );
    let mut root = bare("<text:p>One</text:p><text:p>Two</text:p>");
    let mut styles = Styles::collect(Some(&root), Some(&named));
    let before = serialized(&root);
    let one = paragraph_path(&root, 0);
    let two = paragraph_path(&root, 1);
    set_heading(&mut root, &one, Some(1), &mut styles).expect("made");
    set_heading(&mut root, &two, Some(2), &mut styles).expect("made");
    assert_eq!(
        root.at(&one).and_then(|e| e.attr(&Ns::Text, "style-name")),
        Some("Heading_20_1")
    );
    assert_eq!(
        root.at(&two).and_then(|e| e.attr(&Ns::Text, "style-name")),
        Some("Chapter"),
        "the style that declares the level wins over the conventional name"
    );
    assert!(
        !serialized(&root).contains("automatic-styles"),
        "nothing was written, so nothing was added to {before}"
    );
}

#[test]
fn taking_a_heading_off_gives_the_style_most_paragraphs_have() {
    let mut root = content(
        "",
        r#"<text:h text:outline-level="1" text:style-name="Heading_20_1">Title</text:h><text:p text:style-name="Body">a</text:p><text:p text:style-name="Body">b</text:p><text:p>c</text:p>"#,
    );
    let mut styles = Styles::collect(Some(&root), None);
    let at = paragraph_path(&root, 0);
    set_heading(&mut root, &at, None, &mut styles).expect("made");
    let paragraph = root.at(&at).expect("there");
    assert!(paragraph.is(&Ns::Text, "p"));
    assert_eq!(paragraph.attr(&Ns::Text, "outline-level"), None);
    assert_eq!(paragraph.attr(&Ns::Text, "style-name"), Some("Body"));
}

#[test]
fn a_paragraph_that_is_already_what_is_asked_for_is_left_alone() {
    let mut root = content("", "<text:p>x</text:p>");
    let mut styles = Styles::collect(Some(&root), None);
    let before = root.clone();
    let at = paragraph_path(&root, 0);
    set_heading(&mut root, &at, None, &mut styles).expect("nothing to do");
    assert_eq!(root, before);
}

#[test]
fn a_heading_keeps_what_is_inside_the_paragraph() {
    let mut root = content(
        "",
        "<text:p>a <text:span text:style-name=\"T1\">b</text:span> c</text:p>",
    );
    let mut styles = Styles::collect(Some(&root), None);
    let at = paragraph_path(&root, 0);
    let before = root.at(&at).expect("there").children.clone();
    set_heading(&mut root, &at, Some(3), &mut styles).expect("made");
    assert_eq!(root.at(&at).expect("there").children, before);
}

#[test]
fn a_document_without_the_namespaces_is_refused_and_unchanged() {
    let source = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:text><text:p>x</text:p></office:text></office:body></office:document-content>"#;
    let mut root = xml::parse(source.as_bytes(), "test").expect("parses");
    let mut styles = Styles::collect(Some(&root), None);
    let before = root.clone();
    assert_eq!(
        set_heading(&mut root, &[0, 0, 0], Some(1), &mut styles),
        Err(Refused::Namespace)
    );
    assert_eq!(root, before);
}

#[test]
fn paragraphs_side_by_side_become_one_list_and_come_back() {
    let compact = "<text:p>a</text:p><text:p>b</text:p><text:p>c</text:p><text:p>after</text:p>";
    let mut root = content("", compact);
    let mut styles = Styles::collect(Some(&root), None);
    let paths: Vec<_> = (0..3).map(|i| paragraph_path(&root, i)).collect();
    set_list(&mut root, &paths, Some(ListKind::Number), &mut styles).expect("made");

    let written = serialized(&root);
    assert!(written.contains(r#"<text:list text:style-name="L1"><text:list-item><text:p>a</text:p></text:list-item><text:list-item><text:p>b</text:p></text:list-item><text:list-item><text:p>c</text:p></text:list-item></text:list><text:p>after</text:p>"#));
    assert!(written.contains(
        r#"<text:list-level-style-number text:level="1" style:num-suffix="." style:num-format="1">"#
    ));
    let state = |index: usize, root: &Element, styles: &Styles| {
        let mut path = body_path(root);
        path.extend([0, index, 0]);
        block_state(root, &path, styles)
    };
    assert_eq!(state(1, &root, &styles), (None, Some(ListKind::Number)));

    // Every paragraph is in the list now, wherever it moved to.
    let moved: Vec<_> = (0..3)
        .map(|i| {
            let mut path = body_path(&root);
            path.extend([0, i, 0]);
            path
        })
        .collect();
    set_list(&mut root, &moved, None, &mut styles).expect("taken out");
    let restored = serialized(&root);
    assert!(restored.contains(compact), "{restored}");
    assert_eq!(
        heading_level(root.at(&paragraph_path(&root, 0)).expect("a")),
        None
    );
}

#[test]
fn a_bulleted_list_reuses_the_style_the_document_has() {
    let automatic = r#"<text:list-style style:name="WW8Num1"><text:list-level-style-bullet text:level="1" text:bullet-char="-"/></text:list-style>"#;
    let mut root = content(automatic, "<text:p>a</text:p>");
    let mut styles = Styles::collect(Some(&root), None);
    let paths = vec![paragraph_path(&root, 0)];
    set_list(&mut root, &paths, Some(ListKind::Bullet), &mut styles).expect("made");
    let written = serialized(&root);
    assert!(written.contains(r#"<text:list text:style-name="WW8Num1">"#));
    assert_eq!(written.matches("<text:list-style ").count(), 1);
}

#[test]
fn a_list_of_the_other_kind_is_given_the_requested_style() {
    let bullets = r#"<text:list-style style:name="L1"><text:list-level-style-bullet text:level="1" text:bullet-char="x"/></text:list-style>"#;
    let numbers = r#"<text:list-style style:name="L2"><text:list-level-style-number text:level="1" style:num-format="1"/></text:list-style>"#;
    let body = r#"<text:list text:style-name="L1"><text:list-item><text:p>a</text:p></text:list-item><text:list-item><text:p>b</text:p></text:list-item></text:list>"#;
    let mut root = content(&format!("{bullets}{numbers}"), body);
    let mut styles = Styles::collect(Some(&root), None);
    let mut path = body_path(&root);
    path.extend([0, 1, 0]);
    set_list(
        &mut root,
        &[path.clone()],
        Some(ListKind::Number),
        &mut styles,
    )
    .expect("switched");
    assert_eq!(
        block_state(&root, &path, &styles),
        (None, Some(ListKind::Number))
    );
    assert!(serialized(&root).contains(r#"<text:list text:style-name="L2">"#));
}

#[test]
fn taking_a_middle_item_out_splits_the_list_around_it() {
    let body = r#"<text:list text:style-name="L1"><text:list-item><text:p>a</text:p></text:list-item><text:list-item><text:p>b</text:p></text:list-item><text:list-item><text:p>c</text:p></text:list-item></text:list>"#;
    let automatic = r#"<text:list-style style:name="L1"><text:list-level-style-bullet text:level="1" text:bullet-char="x"/></text:list-style>"#;
    let mut root = content(automatic, body);
    let mut styles = Styles::collect(Some(&root), None);
    let mut path = body_path(&root);
    path.extend([0, 1, 0]);
    set_list(&mut root, &[path], None, &mut styles).expect("out");
    let written = serialized(&root);
    assert!(written.contains(r#"<text:list text:style-name="L1"><text:list-item><text:p>a</text:p></text:list-item></text:list><text:p>b</text:p><text:list text:style-name="L1" text:continue-numbering="true"><text:list-item><text:p>c</text:p></text:list-item></text:list>"#), "{written}");
}

#[test]
fn an_item_that_holds_a_nested_list_leaves_it_beside_the_paragraph() {
    let body = r#"<text:list text:style-name="L1"><text:list-item><text:p>a</text:p><text:list><text:list-item><text:p>nested</text:p></text:list-item></text:list></text:list-item></text:list>"#;
    let automatic = r#"<text:list-style style:name="L1"><text:list-level-style-bullet text:level="1" text:bullet-char="x"/></text:list-style>"#;
    let mut root = content(automatic, body);
    let mut styles = Styles::collect(Some(&root), None);
    let mut path = body_path(&root);
    path.extend([0, 0, 0]);
    set_list(&mut root, &[path], None, &mut styles).expect("out");
    assert!(serialized(&root).contains(
        r#"</office:automatic-styles><office:body><office:text><text:p>a</text:p><text:list><text:list-item><text:p>nested</text:p></text:list-item></text:list></office:text>"#
    ));
}

#[test]
fn a_list_written_into_a_document_with_nowhere_to_keep_styles_moves_the_body() {
    let mut root = bare("<text:p>a</text:p>");
    let mut styles = Styles::collect(Some(&root), None);
    let at = paragraph_path(&root, 0);
    set_list(&mut root, &[at], Some(ListKind::Bullet), &mut styles).expect("made");
    let moved = body_path(&root);
    assert_eq!(moved[0], 1, "the automatic styles now come first");
    assert!(
        root.at(&[1, 0, 0, 0, 0])
            .is_some_and(|e| e.is(&Ns::Text, "p"))
    );
}

#[test]
fn a_path_that_is_not_a_paragraph_is_refused() {
    let mut root = content("", "<text:p>a</text:p>");
    let mut styles = Styles::collect(Some(&root), None);
    let body = body_path(&root);
    assert_eq!(
        set_list(&mut root, &[body], Some(ListKind::Bullet), &mut styles),
        Err(Refused::NotFound)
    );
}

#[test]
fn paragraphs_with_layout_whitespace_between_them_are_still_one_list() {
    let mut root = content(
        "",
        "<text:p>a</text:p>
  <text:p>b</text:p>",
    );
    let mut styles = Styles::collect(Some(&root), None);
    let paths = vec![paragraph_path(&root, 0), paragraph_path(&root, 2)];
    set_list(&mut root, &paths, Some(ListKind::Bullet), &mut styles).expect("made");
    let written = serialized(&root);
    assert_eq!(written.matches("<text:list ").count(), 1, "{written}");
    assert_eq!(written.matches("<text:list-item>").count(), 2);
}

#[test]
fn a_document_edited_into_headings_and_lists_saves_and_reads_back() {
    use odox_core::doc::TextDocument;
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/text.odt"),
    )
    .expect("the corpus document");
    let mut document = TextDocument::read(&bytes).expect("it reads");
    let root = document.body_path().expect("a body");
    let paragraphs: Vec<Vec<usize>> = {
        let body = document.document.content.at(&root).expect("the body");
        body.elements_indexed()
            .filter(|(_, e)| e.is(&Ns::Text, "p"))
            .map(|(index, _)| [root.as_slice(), &[index]].concat())
            .collect()
    };
    assert!(paragraphs.len() >= 3, "the sample has paragraphs to edit");
    let content = &mut document.document.content;
    let styles = &mut document.document.styles;
    set_heading(content, &paragraphs[0], Some(2), styles).expect("heading");
    set_list(content, &paragraphs[1..3], Some(ListKind::Number), styles).expect("list");

    let written = document
        .document
        .write_verified()
        .expect("it comes back the same");
    let reread = TextDocument::read(&written).expect("it reads again");
    let outline: Vec<_> = reread.outline().iter().map(|h| h.text.clone()).collect();
    assert!(outline.len() >= 5, "{outline:?}");
}
