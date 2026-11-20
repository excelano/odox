//! Superscripts and subscripts placed against the text they sit in.
//!
//! egui places a run against the top or the bottom of its row, so a superscript
//! on a line that also holds larger text would rise to the top of that line.
//! The paragraph is laid out as egui lays it out, and each script glyph is then
//! moved to stand off the baseline of the text beside it, by the proportions
//! the PDF export uses.
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::sync::Arc;

use eframe::egui::{Galley, Rect, TextFormat};

use eframe::egui::epaint::text::ByteRangeExt as _;

use crate::format::{SUBSCRIPT, SUPERSCRIPT};

/// How far a superscript's baseline stands above the text it sits in, as a
/// proportion of that text's size.
const RISE: f32 = 0.33;

/// How far a subscript's baseline stands below it.
const FALL: f32 = 0.08;

/// The galley with its superscripts and subscripts moved, or the same galley
/// where it has none. A script with no other text on its row stays where egui
/// put it.
pub fn place(galley: Arc<Galley>) -> Arc<Galley> {
    let job = Arc::clone(&galley.job);
    if !job.sections.iter().any(|s| is_script(&s.format)) {
        return galley;
    }
    let Some(sections) = glyph_sections(&galley) else {
        return galley;
    };
    let pixels = galley.pixels_per_point;
    let mut galley = Arc::unwrap_or_clone(galley);
    let mut sections = sections.into_iter();
    let mut bounds: Option<Rect> = None;
    for placed in &mut galley.rows {
        let formats: Vec<&TextFormat> = sections
            .by_ref()
            .take(placed.row.glyphs.len())
            .map(|section| &job.sections[section].format)
            .collect();
        let shifts: Vec<(usize, f32)> = (0..formats.len())
            .filter_map(|index| {
                let offset = match formats[index].valign {
                    SUPERSCRIPT => -RISE,
                    SUBSCRIPT => FALL,
                    _ => return None,
                };
                let base = (0..index)
                    .rev()
                    .chain(index + 1..formats.len())
                    .find(|&other| !is_script(formats[other]))?;
                let target = placed.row.glyphs[base].pos.y + offset * formats[base].font_id.size;
                let shift = target - placed.row.glyphs[index].pos.y;
                Some((index, (shift * pixels).round() / pixels))
            })
            .collect();
        if shifts.is_empty() {
            continue;
        }
        let row = Arc::make_mut(&mut placed.row);
        for (index, shift) in shifts {
            let glyph = &mut row.glyphs[index];
            glyph.pos.y += shift;
            if !glyph.uv_rect.is_nothing() {
                let first = glyph.first_vertex as usize;
                for vertex in &mut row.visuals.mesh.vertices[first..first + 4] {
                    vertex.pos.y += shift;
                }
            }
        }
        row.visuals.mesh_bounds = row.visuals.mesh.calc_bounds();
        let moved = row.visuals.mesh_bounds.translate(placed.pos.to_vec2());
        bounds = Some(bounds.map_or(moved, |all| all.union(moved)));
    }
    if let Some(moved) = bounds {
        galley.mesh_bounds = galley.mesh_bounds.union(moved);
    }
    Arc::new(galley)
}

fn is_script(format: &TextFormat) -> bool {
    matches!(format.valign, SUPERSCRIPT | SUBSCRIPT)
}

/// The section each glyph of the galley was laid out from, in the order of its
/// rows, or nothing where the glyphs do not follow the job's text one
/// character to a glyph.
fn glyph_sections(galley: &Galley) -> Option<Vec<usize>> {
    let job = &galley.job;
    let mut characters = job
        .text
        .char_indices()
        .filter(|&(_, character)| character != '\n');
    let mut sections = Vec::new();
    for placed in &galley.rows {
        for glyph in &placed.row.glyphs {
            let (byte, character) = characters.next()?;
            if character != glyph.chr {
                return None;
            }
            sections.push(
                job.sections
                    .iter()
                    .position(|section| section.byte_range.as_usize().contains(&byte))?,
            );
        }
    }
    characters.next().is_none().then_some(sections)
}
