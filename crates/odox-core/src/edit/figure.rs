//! What a picture says to someone who cannot see it.
//!
//! A frame holding a picture is a figure, and a figure either has alternative
//! text, the `svg:title` inside the frame that `LibreOffice` calls its text
//! alternative, or the longer `svg:desc`, or is decoration that says nothing.
//! ODF has no decorative flag of its own; `LibreOffice` writes
//! `loext:decorative` into the frame's graphic style, and that is read and
//! written here so that a document marked in one application reads the same in
//! the other.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::Refused;
use super::block::{declares, name_for};
use super::format::{automatic_styles, element};
use crate::style::{Family, Styles};
use crate::xml::{Element, Node, Ns};

/// What a figure says in place of its picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Description {
    /// Its alternative text.
    Text(String),
    /// Nothing: it is decoration.
    Decorative,
    /// Nobody has said.
    Missing,
}

/// Whether a frame is a figure: one that holds a picture.
pub fn is_figure(frame: &Element) -> bool {
    frame.is(&Ns::Draw, "frame") && frame.child(&Ns::Draw, "image").is_some()
}

/// The figures under an element, in reading order, as paths from it.
///
/// A note's body is not read: it is not drawn, so a picture in it is not shown
/// to anyone.
pub fn figures(root: &Element) -> Vec<Vec<usize>> {
    let mut found = Vec::new();
    collect(root, &mut Vec::new(), &mut found);
    found
}

fn collect(parent: &Element, path: &mut Vec<usize>, into: &mut Vec<Vec<usize>>) {
    for (index, child) in parent.elements_indexed() {
        if child.is(&Ns::Text, "note") {
            continue;
        }
        path.push(index);
        if is_figure(child) {
            into.push(path.clone());
        } else {
            collect(child, path, into);
        }
        path.pop();
    }
}

/// What a figure says in place of its picture.
///
/// Decoration wins over a title, because a person who marked a picture
/// decorative has said it is to be skipped whatever text it still carries.
pub fn description(frame: &Element, styles: &Styles) -> Description {
    let decorative = frame
        .attr(&Ns::Draw, "style-name")
        .map(|name| styles.resolve(&Family::Graphic, name))
        .and_then(|properties| properties.graphic.decorative)
        .unwrap_or(false);
    if decorative {
        return Description::Decorative;
    }
    ["title", "desc"]
        .into_iter()
        .filter_map(|local| frame.child(&Ns::Svg, local))
        .map(|text| text.plain_text().trim().to_owned())
        .find(|text| !text.is_empty())
        .map_or(Description::Missing, Description::Text)
}

/// Give the figure at a path under the content root its alternative text,
/// replacing what its `svg:title` said.
///
/// # Errors
///
/// The path does not lead to a figure, or the content root does not declare
/// the SVG namespace. Each is refused before anything changes.
pub fn set_alternative_text(
    content: &mut Element,
    path: &[usize],
    text: &str,
) -> Result<(), Refused> {
    content
        .at(path)
        .filter(|e| is_figure(e))
        .ok_or(Refused::NotFound)?;
    declares(content, &[Ns::Svg])?;
    let mut title = element(content.name_for(&Ns::Svg, "title"));
    title.children.push(Node::Text(text.to_owned()));
    title.self_closing = false;

    let frame = content.at_mut(path).ok_or(Refused::NotFound)?;
    let existing = frame
        .children
        .iter()
        .position(|n| matches!(n, Node::Element(e) if e.is(&Ns::Svg, "title")));
    if let Some(at) = existing {
        frame.children[at] = Node::Element(title);
    } else {
        // After the picture and before a description or a contour, which is
        // the order the schema gives a frame's children.
        let at = frame
            .children
            .iter()
            .position(|n| {
                matches!(n, Node::Element(e)
                    if e.is(&Ns::Svg, "desc")
                        || e.is(&Ns::Draw, "contour-polygon")
                        || e.is(&Ns::Draw, "contour-path"))
            })
            .unwrap_or(frame.children.len());
        frame.children.insert(at, Node::Element(title));
        frame.self_closing = false;
    }
    Ok(())
}

/// Mark the figure at a path under the content root as decoration.
///
/// The frame is given an automatic graphic style that says so: a copy of the
/// one it has where that one is automatic, so that nothing else it says is
/// lost, and otherwise a style that inherits from the one it names. A document
/// with no `office:automatic-styles` is given one before its body, which moves
/// the body one place along: a path held from before the edit is to be taken
/// again.
///
/// # Errors
///
/// The path does not lead to a figure, or the content root does not declare a
/// namespace the edit writes in. Each is refused before anything changes.
pub fn set_decorative(
    content: &mut Element,
    path: &[usize],
    styles: &mut Styles,
) -> Result<(), Refused> {
    let frame = content
        .at(path)
        .filter(|e| is_figure(e))
        .ok_or(Refused::NotFound)?;
    if description(frame, styles) == Description::Decorative {
        return Ok(());
    }
    declares(content, &[Ns::Draw, Ns::Style, Ns::Loext])?;
    let current = frame.attr(&Ns::Draw, "style-name").map(ToOwned::to_owned);

    let automatic = current.as_deref().and_then(|name| {
        content
            .child(&Ns::Office, "automatic-styles")?
            .elements()
            .find(|s| {
                s.is(&Ns::Style, "style")
                    && s.attr(&Ns::Style, "family") == Some("graphic")
                    && s.attr(&Ns::Style, "name") == Some(name)
            })
            .cloned()
    });
    let mut style = if let Some(mut copy) = automatic {
        copy.remove_attr(&Ns::Style, "name");
        copy
    } else {
        let mut style = element(content.name_for(&Ns::Style, "style"));
        style.set_attr(content.name_for(&Ns::Style, "family"), "graphic");
        if let Some(parent) = &current {
            style.set_attr(content.name_for(&Ns::Style, "parent-style-name"), parent);
        }
        style
    };
    let properties = if let Some(at) = style
        .children
        .iter()
        .position(|n| matches!(n, Node::Element(e) if e.is(&Ns::Style, "graphic-properties")))
    {
        at
    } else {
        style.children.push(Node::Element(element(
            content.name_for(&Ns::Style, "graphic-properties"),
        )));
        style.self_closing = false;
        style.children.len() - 1
    };
    if let Node::Element(properties) = &mut style.children[properties] {
        properties.set_attr(content.name_for(&Ns::Loext, "decorative"), "true");
    }

    let (name, fresh) = name_for(content, styles, style, &Family::Graphic, "gr");
    let name_attr = content.name_for(&Ns::Draw, "style-name");
    content
        .at_mut(path)
        .ok_or(Refused::NotFound)?
        .set_attr(name_attr, name);
    if let Some(fresh) = fresh {
        let container = automatic_styles(content);
        container.children.push(Node::Element(fresh));
        container.self_closing = false;
    }
    Ok(())
}
