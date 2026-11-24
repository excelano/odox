//! Hyperlinks: a `text:a` with `xlink:type="simple"` and `xlink:href`, made
//! over a range of one paragraph, changed, or taken off. DESIGN.md §11.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::block::declares;
use super::{Refused, cut, is_paragraph, links, merge_text, text};
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
    let mut head = paragraph.clone();
    cut(&mut head, start..length, |at| at > start);
    let mut middle = paragraph.clone();
    cut(&mut middle, end..length, |at| at >= end);
    cut(&mut middle, 0..start, |at| at <= start);
    let mut tail = paragraph.clone();
    cut(&mut tail, 0..end, |at| at < end);
    for part in [&mut head, &mut middle, &mut tail] {
        merge_text(part);
    }
    link.children = middle.children;
    link.self_closing = link.children.is_empty();
    let at = head.children.len();
    paragraph.children = head.children;
    paragraph.children.push(Node::Element(link));
    paragraph.children.extend(tail.children);
    paragraph.self_closing = false;
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
    let (from, to, with) = difference(&text(element), shown);
    if from != to || !with.is_empty() {
        super::replace(element, from..to, &with);
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
    merge_text(parent);
    Ok(())
}

/// The link at a place in a paragraph's text, from its start to its end
/// inclusive: its path from the paragraph, the characters it covers and its
/// address.
pub fn link_at(paragraph: &Element, at: usize) -> Option<(Vec<usize>, Range<usize>, String)> {
    type Found = (Vec<usize>, Range<usize>, String);
    fn walk(
        parent: &Element,
        start: &mut usize,
        at: usize,
        path: &mut Vec<usize>,
        found: &mut Vec<Found>,
    ) {
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
                    found.push((path.clone(), from..*start, href.to_owned()));
                }
            } else if super::is_inline_container(element) {
                walk(element, start, at, path, found);
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
    }
    let mut found = Vec::new();
    walk(paragraph, &mut 0, at, &mut Vec::new(), &mut found);
    let inside = found.iter().position(|(_, range, _)| at < range.end);
    let chosen = inside.or_else(|| (!found.is_empty()).then_some(0))?;
    Some(found.swap_remove(chosen))
}

/// Where two texts differ: the characters of the first, from and to, that
/// the second puts other text in place of, and that text. Characters both
/// begin and end with are left out.
pub fn difference(old: &str, new: &str) -> (usize, usize, String) {
    let (old, new): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
    let before = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let after = old[before..]
        .iter()
        .rev()
        .zip(new[before..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    (
        before,
        old.len() - after,
        new[before..new.len() - after].iter().collect(),
    )
}
