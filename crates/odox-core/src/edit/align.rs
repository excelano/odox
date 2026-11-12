//! How a paragraph lines up between its margins: against the start, centred,
//! against the end, or justified to both. DESIGN.md §11.
//!
//! Alignment is a paragraph property, so setting it writes an automatic
//! paragraph style in `content.xml`: the paragraph's own automatic style with
//! `fo:text-align` changed, or one inheriting from the named style it had, and
//! an automatic style that already says the same thing is used again. The
//! style the paragraph had is never changed, because other paragraphs may name
//! it too.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::block::{declares, name_for};
use super::format::{automatic_styles, element, unnamed};
use super::{Refused, is_paragraph};
use crate::style::{Family, Styles, TextAlign};
use crate::xml::{Element, Node, Ns};

/// How a paragraph is aligned as it is drawn: by its own style, or on a slide
/// by the style of the shape it is in where its own says nothing, or against
/// the start.
pub fn alignment(content: &Element, path: &[usize], styles: &Styles) -> TextAlign {
    let Some(paragraph) = content.at(path).filter(|e| is_paragraph(e)) else {
        return TextAlign::Start;
    };
    let own = paragraph
        .attr(&Ns::Text, "style-name")
        .and_then(|name| styles.resolve(&Family::Paragraph, name).paragraph.align);
    own.or_else(|| shape_alignment(content, path, styles))
        .unwrap_or(TextAlign::Start)
}

/// The alignment the nearest shape around a paragraph gives its text.
fn shape_alignment(content: &Element, path: &[usize], styles: &Styles) -> Option<TextAlign> {
    (0..path.len()).rev().find_map(|depth| {
        let holder = content.at(&path[..depth])?;
        if let Some(name) = holder.attr(&Ns::Presentation, "style-name") {
            return styles.resolve(&Family::Presentation, name).paragraph.align;
        }
        holder
            .attr(&Ns::Draw, "style-name")
            .and_then(|name| styles.resolve(&Family::Graphic, name).paragraph.align)
    })
}

/// The word ODF writes for an alignment.
fn written(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Start => "start",
        TextAlign::Center => "center",
        TextAlign::End => "end",
        TextAlign::Justify => "justify",
    }
}

/// Align paragraphs. A paragraph already drawn that way is left alone.
///
/// A document with no `office:automatic-styles` is given one before its body
/// when a style has to be written, which moves the body one place along: a
/// path held from before the edit is to be taken again.
///
/// # Errors
///
/// A path does not lead to a paragraph, or the content root does not declare a
/// namespace the edit writes in. Each is refused before anything changes.
pub fn set_alignment(
    content: &mut Element,
    paths: &[Vec<usize>],
    align: TextAlign,
    styles: &mut Styles,
) -> Result<(), Refused> {
    for path in paths {
        content
            .at(path)
            .filter(|e| is_paragraph(e))
            .ok_or(Refused::NotFound)?;
    }
    declares(content, &[Ns::Text, Ns::Style, Ns::Fo])?;
    let style_name = content.name_for(&Ns::Text, "style-name");
    let mut paths = paths.to_vec();
    for index in 0..paths.len() {
        let path = paths[index].clone();
        if alignment(content, &path, styles) == align {
            continue;
        }
        let current = content
            .at(&path)
            .and_then(|paragraph| paragraph.attr(&Ns::Text, "style-name"))
            .map(ToOwned::to_owned);
        let style = aligned(content, current.as_deref(), align);
        let (name, minted) = name_for(content, styles, style, &Family::Paragraph, "P");
        if let Some(minted) = minted {
            let had = content.child(&Ns::Office, "automatic-styles").is_some();
            let container = automatic_styles(content);
            container.children.push(Node::Element(minted));
            container.self_closing = false;
            if !had {
                for later in &mut paths {
                    if let Some(first) = later.first_mut() {
                        *first += 1;
                    }
                }
            }
        }
        if let Some(paragraph) = content.at_mut(&paths[index]) {
            paragraph.set_attr(style_name.clone(), name);
        }
    }
    Ok(())
}

/// An unnamed automatic paragraph style saying what the style a paragraph
/// names says, aligned: a copy of it where it is one of the document's own
/// automatic styles, which cannot be inherited from, and otherwise one
/// inheriting from it.
fn aligned(content: &Element, current: Option<&str>, align: TextAlign) -> Element {
    let own = current.and_then(|name| {
        content
            .child(&Ns::Office, "automatic-styles")?
            .elements()
            .find(|style| {
                style.is(&Ns::Style, "style")
                    && style.attr(&Ns::Style, "family") == Some("paragraph")
                    && style.attr(&Ns::Style, "name") == Some(name)
            })
    });
    let mut style = own.map_or_else(
        || {
            let mut style = element(content.name_for(&Ns::Style, "style"));
            style.set_attr(content.name_for(&Ns::Style, "family"), "paragraph");
            if let Some(parent) = current {
                style.set_attr(content.name_for(&Ns::Style, "parent-style-name"), parent);
            }
            style
        },
        unnamed,
    );
    let at = style.children.iter().position(
        |node| matches!(node, Node::Element(e) if e.is(&Ns::Style, "paragraph-properties")),
    );
    let index = at.unwrap_or_else(|| {
        let properties = element(content.name_for(&Ns::Style, "paragraph-properties"));
        style.children.insert(0, Node::Element(properties));
        0
    });
    if let Node::Element(properties) = &mut style.children[index] {
        properties.set_attr(content.name_for(&Ns::Fo, "text-align"), written(align));
    }
    style.self_closing = false;
    style
}
