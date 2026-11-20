//! Runs drawn off the baseline of the text beside them.
//!
//! egui places a run against the top or the bottom of its row, so a
//! superscript on a line that also holds larger text would rise to the top of
//! that line. A paragraph is laid out as egui lays it out, with every run on
//! the baseline, and each raised glyph is then moved to stand where its
//! [`crate::ParagraphJob::set_rise`] said. An underline stays on the line and
//! is not moved.
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::sync::Arc;

use egui::epaint::text::ByteRangeExt as _;
use egui::{Galley, Rect};

/// The galley with each raised glyph moved off the baseline of the nearest
/// glyph on its row that is not raised, by the rise its section was appended
/// with, or the same galley where nothing is raised. `rises` is
/// [`crate::ParagraphJob::rises`] for the job the galley was laid out from. A
/// raised glyph with nothing else on its row stays where egui put it, and so
/// does every glyph of a galley whose glyphs do not follow its job's text one
/// character to a glyph.
pub fn raise(galley: Arc<Galley>, rises: &[f32]) -> Arc<Galley> {
    let rise = |section: usize| rises.get(section).copied().unwrap_or(0.0);
    if !(0..galley.job.sections.len()).any(|section| rise(section) != 0.0) {
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
        let row_rises: Vec<f32> = sections
            .by_ref()
            .take(placed.row.glyphs.len())
            .map(rise)
            .collect();
        let shifts: Vec<(usize, f32)> = (0..row_rises.len())
            .filter(|&index| row_rises[index] != 0.0)
            .filter_map(|index| {
                let base = (0..index)
                    .rev()
                    .chain(index + 1..row_rises.len())
                    .find(|&other| row_rises[other] == 0.0)?;
                let target = placed.row.glyphs[base].pos.y - row_rises[index];
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
