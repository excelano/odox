//! Comments in a paragraph: the `office:annotation` ODF keeps in the text at
//! the point it is anchored, with who wrote it, when and its paragraphs, and
//! the `office:annotation-end` that closes one over a range. DESIGN.md §11.
//!
//! A comment adds nothing to the paragraph's text, so it is found by where it
//! stands in that text, and edits around it leave it where it is.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::ops::Range;

use super::{Kind, Refused, is_paragraph, segments, set_count};
use crate::value::Length;
use crate::xml::{Element, Node, Ns};

/// A comment in a paragraph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    /// Who wrote it, where the document says.
    pub author: Option<String>,
    /// When, as ISO 8601, where the document says.
    pub date: Option<String>,
    /// Its text, a line to a paragraph.
    pub text: String,
    /// Where in the paragraph's text the range it is on ends, for one on a
    /// range whose end mark is in the paragraph.
    pub end: Option<usize>,
}

/// The comments in a paragraph, each with where it stands in the
/// paragraph's text, in order.
pub fn annotations_in(paragraph: &Element) -> Vec<(usize, Annotation)> {
    let marks = marks(paragraph);
    marks
        .iter()
        .filter(|(_, _, note)| note.is(&Ns::Office, "annotation"))
        .map(|(at, _, note)| {
            let end = note.attr(&Ns::Office, "name").and_then(|name| {
                marks
                    .iter()
                    .find(|(_, _, e)| {
                        e.is(&Ns::Office, "annotation-end")
                            && e.attr(&Ns::Office, "name") == Some(name)
                    })
                    .map(|(at, _, _)| *at)
            });
            (*at, read(note, end))
        })
        .collect()
}

/// The comments and end marks in a paragraph: where each stands in its text,
/// its path from the paragraph, and the element.
fn marks(paragraph: &Element) -> Vec<(usize, Vec<usize>, &Element)> {
    segments(paragraph)
        .into_iter()
        .filter(|segment| matches!(segment.kind, Kind::Marker))
        .filter_map(|segment| {
            let element = paragraph.at(&segment.path)?;
            (element.is(&Ns::Office, "annotation") || element.is(&Ns::Office, "annotation-end"))
                .then_some((segment.start, segment.path, element))
        })
        .collect()
}

fn read(note: &Element, end: Option<usize>) -> Annotation {
    let said = |local: &str| {
        note.child(&Ns::Dc, local)
            .map(Element::plain_text)
            .filter(|text| !text.trim().is_empty())
    };
    Annotation {
        author: said("creator"),
        date: said("date"),
        text: note
            .elements()
            .filter(|e| e.is(&Ns::Text, "p"))
            .map(Element::plain_text)
            .collect::<Vec<_>>()
            .join("\n"),
        end,
    }
}

/// The names a comment's author, date and paragraphs are written with, in
/// the prefixes the document declares.
struct Names {
    creator: crate::xml::Name,
    date: crate::xml::Name,
    paragraph: crate::xml::Name,
}

impl Names {
    fn of(content: &Element) -> Self {
        Self {
            creator: content.name_for(&Ns::Dc, "creator"),
            date: content.name_for(&Ns::Dc, "date"),
            paragraph: content.name_for(&Ns::Text, "p"),
        }
    }
}

