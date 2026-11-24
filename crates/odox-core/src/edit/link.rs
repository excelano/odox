//! Hyperlinks: a `text:a` with `xlink:type="simple"` and `xlink:href`, made
//! over a range of one paragraph, changed, or taken off. DESIGN.md §11.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::block::declares;
use super::{Refused, is_paragraph, links, normalize, split, text};
use std::ops::Range;

use crate::xml::{Element, Node, Ns};

/// Whether an address is one a link here may point to and a window follows:
/// a web page, a mail address, or a place in the document.
pub fn followable(href: &str) -> bool {
    if let Some(place) = href.strip_prefix('#') {
        return !place.is_empty();
    }
    href.split_once(':').is_some_and(|(scheme, rest)| {
        !rest.is_empty()
            && ["http", "https", "mailto"]
                .iter()
                .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
    })
}

/// Make the range of a paragraph from one place to another a link.
/// Answers the path of the link.
///
/// # Errors
///
/// The two places are not in one paragraph, or the range is empty, reaches
/// into a link or holds one ([`Refused::Structure`]), or the address is not
/// followable ([`Refused::Address`]).
pub fn make_link(
    content: &mut Element,
    from: (&[usize], usize),
    to: (&[usize], usize),
    href: &str,
) -> Result<Vec<usize>, Refused> {
    if from.0 != to.0 {
        return Err(Refused::Structure);
    }
    if !followable(href) {
        return Err(Refused::Address);
    }
    declares(content, &[Ns::Text, Ns::Xlink])?;
    let path = from.0;
    let (start, end) = (from.1.min(to.1), from.1.max(to.1));
    let paragraph = content
        .at(path)
        .filter(|e| is_paragraph(e))
        .ok_or(Refused::NotFound)?;
    let length = text(paragraph).chars().count();
    if start == end || end > length {
        return Err(Refused::Structure);
    }
    if links(paragraph)
        .iter()
        .any(|(range, _)| range.start < end && start < range.end)
    {
        return Err(Refused::Structure);
    }
    let mut link = Element::new("", "a", Ns::Text);
    link.name = content.name_for(&Ns::Text, "a");
    link.set_attr(content.name_for(&Ns::Xlink, "type"), "simple");
    link.set_attr(content.name_for(&Ns::Xlink, "href"), href);
    let paragraph = content.at_mut(path).ok_or(Refused::NotFound)?;
    let (mut head, rest) = split(paragraph, start);
    let (middle, tail) = split(&rest, end - start);
    link.children = middle.children;
    link.self_closing = link.children.is_empty();
    head.children.push(Node::Element(link));
    head.children.extend(tail.children);
    head.self_closing = false;
    normalize(&mut head);
    let at = head
        .children
        .iter()
        .enumerate()
        .position(|(index, child)| {
            matches!(child, Node::Element(e) if e.is(&Ns::Text, "a"))
                && text(&Element {
                    children: head.children[..index].to_vec(),
                    ..head.clone()
                })
                .chars()
                .count()
                    == start
        })
        .ok_or(Refused::NotFound)?;
    *paragraph = head;
    Ok([path, &[at]].concat())
}

/// Point a link somewhere else and give it other text; text the same as it
/// had keeps its formatting.
///
/// # Errors
///
/// The path does not lead to a link, or the address is not followable.
pub fn set_link(
    content: &mut Element,
    link: &[usize],
    href: &str,
    shown: &str,
) -> Result<(), Refused> {
    if !followable(href) {
        return Err(Refused::Address);
    }
    let name = content.name_for(&Ns::Xlink, "href");
    let element = content
        .at_mut(link)
        .filter(|e| e.is(&Ns::Text, "a"))
        .ok_or(Refused::NotFound)?;
    element.set_attr(name, href);
    if text(element) != shown {
        let length = text(element).chars().count();
        super::replace(element, 0..length, shown);
    }
    Ok(())
}

/// Take a link off, its text and formatting staying where they are.
///
/// # Errors
///
/// The path does not lead to a link.
pub fn remove_link(content: &mut Element, link: &[usize]) -> Result<(), Refused> {
    let (&index, parent_path) = link.split_last().ok_or(Refused::NotFound)?;
    let parent = content.at_mut(parent_path).ok_or(Refused::NotFound)?;
    let Some(Node::Element(element)) = parent.children.get(index) else {
        return Err(Refused::NotFound);
    };
    if !element.is(&Ns::Text, "a") {
        return Err(Refused::NotFound);
    }
    let inner = element.children.clone();
    parent.children.splice(index..=index, inner);
    normalize(parent);
    Ok(())
}

/// The link at a place in a paragraph's text, from its start to its end
/// inclusive: its path from the paragraph, the characters it covers and its
/// address.
pub fn link_at(paragraph: &Element, at: usize) -> Option<(Vec<usize>, Range<usize>, String)> {
    fn walk(
        parent: &Element,
        start: &mut usize,
        at: usize,
        path: &mut Vec<usize>,
    ) -> Option<(Vec<usize>, Range<usize>, String)> {
        for (index, child) in parent.children.iter().enumerate() {
            let Node::Element(element) = child else {
                if let Node::Text(t) | Node::CData(t) = child {
                    *start += t.chars().count();
                }
                continue;
            };
            let from = *start;
            path.push(index);
            if element.is(&Ns::Text, "a") {
                *start += text(element).chars().count();
                if (from..=*start).contains(&at)
                    && let Some(href) = element.attr(&Ns::Xlink, "href")
                {
                    return Some((path.clone(), from..*start, href.to_owned()));
                }
            } else if super::is_inline_container(element) {
                if let Some(found) = walk(element, start, at, path) {
                    return Some(found);
                }
            } else {
                *start += text(&Element {
                    children: vec![child.clone()],
                    ..parent.clone()
                })
                .chars()
                .count();
            }
            path.pop();
        }
        None
    }
    walk(paragraph, &mut 0, at, &mut Vec::new())
}
