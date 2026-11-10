//! Page headers and footers: which one a page shows, and its fields put in
//! for that page, against `corpus/libreoffice/pages.odt`.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use odox_core::doc::{PageFields, TextDocument, page_band, with_page_fields};

fn pages() -> Option<TextDocument> {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/pages.odt"),
    )
    .ok()?;
    Some(TextDocument::read(&bytes).expect("it reads"))
}

#[test]
fn the_first_page_has_its_own_header_and_the_rest_fill_in_their_number() {
    let Some(document) = pages() else {
        return;
    };
    let master = document.master_page().expect("a master page");
    let shown = |footer: bool, page: usize| {
        let band = page_band(master, footer, page)?;
        let fields = PageFields {
            page,
            pages: 4,
            sheet: None,
            title: "Quarterly report",
            today: (2026, 1, 1),
        };
        Some(with_page_fields(band, &fields, &document.document.styles).plain_text())
    };
    assert_eq!(shown(false, 1).as_deref(), Some("First page"));
    assert_eq!(
        shown(false, 3).as_deref(),
        Some("Quarterly report, page 3 of 4")
    );
    assert_eq!(shown(true, 1).as_deref(), Some(""), "an empty first footer");
    assert_eq!(shown(true, 2).as_deref(), Some("Confidential"));
    let layout = document.page_layout();
    assert!(layout.header.is_some() && layout.footer.is_some());
}
