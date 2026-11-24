//! Tracked changes: the `text:tracked-changes` regions a document records,
//! with who made each and when, and taking each in (accepting it) or undoing
//! it (rejecting it). DESIGN.md §11.
//!
//! An insertion is the text between a `text:change-start` and a
//! `text:change-end` naming its region; a deletion is a `text:change` where
//! the text was, its paragraphs kept in the region; a format change is marked
//! as an insertion is. Edits made here are not recorded as changes.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::HashSet;

use super::annotation::take_out;
use super::{Kind, Refused, insert_inline, is_paragraph, replace_range, segments, split};
use crate::xml::{Element, Node, Ns};

/// What a change did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    /// Text put in.
    Insertion,
    /// Text taken out.
    Deletion,
    /// Formatting changed.
    Format,
}

/// A recorded change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The region's `text:id`, which its marks name.
    pub id: String,
    /// What it did.
    pub kind: ChangeKind,
    /// Who made it, where the document says.
    pub author: Option<String>,
    /// When, as ISO 8601, where the document says.
    pub date: Option<String>,
    /// What a deletion took out, a line to a paragraph.
    pub deleted: Option<String>,
}

/// The changes a document records, in the order it lists them.
pub fn changes(content: &Element) -> Vec<Change> {
    let Some(list) = list_path(content).and_then(|path| content.at(&path)) else {
        return Vec::new();
    };
    list.elements()
        .filter(|e| e.is(&Ns::Text, "changed-region"))
        .filter_map(read)
        .collect()
}

fn read(region: &Element) -> Option<Change> {
    let id = region.attr(&Ns::Text, "id")?.to_owned();
    let (kind, what) = [
        ("insertion", ChangeKind::Insertion),
        ("deletion", ChangeKind::Deletion),
        ("format-change", ChangeKind::Format),
    ]
    .into_iter()
    .find_map(|(local, kind)| region.child(&Ns::Text, local).map(|e| (kind, e)))?;
    let info = what.child(&Ns::Office, "change-info");
    let said = |local: &str| {
        info.and_then(|info| info.child(&Ns::Dc, local))
            .map(Element::plain_text)
            .filter(|text| !text.trim().is_empty())
    };
    let deleted = (kind == ChangeKind::Deletion).then(|| {
        what.elements()
            .filter(|e| !e.is(&Ns::Office, "change-info"))
            .map(Element::plain_text)
            .collect::<Vec<_>>()
            .join("\n")
    });
    Some(Change {
        id,
        kind,
        author: said("creator"),
        date: said("date"),
        deleted,
    })
}

/// A paragraph's change marks: where each stands in its text, which of
/// `start`, `end` or `change` it is, and the change it names.
pub fn change_marks(paragraph: &Element) -> Vec<(usize, &'static str, String)> {
    segments(paragraph)
        .into_iter()
        .filter(|segment| matches!(segment.kind, Kind::Marker))
        .filter_map(|segment| {
            let mark = paragraph.at(&segment.path)?;
            let kind = ["change-start", "change-end", "change"]
                .into_iter()
                .zip(["start", "end", "change"])
                .find_map(|(local, kind)| mark.is(&Ns::Text, local).then_some(kind))?;
            Some((
                segment.start,
                kind,
                mark.attr(&Ns::Text, "change-id")?.to_owned(),
            ))
        })
        .collect()
}

/// Whether the document asks for edits to be recorded as changes: it has a
/// `text:tracked-changes` whose `text:track-changes` is not `false`, true
/// being the attribute's default.
pub fn tracking(content: &Element) -> bool {
    list_path(content)
        .and_then(|path| content.at(&path))
        .is_some_and(|list| list.attr(&Ns::Text, "track-changes") != Some("false"))
}

/// The path of the body's `office:text`.
fn text_path(content: &Element) -> Option<Vec<usize>> {
    let (body, element) = content
        .elements_indexed()
        .find(|(_, e)| e.is(&Ns::Office, "body"))?;
    let (text, _) = element
        .elements_indexed()
        .find(|(_, e)| e.is(&Ns::Office, "text"))?;
    Some(vec![body, text])
}

/// The path of the body's `text:tracked-changes`.
fn list_path(content: &Element) -> Option<Vec<usize>> {
    let text = text_path(content)?;
    let (list, _) = content
        .at(&text)?
        .elements_indexed()
        .find(|(_, e)| e.is(&Ns::Text, "tracked-changes"))?;
    Some([text.as_slice(), &[list]].concat())
}

