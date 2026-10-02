//! Resolving the font families a document names to faces on this machine.
//!
//! A document names a family — `Liberation Serif`, `Times New Roman`, `Arial` —
//! and the machine is asked for it, falling back to a metrically compatible
//! substitute and then to a generic family of roughly the right shape. The
//! window draws with the face this answers and a PDF export embeds the same
//! one, so the two agree on what a document looks like. DESIGN.md §6.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

#![forbid(unsafe_code)]

pub use fontdb;

/// The four faces a family is asked for.
///
/// ODF says bold and italic per run, and a renderer that synthesized them by
/// skewing and thickening the regular face would be drawing something no font
/// designer made. So each combination is its own face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Variant {
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
}

impl Variant {
    /// All four, regular first.
    pub const ALL: [Self; 4] = [
        Self {
            bold: false,
            italic: false,
        },
        Self {
            bold: true,
            italic: false,
        },
        Self {
            bold: false,
            italic: true,
        },
        Self {
            bold: true,
            italic: true,
        },
    ];
}

/// The machine's fonts, with the generic families pointed at faces it has.
pub fn database() -> fontdb::Database {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    set_generics(&mut database);
    database
}

/// The face a family and variant resolve to: the family itself, then its
/// metric substitutes, then the generic family its name suggests.
pub fn find(database: &fontdb::Database, family: &str, variant: Variant) -> Option<fontdb::ID> {
    let mut wanted = vec![fontdb::Family::Name(family)];
    wanted.extend(substitute_families(family));
    query(database, &wanted, variant)
}

/// The face a family resolves to when the family itself may not be used: its
/// metric substitutes, then the generic family its name suggests.
pub fn find_substitute(
    database: &fontdb::Database,
    family: &str,
    variant: Variant,
) -> Option<fontdb::ID> {
    query(database, &substitute_families(family), variant)
}

fn substitute_families(family: &str) -> Vec<fontdb::Family<'static>> {
    // A document laid out in Times New Roman on a machine that has Liberation
    // Serif keeps its line breaks; the same document in whatever the generic
    // serif happens to be does not. The generic comes last, so that a document
    // naming something absent lands on a face of roughly the right shape rather
    // than on the first font in alphabetical order.
    let mut wanted: Vec<fontdb::Family<'static>> = metric_substitutes(family)
        .iter()
        .map(|name| fontdb::Family::Name(name))
        .collect();
    wanted.push(generic(family));
    wanted
}

fn query(
    database: &fontdb::Database,
    families: &[fontdb::Family<'_>],
    variant: Variant,
) -> Option<fontdb::ID> {
    database.query(&fontdb::Query {
        families,
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
    })
}

/// Whether a face has a glyph for a character.
pub fn covers(data: &[u8], index: u32, character: char) -> bool {
    ttf_parser::Face::parse(data, index)
        .ok()
        .and_then(|face| face.glyph_index(character))
        .is_some_and(|glyph| glyph.0 != 0)
}

/// The faces on the machine that have a glyph for a character, best first:
/// the generic families' faces in the variant asked for, then every face in
/// the order the machine lists them. What draws a character the face a
/// document names does not have.
pub fn faces_with(
    database: &fontdb::Database,
    character: char,
    variant: Variant,
) -> impl Iterator<Item = fontdb::ID> + '_ {
    [
        fontdb::Family::SansSerif,
        fontdb::Family::Serif,
        fontdb::Family::Monospace,
    ]
    .into_iter()
    .filter_map(move |generic| query(database, &[generic], variant))
    .chain(database.faces().map(|info| info.id))
    .filter(move |&id| {
        database
            .with_face_data(id, |data, index| covers(data, index, character))
            .unwrap_or(false)
    })
}

/// The families that are metrically compatible with the ones office documents
/// name, in the order to try them.
///
/// Each pair here has the same advance widths as the family it stands in for, so
/// a document laid out in one and drawn in the other breaks its lines in the same
/// places. The list is short because that property is what earns a place on it:
/// a face that merely looks similar belongs to the generic fallback.
pub fn metric_substitutes(family: &str) -> &'static [&'static str] {
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
/// resolves to nothing at all. Measured on Debian 13, where none of the three
/// exists.
fn set_generics(database: &mut fontdb::Database) {
    let present = |database: &fontdb::Database, name: &str| {
        query(database, &[fontdb::Family::Name(name)], Variant::default()).is_some()
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
pub fn generic(family: &str) -> fontdb::Family<'static> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_substitutes_matches_regardless_of_case_or_spaces() {
        assert_eq!(
            metric_substitutes("Times New Roman"),
            ["Liberation Serif", "DejaVu Serif"]
        );
        assert_eq!(
            metric_substitutes("TIMES NEW ROMAN"),
            ["Liberation Serif", "DejaVu Serif"]
        );
        assert_eq!(
            metric_substitutes("TimesNewRoman"),
            ["Liberation Serif", "DejaVu Serif"]
        );
        assert_eq!(metric_substitutes("Calibri"), ["Carlito"]);
    }

    #[test]
    fn metric_substitutes_is_empty_for_a_family_with_no_known_substitute() {
        assert_eq!(metric_substitutes("Comic Sans MS"), &[] as &[&str]);
    }

    /// `Liberation Mono` contains neither "times" nor "serif", but a family
    /// named for a monospace face still has to be caught before falling
    /// through to the serif check.
    #[test]
    fn generic_recognises_monospace_families() {
        assert_eq!(generic("Liberation Mono"), fontdb::Family::Monospace);
        assert_eq!(generic("Courier New"), fontdb::Family::Monospace);
        assert_eq!(generic("Consolas"), fontdb::Family::Monospace);
    }

    #[test]
    fn generic_recognises_serif_families() {
        assert_eq!(generic("Times New Roman"), fontdb::Family::Serif);
        assert_eq!(generic("Liberation Serif"), fontdb::Family::Serif);
        assert_eq!(generic("Georgia"), fontdb::Family::Serif);
    }

    /// `Liberation Sans` contains "sans", not "serif", so the serif check must
    /// not catch it: if it did, every sans-serif document would draw with
    /// serifs.
    #[test]
    fn generic_defaults_to_sans_serif_rather_than_matching_serif_by_accident() {
        assert_eq!(generic("Liberation Sans"), fontdb::Family::SansSerif);
        assert_eq!(generic("Wingdings"), fontdb::Family::SansSerif);
    }
}
