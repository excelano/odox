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
use odox_core::{
    Anchor, Color, Family, Fill, GradientStyle, Length, Ns, Offset, Tiling, Transform,
};

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

#[test]
fn a_numbered_circle_asks_for_its_numeral_in_the_middle() {
    // The label is paragraphs of the shape's own, with no `draw:text-box`
    // around them, and the style says where between the edges they go. Without
    // both the numerals in this template's circles draw at the top or not at
    // all.
    let Some(deck) = fixture("growing-liberty.odp") else {
        return;
    };
    let slides = deck.slides();
    let mut anchored = 0;
    for slide in &slides {
        for shape in slide.element.elements() {
            if !shape.is(&Ns::Draw, "custom-shape") {
                continue;
            }
            if shape.child(&Ns::Text, "p").is_none() {
                continue;
            }
            assert!(
                shape.child(&Ns::Draw, "text-box").is_none(),
                "a drawing shape put a box around its label"
            );
            let style = shape.attr(&Ns::Draw, "style-name").unwrap_or_default();
            let properties = deck.document.styles.resolve(&Family::Graphic, style);
            if properties.graphic.text_anchor == Some(Anchor::Middle) {
                anchored += 1;
            }
        }
    }
    assert!(anchored > 0, "no labelled shape asked to be centred");
}

#[test]
fn a_decoration_offers_a_second_picture_where_the_first_is_a_format_nothing_reads() {
    // The template draws its right-hand bar as an SVG with a PNG of the same
    // drawing after it. A reader that takes the first child and stops draws
    // nothing there.
    let Some(deck) = fixture("growing-liberty.odp") else {
        return;
    };
    let slides = deck.slides();
    let mut alternatives = 0;
    for slide in &slides {
        for shape in deck.background_objects(slide) {
            let images: Vec<_> = shape
                .elements()
                .filter(|child| child.is(&Ns::Draw, "image"))
                .collect();
            if images.len() < 2 {
                continue;
            }
            alternatives += 1;
            let types: Vec<_> = images
                .iter()
                .filter_map(|image| image.attr(&Ns::Draw, "mime-type"))
                .collect();
            assert!(
                types.contains(&"image/png"),
                "no readable alternative among {types:?}"
            );
        }
    }
    assert!(alternatives > 0, "the template offers no alternatives");
}

#[test]
fn every_transform_in_the_corpus_places_the_shape_on_the_page() {
    // A shape placed this way routinely gives no `svg:x` at all, so a reader
    // that cannot read the list drops it. What is checked here is that the list
    // parses and that the box it puts the shape in touches the page: a
    // composition order the wrong way round sends the decoration off the far
    // side, which is how the order was settled.
    let Some(deck) = fixture("growing-liberty.odp") else {
        return;
    };
    let mut found = 0;
    for slide in &deck.slides() {
        let page = deck.page_layout(slide);
        let (wide, high) = (page.width.points(), page.height.points());
        for shape in deck.background_objects(slide) {
            let Some(text) = shape.attr(&Ns::Draw, "transform") else {
                continue;
            };
            found += 1;
            let placed = Transform::parse(text).expect("a readable transform");
            let at = |name: &str| {
                shape
                    .attr(&Ns::Svg, name)
                    .and_then(Length::parse)
                    .map_or(0.0, |l| l.points())
            };
            let (width, height) = (at("width"), at("height"));
            let corners = [(0.0, 0.0), (width, 0.0), (width, height), (0.0, height)]
                .map(|corner| placed.apply(corner));
            let left = corners.iter().map(|c| c.0).fold(f32::MAX, f32::min);
            let right = corners.iter().map(|c| c.0).fold(f32::MIN, f32::max);
            let top = corners.iter().map(|c| c.1).fold(f32::MAX, f32::min);
            let bottom = corners.iter().map(|c| c.1).fold(f32::MIN, f32::max);
            assert!(
                right > 0.0 && left < wide && bottom > 0.0 && top < high,
                "a decoration landed at ({left}, {top})-({right}, {bottom}) \
                 outside a page of {wide} by {high}"
            );
        }
    }
    assert!(found > 0, "no decoration in the corpus is placed this way");
}

