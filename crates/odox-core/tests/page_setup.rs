//! Format > Page setup…: the paper, its orientation and its margins, changed
//! in the body's page layout, each length in the unit the layout writes it in.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::doc::{PageSetup, TextDocument};
use odox_core::{Length, Ns};

fn corpus() -> TextDocument {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/libreoffice/text.odt"
    ))
    .expect("the corpus document");
    TextDocument::read(&bytes).expect("it reads")
}

/// An attribute of the body's page layout properties, as written.
fn properties(document: &TextDocument, local: &str) -> Option<String> {
    let properties = document
        .document
        .styles_part
        .as_ref()?
        .child(&Ns::Office, "automatic-styles")?
        .elements()
        .find(|e| e.attr(&Ns::Style, "name") == Some("Mpm1"))?
        .child(&Ns::Style, "page-layout-properties")?;
    properties
        .attr(&Ns::Fo, local)
        .or_else(|| properties.attr(&Ns::Style, local))
        .map(ToOwned::to_owned)
}

#[test]
fn a_page_turned_landscape_with_wider_margins_is_written_in_its_own_unit() {
    let mut document = corpus();
    let before = document.page_setup();
    assert!(!before.landscape());
    let inch = Length(72.0);
    let setup = PageSetup {
        width: before.height,
        height: before.width,
        margins: [inch, inch, inch, Length(108.0)],
    };
    assert_eq!(document.set_page_setup(&setup), Ok(true));
    assert_eq!(document.page_setup(), setup);
    assert!((document.text_width() - (11.0 * 72.0 - 72.0 - 108.0)).abs() < 0.01);
    assert_eq!(properties(&document, "page-width").as_deref(), Some("11in"));
    assert_eq!(
        properties(&document, "margin-left").as_deref(),
        Some("1.5in")
    );
    assert_eq!(
        properties(&document, "print-orientation").as_deref(),
        Some("landscape")
    );

    let bytes = document.document.write_verified().expect("it writes");
    let read_back = TextDocument::read(&bytes).expect("it reads back");
    assert_eq!(read_back.page_setup(), setup);
}

#[test]
fn the_same_setup_changes_nothing() {
    let mut document = corpus();
    let styles = document.document.styles_part.clone();
    let setup = document.page_setup();
    assert_eq!(document.set_page_setup(&setup), Ok(false));
    assert_eq!(document.document.styles_part, styles);
}
