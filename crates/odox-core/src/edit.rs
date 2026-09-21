//! Changing the text of a paragraph, and what an edit may refuse.
//!
//! A paragraph's text is not one string in the tree. It is text nodes, inside
//! spans and links and marks, with `text:s`, `text:tab` and `text:line-break`
//! standing for the whitespace ODF will not write literally, and with elements
//! that contribute no characters at all — a bookmark, a frame, a note, a field
//! — sitting between them. An editor works on the flat string and this module
//! maps the edit back: characters are deleted from the nodes that hold them,
//! new text goes into the node at the insertion point, and every zero-length
//! element stays where it was. DESIGN.md §11.
//!
//! The string the editor sees, [`text`], is built from the tree and not from
//! the renderer's layout, because the two differ: the renderer draws a tab as
//! four spaces and a footnote as its citation. Here a tab is a tab, and a
//! footnote contributes nothing and is kept.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::fmt;
use std::ops::Range;

use crate::xml::{Attribute, Element, Name, Node, Ns};

/// Why an edit was not made. Each is a state of the document rather than a
/// failure, and the window says which.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The cell is under a neighbour's span; the neighbour is the one to edit.
    Covered,
    /// The cell holds a formula, which this version does not edit.
    Formula,
    /// There is no such sheet, slide, shape or paragraph.
    NotFound,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Covered => write!(f, "the cell is covered by a neighbour's span"),
            Self::Formula => write!(f, "the cell holds a formula"),
            Self::NotFound => write!(f, "nothing is there to edit"),
        }
    }
}

impl std::error::Error for Refused {}

/// What one node contributes to a paragraph's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A text node, this many characters long.
    Text(usize),
    /// `text:s`, this many spaces.
    Spaces(usize),
    /// `text:tab`.
    Tab,
    /// `text:line-break`.
    LineBreak,
    /// An element that contributes nothing and is kept where it is.
    Marker,
}

impl Kind {
    fn len(self) -> usize {
        match self {
            Self::Text(n) | Self::Spaces(n) => n,
            Self::Tab | Self::LineBreak => 1,
            Self::Marker => 0,
        }
    }
}

/// One node's place in the paragraph's text.
#[derive(Debug)]
struct Segment {
    /// The route from the paragraph to the node.
    path: Vec<usize>,
    /// Where its characters begin.
    start: usize,
    kind: Kind,
}

/// Whether an element holds text on the paragraph's behalf: a span, a link, a
/// ruby or a field whose contents are the paragraph's own characters. Anything
/// else inside a paragraph is a marker, and stays; so is one of these with
/// nothing in it, which is what lets a split carry it to one side.
fn is_inline_container(element: &Element) -> bool {
    element.name.ns == Ns::Text
        && !element.children.is_empty()
        && matches!(
            &*element.name.local,
            "span" | "a" | "bibliography-mark" | "ruby" | "ruby-base" | "meta" | "meta-field"
        )
}

/// The containers above, whether or not they hold anything: what an edit may
/// drop once it has emptied one.
fn is_inline_container_name(element: &Element) -> bool {
    element.name.ns == Ns::Text
        && matches!(
            &*element.name.local,
            "span" | "a" | "bibliography-mark" | "ruby" | "ruby-base" | "meta" | "meta-field"
        )
}

fn segments(paragraph: &Element) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut path = Vec::new();
    let mut at = 0;
    collect(paragraph, &mut path, &mut at, &mut out);
    out
}

fn collect(parent: &Element, path: &mut Vec<usize>, at: &mut usize, out: &mut Vec<Segment>) {
    for (index, child) in parent.children.iter().enumerate() {
        let kind = match child {
            Node::Text(t) | Node::CData(t) => Kind::Text(t.chars().count()),
            Node::Comment(_) | Node::ProcessingInstruction(_) => continue,
            Node::Element(e) if e.is(&Ns::Text, "s") => {
                Kind::Spaces(e.attr_usize(&Ns::Text, "c").unwrap_or(1))
            }
            Node::Element(e) if e.is(&Ns::Text, "tab") => Kind::Tab,
            Node::Element(e) if e.is(&Ns::Text, "line-break") => Kind::LineBreak,
            Node::Element(e) if is_inline_container(e) => {
                path.push(index);
                collect(e, path, at, out);
                path.pop();
                continue;
            }
            Node::Element(_) => Kind::Marker,
        };
        path.push(index);
        out.push(Segment {
            path: path.clone(),
            start: *at,
            kind,
        });
        path.pop();
        *at += kind.len();
    }
}

