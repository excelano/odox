//! A new document of each kind, with nothing in it: what File > New opens.
//!
//! Each is a whole package, written here as text and assembled, so that a new
//! document is read through the same reader as any other and is saved as one
//! the reader has already proved it can read. It carries what a person would
//! otherwise find missing at once: a default text style in the language they
//! read, a page of the size their country prints on, and for a deck one slide
//! with a title to type into.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use crate::media_type;
use crate::package::{Package, Part};

/// The paper a new document is laid out on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paper {
    /// 8.5 by 11 inches, with one-inch margins.
    Letter,
    /// 210 by 297 millimetres, with two-centimetre margins.
    A4,
}

impl Paper {
    /// The paper a language tag's country prints on: Letter in the United
    /// States and Canada, A4 everywhere else.
    pub fn for_language(tag: &str) -> Self {
        let country = tag
            .split(['-', '_'])
            .nth(1)
            .unwrap_or_default()
            .to_ascii_uppercase();
        if matches!(country.as_str(), "US" | "CA") {
            Self::Letter
        } else {
            Self::A4
        }
    }

    /// Width, height and margin, as ODF writes them.
    fn sizes(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Letter => ("8.5in", "11in", "1in"),
            Self::A4 => ("21cm", "29.7cm", "2cm"),
        }
    }
}

const NS: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" office:version="1.3""#;

/// The language a style says text is in: `fo:language` and, where the tag
/// names one, `fo:country`.
fn language_attributes(tag: &str) -> String {
    let mut parts = tag.split(['-', '_']);
    let language = parts
        .next()
        .filter(|l| !l.is_empty())
        .unwrap_or("en")
        .to_ascii_lowercase();
    match parts.next().filter(|c| c.len() == 2) {
        Some(country) => format!(
            r#"fo:language="{}" fo:country="{}""#,
            escape(&language),
            escape(&country.to_ascii_uppercase())
        ),
        None => format!(r#"fo:language="{}""#, escape(&language)),
    }
}

/// A value made safe to stand between an attribute's quotes.
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

/// A blank text document: one empty paragraph.
pub fn text(language: &str) -> Vec<u8> {
    let (width, height, margin) = Paper::for_language(language).sizes();
    let lang = language_attributes(language);
    let styles = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles {NS}><office:font-face-decls><style:font-face style:name="Liberation Serif" svg:font-family="'Liberation Serif'"/></office:font-face-decls><office:styles><style:default-style style:family="paragraph"><style:text-properties style:font-name="Liberation Serif" fo:font-size="12pt" {lang}/></style:default-style><style:style style:name="Standard" style:family="paragraph" style:class="text"/></office:styles><office:automatic-styles><style:page-layout style:name="pm1"><style:page-layout-properties fo:page-width="{width}" fo:page-height="{height}" fo:margin-top="{margin}" fo:margin-bottom="{margin}" fo:margin-left="{margin}" fo:margin-right="{margin}" style:print-orientation="portrait"/></style:page-layout></office:automatic-styles><office:master-styles><style:master-page style:name="Standard" style:page-layout-name="pm1"/></office:master-styles></office:document-styles>"#
    );
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content {NS}><office:automatic-styles/><office:body><office:text><text:p text:style-name="Standard"/></office:text></office:body></office:document-content>"#
    );
    assemble(media_type::TEXT, &content, &styles)
}

/// A blank spreadsheet: one sheet of the given name.
pub fn spreadsheet(language: &str, sheet: &str) -> Vec<u8> {
    let (width, height, margin) = Paper::for_language(language).sizes();
    let lang = language_attributes(language);
    let sheet = escape(sheet);
    let styles = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles {NS}><office:font-face-decls><style:font-face style:name="Liberation Sans" svg:font-family="'Liberation Sans'"/></office:font-face-decls><office:styles><style:default-style style:family="table-cell"><style:text-properties style:font-name="Liberation Sans" fo:font-size="10pt" {lang}/></style:default-style><style:style style:name="Default" style:family="table-cell"/></office:styles><office:automatic-styles><style:page-layout style:name="pm1"><style:page-layout-properties fo:page-width="{width}" fo:page-height="{height}" fo:margin-top="{margin}" fo:margin-bottom="{margin}" fo:margin-left="{margin}" fo:margin-right="{margin}" style:print-orientation="portrait"/></style:page-layout></office:automatic-styles><office:master-styles><style:master-page style:name="Default" style:page-layout-name="pm1"/></office:master-styles></office:document-styles>"#
    );
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content {NS}><office:automatic-styles/><office:body><office:spreadsheet><table:table table:name="{sheet}"><table:table-column table:default-cell-style-name="Default"/><table:table-row><table:table-cell/></table:table-row></table:table></office:spreadsheet></office:body></office:document-content>"#
    );
    assemble(media_type::SPREADSHEET, &content, &styles)
}

