//! Working out the outline of a custom shape.
//!
//! The inputs here are paths and formulas taken verbatim from the presentation
//! templates LibreOffice ships, because the point of the exercise is that a
//! document says `M ?f7 0 X 0 ?f8 …` and something has to turn that into points.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::draw::Geometry;
use odox_core::xml;

/// Wrap an enhanced-geometry element in the namespaces it uses.
fn geometry(inner: &str) -> Geometry {
    let document = format!(
        r#"<draw:enhanced-geometry
             xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
             xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0"
             {inner}"#
    );
    let element = xml::parse(document.as_bytes(), "test").expect("well-formed XML");
    Geometry::read(&element).expect("a readable geometry")
}

#[test]
fn a_full_ellipse_is_the_commonest_shape_there_is() {
    // `U` and this exact path is 176 of the 576 custom shapes in the templates.
    let shape = geometry(
        r#"svg:viewBox="0 0 21600 21600"
           draw:enhanced-path="U 10800 10800 10800 10800 0 360 Z N"/>"#,
    );

    assert_eq!(shape.paths.len(), 1);
    let path = &shape.paths[0];
    assert!(path.closed && path.fill && path.stroke);
    assert!(path.points.len() > 30, "{} points", path.points.len());

    // Every point is on the circle, which is the whole claim.
    for (x, y) in &path.points {
        let radius = ((x - 10800.0).powi(2) + (y - 10800.0).powi(2)).sqrt();
        assert!(
            (radius - 10800.0).abs() < 1.0,
            "({x}, {y}) is off the circle"
        );
    }
}

#[test]
fn a_rectangle_is_four_lines_written_once() {
    // `L` takes its two arguments over and over: one letter, four corners.
    let shape = geometry(
        r#"svg:viewBox="0 0 21600 21600"
           draw:enhanced-path="M 0 0 L 21600 0 21600 21600 0 21600 0 0 Z N"/>"#,
    );

    assert_eq!(shape.paths.len(), 1);
    assert!(shape.paths[0].closed);
    assert_eq!(
        shape.paths[0].points,
        vec![
            (0.0, 0.0),
            (21600.0, 0.0),
            (21600.0, 21600.0),
            (0.0, 21600.0),
            (0.0, 0.0),
        ]
    );
}

#[test]
fn a_rounded_rectangle_resolves_its_formulas_and_rounds_its_corners() {
    // Verbatim from Candy.odp, handle and text areas dropped. The corner radius
    // is the modifier, and the four corners are quadrant arcs.
    let shape = geometry(
        r#"svg:viewBox="0 0 21600 21600" draw:type="round-rectangle"
           draw:modifiers="10800"
           draw:enhanced-path="M ?f7 0 X 0 ?f8 L 0 ?f9 Y ?f7 21600 L ?f10 21600 X 21600 ?f9 L 21600 ?f8 Y ?f10 0 Z N">
             <draw:equation draw:name="f0" draw:formula="45"/>
             <draw:equation draw:name="f1" draw:formula="$0 *sin(?f0 *(pi/180))"/>
             <draw:equation draw:name="f2" draw:formula="?f1 *3163/7636"/>
             <draw:equation draw:name="f7" draw:formula="left+$0 "/>
             <draw:equation draw:name="f8" draw:formula="top+$0 "/>
             <draw:equation draw:name="f9" draw:formula="bottom-$0 "/>
             <draw:equation draw:name="f10" draw:formula="right-$0 "/>
           </draw:enhanced-geometry>"#,
    );

    assert_eq!(shape.paths.len(), 1);
    let path = &shape.paths[0];
    assert!(path.closed);

    // `?f7` is left + the modifier, so the path begins halfway along the top.
    assert_eq!(path.points[0], (10800.0, 0.0));

    // With a radius of half the side, every corner is a quarter circle and the
    // whole outline is a circle inscribed in the box. What that proves is that
    // the formulas were evaluated: unresolved, every `?f7` would be nought and
    // the shape would collapse into the corner.
    let (mut left, mut right, mut top, mut bottom) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for (x, y) in &path.points {
        left = left.min(*x);
        right = right.max(*x);
        top = top.min(*y);
        bottom = bottom.max(*y);
    }
    assert!(left.abs() < 1.0 && top.abs() < 1.0, "{left}, {top}");
    assert!(
        (right - 21600.0).abs() < 1.0 && (bottom - 21600.0).abs() < 1.0,
        "{right}, {bottom}"
    );
    assert!(path.points.len() > 40, "the corners are not arcs");
}

