//! The order of a deck's slides, and whether it has one more or one less.
//!
//! A slide is a `draw:page` with the notes, the shapes and the animations it
//! carries, and it is named: links, custom shows and the start page refer to a
//! slide by its `draw:name`. Moving one changes nothing that refers to it.
//! Deleting one takes its name out of the places that list it. Duplicating one
//! gives the copy a name of its own and a fresh id for everything in it that
//! has one, because an id names one element of a document, and the references
//! between those elements inside the copy follow. A new slide takes the
//! master of the one it follows, and a title and a text frame where that
//! master puts its placeholders.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::{HashMap, HashSet};

use super::Presentation;
use crate::edit::Refused;
use crate::xml::{Attribute, Element, Node, Ns};

/// The attributes that name another element by its id, which a copy of a
/// slide has to point at the copy's own.
const REFERENCES: [&str; 3] = ["start-shape", "end-shape", "targetElement"];

fn is_page(node: &Node) -> bool {
    matches!(node, Node::Element(e) if e.is(&Ns::Draw, "page"))
}

fn is_id(attribute: &Attribute) -> bool {
    attribute.name.is(&Ns::Draw, "id")
        || attribute.name.local.as_ref() == "id" && attribute.name.prefix.as_deref() == Some("xml")
}

impl Presentation {
    fn presentation_mut(&mut self) -> Option<&mut Element> {
        self.document
            .content
            .child_mut(&Ns::Office, "body")?
            .child_mut(&Ns::Office, "presentation")
    }

    /// Move a slide one place earlier or later among the slides, and answer
    /// where it is now. The slide is named by its position among the body's
    /// children, as [`Presentation::slides`] gives it.
    ///
    /// # Errors
    ///
    /// There is no such slide, or no slide on that side of it.
    pub fn move_slide(&mut self, position: usize, later: bool) -> Result<usize, Refused> {
        let presentation = self.presentation_mut().ok_or(Refused::NotFound)?;
        if !presentation.children.get(position).is_some_and(is_page) {
            return Err(Refused::NotFound);
        }
        let other = if later {
            (position + 1..presentation.children.len())
                .find(|&i| is_page(&presentation.children[i]))
        } else {
            (0..position)
                .rev()
                .find(|&i| is_page(&presentation.children[i]))
        }
        .ok_or(Refused::NotFound)?;
        presentation.children.swap(position, other);
        Ok(other)
    }

    /// Take a slide out of the deck, with its notes, and its name out of the
    /// custom shows and the start page that list it.
    ///
    /// # Errors
    ///
    /// There is no such slide, or it is the only one.
    pub fn delete_slide(&mut self, position: usize) -> Result<(), Refused> {
        let presentation = self.presentation_mut().ok_or(Refused::NotFound)?;
        let Some(Node::Element(page)) = presentation.children.get(position) else {
            return Err(Refused::NotFound);
        };
        if !page.is(&Ns::Draw, "page") {
            return Err(Refused::NotFound);
        }
        if presentation.children.iter().filter(|n| is_page(n)).count() == 1 {
            return Err(Refused::LastOne);
        }
        let name = page.attr(&Ns::Draw, "name").map(ToOwned::to_owned);
        presentation.children.remove(position);
        if let Some(name) = name {
            forget_name(presentation, &name);
        }
        Ok(())
    }

    /// Put a copy of a slide after it, and answer where the copy is. The copy
    /// has a name no slide has, and every `xml:id` and `draw:id` in it, the
    /// notes included, is new, with the references to them inside the copy
    /// following.
    ///
    /// # Errors
    ///
    /// There is no such slide.
    pub fn duplicate_slide(&mut self, position: usize) -> Result<usize, Refused> {
        let mut taken = HashSet::new();
        collect_ids(&self.document.content, &mut taken);
        if let Some(styles) = &self.document.styles_part {
            collect_ids(styles, &mut taken);
        }
        let presentation = self.presentation_mut().ok_or(Refused::NotFound)?;
        let Some(Node::Element(original)) = presentation.children.get(position) else {
            return Err(Refused::NotFound);
        };
        if !original.is(&Ns::Draw, "page") {
            return Err(Refused::NotFound);
        }
        let names: HashSet<String> = presentation
            .elements()
            .filter(|e| e.is(&Ns::Draw, "page"))
            .filter_map(|e| e.attr(&Ns::Draw, "name").map(ToOwned::to_owned))
            .collect();
        let mut copy = original.clone();
        if let Some(attribute) = copy.attrs.iter_mut().find(|a| a.name.is(&Ns::Draw, "name")) {
            attribute.value = (2..=names.len() + 2)
                .map(|n| format!("{} ({n})", attribute.value))
                .find(|candidate| !names.contains(candidate))
                .unwrap_or_default();
        }
        let mut renamed = HashMap::new();
        let mut counter = 0;
        renumber(&mut copy, &mut renamed, &mut taken, &mut counter);
        follow_references(&mut copy, &renamed);
        presentation
            .children
            .insert(position + 1, Node::Element(copy));
        Ok(position + 1)
    }
}