/// The paragraph's text as an editor sees it: every character the tree
/// holds, in order, with ODF's whitespace elements as the characters they
/// stand for.
pub fn text(paragraph: &Element) -> String {
    let mut out = String::new();
    write_text(paragraph, &mut out);
    out
}

fn write_text(parent: &Element, out: &mut String) {
    for child in &parent.children {
        match child {
            Node::Text(t) | Node::CData(t) => out.push_str(t),
            Node::Element(e) if e.is(&Ns::Text, "s") => {
                for _ in 0..e.attr_usize(&Ns::Text, "c").unwrap_or(1) {
                    out.push(' ');
                }
            }
            Node::Element(e) if e.is(&Ns::Text, "tab") => out.push('\t'),
            Node::Element(e) if e.is(&Ns::Text, "line-break") => out.push('\n'),
            Node::Element(e) if is_inline_container(e) => write_text(e, out),
            Node::Element(_) | Node::Comment(_) | Node::ProcessingInstruction(_) => {}
        }
    }
}

/// Replace a range of the paragraph's text, in characters, with new text.
///
/// The characters are removed from the nodes that hold them and the new text
/// goes into the text node at the start of the range, or into a new one where
/// none is there. Every element that contributes no characters stays where it
/// was, so a bookmark inside the deleted range is a bookmark still. Whitespace
/// is then written the way ODF requires, so the new text may hold tabs,
/// newlines and runs of spaces.
///
/// A range past the end of the text is clamped to it.
pub fn replace(paragraph: &mut Element, range: Range<usize>, with: &str) {
    let len = text(paragraph).chars().count();
    let start = range.start.min(len);
    let end = range.end.clamp(start, len);
    cut(paragraph, start..end, |_| false);
    insert(paragraph, start, with);
    normalize(paragraph);
}

/// Split a paragraph at a character offset into the paragraph before it and
/// the paragraph after, each with the original's style and attributes.
///
/// A zero-length element at the split point stays with the first half. The
/// second half drops any `xml:id` or `text:id`, which name one element and
/// cannot name two.
pub fn split(paragraph: &Element, at: usize) -> (Element, Element) {
    let len = text(paragraph).chars().count();
    let at = at.min(len);
    let mut first = paragraph.clone();
    cut(&mut first, at..len, |start| start > at);
    let mut second = paragraph.clone();
    cut(&mut second, 0..at, |start| start <= at);
    second.attrs.retain(|a| {
        !(a.name.is(&Ns::Text, "id")
            || a.name.local.as_ref() == "id" && a.name.prefix.as_deref() == Some("xml"))
    });
    normalize(&mut first);
    normalize(&mut second);
    (first, second)
}

/// Join a paragraph onto the end of another. The first keeps its style; the
/// second's contents, markers included, follow the first's.
pub fn join(first: &mut Element, second: Element) {
    first.children.extend(second.children);
    first.self_closing = false;
    normalize(first);
}

/// Delete a range of characters, and any zero-length element whose position
/// the predicate names.
///
/// Segments are visited last to first, so removing a node never moves one
/// that is still to be visited.
fn cut(paragraph: &mut Element, range: Range<usize>, drop_marker: impl Fn(usize) -> bool) {
    for segment in segments(paragraph).into_iter().rev() {
        let end = segment.start + segment.kind.len();
        let overlap_start = range.start.max(segment.start);
        let overlap_end = range.end.min(end);
        let overlaps = overlap_start < overlap_end;
        let remove = match segment.kind {
            Kind::Marker => drop_marker(segment.start),
            Kind::Tab | Kind::LineBreak => overlaps,
            Kind::Spaces(n) => {
                if !overlaps {
                    continue;
                }
                let left = n - (overlap_end - overlap_start);
                if left > 0
                    && let Some(e) = paragraph.at_mut(&segment.path)
                {
                    set_count(e, left);
                }
                left == 0
            }
            Kind::Text(_) => {
                if !overlaps {
                    continue;
                }
                let Some((parent, index)) = parent_of(paragraph, &segment.path) else {
                    continue;
                };
                let Some(Node::Text(t) | Node::CData(t)) = parent.children.get_mut(index) else {
                    continue;
                };
                let kept: String = t
                    .chars()
                    .enumerate()
                    .filter(|(i, _)| {
                        let at = segment.start + i;
                        !(overlap_start..overlap_end).contains(&at)
                    })
                    .map(|(_, c)| c)
                    .collect();
                *t = kept;
                t.is_empty()
            }
        };
        if remove && let Some((parent, index)) = parent_of(paragraph, &segment.path) {
            parent.children.remove(index);
            prune_emptied(paragraph, &segment.path[..segment.path.len() - 1]);
        }
    }
}

