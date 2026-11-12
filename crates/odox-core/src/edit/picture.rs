//! A picture put into a text document, in a paragraph of its own beside the
//! paragraph the caret is in. DESIGN.md §11.
//!
//! The picture is a `draw:frame` anchored as a character, holding a
//! `draw:image` that names a part the package keeps, the way `LibreOffice`
//! writes one, sized by the caller. What it shows is written with it, or that
//! it is decoration, so that an export does not have to ask.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::block::declares;
use super::figure::{set_alternative_text, set_decorative};
use super::format::element;
use super::{Refused, is_paragraph};
use crate::style::Styles;
use crate::xml::{Element, Node, Ns};

/// A picture to put in, already kept in the package.
pub struct NewPicture<'a> {
    /// The package entry it is kept in.
    pub href: &'a str,
    /// Its media type.
    pub media_type: &'a str,
    /// How wide and how tall it is drawn, in points.
    pub size: (f32, f32),
    /// What it shows, or `None` for a picture that is decoration only. Empty
    /// text says nothing, and an export asks about it later.
    pub description: Option<&'a str>,
}

/// Put a picture in a paragraph of its own beside a paragraph, and answer the
/// path of the paragraph the caret goes to: the first after the picture, or
/// where none follows before something that is not a paragraph, the one it
/// was in.
///
/// The picture goes after the paragraph, or after the list it is an item of;
/// an empty paragraph has it go before it instead, and is left to type in
/// below it. A picture with nothing after it in its parent is given an empty
/// paragraph to follow it, so the text can go on there. The paragraph holding
/// the picture takes the style of the one beside it, unless that is a
/// heading's.
///
/// A picture marked as decoration may write an automatic style, which in a
/// document without any moves the body one place along: a path held from
/// before the edit is to be taken again.
///
/// # Errors
///
/// The path does not lead to a paragraph, or the content root does not
/// declare a namespace the picture is written in.
pub fn insert_picture(
    content: &mut Element,
    paragraph: &[usize],
    picture: &NewPicture<'_>,
    styles: &mut Styles,
) -> Result<Vec<usize>, Refused> {
    let found = content
        .at(paragraph)
        .filter(|e| is_paragraph(e))
        .ok_or(Refused::NotFound)?;
    declares(content, &[Ns::Text, Ns::Draw, Ns::Xlink, Ns::Svg])?;
    let empty = super::text(found).is_empty();
    let style = found
        .is(&Ns::Text, "p")
        .then(|| found.attr(&Ns::Text, "style-name"))
        .flatten()
        .map(ToOwned::to_owned);
    let place = super::table::outside_lists(content, paragraph);
    let (parent_path, index) = place.split_at(place.len() - 1);
    let at = if empty && place == paragraph {
        index[0]
    } else {
        index[0] + 1
    };

    let set = |element: &mut Element, ns: Ns, local: &str, value: &str| {
        element.set_attr(content.name_for(&ns, local), value);
    };
    let mut number = 1;
    while has_frame_named(content, &format!("Image{number}")) {
        number += 1;
    }
    let mut image = element(content.name_for(&Ns::Draw, "image"));
    set(&mut image, Ns::Xlink, "href", picture.href);
    set(&mut image, Ns::Xlink, "type", "simple");
    set(&mut image, Ns::Xlink, "show", "embed");
    set(&mut image, Ns::Xlink, "actuate", "onLoad");
    set(&mut image, Ns::Draw, "mime-type", picture.media_type);
    let mut frame = element(content.name_for(&Ns::Draw, "frame"));
    set(&mut frame, Ns::Draw, "name", &format!("Image{number}"));
    set(&mut frame, Ns::Text, "anchor-type", "as-char");
    set(&mut frame, Ns::Svg, "width", &length(picture.size.0));
    set(&mut frame, Ns::Svg, "height", &length(picture.size.1));
    set(&mut frame, Ns::Draw, "z-index", "0");
    frame.children.push(Node::Element(image));
    frame.self_closing = false;
    let mut holder = element(content.name_for(&Ns::Text, "p"));
    if let Some(style) = &style {
        set(&mut holder, Ns::Text, "style-name", style);
    }
    holder.children.push(Node::Element(frame));
    holder.self_closing = false;
    let follower = element(content.name_for(&Ns::Text, "p"));

    let parent = content.at_mut(parent_path).ok_or(Refused::NotFound)?;
    parent.children.insert(at, Node::Element(holder));
    parent.self_closing = false;
    let last = !parent.children[at + 1..]
        .iter()
        .any(|node| matches!(node, Node::Element(_)));
    if last {
        parent.children.push(Node::Element(follower));
    }
    let next = parent.children[at + 1..]
        .iter()
        .position(|node| matches!(node, Node::Element(e) if is_paragraph(e)))
        .map(|offset| [parent_path, &[at + 1 + offset]].concat());
    let caret = next.unwrap_or_else(|| paragraph.to_vec());

    let frame_path = [parent_path, &[at, 0]].concat();
    match picture.description {
        Some(text) if !text.trim().is_empty() => {
            set_alternative_text(content, &frame_path, text.trim())?;
        }
        Some(_) => {}
        None => set_decorative(content, &frame_path, styles)?,
    }
    Ok(caret)
}

fn has_frame_named(content: &Element, name: &str) -> bool {
    (content.is(&Ns::Draw, "frame") && content.attr(&Ns::Draw, "name") == Some(name))
        || content.elements().any(|child| has_frame_named(child, name))
}

fn length(points: f32) -> String {
    format!("{:.3}cm", points / 72.0 * 2.54)
}
