//! Every document in the corpus that a window exports exports, and every
//! export passes veraPDF's PDF/UA-1 profile.
//!
//! veraPDF is a Java program and not a dependency of the build. Where it is
//! not on the path, the exports are still made and this says that the check
//! was skipped; `packaging/verapdf.sh` runs the same check by hand.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::{Path, PathBuf};
use std::process::Command;

use odox_core::doc::{Presentation, SheetDocument, TextDocument};
use odox_core::edit;
use odox_pdf::{Exportable, Options, Refusal};

fn corpus(directory: &Path, extension: &str, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
        if path.is_dir() {
            corpus(&path, extension, into);
        } else if path.extension().and_then(|e| e.to_str()) == Some(extension) {
            into.push(path);
        }
    }
}

fn options() -> Options {
    Options {
        title: "Untitled".to_owned(),
        language: "en-US".to_owned(),
        untitled_slide: "Slide {number}".to_owned(),
    }
}

/// The document with every undescribed figure marked decorative, which is
/// what the export dialog's "mark the rest as decoration" does.
fn described<D: Exportable>(mut document: D) -> D {
    for path in document.undescribed().into_iter().rev() {
        let odox_core::Document {
            content, styles, ..
        } = document.document_mut();
        edit::set_decorative(content, &path, styles).expect("marked decorative");
    }
    assert!(
        document.undescribed().is_empty(),
        "nothing left to describe"
    );
    document
}

/// Export every document of a kind in the corpus and put the PDFs through
/// veraPDF.
fn conforms<D: Exportable>(extension: &str, read: fn(&[u8]) -> Result<D, odox_core::Error>) {
    let mut paths = Vec::new();
    corpus(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"),
        extension,
        &mut paths,
    );
    assert!(!paths.is_empty(), "the corpus is where it was");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("conformance")
        .join(extension);
    std::fs::create_dir_all(&out).expect("an output directory");

    let mut written = Vec::new();
    for path in &paths {
        let bytes = std::fs::read(path).expect("a corpus document reads");
        let document = described(read(&bytes).expect("it parses"));
        let exported = document
            .export(&options())
            .unwrap_or_else(|refusal| panic!("{}: {refusal}", path.display()));
        let target = out
            .join(path.file_stem().expect("a name"))
            .with_extension("pdf");
        std::fs::write(&target, exported.pdf).expect("written");
        written.push(target);
    }

    let Ok(output) = Command::new("verapdf")
        .args(["--flavour", "ua1", "--format", "text"])
        .args(&written)
        .output()
    else {
        eprintln!("veraPDF is not on the path: the exports were not checked");
        return;
    };
    let report = String::from_utf8_lossy(&output.stdout);
    let failed: Vec<&str> = report.lines().filter(|l| !l.starts_with("PASS")).collect();
    assert!(
        failed.is_empty() && report.lines().count() == written.len(),
        "veraPDF:\n{report}"
    );
}

#[test]
fn every_text_document_in_the_corpus_exports_as_pdf_ua() {
    conforms("odt", TextDocument::read);
}

#[test]
fn every_spreadsheet_in_the_corpus_exports_as_pdf_ua() {
    conforms("ods", SheetDocument::read);
}

#[test]
fn every_presentation_in_the_corpus_exports_as_pdf_ua() {
    conforms("odp", Presentation::read);
}

#[test]
fn a_figure_with_nothing_to_say_is_refused_until_it_is_described() {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/export.odt"),
    )
    .expect("the export fixture");
    let mut document = TextDocument::read(&bytes).expect("it parses");
    let body = document.body_path().expect("a body");
    let chart = [
        body,
        edit::figures(document.body().expect("a body"))[0].clone(),
    ]
    .concat();
    document
        .document
        .content
        .at_mut(&chart)
        .expect("the frame")
        .children
        .retain(
            |n| !matches!(n, odox_core::Node::Element(e) if e.is(&odox_core::Ns::Svg, "title")),
        );

    assert_eq!(document.undescribed(), std::slice::from_ref(&chart));
    assert_eq!(
        document.export(&options()).err(),
        Some(Refusal::Undescribed(vec![chart.clone()]))
    );
    edit::set_alternative_text(&mut document.document.content, &chart, "A chart")
        .expect("described");
    assert!(document.export(&options()).is_ok());
}
