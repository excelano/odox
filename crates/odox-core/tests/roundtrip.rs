//! What a read and a write have to agree on.
//!
//! The claim this suite makes about compliance is not that it renders a document
//! the way another application does — nothing automated can measure that — but
//! that it gives a document back unchanged. A reader that silently drops the part
//! of ODF it has no opinion about looks identical in a window and destroys
//! documents the first time anything saves one, so the round trip is measured
//! from the first release, before anything can save at all.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::{Path, PathBuf};

use odox_core::doc::TextDocument;
use odox_core::{Package, xml};

/// Every document under `corpus/`, at any depth.
///
/// The corpus is walked rather than named, so that adding a document to it is a
/// change to the directory and not to this file, and so that the documents a
/// real producer wrote are covered by having been put there.
fn corpus() -> Vec<PathBuf> {
    let mut documents = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"),
        &mut documents,
    );
    documents.sort();
    assert!(!documents.is_empty(), "the corpus is empty");
    documents
}

fn walk(directory: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, into);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("odt" | "ods" | "odp")
        ) {
            into.push(path);
        }
    }
}

#[test]
fn every_entry_survives_a_round_trip() {
    for path in corpus() {
        let bytes = std::fs::read(&path).expect("the document");
        let first = Package::read(&bytes).expect("a readable package");
        let written = first.write().expect("a writable package");
        let second = Package::read(&written).expect("the package we just wrote");

        let before: Vec<&str> = first.parts().map(|p| p.name.as_str()).collect();
        let after: Vec<&str> = second.parts().map(|p| p.name.as_str()).collect();
        assert_eq!(
            before,
            after,
            "{} lost or reordered an entry",
            path.display()
        );

        // `mimetype` first and stored is the one ordering rule the format has.
        assert_eq!(before.first(), Some(&"mimetype"), "{}", path.display());
        assert!(
            second.part("mimetype").expect("the mimetype entry").stored,
            "{} wrote its mimetype compressed",
            path.display()
        );
        assert_eq!(
            first.media_type(),
            second.media_type(),
            "{}",
            path.display()
        );

        for part in first.parts() {
            let same = second.part(&part.name).expect("the entry in the copy");
            assert_eq!(
                part.data,
                same.data,
                "{} changed the bytes of {}",
                path.display(),
                part.name
            );
        }
    }
}

#[test]
fn parsing_and_writing_xml_gives_back_an_equal_tree() {
    for path in corpus() {
        let bytes = std::fs::read(&path).expect("the document");
        let package = Package::read(&bytes).expect("a readable package");
        for part in package.parts() {
            if !part.name.ends_with(".xml") {
                continue;
            }
            let first = xml::parse(&part.data, &part.name).expect("well-formed XML");
            let written = xml::serialize(&first);
            let second = xml::parse(&written, &part.name).expect("the XML we just wrote");
            assert_eq!(
                first,
                second,
                "{} does not survive a round trip of {}",
                path.display(),
                part.name
            );
        }
    }
}

#[test]
fn a_document_writes_back_what_it_read() {
    for path in corpus() {
        let Some("odt") = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let bytes = std::fs::read(&path).expect("the document");
        let mut document = TextDocument::read(&bytes).expect("a readable text document");
        let written = document.document.write().expect("a writable document");
        let again = TextDocument::read(&written).expect("the document we just wrote");

        assert_eq!(
            document.document.content,
            again.document.content,
            "{} changed its content on the way out",
            path.display()
        );
        assert_eq!(
            document.document.meta,
            again.document.meta,
            "{}",
            path.display()
        );
        assert_eq!(
            document.outline().len(),
            again.outline().len(),
            "{} changed its outline",
            path.display()
        );
    }
}

#[test]
fn the_sample_document_reads_as_a_document() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/sample.odt");
    if !path.is_file() {
        return;
    }
    let bytes = std::fs::read(path).expect("the sample document");
    let document = TextDocument::read(&bytes).expect("a readable text document");

    assert!(document.body().is_some(), "no text body");

    let outline = document.outline();
    assert_eq!(
        outline.first().map(|h| h.text.as_str()),
        Some("Sample document")
    );
    assert!(
        outline.iter().any(|h| h.level == 2 && h.text == "A table"),
        "the second-level headings are not in the outline: {:?}",
        outline
            .iter()
            .map(|h| (h.level, &h.text))
            .collect::<Vec<_>>()
    );

    // A page layout the document does declare, rather than the fallback.
    let layout = document.page_layout();
    assert!(layout.width.points() > 100.0 && layout.height.points() > layout.width.points());
    assert!(document.text_width() < layout.width.points());
}
