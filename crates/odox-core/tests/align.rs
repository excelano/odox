//! Alignment: what a paragraph is drawn as, and the automatic style written
//! to change it, which leaves every style already there as it was.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::edit::{alignment, set_alignment};
use odox_core::{Element, Ns, Styles, TextAlign, xml};

const NAMESPACES: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0""#;

fn parse(source: &str) -> Element {
    xml::parse(source.as_bytes(), "test").expect("a content root")
}

fn content(automatic: &str, body: &str) -> Element {
    parse(&format!(
        "<office:document-content {NAMESPACES}><office:automatic-styles>{automatic}</office:automatic-styles><office:body><office:text>{body}</office:text></office:body></office:document-content>"
    ))
}

/// The path of a paragraph of the body, which follows the automatic styles
/// when there are some.
fn paragraph(content: &Element, index: usize) -> Vec<usize> {
    let body = content
        .elements_indexed()
        .find(|(_, e)| e.is(&Ns::Office, "body"))
        .map(|(i, _)| i)
        .expect("a body");
    vec![body, 0, index]
}

fn style_of(content: &Element, path: &[usize]) -> String {
    content
        .at(path)
        .and_then(|p| p.attr(&Ns::Text, "style-name"))
        .expect("a style")
        .to_owned()
}

fn automatic<'a>(content: &'a Element, name: &str) -> &'a Element {
    content
        .child(&Ns::Office, "automatic-styles")
        .expect("automatic styles")
        .elements()
        .find(|s| s.attr(&Ns::Style, "name") == Some(name))
        .expect("the style")
}

#[test]
fn a_paragraph_naming_a_style_is_given_one_inheriting_from_it() {
    let mut content = content("", r#"<text:p text:style-name="Standard">a</text:p>"#);
    let mut styles = Styles::collect(Some(&content), None);
    let path = paragraph(&content, 0);
    set_alignment(
        &mut content,
        std::slice::from_ref(&path),
        TextAlign::Center,
        &mut styles,
    )
    .unwrap();
    assert_eq!(alignment(&content, &path, &styles), TextAlign::Center);
    let style = automatic(&content, &style_of(&content, &path));
    assert_eq!(
        style.attr(&Ns::Style, "parent-style-name"),
        Some("Standard")
    );
}

#[test]
fn an_automatic_style_is_copied_and_kept_for_the_others_naming_it() {
    let mut content = content(
        r#"<style:style style:name="P1" style:family="paragraph" style:parent-style-name="Standard"><style:paragraph-properties fo:margin-top="1cm"/></style:style>"#,
        r#"<text:p text:style-name="P1">a</text:p><text:p text:style-name="P1">b</text:p>"#,
    );
    let mut styles = Styles::collect(Some(&content), None);
    let (first, second) = (paragraph(&content, 0), paragraph(&content, 1));
    set_alignment(
        &mut content,
        std::slice::from_ref(&first),
        TextAlign::Justify,
        &mut styles,
    )
    .unwrap();
    assert_eq!(alignment(&content, &first, &styles), TextAlign::Justify);
    assert_eq!(alignment(&content, &second, &styles), TextAlign::Start);
    let name = style_of(&content, &first);
    assert_ne!(name, "P1");
    let properties = automatic(&content, &name)
        .child(&Ns::Style, "paragraph-properties")
        .expect("paragraph properties");
    assert_eq!(properties.attr(&Ns::Fo, "margin-top"), Some("1cm"));
    assert_eq!(properties.attr(&Ns::Fo, "text-align"), Some("justify"));
    let kept = automatic(&content, "P1")
        .child(&Ns::Style, "paragraph-properties")
        .expect("paragraph properties");
    assert_eq!(kept.attr(&Ns::Fo, "text-align"), None);
}

#[test]
fn paragraphs_aligned_alike_share_a_style_and_aligning_again_changes_nothing() {
    let mut content = content("", "<text:p>a</text:p><text:p>b</text:p>");
    let mut styles = Styles::collect(Some(&content), None);
    let paths = vec![paragraph(&content, 0), paragraph(&content, 1)];
    set_alignment(&mut content, &paths, TextAlign::End, &mut styles).unwrap();
    assert_eq!(style_of(&content, &paths[0]), style_of(&content, &paths[1]));
    let before = content.clone();
    set_alignment(&mut content, &paths, TextAlign::End, &mut styles).unwrap();
    assert_eq!(content, before);
}

#[test]
fn a_document_without_automatic_styles_is_given_them_before_its_body() {
    let mut content = parse(&format!(
        "<office:document-content {NAMESPACES}><office:body><office:text><text:p>a</text:p><text:p>b</text:p></office:text></office:body></office:document-content>"
    ));
    let mut styles = Styles::collect(Some(&content), None);
    let paths = vec![paragraph(&content, 0), paragraph(&content, 1)];
    set_alignment(&mut content, &paths, TextAlign::Center, &mut styles).unwrap();
    for index in 0..2 {
        let path = paragraph(&content, index);
        assert_eq!(alignment(&content, &path, &styles), TextAlign::Center);
    }
}

#[test]
fn a_slide_paragraph_says_nothing_and_takes_its_frame_alignment() {
    let content = parse(&format!(
        r#"<office:document-content {NAMESPACES}><office:automatic-styles><style:style style:name="pr1" style:family="presentation"><style:paragraph-properties fo:text-align="center"/></style:style></office:automatic-styles><office:body><office:presentation><draw:page><draw:frame presentation:style-name="pr1"><draw:text-box><text:p>a</text:p></draw:text-box></draw:frame></draw:page></office:presentation></office:body></office:document-content>"#
    ));
    let styles = Styles::collect(Some(&content), None);
    assert_eq!(
        alignment(&content, &[1, 0, 0, 0, 0, 0], &styles),
        TextAlign::Center
    );
}
