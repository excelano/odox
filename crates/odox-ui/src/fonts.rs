//! Resolving the font families a document names to faces on this machine.
//!
//! Nothing is embedded. A document names a family — `Liberation Serif`, `Times
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

/// The four faces a family is asked for.
///
/// ODF says bold and italic per run, and a renderer that synthesized them by
/// skewing and thickening the regular face would be drawing something no font
/// designer made. So each combination is its own face, and its own egui family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Variant {
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
}

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

    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    set_generics(&mut database);

    for family in families {
        for variant in [
            Variant {
                bold: false,
                italic: false,
            },
            Variant {
                bold: true,
                italic: false,
            },
            Variant {
                bold: false,
                italic: true,
            },
            Variant {
                bold: true,
                italic: true,
            },
        ] {
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
fn load(database: &fontdb::Database, family: &str, variant: Variant) -> Option<FontData> {
    let mut wanted = vec![fontdb::Family::Name(family)];
    // The metrically compatible substitute, where the family is one of the
    // handful that has a well-known one. A document laid out in Times New Roman
    // on a machine that has Liberation Serif keeps its line breaks; the same
    // document in whatever the generic serif happens to be does not.
    wanted.extend(
        metric_substitutes(family)
            .iter()
            .map(|name| fontdb::Family::Name(name)),
    );
    // A generic family last, so that a document naming something absent lands on
    // a face of roughly the right shape rather than on the first font in
    // alphabetical order.
    wanted.push(generic(family));
    let query = fontdb::Query {
        families: &wanted,
        weight: if variant.bold {
            fontdb::Weight::BOLD
        } else {
            fontdb::Weight::NORMAL
        },
        stretch: fontdb::Stretch::Normal,
        style: if variant.italic {
            fontdb::Style::Italic
        } else {
            fontdb::Style::Normal
        },
    };
    let id = database.query(&query)?;
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

/// The families that are metrically compatible with the ones office documents
/// name, in the order to try them.
///
/// Each pair here has the same advance widths as the family it stands in for, so
/// a document laid out in one and drawn in the other breaks its lines in the same
/// places. The list is short because that property is what earns a place on it:
/// a face that merely looks similar belongs to the generic fallback below.
fn metric_substitutes(family: &str) -> &'static [&'static str] {
    match family.to_ascii_lowercase().as_str() {
        "times new roman" | "times" | "timesnewroman" => &["Liberation Serif", "DejaVu Serif"],
        "arial" | "helvetica" | "arialmt" => &["Liberation Sans", "DejaVu Sans"],
        "courier new" | "courier" => &["Liberation Mono", "DejaVu Sans Mono"],
        "calibri" => &["Carlito"],
        "cambria" => &["Caladea"],
        "liberation serif" => &["DejaVu Serif"],
        "liberation sans" => &["DejaVu Sans"],
        "liberation mono" => &["DejaVu Sans Mono"],
        _ => &[],
    }
}

/// Point the generic families at faces this machine has.
///
/// `fontdb` names Times New Roman, Arial and Courier New as its own defaults,
/// which is right on Windows and wrong on every Linux box: the generic fallback
/// resolves to nothing at all, and a document naming a family nobody has draws in
/// egui's built-in face rather than in anything the document asked for. Measured
/// on Debian 13, where none of the three exists.
fn set_generics(database: &mut fontdb::Database) {
    let present = |database: &fontdb::Database, name: &str| {
        database
            .query(&fontdb::Query {
                families: &[fontdb::Family::Name(name)],
                weight: fontdb::Weight::NORMAL,
                stretch: fontdb::Stretch::Normal,
                style: fontdb::Style::Normal,
            })
            .is_some()
    };
    let first = |database: &fontdb::Database, names: &[&str]| {
        names
            .iter()
            .find(|name| present(database, name))
            .map(|name| (*name).to_owned())
    };

    if let Some(name) = first(
        database,
        &[
            "Liberation Serif",
            "DejaVu Serif",
            "Noto Serif",
            "Times New Roman",
            "Georgia",
        ],
    ) {
        database.set_serif_family(name);
    }
    if let Some(name) = first(
        database,
        &[
            "Liberation Sans",
            "DejaVu Sans",
            "Noto Sans",
            "Arial",
            "Helvetica",
        ],
    ) {
        database.set_sans_serif_family(name);
    }
    if let Some(name) = first(
        database,
        &[
            "Liberation Mono",
            "DejaVu Sans Mono",
            "Noto Sans Mono",
            "Courier New",
        ],
    ) {
        database.set_monospace_family(name);
    }
}

/// The generic family a name suggests, for a machine that does not have it.
///
/// The names are the ones office documents actually carry. It is a short list on
/// purpose: guessing from the name is what produces a serif document drawn in a
/// sans face, so anything unrecognized asks for the default proportional family
/// rather than for a shape.
fn generic(family: &str) -> fontdb::Family<'static> {
    let lower = family.to_ascii_lowercase();
    if lower.contains("mono") || lower.contains("courier") || lower.contains("consol") {
        return fontdb::Family::Monospace;
    }
    if lower.contains("times") || lower.contains("serif") || lower.contains("georgia") {
        // `Liberation Sans` contains neither, and `Liberation Serif` contains
        // `serif`, which is why the sans test comes second rather than first.
        return fontdb::Family::Serif;
    }
    fontdb::Family::SansSerif
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
