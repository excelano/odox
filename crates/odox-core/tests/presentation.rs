//! What a slide inherits from its master page, measured on documents Impress
//! wrote.
//!
//! The three fixtures differ in the way that matters here. `deck.odp` was built
//! from text and has a plain white master and no decorations; `blue-curve.odp`
//! and `focus.odp` are two of the templates LibreOffice ships, and their whole
//! visual identity is in the master page — a gradient band in one, six polygons
//! in the other.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::{Path, PathBuf};

use odox_core::doc::Presentation;
use odox_core::{Color, Fill, GradientStyle, Ns};

fn fixture(name: &str) -> Option<Presentation> {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/libreoffice")
        .join(name);
    // A published crate carries this file and not the corpus beside it.
    let bytes = std::fs::read(path).ok()?;
    Some(Presentation::read(&bytes).expect("a readable presentation"))
}

#[test]
fn a_plain_deck_takes_its_masters_white_ground_and_has_no_decorations() {
    let Some(deck) = fixture("deck.odp") else {
        return;
    };
    let slides = deck.slides();
    let first = &slides[0];

    assert_eq!(first.master_page, Some("Title_20_Slide"));
    assert!(
        deck.master(first).is_some(),
        "the master page was not found"
    );
    // The slide's own drawing-page style states no fill, so the master's does.
    assert_eq!(
        deck.background(first),
        Fill::Solid(Color {
            r: 0xff,
            g: 0xff,
            b: 0xff
        })
    );
    // Every frame on that master carries a presentation class, so every one of
    // them is a slot the slide fills rather than a decoration — including the
    // title frame, which holds the prompt and is not marked a placeholder.
    assert!(deck.background_objects(first).is_empty());
}

#[test]
fn a_template_contributes_its_decorations_and_nothing_it_only_prompts_with() {
    let Some(deck) = fixture("blue-curve.odp") else {
        return;
    };
    let slides = deck.slides();
    let first = &slides[0];

    // The master's own style says `draw:fill="none"`: this template's ground is
    // the paper, and its identity is the shape below.
    assert_eq!(deck.background(first), Fill::None);

    let objects = deck.background_objects(first);
    assert_eq!(objects.len(), 1, "the prompts or the fields were kept");
    assert!(objects[0].is(&Ns::Draw, "custom-shape"));

    // That shape is filled with a named gradient the document defines once.
    let style = objects[0]
        .attr(&Ns::Draw, "style-name")
        .expect("the shape names a graphic style");
    let properties = deck
        .document
        .styles
        .resolve(&odox_core::Family::Graphic, style);
    let Fill::Gradient(name) = properties.graphic.fill() else {
        panic!("expected a gradient, got {:?}", properties.graphic.fill());
    };

    let gradient = deck
        .document
        .styles
        .gradient(&name)
        .expect("the gradient it names");
    assert_eq!(gradient.style, GradientStyle::Linear);
    assert_eq!(
        gradient.start,
        Color {
            r: 0x77,
            g: 0xca,
            b: 0xee
        }
    );
    assert_eq!(
        gradient.end,
        Color {
            r: 0x00,
            g: 0x9b,
            b: 0xdd
        }
    );
    assert!((gradient.angle - 270.0).abs() < 0.01, "{}", gradient.angle);
}

#[test]
fn a_template_of_polygons_hands_over_all_of_them() {
    let Some(deck) = fixture("focus.odp") else {
        return;
    };
    let slides = deck.slides();
    let objects = deck.background_objects(&slides[0]);

    assert_eq!(objects.len(), 6, "not the six polygons the master draws");
    for shape in &objects {
        assert!(shape.is(&Ns::Draw, "polygon"), "{:?}", shape.name.local);
        // Each carries the coordinate space its points are in, which is what
        // maps them into the rectangle the shape occupies on the page.
        assert!(shape.attr(&Ns::Svg, "viewBox").is_some());
        assert!(shape.attr(&Ns::Draw, "points").is_some());
    }
}
