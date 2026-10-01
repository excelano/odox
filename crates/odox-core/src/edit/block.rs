//! What a paragraph is: body text or a heading of some level, and whether it
//! is an item of a list. DESIGN.md §11.
//!
//! A heading is a `text:h` with an outline level and the paragraph style the
//! document gives headings of that level, and a document that has none is
//! given an automatic one in `content.xml` that says only size and weight.
//! Taking the heading off gives back a `text:p` in the style most of the
//! document's paragraphs have. A list is a `text:list` of `text:list-item`s
//! each holding a paragraph; a run of paragraphs side by side becomes one
//! list, so that numbers run on, and taking the list off stands each
//! paragraph where its item was.
//!
//! None of it adds or removes a paragraph, so a paragraph's place in the
//! order the paragraphs are drawn in is the same after, which is how a caret
//! finds it again.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::BTreeMap;

use super::format::{automatic_styles, element, unnamed};
use super::{Refused, is_paragraph, take_out_of_list};
use crate::style::{Family, ListKind, Styles};
use crate::xml::{Element, Node, Ns};

/// A paragraph's heading level, if it is a heading.
pub fn heading_level(paragraph: &Element) -> Option<u8> {
    paragraph.is(&Ns::Text, "h").then(|| {
        paragraph
            .attr_usize(&Ns::Text, "outline-level")
            .and_then(|level| u8::try_from(level).ok())
            .unwrap_or(1)
    })
}

/// A paragraph's heading level and the kind of list it is an item of, for a
/// button to light. A paragraph in a list whose style the document does not
/// give is taken for a bulleted one, which is how it is drawn.
pub fn block_state(
    content: &Element,
    path: &[usize],
    styles: &Styles,
) -> (Option<u8>, Option<ListKind>) {
    let Some(paragraph) = content.at(path).filter(|e| is_paragraph(e)) else {
        return (None, None);
    };
    (
        heading_level(paragraph),
        list_kind_of(content, path, styles),
    )
}

/// Whether an element is what holds a paragraph in a list.
fn is_item(element: &Element) -> bool {
    element.is(&Ns::Text, "list-item") || element.is(&Ns::Text, "list-header")
}

/// The kind of list a paragraph is an item of, by the nearest list above it
/// that names a style.
fn list_kind_of(content: &Element, path: &[usize], styles: &Styles) -> Option<ListKind> {
    let (_, item_path) = path.split_last()?;
    if !is_item(content.at(item_path)?) {
        return None;
    }
    let mut at = item_path;
    while let Some((_, above)) = at.split_last() {
        at = above;
        let Some(list) = content.at(at).filter(|e| e.is(&Ns::Text, "list")) else {
            continue;
        };
        if let Some(name) = list.attr(&Ns::Text, "style-name") {
            return Some(styles.list_kind(name).unwrap_or(ListKind::Bullet));
        }
    }
    Some(ListKind::Bullet)
}

/// What the namespaces an edit writes in have to be declared.
fn declares(content: &Element, spaces: &[Ns]) -> Result<(), Refused> {
    spaces
        .iter()
        .all(|space| content.declares(space))
        .then_some(())
        .ok_or(Refused::Namespace)
}

