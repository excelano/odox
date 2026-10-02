//! A paragraph's characters as glyphs, and where its lines break.
//!
//! Each run is shaped in the face its style resolves to, and a character
//! that face lacks is shaped in the first face that has it. Text is shaped
//! left to right whatever its script, as the window draws it (DESIGN.md §6).
//! Lines break where the Unicode line breaking algorithm allows, at the
//! width the page gives; a word wider than the line is cut where it must be.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::ops::Range;

use unicode_linebreak::{BreakOpportunity, linebreaks};

use crate::Refusal;
use crate::fonts::Faces;
use crate::model::Paragraph;

/// One glyph of a paragraph, in reading order.
pub(crate) struct Glyph {
    pub id: u16,
    /// The byte in the paragraph's text the glyph's cluster begins at.
    pub cluster: usize,
    /// Measured in points at the run's size.
    pub advance: f32,
    pub dx: f32,
    pub dy: f32,
    /// The run it belongs to and the face it is drawn in.
    pub run: usize,
    pub face: usize,
}

/// Shape a paragraph's text.
///
/// # Errors
///
/// A face cannot be had for a run, or a character is in no face on the
/// machine.
pub(crate) fn shape(paragraph: &Paragraph, faces: &mut Faces) -> Result<Vec<Glyph>, Refusal> {
    let mut glyphs = Vec::new();
    for (index, run) in paragraph.runs.iter().enumerate() {
        let primary = faces.request(&run.style.family, run.style.variant)?;
        // Split the run where the face that has its characters changes. A
        // character the previous one's face also has stays with it, so that a
        // combining mark is shaped with the letter it sits on.
        let mut segments: Vec<(Range<usize>, usize)> = Vec::new();
        let text = &paragraph.text[run.range.clone()];
        for (offset, character) in text.char_indices() {
            let at = run.range.start + offset;
            let end = at + character.len_utf8();
            if character == '\n' {
                continue;
            }
            let face = match segments.last() {
                Some((range, face)) if range.end == at && faces.covers(*face, character) => *face,
                _ if faces.covers(primary, character) => primary,
                _ => faces
                    .fallback(character, run.style.variant)
                    .ok_or(Refusal::Glyph(character))?,
            };
            match segments.last_mut() {
                Some((range, last)) if *last == face && range.end == at => range.end = end,
                _ => segments.push((at..end, face)),
            }
        }
        for (range, face) in segments {
            shape_segment(paragraph, range, face, index, faces, &mut glyphs)?;
        }
    }
    Ok(glyphs)
}

fn shape_segment(
    paragraph: &Paragraph,
    range: Range<usize>,
    face: usize,
    run: usize,
    faces: &Faces,
    into: &mut Vec<Glyph>,
) -> Result<(), Refusal> {
    let loaded = faces.get(face);
    let shaper = rustybuzz::Face::from_slice(&loaded.data, loaded.index)
        .ok_or_else(|| Refusal::Writer("a face could not be read for shaping".to_owned()))?;
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(&paragraph.text[range.clone()]);
    buffer.guess_segment_properties();
    buffer.set_direction(rustybuzz::Direction::LeftToRight);
    let output = rustybuzz::shape(&shaper, &[], buffer);
    let size = paragraph.runs[run].style.size;
    let scale = size / loaded.units_per_em;
    for (info, position) in output.glyph_infos().iter().zip(output.glyph_positions()) {
        let cluster = range.start + info.cluster as usize;
        let id = u16::try_from(info.glyph_id).unwrap_or(0);
        if id == 0 {
            let character = paragraph.text[cluster..]
                .chars()
                .next()
                .unwrap_or('\u{FFFD}');
            return Err(Refusal::Glyph(character));
        }
        #[allow(clippy::cast_precision_loss)]
        into.push(Glyph {
            id,
            cluster,
            advance: position.x_advance as f32 * scale,
            dx: position.x_offset as f32 * scale,
            dy: position.y_offset as f32 * scale,
            run,
            face,
        });
    }
    Ok(())
}

