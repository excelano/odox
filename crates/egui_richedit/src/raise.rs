//! Runs drawn off the baseline of the text beside them.
//!
//! egui places a run against the top or the bottom of its row, so a
//! superscript on a line that also holds larger text would rise to the top of
//! that line. A paragraph is laid out as egui lays it out, with every run on
//! the baseline, and each raised glyph is then moved to stand where its
//! [`crate::ParagraphJob::set_rise`] said. An underline stays on the line and
//! is not moved; a highlight moves with its run.
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::ops::Range;
use std::sync::Arc;

use egui::epaint::text::{ByteRangeExt as _, Glyph};
use egui::{Color32, Galley, Rect, TextFormat};

/// A raised glyph's move: the glyph, how far it moves down (up where
/// negative), and the glyph it stands off.
struct Shift {
    glyph: usize,
    by: f32,
    base: usize,
}

/// The galley with each raised glyph moved off the baseline of the nearest
/// glyph on its row that is not raised, by the rise its section was appended
/// with, or the same galley where nothing is raised. `rises` is
/// [`crate::ParagraphJob::rises`] for the job the galley was laid out from. A
/// raised glyph with nothing else on its row stays where egui put it, and so
/// does every glyph of a galley with a glyph that stands for none of its
/// job's characters.
///
/// A raised run's highlight is moved with it and made as tall as the
/// highlight of the text it stands off. A row whose highlights egui did not
/// draw one to a run, as [`highlights`] reads them, keeps them where they are.
#[allow(clippy::float_cmp)]
pub fn raise(galley: Arc<Galley>, rises: &[f32]) -> Arc<Galley> {
    let rise = |section: usize| rises.get(section).copied().unwrap_or(0.0);
    if !(0..galley.job.sections.len()).any(|section| rise(section) != 0.0) {
        return galley;
    }
    let Some(sections) = glyph_sections(&galley) else {
        return galley;
    };
    let pixels = galley.pixels_per_point;
    let round = |y: f32| (y * pixels).round() / pixels;
    let job = Arc::clone(&galley.job);
    let mut galley = Arc::unwrap_or_clone(galley);
    let mut sections = sections.into_iter();
    let mut bounds: Option<Rect> = None;
    for placed in &mut galley.rows {
        let row_sections: Vec<usize> = sections.by_ref().take(placed.row.glyphs.len()).collect();
        let row_rises: Vec<f32> = row_sections.iter().map(|&s| rise(s)).collect();
        let formats: Vec<&TextFormat> = row_sections
            .iter()
            .map(|&s| &job.sections[s].format)
            .collect();
        let shifts: Vec<Shift> = (0..row_rises.len())
            .filter(|&index| row_rises[index] != 0.0)
            .filter_map(|index| {
                let base = (0..index)
                    .rev()
                    .chain(index + 1..row_rises.len())
                    .find(|&other| row_rises[other] == 0.0)?;
                let target = placed.row.glyphs[base].pos.y - row_rises[index];
                Some(Shift {
                    glyph: index,
                    by: round(target - placed.row.glyphs[index].pos.y),
                    base,
                })
            })
            .collect();
        if shifts.is_empty() {
            continue;
        }
        let runs = highlights(&placed.row.glyphs, &formats);
        let row = Arc::make_mut(&mut placed.row);
        if runs.len() * 4 == row.visuals.glyph_vertex_range.start {
            for (rectangle, run) in runs.into_iter().enumerate() {
                let Some(first) = shifts.iter().find(|shift| shift.glyph == run.start) else {
                    continue;
                };
                let moved_whole = run.clone().all(|glyph| {
                    shifts
                        .iter()
                        .any(|shift| shift.glyph == glyph && shift.by == first.by)
                });
                if !moved_whole {
                    continue;
                }
                let base = row.glyphs[first.base].logical_rect();
                let lift = row_rises[run.start];
                let expand = formats[run.start].expand_bg;
                let corners = &mut row.visuals.mesh.vertices[rectangle * 4..rectangle * 4 + 4];
                let middle = corners.iter().map(|v| v.pos.y).sum::<f32>() / 4.0;
                for vertex in corners {
                    vertex.pos.y = if vertex.pos.y < middle {
                        round(base.top() - lift - expand)
                    } else {
                        round(base.bottom() - lift + expand)
                    };
                }
            }
        }
        for shift in shifts {
            let glyph = &mut row.glyphs[shift.glyph];
            glyph.pos.y += shift.by;
            if !glyph.uv_rect.is_nothing() {
                let first = glyph.first_vertex as usize;
                for vertex in &mut row.visuals.mesh.vertices[first..first + 4] {
                    vertex.pos.y += shift.by;
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

/// The glyphs behind which each of a row's highlight rectangles is drawn, in
/// the order egui draws them: one for each run of glyphs with the same colour
/// behind them, the same top and bottom and the same expansion.
#[allow(clippy::float_cmp)]
fn highlights(glyphs: &[Glyph], formats: &[&TextFormat]) -> Vec<Range<usize>> {
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    for (index, glyph) in glyphs.iter().enumerate() {
        let format = formats[index];
        if format.background == Color32::TRANSPARENT {
            if let Some(start) = start.take() {
                runs.push(start..index);
            }
            continue;
        }
        let joins = start.is_some_and(|start| {
            let (first, rect) = (formats[start], glyphs[start].logical_rect());
            let own = glyph.logical_rect();
            first.background == format.background
                && rect.top() == own.top()
                && rect.bottom() == own.bottom()
                && first.expand_bg == format.expand_bg
        });
        if !joins {
            if let Some(start) = start.take() {
                runs.push(start..index);
            }
            start = Some(index);
        }
    }
    if let Some(start) = start {
        runs.push(start..glyphs.len());
    }
    runs
}

/// The section each glyph of the galley was laid out from, in the order of its
/// rows, or nothing where a glyph stands for no character its job has left to
/// draw. Each glyph draws the next character it names that a section from
/// the one before it on has not drawn yet, so that a word shaped right to
/// left, whose glyphs come last character first, still draws its own. A glyph
/// that takes no room and is not drawn, where its section has no such
/// character left, is one of the glyphs egui adds to a cluster it shaped and
/// belongs to that section too.
#[allow(clippy::float_cmp)]
fn glyph_sections(galley: &Galley) -> Option<Vec<usize>> {
    let job = &galley.job;
    let mut undrawn: Vec<Vec<char>> = job
        .sections
        .iter()
        .map(|section| {
            job.text[section.byte_range.as_usize()]
                .chars()
                .filter(|&character| character != '\n')
                .collect()
        })
        .collect();
    let mut current = 0;
    let mut sections = Vec::new();
    for glyph in galley.rows.iter().flat_map(|placed| &placed.row.glyphs) {
        let takes = |section: &Vec<char>| section.contains(&glyph.chr);
        let added = glyph.advance_width == 0.0 && glyph.uv_rect.is_nothing();
        if added && !undrawn.get(current).is_some_and(takes) {
            sections.push(current);
            continue;
        }
        current += undrawn[current..].iter().position(takes)?;
        let section = &mut undrawn[current];
        let at = section
            .iter()
            .position(|&character| character == glyph.chr)?;
        section.remove(at);
        sections.push(current);
    }
    Some(sections)
}
