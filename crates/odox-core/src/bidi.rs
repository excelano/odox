//! The order a line of text is drawn in where it mixes a script that reads
//! right to left with one that reads left to right: the Unicode bidirectional
//! algorithm, which a renderer applies to each line once it knows where its
//! lines break.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::ops::Range;

use unicode_bidi::{BidiClass, Level, ParagraphBidiInfo, bidi_class};

/// A stretch of a line drawn in one direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// Its bytes in the paragraph's text.
    pub range: Range<usize>,
    /// Whether it reads right to left, its last character drawn first.
    pub right_to_left: bool,
}

/// A paragraph's text with the direction of each of its characters worked
/// out, from which each of its lines is put in order.
pub struct Paragraph<'text> {
    info: ParagraphBidiInfo<'text>,
}

impl<'text> Paragraph<'text> {
    /// The text of one paragraph, in a paragraph that reads right to left as
    /// a whole or left to right. Its direction is the paragraph's, as
    /// `style:writing-mode` says, and not guessed from its first letter.
    pub fn new(text: &'text str, right_to_left: bool) -> Self {
        let level = if right_to_left {
            Level::rtl()
        } else {
            Level::ltr()
        };
        Self {
            info: ParagraphBidiInfo::new(text, Some(level)),
        }
    }

    /// Whether the character at a byte of the paragraph's text reads right to
    /// left where it stands, which is the direction it is shaped in.
    pub fn reads_right_to_left(&self, at: usize) -> bool {
        self.info.levels.get(at).is_some_and(Level::is_rtl)
    }

    /// The runs a line is drawn as, from left to right: `line` is the line's
    /// bytes in the paragraph's text.
    pub fn line(&self, line: Range<usize>) -> Vec<Run> {
        let (levels, runs) = self.info.visual_runs(line);
        runs.into_iter()
            .map(|range| Run {
                right_to_left: levels.get(range.start).is_some_and(Level::is_rtl),
                range,
            })
            .collect()
    }
}

/// Whether a paragraph needs putting in order to be drawn: it reads right to
/// left, or holds a character that does.
pub fn needed(text: &str, right_to_left: bool) -> bool {
    right_to_left || text.chars().any(is_right_to_left)
}

/// Whether a character reads right to left wherever it stands: a Hebrew or an
/// Arabic letter, an Arabic digit, or a mark that turns what follows.
pub fn is_right_to_left(character: char) -> bool {
    matches!(
        bidi_class(character),
        BidiClass::R
            | BidiClass::AL
            | BidiClass::AN
            | BidiClass::RLE
            | BidiClass::RLO
            | BidiClass::RLI
    )
}

/// Whether a character is a mark that combines with the one before it, and
/// so belongs to the same word whichever way the word reads.
pub fn is_mark(character: char) -> bool {
    bidi_class(character) == BidiClass::NSM
}

/// The character drawn for one in a stretch read right to left: an opening
/// bracket becomes a closing one and the reverse, so that a bracket still
/// faces what it holds.
pub fn mirrored(character: char) -> char {
    match character {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        '‹' => '›',
        '›' => '‹',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawn(text: &str, right_to_left: bool) -> Vec<(&str, bool)> {
        Paragraph::new(text, right_to_left)
            .line(0..text.len())
            .into_iter()
            .map(|run| (&text[run.range], run.right_to_left))
            .collect()
    }

    #[test]
    fn hebrew_in_an_english_line_is_one_run_read_right_to_left() {
        assert_eq!(
            drawn("English then עברית אחת then English", false),
            [
                ("English then ", false),
                ("עברית אחת", true),
                (" then English", false)
            ]
        );
    }

    #[test]
    fn a_right_to_left_paragraph_draws_its_first_run_on_the_right() {
        assert_eq!(
            drawn("שלום abc def 2024", true),
            [("abc def 2024", false), ("שלום ", true)]
        );
    }

    #[test]
    fn a_paragraph_reads_the_way_it_says_and_not_the_way_its_first_letter_does() {
        assert_eq!(drawn("1. שלום", false), [("1. ", false), ("שלום", true)]);
        assert!(!needed("plain English", false));
        assert!(needed("plain English", true));
        assert!(needed("a שלום", false));
    }
}
