//! A picture put in through the page's model: one step to undo, which takes
//! the picture's part and its manifest entry back out with the frame, and puts
//! them back on redo.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use egui_richedit::{Model, Position, Selection};
use odox_core::Document;
use odox_core::doc::TextDocument;
use odox_ui::{Editing, FlowModel, Picture, page_editor};

const PNG: [u8; 24] = [
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 0,
    8, 0, 0, 0, 4,
];

fn manifest(document: &Document) -> String {
    String::from_utf8(
        document
            .package
            .part("META-INF/manifest.xml")
            .expect("a manifest")
            .data
            .clone(),
    )
    .expect("text")
}

#[test]
fn a_picture_put_in_is_one_step_that_takes_its_part_back_out() {
    let blank = odox_core::blank::text("en-US");
    let mut document = TextDocument::read(&blank).expect("a blank document");
    let root = document.body_path().expect("a body");
    let content_before = document.document.content.clone();
    let mut editing = Editing::default();
    let mut editor = page_editor();
    {
        let Document {
            content,
            styles,
            package,
            ..
        } = &mut document.document;
        let mut model = FlowModel::new(content, styles, root, &mut editing);
        let first = model.first().expect("a paragraph");
        editor.select(Selection::caret(Position::new(first, 0)));
        let picture = Picture {
            bytes: PNG.to_vec(),
            media_type: "image/png",
            extension: "png",
            size: (6.0, 3.0),
            description: Some("A small picture".to_owned()),
        };
        model
            .insert_picture(&mut editor, package, &picture)
            .expect("it goes in");
    }
    let name = document.document.package.picture_name(&PNG, "png");
    assert!(document.document.package.part(&name).is_some());
    assert!(manifest(&document.document).contains(&name));

    editing.undo(&mut document.document, None).expect("a step");
    assert_eq!(document.document.content, content_before);
    assert!(document.document.package.part(&name).is_none());
    assert!(!manifest(&document.document).contains(&name));

    editing.redo(&mut document.document, None).expect("a step");
    assert!(document.document.package.part(&name).is_some());
    assert!(manifest(&document.document).contains(&name));
}
