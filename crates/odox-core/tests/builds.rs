//! A slide's builds: what each click in a show reveals or takes away.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::doc::{Presentation, Steps, Target};

fn deck() -> Presentation {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/libreoffice/builds.odp"),
    )
    .expect("the fixture");
    Presentation::read(&bytes).expect("it reads")
}

/// The index among the page's children of the shape named `name`.
fn shape(deck: &Presentation, name: &str) -> usize {
    deck.slides()[0]
        .shapes_indexed()
        .find(|(_, shape)| shape.attr(&odox_core::Ns::Draw, "name") == Some(name))
        .map(|(index, _)| index)
        .expect("the shape")
}

fn steps(deck: &Presentation, slide: usize) -> Steps {
    Steps::read(deck.slides()[slide].element)
}

#[test]
fn shape_hidden_until_its_click() {
    let deck = deck();
    let steps = steps(&deck, 0);
    let appears = Target::Shape(shape(&deck, "Appears"));
    assert!(!steps.visible(0, appears));
    assert!(steps.visible(1, appears));
    assert!(steps.visible(steps.len(), appears));
}

#[test]
fn paragraphs_appear_one_per_click() {
    let deck = deck();
    let steps = steps(&deck, 0);
    let points = shape(&deck, "Points");
    assert!(steps.visible(0, Target::Shape(points)));
    for (step, shown) in [(1, 0), (2, 1), (3, 2), (4, 3)] {
        for paragraph in 0..3 {
            assert_eq!(
                steps.visible(step, Target::Paragraph(points, paragraph)),
                paragraph < shown,
                "step {step}, paragraph {paragraph}"
            );
        }
    }
    assert_eq!(steps.veiled(1, points), vec![0, 1, 2]);
    assert_eq!(steps.veiled(3, points), vec![2]);
}

#[test]
fn exit_effect_hides_after_click() {
    let deck = deck();
    let steps = steps(&deck, 0);
    let leaves = Target::Shape(shape(&deck, "Leaves"));
    assert_eq!(steps.len(), 5);
    assert!(steps.visible(0, leaves));
    assert!(steps.visible(4, leaves));
    assert!(!steps.visible(5, leaves));
}

#[test]
fn with_previous_joins_step() {
    let deck = deck();
    let steps = steps(&deck, 0);
    let with = Target::Shape(shape(&deck, "WithFirst"));
    assert!(!steps.visible(0, with));
    assert!(steps.visible(1, with));
}

#[test]
fn after_previous_joins_step() {
    let deck = deck();
    let steps = steps(&deck, 0);
    let after = Target::Shape(shape(&deck, "AfterFirst"));
    assert!(!steps.visible(0, after));
    assert!(steps.visible(1, after));
}

#[test]
fn slide_without_timing_has_zero_steps() {
    let deck = deck();
    let steps = steps(&deck, 1);
    assert_eq!(steps.len(), 0);
    let plain = deck.slides()[1].shapes_indexed().next().expect("a shape").0;
    assert!(steps.visible(0, Target::Shape(plain)));
}
