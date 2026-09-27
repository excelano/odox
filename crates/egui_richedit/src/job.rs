//! A paragraph's layout job, and the map between what it draws and what the
//! model holds.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::ops::Range;

use egui::TextFormat;
use egui::text::LayoutJob;

/// A paragraph's text as it is drawn, built piece by piece so that every
/// drawn character is known to stand for some part of the model's text.
///
/// Start from a [`LayoutJob`] with its wrapping and alignment set and no text,
/// append the paragraph's runs in order, then take the job and its
/// [`OffsetMap`] apart with [`Self::into_parts`].
#[derive(Clone, Debug, Default)]
pub struct ParagraphJob {
    job: LayoutJob,
    map: OffsetMap,
}

impl ParagraphJob {
    /// A paragraph drawn with a job's wrapping and alignment. Any text the job
    /// already holds is kept and counted as the model's own text.
    pub fn new(job: LayoutJob) -> Self {
        let mut map = OffsetMap::default();
        let len = job.text.chars().count();
        if len > 0 {
            map.push(len, len, false);
        }
        Self { job, map }
    }

    /// Text drawn as the model holds it: one drawn character for each of the
    /// model's.
    pub fn text(&mut self, text: &str, format: TextFormat) {
        let len = text.chars().count();
        if len == 0 {
            return;
        }
        self.job.append(text, 0.0, format);
        self.map.push(len, len, false);
    }

    /// Something drawn differently from what the model holds: a tab the model
    /// counts as one character drawn as four spaces is `atom("    ", 1, ..)`,
    /// and a citation the model does not count is `atom("1", 0, ..)`. The
    /// caret stands on either side of an atom and never inside it.
    pub fn atom(&mut self, shown: &str, model_len: usize, format: TextFormat) {
        let shown_len = shown.chars().count();
        if shown_len == 0 && model_len == 0 {
            return;
        }
        if shown_len > 0 {
            self.job.append(shown, 0.0, format);
        }
        self.map.push(shown_len, model_len, true);
    }

    /// Whether nothing has been drawn yet.
    pub fn is_empty(&self) -> bool {
        self.job.text.is_empty()
    }

    /// The formats of the runs appended so far, to adjust once they are all
    /// known, as a line height set per paragraph is. The text is not
    /// reachable from here, so the map stays true.
    pub fn formats_mut(&mut self) -> impl Iterator<Item = &mut TextFormat> {
        self.job
            .sections
            .iter_mut()
            .map(|section| &mut section.format)
    }

    /// The layout job to hand to egui, and the map to hand to the editor with
    /// the galley egui makes of it.
    pub fn into_parts(self) -> (LayoutJob, OffsetMap) {
        (self.job, self.map)
    }
}

/// Which drawn characters of a paragraph stand for which of the model's.
///
/// Built by [`ParagraphJob`]; the editor reads it to turn a point in a galley
/// into a model offset and back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OffsetMap {
    pieces: Vec<Piece>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Piece {
    /// Character range in the galley.
    shown: Range<usize>,
    /// Character range in the model's text.
    model: Range<usize>,
    /// Whether the caret steps over the piece whole.
    atom: bool,
}

impl OffsetMap {
    fn push(&mut self, shown_len: usize, model_len: usize, atom: bool) {
        let (shown, model) = self
            .pieces
            .last()
            .map_or((0, 0), |last| (last.shown.end, last.model.end));
        // Consecutive plain text is one piece, so a paragraph of many runs and
        // no atoms maps as one.
        if !atom
            && let Some(last) = self.pieces.last_mut()
            && !last.atom
        {
            last.shown.end += shown_len;
            last.model.end += model_len;
            return;
        }
        self.pieces.push(Piece {
            shown: shown..shown + shown_len,
            model: model..model + model_len,
            atom,
        });
    }

    /// How many characters the model holds for the paragraph.
    pub fn model_len(&self) -> usize {
        self.pieces.last().map_or(0, |last| last.model.end)
    }

