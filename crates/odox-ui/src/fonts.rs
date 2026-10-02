//! Handing egui the faces a document's font families resolve to on this
//! machine; which face that is, is `odox-fonts`' answer.
//!
//! The applications carry no fonts. A document names a family — `Liberation Serif`, `Times
//! New Roman`, `Arial` — and the machine is asked for it, because every platform
//! this ships on carries a metrically compatible face for the families office
//! documents use, and three applications carrying a megabyte of fonts each would
//! be three megabytes spent on a question the operating system has already
//! answered. The cost is that a document naming a family the machine does not
//! have is drawn in a fallback, which is what every other application does too.
//!
//! Faces are loaded once per document, not once per frame: egui rebuilds its
//! glyph atlas when the font definitions change, so the families a document uses
//! are resolved when it opens and handed over in one call.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use eframe::egui::{FontData, FontDefinitions, FontFamily};
use odox_core::{Element, Ns};

pub use odox_fonts::Variant;

/// The egui font family a document's family name and variant resolve to.
///
/// The name is the one the document used, so that two documents naming the same
/// family share an atlas entry and a document naming a family nobody has still
/// gets a family that exists and draws in the fallback.
pub fn family_of(family: &str, variant: Variant) -> FontFamily {
    let suffix = match (variant.bold, variant.italic) {
        (false, false) => "",
        (true, false) => ":bold",
        (false, true) => ":italic",
        (true, true) => ":bolditalic",
    };
    FontFamily::Name(format!("{family}{suffix}").into())
}

/// Build the font definitions for a document: egui's own, plus a face for every
/// family the document names.
///
/// The fallback chain behind each face is egui's built-in proportional font and
/// its emoji fonts, so a glyph the document's own face lacks is still drawn
/// rather than shown as a box.
pub fn definitions(families: &BTreeSet<String>) -> FontDefinitions {
    let mut definitions = FontDefinitions::default();
    let fallback = definitions
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    let database = odox_fonts::database();

    for family in families {
        for variant in Variant::ALL {
            let key = match family_of(family, variant) {
                FontFamily::Name(name) => name.to_string(),
                // `family_of` builds a named family and nothing else; the other
                // arms exist only because `FontFamily` is an enum.
                other => format!("{other:?}"),
            };
            let mut chain = Vec::new();
            if let Some(face) = load(&database, family, variant) {
                definitions.font_data.insert(key.clone(), Arc::new(face));
                chain.push(key.clone());
            }
            chain.extend(fallback.iter().cloned());
            definitions
                .families
                .insert(FontFamily::Name(key.into()), chain);
        }
    }
    definitions
}

/// Ask the machine for one face.
fn load(
    database: &odox_fonts::fontdb::Database,
    family: &str,
    variant: Variant,
) -> Option<FontData> {
    let id = odox_fonts::find(database, family, variant)?;
    let index = database.face(id)?.index;
    database.with_face_data(id, |data, face_index| FontData {
        font: data.to_vec().into(),
        // A font collection holds several faces in one file and `fontdb` reports
        // which of them answered; handing over the file without the index draws
        // the wrong one.
        index: face_index.max(index),
        tweak: eframe::egui::FontTweak::default(),
    })
}

/// Every font family a document's styles name.
///
/// Read from the styles rather than from the text, because a family is named in a
/// style and used by whatever references it, and because the answer is wanted
/// before the first frame is drawn.
pub fn families_used(parts: &[&Element]) -> BTreeSet<String> {
    let mut families = BTreeSet::new();
    let mut faces: BTreeMap<String, String> = BTreeMap::new();
    for root in parts {
        collect(root, &mut families, &mut faces);
    }
    // A style naming a font face rather than a family resolves through the
    // declarations, and the declarations are what a renderer has to ask the
    // machine for.
    let resolved: BTreeSet<String> = families
        .iter()
        .map(|name| faces.get(name).cloned().unwrap_or_else(|| name.clone()))
        .collect();
    resolved
}

fn collect(
    element: &Element,
    families: &mut BTreeSet<String>,
    faces: &mut BTreeMap<String, String>,
) {
    if element.is(&Ns::Style, "font-face")
        && let Some(name) = element.attr(&Ns::Style, "name")
    {
        let family = element
            .attr(&Ns::Svg, "font-family")
            .unwrap_or(name)
            .trim_matches('\'')
            .to_owned();
        faces.insert(name.to_owned(), family);
    }
    for (ns, local) in [(Ns::Style, "font-name"), (Ns::Fo, "font-family")] {
        if let Some(name) = element.attr(&ns, local) {
            let name = name.trim_matches('\'');
            if !name.is_empty() {
                families.insert(name.to_owned());
            }
        }
    }
    for child in element.elements() {
        collect(child, families, faces);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_of_names_a_variant_with_a_suffix() {
        let plain = Variant {
            bold: false,
            italic: false,
        };
        let bold = Variant {
            bold: true,
            italic: false,
        };
        let italic = Variant {
            bold: false,
            italic: true,
        };
        let both = Variant {
            bold: true,
            italic: true,
        };
        assert_eq!(family_of("Arial", plain), FontFamily::Name("Arial".into()));
        assert_eq!(
            family_of("Arial", bold),
            FontFamily::Name("Arial:bold".into())
        );
        assert_eq!(
            family_of("Arial", italic),
            FontFamily::Name("Arial:italic".into())
        );
        assert_eq!(
            family_of("Arial", both),
            FontFamily::Name("Arial:bolditalic".into())
        );
    }
}