/// The path of a change's region.
fn region_path(content: &Element, id: &str) -> Option<Vec<usize>> {
    let list = list_path(content)?;
    let (at, _) = content
        .at(&list)?
        .elements_indexed()
        .find(|(_, region)| region.attr(&Ns::Text, "id") == Some(id))?;
    Some([list.as_slice(), &[at]].concat())
}

/// Every change mark in the body, with the change it names, in document
/// order.
fn all_marks(content: &Element) -> Vec<(Vec<usize>, String)> {
    fn walk(element: &Element, path: &mut Vec<usize>, into: &mut Vec<(Vec<usize>, String)>) {
        for (index, child) in element.elements_indexed() {
            if child.is(&Ns::Text, "tracked-changes") {
                continue;
            }
            path.push(index);
            match child.attr(&Ns::Text, "change-id") {
                Some(id) => into.push((path.clone(), id.to_owned())),
                None => walk(child, path, into),
            }
            path.pop();
        }
    }
    let mut into = Vec::new();
    walk(content, &mut Vec::new(), &mut into);
    into
}

/// The marks in the body that name a change, in document order.
fn marks(content: &Element, id: &str) -> Vec<Vec<usize>> {
    all_marks(content)
        .into_iter()
        .filter(|(_, named)| named == id)
        .map(|(path, _)| path)
        .collect()
}

/// Take out the regions of changes whose marks an edit took out of the body:
/// those marked before it and no longer.
fn prune(content: &mut Element, marked: &HashSet<String>) -> Result<(), Refused> {
    let still: HashSet<String> = all_marks(content).into_iter().map(|(_, id)| id).collect();
    for change in changes(content) {
        if marked.contains(&change.id) && !still.contains(&change.id) {
            forget(content, &change.id, &[])?;
        }
    }
    Ok(())
}

/// Where a mark stands: the paragraph holding it, and its place in the
/// paragraph's text.
fn position(content: &Element, mark: &[usize]) -> Option<(Vec<usize>, usize)> {
    let depth = (1..mark.len())
        .rev()
        .find(|&depth| content.at(&mark[..depth]).is_some_and(is_paragraph))?;
    let paragraph = content.at(&mark[..depth])?;
    let within = &mark[depth..];
    let at = segments(paragraph)
        .into_iter()
        .find(|segment| matches!(segment.kind, Kind::Marker) && segment.path == within)?
        .start;
    Some((mark[..depth].to_vec(), at))
}

/// Take out a change's marks, last first, and its region, and the list of
/// regions where it is left empty.
fn forget(content: &mut Element, id: &str, marks: &[Vec<usize>]) -> Result<(), Refused> {
    for mark in marks.iter().rev() {
        take_out(content, mark)?;
    }
    let region = region_path(content, id).ok_or(Refused::NotFound)?;
    take_out(content, &region)?;
    if let Some(list) = list_path(content)
        && content
            .at(&list)
            .is_some_and(|l| l.elements().next().is_none() && l.attrs.is_empty())
    {
        take_out(content, &list)?;
    }
    Ok(())
}

/// Take a change in: an insertion's text stays, a deletion's is gone, and
/// the record of it goes.
///
/// # Errors
///
/// There is no such change, or a deletion's text cannot be put back where it
/// was.
pub fn accept(content: &mut Element, id: &str) -> Result<(), Refused> {
    region_path(content, id).ok_or(Refused::NotFound)?;
    let marks = marks(content, id);
    forget(content, id, &marks)
}

/// Undo a change: an insertion's text goes, a deletion's comes back where it
/// was, and the record of it goes, with the record of any change whose marks
/// went with the text.
///
/// # Errors
///
/// There is no such change, or it cannot be undone as it is marked: an
/// insertion without both its ends in paragraphs, or a deletion without its
/// one mark. Nothing changes then.
pub fn reject(content: &mut Element, id: &str) -> Result<(), Refused> {
    let before = content.clone();
    let marked: HashSet<String> = all_marks(content).into_iter().map(|(_, id)| id).collect();
    let undone = undo(content, id).and_then(|()| prune(content, &marked));
    if undone.is_err() {
        *content = before;
    }
    undone
}