#[test]
fn a_shape_is_moved_and_sized_in_the_unit_it_was_written_in() {
    let Some(mut deck) = fixture("deck.odp") else {
        return;
    };
    let (slide, shape) = {
        let slides = deck.slides();
        let slide = slides.first().expect("a slide");
        let (index, frame) = slide
            .shapes_indexed()
            .find(|(_, s)| s.attr(&Ns::Svg, "x").is_some())
            .expect("a placed shape");
        assert!(frame.attr(&Ns::Svg, "x").expect("x").ends_with("cm"));
        (slide.position, index)
    };
    deck.set_geometry(
        slide,
        shape,
        Length(72.0),
        Length(36.0),
        Length(144.0),
        Length(72.0),
    )
    .expect("a movable shape");
    let slides = deck.slides();
    let frame = slides[0]
        .shapes_indexed()
        .find(|(i, _)| *i == shape)
        .map(|(_, s)| s)
        .expect("the shape");
    assert_eq!(frame.attr(&Ns::Svg, "x"), Some("2.54cm"));
    assert_eq!(frame.attr(&Ns::Svg, "y"), Some("1.27cm"));
    assert_eq!(frame.attr(&Ns::Svg, "width"), Some("5.08cm"));
    assert_eq!(frame.attr(&Ns::Svg, "height"), Some("2.54cm"));
    assert_eq!(
        deck.set_geometry(
            slide,
            9999,
            Length(0.0),
            Length(0.0),
            Length(1.0),
            Length(1.0)
        ),
        Err(odox_core::Refused::NotFound)
    );
    deck.document.write_verified().expect("saves");
}

#[test]
fn a_placeholders_text_takes_what_its_paragraph_does_not_say_from_the_frame() {
    let Some(deck) = fixture("growing-liberty.odp") else {
        return;
    };
    let styles = &deck.document.styles;
    let frame = styles.resolve(&Family::Presentation, "pr1");
    let paragraph = styles.resolve(&Family::Paragraph, "Standard");
    let laid = paragraph.in_shape(&frame);
    assert_eq!(laid.text.bold, Some(true), "the title style's weight");
    assert_eq!(laid.text.size, frame.text.size);
    assert!(laid.text.size.is_some());

    let mut own = (*paragraph).clone();
    own.text.bold = Some(false);
    assert_eq!(
        own.in_shape(&frame).text.bold,
        Some(false),
        "the paragraph's own wins"
    );
}

#[test]
fn a_slide_shows_the_footer_date_and_number_it_names_and_turns_on() {
    let Some(deck) = fixture("slides.odp") else {
        return;
    };
    let slides = deck.slides();
    let shown = |at: usize| -> Vec<(String, String)> {
        deck.master_fields(&slides[at], (2026, 11, 10))
            .iter()
            .map(|frame| {
                (
                    frame
                        .attr(&Ns::Presentation, "class")
                        .unwrap_or_default()
                        .to_owned(),
                    frame.plain_text(),
                )
            })
            .collect()
    };
    let pairs = |list: &[(&str, &str)]| -> Vec<(String, String)> {
        list.iter()
            .map(|(class, text)| ((*class).to_owned(), (*text).to_owned()))
            .collect()
    };
    assert_eq!(
        shown(0),
        pairs(&[
            ("date-time", "2026-03-14"),
            ("footer", "Quarterly review"),
            ("page-number", "1"),
        ])
    );
    assert_eq!(
        shown(1),
        pairs(&[("footer", "Quarterly review"), ("page-number", "2")]),
        "a slide that names no date shows none"
    );
}

#[test]
fn a_date_that_follows_the_clock_shows_today() {
    let Some(mut deck) = fixture("slides.odp") else {
        return;
    };
    let presentation = deck
        .document
        .content
        .child_mut(&Ns::Office, "body")
        .and_then(|body| body.child_mut(&Ns::Office, "presentation"))
        .expect("a presentation body");
    let declaration = presentation
        .child_mut(&Ns::Presentation, "date-time-decl")
        .expect("a date declaration");
    let source = declaration
        .attrs
        .iter_mut()
        .find(|a| a.name.is(&Ns::Presentation, "source"))
        .expect("a source");
    source.value = "current-date".to_owned();
    let slides = deck.slides();
    let date = deck
        .master_fields(&slides[0], (2026, 11, 10))
        .into_iter()
        .find(|frame| frame.attr(&Ns::Presentation, "class") == Some("date-time"))
        .expect("the date")
        .plain_text();
    assert_eq!(date, "2026-11-10");
}

