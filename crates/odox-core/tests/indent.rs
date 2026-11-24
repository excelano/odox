//! Indents and line spacing: written into automatic styles as an alignment
//! is, and a list item indented by its level.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::edit::{LineSpacing, line_spacing, set_indent, set_line_spacing};
use odox_core::{Element, Family, Length, Ns, Styles, xml};

const NAMESPACES: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0""#;

fn content(body: &str) -> Element {
    let source = format!(
        "<office:document-content {NAMESPACES}><office:automatic-styles/><office:body><office:text>{body}</office:text></office:body></office:document-content>"
    );
    xml::parse(source.as_bytes(), "test").expect("a content root")
}

fn named(styles: &str) -> Element {
    let source = format!(
        "<office:document-styles {NAMESPACES}><office:styles>{styles}</office:styles></office:document-styles>"
    );
    xml::parse(source.as_bytes(), "test").expect("a styles root")
}

const ONE_CM_TABS: &str = r#"<style:default-style style:family="paragraph"><style:paragraph-properties style:tab-stop-distance="1cm"/></style:default-style><style:style style:name="Body" style:family="paragraph"/>"#;

fn left(content: &Element, path: &[usize], styles: &Styles) -> f32 {
    let name = content
        .at(path)
        .and_then(|p| p.attr(&Ns::Text, "style-name"))
        .unwrap_or("Body");
    styles
        .resolve(&Family::Paragraph, name)
        .paragraph
        .margin
        .left
        .map_or(0.0, Length::points)
}

fn cm(cm: f32) -> f32 {
    cm / 2.54 * 72.0
}

#[test]
fn indent_steps_by_tab_distance() {
    let mut root = content(r#"<text:p text:style-name="Body">Text</text:p>"#);
    let mut styles = Styles::collect(Some(&root), Some(&named(ONE_CM_TABS)));
    let path = vec![1, 0, 0];
    set_indent(&mut root, std::slice::from_ref(&path), 1, &mut styles).expect("in");
    set_indent(&mut root, std::slice::from_ref(&path), 1, &mut styles).expect("in");
    assert!((left(&root, &path, &styles) - cm(2.0)).abs() < 0.05);
}

#[test]
fn outdent_stops_at_zero() {
    let mut root = content(r#"<text:p text:style-name="Body">Text</text:p>"#);
    let mut styles = Styles::collect(Some(&root), Some(&named(ONE_CM_TABS)));
    let path = vec![1, 0, 0];
    set_indent(&mut root, std::slice::from_ref(&path), 1, &mut styles).expect("in");
    set_indent(&mut root, std::slice::from_ref(&path), -1, &mut styles).expect("out");
    set_indent(&mut root, std::slice::from_ref(&path), -1, &mut styles).expect("out");
    assert!(left(&root, &path, &styles).abs() < 0.05);
}

#[test]
fn indent_in_list_item_changes_level() {
    let mut root = content(
        "<text:list><text:list-item><text:p>One</text:p></text:list-item><text:list-item><text:p>Two</text:p></text:list-item></text:list>",
    );
    let mut styles = Styles::collect(Some(&root), None);
    set_indent(&mut root, &[vec![1, 0, 0, 1, 0]], 1, &mut styles).expect("in");
    let one = root.at(&[1, 0, 0, 0]).expect("the first item");
    assert!(one.child(&Ns::Text, "list").is_some(), "Two went into One");
}

#[test]
fn line_spacing_written_on_automatic_style() {
    let mut root = content(r#"<text:p text:style-name="Body">Text</text:p>"#);
    let mut styles = Styles::collect(Some(&root), Some(&named(ONE_CM_TABS)));
    let path = vec![1, 0, 0];
    set_line_spacing(
        &mut root,
        std::slice::from_ref(&path),
        LineSpacing::OnePointFive,
        &mut styles,
    )
    .expect("spaced");
    let name = root
        .at(&path)
        .and_then(|p| p.attr(&Ns::Text, "style-name"))
        .expect("a style");
    assert_ne!(name, "Body", "the named style is left as it is");
    let written = String::from_utf8(xml::serialize(&root)).expect("UTF-8");
    assert!(written.contains(r#"fo:line-height="150%""#), "{written}");
    assert!(
        written.contains(r#"style:parent-style-name="Body""#),
        "{written}"
    );
}

#[test]
fn reads_back_current_spacing() {
    let mut root = content(r#"<text:p text:style-name="Body">Text</text:p>"#);
    let mut styles = Styles::collect(Some(&root), Some(&named(ONE_CM_TABS)));
    let path = vec![1, 0, 0];
    assert_eq!(
        line_spacing(&root, &path, &styles),
        Some(LineSpacing::Single)
    );
    set_line_spacing(
        &mut root,
        std::slice::from_ref(&path),
        LineSpacing::Double,
        &mut styles,
    )
    .expect("spaced");
    assert_eq!(
        line_spacing(&root, &path, &styles),
        Some(LineSpacing::Double)
    );
}
