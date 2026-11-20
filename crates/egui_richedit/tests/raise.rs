//! Runs moved off the baseline after egui has laid a paragraph out, checked on
//! the galley [`egui_richedit::raise`] returns.
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::sync::Arc;

use egui::epaint::text::Glyph;
use egui::text::LayoutJob;
use egui::{Color32, Context, FontId, Galley, RawInput, TextFormat};
use egui_richedit::{ParagraphJob, raise};

const RISE: f32 = 6.0;

/// Text in `size` points, highlighted where `background` is given.
fn format(size: f32, background: Option<Color32>) -> TextFormat {
    TextFormat {
        font_id: FontId::proportional(size),
        background: background.unwrap_or(Color32::TRANSPARENT),
        ..Default::default()
    }
}

/// Each run of the paragraph with the rise it is appended at, laid out by
/// egui as it is and again after [`raise`].
fn laid(runs: &[(&str, TextFormat, f32)], wrap: f32) -> (Arc<Galley>, Arc<Galley>) {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let mut job = ParagraphJob::new(job);
    for (text, format, rise) in runs {
        job.set_rise(*rise);
        job.text(text, format.clone());
    }
    let rises = job.rises().to_vec();
    let (job, _) = job.into_parts();
    let ctx = Context::default();
    let mut galleys = None;
    let mut output = ctx.run_ui(RawInput::default(), |ui| {
        let before = ui.fonts_mut(|fonts| fonts.layout_job(job.clone()));
        let after = raise(Arc::clone(&before), &rises);
        galleys = Some((before, after));
    });
    output.textures_delta.clear();
    galleys.expect("the frame ran")
}

/// The glyph on a row at a place on it.
fn glyph(galley: &Galley, row: usize, index: usize) -> &Glyph {
    &galley.rows[row].row.glyphs[index]
}

/// The top and bottom of one of a row's highlight rectangles.
fn highlight(galley: &Galley, row: usize, rectangle: usize) -> (f32, f32) {
    let corners = &galley.rows[row].row.visuals.mesh.vertices[rectangle * 4..rectangle * 4 + 4];
    let ys = corners.iter().map(|vertex| vertex.pos.y);
    (
        ys.clone().fold(f32::INFINITY, f32::min),
        ys.fold(f32::NEG_INFINITY, f32::max),
    )
}

/// Within the pixel `raise` rounds to.
fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.0
}

#[test]
fn a_paragraph_with_nothing_raised_is_the_galley_egui_laid_out() {
    let (before, after) = laid(&[("plain text", format(14.0, None), 0.0)], f32::INFINITY);
    assert!(Arc::ptr_eq(&before, &after));
}

#[test]
fn a_raised_run_stands_its_rise_above_the_baseline_of_the_text_before_it() {
    let (before, after) = laid(
        &[
            ("Big", format(24.0, None), 0.0),
            ("2", format(12.0, None), RISE),
        ],
        f32::INFINITY,
    );
    let base = glyph(&after, 0, 0).pos.y;
    let raised = glyph(&after, 0, 3);
    assert!(
        near(raised.pos.y, base - RISE),
        "the superscript's baseline is {} for a base of {base}",
        raised.pos.y
    );
    assert_eq!(glyph(&after, 0, 0).pos, glyph(&before, 0, 0).pos);
    let shift = raised.pos.y - glyph(&before, 0, 3).pos.y;
    let first = raised.first_vertex as usize;
    for (moved, laid) in after.rows[0].row.visuals.mesh.vertices[first..first + 4]
        .iter()
        .zip(&before.rows[0].row.visuals.mesh.vertices[first..first + 4])
    {
        assert!((moved.pos.y - laid.pos.y - shift).abs() < 0.01);
    }
}

#[test]
fn a_run_raised_above_its_row_is_inside_the_bounds_the_galley_is_drawn_in() {
    let (before, after) = laid(
        &[
            ("x", format(12.0, None), 0.0),
            ("x", format(12.0, None), 3.0 * RISE),
        ],
        f32::INFINITY,
    );
    let placed = &after.rows[0];
    let first = placed.row.glyphs[1].first_vertex as usize;
    let top = placed.row.visuals.mesh.vertices[first..first + 4]
        .iter()
        .map(|vertex| vertex.pos.y + placed.pos.y)
        .fold(f32::INFINITY, f32::min);
    assert!(
        top < before.mesh_bounds.top(),
        "the glyph rose above the row"
    );
    assert!(after.mesh_bounds.top() <= top);
}

