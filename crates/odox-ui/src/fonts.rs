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
//! are resolved when it opens and handed over in one call. A character typed
//! later that no loaded face has is given one then, which is the only other
//! time the definitions change.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use eframe::egui::{Context, Event, FontData, FontDefinitions, FontFamily, Id, ImeEvent};
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

/// The fonts a window draws with and the characters already checked against
/// them, kept so that a character typed or pasted later that none of them has
/// can be given a face then.
struct Installed {
    definitions: FontDefinitions,
    checked: BTreeSet<char>,
}

fn installed_id() -> Id {
    Id::new("odox-fonts-installed")
}

/// Hand a window the fonts for a document, and keep them for
/// [`follow_input`].
pub fn install(ctx: &Context, parts: &[&Element]) {
    let checked = characters_used(parts);
    let definitions = definitions(&families_used(parts), &checked);
    ctx.set_fonts(definitions.clone());
    let installed = Arc::new(Mutex::new(Installed {
        definitions,
        checked,
    }));
    ctx.data_mut(|data| data.insert_temp(installed_id(), installed));
}

/// Give a face to a character typed, pasted or composed this frame that none
/// of the window's fonts has.
///
/// Each character is checked once, so typing costs a lookup; the glyph atlas
/// is rebuilt only when a face is added, which for a script the document did
/// not hold is its first character. The face takes effect from the next
/// frame, and the family names do not change, so nothing laid out this frame
/// asks for one that is not there.
pub fn follow_input(ctx: &Context) {
    let typed: BTreeSet<char> = ctx.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Text(text)
                | Event::Paste(text)
                | Event::Ime(ImeEvent::Commit(text) | ImeEvent::Preedit { text, .. }) => Some(text),
                _ => None,
            })
            .flat_map(|text| text.chars())
            .collect()
    });
    if typed.is_empty() {
        return;
    }
    let Some(installed) = ctx.data(|data| data.get_temp::<Arc<Mutex<Installed>>>(installed_id()))
    else {
        return;
    };
    let Ok(mut installed) = installed.lock() else {
        return;
    };
    let fresh: Vec<char> = typed
        .into_iter()
        .filter(|c| !installed.checked.contains(c))
        .collect();
    if fresh.is_empty() {
        return;
    }
    installed.checked.extend(fresh.iter().copied());
    if cover(&mut installed.definitions, fresh, None) {
        ctx.set_fonts(installed.definitions.clone());
    }
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

    cover(&mut definitions, characters.iter().copied(), Some(database));
    definitions
}

/// Add a machine face for each character no face in the definitions has, at
/// the end of every chain. Answers whether any was added. The machine's fonts
/// are listed only once a character needs them.
fn cover(
    definitions: &mut FontDefinitions,
    characters: impl IntoIterator<Item = char>,
    mut database: Option<odox_fonts::fontdb::Database>,
) -> bool {
    let mut added = Vec::new();
    for character in characters {
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
        let database = database.get_or_insert_with(odox_fonts::database);
        let Some(face) = odox_fonts::faces_with(database, character, Variant::default())
            .next()
            .and_then(|id| load(database, id))
        else {
            continue;
        };
        let count = definitions
            .font_data
            .keys()
            .filter(|key| key.starts_with(FALLBACK))
            .count();
        let key = format!("{FALLBACK} {count}");
        definitions.font_data.insert(key.clone(), Arc::new(face));
        added.push(key);
    }
    for chain in definitions.families.values_mut() {
        chain.extend(added.iter().cloned());
    }
    !added.is_empty()
}

/// What a face added for characters the document's own fonts lack is called.
const FALLBACK: &str = "machine fallback";

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

    #[test]
    fn a_character_typed_that_no_face_has_is_given_one_once() {
        let database = odox_fonts::database();
        if odox_fonts::faces_with(&database, '日', Variant::default())
            .next()
            .is_none()
        {
            eprintln!("no face on this machine has 日: nothing to check");
            return;
        }
        let ctx = Context::default();
        install(&ctx, &[]);
        let fallbacks = |ctx: &Context| {
            let installed = ctx
                .data(|data| data.get_temp::<Arc<Mutex<Installed>>>(installed_id()))
                .expect("installed");
            let installed = installed.lock().expect("not poisoned");
            installed
                .definitions
                .font_data
                .keys()
                .filter(|key| key.starts_with(FALLBACK))
                .count()
        };
        let type_in = |text: &str| {
            let input = eframe::egui::RawInput {
                events: vec![Event::Text(text.to_owned())],
                ..Default::default()
            };
            ctx.run_ui(input, |ui| follow_input(ui.ctx()))
                .textures_delta
                .clear();
        };
        type_in("a");
        assert_eq!(fallbacks(&ctx), 0, "egui's own font draws a");
        type_in("日");
        assert_eq!(fallbacks(&ctx), 1);
        type_in("日本");
        assert_eq!(fallbacks(&ctx), 1, "the face added draws the rest too");
    }
}