/// Give a comment its author, date and text, keeping anything else it holds
/// where it stands. A line goes into the paragraph that held the line at its
/// place, which is left as it was when the line is unchanged.
fn write(note: &mut Element, names: &Names, text: &str, author: Option<&str>, date: &str) {
    note.children.retain(
        |n| !matches!(n, Node::Element(e) if e.is(&Ns::Dc, "creator") || e.is(&Ns::Dc, "date")),
    );
    let leaf = |name, text: &str| {
        let mut element = Element {
            name,
            attrs: Vec::new(),
            children: Vec::new(),
            self_closing: text.is_empty(),
        };
        if !text.is_empty() {
            element.children.push(Node::Text(text.to_owned()));
        }
        Node::Element(element)
    };
    let mut head = Vec::new();
    if let Some(author) = author.filter(|a| !a.trim().is_empty()) {
        head.push(leaf(names.creator.clone(), author));
    }
    head.push(leaf(names.date.clone(), date));
    note.children.splice(0..0, head);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut paragraphs: Vec<usize> = note
        .children
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n, Node::Element(e) if e.is(&Ns::Text, "p")))
        .map(|(index, _)| index)
        .collect();
    for &index in paragraphs.iter().skip(lines.len()).rev() {
        note.children.remove(index);
    }
    paragraphs.truncate(lines.len());
    for (&index, line) in paragraphs.iter().zip(&lines) {
        if let Node::Element(p) = &mut note.children[index]
            && p.plain_text() != *line
        {
            p.children.clear();
            p.self_closing = true;
            super::replace(p, 0..0, line);
        }
    }
    let at = paragraphs
        .last()
        .map_or(note.children.len(), |index| index + 1);
    let added: Vec<Node> = lines[paragraphs.len()..]
        .iter()
        .map(|line| {
            let mut written = leaf(names.paragraph.clone(), "");
            if let Node::Element(p) = &mut written {
                super::replace(p, 0..0, line);
            }
            written
        })
        .collect();
    note.children.splice(at..at, added);
    note.self_closing = false;
}

/// The path from a paragraph of its comment at a place among its comments.
fn nth(paragraph: &Element, index: usize) -> Option<Vec<usize>> {
    marks(paragraph)
        .into_iter()
        .filter(|(_, _, e)| e.is(&Ns::Office, "annotation"))
        .nth(index)
        .map(|(_, path, _)| path)
}

/// Put a comment on a range of a paragraph's text, or at a point where the
/// range is empty. `date` is the moment it is written, as ISO 8601.
///
/// # Errors
///
/// The path does not lead to a paragraph, or the content root cannot declare
/// the namespaces a comment is written in.
pub fn add_annotation(
    content: &mut Element,
    path: &[usize],
    range: Range<usize>,
    text: &str,
    author: Option<&str>,
    date: &str,
) -> Result<(), Refused> {
    content
        .at(path)
        .filter(|e| is_paragraph(e))
        .ok_or(Refused::NotFound)?;
    if !content.declare(&Ns::Dc) {
        return Err(Refused::Namespace);
    }
    let mut note = Element {
        name: content.name_for(&Ns::Office, "annotation"),
        attrs: Vec::new(),
        children: Vec::new(),
        self_closing: false,
    };
    let named = (!range.is_empty()).then(|| fresh_name(content));
    if let Some(name) = &named {
        note.set_attr(content.name_for(&Ns::Office, "name"), name.as_str());
    }
    write(&mut note, &Names::of(content), text, author, date);
    let end = named.map(|name| {
        let mut end = Element {
            name: content.name_for(&Ns::Office, "annotation-end"),
            attrs: Vec::new(),
            children: Vec::new(),
            self_closing: true,
        };
        end.set_attr(content.name_for(&Ns::Office, "name"), name);
        end
    });
    let paragraph = content.at_mut(path).ok_or(Refused::NotFound)?;
    if let Some(end) = end {
        insert_mark(paragraph, range.end, end);
    }
    insert_mark(paragraph, range.start, note);
    Ok(())
}

/// A name for a comment's range that no comment in the document has.
fn fresh_name(content: &Element) -> String {
    fn taken(element: &Element, name: &str) -> bool {
        ((element.is(&Ns::Office, "annotation") || element.is(&Ns::Office, "annotation-end"))
            && element.attr(&Ns::Office, "name") == Some(name))
            || element.elements().any(|child| taken(child, name))
    }
    let mut number = 1;
    while taken(content, &format!("__Annotation__{number}")) {
        number += 1;
    }
    format!("__Annotation__{number}")
}

