//! Tab stops, read from what LibreOffice writes.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use odox_core::doc::TextDocument;
use odox_core::{Family, TabKind};

fn fixture() -> TextDocument {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/tabs.odt"),
    )
    .expect("the tabs fixture");
    TextDocument::read(&bytes).expect("it reads")
}

fn cm(points: f32) -> f32 {
    (points / 72.0 * 2.54 * 100.0).round() / 100.0
}

#[test]
fn a_paragraph_styles_tab_stops_are_read() {
    let document = fixture();
    let styles = &document.document.styles;
    let contents = styles.resolve(&Family::Paragraph, "Contents");
    let stops = contents.paragraph.tab_stops.clone().expect("stops");
    assert_eq!(stops.len(), 1);
    assert_eq!(stops[0].kind, TabKind::Right);
    assert_eq!(stops[0].leader, Some('.'));
    assert!((cm(stops[0].position.points()) - 15.0).abs() < 0.01);

    let columns = styles.resolve(&Family::Paragraph, "Columns");
    let kinds: Vec<TabKind> = columns
        .paragraph
        .tab_stops
        .clone()
        .expect("stops")
        .iter()
        .map(|s| s.kind)
        .collect();
    assert_eq!(kinds, [TabKind::Left, TabKind::Center, TabKind::Right]);

    let figures = styles.resolve(&Family::Paragraph, "Figures");
    assert_eq!(
        figures.paragraph.tab_stops.clone().expect("stops")[0].kind,
        TabKind::Char('.')
    );
}

#[test]
fn a_paragraph_with_no_stops_takes_the_default_distance() {
    let document = fixture();
    let standard = document
        .document
        .styles
        .resolve(&Family::Paragraph, "Standard");
    assert!(
        standard
            .paragraph
            .tab_stops
            .as_ref()
            .is_none_or(Vec::is_empty)
    );
    let distance = standard.paragraph.tab_distance.expect("a distance");
    assert!((cm(distance.points()) - 1.25).abs() < 0.01);
}
