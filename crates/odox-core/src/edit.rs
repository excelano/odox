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

mod format;

use std::fmt;
use std::ops::Range;

use crate::xml::{Attribute, Element, Name, Node, Ns};

pub use format::{Mark, format, marked};

/// Why an edit was not made. Each is a state of the document rather than a
/// failure, and the window says which.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The cell is under a neighbour's span; the neighbour is the one to edit.
    Covered,
    /// The cell holds a formula, which this version does not edit.
    Formula,
    /// The document does not declare a namespace the edit would write in.
    Namespace,
    /// There is no such sheet, slide, shape or paragraph.
    NotFound,
    /// An end of the range is inside a table, a cell or a frame the range
    /// does not wholly contain, which a join of text does not take apart.
    Structure,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Covered => write!(f, "the cell is covered by a neighbour's span"),
            Self::Formula => write!(f, "the cell holds a formula"),
            Self::Namespace => write!(f, "the document does not declare the namespace needed"),
            Self::NotFound => write!(f, "nothing is there to edit"),
            Self::Structure => write!(f, "the range crosses a table or a frame"),
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

/// Whether an element is a paragraph or a heading: what an editor edits.
pub fn is_paragraph(element: &Element) -> bool {
    element.is(&Ns::Text, "p") || element.is(&Ns::Text, "h")
}

