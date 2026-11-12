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
//! are resolved when it opens and handed over in one call. A character none of
//! them has is drawn from a face of the machine's that egui asks [`Fallback`]
//! for when it first lays the character out, whichever way the character
//! arrived.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui::epaint::text::{
    FallbackRequest, FontInsert, FontPriority, FontProvider, InsertFontFamily,
};
use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};
use odox_core::{Element, Ns};

pub use odox_fonts::Variant;

/// The machine's fonts, read the first time a list of them or one of them is
/// asked for after a document has opened.
fn machine() -> &'static odox_fonts::fontdb::Database {
    static DATABASE: OnceLock<odox_fonts::fontdb::Database> = OnceLock::new();
    DATABASE.get_or_init(odox_fonts::database)
}

/// The families this machine has, by the name each gives first, sorted as a
/// person reads them.
pub fn system_families() -> &'static [String] {
    static FAMILIES: OnceLock<Vec<String>> = OnceLock::new();
    FAMILIES.get_or_init(|| {
        let mut families: Vec<String> = machine()
            .faces()
            .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
            .collect();
        families.sort_by_key(|family| family.to_lowercase());
        families.dedup();
        families
    })
}

/// Hand a window the faces of a family a person has just picked, which the
/// document did not name when it opened. A face the window has already is not
/// read again, and a character the face lacks is asked of [`Fallback`].
pub fn add_family(ctx: &Context, family: &str) {
    let database = machine();
    for variant in Variant::ALL {
        let named = family_of(family, variant);
        let FontFamily::Name(key) = &named else {
            continue;
        };
        let Some(data) =
            odox_fonts::find(database, family, variant).and_then(|id| load(database, id))
        else {
            continue;
        };
        ctx.add_font(FontInsert::new(
            key,
            data,
            vec![InsertFontFamily {
                family: named.clone(),
                priority: FontPriority::Highest,
            }],
        ));
    }
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

/// The font definitions for a document, from its parts: the families its
/// styles name.
pub fn for_document(parts: &[&Element]) -> FontDefinitions {
    definitions(&families_used(parts))
}

/// Hand a window the fonts for a document.
pub fn install(ctx: &Context, parts: &[&Element]) {
    ctx.set_fonts(for_document(parts));
}

/// Have a window ask [`Fallback`] for a character none of its fonts has, for
/// as long as it is open.
pub fn fall_back(ctx: &Context) {
    static FALLBACK: OnceLock<Arc<Fallback>> = OnceLock::new();
    let fallback = FALLBACK.get_or_init(|| Arc::new(Fallback::default()));
    ctx.set_font_providers(vec![Arc::clone(fallback) as Arc<dyn FontProvider>]);
}

/// Build the font definitions for a document: egui's own, plus a face for every
/// family the document names.
///
/// The fallback chain behind each face is egui's built-in proportional font and
/// its emoji fonts, so a glyph the document's own face lacks is still drawn
/// rather than shown as a box. Behind those, egui asks [`Fallback`].
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
    definitions
}

/// The machine's faces, asked for a character no font of a family has: the
/// first face `odox-fonts` finds with it, in the family's variant, which is
/// the face a PDF export embeds for it.
///
/// egui asks once for each family and character, as it lays the character out
/// for the first time; the answer for a character and variant is kept here
/// across families and documents, so the machine is searched once for each.
/// The machine's fonts are listed only when a character first needs them.
#[derive(Default)]
pub struct Fallback {
    state: Mutex<FallbackState>,
}

#[derive(Default)]
struct FallbackState {
    database: Option<odox_fonts::fontdb::Database>,
    found: HashMap<(char, Variant), Option<odox_fonts::fontdb::ID>>,
    faces: HashMap<odox_fonts::fontdb::ID, FontData>,
}

