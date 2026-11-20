//! Every text document in the corpus drawn as the window draws it, with no
//! window: wide, narrow enough that every line wraps, and zoomed.
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::{Path, PathBuf};

use eframe::egui;
use odox_core::doc::TextDocument;
use odox_ui::{Flow, Pictures, fonts};

fn text_documents(directory: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
        if path.is_dir() {
            text_documents(&path, into);
        } else if path.extension().is_some_and(|e| e == "odt") {
            into.push(path);
        }
    }
}

/// The document drawn at a width and a zoom, with the fonts it names, and
/// the height it took.
fn drawn(document: &TextDocument, width: f32, zoom: f32) -> f32 {
    let body = document.body().expect("a body");
    let ctx = egui::Context::default();
    let mut parts = vec![&document.document.content];
    if let Some(styles) = &document.document.styles_part {
        parts.push(styles);
    }
    ctx.set_fonts(fonts::for_document(&parts));
    let mut pictures = Pictures::default();
    let mut height = 0.0;
    for _ in 0..2 {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut flow = Flow::new(&document.document, &mut pictures, zoom);
                let top = ui.cursor().top();
                flow.blocks(ui, body, width);
                height = ui.cursor().top() - top;
            });
        });
        output.textures_delta.clear();
    }
    height
}

#[test]
fn every_text_document_in_the_corpus_draws_at_any_width_and_zoom() {
    let mut paths = Vec::new();
    text_documents(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"),
        &mut paths,
    );
    assert!(paths.len() > 3, "the corpus is where it was");
    for path in paths {
        let bytes = std::fs::read(&path).expect("a corpus document reads");
        let document = TextDocument::read(&bytes).expect("a corpus document parses");
        let wide = drawn(&document, 600.0, 1.0);
        let narrow = drawn(&document, 90.0, 1.0);
        let zoomed = drawn(&document, 600.0, 2.0);
        let name = path.display();
        assert!(wide > 0.0, "{name} drew nothing");
        assert!(narrow >= wide, "{name}: {narrow} narrow, {wide} wide");
        assert!(zoomed > wide, "{name}: {zoomed} zoomed, {wide} at 100%");
    }
}