/// Change the text of a paragraph's comment, by its place among the
/// paragraph's comments, taking the new author and date with it.
///
/// # Errors
///
/// There is no such comment.
pub fn set_annotation_text(
    content: &mut Element,
    path: &[usize],
    index: usize,
    text: &str,
    author: Option<&str>,
    date: &str,
) -> Result<(), Refused> {
    let paragraph = content.at(path).ok_or(Refused::NotFound)?;
    let within = nth(paragraph, index).ok_or(Refused::NotFound)?;
    let names = Names::of(content);
    let note = content
        .at_mut(&[path, within.as_slice()].concat())
        .ok_or(Refused::NotFound)?;
    write(note, &names, text, author, date);
    Ok(())
}

/// Take a paragraph's comment out, by its place among the paragraph's
/// comments, and its end mark with it.
///
/// # Errors
///
/// There is no such comment.
pub fn remove_annotation(
    content: &mut Element,
    path: &[usize],
    index: usize,
) -> Result<(), Refused> {
    let paragraph = content.at(path).ok_or(Refused::NotFound)?;
    let within = nth(paragraph, index).ok_or(Refused::NotFound)?;
    let name = paragraph
        .at(&within)
        .and_then(|note| note.attr(&Ns::Office, "name"))
        .map(ToOwned::to_owned);
    let end = name.and_then(|name| {
        find(content, &|e: &Element| {
            e.is(&Ns::Office, "annotation-end") && e.attr(&Ns::Office, "name") == Some(&name)
        })
    });
    let mut gone = vec![[path, within.as_slice()].concat()];
    gone.extend(end);
    gone.sort();
    for removed in gone.iter().rev() {
        take_out(content, removed)?;
    }
    Ok(())
}

/// The path of the first element under a root that answers to a test.
fn find(root: &Element, test: &dyn Fn(&Element) -> bool) -> Option<Vec<usize>> {
    for (index, child) in root.elements_indexed() {
        if test(child) {
            return Some(vec![index]);
        }
        if let Some(rest) = find(child, test) {
            return Some([vec![index], rest].concat());
        }
    }
    None
}

/// Take out the element at a path, joining the text either side of it again.
pub(super) fn take_out(content: &mut Element, path: &[usize]) -> Result<(), Refused> {
    let (&last, parent) = path.split_last().ok_or(Refused::NotFound)?;
    let parent = content.at_mut(parent).ok_or(Refused::NotFound)?;
    if last >= parent.children.len() {
        return Err(Refused::NotFound);
    }
    parent.children.remove(last);
    if last > 0
        && let (Some(Node::Text(_)), Some(Node::Text(after))) = (
            parent.children.get(last - 1),
            parent.children.get(last).cloned(),
        )
    {
        parent.children.remove(last);
        if let Some(Node::Text(before)) = parent.children.get_mut(last - 1) {
            before.push_str(&after);
        }
    }
    parent.self_closing = parent.children.is_empty();
    Ok(())
}

/// Put an element in a paragraph at a place in its text, splitting the text
/// node it falls in and nothing else, so taking it out again gives back the
/// paragraph it was. Answers where it went, from the paragraph.
pub(super) fn insert_mark(paragraph: &mut Element, at: usize, mark: Element) -> Vec<usize> {
    let found = segments(paragraph)
        .into_iter()
        .find(|segment| segment.start + segment.kind.len() > at || segment.start >= at);
    let Some(segment) = found else {
        paragraph.children.push(Node::Element(mark));
        paragraph.self_closing = false;
        return vec![paragraph.children.len() - 1];
    };
    let (&index, above) = segment.path.split_last().unwrap_or((&0, &[]));
    let Some(parent) = paragraph.at_mut(above) else {
        return Vec::new();
    };
    let offset = at.saturating_sub(segment.start);
    let placed = match &mut parent.children[index] {
        Node::Text(text) if offset > 0 => {
            let byte = text
                .char_indices()
                .nth(offset)
                .map_or(text.len(), |(b, _)| b);
            let rest = text.split_off(byte);
            parent.children.insert(index + 1, Node::Element(mark));
            parent.children.insert(index + 2, Node::Text(rest));
            index + 1
        }
        Node::Element(spaces) if offset > 0 && matches!(segment.kind, Kind::Spaces(_)) => {
            let count = match segment.kind {
                Kind::Spaces(count) => count,
                _ => 0,
            };
            let mut rest = spaces.clone();
            set_count(spaces, offset);
            set_count(&mut rest, count - offset);
            parent.children.insert(index + 1, Node::Element(mark));
            parent.children.insert(index + 2, Node::Element(rest));
            index + 1
        }
        _ => {
            parent.children.insert(index, Node::Element(mark));
            index
        }
    };
    [above, &[placed]].concat()
}

