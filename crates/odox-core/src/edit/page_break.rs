//! A page break by hand: `fo:break-before="page"` on a paragraph's automatic
//! style, written as an alignment is (`edit::set_alignment`). DESIGN.md §11.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::align::restyle_paragraphs;
use super::block::declares;
use super::{Refused, is_paragraph, split_at};
use crate::style::{Break, Family, Styles};
use crate::xml::{Element, Ns};

/// Whether a page begins at a paragraph.
pub fn page_break(content: &Element, path: &[usize], styles: &Styles) -> bool {
    content
        .at(path)
        .filter(|e| is_paragraph(e))
        .and_then(|paragraph| paragraph.attr(&Ns::Text, "style-name"))
        .is_some_and(|name| {
            styles
                .resolve(&Family::Paragraph, name)
                .paragraph
                .break_before
                == Some(Break::Page)
        })
}

/// Begin a page at the caret: split the paragraph there and begin the page
/// at its second half, or, with the caret at the paragraph's start, at the
/// paragraph itself. Answers the path of the paragraph the page begins at.
///
/// # Errors
///
/// As `edit::split_at` and `edit::set_alignment`, and a paragraph in a table
/// cell, whose row a page cannot begin inside, is refused.
pub fn insert_page_break(
    content: &mut Element,
    path: &[usize],
    at: usize,
    styles: &mut Styles,
) -> Result<Vec<usize>, Refused> {
    content
        .at(path)
        .filter(|e| is_paragraph(e))
        .ok_or(Refused::NotFound)?;
    declares(content, &[Ns::Text, Ns::Style, Ns::Fo])?;
    if (0..path.len()).any(|depth| {
        content
            .at(&path[..depth])
            .is_some_and(|e| e.is(&Ns::Table, "table-cell"))
    }) {
        return Err(Refused::Structure);
    }
    let begins = if at == 0 {
        path.to_vec()
    } else {
        split_at(content, path, at)?
    };
    let had = content.child(&Ns::Office, "automatic-styles").is_some();
    restyle_paragraphs(
        content,
        std::slice::from_ref(&begins),
        styles,
        |content, path, styles| {
            (!page_break(content, path, styles))
                .then(|| vec![(Ns::Fo, "break-before", Some("page".to_owned()))])
        },
    )?;
    let mut begins = begins;
    if !had
        && content.child(&Ns::Office, "automatic-styles").is_some()
        && let Some(first) = begins.first_mut()
    {
        *first += 1;
    }
    Ok(begins)
}

/// Take the page break off a paragraph: out of its automatic style, and as
/// `auto` where the style it inherits from breaks the page.
///
/// # Errors
///
/// As `edit::set_alignment`.
pub fn remove_page_break(
    content: &mut Element,
    path: &[usize],
    styles: &mut Styles,
) -> Result<(), Refused> {
    restyle_paragraphs(
        content,
        &[path.to_vec()],
        styles,
        |content, path, styles| {
            page_break(content, path, styles).then(|| {
                let value = inherits_a_break(content, path, styles).then(|| "auto".to_owned());
                vec![(Ns::Fo, "break-before", value)]
            })
        },
    )
}

/// Whether the style a paragraph's own automatic style inherits from, or the
/// named style it has, breaks the page: whether taking the break out of its
/// automatic style would leave one.
fn inherits_a_break(content: &Element, path: &[usize], styles: &Styles) -> bool {
    let Some(name) = content
        .at(path)
        .and_then(|paragraph| paragraph.attr(&Ns::Text, "style-name"))
    else {
        return false;
    };
    let own = content
        .child(&Ns::Office, "automatic-styles")
        .into_iter()
        .flat_map(Element::elements)
        .find(|style| {
            style.attr(&Ns::Style, "family") == Some("paragraph")
                && style.attr(&Ns::Style, "name") == Some(name)
        });
    let inherited = match own {
        Some(style) => style.attr(&Ns::Style, "parent-style-name"),
        None => Some(name),
    };
    inherited.is_some_and(|parent| {
        styles
            .resolve(&Family::Paragraph, parent)
            .paragraph
            .break_before
            == Some(Break::Page)
    })
}
