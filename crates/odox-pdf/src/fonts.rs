//! The faces an export draws with: the ones the window draws with, as
//! `odox-fonts` resolves them, so long as each may be embedded.
//!
//! A PDF/UA file carries its fonts, and only a face whose licence allows that
//! is embedded. A face that forbids it is replaced by its metric substitute or
//! the generic family, and the substitution is reported. A character the
//! chosen face lacks is drawn from the first face on the machine that has it,
//! as the window draws it from its fallback chain, because a PDF/UA file may
//! not hold a glyph that stands for no character.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::HashMap;
use std::sync::Arc;

use odox_fonts::{Variant, fontdb};
use rustybuzz::ttf_parser;

use crate::Refusal;

/// One face, loaded.
pub(crate) struct Face {
    /// The face's bytes and its index in them, which shaping reads.
    pub data: Arc<Vec<u8>>,
    pub index: u32,
    /// The same face as krilla embeds it.
    pub font: krilla::text::Font,
    /// Font units per em, which every metric below is divided by.
    pub units_per_em: f32,
    /// Above the baseline, as a proportion of the size.
    pub ascent: f32,
    /// Below it, as a positive proportion of the size.
    pub descent: f32,
    /// The gap the face asks for between lines, as a proportion of the size.
    pub gap: f32,
}

impl Face {
    /// The height of a line of this face at a size, as its designer spaced it.
    pub fn line(&self, size: f32) -> f32 {
        (self.ascent + self.descent + self.gap) * size
    }
}

/// A family that was drawn in another face because its own may not be
/// embedded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Substitution {
    /// The family the document names.
    pub family: String,
    /// The family drawn instead.
    pub used: String,
}

/// Every face an export has needed, loaded once.
pub(crate) struct Faces {
    database: fontdb::Database,
    faces: Vec<Face>,
    loaded: HashMap<fontdb::ID, usize>,
    requested: HashMap<(String, Variant), usize>,
    fallbacks: HashMap<(char, Variant), Option<usize>>,
    coverage: HashMap<(usize, char), bool>,
    pub substituted: Vec<Substitution>,
}

impl Faces {
    pub(crate) fn new() -> Self {
        Self {
            database: odox_fonts::database(),
            faces: Vec::new(),
            loaded: HashMap::new(),
            requested: HashMap::new(),
            fallbacks: HashMap::new(),
            coverage: HashMap::new(),
            substituted: Vec::new(),
        }
    }

    pub(crate) fn get(&self, face: usize) -> &Face {
        &self.faces[face]
    }

    /// The face a family and variant resolve to.
    ///
    /// # Errors
    ///
    /// Neither the family nor anything standing in for it is on the machine
    /// in a face that may be embedded.
    pub(crate) fn request(&mut self, family: &str, variant: Variant) -> Result<usize, Refusal> {
        let key = (family.to_owned(), variant);
        if let Some(&face) = self.requested.get(&key) {
            return Ok(face);
        }
        let found = odox_fonts::find(&self.database, family, variant)
            .ok_or_else(|| Refusal::Font(family.to_owned()))?;
        let face = if let Some(face) = self.load(found) {
            face
        } else {
            let substitute = odox_fonts::find_substitute(&self.database, family, variant)
                .filter(|&id| id != found)
                .and_then(|id| self.load(id))
                .ok_or_else(|| Refusal::Font(family.to_owned()))?;
            let used = self.family_name(substitute);
            let substitution = Substitution {
                family: self.family_of(found).unwrap_or_else(|| family.to_owned()),
                used,
            };
            if !self.substituted.contains(&substitution) {
                self.substituted.push(substitution);
            }
            substitute
        };
        self.requested.insert(key, face);
        Ok(face)
    }

    /// Whether a face has a glyph for a character.
    pub(crate) fn covers(&mut self, face: usize, character: char) -> bool {
        let data = &self.faces[face];
        *self
            .coverage
            .entry((face, character))
            .or_insert_with(|| odox_fonts::covers(&data.data, data.index, character))
    }

    /// The first face on the machine that has a character and may be
    /// embedded, as `odox-fonts` orders them.
    pub(crate) fn fallback(&mut self, character: char, variant: Variant) -> Option<usize> {
        if let Some(&found) = self.fallbacks.get(&(character, variant)) {
            return found;
        }
        let candidates: Vec<fontdb::ID> =
            odox_fonts::faces_with(&self.database, character, variant)
                .take(8)
                .collect();
        let found = candidates.into_iter().find_map(|id| self.load(id));
        self.fallbacks.insert((character, variant), found);
        found
    }

    /// Load a face, once, unless it may not be embedded.
    fn load(&mut self, id: fontdb::ID) -> Option<usize> {
        if let Some(&face) = self.loaded.get(&id) {
            return Some(face);
        }
        let (data, index) = self
            .database
            .with_face_data(id, |data, index| (data.to_vec(), index))?;
        let data = Arc::new(data);
        let parsed = ttf_parser::Face::parse(&data, index).ok()?;
        if !embeddable(&parsed) {
            return None;
        }
        let units_per_em = f32::from(parsed.units_per_em());
        let ascent = f32::from(parsed.ascender()) / units_per_em;
        let descent = -f32::from(parsed.descender()) / units_per_em;
        let gap = f32::from(parsed.line_gap()) / units_per_em;
        let font = krilla::text::Font::new(Arc::clone(&data).into(), index)?;
        self.faces.push(Face {
            data,
            index,
            font,
            units_per_em,
            ascent,
            descent,
            gap,
        });
        let face = self.faces.len() - 1;
        self.loaded.insert(id, face);
        Some(face)
    }

    fn family_of(&self, id: fontdb::ID) -> Option<String> {
        self.database
            .face(id)
            .and_then(|info| info.families.first())
            .map(|(name, _)| name.clone())
    }

    fn family_name(&self, face: usize) -> String {
        self.loaded
            .iter()
            .find(|&(_, &loaded)| loaded == face)
            .and_then(|(&id, _)| self.family_of(id))
            .unwrap_or_default()
    }
}

/// Whether a face's licence lets it travel inside a document: not restricted,
/// and open to subsetting, which is the only way krilla embeds a face.
fn embeddable(face: &ttf_parser::Face<'_>) -> bool {
    face.permissions() != Some(ttf_parser::Permissions::Restricted) && face.is_subsetting_allowed()
}
