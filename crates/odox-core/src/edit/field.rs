//! Fields put in at the caret: a date, fixed or following today, the page
//! number and the page count, as `LibreOffice` writes them. DESIGN.md §11.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::block::declares;
use super::{Refused, insert_inline, is_paragraph};
use crate::xml::{Element, Node, Ns};

/// A field to put in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// Today's date, kept as it is or following the day the document is
    /// saved.
    Date {
        /// Whether it keeps the day it was put in.
        fixed: bool,
    },
    /// The number of the page it is on.
    PageNumber,
    /// How many pages there are.
    PageCount,
}

/// Put a field in a paragraph at a place in its text.
///
/// # Errors
///
/// The path does not lead to a paragraph.
pub fn insert_field(
    content: &mut Element,
    path: &[usize],
    at: usize,
    field: Field,
    today: (i64, i64, i64),
) -> Result<(), Refused> {
    content
        .at(path)
        .filter(|e| is_paragraph(e))
        .ok_or(Refused::NotFound)?;
    declares(content, &[Ns::Text])?;
    let name = |local: &str| content.name_for(&Ns::Text, local);
    let element = match field {
        Field::Date { fixed } => {
            let iso = iso(today);
            let mut date = Element::new("", "date", Ns::Text);
            date.name = name("date");
            date.set_attr(name("date-value"), iso.clone());
            if fixed {
                date.set_attr(name("fixed"), "true");
            }
            date.children = vec![Node::Text(iso)];
            date
        }
        Field::PageNumber => {
            let mut number = Element::new("", "page-number", Ns::Text);
            number.name = name("page-number");
            number.set_attr(name("select-page"), "current");
            number.children = vec![Node::Text("1".to_owned())];
            number
        }
        Field::PageCount => {
            let mut count = Element::new("", "page-count", Ns::Text);
            count.name = name("page-count");
            count.children = vec![Node::Text("1".to_owned())];
            count
        }
    };
    let mut element = element;
    element.self_closing = false;
    let paragraph = content.at_mut(path).ok_or(Refused::NotFound)?;
    insert_inline(paragraph, at, &[Node::Element(element)]);
    Ok(())
}

/// A day as ISO 8601 writes it.
fn iso((year, month, day): (i64, i64, i64)) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

/// Write today into every date that follows the day, answering whether any
/// changed.
pub fn follow_dates(content: &mut Element, today: (i64, i64, i64)) -> bool {
    let iso = iso(today);
    let mut changed = false;
    let value = content.name_for(&Ns::Text, "date-value");
    walk(content, &mut |date| {
        if date.attr(&Ns::Text, "fixed") == Some("true")
            || date.attr(&Ns::Text, "date-value") == Some(iso.as_str())
        {
            return;
        }
        date.set_attr(value.clone(), iso.clone());
        date.children = vec![Node::Text(iso.clone())];
        date.self_closing = false;
        changed = true;
    });
    changed
}

/// Every `text:date` under an element.
fn walk(element: &mut Element, each: &mut impl FnMut(&mut Element)) {
    for child in &mut element.children {
        if let Node::Element(child) = child {
            if child.is(&Ns::Text, "date") {
                each(child);
            } else {
                walk(child, each);
            }
        }
    }
}