fn undo(content: &mut Element, id: &str) -> Result<(), Refused> {
    let region = region_path(content, id).ok_or(Refused::NotFound)?;
    let change = content
        .at(&region)
        .and_then(read)
        .ok_or(Refused::NotFound)?;
    let marks = marks(content, id);
    match change.kind {
        ChangeKind::Format => forget(content, id, &marks),
        ChangeKind::Insertion => {
            let named = |mark: &Vec<usize>, local: &str| {
                content.at(mark).is_some_and(|e| e.is(&Ns::Text, local))
            };
            let [start, end] = marks.as_slice() else {
                return Err(Refused::Structure);
            };
            if !named(start, "change-start") || !named(end, "change-end") {
                return Err(Refused::Structure);
            }
            let (Some((from, first)), Some((to, last))) =
                (position(content, start), position(content, end))
            else {
                return Err(Refused::Structure);
            };
            take_out(content, end)?;
            take_out(content, start)?;
            replace_range(content, (&from, first), (&to, last), "")?;
            forget(content, id, &[])
        }
        ChangeKind::Deletion => {
            let deleted: Vec<Element> = content
                .at(&region)
                .and_then(|r| r.child(&Ns::Text, "deletion"))
                .map(|d| {
                    d.elements()
                        .filter(|e| !e.is(&Ns::Office, "change-info"))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            let [mark] = marks.as_slice() else {
                return Err(Refused::Structure);
            };
            let within_paragraph =
                (1..mark.len()).any(|depth| content.at(&mark[..depth]).is_some_and(is_paragraph));
            let at = position(content, mark);
            take_out(content, mark)?;
            match at {
                Some((paragraph, offset)) => restore(content, &paragraph, offset, &deleted)?,
                None if !within_paragraph => {
                    let (&index, parent) = mark.split_last().ok_or(Refused::NotFound)?;
                    let parent = content.at_mut(parent).ok_or(Refused::NotFound)?;
                    parent
                        .children
                        .splice(index..index, deleted.into_iter().map(Node::Element));
                }
                None => return Err(Refused::Structure),
            }
            forget(content, id, &[])
        }
    }
}

/// Put deleted blocks back at a place in a paragraph. One paragraph's text
/// goes in there; otherwise the paragraph is split there, a first deleted
/// paragraph's text ends the first half, a last one, keeping its own kind and
/// style, begins the second, and every other block stands between them.
fn restore(
    content: &mut Element,
    path: &[usize],
    at: usize,
    deleted: &[Element],
) -> Result<(), Refused> {
    let paragraph = content.at_mut(path).ok_or(Refused::NotFound)?;
    if let [only] = deleted
        && is_paragraph(only)
    {
        insert_inline(paragraph, at, &only.children);
        return Ok(());
    }
    let (mut head, tail) = split(paragraph, at);
    let mut rest = deleted;
    if let [first, after @ ..] = rest
        && is_paragraph(first)
    {
        head.children.extend(first.children.iter().cloned());
        rest = after;
    }
    let end = match rest.split_last() {
        Some((last, middle)) if is_paragraph(last) => {
            rest = middle;
            let mut end = last.clone();
            end.children.extend(tail.children);
            end
        }
        _ => tail,
    };
    let mut end = end;
    end.self_closing = end.children.is_empty();
    head.self_closing = head.children.is_empty();
    *paragraph = head;
    let (&index, parent) = path.split_last().ok_or(Refused::NotFound)?;
    let parent = content.at_mut(parent).ok_or(Refused::NotFound)?;
    let after = rest
        .iter()
        .cloned()
        .chain(std::iter::once(end))
        .map(Node::Element);
    let at = index + 1;
    parent.children.splice(at..at, after);
    Ok(())
}

/// Take every change in.
///
/// # Errors
///
/// As [`accept`]; the changes before the one refused stay taken in.
pub fn accept_all(content: &mut Element) -> Result<(), Refused> {
    for change in changes(content) {
        accept(content, &change.id)?;
    }
    Ok(())
}

/// Undo every change.
///
/// # Errors
///
/// As [`reject`].
pub fn reject_all(content: &mut Element) -> Result<(), Refused> {
    for change in changes(content).into_iter().rev() {
        reject(content, &change.id)?;
    }
    Ok(())
}
