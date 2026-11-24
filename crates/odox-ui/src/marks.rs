//! Formatting marks: where a paragraph's flat text has a space, a no-break
//! space, a tab or a line break, and where the paragraph ends, for a flow to
//! draw over its text when View > Formatting marks is on.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::text::CCursor;
use eframe::egui::{
    Align2, Color32, Context, FontId, Galley, Id, Key, KeyboardShortcut, Modifiers, Pos2, Rect, Ui,
    pos2,
};
use egui_richedit::OffsetMap;

use crate::i18n::t;

/// Whether the marks are shown in this window. They last as long as the
/// window and are not stored.
pub fn shown(ctx: &Context) -> bool {
    ctx.data(|data| data.get_temp(id()).unwrap_or(false))
}

/// View > Formatting marks, and Ctrl+F10, which is where `LibreOffice` keeps
/// them.
pub fn menu_item(ui: &mut Ui) {
    let mut on = shown(ui.ctx());
    let label = format!(
        "{}  ({})",
        t("Formatting &marks"),
        ui.ctx().format_shortcut(&TOGGLE)
    );
    if crate::mnemonic::checkbox(ui, &mut on, &label).changed() {
        set(ui.ctx(), on);
    }
}

/// Take Ctrl+F10 when it is pressed.
pub fn keys(ctx: &Context) {
    if ctx.input_mut(|input| input.consume_shortcut(&TOGGLE)) {
        set(ctx, !shown(ctx));
    }
}

const TOGGLE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::F10);

fn id() -> Id {
    Id::unique("odox-formatting-marks")
}

fn set(ctx: &Context, on: bool) {
    ctx.data_mut(|data| data.insert_temp(id(), on));
}

/// Draw the marks of a paragraph over its laid out text, where the galley
/// put each character, so that nothing moves.
pub fn paint(ui: &Ui, galley: &Galley, map: &OffsetMap, origin: Pos2, flat: &str, ink: Color32) {
    let ink = ink.gamma_multiply(0.45);
    let painter = ui.painter();
    for (at, symbol, align, size) in placed(galley, map, flat) {
        painter.text(
            origin + at.to_vec2(),
            align,
            symbol,
            FontId::proportional(size),
            ink,
        );
    }
}

/// Where each mark goes in the galley, what it is drawn as, which side of it
/// the point is, and how large. The paragraph's end is after the last thing
/// shown, a note's citation or a field's value included.
fn placed(galley: &Galley, map: &OffsetMap, flat: &str) -> Vec<(Pos2, &'static str, Align2, f32)> {
    let place = |offset: usize| {
        galley.pos_from_cursor(CCursor::new(map.to_galley(offset.min(map.model_len()))))
    };
    of(flat)
        .into_iter()
        .map(|(at, mark)| {
            let here = match mark {
                Mark::ParagraphEnd => galley.pos_from_cursor(CCursor::new(galley.len())),
                _ => place(at),
            };
            let size = here.height() * 0.7;
            let span = |here: Rect| {
                let next = place(at + 1);
                let end = if (next.center().y - here.center().y).abs() < 1.0 {
                    next.left()
                } else {
                    here.left() + here.height() * 0.3
                };
                pos2(f32::midpoint(here.left(), end), here.center().y)
            };
            let (point, symbol, align) = match mark {
                Mark::Space => (span(here), "\u{b7}", Align2::CENTER_CENTER),
                Mark::NoBreakSpace => (span(here), "\u{b0}", Align2::CENTER_CENTER),
                Mark::Tab => (span(here), "\u{2192}", Align2::CENTER_CENTER),
                Mark::LineBreak => (here.left_center(), "\u{21b5}", Align2::LEFT_CENTER),
                Mark::ParagraphEnd => (here.left_center(), "\u{b6}", Align2::LEFT_CENTER),
            };
            (point, symbol, align, size)
        })
        .collect()
}

/// What a mark stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// A space, drawn as a raised dot.
    Space,
    /// A space a line may not break at, drawn as a degree sign.
    NoBreakSpace,
    /// A tab, drawn as an arrow across its span.
    Tab,
    /// A line broken by hand, drawn as a return arrow.
    LineBreak,
    /// The paragraph's end, drawn as a pilcrow.
    ParagraphEnd,
}

/// The marks in a paragraph's flat text, each at the character index it
/// stands at; the paragraph's end is at the index past the last character.
pub fn of(flat: &str) -> Vec<(usize, Mark)> {
    let mut marks: Vec<(usize, Mark)> = flat
        .chars()
        .enumerate()
        .filter_map(|(at, c)| {
            let mark = match c {
                ' ' => Mark::Space,
                '\u{a0}' => Mark::NoBreakSpace,
                '\t' => Mark::Tab,
                '\n' => Mark::LineBreak,
                _ => return None,
            };
            Some((at, mark))
        })
        .collect();
    marks.push((flat.chars().count(), Mark::ParagraphEnd));
    marks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_for_space_tab_nbsp_and_break() {
        assert_eq!(
            of("a b\tc\u{a0}d\ne"),
            vec![
                (1, Mark::Space),
                (3, Mark::Tab),
                (5, Mark::NoBreakSpace),
                (7, Mark::LineBreak),
                (9, Mark::ParagraphEnd),
            ]
        );
    }

    #[test]
    fn paragraph_end_mark_after_last_char() {
        assert_eq!(of("ünï"), vec![(3, Mark::ParagraphEnd)]);
        assert_eq!(of(""), vec![(0, Mark::ParagraphEnd)]);
    }

    #[test]
    fn the_paragraph_end_follows_a_citation_that_ends_it() {
        use eframe::egui::text::{LayoutJob, TextFormat};
        let ctx = Context::default();
        let mut job = egui_richedit::ParagraphJob::new(LayoutJob::default());
        job.text("End.", TextFormat::default());
        job.atom("12", 0, TextFormat::default());
        let (job, map) = job.into_parts();
        let mut marks = Vec::new();
        let mut output = ctx.run_ui(eframe::egui::RawInput::default(), |ui| {
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job.clone()));
            let end = galley.pos_from_cursor(CCursor::new(galley.len())).left();
            marks = placed(&galley, &map, "End.")
                .into_iter()
                .map(|(at, _, _, _)| at.x - end)
                .collect();
        });
        output.textures_delta.clear();
        assert_eq!(marks.len(), 1);
        assert!(marks[0].abs() < 0.01, "{marks:?}");
    }
}
