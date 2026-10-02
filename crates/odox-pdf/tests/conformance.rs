//! Every text document in the corpus exports, and every export passes
//! veraPDF's PDF/UA-1 profile.
//!
//! veraPDF is a Java program and not a dependency of the build. Where it is
//! not on the path, the exports are still made and this says that the check
//! was skipped; `packaging/verapdf.sh` runs the same check by hand.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::{Path, PathBuf};
use std::process::Command;

use odox_core::doc::TextDocument;
use odox_core::edit;
use odox_pdf::{Options, Refusal, export, undescribed};

fn corpus(directory: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
        if path.is_dir() {
            corpus(&path, into);
        } else if path.extension().and_then(|e| e.to_str()) == Some("odt") {
            into.push(path);
        }
    }
}

fn options() -> Options {
    Options {
        title: "Untitled".to_owned(),
        language: "en-US".to_owned(),
    }
}

/// The document with every undescribed figure marked decorative, which is
/// what the export dialog's "mark all decorative" does.
fn described(mut document: TextDocument) -> TextDocument {
    for path in undescribed(&document).into_iter().rev() {
        edit::set_decorative(
            &mut document.document.content,
            &path,
            &mut document.document.styles,
        )
        .expect("marked decorative");
    }
    document
}

#[test]
fn every_text_document_in_the_corpus_exports_as_pdf_ua() {
    let mut paths = Vec::new();
    corpus(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"),
        &mut paths,
    );
    assert!(!paths.is_empty(), "the corpus is where it was");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("conformance");
    std::fs::create_dir_all(&out).expect("an output directory");

    let mut written = Vec::new();
    for path in &paths {
        let bytes = std::fs::read(path).expect("a corpus document reads");
        let document = described(TextDocument::read(&bytes).expect("it parses"));
        let exported = export(&document, &options())
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

    assert_eq!(undescribed(&document), std::slice::from_ref(&chart));
    assert_eq!(
        export(&document, &options()).err(),
        Some(Refusal::Undescribed(vec![chart.clone()]))
    );
    edit::set_alternative_text(&mut document.document.content, &chart, "A chart")
        .expect("described");
    assert!(export(&document, &options()).is_ok());
}
