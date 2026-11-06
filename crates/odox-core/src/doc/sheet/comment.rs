//! A cell's comment: the `office:annotation` ODF keeps inside the cell, with
//! who wrote it, when, and its paragraphs. DESIGN.md §11.
//!
//! A comment written here carries its author, its date and its text, and
//! nothing about the box an application draws it in, which `LibreOffice` places
//! itself when the box has no place given. A comment changed here keeps its box
//! and whether it is shown, and takes the new author and date with its text.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::{Cell, SheetDocument};
use crate::edit::Refused;
use crate::xml::{Element, Node, Ns};

/// A comment on a cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// Who wrote it, where the document says.
    pub author: Option<String>,
    /// When, as ISO 8601, where the document says.
    pub date: Option<String>,
    /// Its text, a line to a paragraph.
    pub text: String,
}

impl Cell<'_> {
    /// The cell's comment, if it has one.
    pub fn comment(&self) -> Option<Comment> {
        let note = self.element.child(&Ns::Office, "annotation")?;
        let said = |local: &str| {
            note.child(&Ns::Dc, local)
                .map(Element::plain_text)
                .filter(|text| !text.trim().is_empty())
        };
        let text = note
            .elements()
            .filter(|e| e.is(&Ns::Text, "p"))
            .map(Element::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        Some(Comment {
            author: said("creator"),
            date: said("date"),
            text,
        })
    }
}

impl SheetDocument {
    /// Give a cell a comment, change the one it has, or with `None` take it
    /// away. `date` is the moment it is written, as ISO 8601.
    ///
    /// # Errors
    ///
    /// The sheet does not exist, the cell is under a neighbour's span, or the
    /// document cannot declare the namespace an author is written in. Nothing
    /// is changed in any case.
    pub fn set_comment(
        &mut self,
        sheet: usize,
        row: usize,
        column: usize,
        text: Option<&str>,
        author: Option<&str>,
        date: &str,
    ) -> Result<(), Refused> {
        self.can_edit(sheet, row, column)?;
        if text.is_some() && !self.document.content.declare(&Ns::Dc) {
            return Err(Refused::Namespace);
        }
        let name = |local: &str, ns: Ns| self.document.name(&ns, local);
        let (note_name, creator, dated, paragraph) = (
            name("annotation", Ns::Office),
            name("creator", Ns::Dc),
            name("date", Ns::Dc),
            name("p", Ns::Text),
        );
        let cell = self.cell_element(sheet, row, column)?;
        let at = cell
            .children
            .iter()
            .position(|n| matches!(n, Node::Element(e) if e.is(&Ns::Office, "annotation")));
        let Some(text) = text else {
            if let Some(at) = at {
                cell.children.remove(at);
                cell.self_closing = cell.children.is_empty();
            }
            self.reindex();
            return Ok(());
        };
        let mut note = match at {
            Some(at) => match cell.children.remove(at) {
                Node::Element(note) => note,
                _ => unreachable!("found as an element"),
            },
            None => Element {
                name: note_name,
                attrs: Vec::new(),
                children: Vec::new(),
                self_closing: false,
            },
        };
        note.children.retain(|n| {
            !matches!(n, Node::Element(e)
                if e.is(&Ns::Dc, "creator") || e.is(&Ns::Dc, "date") || e.is(&Ns::Text, "p"))
        });
        let leaf = |name, text: &str| {
            let mut element = Element {
                name,
                attrs: Vec::new(),
                children: Vec::new(),
                self_closing: true,
            };
            if !text.is_empty() {
                element.children.push(Node::Text(text.to_owned()));
                element.self_closing = false;
            }
            Node::Element(element)
        };
        let mut head = Vec::new();
        if let Some(author) = author.filter(|a| !a.trim().is_empty()) {
            head.push(leaf(creator, author));
        }
        head.push(leaf(dated, date));
        note.children.splice(0..0, head);
        for line in text.split('\n') {
            let mut written = leaf(paragraph.clone(), "");
            if let Node::Element(p) = &mut written {
                crate::edit::replace(p, 0..0, line);
            }
            note.children.push(written);
        }
        note.self_closing = false;
        cell.children.insert(0, Node::Element(note));
        cell.self_closing = false;
        self.reindex();
        Ok(())
    }

    /// A cell's comment as it stands in the tree, for moving it whole.
    pub(super) fn note(&self, sheet: usize, row: usize, column: usize) -> Option<Element> {
        let index = self.sheets.get(sheet)?;
        self.cell(index, row, column)?
            .element
            .child(&Ns::Office, "annotation")
            .cloned()
    }

    /// Put a comment, moved whole from another cell, in place of the one a
    /// cell has, or take the cell's away.
    pub(super) fn put_note(
        &mut self,
        sheet: usize,
        row: usize,
        column: usize,
        note: Option<Element>,
    ) -> Result<(), Refused> {
        let cell = self.cell_element(sheet, row, column)?;
        cell.children
            .retain(|n| !matches!(n, Node::Element(e) if e.is(&Ns::Office, "annotation")));
        if let Some(note) = note {
            cell.children.insert(0, Node::Element(note));
        }
        cell.self_closing = cell.children.is_empty();
        self.touched();
        Ok(())
    }
}
