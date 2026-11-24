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