/// A span or a link left with nothing in it says nothing, and goes; and so
/// does the one it was in, if that is now empty too. One that was empty
/// before the edit is not touched, because it is not this edit's: it never
/// had a child removed.
///
/// Done as soon as the container empties, while the segments still to be
/// visited are all before it and its own index is still right.
fn prune_emptied(paragraph: &mut Element, container: &[usize]) {
    let mut path = container.to_vec();
    while !path.is_empty() {
        let Some((parent, index)) = parent_of(paragraph, &path) else {
            return;
        };
        match parent.children.get(index) {
            Some(Node::Element(e)) if is_inline_container_name(e) && e.children.is_empty() => {
                parent.children.remove(index);
                path.pop();
            }
            _ => return,
        }
    }
}

/// Put text at a character offset.
///
/// Into the text node whose characters surround the offset; failing that, the
/// one that ends there, then the one that begins there; failing that, as a new
/// text node after whatever ends there, which puts new text after a bookmark
/// at the same offset rather than before it.
fn insert(paragraph: &mut Element, at: usize, with: &str) {
    if with.is_empty() {
        return;
    }
    let segments = segments(paragraph);
    let text_at = |predicate: &dyn Fn(&Segment, usize) -> bool| {
        segments
            .iter()
            .find(|s| matches!(s.kind, Kind::Text(_)) && predicate(s, s.start + s.kind.len()))
    };
    let target = text_at(&|s, end| s.start < at && at < end)
        .or_else(|| text_at(&|_, end| end == at))
        .or_else(|| text_at(&|s, _| s.start == at));
    if let Some(segment) = target
        && let Some((parent, index)) = parent_of(paragraph, &segment.path)
        && let Some(Node::Text(t) | Node::CData(t)) = parent.children.get_mut(index)
    {
        let byte = t
            .char_indices()
            .nth(at - segment.start)
            .map_or(t.len(), |(b, _)| b);
        t.insert_str(byte, with);
        return;
    }
    // No text node touches the offset: a new one goes after the last segment
    // ending at it, before the first beginning at it, or at the paragraph's
    // end.
    let after = segments
        .iter()
        .rev()
        .find(|s| s.start + s.kind.len() == at)
        .map(|s| s.path.clone());
    let before = segments
        .iter()
        .find(|s| s.start >= at)
        .map(|s| s.path.clone());
    let node = Node::Text(with.to_owned());
    if let Some(path) = after
        && let Some((parent, index)) = parent_of(paragraph, &path)
    {
        parent.children.insert(index + 1, node);
    } else if let Some(path) = before
        && let Some((parent, index)) = parent_of(paragraph, &path)
    {
        parent.children.insert(index, node);
    } else {
        paragraph.children.push(node);
        paragraph.self_closing = false;
    }
}

/// The parent of the node a path leads to, and the node's index in it.
fn parent_of<'a>(paragraph: &'a mut Element, path: &[usize]) -> Option<(&'a mut Element, usize)> {
    let (last, above) = path.split_last()?;
    Some((paragraph.at_mut(above)?, *last))
}

fn set_count(space: &mut Element, count: usize) {
    if count <= 1 {
        space.remove_attr(&Ns::Text, "c");
    } else {
        let name = space
            .attrs
            .iter()
            .find(|a| a.name.is(&Ns::Text, "c"))
            .map_or_else(
                || {
                    Name::new(
                        space.name.prefix.as_deref().unwrap_or("text"),
                        "c",
                        Ns::Text,
                    )
                },
                |a| a.name.clone(),
            );
        space.set_attr(name, count.to_string());
    }
}

/// Write whitespace the way ODF requires.
///
/// A conforming reader collapses a run of spaces to one and drops the spaces
/// at the start of a paragraph, and reads a literal tab or newline as a
/// space. So a tab becomes `text:tab`, a newline `text:line-break`, a run of
/// spaces one literal space and a `text:s` for the rest, and a run at the very
/// start a `text:s` for all of them. Text nodes are split where an element has
/// to go; nothing else in the tree is touched.
fn normalize(paragraph: &mut Element) {
    let prefix = paragraph
        .name
        .prefix
        .as_deref()
        .unwrap_or("text")
        .to_owned();
    let mut previous_was_space = true;
    normalize_in(paragraph, &prefix, &mut previous_was_space);
}