/// Make a paragraph a heading of a level, or body text again.
///
/// A paragraph that is already what is asked for is left alone. The
/// paragraph's own style is replaced, which is what applying a paragraph style
/// is; its text, its spans and everything in it stay.
///
/// A document with no `office:automatic-styles` is given one before its body
/// when a heading style has to be written, which moves the body one place
/// along: a path held from before the edit is to be taken again.
///
/// # Errors
///
/// The path does not lead to a paragraph, or the content root does not
/// declare a namespace the edit writes in. Each is refused before anything
/// changes.
pub fn set_heading(
    content: &mut Element,
    path: &[usize],
    level: Option<u8>,
    styles: &mut Styles,
) -> Result<(), Refused> {
    let paragraph = content
        .at(path)
        .filter(|e| is_paragraph(e))
        .ok_or(Refused::NotFound)?;
    let level = level.map(|level| level.clamp(1, 10));
    if heading_level(paragraph) == level {
        return Ok(());
    }
    declares(content, &[Ns::Text, Ns::Style, Ns::Fo])?;

    let (style, minted) = match level {
        Some(level) => heading_style(content, styles, level),
        None => (body_style(content), None),
    };
    let name = content.name_for(&Ns::Text, if level.is_some() { "h" } else { "p" });
    let outline = content.name_for(&Ns::Text, "outline-level");
    let style_name = content.name_for(&Ns::Text, "style-name");
    let paragraph = content.at_mut(path).ok_or(Refused::NotFound)?;
    paragraph.name = name;
    paragraph.remove_attr(&Ns::Text, "outline-level");
    paragraph.remove_attr(&Ns::Text, "style-name");
    if let Some(level) = level {
        paragraph.set_attr(outline, level.to_string());
    }
    if let Some(style) = style {
        paragraph.set_attr(style_name, style);
    }
    if let Some(minted) = minted {
        let container = automatic_styles(content);
        container.children.push(Node::Element(minted));
        container.self_closing = false;
    }
    Ok(())
}

/// The style headings of a level take: the document's own, or one written for
/// the purpose, which is answered too.
fn heading_style(
    content: &Element,
    styles: &mut Styles,
    level: u8,
) -> (Option<String>, Option<Element>) {
    if let Some(own) = styles.heading_style(level) {
        return (Some(own.to_owned()), None);
    }
    let mut style = element(content.name_for(&Ns::Style, "style"));
    let set = |element: &mut Element, ns: Ns, local: &str, value: &str| {
        element.set_attr(content.name_for(&ns, local), value);
    };
    set(&mut style, Ns::Style, "family", "paragraph");
    let mut paragraph = element(content.name_for(&Ns::Style, "paragraph-properties"));
    set(&mut paragraph, Ns::Fo, "margin-top", "0.423cm");
    set(&mut paragraph, Ns::Fo, "margin-bottom", "0.212cm");
    set(&mut paragraph, Ns::Fo, "keep-with-next", "always");
    let mut text = element(content.name_for(&Ns::Style, "text-properties"));
    let size = match level {
        1 => "18pt",
        2 => "14pt",
        3 => "12pt",
        _ => "11pt",
    };
    set(&mut text, Ns::Fo, "font-size", size);
    set(&mut text, Ns::Style, "font-size-asian", size);
    set(&mut text, Ns::Style, "font-size-complex", size);
    set(&mut text, Ns::Fo, "font-weight", "bold");
    set(&mut text, Ns::Style, "font-weight-asian", "bold");
    set(&mut text, Ns::Style, "font-weight-complex", "bold");
    for properties in [paragraph, text] {
        style.children.push(Node::Element(properties));
    }
    style.self_closing = false;
    let (name, fresh) = name_for(content, styles, style, &Family::Paragraph, "P");
    (Some(name), fresh)
}

/// The name of an automatic style that says what this one does: one the
/// document already has, or this one written under the first name no style of
/// the family has, which is answered as the style to write.
fn name_for(
    content: &Element,
    styles: &mut Styles,
    mut style: Element,
    family: &Family,
    prefix: &str,
) -> (String, Option<Element>) {
    let wanted = unnamed(&style);
    let family_name = style.attr(&Ns::Style, "family").map(ToOwned::to_owned);
    if let Some(existing) = content
        .child(&Ns::Office, "automatic-styles")
        .into_iter()
        .flat_map(Element::elements)
        .filter(|s| {
            s.is(&Ns::Style, "style") && s.attr(&Ns::Style, "family") == family_name.as_deref()
        })
        .find(|s| unnamed(s) == wanted)
        .and_then(|s| s.attr(&Ns::Style, "name"))
    {
        return (existing.to_owned(), None);
    }
    let mut number = 1;
    while styles.has_style(family, &format!("{prefix}{number}")) {
        number += 1;
    }
    let name = format!("{prefix}{number}");
    style.attrs.insert(
        0,
        crate::xml::Attribute {
            name: content.name_for(&Ns::Style, "name"),
            value: name.clone(),
        },
    );
    styles.add(&style);
    (name, Some(style))
}

