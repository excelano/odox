//! Alternative text and decoration: what a figure says in place of its
//! picture, read and written the way `LibreOffice` does.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use odox_core::doc::TextDocument;
use odox_core::edit::{self, Description};

fn open() -> TextDocument {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/export.odt"),
    )
    .expect("the export fixture");
    TextDocument::read(&bytes).expect("it reads")
}

/// The figures of the document as paths from the content root, and what each
/// says.
fn described(document: &TextDocument) -> Vec<(Vec<usize>, Description)> {
    let body = document.body_path().expect("a body");
    edit::figures(document.body().expect("a body"))
        .into_iter()
        .map(|path| {
            let full = [body.clone(), path].concat();
            let frame = document.document.content.at(&full).expect("the frame");
            let description = edit::description(frame, &document.document.styles);
            (full, description)
        })
        .collect()
}

#[test]
fn the_fixture_has_a_described_figure_and_a_decorative_one_and_no_text_box() {
    let document = open();
    let found: Vec<Description> = described(&document).into_iter().map(|(_, d)| d).collect();
    assert_eq!(
        found,
        [
            Description::Text("A blue rectangle standing in for a chart".to_owned()),
            Description::Decorative,
        ]
    );
}

#[test]
fn alternative_text_replaces_the_title_and_reads_back_after_saving() {
    let mut document = open();
    let (chart, _) = described(&document).remove(0);
    edit::set_alternative_text(&mut document.document.content, &chart, "Sales by quarter")
        .expect("written");
    let bytes = document.document.write_verified().expect("saved");
    let again = TextDocument::read(&bytes).expect("it reads");
    assert_eq!(
        described(&again)[0].1,
        Description::Text("Sales by quarter".to_owned())
    );
}

#[test]
fn a_figure_without_a_title_is_missing_until_it_is_given_one() {
    let mut document = open();
    let (chart, _) = described(&document).remove(0);
    let frame = document.document.content.at_mut(&chart).expect("the frame");
    frame.children.retain(
        |n| !matches!(n, odox_core::Node::Element(e) if e.is(&odox_core::Ns::Svg, "title")),
    );
    assert_eq!(described(&document)[0].1, Description::Missing);
    edit::set_alternative_text(&mut document.document.content, &chart, "A chart").expect("written");
    assert_eq!(
        described(&document)[0].1,
        Description::Text("A chart".to_owned())
    );
}

#[test]
fn marking_decorative_touches_only_that_figure_and_reads_back_after_saving() {
    let mut document = open();
    let (chart, _) = described(&document).remove(0);
    edit::set_decorative(
        &mut document.document.content,
        &chart,
        &mut document.document.styles,
    )
    .expect("marked");
    let found: Vec<Description> = described(&document).into_iter().map(|(_, d)| d).collect();
    assert_eq!(found, [Description::Decorative, Description::Decorative]);

    let bytes = document.document.write_verified().expect("saved");
    let again = TextDocument::read(&bytes).expect("it reads");
    let found: Vec<Description> = described(&again).into_iter().map(|(_, d)| d).collect();
    assert_eq!(found, [Description::Decorative, Description::Decorative]);
}

#[test]
fn something_that_is_not_a_figure_is_refused() {
    let mut document = open();
    let body = document.body_path().expect("a body");
    let heading = [body, vec![0]].concat();
    assert_eq!(
        edit::set_alternative_text(&mut document.document.content, &heading, "no"),
        Err(odox_core::Refused::NotFound)
    );
}

#[test]
fn a_document_that_never_declared_libreoffices_namespace_is_given_it_to_mark_decoration() {
    let mut document = open();
    let root = &mut document.document.content;
    root.attrs
        .retain(|a| !(a.name.ns == odox_core::Ns::Xmlns && &*a.name.local == "loext"));
    assert!(!root.declares(&odox_core::Ns::Loext));
    let (chart, _) = described(&document).remove(0);
    edit::set_decorative(
        &mut document.document.content,
        &chart,
        &mut document.document.styles,
    )
    .expect("marked");
    let bytes = document.document.write_verified().expect("saved");
    let again = TextDocument::read(&bytes).expect("it reads");
    assert_eq!(described(&again)[0].1, Description::Decorative);
}
