//! What a screen reader is told about a page: its paragraphs, and which of
//! them are headings.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{self, accesskit::Role};
use odox_core::doc::TextDocument;
use odox_ui::{Flow, Pictures, fonts};

#[test]
fn a_page_is_read_as_paragraphs_and_headings_with_their_levels() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/text.odt"),
    )
    .expect("the corpus document");
    let document = TextDocument::read(&bytes).expect("it reads");
    let body = document.body().expect("a body");

    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut parts = vec![&document.document.content];
    if let Some(styles) = &document.document.styles_part {
        parts.push(styles);
    }
    ctx.set_fonts(fonts::for_document(&parts));
    let mut pictures = Pictures::default();
    let mut update = None;
    // One pass for the fonts to take effect, then the page itself.
    for _ in 0..2 {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut flow = Flow::new(&document.document, &mut pictures, 1.0);
            flow.blocks(ui, body, 500.0);
        });
        output.textures_delta.clear();
        update = output.platform_output.accesskit_update.take().or(update);
    }
    let update = update.expect("a tree update");

    let headings: Vec<_> = update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == Role::Heading)
        .map(|(_, node)| node.level())
        .collect();
    assert!(
        headings.contains(&Some(1)) && headings.contains(&Some(2)),
        "the sample has a title and second-level headings: {headings:?}"
    );
    assert!(
        update
            .nodes
            .iter()
            .any(|(_, node)| node.role() == Role::Paragraph),
        "and ordinary paragraphs"
    );
    assert!(
        update.nodes.iter().any(|(_, node)| {
            node.role() == Role::TextRun
                && node.value().is_some_and(|v| v.contains("Sample document"))
        }),
        "with their text"
    );
}