impl FontProvider for Fallback {
    fn font_for(&self, request: &FallbackRequest<'_>) -> Option<FontInsert> {
        let character = request.cluster.chars().next()?;
        if character.is_whitespace() || character.is_control() {
            return None;
        }
        let variant = variant_of(request.family);
        let mut state = self.state.lock().ok()?;
        let FallbackState {
            database,
            found,
            faces,
        } = &mut *state;
        let database = database.get_or_insert_with(odox_fonts::database);
        let id = (*found
            .entry((character, variant))
            .or_insert_with(|| odox_fonts::faces_with(database, character, variant).next()))?;
        let data = match faces.entry(id) {
            Entry::Occupied(known) => known.get().clone(),
            Entry::Vacant(new) => new.insert(load(database, id)?).clone(),
        };
        Some(FontInsert::new(
            &format!("{FALLBACK} {id:?}"),
            data,
            Vec::new(),
        ))
    }
}

/// The variant a family built by [`family_of`] is in.
fn variant_of(family: &FontFamily) -> Variant {
    let FontFamily::Name(name) = family else {
        return Variant::default();
    };
    Variant {
        bold: name.ends_with(":bold") || name.ends_with(":bolditalic"),
        italic: name.ends_with(":italic") || name.ends_with(":bolditalic"),
    }
}

/// What a face of the machine's drawing a character a family lacks is called.
const FALLBACK: &str = "machine fallback";

/// One face of the machine's, read.
fn load(database: &odox_fonts::fontdb::Database, id: odox_fonts::fontdb::ID) -> Option<FontData> {
    let index = database.face(id)?.index;
    database.with_face_data(id, |data, face_index| {
        // A font collection holds several faces in one file and `fontdb` reports
        // which of them answered; handing over the file without the index draws
        // the wrong one.
        FontData::from_blob(Arc::new(data.to_vec()), face_index.max(index))
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

    fn has(character: char) -> bool {
        let database = odox_fonts::database();
        let found = odox_fonts::faces_with(&database, character, Variant::default())
            .next()
            .is_some();
        if !found {
            eprintln!("no face on this machine has {character}: nothing to check");
        }
        found
    }

    #[test]
    fn a_character_no_face_has_is_drawn_from_one_of_the_machines() {
        if !has('日') {
            return;
        }
        let fallback = Fallback::default();
        let family = family_of("Liberation Serif", Variant::default());
        let ask = |cluster: &str| {
            fallback.font_for(&FallbackRequest {
                cluster,
                family: &family,
            })
        };
        let insert = ask("日").expect("a face");
        assert!(odox_fonts::covers(
            insert.data.bytes(),
            insert.data.index,
            '日'
        ));
        assert_eq!(ask("日").map(|again| again.name), Some(insert.name));
        assert!(ask(" ").is_none(), "a space is not looked for");
    }

    #[test]
    fn a_bold_family_asks_for_a_bold_face() {
        assert_eq!(
            variant_of(&family_of(
                "Arial",
                Variant {
                    bold: true,
                    italic: true
                }
            )),
            Variant {
                bold: true,
                italic: true
            }
        );
        assert_eq!(variant_of(&FontFamily::Proportional), Variant::default());
    }

    #[test]
    fn a_window_draws_a_character_its_fonts_lack_from_the_machine() {
        if !has('日') {
            return;
        }
        let ctx = Context::default();
        fall_back(&ctx);
        install(&ctx, &[]);
        let mut job = eframe::egui::text::LayoutJob::default();
        job.append("日本", 0.0, eframe::egui::TextFormat::default());
        for _ in 0..2 {
            ctx.run_ui(eframe::egui::RawInput::default(), |ui| {
                ui.fonts_mut(|fonts| fonts.layout_job(job.clone()));
            })
            .textures_delta
            .clear();
        }
        let discovered = ctx.fonts(|fonts| {
            fonts
                .discovered_fonts()
                .iter()
                .map(|insert| insert.name.clone())
                .collect::<Vec<_>>()
        });
        assert_eq!(discovered.len(), 1, "one face draws both: {discovered:?}");
        assert!(discovered[0].starts_with(FALLBACK));
    }
}