fn normalize_in(parent: &mut Element, prefix: &str, previous_was_space: &mut bool) {
    merge_text(parent);
    let mut index = 0;
    while index < parent.children.len() {
        match &mut parent.children[index] {
            Node::Text(t) | Node::CData(t) => {
                let replacement = encode(t, prefix, previous_was_space);
                match replacement {
                    None => index += 1,
                    Some(nodes) => {
                        let count = nodes.len();
                        parent.children.splice(index..=index, nodes);
                        index += count;
                    }
                }
            }
            Node::Element(e) if is_inline_container(e) => {
                normalize_in(e, prefix, previous_was_space);
                index += 1;
            }
            Node::Element(e) => {
                // Whitespace elements are whitespace; a marker is nothing,
                // and what came before it still stands.
                if e.is(&Ns::Text, "s") || e.is(&Ns::Text, "tab") || e.is(&Ns::Text, "line-break") {
                    *previous_was_space = true;
                }
                index += 1;
            }
            Node::Comment(_) | Node::ProcessingInstruction(_) => index += 1,
        }
    }
}

/// Join text nodes that sit side by side into one.
///
/// The parser keeps an entity reference as a text node of its own, so a
/// paragraph reads `a`, `&lt;`, `b` as three nodes; delete the middle one and
/// the two left would serialize as one and parse back as one, which is a tree
/// that is not equal to itself across a round trip. Joined here, it is.
fn merge_text(parent: &mut Element) {
    let mut index = 1;
    while index < parent.children.len() {
        let joinable = matches!(
            (&parent.children[index - 1], &parent.children[index]),
            (Node::Text(_), Node::Text(_))
        );
        if joinable {
            let Node::Text(tail) = parent.children.remove(index) else {
                unreachable!("matched a text node");
            };
            if let Node::Text(head) = &mut parent.children[index - 1] {
                head.push_str(&tail);
            }
        } else {
            index += 1;
        }
    }
}

/// The nodes a text node becomes, or `None` where it is already as ODF
/// writes it.
fn encode(text: &str, prefix: &str, previous_was_space: &mut bool) -> Option<Vec<Node>> {
    let needs_work = text.contains(['\t', '\n', '\r'])
        || text.contains("  ")
        || (*previous_was_space && text.starts_with(' '));
    if !needs_work {
        if let Some(last) = text.chars().last() {
            *previous_was_space = last == ' ';
        }
        return None;
    }
    let mut nodes = Vec::new();
    let mut run = String::new();
    let mut spaces = 0usize;
    let flush_spaces = |nodes: &mut Vec<Node>,
                        run: &mut String,
                        spaces: &mut usize,
                        previous_was_space: &mut bool| {
        if *spaces == 0 {
            return;
        }
        // One literal space where a space may be literal, and `text:s` for
        // whatever a reader would otherwise collapse.
        let mut counted = *spaces;
        if !*previous_was_space {
            run.push(' ');
            counted -= 1;
        }
        if counted > 0 {
            if !run.is_empty() {
                nodes.push(Node::Text(std::mem::take(run)));
            }
            let mut s = Element::new(prefix, "s", Ns::Text);
            if counted > 1 {
                s.attrs.push(Attribute {
                    name: Name::new(prefix, "c", Ns::Text),
                    value: counted.to_string(),
                });
            }
            nodes.push(Node::Element(s));
        }
        *spaces = 0;
        *previous_was_space = true;
    };
    for c in text.chars() {
        match c {
            ' ' => spaces += 1,
            // A carriage return is not a character ODF has a spelling for; a
            // newline follows it wherever it was typed.
            '\r' => {}
            '\t' | '\n' => {
                flush_spaces(&mut nodes, &mut run, &mut spaces, previous_was_space);
                if !run.is_empty() {
                    nodes.push(Node::Text(std::mem::take(&mut run)));
                }
                let local = if c == '\t' { "tab" } else { "line-break" };
                nodes.push(Node::Element(Element::new(prefix, local, Ns::Text)));
                *previous_was_space = true;
            }
            other => {
                flush_spaces(&mut nodes, &mut run, &mut spaces, previous_was_space);
                run.push(other);
                *previous_was_space = false;
            }
        }
    }
    flush_spaces(&mut nodes, &mut run, &mut spaces, previous_was_space);
    if !run.is_empty() {
        nodes.push(Node::Text(run));
    }
    Some(nodes)
}