/// The comments on a slide: each one's place among the page's children,
/// where it is on the slide in points from the top left, and what it says.
pub fn slide_annotations(page: &Element) -> Vec<(usize, (f32, f32), Annotation)> {
    page.elements_indexed()
        .filter(|(_, e)| e.is(&Ns::Officeooo, "annotation"))
        .map(|(index, note)| {
            let at = |local: &str| {
                note.attr(&Ns::Svg, local)
                    .and_then(Length::parse)
                    .map_or(0.0, Length::points)
            };
            (index, (at("x"), at("y")), read(note, None))
        })
        .collect()
}

/// Put a comment on a slide at a place in points, and answer its place among
/// the page's children.
///
/// # Errors
///
/// The path does not lead to a page, or the namespaces a comment is written
/// in cannot be declared.
pub fn add_slide_annotation(
    content: &mut Element,
    page: &[usize],
    at: (f32, f32),
    text: &str,
    author: Option<&str>,
    date: &str,
) -> Result<usize, Refused> {
    content
        .at(page)
        .filter(|e| e.is(&Ns::Draw, "page"))
        .ok_or(Refused::NotFound)?;
    if !content.declare(&Ns::Dc) || !content.declare(&Ns::Officeooo) || !content.declare(&Ns::Svg) {
        return Err(Refused::Namespace);
    }
    let mut note = Element {
        name: content.name_for(&Ns::Officeooo, "annotation"),
        attrs: Vec::new(),
        children: Vec::new(),
        self_closing: false,
    };
    let cm = |points: f32| format!("{:.3}cm", points / 72.0 * 2.54);
    for (local, value) in [("x", cm(at.0)), ("y", cm(at.1))] {
        note.set_attr(content.name_for(&Ns::Svg, local), value);
    }
    write(&mut note, &Names::of(content), text, author, date);
    let element = content.at_mut(page).ok_or(Refused::NotFound)?;
    element.children.push(Node::Element(note));
    element.self_closing = false;
    Ok(element.children.len() - 1)
}

/// A slide's comment, by its place among the page's children.
fn slide_note<'a>(
    content: &'a mut Element,
    page: &[usize],
    child: usize,
) -> Result<&'a mut Element, Refused> {
    content
        .at_mut(&[page, &[child]].concat())
        .filter(|e| e.is(&Ns::Officeooo, "annotation"))
        .ok_or(Refused::NotFound)
}

/// Change the text of a slide's comment, by its place among the page's
/// children, taking the new author and date with it.
///
/// # Errors
///
/// There is no comment there.
pub fn set_slide_annotation_text(
    content: &mut Element,
    page: &[usize],
    child: usize,
    text: &str,
    author: Option<&str>,
    date: &str,
) -> Result<(), Refused> {
    let names = Names::of(content);
    write(
        slide_note(content, page, child)?,
        &names,
        text,
        author,
        date,
    );
    Ok(())
}

/// Take a slide's comment out, by its place among the page's children.
///
/// # Errors
///
/// There is no comment there.
pub fn remove_slide_annotation(
    content: &mut Element,
    page: &[usize],
    child: usize,
) -> Result<(), Refused> {
    slide_note(content, page, child)?;
    let element = content.at_mut(page).ok_or(Refused::NotFound)?;
    element.children.remove(child);
    element.self_closing = element.children.is_empty();
    Ok(())
}