/// A blank presentation: one wide slide with an empty title.
pub fn presentation(language: &str) -> Vec<u8> {
    let lang = language_attributes(language);
    let styles = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles {NS}><office:font-face-decls><style:font-face style:name="Liberation Sans" svg:font-family="'Liberation Sans'"/></office:font-face-decls><office:styles><style:default-style style:family="graphic"><style:graphic-properties draw:fill="none" draw:stroke="none"/><style:text-properties style:font-name="Liberation Sans" fo:font-size="18pt" {lang}/></style:default-style></office:styles><office:automatic-styles><style:page-layout style:name="PM1"><style:page-layout-properties fo:page-width="28cm" fo:page-height="15.75cm" fo:margin-top="0cm" fo:margin-bottom="0cm" fo:margin-left="0cm" fo:margin-right="0cm" style:print-orientation="landscape"/></style:page-layout></office:automatic-styles><office:master-styles><style:master-page style:name="Default" style:page-layout-name="PM1"/></office:master-styles></office:document-styles>"#
    );
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content {NS}><office:automatic-styles><style:style style:name="title" style:family="presentation"><style:graphic-properties draw:textarea-vertical-align="middle"/></style:style><style:style style:name="P1" style:family="paragraph"><style:paragraph-properties fo:text-align="center"/><style:text-properties fo:font-size="40pt"/></style:style></office:automatic-styles><office:body><office:presentation><draw:page draw:name="page1" draw:master-page-name="Default"><draw:frame presentation:style-name="title" presentation:class="title" svg:x="1.4cm" svg:y="0.6cm" svg:width="25.2cm" svg:height="2.6cm"><draw:text-box><text:p text:style-name="P1"/></draw:text-box></draw:frame></draw:page></office:presentation></office:body></office:document-content>"#
    );
    assemble(media_type::PRESENTATION, &content, &styles)
}

/// The package around a content part and a styles part.
fn assemble(media_type: &str, content: &str, styles: &str) -> Vec<u8> {
    let manifest = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3"><manifest:file-entry manifest:full-path="/" manifest:media-type="{media_type}"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/><manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/><manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/></manifest:manifest>"#
    );
    let meta = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-meta {NS}><office:meta><meta:generator>odox</meta:generator></office:meta></office:document-meta>"#
    );
    let part = |name: &str, data: &str, stored: bool| Part {
        name: name.to_owned(),
        data: data.as_bytes().to_vec(),
        stored,
        directory: false,
    };
    Package::from_parts(vec![
        part("mimetype", media_type, true),
        part("META-INF/manifest.xml", &manifest, false),
        part("content.xml", content, false),
        part("styles.xml", styles, false),
        part("meta.xml", &meta, false),
    ])
    .write()
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Presentation, SheetDocument, TextDocument};

    #[test]
    fn paper_follows_the_country() {
        assert_eq!(Paper::for_language("en-US"), Paper::Letter);
        assert_eq!(Paper::for_language("fr_CA"), Paper::Letter);
        assert_eq!(Paper::for_language("en-GB"), Paper::A4);
        assert_eq!(Paper::for_language("de"), Paper::A4);
    }

    #[test]
    fn each_blank_document_reads_and_writes_back() {
        let mut text = TextDocument::read(&text("en-US")).expect("a text document");
        assert!(text.document.write_verified().is_ok());
        let mut sheet =
            SheetDocument::read(&spreadsheet("de-DE", "Tabelle1")).expect("a spreadsheet");
        assert_eq!(sheet.sheets()[0].name, "Tabelle1");
        assert!(sheet.document.write_verified().is_ok());
        let mut deck = Presentation::read(&presentation("en")).expect("a presentation");
        assert_eq!(deck.slides().len(), 1);
        assert!(deck.document.write_verified().is_ok());
    }
}