    /// The galley character a model offset is drawn at. At an atom the model
    /// does not count, the offset is drawn before it.
    pub fn to_galley(&self, offset: usize) -> usize {
        for piece in &self.pieces {
            if piece.atom {
                if offset == piece.model.start {
                    return piece.shown.start;
                }
                if offset < piece.model.end {
                    // Inside an atom, which the editor never stands in; the
                    // nearest place it can is after it.
                    return piece.shown.end;
                }
            } else if offset <= piece.model.end {
                return piece.shown.start + offset.saturating_sub(piece.model.start);
            }
        }
        self.pieces.last().map_or(0, |last| last.shown.end)
    }

    /// The model offset a galley character stands at. Inside an atom, the
    /// nearer of its two ends.
    pub fn to_model(&self, index: usize) -> usize {
        for piece in &self.pieces {
            if index < piece.shown.end {
                let into = index.saturating_sub(piece.shown.start);
                return if !piece.atom {
                    piece.model.start + into
                } else if into * 2 <= piece.shown.len() {
                    piece.model.start
                } else {
                    piece.model.end
                };
            }
        }
        self.model_len()
    }
}

#[cfg(test)]
mod tests {
    use super::{OffsetMap, ParagraphJob};
    use egui::TextFormat;

    fn map(build: impl FnOnce(&mut ParagraphJob)) -> OffsetMap {
        let mut job = ParagraphJob::default();
        build(&mut job);
        job.into_parts().1
    }

    #[test]
    fn plain_text_maps_one_to_one() {
        let map = map(|job| {
            job.text("Hello, ", TextFormat::default());
            job.text("world", TextFormat::default());
        });
        assert_eq!(map.model_len(), 12);
        for i in 0..=12 {
            assert_eq!(map.to_galley(i), i);
            assert_eq!(map.to_model(i), i);
        }
    }

    #[test]
    fn a_tab_drawn_as_spaces_is_one_character_stepped_over_whole() {
        // "a\tb" drawn "a    b".
        let map = map(|job| {
            job.text("a", TextFormat::default());
            job.atom("    ", 1, TextFormat::default());
            job.text("b", TextFormat::default());
        });
        assert_eq!(map.model_len(), 3);
        assert_eq!(map.to_galley(0), 0);
        assert_eq!(map.to_galley(1), 1, "before the tab");
        assert_eq!(map.to_galley(2), 5, "after the tab");
        assert_eq!(map.to_galley(3), 6);
        assert_eq!(map.to_model(2), 1, "a click early in the tab is before it");
        assert_eq!(map.to_model(4), 2, "a click late in the tab is after it");
        assert_eq!(map.to_model(5), 2);
        assert_eq!(map.to_model(6), 3);
    }

    #[test]
    fn an_uncounted_citation_takes_no_offsets() {
        // "ab" with a citation "12" drawn between the two.
        let map = map(|job| {
            job.text("a", TextFormat::default());
            job.atom("12", 0, TextFormat::default());
            job.text("b", TextFormat::default());
        });
        assert_eq!(map.model_len(), 2);
        assert_eq!(map.to_galley(1), 1, "the offset between is drawn before it");
        assert_eq!(map.to_galley(2), 4);
        assert_eq!(map.to_model(1), 1);
        assert_eq!(map.to_model(2), 1);
        assert_eq!(map.to_model(3), 1);
        assert_eq!(map.to_model(4), 2);
    }

    #[test]
    fn an_empty_paragraph_drawn_as_a_space_holds_nothing() {
        let map = map(|job| job.atom(" ", 0, TextFormat::default()));
        assert_eq!(map.model_len(), 0);
        assert_eq!(map.to_galley(0), 0);
        assert_eq!(map.to_model(0), 0);
        assert_eq!(map.to_model(1), 0);
    }

    #[test]
    fn text_already_in_the_job_is_the_model_s() {
        let mut layout = egui::text::LayoutJob::default();
        layout.append("abc", 0.0, TextFormat::default());
        let mut job = ParagraphJob::new(layout);
        job.text("de", TextFormat::default());
        let (_, map) = job.into_parts();
        assert_eq!(map.model_len(), 5);
        assert_eq!(map.to_galley(4), 4);
    }
}
