//! Searching a document's text: where the matches are, which one is current,
//! and what a flow needs to draw them.
//!
//! A match is a range of characters in one paragraph's flat text, the string
//! [`odox_core::edit::text`] answers and the page editor's offsets count in, so
//! the same range lights up the same characters whether the paragraph is being
//! read or edited. What a match is found in is the view's business; this module
//! knows only how to look in a string and how to keep the results. Case is not
//! significant, and the query is not a pattern. DESIGN.md §11.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::ops::Range;

use odox_core::Element;
use odox_core::edit;

use crate::flow_model::paragraph_paths;

/// One place a query was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    /// Which part of the document it is in, where a document has parts a view
    /// shows one at a time: a slide, a sheet. Zero where it has none.
    pub scope: usize,
    /// The paragraph's path under the root the view draws, as a flow reports
    /// it, or a cell's row and column in a sheet.
    pub paragraph: Vec<usize>,
    /// The characters matched, counted in the paragraph's flat text.
    pub range: Range<usize>,
}

/// A character with its case taken away, one for one, so that a range found in
/// the folded text is the same range in the original.
fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn folded(text: &str) -> Vec<char> {
    text.chars().map(fold).collect()
}

/// Where a query occurs in a text, as character ranges that do not overlap.
/// An empty query occurs nowhere.
pub fn ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    let needle = folded(query);
    if needle.is_empty() {
        return Vec::new();
    }
    let hay = folded(text);
    let mut found = Vec::new();
    let mut at = 0;
    while at + needle.len() <= hay.len() {
        if hay[at..at + needle.len()] == needle[..] {
            found.push(at..at + needle.len());
            at += needle.len();
        } else {
            at += 1;
        }
    }
    found
}

/// Every match of a query in the paragraphs under a root, in the order the
/// flow draws them, tagged with a scope.
pub fn in_paragraphs(root: &Element, scope: usize, query: &str) -> Vec<Match> {
    let mut found = Vec::new();
    if query.is_empty() {
        return found;
    }
    for path in paragraph_paths(root) {
        let Some(paragraph) = root.at(&path) else {
            continue;
        };
        for range in ranges(&edit::text(paragraph), query) {
            found.push(Match {
                scope,
                paragraph: path.clone(),
                range,
            });
        }
    }
    found
}

/// What a view keeps of a search: the matches, which one is current, and
/// whether the view still owes a scroll to it.
#[derive(Default)]
pub struct Found {
    matches: Vec<Match>,
    current: Option<usize>,
    reveal: bool,
}

impl Found {
    /// Take new results. The current match stays where it was, as far as there
    /// are as many, so that typing in a document being searched does not send
    /// the view back to the first match.
    pub fn set(&mut self, matches: Vec<Match>) {
        self.matches = matches;
        self.current = self
            .current
            .filter(|&at| at < self.matches.len())
            .or_else(|| (!self.matches.is_empty()).then_some(0));
    }

    /// Forget the search.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// How many matches there are.
    pub fn len(&self) -> usize {
        self.matches.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    /// Which match is current, counted from zero.
    pub fn current(&self) -> Option<usize> {
        self.current
    }

    /// Make a match the current one and scroll to it when it is drawn.
    pub fn show(&mut self, index: usize) -> Option<&Match> {
        let found = self.matches.get(index)?;
        self.current = Some(index);
        self.reveal = true;
        Some(found)
    }

    /// The current match.
    pub fn current_match(&self) -> Option<&Match> {
        self.matches.get(self.current?)
    }

    /// What a flow draws, for one scope. A scroll to the current match is
    /// owed from [`Self::show`] until the view says it has drawn the frame it
    /// was owed in, with [`Self::drawn`].
    pub fn highlights(&self, scope: usize) -> Option<Highlights<'_>> {
        if self.matches.is_empty() {
            return None;
        }
        Some(Highlights {
            matches: &self.matches,
            current: self.current,
            reveal: self.reveal,
            scope,
        })
    }

