//! The language a run of text is in, which a spell check reads.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::doc::TextDocument;
use odox_core::{Family, Styles};

fn styles() -> Styles {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/text.odt"),
    )
    .expect("the fixture");
    TextDocument::read(&bytes)
        .expect("it reads")
        .document
        .styles
}

#[test]
fn a_paragraph_is_in_the_language_its_styles_name() {
    let styles = styles();
    let resolved = styles.resolve(&Family::Paragraph, "Standard");
    assert_eq!(resolved.text.language.as_deref(), Some("en-US"));
}
