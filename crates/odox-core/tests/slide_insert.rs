//! A text box and a picture put on a slide.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use odox_core::doc::{Presentation, SlidePicture};
use odox_core::{Element, Ns};

fn deck() -> Presentation {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/deck.odp"),
    )
    .expect("the deck");
    Presentation::read(&bytes).expect("it reads")
}

fn png() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/icons/xodt-rounded-512.png"),
    )
    .expect("a picture")
}

fn length(element: &Element, local: &str) -> f32 {
    element
        .attr(&Ns::Svg, local)
        .and_then(odox_core::Length::parse)
        .map_or(-1.0, odox_core::Length::points)
}

#[test]
fn text_box_frame_with_empty_paragraph() {
    let mut deck = deck();
    let position = deck.slides()[1].position;
    let path = deck
        .insert_text_box(position, (72.0, 144.0, 283.5, 56.7))
        .expect("put in");
    let frame = deck.document.content.at(&path).expect("the frame");
    assert!(frame.is(&Ns::Draw, "frame"));
    assert!((length(frame, "x") - 72.0).abs() < 0.1);
    assert!((length(frame, "width") - 283.5).abs() < 0.1);
    let paragraph = frame
        .child(&Ns::Draw, "text-box")
        .and_then(|text_box| text_box.child(&Ns::Text, "p"))
        .expect("an empty paragraph to type in");
    assert!(paragraph.children.is_empty());
}

#[test]
fn picture_stored_in_package_with_alt() {
    let mut deck = deck();
    let position = deck.slides()[1].position;
    let bytes = png();
    let path = deck
        .insert_picture(
            position,
            &SlidePicture {
                bytes: &bytes,
                media_type: "image/png",
                extension: "png",
                size: (100.0, 100.0),
                description: Some("The odox icon"),
            },
        )
        .expect("put in");
    let frame = deck.document.content.at(&path).expect("the frame");
    let href = frame
        .child(&Ns::Draw, "image")
        .and_then(|image| image.attr(&Ns::Xlink, "href"))
        .expect("an image");
    assert_eq!(
        deck.document
            .package
            .part(href)
            .map(|part| part.data.as_slice()),
        Some(bytes.as_slice())
    );
    let title = frame.child(&Ns::Svg, "title").map(Element::plain_text);
    assert_eq!(title.as_deref(), Some("The odox icon"));
}

#[test]
fn picture_fitted_inside_slide() {
    let mut deck = deck();
    let slide = &deck.slides()[1];
    let position = slide.position;
    let layout = deck.page_layout(slide);
    let (width, height) = (layout.width.points(), layout.height.points());
    let bytes = png();
    let path = deck
        .insert_picture(
            position,
            &SlidePicture {
                bytes: &bytes,
                media_type: "image/png",
                extension: "png",
                size: (width * 3.0, height * 3.0),
                description: None,
            },
        )
        .expect("put in");
    let frame = deck.document.content.at(&path).expect("the frame");
    let (x, y, w, h) = (
        length(frame, "x"),
        length(frame, "y"),
        length(frame, "width"),
        length(frame, "height"),
    );
    assert!(w <= width && h <= height, "{w} {h} in {width} {height}");
    assert!((w / h - width / height).abs() < 0.01, "keeps its shape");
    assert!((x + w / 2.0 - width / 2.0).abs() < 0.5 && (y + h / 2.0 - height / 2.0).abs() < 0.5);
}