#[test]
fn a_tiled_picture_lies_where_libreoffice_draws_it() {
    let Some(deck) = fixture("fills.odp") else {
        return;
    };
    let points = |cm: f32| cm * 72.0 / 2.54;
    let shape = (points(11.0), points(7.0));
    let slides = deck.slides();
    let fill = |index: usize| {
        let shape = slides[index]
            .element
            .elements()
            .find(|e| e.is(&Ns::Draw, "rect") || e.is(&Ns::Draw, "ellipse"))
            .expect("a filled shape");
        let style = shape.attr(&Ns::Draw, "style-name").unwrap_or_default();
        deck.document
            .styles
            .resolve(&Family::Graphic, style)
            .graphic
            .fill()
    };
    let tile = |index: usize, picture: (f32, f32)| {
        let Fill::Tiled(_, tiling) = fill(index) else {
            panic!("slide {index} is not tiled: {:?}", fill(index));
        };
        let (x, y, width, height) = tiling.tile(shape, picture).expect("a tile");
        [x, y, width, height].map(|n| (n * 2.54 / 72.0 * 1000.0).round() / 1000.0)
    };
    let own = (30.0, 22.5);
    assert_eq!(
        tile(0, own),
        [4.971, 3.103, 1.058, 0.794],
        "own size, centred"
    );
    assert_eq!(tile(1, own), [4.5, 3.0, 2.0, 1.0], "the style's size");
    assert_eq!(tile(2, own), [9.0, 6.0, 2.0, 1.0], "the bottom right");
    assert_eq!(tile(3, own), [5.5, 3.25, 2.0, 1.0], "moved by a share");
    assert_eq!(
        tile(6, own),
        [4.125, 2.625, 2.75, 1.75],
        "a share of the shape"
    );
    assert_eq!(tile(7, own), [0.0, 0.0, 2.0, 1.0], "the top left");
    for (index, offset) in [(4, Offset::Rows(0.5)), (5, Offset::Columns(0.5))] {
        let Fill::Tiled(_, tiling) = fill(index) else {
            panic!("slide {index} is not tiled");
        };
        assert_eq!(tiling.offset, Some(offset));
    }
    let Fill::Tiled(_, once) = fill(7) else {
        panic!("placed once");
    };
    assert!(!once.repeat);
    let Fill::Tiled(name, _) = fill(8) else {
        panic!("the dense picture is tiled");
    };
    let href = deck.document.styles.fill_image(&name).expect("a picture");
    let bytes = deck.document.picture(href).expect("in the package");
    assert_eq!(
        odox_core::picture::dots_per_inch(bytes).map(|(x, _)| x.round()),
        Some(72.0)
    );
    assert!(matches!(fill(9), Fill::Image(_)), "stretched");
    assert!(matches!(
        deck.background(&slides[10]),
        Fill::Tiled(_, Tiling { repeat: true, .. })
    ));
}

#[test]
fn a_gradient_from_a_point_runs_as_libreoffice_draws_it() {
    let Some(deck) = fixture("gradients.odp") else {
        return;
    };
    let gradient = |index: usize| {
        let slides = deck.slides();
        let shape = slides[index]
            .element
            .elements()
            .find(|e| e.is(&Ns::Draw, "rect"))
            .expect("a filled shape");
        let style = shape.attr(&Ns::Draw, "style-name").unwrap_or_default();
        let Fill::Gradient(name) = deck
            .document
            .styles
            .resolve(&Family::Graphic, style)
            .graphic
            .fill()
        else {
            panic!("slide {index} has no gradient");
        };
        deck.document
            .styles
            .gradient(&name)
            .expect("defined")
            .clone()
    };
    // LibreOffice's own PDF of each slide at 100 pixels to the centimetre,
    // black to white, at five points of the 11 by 7 centimetre shape.
    let points = [
        (100.0, 100.0),
        (400.0, 200.0),
        (550.0, 350.0),
        (700.0, 550.0),
        (1000.0, 650.0),
    ];
    let drawn: [[f32; 5]; 8] = [
        [53.0, 173.0, 255.0, 157.0, 42.0],
        [30.0, 84.0, 94.0, 46.0, 0.0],
        [72.0, 173.0, 255.0, 147.0, 48.0],
        [0.0, 86.0, 189.0, 52.0, 0.0],
        [46.0, 187.0, 255.0, 163.0, 46.0],
        [62.0, 141.0, 116.0, 104.0, 14.0],
        [72.0, 147.0, 255.0, 108.0, 36.0],
        [4.0, 135.0, 107.0, 90.0, 0.0],
    ];
    for (index, grey) in drawn.iter().enumerate() {
        let gradient = gradient(index);
        for (point, grey) in points.iter().zip(grey) {
            let level = gradient.level(*point, (1100.0, 700.0)) * 255.0;
            assert!(
                (level - grey).abs() <= 4.0,
                "slide {index} at {point:?}: {level}, LibreOffice {grey}"
            );
        }
    }
    assert_eq!(
        gradient(8).start,
        Color {
            r: 128,
            g: 128,
            b: 128
        },
        "white at half its intensity"
    );
}