/// The two placeholders a new slide is given, in drawing order.
const PLACEHOLDERS: [&str; 2] = ["title", "outline"];

impl Presentation {
    /// Put a new slide after a slide, on the same master, with an empty title
    /// and an empty text frame, and answer where it is. Each frame is where
    /// the master's placeholder of its class is, in that placeholder's style
    /// where the slide can name it; where the master has none, it is laid out
    /// across the page. Where the slide it follows has a frame of the class,
    /// the new one takes that frame's style and its first paragraph's, so
    /// that a title looks like the title before it.
    ///
    /// # Errors
    ///
    /// There is no such slide.
    pub fn add_slide(&mut self, position: usize) -> Result<usize, Refused> {
        let slides = self.slides();
        let slide = slides
            .iter()
            .find(|slide| slide.position == position)
            .ok_or(Refused::NotFound)?;
        let layout = self.page_layout(slide);
        let master = self.master(slide);
        let frames: Vec<Element> = PLACEHOLDERS
            .iter()
            .map(|class| {
                let mut frame = match master.and_then(|m| placeholder(m, class)) {
                    Some(placeholder) => self.frame_from(placeholder),
                    None => self.frame_across(class, layout.width.points(), layout.height.points()),
                };
                if let Some(alike) = placeholder(slide.element, class) {
                    self.style_like(&mut frame, alike);
                }
                frame
            })
            .collect();
        let names: HashSet<&str> = slides.iter().filter_map(|slide| slide.name).collect();
        let name = (slides.len() + 1..=slides.len() * 2 + 1)
            .map(|n| format!("page{n}"))
            .find(|candidate| !names.contains(candidate.as_str()))
            .unwrap_or_default();
        let mut page = Element::new("", "page", Ns::Draw);
        page.name = self.document.name(&Ns::Draw, "page");
        page.set_attr(self.document.name(&Ns::Draw, "name"), name);
        if let Some(master) = slide.master_page {
            page.set_attr(
                self.document.name(&Ns::Draw, "master-page-name"),
                master.to_owned(),
            );
        }
        page.children = frames.into_iter().map(Node::Element).collect();
        page.self_closing = false;
        let presentation = self.presentation_mut().ok_or(Refused::NotFound)?;
        presentation
            .children
            .insert(position + 1, Node::Element(page));
        Ok(position + 1)
    }

    /// A slide's frame where a master's placeholder is, holding one empty
    /// paragraph in the placeholder's paragraph style. What only the master
    /// can be in, its layer and a style of its own automatic ones, is left
    /// behind.
    fn frame_from(&self, placeholder: &Element) -> Element {
        let paragraph_style = placeholder
            .descendant(&Ns::Text, "p")
            .and_then(|p| p.attr(&Ns::Text, "style-name"))
            .filter(|style| self.is_common_style(style));
        let mut frame = placeholder.clone();
        frame.attrs.retain(|attribute| {
            let name = &attribute.name;
            !(name.is(&Ns::Draw, "layer")
                || name.is(&Ns::Draw, "name")
                || name.is(&Ns::Presentation, "placeholder")
                || name.is(&Ns::Presentation, "user-transformed")
                || is_id(attribute))
        });
        for (ns, local) in [
            (Ns::Presentation, "style-name"),
            (Ns::Draw, "text-style-name"),
        ] {
            if frame
                .attr(&ns, local)
                .is_some_and(|style| !self.is_common_style(style))
            {
                frame.remove_attr(&ns, local);
            }
        }
        let mut text_box = self.empty_text_box();
        if let (Some(style), Some(paragraph)) =
            (paragraph_style, text_box.child_mut(&Ns::Text, "p"))
        {
            paragraph.set_attr(
                self.document.name(&Ns::Text, "style-name"),
                style.to_owned(),
            );
        }
        frame.children = vec![Node::Element(text_box)];
        frame.self_closing = false;
        frame
    }

    /// A frame of a placeholder's class on a page with no placeholder for it:
    /// the title across the top, the text under it.
    fn frame_across(&self, class: &str, width: f32, height: f32) -> Element {
        let (top, tall) = if class == "title" {
            (0.04, 0.17)
        } else {
            (0.24, 0.68)
        };
        let cm = |points: f32| format!("{:.3}cm", points / 28.346_457);
        let mut frame = Element::new("", "frame", Ns::Draw);
        frame.name = self.document.name(&Ns::Draw, "frame");
        for (ns, local, value) in [
            (Ns::Presentation, "class", class.to_owned()),
            (Ns::Svg, "x", cm(width * 0.05)),
            (Ns::Svg, "y", cm(height * top)),
            (Ns::Svg, "width", cm(width * 0.9)),
            (Ns::Svg, "height", cm(height * tall)),
        ] {
            frame.set_attr(self.document.name(&ns, local), value);
        }
        frame.children = vec![Node::Element(self.empty_text_box())];
        frame.self_closing = false;
        frame
    }