#[test]
fn a_formula_may_name_another_and_the_chain_is_followed() {
    // `?f2` is `?f1 * 3163/7636`, `?f1` is `$0 * sin(?f0 * pi/180)`, `?f0` is 45.
    // A path point of `?f2` is therefore 10800 * sin(45°) * 3163/7636 ≈ 3163.5.
    let shape = geometry(
        r#"svg:viewBox="0 0 21600 21600" draw:modifiers="10800"
           draw:enhanced-path="M ?f2 0 L 21600 0 Z N">
             <draw:equation draw:name="f0" draw:formula="45"/>
             <draw:equation draw:name="f1" draw:formula="$0 *sin(?f0 *(pi/180))"/>
             <draw:equation draw:name="f2" draw:formula="?f1 *3163/7636"/>
           </draw:enhanced-geometry>"#,
    );
    let x = shape.paths[0].points[0].0;
    assert!((x - 3163.5).abs() < 1.0, "{x}");
}

#[test]
fn a_formula_that_refers_to_itself_gives_up_rather_than_hanging() {
    // No producer writes this. A hand-edited file can, and the alternative to a
    // limit is a window that stops responding.
    let shape = geometry(
        r#"svg:viewBox="0 0 100 100"
           draw:enhanced-path="M ?f0 0 L 100 100 Z N">
             <draw:equation draw:name="f0" draw:formula="?f1 + 1"/>
             <draw:equation draw:name="f1" draw:formula="?f0 + 1"/>
           </draw:enhanced-geometry>"#,
    );
    assert!(shape.paths[0].points[0].0.is_finite());
}

#[test]
fn a_shape_may_be_several_strokes_and_say_which_are_filled() {
    let shape = geometry(
        r#"svg:viewBox="0 0 100 100"
           draw:enhanced-path="M 0 0 L 100 0 100 100 Z N F M 10 10 L 90 10 Z N"/>"#,
    );
    assert_eq!(shape.paths.len(), 2);
    assert!(shape.paths[0].fill, "the body should be filled");
    assert!(
        !shape.paths[1].fill,
        "F turns the fill off for what follows"
    );
}

#[test]
fn a_mirrored_shape_is_flipped_within_its_own_space() {
    let plain = geometry(r#"svg:viewBox="0 0 100 50" draw:enhanced-path="M 10 5 L 90 5 Z N"/>"#);
    let flipped = geometry(
        r#"svg:viewBox="0 0 100 50" draw:mirror-horizontal="true"
           draw:enhanced-path="M 10 5 L 90 5 Z N"/>"#,
    );
    assert_eq!(plain.paths[0].points[0], (10.0, 5.0));
    assert_eq!(flipped.paths[0].points[0], (90.0, 5.0));
}

#[test]
fn every_custom_shape_in_the_corpus_yields_an_outline() {
    // The whole point of the exercise, over the documents that motivated it: no
    // shape may come back empty, and every point must be a number.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };

    let mut shapes = 0;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("odp") {
            continue;
        }
        let bytes = std::fs::read(&path).expect("the fixture");
        let package = odox_core::Package::read(&bytes).expect("a readable package");
        for part in ["content.xml", "styles.xml"] {
            let Ok(Some(tree)) = package.optional_xml(part) else {
                continue;
            };
            for element in descendants(&tree) {
                if !element.is(&odox_core::Ns::Draw, "enhanced-geometry") {
                    continue;
                }
                shapes += 1;
                let geometry = Geometry::read(element)
                    .unwrap_or_else(|| panic!("{} in {part}", path.display()));
                assert!(
                    !geometry.paths.is_empty(),
                    "{} in {part} drew nothing",
                    path.display()
                );
                for stroke in &geometry.paths {
                    for (x, y) in &stroke.points {
                        assert!(
                            x.is_finite() && y.is_finite(),
                            "{} gave ({x}, {y})",
                            path.display()
                        );
                    }
                }
            }
        }
    }
    assert!(shapes > 0, "the corpus has no custom shapes to read");
}

fn descendants(element: &odox_core::Element) -> Vec<&odox_core::Element> {
    let mut found = vec![element];
    for child in element.elements() {
        found.extend(descendants(child));
    }
    found
}