/// Where a paragraph's lines break, as byte ranges of its text: the first
/// line `first` points wide and the rest `rest`.
pub(crate) fn lines(text: &str, glyphs: &[Glyph], first: f32, rest: f32) -> Vec<Range<usize>> {
    let breaks: Vec<(usize, bool)> = linebreaks(text)
        .map(|(at, kind)| (at, kind == BreakOpportunity::Mandatory))
        .collect();
    let mut lines = Vec::new();
    let mut start = 0;
    loop {
        let available = if lines.is_empty() { first } else { rest };
        let mut chosen = None;
        for &(at, mandatory) in breaks.iter().filter(|(at, _)| *at > start) {
            if width(text, glyphs, start..at) <= available {
                chosen = Some(at);
                if mandatory {
                    break;
                }
            } else {
                break;
            }
        }
        let end = chosen.unwrap_or_else(|| cut(text, glyphs, start, &breaks, available));
        lines.push(start..end);
        start = end;
        if start >= text.len() {
            break;
        }
    }
    lines
}

/// Where a line with no break that fits is cut: after as many whole clusters
/// as fit, and at least one.
fn cut(
    text: &str,
    glyphs: &[Glyph],
    start: usize,
    breaks: &[(usize, bool)],
    available: f32,
) -> usize {
    let next = breaks
        .iter()
        .map(|(at, _)| *at)
        .find(|at| *at > start)
        .unwrap_or(text.len());
    let mut total = 0.0;
    for glyph in glyphs
        .iter()
        .filter(|g| g.cluster >= start && g.cluster < next)
    {
        if glyph.cluster > start && total + glyph.advance > available {
            return glyph.cluster;
        }
        total += glyph.advance;
    }
    next
}

/// The width of a range of the text, leaving out the spaces and the line
/// break it ends with.
pub(crate) fn width(text: &str, glyphs: &[Glyph], range: Range<usize>) -> f32 {
    let end = trimmed(text, range.clone());
    glyphs
        .iter()
        .filter(|g| g.cluster >= range.start && g.cluster < end)
        .map(|g| g.advance)
        .sum()
}

/// Where a range of the text ends once the whitespace at its end is left
/// out.
pub(crate) fn trimmed(text: &str, range: Range<usize>) -> usize {
    range.start + text[range].trim_end().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character a glyph ten points wide.
    fn glyphs(text: &str) -> Vec<Glyph> {
        text.char_indices()
            .filter(|(_, c)| *c != '\n')
            .map(|(cluster, _)| Glyph {
                id: 1,
                cluster,
                advance: 10.0,
                dx: 0.0,
                dy: 0.0,
                run: 0,
                face: 0,
            })
            .collect()
    }

    fn broken(text: &str, first: f32, rest: f32) -> Vec<&str> {
        lines(text, &glyphs(text), first, rest)
            .into_iter()
            .map(|range| &text[range])
            .collect()
    }

    #[test]
    fn lines_break_between_words_and_the_trailing_space_does_not_count() {
        assert_eq!(broken("aaa bbb ccc", 70.0, 70.0), ["aaa bbb ", "ccc"]);
    }

    #[test]
    fn the_first_line_has_its_own_width() {
        assert_eq!(broken("aaa bbb ccc", 30.0, 70.0), ["aaa ", "bbb ccc"]);
    }

    #[test]
    fn a_line_break_in_the_text_ends_the_line() {
        assert_eq!(broken("aa\nbb", 100.0, 100.0), ["aa\n", "bb"]);
    }

    #[test]
    fn a_word_wider_than_the_line_is_cut_where_it_has_to_be() {
        assert_eq!(broken("abcdefgh", 30.0, 30.0), ["abc", "def", "gh"]);
    }

    #[test]
    fn an_empty_paragraph_is_one_empty_line() {
        assert_eq!(broken("", 30.0, 30.0), [""]);
    }
}