/// Whether an element inside a paragraph holds characters of the paragraph's
/// text, as a span or a link does, rather than being a mark or a field that
/// contributes none of its own.
pub fn holds_text(element: &Element) -> bool {
    is_inline_container_name(element)
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
    let added = with.chars().count();
    // Where the replaced range begins inside a text node, the new text goes
    // into that node before the old comes out, so that what replaces a bold
    // word is bold and a span emptied by the edit is not emptied first.
    let holder = segments(paragraph).into_iter().find(|s| {
        matches!(s.kind, Kind::Text(_)) && s.start <= start && start < s.start + s.kind.len()
    });
    if start < end
        && added > 0
        && let Some(segment) = holder
    {
        insert_into(paragraph, &segment.path, start - segment.start, with);
        cut(paragraph, start + added..end + added, |_| false);
    } else {
        cut(paragraph, start..end, |_| false);
        insert(paragraph, start, with);
    }
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

/// What a paragraph becomes when its whole text is replaced by what an editor
/// hands back: one paragraph, or several where the new text holds newlines
/// the old one did not.
///
/// The old and the new text are compared from both ends, and only the middle
/// they differ in is replaced, so spans and markers outside it are untouched
/// and a marker inside it stays where it was. A newline inside that middle is
/// a paragraph break: the paragraph is split there, each half with the
/// original's style, and the newline itself is not kept. A newline outside
/// it is a `text:line-break` the editor was shown and left alone.
pub fn rewrite(paragraph: &Element, edited: &str) -> Vec<Element> {
    let before = text(paragraph);
    let old: Vec<char> = before.chars().collect();
    let new: Vec<char> = edited.chars().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let inserted: String = new[prefix..new.len() - suffix].iter().collect();

    let mut whole = paragraph.clone();
    replace(&mut whole, prefix..old.len() - suffix, &inserted);

    // The breaks, as offsets into the rewritten text, last first so that each
    // split leaves the earlier offsets where they were.
    let mut breaks: Vec<usize> = inserted
        .chars()
        .enumerate()
        .filter(|(_, c)| *c == '\n')
        .map(|(i, _)| prefix + i)
        .collect();
    breaks.reverse();
    let mut after = Vec::new();
    for at in breaks {
        let (first, mut second) = split(&whole, at);
        // The newline itself, which the split left at the head of the second.
        replace(&mut second, 0..1, "");
        after.push(second);
        whole = first;
    }
    after.push(whole);
    after.reverse();
    after
}

/// Replace the paragraph at a path under a root with what the editor hands
/// back, which may be several paragraphs. Answers how many now stand there.
///
/// # Errors
///
/// The path leads to nothing, or to something that is not a paragraph.
pub fn apply(root: &mut Element, path: &[usize], edited: &str) -> Result<usize, Refused> {
    let (last, above) = path.split_last().ok_or(Refused::NotFound)?;
    let parent = root.at_mut(above).ok_or(Refused::NotFound)?;
    let Some(Node::Element(paragraph)) = parent.children.get(*last) else {
        return Err(Refused::NotFound);
    };
    if !is_paragraph(paragraph) {
        return Err(Refused::NotFound);
    }
    let paragraphs = rewrite(paragraph, edited);
    let count = paragraphs.len();
    parent
        .children
        .splice(*last..=*last, paragraphs.into_iter().map(Node::Element));
    Ok(count)
}

/// Split the paragraph at a path under a root at a character offset, the
/// second half becoming the paragraph after it, and answer where the second
/// half is.
///
/// In a list item the second half begins a new item after it, as Enter does
/// in a list, and whatever followed the paragraph in the item goes with it.
///
/// An item holding one empty paragraph is taken out of its list instead: the
/// paragraph stands where the item was, between the two halves of the list.
///
/// # Errors
///
/// The path leads to nothing, or to something that is not a paragraph.
pub fn split_at(root: &mut Element, path: &[usize], at: usize) -> Result<Vec<usize>, Refused> {
    let (last, above) = path.split_last().ok_or(Refused::NotFound)?;
    let parent = root.at_mut(above).ok_or(Refused::NotFound)?;
    let Some(Node::Element(paragraph)) = parent.children.get(*last) else {
        return Err(Refused::NotFound);
    };
    if !is_paragraph(paragraph) {
        return Err(Refused::NotFound);
    }
    if parent.is(&Ns::Text, "list-item")
        && parent.children.len() == 1
        && paragraph.children.is_empty()
    {
        return leave_list(root, above);
    }
    let (first, second) = split(paragraph, at);
    let parent = root.at_mut(above).ok_or(Refused::NotFound)?;
    if !parent.is(&Ns::Text, "list-item") {
        parent
            .children
            .splice(*last..=*last, [Node::Element(first), Node::Element(second)]);
        let mut second_at = above.to_vec();
        second_at.push(last + 1);
        return Ok(second_at);
    }

    // A new item, named the way the document names its items, holding the
    // second half and what followed it. The item's attributes stay with the
    // item: a start value or an id belongs to one item and not to two.
    let mut item = Element {
        name: parent.name.clone(),
        attrs: Vec::new(),
        children: vec![Node::Element(second)],
        self_closing: false,
    };
    item.children.extend(parent.children.drain(last + 1..));
    parent.children[*last] = Node::Element(first);
    let (item_at, list_path) = above.split_last().ok_or(Refused::NotFound)?;
    let enclosing = root.at_mut(list_path).ok_or(Refused::NotFound)?;
    enclosing.children.insert(item_at + 1, Node::Element(item));
    let mut second_at = list_path.to_vec();
    second_at.extend([item_at + 1, 0]);
    Ok(second_at)
}

/// Take an item that holds one empty paragraph out of its list: the list is
/// split around it and the paragraph stands between the halves, as Enter on
/// an empty item leaves the list in any word processor. A list left with no
/// items is dropped, and the half after the item continues the numbering.
fn leave_list(root: &mut Element, item_path: &[usize]) -> Result<Vec<usize>, Refused> {
    let (item_at, list_path) = item_path.split_last().ok_or(Refused::NotFound)?;
    let (list_at, container_path) = list_path.split_last().ok_or(Refused::NotFound)?;
    let continue_numbering = root.name_for(&Ns::Text, "continue-numbering");
    let list = root.at(list_path).ok_or(Refused::NotFound)?;
    let Some(Node::Element(item)) = list.children.get(*item_at) else {
        return Err(Refused::NotFound);
    };
    let Some(Node::Element(paragraph)) = item.children.first().cloned() else {
        return Err(Refused::NotFound);
    };
    let mut before = list.clone();
    before.children.truncate(*item_at);
    let mut after = list.clone();
    after.children.drain(..=*item_at);
    let before_holds_items = before.elements().next().is_some();
    let after_holds_items = after.elements().next().is_some();
    after.attrs.retain(|a| {
        !(a.name.is(&Ns::Text, "id")
            || a.name.local.as_ref() == "id" && a.name.prefix.as_deref() == Some("xml"))
    });
    after.set_attr(continue_numbering, "true");

    let container = root.at_mut(container_path).ok_or(Refused::NotFound)?;
    let mut replacement = Vec::new();
    if before_holds_items {
        replacement.push(Node::Element(before));
    }
    let paragraph_at = *list_at + replacement.len();
    replacement.push(Node::Element(paragraph));
    if after_holds_items {
        replacement.push(Node::Element(after));
    }
    container
        .children
        .splice(*list_at..=*list_at, replacement);
    let mut at = container_path.to_vec();
    at.push(paragraph_at);
    Ok(at)
}

/// Replace the text from one position to another with new text, and join
/// what is left of the last paragraph onto the first. A position is a
/// paragraph's path under the root and a character offset into its text.
///
/// Everything between the two goes, as a selection over it takes it:
/// paragraphs, tables and frames the range wholly contains, and list items
/// and lists left with nothing in them. The first paragraph keeps its style
/// and its place, and the last one's remaining text follows the new text in
/// it.
///
/// # Errors
///
/// Either path does not lead to a paragraph, or the first is not before the
/// last; or an end is inside a table, a cell or a frame that the range does
/// not wholly contain ([`Refused::Structure`]), which no join takes apart.
/// Each is refused before anything changes.
pub fn replace_range(
    root: &mut Element,
    from: (&[usize], usize),
    to: (&[usize], usize),
    with: &str,
) -> Result<(), Refused> {
    let ((from, start), (to, end)) = (from, to);
    if !root.at(from).is_some_and(is_paragraph) || !root.at(to).is_some_and(is_paragraph) {
        return Err(Refused::NotFound);
    }
    if from == to {
        let paragraph = root.at_mut(from).ok_or(Refused::NotFound)?;
        replace(paragraph, start..end, with);
        return Ok(());
    }
    // The deepest element holding both ends, and which of its children each
    // end is in.
    let common = from.iter().zip(to).take_while(|(a, b)| a == b).count();
    if common >= from.len() || common >= to.len() || from[common] >= to[common] {
        return Err(Refused::NotFound);
    }
    if end_inside_structure(root, common, from, to) {
        return Err(Refused::Structure);
    }
    let (first_child, last_child) = (from[common], to[common]);

    // The last paragraph first: it is after everything else touched, so
    // taking it apart moves no path still to be used.
    let last = root.at_mut(to).ok_or(Refused::NotFound)?;
    replace(last, 0..end, "");
    let tail = std::mem::take(&mut last.children);

    let holder = root.at_mut(&from[..common]).ok_or(Refused::NotFound)?;
    let emptied = match holder.children.get_mut(last_child) {
        Some(Node::Element(child)) if common + 1 < to.len() => {
            strip_before(child, &to[common + 1..])
        }
        _ => true,
    };
    if emptied {
        holder.children.remove(last_child);
    }
    holder.children.drain(first_child + 1..last_child);
    if let Some(Node::Element(child)) = holder.children.get_mut(first_child) {
        strip_after(child, &from[common + 1..]);
    }

    let first = root.at_mut(from).ok_or(Refused::NotFound)?;
    let len = text(first).chars().count();
    replace(first, start..len, with);
    let mut rest = Element::new("text", "p", Ns::Text);
    rest.children = tail;
    join(first, rest);
    Ok(())
}

/// Whether either end of a range, whose two paths agree for their first
/// `common` steps, sits inside a table, a cell or a frame that the range
/// does not wholly contain. One wholly between the ends goes with the rest.
fn end_inside_structure(root: &Element, common: usize, from: &[usize], to: &[usize]) -> bool {
    let inside = |path: &[usize]| {
        (common + 1..path.len()).any(|depth| root.at(&path[..depth]).is_some_and(is_structure))
    };
    inside(from) || inside(to)
}

fn is_structure(element: &Element) -> bool {
    element.is(&Ns::Table, "table")
        || element.is(&Ns::Table, "table-row")
        || element.is(&Ns::Table, "table-cell")
        || element.is(&Ns::Table, "covered-table-cell")
        || element.is(&Ns::Draw, "frame")
        || element.is(&Ns::Draw, "text-box")
}

/// Remove, under an element, everything before the end of a path, the end
/// itself, and every element that leaves empty. Answers whether the element
/// is left with no elements in it.
fn strip_before(element: &mut Element, path: &[usize]) -> bool {
    let Some((&at, rest)) = path.split_first() else {
        return true;
    };
    let emptied = match element.children.get_mut(at) {
        Some(Node::Element(child)) if !rest.is_empty() => strip_before(child, rest),
        _ => true,
    };
    let through = if emptied { at + 1 } else { at };
    element
        .children
        .drain(..through.min(element.children.len()));
    !element
        .children
        .iter()
        .any(|n| matches!(n, Node::Element(_)))
}

/// Remove, under an element, everything after the end of a path, at every
/// level down to it.
fn strip_after(element: &mut Element, path: &[usize]) {
    let Some((&at, rest)) = path.split_first() else {
        return;
    };
    if let Some(Node::Element(child)) = element.children.get_mut(at) {
        strip_after(child, rest);
    }
    element.children.truncate(at + 1);
}

/// Join the paragraph at a path onto the paragraph before it among its
/// parent's children, and answer where the joined paragraph is.
///
/// # Errors
///
/// The path leads to nothing, or the element before it is not a paragraph:
/// there is none, or a table or a list stands between, which a join would
/// otherwise delete.
pub fn join_with_previous(root: &mut Element, path: &[usize]) -> Result<Vec<usize>, Refused> {
    let (last, above) = path.split_last().ok_or(Refused::NotFound)?;
    let parent = root.at_mut(above).ok_or(Refused::NotFound)?;
    let paragraph_node = |node: &Node| matches!(node, Node::Element(e) if is_paragraph(e));
    if !parent.children.get(*last).is_some_and(paragraph_node) {
        return Err(Refused::NotFound);
    }
    let previous = parent.children[..*last]
        .iter()
        .rposition(|node| matches!(node, Node::Element(_)))
        .filter(|&index| paragraph_node(&parent.children[index]))
        .ok_or(Refused::NotFound)?;
    // Whitespace between the two, which was between them and is now inside
    // neither, goes too.
    let Node::Element(second) = parent.children.remove(*last) else {
        return Err(Refused::NotFound);
    };
    parent.children.drain(previous + 1..*last);
    let Some(Node::Element(first)) = parent.children.get_mut(previous) else {
        return Err(Refused::NotFound);
    };
    join(first, second);
    let mut at = above.to_vec();
    at.push(previous);
    Ok(at)
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
    if let Some(segment) = target {
        insert_into(paragraph, &segment.path, at - segment.start, with);
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

/// Put text into the text node at a path, at a character offset within it.
fn insert_into(paragraph: &mut Element, path: &[usize], offset: usize, with: &str) {
    if let Some((parent, index)) = parent_of(paragraph, path)
        && let Some(Node::Text(t) | Node::CData(t)) = parent.children.get_mut(index)
    {
        let byte = t.char_indices().nth(offset).map_or(t.len(), |(b, _)| b);
        t.insert_str(byte, with);
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
