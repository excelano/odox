//! Words in a text, by the Unicode word boundaries (UAX #29) the editor's caret
//! moves by.
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
}
