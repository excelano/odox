//! Footnotes and endnotes in a text document: put in at the caret, numbered
//! in document order, and their text changed. DESIGN.md §11.
//!
//! A note is a `text:note` at the place it is cited, holding its citation and
//! its body, as `LibreOffice` writes one. A citation with a `text:label` of
//! its own keeps it; every other is its place among the notes of its class.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::annotation::insert_mark;
use super::{Refused, is_paragraph};
use crate::style::{Family, Styles};
use crate::xml::{Element, Node, Ns};

/// The two kinds of note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteClass {
    /// At the foot of the page it is cited on.
    Footnote,
    /// At the end of the document.
    Endnote,
}

/// Put an empty note of a class at a place in a paragraph's text, number the
/// notes again, and answer the path of the note's body paragraph.
///
/// # Errors
///
/// The path does not lead to a paragraph.
pub fn insert_note(
    content: &mut Element,
    path: &[usize],
    at: usize,
    class: NoteClass,
    styles: &Styles,
) -> Result<Vec<usize>, Refused> {
    content
        .at(path)
        .filter(|e| is_paragraph(e))
        .ok_or(Refused::NotFound)?;
    let (word, style, prefix) = match class {
        NoteClass::Footnote => ("footnote", "Footnote", "ftn"),
        NoteClass::Endnote => ("endnote", "Endnote", "edn"),
    };
    let make = |local: &str| Element {
        name: content.name_for(&Ns::Text, local),
        attrs: Vec::new(),
        children: Vec::new(),
        self_closing: false,
    };
    let mut paragraph = make("p");
    paragraph.self_closing = true;
    if styles.has_style(&Family::Paragraph, style) {
        paragraph.set_attr(content.name_for(&Ns::Text, "style-name"), style);
    }
    let mut body = make("note-body");
    body.children.push(Node::Element(paragraph));
    let mut note = make("note");
    note.set_attr(content.name_for(&Ns::Text, "id"), fresh_id(content, prefix));
    note.set_attr(content.name_for(&Ns::Text, "note-class"), word);
    note.children.push(Node::Element(make("note-citation")));
    note.children.push(Node::Element(body));
    let paragraph = content.at_mut(path).ok_or(Refused::NotFound)?;
    let placed = insert_mark(paragraph, at, note);
    renumber_notes(content);
    let note_path = [path, placed.as_slice()].concat();
    let body_at = content
        .at(&note_path)
        .and_then(|n| {
            n.elements_indexed()
                .find(|(_, e)| e.is(&Ns::Text, "note-body"))
        })
        .map(|(index, _)| index)
        .ok_or(Refused::NotFound)?;
    Ok([note_path.as_slice(), &[body_at, 0]].concat())
}

/// An id for a note no note in the document has.
fn fresh_id(content: &Element, prefix: &str) -> String {
    fn taken(element: &Element, id: &str) -> bool {
        element.attr(&Ns::Text, "id") == Some(id) || element.elements().any(|e| taken(e, id))
    }
    let mut number = 1;
    while taken(content, &format!("{prefix}{number}")) {
        number += 1;
    }
    format!("{prefix}{number}")
}

/// Number every note's citation by its place among the notes of its class in
/// document order, leaving a citation that has a label of its own as it is.
pub fn renumber_notes(content: &mut Element) {
    fn walk(element: &mut Element, counts: &mut [usize; 2]) {
        for child in &mut element.children {
            let Node::Element(child) = child else {
                continue;
            };
            if child.is(&Ns::Text, "note") {
                let class = usize::from(child.attr(&Ns::Text, "note-class") == Some("endnote"));
                counts[class] += 1;
                let number = counts[class].to_string();
                if let Some(citation) = child
                    .children
                    .iter_mut()
                    .find_map(|n| match n {
                        Node::Element(e) if e.is(&Ns::Text, "note-citation") => Some(e),
                        _ => None,
                    })
                    .filter(|c| c.attr(&Ns::Text, "label").is_none())
                {
                    citation.children = vec![Node::Text(number)];
                    citation.self_closing = false;
                }
                continue;
            }
            walk(child, counts);
        }
    }
    walk(content, &mut [0, 0]);
}

/// The text of a note, a line to a paragraph of its body.
pub fn note_text(note: &Element) -> String {
    note.child(&Ns::Text, "note-body")
        .into_iter()
        .flat_map(Element::elements)
        .filter(|e| is_paragraph(e))
        .map(Element::plain_text)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Change the text of a note's body, a line to a paragraph, the paragraphs
/// keeping the style of its first.
///
/// # Errors
///
/// The path does not lead to a note.
pub fn set_note_text(content: &mut Element, note: &[usize], text: &str) -> Result<(), Refused> {
    let element = content
        .at_mut(note)
        .filter(|e| e.is(&Ns::Text, "note"))
        .ok_or(Refused::NotFound)?;
    let body = element
        .children
        .iter_mut()
        .find_map(|n| match n {
            Node::Element(e) if e.is(&Ns::Text, "note-body") => Some(e),
            _ => None,
        })
        .ok_or(Refused::NotFound)?;
    let mut model = body
        .elements()
        .find(|e| is_paragraph(e))
        .cloned()
        .unwrap_or_else(|| Element {
            name: body.name_for(&Ns::Text, "p"),
            attrs: Vec::new(),
            children: Vec::new(),
            self_closing: true,
        });
    model.children.clear();
    model.self_closing = true;
    body.children
        .retain(|n| !matches!(n, Node::Element(e) if is_paragraph(e)));
    for line in text.split('\n') {
        let mut paragraph = model.clone();
        super::replace(&mut paragraph, 0..0, line);
        body.children.push(Node::Element(paragraph));
    }
    body.self_closing = false;
    Ok(())
}

/// The notes in a paragraph, each with where it stands in the paragraph's
/// text and its path from the paragraph.
pub fn notes_in(paragraph: &Element) -> Vec<(usize, Vec<usize>)> {
    super::segments(paragraph)
        .into_iter()
        .filter(|segment| {
            matches!(segment.kind, super::Kind::Marker)
                && paragraph
                    .at(&segment.path)
                    .is_some_and(|e| e.is(&Ns::Text, "note"))
        })
        .map(|segment| (segment.start, segment.path))
        .collect()
}

/// Take a note out, and number the notes again.
///
/// # Errors
///
/// The path does not lead to a note.
pub fn remove_note(content: &mut Element, note: &[usize]) -> Result<(), Refused> {
    content
        .at(note)
        .filter(|e| e.is(&Ns::Text, "note"))
        .ok_or(Refused::NotFound)?;
    super::annotation::take_out(content, note)?;
    renumber_notes(content);
    Ok(())
}