/// The paragraph style most of the document's body paragraphs have, where a
/// heading taken off goes back to: none, where most have none.
fn body_style(content: &Element) -> Option<String> {
    let body = content
        .child(&Ns::Office, "body")
        .and_then(|body| body.child(&Ns::Office, "text"))?;
    let mut counts: BTreeMap<Option<&str>, usize> = BTreeMap::new();
    for paragraph in body.elements().filter(|e| e.is(&Ns::Text, "p")) {
        *counts
            .entry(paragraph.attr(&Ns::Text, "style-name"))
            .or_default() += 1;
    }
    counts
        .into_iter()
        .rev()
        .max_by_key(|&(_, count)| count)
        .and_then(|(style, _)| style.map(ToOwned::to_owned))
}

/// Make paragraphs items of a list of a kind, or take them out of the list
/// they are in. The paths are of paragraphs, in the order they are drawn in.
///
/// With a kind, a paragraph in no list is put in one, side by side with the
/// others that are, so that a run of them is one list and its numbers run on;
/// one already in a list of another kind has its list given this kind's style;
/// one in a list of this kind is left. Without a kind, each is taken out of
/// its list, and what else its item held, a list nested under it, stands
/// beside it.
///
/// A document with no list style of the kind is given one in
/// `office:automatic-styles`, which if the document has none is written
/// before the body and moves it one place along: a path held from before the
/// edit is to be taken again.
///
/// # Errors
///
/// A path does not lead to a paragraph, or the content root does not declare a
/// namespace the edit writes in. Each is refused before anything changes.
pub fn set_list(
    content: &mut Element,
    paths: &[Vec<usize>],
    kind: Option<ListKind>,
    styles: &mut Styles,
) -> Result<(), Refused> {
    for path in paths {
        content
            .at(path)
            .filter(|e| is_paragraph(e))
            .ok_or(Refused::NotFound)?;
    }
    declares(content, &[Ns::Text, Ns::Style, Ns::Fo])?;
    let Some(kind) = kind else {
        take_out(content, paths);
        return Ok(());
    };

    let (style, minted) = list_style(content, styles, kind);
    let listed: Vec<&Vec<usize>> = paths
        .iter()
        .filter(|path| list_kind_of(content, path, styles).is_some())
        .collect();
    let style_name = content.name_for(&Ns::Text, "style-name");
    for path in &listed {
        if list_kind_of(content, path, styles) != Some(kind) {
            let item = path.len() - 1;
            if let Some(list) = content
                .at_mut(&path[..item - 1])
                .filter(|e| e.is(&Ns::Text, "list"))
            {
                list.set_attr(style_name.clone(), style.clone());
            }
        }
    }
    let unlisted: Vec<Vec<usize>> = paths
        .iter()
        .filter(|path| list_kind_of(content, path, styles).is_none())
        .cloned()
        .collect();
    wrap(content, &unlisted, &style);
    if let Some(minted) = minted {
        let container = automatic_styles(content);
        container.children.push(Node::Element(minted));
        container.self_closing = false;
    }
    Ok(())
}

/// Take each paragraph's item out of its list, last first so that the paths of
/// the ones before hold, and each item once.
fn take_out(content: &mut Element, paths: &[Vec<usize>]) {
    let mut items: Vec<Vec<usize>> = paths
        .iter()
        .filter_map(|path| {
            let (_, item) = path.split_last()?;
            is_item(content.at(item)?).then(|| item.to_vec())
        })
        .collect();
    items.sort();
    items.dedup();
    for item in items.iter().rev() {
        let _ = take_out_of_list(content, item);
    }
}

