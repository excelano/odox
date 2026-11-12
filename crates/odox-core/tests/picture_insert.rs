//! A picture put into a text document: what its header says, the part it is
//! kept in, and where its paragraph and the caret go.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::edit::{Description, NewPicture, description, insert_picture};
use odox_core::{Element, Ns, Package, Styles, picture, xml};

const NAMESPACES: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:loext="urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0""#;

fn content(body: &str) -> Element {
    xml::parse(
        format!(
            "<office:document-content {NAMESPACES}><office:automatic-styles/><office:body><office:text>{body}</office:text></office:body></office:document-content>"
        )
        .as_bytes(),
        "test",
    )
    .expect("a content root")
}

fn picture(description: Option<&str>) -> NewPicture<'_> {
    NewPicture {
        href: "Pictures/a.png",
        media_type: "image/png",
        size: (144.0, 72.0),
        description,
    }
}

/// The text of each element of the body, a frame's paragraph as `[picture]`.
fn body(content: &Element) -> Vec<String> {
    content
        .child(&Ns::Office, "body")
        .and_then(|b| b.child(&Ns::Office, "text"))
        .expect("a body")
        .elements()
        .map(|p| {
            if p.child(&Ns::Draw, "frame").is_some() {
                "[picture]".to_owned()
            } else {
                p.plain_text()
            }
        })
        .collect()
}

const PNG: [u8; 24] = [
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 1,
    0x2C, 0, 0, 0, 0x96,
];

#[test]
fn a_png_and_a_jpeg_say_what_they_are_and_how_large() {
    assert_eq!(picture::kind(&PNG), Some(("image/png", "png")));
    assert_eq!(picture::pixel_size(&PNG), Some((300, 150)));
    let jpeg = [
        0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0, 0xFF, 0xC0, 0, 11, 8, 0, 100, 0, 200, 3, 0, 0, 0,
    ];
    assert_eq!(picture::kind(&jpeg), Some(("image/jpeg", "jpg")));
    assert_eq!(picture::pixel_size(&jpeg), Some((200, 100)));
    assert_eq!(picture::kind(b"GIF89a"), None);
}

#[test]
fn the_same_picture_is_kept_once_and_listed_in_the_manifest() {
    let mut package = Package::read(&odox_core::blank::text("en-US")).expect("a package");
    let first = package.add_picture(&PNG, "png", "image/png");
    let second = package.add_picture(&PNG, "png", "image/png");
    assert_eq!(first, second);
    assert!(first.starts_with("Pictures/") && first.ends_with(".png"));
    let manifest = String::from_utf8(
        package
            .part("META-INF/manifest.xml")
            .expect("a manifest")
            .data
            .clone(),
    )
    .expect("text");
    assert_eq!(manifest.matches(&first).count(), 1);
}

#[test]
fn a_picture_goes_after_the_caret_paragraph_and_the_caret_to_the_next() {
    let mut content = content("<text:p>a</text:p><text:p>b</text:p>");
    let mut styles = Styles::collect(Some(&content), None);
    let caret = insert_picture(
        &mut content,
        &[1, 0, 0],
        &picture(Some("A chart")),
        &mut styles,
    )
    .expect("it goes in");
    assert_eq!(body(&content), ["a", "[picture]", "b"]);
    assert_eq!(caret, [1, 0, 2]);
    let frame = content.at(&[1, 0, 1, 0]).expect("the frame");
    assert_eq!(frame.attr(&Ns::Text, "anchor-type"), Some("as-char"));
    assert_eq!(frame.attr(&Ns::Svg, "width"), Some("5.080cm"));
    assert_eq!(
        frame
            .child(&Ns::Svg, "title")
            .map(Element::plain_text)
            .as_deref(),
        Some("A chart")
    );
}

#[test]
fn an_empty_paragraph_is_left_below_the_picture_and_a_last_one_gets_a_follower() {
    let mut content = content("<text:p/>");
    let mut styles = Styles::collect(Some(&content), None);
    let caret =
        insert_picture(&mut content, &[1, 0, 0], &picture(None), &mut styles).expect("it goes in");
    assert_eq!(body(&content), ["[picture]", ""]);
    let frame_path = [1, 0, 0, 0];
    assert_eq!(caret, [1, 0, 1]);
    assert_eq!(
        description(content.at(&frame_path).expect("the frame"), &styles),
        Description::Decorative
    );

    let mut content = self::content("<text:p>last</text:p>");
    let mut styles = Styles::collect(Some(&content), None);
    let caret = insert_picture(&mut content, &[1, 0, 0], &picture(Some("")), &mut styles)
        .expect("it goes in");
    assert_eq!(body(&content), ["last", "[picture]", ""]);
    assert_eq!(caret, [1, 0, 2]);
}
