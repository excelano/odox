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

/// The font definitions for a document, from its parts: the families its
/// styles name and the characters its text holds.
pub fn for_document(parts: &[&Element]) -> FontDefinitions {
    definitions(&families_used(parts), &characters_used(parts))
}

/// Build the font definitions for a document: egui's own, plus a face for every
/// family the document names.
///
/// The fallback chain behind each face is egui's built-in proportional font and
/// its emoji fonts, so a glyph the document's own face lacks is still drawn
/// rather than shown as a box. egui's own fonts are small and cover few
/// scripts, so a character in `characters` that no face loaded so far has is
/// drawn from the first face on the machine that has it, added at the end of
/// every chain where it catches only what nothing before it draws.
pub fn definitions(families: &BTreeSet<String>, characters: &BTreeSet<char>) -> FontDefinitions {
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
            if let Some(face) =
                odox_fonts::find(&database, family, variant).and_then(|id| load(&database, id))
            {
                definitions.font_data.insert(key.clone(), Arc::new(face));
                chain.push(key.clone());
            }
            chain.extend(fallback.iter().cloned());
            definitions
                .families
                .insert(FontFamily::Name(key.into()), chain);
        }
    }

    let mut added = Vec::new();
    for &character in characters {
        if character.is_whitespace() || character.is_control() {
            continue;
        }
        let drawn = definitions
            .font_data
            .values()
            .any(|face| odox_fonts::covers(&face.font, face.index, character));
        if drawn {
            continue;
        }
        let Some(face) = odox_fonts::faces_with(&database, character, Variant::default())
            .next()
            .and_then(|id| load(&database, id))
        else {
            continue;
        };
        let key = format!("machine fallback {}", added.len());
        definitions.font_data.insert(key.clone(), Arc::new(face));
        added.push(key);
    }
    for chain in definitions.families.values_mut() {
        chain.extend(added.iter().cloned());
    }
    definitions
}

/// One face of the machine's, read.
fn load(database: &odox_fonts::fontdb::Database, id: odox_fonts::fontdb::ID) -> Option<FontData> {
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

/// Every character a document's parts hold as text.
pub fn characters_used(parts: &[&Element]) -> BTreeSet<char> {
    parts
        .iter()
        .flat_map(|part| part.plain_text().chars().collect::<Vec<_>>())
        .collect()
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

    #[test]
    fn a_character_no_loaded_face_has_is_drawn_from_one_of_the_machines() {
        let database = odox_fonts::database();
        if odox_fonts::faces_with(&database, '日', Variant::default())
            .next()
            .is_none()
        {
            eprintln!("no face on this machine has 日: nothing to check");
            return;
        }
        let families = BTreeSet::from(["Liberation Serif".to_owned()]);
        let defs = definitions(&families, &BTreeSet::from(['日', 'a']));
        let chain = &defs.families[&family_of("Liberation Serif", Variant::default())];
        let added: Vec<&String> = chain
            .iter()
            .filter(|key| key.starts_with("machine fallback"))
            .collect();
        assert_eq!(added.len(), 1, "one face for the one character missing");
        assert_eq!(chain.last(), Some(added[0]), "it comes last");
        let face = &defs.font_data[added[0]];
        assert!(odox_fonts::covers(&face.font, face.index, '日'));
    }
}
