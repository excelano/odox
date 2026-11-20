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
    /// bytes in the paragraph's text. An empty line is drawn as no runs.
    pub fn line(&self, line: Range<usize>) -> Vec<Run> {
        if line.is_empty() {
            return Vec::new();
        }
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

    #[test]
    fn a_number_in_a_right_to_left_line_still_reads_left_to_right() {
        assert_eq!(
            drawn("מחיר 100 שקל", true),
            [(" שקל", true), ("100", false), ("מחיר ", true)]
        );
    }

    #[test]
    fn punctuation_after_hebrew_in_an_english_line_takes_the_lines_direction() {
        assert_eq!(
            drawn("Say שלום!", false),
            [("Say ", false), ("שלום", true), ("!", false)]
        );
    }

    #[test]
    fn a_later_line_is_put_in_order_by_its_bytes_in_the_paragraph() {
        let text = "first line then שלום עולם at the end";
        let start = text.find("then").unwrap();
        let runs = Paragraph::new(text, false).line(start..text.len());
        let drawn: Vec<(&str, bool)> = runs
            .iter()
            .map(|run| (&text[run.range.clone()], run.right_to_left))
            .collect();
        assert_eq!(
            drawn,
            [
                ("then ", false),
                ("שלום עולם", true),
                (" at the end", false)
            ]
        );
        assert_eq!(runs[0].range.start, start);
    }

    #[test]
    fn a_character_reads_the_way_the_algorithm_put_it_where_it_stands() {
        let text = "ab שלום cd";
        let paragraph = Paragraph::new(text, false);
        let hebrew = text.find('ש').unwrap();
        assert!(!paragraph.reads_right_to_left(0));
        assert!(paragraph.reads_right_to_left(hebrew));
        assert!(!paragraph.reads_right_to_left(text.len() - 1));
        assert!(!paragraph.reads_right_to_left(text.len()));
    }

    #[test]
    fn an_empty_line_is_drawn_as_nothing() {
        assert_eq!(Paragraph::new("", true).line(0..0), []);
        assert_eq!(Paragraph::new("שלום", true).line(2..2), []);
    }

    #[test]
    fn hebrew_and_arabic_letters_arabic_digits_and_turning_marks_read_right_to_left() {
        for character in ['ש', 'م', '٣', '\u{202B}', '\u{202E}', '\u{2067}'] {
            assert!(is_right_to_left(character), "{character:?}");
        }
        for character in ['a', '3', ' ', '!', '\u{202A}', '\u{05B8}'] {
            assert!(!is_right_to_left(character), "{character:?}");
        }
    }

    #[test]
    fn a_hebrew_point_or_an_arabic_vowel_mark_joins_the_letter_before_it() {
        assert!(is_mark('\u{05B8}'));
        assert!(is_mark('\u{064E}'));
        assert!(!is_mark('ש'));
        assert!(!is_mark('a'));
    }

    #[test]
    fn a_bracket_turned_round_faces_the_other_way_and_anything_else_is_kept() {
        for (open, close) in [
            ('(', ')'),
            ('[', ']'),
            ('{', '}'),
            ('<', '>'),
            ('«', '»'),
            ('‹', '›'),
        ] {
            assert_eq!(mirrored(open), close);
            assert_eq!(mirrored(close), open);
        }
        for character in ['a', 'ש', '!', '-', '"'] {
            assert_eq!(mirrored(character), character);
        }
    }
}