/// Put paragraphs that are in no list in lists of a style: each run of them
/// that stand side by side in one parent in one list, the last run first so
/// that the paths of the ones before hold.
fn wrap(content: &mut Element, paths: &[Vec<usize>], style: &str) {
    let mut runs: Vec<(Vec<usize>, Vec<usize>)> = Vec::new();
    for path in paths {
        let Some((&at, parent)) = path.split_last() else {
            continue;
        };
        let joins = runs.last().is_some_and(|(above, indices)| {
            above == parent
                && indices.last().is_some_and(|&last| {
                    content.at(parent).is_some_and(|holder| {
                        holder.children[last + 1..at]
                            .iter()
                            .all(|node| matches!(node, Node::Text(t) if t.trim().is_empty()))
                    })
                })
        });
        if joins {
            if let Some((_, indices)) = runs.last_mut() {
                indices.push(at);
            }
        } else {
            runs.push((parent.to_vec(), vec![at]));
        }
    }
    let list_name = content.name_for(&Ns::Text, "list");
    let item_name = content.name_for(&Ns::Text, "list-item");
    let style_name = content.name_for(&Ns::Text, "style-name");
    for (parent, indices) in runs.iter().rev() {
        let (Some(&first), Some(&last)) = (indices.first(), indices.last()) else {
            continue;
        };
        let Some(holder) = content.at_mut(parent) else {
            continue;
        };
        let mut wrapped = element(list_name.clone());
        wrapped.set_attr(style_name.clone(), style);
        wrapped.self_closing = false;
        for node in &holder.children[first..=last] {
            match node {
                Node::Element(paragraph) if is_paragraph(paragraph) => {
                    let mut item = element(item_name.clone());
                    item.children.push(Node::Element(paragraph.clone()));
                    item.self_closing = false;
                    wrapped.children.push(Node::Element(item));
                }
                other => wrapped.children.push(other.clone()),
            }
        }
        holder
            .children
            .splice(first..=last, [Node::Element(wrapped)]);
    }
}

/// The list style a kind of list takes: the document's own, or one written
/// for the purpose, which is answered too.
fn list_style(content: &Element, styles: &mut Styles, kind: ListKind) -> (String, Option<Element>) {
    if let Some(own) = styles.list_style_for(kind) {
        return (own.to_owned(), None);
    }
    let mut number = 1;
    while styles.list_style(&format!("L{number}")).is_some() {
        number += 1;
    }
    let name = format!("L{number}");
    let set = |element: &mut Element, ns: Ns, local: &str, value: &str| {
        element.set_attr(content.name_for(&ns, local), value);
    };
    let mut style = element(content.name_for(&Ns::Text, "list-style"));
    set(&mut style, Ns::Style, "name", &name);
    style.self_closing = false;
    for level in 1..=6u8 {
        let tag = match kind {
            ListKind::Bullet => "list-level-style-bullet",
            ListKind::Number => "list-level-style-number",
        };
        let mut entry = element(content.name_for(&Ns::Text, tag));
        set(&mut entry, Ns::Text, "level", &level.to_string());
        match kind {
            ListKind::Bullet => set(&mut entry, Ns::Text, "bullet-char", "\u{2022}"),
            ListKind::Number => {
                set(&mut entry, Ns::Style, "num-suffix", ".");
                set(&mut entry, Ns::Style, "num-format", "1");
            }
        }
        let indent = format!("{:.3}cm", 0.635 * (f32::from(level) + 1.0));
        let mut properties = element(content.name_for(&Ns::Style, "list-level-properties"));
        set(
            &mut properties,
            Ns::Text,
            "list-level-position-and-space-mode",
            "label-alignment",
        );
        let mut alignment = element(content.name_for(&Ns::Style, "list-level-label-alignment"));
        set(&mut alignment, Ns::Text, "label-followed-by", "listtab");
        set(&mut alignment, Ns::Text, "list-tab-stop-position", &indent);
        set(&mut alignment, Ns::Fo, "text-indent", "-0.635cm");
        set(&mut alignment, Ns::Fo, "margin-left", &indent);
        properties.children.push(Node::Element(alignment));
        properties.self_closing = false;
        entry.children.push(Node::Element(properties));
        entry.self_closing = false;
        style.children.push(Node::Element(entry));
    }
    styles.add(&style);
    (name, Some(style))
}
