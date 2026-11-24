//! Words in a text, by the Unicode word boundaries (UAX #29) the editor's caret
//! moves by, and how many there are.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// Where the words of a text are, as ranges of characters. A word is a stretch
/// between two Unicode word boundaries holding at least one letter or digit, so
/// spaces and punctuation between words are none.
pub fn word_ranges(text: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut at = 0;
    for segment in text.split_word_bounds() {
        let length = segment.chars().count();
        if segment.chars().any(char::is_alphanumeric) {
            found.push(at..at + length);
        }
        at += length;
    }
    found
}

/// How much text there is: words, and characters with the spaces among them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Count {
    /// Words, as [`word_ranges`] finds them.
    pub words: usize,
    /// Characters, spaces included.
    pub characters: usize,
}

/// How many words and characters a text has.
pub fn count(text: &str) -> Count {
    Count {
        words: word_ranges(text).len(),
        characters: text.chars().filter(|&c| c != crate::edit::FIELD).count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_counted_in_characters_and_skip_what_is_between_them() {
        assert_eq!(word_ranges("Grüße, ihr 2!"), vec![0..5, 7..10, 11..12]);
    }

    #[test]
    fn an_apostrophe_and_a_decimal_point_stay_inside_their_word() {
        assert_eq!(word_ranges("can't 3.5"), vec![0..5, 6..9]);
    }

    #[test]
    fn a_field_counts_as_neither_word_nor_character() {
        assert_eq!(
            count("page \u{fffc} of"),
            Count {
                words: 2,
                characters: 8
            }
        );
    }

    #[test]
    fn count_of_plain_sentence() {
        assert_eq!(
            count("The quick brown fox, jumping."),
            Count {
                words: 5,
                characters: 29
            }
        );
    }

    #[test]
    fn a_hyphenated_word_is_two_and_a_lone_symbol_is_none() {
        assert_eq!(count("right-aligned & red").words, 3);
    }
}
