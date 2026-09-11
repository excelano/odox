//! What a document says about itself: `meta.xml`.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use crate::xml::{Element, Ns};

/// A document's metadata, as far as a window shows it.
///
/// Every field is optional because every field is optional in the format. The
/// whole part is optional too, and a document with no `meta.xml` gives back the
/// default of this.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Meta {
    /// `dc:title`, which a window shows in preference to the file name when the
    /// document carries one.
    pub title: Option<String>,
    /// `dc:subject`.
    pub subject: Option<String>,
    /// `dc:description`.
    pub description: Option<String>,
    /// `meta:initial-creator`, who made the document.
    pub initial_creator: Option<String>,
    /// `dc:creator`, who last changed it. ODF uses the Dublin Core term for the
    /// last editor rather than the author, which is the opposite of what the
    /// word suggests.
    pub last_editor: Option<String>,
    /// `meta:creation-date`, as written: an ISO 8601 timestamp, kept as text
    /// because nothing here does arithmetic on it and a date library is a
    /// dependency this crate would otherwise not have.
    pub created: Option<String>,
    /// `dc:date`, when it was last changed.
    pub modified: Option<String>,
    /// `meta:generator`, the application that wrote it.
    pub generator: Option<String>,
    /// Every `meta:keyword`, in order.
    pub keywords: Vec<String>,
}

impl Meta {
    /// Read the metadata out of a parsed `meta.xml`.
    pub fn read(root: &Element) -> Self {
        let mut meta = Self::default();
        let Some(body) = root.child(&Ns::Office, "meta") else {
            return meta;
        };
        for element in body.elements() {
            let text = element.plain_text();
            let slot = match () {
                () if element.is(&Ns::Dc, "title") => &mut meta.title,
                () if element.is(&Ns::Dc, "subject") => &mut meta.subject,
                () if element.is(&Ns::Dc, "description") => &mut meta.description,
                () if element.is(&Ns::Meta, "initial-creator") => &mut meta.initial_creator,
                () if element.is(&Ns::Dc, "creator") => &mut meta.last_editor,
                () if element.is(&Ns::Meta, "creation-date") => &mut meta.created,
                () if element.is(&Ns::Dc, "date") => &mut meta.modified,
                () if element.is(&Ns::Meta, "generator") => &mut meta.generator,
                () if element.is(&Ns::Meta, "keyword") => {
                    if !text.is_empty() {
                        meta.keywords.push(text);
                    }
                    continue;
                }
                () => continue,
            };
            if !text.is_empty() {
                *slot = Some(text);
            }
        }
        meta
    }
}
