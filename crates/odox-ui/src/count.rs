//! How many words and characters the paragraphs under a root hold, summed one
//! paragraph at a time so that no word runs across a paragraph's end.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::words::{Count, count};
use odox_core::{Element, edit};

use crate::flow_model::paragraph_paths;

/// The words and characters of every paragraph under a root.
pub fn in_paragraphs(root: &Element) -> Count {
    paragraph_paths(root)
        .iter()
        .filter_map(|path| root.at(path))
        .map(|paragraph| count(&edit::text(paragraph)))
        .fold(Count::default(), add)
}

/// The words and characters between two places in the paragraphs under a
/// root, each a paragraph's path and a character offset in its flat text, in
/// either order.
pub fn between(root: &Element, one: (&[usize], usize), other: (&[usize], usize)) -> Count {
    let paths = paragraph_paths(root);
    let place = |(path, offset): (&[usize], usize)| {
        paths
            .iter()
            .position(|p| p == path)
            .map(|index| (index, offset))
    };
    let (Some(one), Some(other)) = (place(one), place(other)) else {
        return Count::default();
    };
    let (start, end) = if one <= other {
        (one, other)
    } else {
        (other, one)
    };
    (start.0..=end.0)
        .filter_map(|index| {
            let text = edit::text(root.at(&paths[index])?);
            let from = if index == start.0 { start.1 } else { 0 };
            let to = if index == end.0 { end.1 } else { usize::MAX };
            let piece: String = text.chars().take(to).skip(from).collect();
            Some(count(&piece))
        })
        .fold(Count::default(), add)
}

fn add(total: Count, one: Count) -> Count {
    Count {
        words: total.words + one.words,
        characters: total.characters + one.characters,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use odox_core::{Ns, Package};

    fn read(name: &str) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/libreoffice")
                .join(name),
        )
        .expect("the corpus document")
    }

    #[test]
    fn corpus_counts_match_libreoffice_where_it_counts_by_the_same_rule() {
        for name in ["tabs.odt", "directions.odt"] {
            let bytes = read(name);
            let meta = Package::read(&bytes)
                .and_then(|p| p.xml("meta.xml"))
                .expect("its metadata");
            let statistic = meta
                .child(&Ns::Office, "meta")
                .and_then(|m| m.child(&Ns::Meta, "document-statistic"))
                .expect("LibreOffice's statistics");
            let document = odox_core::doc::TextDocument::read(&bytes).expect("it reads");
            let libreoffice = Count {
                words: statistic
                    .attr_usize(&Ns::Meta, "word-count")
                    .expect("words"),
                characters: statistic
                    .attr_usize(&Ns::Meta, "character-count")
                    .expect("characters"),
            };
            assert_eq!(
                in_paragraphs(document.body().expect("a body")),
                libreoffice,
                "{name}"
            );
        }
    }

    #[test]
    fn list_labels_are_not_counted() {
        let document = odox_core::doc::TextDocument::read(&read("text.odt")).expect("it reads");
        assert_eq!(
            in_paragraphs(document.body().expect("a body")),
            Count {
                words: 79,
                characters: 465
            }
        );
    }

    #[test]
    fn a_selection_counts_from_its_start_to_its_end_in_either_order() {
        let document = odox_core::doc::TextDocument::read(&read("text.odt")).expect("it reads");
        let body = document.body().expect("a body");
        let paths = crate::flow_model::paragraph_paths(body);
        let within = Count {
            words: 1,
            characters: 10,
        };
        assert_eq!(between(body, (&paths[1], 2), (&paths[1], 12)), within);
        let across = Count {
            words: 3,
            characters: 14,
        };
        assert_eq!(between(body, (&paths[3], 7), (&paths[2], 15)), across);
    }
}