#[test]
fn a_raised_run_in_the_format_of_the_text_around_it_is_raised_alone() {
    let (_, after) = laid(
        &[
            ("ab", format(14.0, None), 0.0),
            ("cd", format(14.0, None), RISE),
            ("ef", format(14.0, None), 0.0),
        ],
        f32::INFINITY,
    );
    let ys: Vec<f32> = after.rows[0]
        .row
        .glyphs
        .iter()
        .map(|glyph| glyph.pos.y)
        .collect();
    let base = ys[0];
    assert_eq!([ys[1], ys[4], ys[5]], [base; 3]);
    assert!(
        near(ys[2], base - RISE) && near(ys[3], base - RISE),
        "{ys:?}"
    );
}

#[test]
fn a_raised_run_on_a_line_with_a_word_read_right_to_left_is_raised() {
    let (_, after) = laid(
        &[
            ("x", format(24.0, None), 0.0),
            ("2", format(12.0, None), RISE),
            (" שלום", format(24.0, None), 0.0),
        ],
        f32::INFINITY,
    );
    let glyphs = &after.rows[0].row.glyphs;
    let at = |chr: char| {
        glyphs
            .iter()
            .find(|glyph| glyph.chr == chr)
            .expect("drawn")
            .pos
            .y
    };
    assert!(
        near(at('2'), at('x') - RISE),
        "{} for a base of {}",
        at('2'),
        at('x')
    );
    assert_eq!(at('ש'), at('x'));
}

#[test]
fn a_lowered_run_with_nothing_before_it_stands_off_the_text_after_it() {
    let (_, after) = laid(
        &[
            ("2", format(12.0, None), -RISE),
            ("Big", format(24.0, None), 0.0),
        ],
        f32::INFINITY,
    );
    let base = glyph(&after, 0, 1).pos.y;
    assert!(near(glyph(&after, 0, 0).pos.y, base + RISE));
}

#[test]
fn a_raised_run_alone_on_its_row_stays_where_egui_put_it() {
    let (before, after) = laid(&[("2", format(12.0, None), RISE)], f32::INFINITY);
    assert_eq!(glyph(&after, 0, 0).pos, glyph(&before, 0, 0).pos);
}

#[test]
fn each_row_of_a_wrapped_paragraph_raises_off_its_own_baseline() {
    let (_, after) = laid(
        &[
            ("one two three four five six", format(16.0, None), 0.0),
            ("2", format(10.0, None), RISE),
            (
                " seven eight nine ten eleven twelve",
                format(16.0, None),
                0.0,
            ),
            ("3", format(10.0, None), RISE),
        ],
        120.0,
    );
    assert!(
        after.rows.len() > 2,
        "the paragraph wrapped onto {} rows",
        after.rows.len()
    );
    let mut raised = 0;
    for (row, placed) in after.rows.iter().enumerate() {
        let glyphs = &placed.row.glyphs;
        for (index, glyph) in glyphs.iter().enumerate() {
            if glyph.chr == '2' || glyph.chr == '3' {
                assert!(
                    near(glyph.pos.y, glyphs[index - 1].pos.y - RISE),
                    "row {row}"
                );
                raised += 1;
            }
        }
    }
    assert_eq!(raised, 2);
}

#[test]
fn a_raised_runs_highlight_moves_with_it_as_tall_as_the_text_it_stands_off() {
    let yellow = Some(Color32::YELLOW);
    let (before, after) = laid(
        &[
            ("Big", format(24.0, yellow), 0.0),
            ("2", format(12.0, yellow), RISE),
        ],
        f32::INFINITY,
    );
    let base = glyph(&after, 0, 0).logical_rect();
    assert_eq!(highlight(&after, 0, 0), highlight(&before, 0, 0));
    let (top, bottom) = highlight(&after, 0, 1);
    assert!(near(top, base.top() - RISE), "top {top} for {base:?}");
    assert!(
        near(bottom, base.bottom() - RISE),
        "bottom {bottom} for {base:?}"
    );
}

#[test]
fn a_highlight_on_the_text_beside_a_raised_run_stays_where_it_is() {
    let (before, after) = laid(
        &[
            ("Big", format(24.0, Some(Color32::YELLOW)), 0.0),
            ("2", format(12.0, None), RISE),
        ],
        f32::INFINITY,
    );
    assert_eq!(highlight(&after, 0, 0), highlight(&before, 0, 0));
    assert_ne!(glyph(&after, 0, 3).pos, glyph(&before, 0, 3).pos);
}