    /// The frame the highlights were drawn in is done, and any scroll they
    /// asked for has been made.
    pub fn drawn(&mut self) {
        self.reveal = false;
    }
}

/// The matches a flow draws behind its text.
#[derive(Clone)]
pub struct Highlights<'a> {
    matches: &'a [Match],
    current: Option<usize>,
    reveal: bool,
    scope: usize,
}

impl Highlights<'_> {
    /// The matches in one paragraph, with whether each is the current one.
    pub fn within(&self, paragraph: &[usize]) -> Vec<(Range<usize>, bool)> {
        let first = self
            .matches
            .partition_point(|m| (m.scope, m.paragraph.as_slice()) < (self.scope, paragraph));
        self.matches[first..]
            .iter()
            .enumerate()
            .take_while(|(_, m)| m.scope == self.scope && m.paragraph == paragraph)
            .map(|(offset, m)| (m.range.clone(), self.current == Some(first + offset)))
            .collect()
    }

    /// Whether the view is owed a scroll to the current match.
    pub fn reveal(&self) -> bool {
        self.reveal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scan_finds_paragraphs_by_the_paths_a_flow_reports() {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/libreoffice/text.odt"),
        )
        .expect("the corpus document");
        let document = odox_core::doc::TextDocument::read(&bytes).expect("it reads");
        let body = document.body().expect("it has a body");
        let found = in_paragraphs(body, 0, "AND");
        assert_eq!(found.len(), 6);
        for hit in &found {
            let paragraph = body.at(&hit.paragraph).expect("a path to a paragraph");
            let text: Vec<char> = edit::text(paragraph).chars().collect();
            let matched: String = text[hit.range.clone()].iter().collect();
            assert_eq!(matched.to_lowercase(), "and");
        }
    }

    #[test]
    fn a_query_is_found_whatever_its_case() {
        assert_eq!(ranges("Odox and ODOX", "odox"), vec![0..4, 9..13]);
    }

    #[test]
    fn matches_do_not_overlap() {
        assert_eq!(ranges("aaaa", "aa"), vec![0..2, 2..4]);
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        assert!(ranges("anything", "").is_empty());
    }

    #[test]
    fn ranges_count_characters_and_not_bytes() {
        assert_eq!(ranges("ünï ünï", "ünï"), vec![0..3, 4..7]);
    }

    fn found_in(paragraph: &[usize], range: Range<usize>) -> Match {
        Match {
            scope: 0,
            paragraph: paragraph.to_vec(),
            range,
        }
    }

    #[test]
    fn a_paragraphs_highlights_are_the_matches_with_its_path() {
        let mut found = Found::default();
        found.set(vec![
            found_in(&[0], 1..2),
            found_in(&[1], 0..3),
            found_in(&[1], 5..8),
            found_in(&[2, 0], 0..1),
        ]);
        found.show(2);
        let highlights = found.highlights(0).expect("there are matches");
        assert_eq!(highlights.within(&[1]), vec![(0..3, false), (5..8, true)]);
        assert!(highlights.within(&[3]).is_empty());
    }

    #[test]
    fn the_current_match_survives_new_results_that_still_reach_it() {
        let mut found = Found::default();
        found.set(vec![found_in(&[0], 0..1), found_in(&[1], 0..1)]);
        found.show(1);
        found.set(vec![found_in(&[0], 0..1), found_in(&[1], 0..2)]);
        assert_eq!(found.current(), Some(1));
        found.set(vec![found_in(&[0], 0..1)]);
        assert_eq!(found.current(), Some(0));
    }

    #[test]
    fn a_scroll_is_owed_until_the_frame_is_drawn() {
        let mut found = Found::default();
        found.set(vec![found_in(&[0], 0..1)]);
        found.show(0);
        assert!(found.highlights(0).is_some_and(|h| h.reveal()));
        assert!(found.highlights(0).is_some_and(|h| h.reveal()));
        found.drawn();
        assert!(found.highlights(0).is_some_and(|h| !h.reveal()));
    }
}
