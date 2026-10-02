//! Export one text document to PDF/UA from the command line, marking every
//! figure with nothing to say as decorative:
//! `cargo run -p odox-pdf --example export -- in.odt out.pdf`.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::doc::TextDocument;
use odox_core::edit;

fn main() {
    let mut arguments = std::env::args().skip(1);
    let (Some(input), Some(output)) = (arguments.next(), arguments.next()) else {
        eprintln!("usage: export <in.odt> <out.pdf>");
        std::process::exit(2);
    };
    let bytes = std::fs::read(&input).expect("the document reads");
    let mut document = TextDocument::read(&bytes).expect("it is a text document");
    for path in odox_pdf::undescribed(&document).into_iter().rev() {
        edit::set_decorative(
            &mut document.document.content,
            &path,
            &mut document.document.styles,
        )
        .expect("marked decorative");
    }
    let options = odox_pdf::Options {
        title: std::path::Path::new(&input)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        language: "en-US".to_owned(),
    };
    match odox_pdf::export(&document, &options) {
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
