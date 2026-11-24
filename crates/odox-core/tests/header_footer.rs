//! A text document's page header and footer turned on and off.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::doc::TextDocument;
use odox_core::{Element, Ns, xml};

fn blank() -> TextDocument {
    TextDocument::read(&odox_core::blank::text("en-US")).expect("a blank document")
}

fn master(document: &TextDocument) -> &Element {
    document.master_page().expect("a master page")
}

#[test]
fn turning_on_writes_empty_paragraph_in_header_style() {
    let mut document = blank();
    assert!(master(&document).child(&Ns::Style, "header").is_none());
    let content = document.document.content.clone();
    assert!(document.set_header(true).expect("on"));
    let header = master(&document)
        .child(&Ns::Style, "header")
        .expect("a header");
    let paragraph = header.child(&Ns::Text, "p").expect("a paragraph");
    assert!(paragraph.children.is_empty());
    assert!(
        document.document.content == content,
        "the content part is untouched"
    );
    let path = document.band_path(false).expect("its path");
    assert!(
        document
            .document
            .styles_part
            .as_ref()
            .and_then(|part| part.at(&path))
            .is_some_and(|band| band.is(&Ns::Style, "header"))
    );
    assert!(
        !document.set_header(true).expect("still on"),
        "nothing more to do"
    );
    assert!(document.set_footer(true).expect("on"));
    let names: Vec<&str> = master(&document)
        .elements()
        .map(|e| e.name.local.as_ref())
        .collect();
    assert_eq!(names, ["header", "footer"]);
}

#[test]
fn first_and_left_variants_kept_untouched() {
    let mut document = blank();
    let part = document.document.styles_part.as_mut().expect("styles");
    let source = r#"<style:header-first xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><text:p>First</text:p></style:header-first>"#;
    let first = xml::parse(source.as_bytes(), "test").expect("parsed");
    let master = part
        .child_mut(&Ns::Office, "master-styles")
        .and_then(|styles| styles.child_mut(&Ns::Style, "master-page"))
        .expect("a master page");
    master.children.push(odox_core::Node::Element(first));
    master.self_closing = false;
    document.set_header(true).expect("on");
    document.set_header(false).expect("off");
    let master = document.master_page().expect("a master page");
    assert!(master.child(&Ns::Style, "header").is_none());
    assert!(master.child(&Ns::Style, "header-first").is_some());
}

#[test]
fn turning_on_a_header_gives_its_page_layout_room_for_it() {
    let mut document = blank();
    assert!(document.page_layout().header.is_none());
    document.set_header(true).expect("on");
    document.set_footer(true).expect("on");
    assert!(document.page_layout().header.is_some());
    assert!(document.page_layout().footer.is_some());
}

#[test]
fn the_body_master_page_is_the_one_its_first_paragraph_names() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/text.odt"),
    )
    .expect("the fixture");
    let mut document = TextDocument::read(&bytes).expect("it reads");
    assert_eq!(master(&document).attr(&Ns::Style, "name"), Some("HTML"));
    document.set_header(true).expect("on");
    assert!(master(&document).child(&Ns::Style, "header").is_some());
}

#[test]
fn a_header_not_displayed_is_none_until_turned_on() {
    let mut document = blank();
    document.set_header(true).expect("on");
    let path = document.band_path(false).expect("a header");
    let header = document
        .document
        .styles_part
        .as_mut()
        .and_then(|part| part.at_mut(&path))
        .expect("the header");
    let display = header.name_for(&Ns::Style, "display");
    header.set_attr(display, "false");
    assert!(document.band_path(false).is_none());
    assert!(document.set_header(true).expect("on"));
    assert!(document.band_path(false).is_some());
}

#[test]
fn a_band_goes_after_a_master_pages_forms_and_style() {
    let mut document = blank();
    let part = document.document.styles_part.as_mut().expect("styles");
    let source = r#"<style:style xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" style:family="paragraph"/>"#;
    let style = xml::parse(source.as_bytes(), "test").expect("parsed");
    let master = part
        .child_mut(&Ns::Office, "master-styles")
        .and_then(|styles| styles.child_mut(&Ns::Style, "master-page"))
        .expect("a master page");
    master.children.push(odox_core::Node::Element(style));
    master.self_closing = false;
    document.set_footer(true).expect("on");
    let names: Vec<&str> = master_of(&document);
    assert_eq!(names, ["style", "footer"]);
}

fn master_of(document: &TextDocument) -> Vec<&str> {
    master(document)
        .elements()
        .map(|e| e.name.local.as_ref())
        .collect()
}
