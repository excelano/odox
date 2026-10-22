//! Export one document to PDF/UA from the command line, marking every figure
//! with nothing to say as decorative:
//! `cargo run -p odox-pdf --example export -- in.odp out.pdf`.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use odox_core::doc::{Presentation, TextDocument};
use odox_core::edit;
use odox_pdf::Exportable;

fn main() {
    let mut arguments = std::env::args().skip(1);
    let (Some(input), Some(output)) = (arguments.next(), arguments.next()) else {
        eprintln!("usage: export <in.odt|in.odp> <out.pdf>");
        std::process::exit(2);
    };
    let bytes = std::fs::read(&input).expect("the document reads");
    let result = match Path::new(&input).extension().and_then(|e| e.to_str()) {
        Some("odp") => run(Presentation::read(&bytes).expect("a presentation"), &input),
        _ => run(TextDocument::read(&bytes).expect("a text document"), &input),
    };
    match result {
        Ok(exported) => {
            for substitution in exported.substituted {
                eprintln!("{} drawn as {}", substitution.family, substitution.used);
            }
            std::fs::write(output, exported.pdf).expect("the PDF writes");
        }
        Err(refusal) => {
            eprintln!("{input}: {refusal}");
            std::process::exit(1);
        }
    }
}

fn run(
    mut document: impl Exportable,
    input: &str,
) -> Result<odox_pdf::Exported, odox_pdf::Refusal> {
    for path in document.undescribed().into_iter().rev() {
        let odox_core::Document {
            content, styles, ..
        } = document.document_mut();
        edit::set_decorative(content, &path, styles).expect("marked decorative");
    }
    let options = odox_pdf::Options {
        title: Path::new(input)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        language: "en-US".to_owned(),
        untitled_slide: "Slide {number}".to_owned(),
    };
    document.export(&options)
}
