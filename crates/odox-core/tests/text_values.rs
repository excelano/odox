//! A family and a size given to a range of text: the span and the automatic
//! style written for them, the font face declaration a family is named by,
//! and a size taken back to the paragraph's own giving back the paragraph.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::edit::{Setting, TextProperty, set_text, text_value};
use odox_core::{Element, Ns, Styles, xml};

const NAMESPACES: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0""#;

fn parse(source: &str) -> Element {
    xml::parse(source.as_bytes(), "test").expect("a root")
}

fn content(before_body: &str, body: &str) -> Element {
    parse(&format!(
        "<office:document-content {NAMESPACES}>{before_body}<office:body><office:text>{body}</office:text></office:body></office:document-content>"
    ))
}

/// A styles part whose default paragraph text is Liberation Serif at 12pt.
fn styles_part() -> Element {
    parse(&format!(
        r#"<office:document-styles {NAMESPACES}><office:font-face-decls><style:font-face style:name="Liberation Serif" svg:font-family="'Liberation Serif'"/></office:font-face-decls><office:styles><style:default-style style:family="paragraph"><style:text-properties style:font-name="Liberation Serif" fo:font-size="12pt"/></style:default-style></office:styles></office:document-styles>"#
    ))
}

fn paragraph(content: &Element) -> Vec<usize> {
    let body = content
        .elements_indexed()
        .find(|(_, e)| e.is(&Ns::Office, "body"))
        .map(|(i, _)| i)
        .expect("a body");
    vec![body, 0, 0]
}

fn value(
    content: &Element,
    styles: &Styles,
    range: std::ops::Range<usize>,
    of: TextProperty,
) -> Option<Setting> {
    text_value(
        content.at(&paragraph(content)).expect("a paragraph"),
        range,
        of,
        styles,
    )
}

#[test]
fn a_size_is_written_for_every_script_and_read_back() {
    let mut content = content("", "<text:p>small big small</text:p>");
    let styles_root = styles_part();
    let mut styles = Styles::collect(Some(&content), Some(&styles_root));
    let path = paragraph(&content);
    set_text(&mut content, &path, 6..9, &Setting::Size(18.0), &mut styles).unwrap();
    assert_eq!(
        value(&content, &styles, 6..9, TextProperty::Size),
        Some(Setting::Size(18.0))
    );
    assert_eq!(
        value(&content, &styles, 0..5, TextProperty::Size),
        Some(Setting::Size(12.0))
    );
    assert_eq!(value(&content, &styles, 0..9, TextProperty::Size), None);
    let style = content
        .child(&Ns::Office, "automatic-styles")
        .and_then(|a| a.elements().next())
        .and_then(|s| s.child(&Ns::Style, "text-properties"))
        .expect("a text style");
    for (ns, local) in [
        (Ns::Fo, "font-size"),
        (Ns::Style, "font-size-asian"),
        (Ns::Style, "font-size-complex"),
    ] {
        assert_eq!(style.attr(&ns, local), Some("18pt"));
    }
}

#[test]
fn a_size_put_back_to_the_paragraph_own_gives_back_the_paragraph() {
    let mut content = content("", "<text:p>one two three</text:p>");
    let styles_root = styles_part();
    let mut styles = Styles::collect(Some(&content), Some(&styles_root));
    let path = paragraph(&content);
    let before = content.at(&path).cloned();
    set_text(&mut content, &path, 4..7, &Setting::Size(10.5), &mut styles).unwrap();
    let path = paragraph(&content);
    assert_ne!(content.at(&path).cloned(), before);
    set_text(&mut content, &path, 4..7, &Setting::Size(12.0), &mut styles).unwrap();
    assert_eq!(content.at(&paragraph(&content)).cloned(), before);
}

#[test]
fn a_family_is_declared_before_the_automatic_styles_and_named_by_it() {
    let mut content = content("", "<text:p>plain fancy</text:p>");
    let styles_root = styles_part();
    let mut styles = Styles::collect(Some(&content), Some(&styles_root));
    let path = paragraph(&content);
    set_text(
        &mut content,
        &path,
        6..11,
        &Setting::Family("DejaVu Sans".into()),
        &mut styles,
    )
    .unwrap();
    let order: Vec<&str> = content.elements().map(|e| e.name.local.as_ref()).collect();
    assert_eq!(order, ["font-face-decls", "automatic-styles", "body"]);
    let face = content
        .child(&Ns::Office, "font-face-decls")
        .and_then(|d| d.elements().next())
        .expect("a declaration");
    assert_eq!(face.attr(&Ns::Svg, "font-family"), Some("'DejaVu Sans'"));
    assert_eq!(
        value(&content, &styles, 6..11, TextProperty::Family),
        Some(Setting::Family("DejaVu Sans".into()))
    );
    assert_eq!(
        value(&content, &styles, 0..5, TextProperty::Family),
        Some(Setting::Family("Liberation Serif".into()))
    );
}

#[test]
fn a_family_already_declared_is_named_by_its_declaration() {
    let mut content = content(
        r#"<office:font-face-decls><style:font-face style:name="Sans1" svg:font-family="'DejaVu Sans'"/></office:font-face-decls>"#,
        "<text:p>plain fancy</text:p>",
    );
    let styles_root = styles_part();
    let mut styles = Styles::collect(Some(&content), Some(&styles_root));
    let path = paragraph(&content);
    set_text(
        &mut content,
        &path,
        6..11,
        &Setting::Family("DejaVu Sans".into()),
        &mut styles,
    )
    .unwrap();
    let declarations = content
        .child(&Ns::Office, "font-face-decls")
        .expect("declarations");
    assert_eq!(declarations.elements().count(), 1);
    let text = content
        .child(&Ns::Office, "automatic-styles")
        .and_then(|a| a.elements().next())
        .and_then(|s| s.child(&Ns::Style, "text-properties"))
        .expect("a text style");
    assert_eq!(text.attr(&Ns::Style, "font-name"), Some("Sans1"));
}