    /// Give a new frame the style of another and its paragraph the style of
    /// the other's first.
    fn style_like(&self, frame: &mut Element, alike: &Element) {
        if let Some(style) = alike.attr(&Ns::Presentation, "style-name") {
            frame.set_attr(
                self.document.name(&Ns::Presentation, "style-name"),
                style.to_owned(),
            );
        }
        let style = alike
            .descendant(&Ns::Text, "p")
            .and_then(|p| p.attr(&Ns::Text, "style-name"));
        let paragraph = frame
            .child_mut(&Ns::Draw, "text-box")
            .and_then(|text_box| text_box.child_mut(&Ns::Text, "p"));
        if let (Some(style), Some(paragraph)) = (style, paragraph) {
            paragraph.set_attr(
                self.document.name(&Ns::Text, "style-name"),
                style.to_owned(),
            );
        }
    }

    fn empty_text_box(&self) -> Element {
        let mut paragraph = Element::new("", "p", Ns::Text);
        paragraph.name = self.document.name(&Ns::Text, "p");
        let mut text_box = Element::new("", "text-box", Ns::Draw);
        text_box.name = self.document.name(&Ns::Draw, "text-box");
        text_box.children = vec![Node::Element(paragraph)];
        text_box.self_closing = false;
        text_box
    }

    /// Whether a style is one of the common styles in `styles.xml`, which a
    /// slide can name; the automatic ones there belong to the masters.
    fn is_common_style(&self, name: &str) -> bool {
        self.document
            .styles_part
            .as_ref()
            .and_then(|styles| styles.child(&Ns::Office, "styles"))
            .is_some_and(|common| {
                common
                    .elements()
                    .any(|style| style.attr(&Ns::Style, "name") == Some(name))
            })
    }
}

/// A master's placeholder frame of a class.
fn placeholder<'a>(master: &'a Element, class: &str) -> Option<&'a Element> {
    master
        .elements()
        .find(|e| e.is(&Ns::Draw, "frame") && e.attr(&Ns::Presentation, "class") == Some(class))
}

/// Every id an element and what is under it has.
fn collect_ids(element: &Element, into: &mut HashSet<String>) {
    for attribute in element.attrs.iter().filter(|a| is_id(a)) {
        into.insert(attribute.value.clone());
    }
    for child in element.elements() {
        collect_ids(child, into);
    }
}

/// Give every id in an element and under it a new one, remembering which
/// replaced which.
fn renumber(
    element: &mut Element,
    renamed: &mut HashMap<String, String>,
    taken: &mut HashSet<String>,
    counter: &mut usize,
) {
    for attribute in element.attrs.iter_mut().filter(|a| is_id(a)) {
        let fresh = renamed.entry(attribute.value.clone()).or_insert_with(|| {
            loop {
                *counter += 1;
                let candidate = format!("odox{counter}");
                if taken.insert(candidate.clone()) {
                    break candidate;
                }
            }
        });
        attribute.value.clone_from(fresh);
    }
    for child in &mut element.children {
        if let Node::Element(child) = child {
            renumber(child, renamed, taken, counter);
        }
    }
}

/// Point the references inside an element at the ids that replaced the ones
/// they named.
fn follow_references(element: &mut Element, renamed: &HashMap<String, String>) {
    for attribute in &mut element.attrs {
        if REFERENCES.contains(&attribute.name.local.as_ref())
            && let Some(fresh) = renamed.get(&attribute.value)
        {
            attribute.value.clone_from(fresh);
        }
    }
    for child in &mut element.children {
        if let Node::Element(child) = child {
            follow_references(child, renamed);
        }
    }
}

/// Take a slide's name out of the places that list slides by name: the start
/// page, and the pages of each custom show, which goes where it is left with
/// none.
fn forget_name(presentation: &mut Element, name: &str) {
    for child in &mut presentation.children {
        let Node::Element(element) = child else {
            continue;
        };
        if element.is(&Ns::Presentation, "settings")
            && element.attr(&Ns::Presentation, "start-page") == Some(name)
        {
            element.remove_attr(&Ns::Presentation, "start-page");
        }
        if element.is(&Ns::Presentation, "show")
            && let Some(attribute) = element
                .attrs
                .iter_mut()
                .find(|a| a.name.is(&Ns::Presentation, "pages"))
        {
            attribute.value = attribute
                .value
                .split(',')
                .filter(|page| *page != name)
                .collect::<Vec<_>>()
                .join(",");
        }
    }
    presentation.children.retain(|node| {
        !matches!(node, Node::Element(e) if e.is(&Ns::Presentation, "show") && e.attr(&Ns::Presentation, "pages") == Some(""))
    });
}
