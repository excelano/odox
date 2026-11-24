//! Hyperlinks made, changed and taken off.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::edit::{Refused, followable, make_link, remove_link, set_link, text};
use odox_core::{Element, Ns, xml};

const NAMESPACES: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:xlink="http://www.w3.org/1999/xlink""#;

fn content(body: &str) -> Element {
    let source = format!(
        "<office:document-content {NAMESPACES}><office:body><office:text>{body}</office:text></office:body></office:document-content>"
    );
    xml::parse(source.as_bytes(), "test").expect("a content root")
}

fn written(root: &Element) -> String {
    let body = root.at(&[0, 0]).expect("the text");
    String::from_utf8(xml::serialize(body)).expect("UTF-8")
}

const FIRST: [usize; 3] = [0, 0, 0];

#[test]
fn link_wraps_range_across_spans() {
    let mut root = content(
        r#"<text:p>Read <text:span text:style-name="B">the bold</text:span> news</text:p>"#,
    );
    let link =
        make_link(&mut root, (&FIRST, 9), (&FIRST, 17), "https://example.com").expect("made");
    let out = written(&root);
    assert!(
        out.contains(r#"<text:span text:style-name="B">the </text:span><text:a xlink:type="simple" xlink:href="https://example.com"><text:span text:style-name="B">bold</text:span> new</text:a>s"#),
        "{out}"
    );
    assert_eq!(
        text(root.at(&FIRST).expect("paragraph")),
        "Read the bold news"
    );
    assert!(root.at(&link).is_some_and(|a| a.is(&Ns::Text, "a")));
}

#[test]
fn edit_changes_href_and_text() {
    let mut root = content(
        r#"<text:p>See <text:a xlink:type="simple" xlink:href="https://a.example">here</text:a>.</text:p>"#,
    );
    set_link(&mut root, &[0, 0, 0, 1], "https://b.example", "there").expect("set");
    let out = written(&root);
    assert!(
        out.contains(
            r#"<text:a xlink:type="simple" xlink:href="https://b.example">there</text:a>."#
        ),
        "{out}"
    );
}

#[test]
fn edit_keeps_formatting_when_the_text_stays() {
    let mut root = content(
        r#"<text:p><text:a xlink:type="simple" xlink:href="https://a.example"><text:span text:style-name="B">here</text:span></text:a></text:p>"#,
    );
    set_link(&mut root, &[0, 0, 0, 0], "https://b.example", "here").expect("set");
    assert!(written(&root).contains(r#"<text:span text:style-name="B">here</text:span>"#));
}

#[test]
fn remove_keeps_text_and_formatting() {
    let mut root = content(
        r#"<text:p>See <text:a xlink:type="simple" xlink:href="https://a.example"><text:span text:style-name="B">here</text:span> now</text:a>.</text:p>"#,
    );
    remove_link(&mut root, &[0, 0, 0, 1]).expect("removed");
    let out = written(&root);
    assert!(
        out.contains(
            r#"<text:p>See <text:span text:style-name="B">here</text:span> now.</text:p>"#
        ),
        "{out}"
    );
}

#[test]
fn refuses_range_across_paragraphs() {
    let mut root = content("<text:p>One</text:p><text:p>Two</text:p>");
    let before = root.clone();
    assert_eq!(
        make_link(
            &mut root,
            (&[0, 0, 0], 1),
            (&[0, 0, 1], 2),
            "https://example.com"
        ),
        Err(Refused::Structure)
    );
    assert!(root == before);
}

#[test]
fn refuses_a_range_inside_or_over_a_link() {
    let mut root = content(
        r#"<text:p>See <text:a xlink:type="simple" xlink:href="https://a.example">here</text:a>.</text:p>"#,
    );
    assert_eq!(
        make_link(&mut root, (&FIRST, 2), (&FIRST, 6), "https://example.com"),
        Err(Refused::Structure)
    );
}

#[test]
fn refuses_address_not_followable() {
    let mut root = content("<text:p>Run me</text:p>");
    let before = root.clone();
    for href in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "#",
        "",
        "smb://host/share",
    ] {
        assert_eq!(
            make_link(&mut root, (&FIRST, 0), (&FIRST, 3), href),
            Err(Refused::Address),
            "{href}"
        );
    }
    assert!(root == before);
    assert!(followable("https://example.com"));
    assert!(followable("mailto:me@example.com"));
    assert!(followable("#Chapter 2|outline"));
}

#[test]
fn the_link_at_a_place_is_found_with_its_path() {
    let root = content(
        r#"<text:p>See <text:span text:style-name="B"><text:a xlink:type="simple" xlink:href="https://a.example">here</text:a></text:span> and there</text:p>"#,
    );
    let paragraph = root.at(&FIRST).expect("paragraph");
    let (path, range, href) = odox_core::edit::link_at(paragraph, 5).expect("a link");
    assert_eq!(path, vec![1, 0]);
    assert_eq!(range, 4..8);
    assert_eq!(href, "https://a.example");
    assert!(odox_core::edit::link_at(paragraph, 1).is_none());
    assert!(
        odox_core::edit::link_at(paragraph, 8).is_some(),
        "at its end"
    );
}
