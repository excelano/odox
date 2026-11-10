//! How a list numbers its items and writes the label in front of each, for the
//! window and the PDF alike. DESIGN.md §12.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use crate::xml::{Element, Ns};

use super::number_text;

const BULLET: &str = "\u{2022}";

/// Where a list's counters stand, one per level, as its items are read in
/// document order.
#[derive(Debug, Default)]
pub struct ListCounters(Vec<usize>);

impl ListCounters {
    /// Count a list item at a level and answer its number. An item's own
    /// `text:start-value` restarts the level there; a level not counted yet
    /// begins at its level style's start value, which is one unless the
    /// document says otherwise. Counting at a level resets every level below
    /// it, which is what makes 2.1 follow 1.3.
    pub fn count(&mut self, item: &Element, level: usize, level_style: Option<&Element>) -> usize {
        while self.0.len() <= level {
            self.0.push(0);
        }
        let start = || {
            level_style
                .and_then(|style| style.attr_usize(&Ns::Text, "start-value"))
                .unwrap_or(1)
        };
        self.0[level] = match item.attr_usize(&Ns::Text, "start-value") {
            Some(restart) => restart,
            None if self.0[level] == 0 => start(),
            None => self.0[level] + 1,
        };
        self.0.truncate(level + 1);
        self.0[level]
    }

    /// The label in front of the item counted last at a level, given each
    /// level's style.
    ///
    /// A numbered level writes its own number in its own format between its
    /// prefix and suffix, after the numbers of as many levels above it as
    /// `text:display-levels` asks, each in its own level's format, as 1.2.3 or
    /// ii.A. A level without a style, or one drawn with an image this does
    /// not fetch, is a bullet.
    pub fn label(&self, level: usize, level_style: impl Fn(usize) -> Option<Element>) -> String {
        let Some(style) = level_style(level) else {
            return BULLET.to_owned();
        };
        if style.is(&Ns::Text, "list-level-style-bullet") {
            return style
                .attr(&Ns::Text, "bullet-char")
                .unwrap_or(BULLET)
                .to_owned();
        }
        if !style.is(&Ns::Text, "list-level-style-number") {
            return BULLET.to_owned();
        }
        let format_of = |style: Option<&Element>| {
            style
                .and_then(|style| style.attr(&Ns::Style, "num-format"))
                .unwrap_or("1")
                .to_owned()
        };
        let display = style
            .attr_usize(&Ns::Text, "display-levels")
            .unwrap_or(1)
            .max(1);
        let numbers: Vec<String> = ((level + 1).saturating_sub(display)..level)
            .map(|above| number_text(self.at(above), &format_of(level_style(above).as_ref())))
            .chain(std::iter::once(number_text(
                self.at(level),
                &format_of(Some(&style)),
            )))
            .collect();
        let prefix = style.attr(&Ns::Style, "num-prefix").unwrap_or_default();
        let suffix = style.attr(&Ns::Style, "num-suffix").unwrap_or_default();
        format!("{prefix}{}{suffix}", numbers.join("."))
    }

    fn at(&self, level: usize) -> usize {
        self.0.get(level).copied().unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml;

    fn element(source: &str) -> Element {
        let source = format!(
            r#"<x xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0">{source}</x>"#
        );
        let root = xml::parse(source.as_bytes(), "test").expect("parses");
        root.elements().next().expect("an element").clone()
    }

    fn levels() -> Vec<Element> {
        vec![
            element(
                r#"<text:list-level-style-number text:level="1" style:num-format="i" style:num-suffix="."/>"#,
            ),
            element(
                r#"<text:list-level-style-number text:level="2" style:num-format="A" style:num-suffix="." text:display-levels="2"/>"#,
            ),
        ]
    }

    #[test]
    fn a_level_shown_with_those_above_writes_each_in_its_own_format() {
        let levels = levels();
        let style = |level: usize| levels.get(level).cloned();
        let item = element("<text:list-item/>");
        let mut counters = ListCounters::default();
        let mut labels = Vec::new();
        for level in [0, 0, 1, 1] {
            counters.count(&item, level, style(level).as_ref());
            labels.push(counters.label(level, style));
        }
        assert_eq!(labels, ["i.", "ii.", "ii.A.", "ii.B."]);
    }

    #[test]
    fn an_item_s_own_start_value_restarts_its_level() {
        let levels = levels();
        let style = |level: usize| levels.get(level).cloned();
        let mut counters = ListCounters::default();
        let plain = element("<text:list-item/>");
        let restart = element(r#"<text:list-item text:start-value="5"/>"#);
        let numbers: Vec<usize> = [&plain, &restart, &plain]
            .into_iter()
            .map(|item| counters.count(item, 0, style(0).as_ref()))
            .collect();
        assert_eq!(numbers, [1, 5, 6]);
    }
}
