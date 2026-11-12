//! Bold, italic, underline and strikethrough, a font family and a size, over
//! a range of a paragraph's text. DESIGN.md §11.
//!
//! The range's ends are cut into the nodes they fall inside, and what lies
//! between is given the mark in one of two ways: a span the range holds whole
//! has its style changed, and any other run of nodes is wrapped in a new span,
//! inside whatever holds it, whose style says only the one thing. The spans
//! the document already had are never taken apart, and a property is written
//! only where it changes what is drawn, so that taking a mark off the text it
//! was put on gives back the paragraph it was.
//!
//! A span's style is an automatic style in `content.xml`. One the document
//! already holds is used again where it says the same thing; otherwise one is
//! written, named `T` and the first number no text style has.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::HashSet;
use std::ops::Range;

use super::{
    Kind, Refused, is_inline_container, is_paragraph, normalize, parent_of, segments, set_count,
    text,
};
use crate::style::{Family, Styles, TextProperties};
use crate::value::Measure;
use crate::xml::{Attribute, Element, Name, Node, Ns};

/// What a range of text is given: a mark on or off, a font family by the name
/// a person picks it by, or a size in points.
#[derive(Debug, Clone, PartialEq)]
pub enum Setting {
    /// A mark, given or taken off.
    Mark(Mark, bool),
    /// A font family.
    Family(String),
    /// A size, in points.
    Size(f32),
}

/// A property of text that takes a value: the family or the size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextProperty {
    /// The font family.
    Family,
    /// The size.
    Size,
}

/// What a property reads as at some text, compared with what a setting asks
/// for. A size stated as a share of another reads as no size a setting asks.
#[derive(Debug, Clone, PartialEq)]
enum Reading {
    Flag(bool),
    Family(String),
    Hundredths(i64),
    Relative,
}

/// A size in points, in hundredths, which is as fine as a size is compared.
fn hundredths(points: f32) -> i64 {
    #[allow(clippy::cast_possible_truncation)]
    let hundredths = (f64::from(points) * 100.0).round() as i64;
    hundredths
}

impl Setting {
    /// What properties say of the property this sets.
    fn read(&self, properties: &TextProperties) -> Option<Reading> {
        match self {
            Self::Mark(mark, _) => mark.of(properties).map(Reading::Flag),
            Self::Family(_) => properties.font_family.clone().map(Reading::Family),
            Self::Size(_) => properties.size.map(|size| match size {
                Measure::Absolute(length) => Reading::Hundredths(hundredths(length.points())),
                Measure::Relative(_) => Reading::Relative,
            }),
        }
    }

    /// What text reads where nothing says.
    fn unsaid(&self) -> Reading {
        match self {
            Self::Mark(..) => Reading::Flag(false),
            Self::Family(_) => Reading::Family(String::new()),
            Self::Size(_) => Reading::Hundredths(0),
        }
    }

    /// What text reads once it has been given this.
    fn target(&self) -> Reading {
        match self {
            Self::Mark(_, on) => Reading::Flag(*on),
            Self::Family(family) => Reading::Family(family.clone()),
            Self::Size(points) => Reading::Hundredths(hundredths(*points)),
        }
    }

    /// Every attribute of `style:text-properties` that says something about
    /// the property: what is cleared before the setting is written.
    fn attributes(&self) -> Vec<(Ns, &'static str)> {
        match self {
            Self::Mark(mark, _) => mark.attributes(),
            Self::Family(_) => vec![
                (Ns::Style, "font-name"),
                (Ns::Fo, "font-family"),
                (Ns::Style, "font-family-generic"),
                (Ns::Style, "font-pitch"),
                (Ns::Style, "font-charset"),
                (Ns::Style, "font-style-name"),
            ],
            Self::Size(_) => vec![
                (Ns::Fo, "font-size"),
                (Ns::Style, "font-size-asian"),
                (Ns::Style, "font-size-complex"),
                (Ns::Style, "font-size-rel"),
                (Ns::Style, "font-size-rel-asian"),
                (Ns::Style, "font-size-rel-complex"),
            ],
        }
    }

    /// What is written for the setting, a family by the font face declaration
    /// `face` names. A size is written for Asian and complex scripts too, as a
    /// mark is; a family is written for Western text only, because a family a
    /// person picks for Latin letters seldom has the others.
    fn written(&self, face: &str) -> Vec<(Ns, &'static str, String)> {
        match self {
            Self::Mark(mark, on) => mark
                .written(*on)
                .into_iter()
                .map(|(ns, local, value)| (ns, local, value.to_owned()))
                .collect(),
            Self::Family(_) => vec![(Ns::Style, "font-name", face.to_owned())],
            Self::Size(points) => {
                let size = format!(
                    "{}pt",
                    format!("{points:.2}")
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                );
                vec![
                    (Ns::Fo, "font-size", size.clone()),
                    (Ns::Style, "font-size-asian", size.clone()),
                    (Ns::Style, "font-size-complex", size),
                ]
            }
        }
    }

    /// The namespaces the setting is written in, beside the style namespace.
    fn spaces(&self) -> Vec<Ns> {
        match self {
            Self::Mark(Mark::Bold | Mark::Italic, _) | Self::Size(_) => vec![Ns::Fo],
            Self::Mark(..) => Vec::new(),
            Self::Family(_) => vec![Ns::Svg],
        }
    }
}

/// Formatting a range of text is given or has taken off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mark {
    /// Bold.
    Bold,
    /// Italic.
    Italic,
    /// Underlined.
    Underline,
    /// Struck through.
    Strike,
}

impl Mark {
    /// What properties say of the mark.
    pub(super) fn of(self, properties: &TextProperties) -> Option<bool> {
        match self {
            Self::Bold => properties.bold,
            Self::Italic => properties.italic,
            Self::Underline => properties.underline,
            Self::Strike => properties.strike,
        }
    }

    /// Every attribute of `style:text-properties` that says something about
    /// the mark: what is cleared before the mark is written.
    fn attributes(self) -> Vec<(Ns, &'static str)> {
        let style = |names: &[&'static str]| names.iter().map(|n| (Ns::Style, *n)).collect();
        match self {
            Self::Bold => vec![
                (Ns::Fo, "font-weight"),
                (Ns::Style, "font-weight-asian"),
                (Ns::Style, "font-weight-complex"),
            ],
            Self::Italic => vec![
                (Ns::Fo, "font-style"),
                (Ns::Style, "font-style-asian"),
                (Ns::Style, "font-style-complex"),
            ],
            Self::Underline => style(&[
                "text-underline-type",
                "text-underline-style",
                "text-underline-width",
                "text-underline-color",
                "text-underline-mode",
            ]),
            Self::Strike => style(&[
                "text-line-through-type",
                "text-line-through-style",
                "text-line-through-width",
                "text-line-through-color",
                "text-line-through-text",
                "text-line-through-text-style",
                "text-line-through-mode",
            ]),
        }
    }

    /// What is written to give the mark or take it off. Bold and italic are
    /// written for Asian and complex scripts too, or text in those scripts
    /// would not change in an application that reads them.
    fn written(self, on: bool) -> Vec<(Ns, &'static str, &'static str)> {
        match self {
            Self::Bold => {
                let weight = if on { "bold" } else { "normal" };
                vec![
                    (Ns::Fo, "font-weight", weight),
                    (Ns::Style, "font-weight-asian", weight),
                    (Ns::Style, "font-weight-complex", weight),
                ]
            }
            Self::Italic => {
                let style = if on { "italic" } else { "normal" };
                vec![
                    (Ns::Fo, "font-style", style),
                    (Ns::Style, "font-style-asian", style),
                    (Ns::Style, "font-style-complex", style),
                ]
            }
            Self::Underline if on => vec![
                (Ns::Style, "text-underline-style", "solid"),
                (Ns::Style, "text-underline-width", "auto"),
                (Ns::Style, "text-underline-color", "font-color"),
            ],
            Self::Underline => vec![(Ns::Style, "text-underline-style", "none")],
            Self::Strike => vec![(
                Ns::Style,
                "text-line-through-style",
                if on { "solid" } else { "none" },
            )],
        }
    }
}

/// Whether the characters of a range carry a mark: `Some` when all of them
/// say the same, `None` when they differ. A range of no characters answers
/// for the character before it, or after it at the paragraph's start, which
/// is the one text typed there takes its formatting from; a paragraph with no
/// characters answers for itself.
pub fn marked(
    paragraph: &Element,
    range: Range<usize>,
    mark: Mark,
    styles: &Styles,
) -> Option<bool> {
    match reading(paragraph, range, &Setting::Mark(mark, true), styles)? {
        Reading::Flag(on) => Some(on),
        _ => None,
    }
}

/// The family or the size the characters of a range have, as [`marked`]
/// answers for a mark: `None` where they differ, and where a size is a share
/// of another's.
pub fn text_value(
    paragraph: &Element,
    range: Range<usize>,
    property: TextProperty,
    styles: &Styles,
) -> Option<Setting> {
    let probe = match property {
        TextProperty::Family => Setting::Family(String::new()),
        TextProperty::Size => Setting::Size(0.0),
    };
    match reading(paragraph, range, &probe, styles)? {
        Reading::Family(family) if !family.is_empty() => Some(Setting::Family(family)),
        #[allow(clippy::cast_precision_loss)]
        Reading::Hundredths(size) if size > 0 => Some(Setting::Size(size as f32 / 100.0)),
        _ => None,
    }
}

/// What the characters of a range read as for the property a setting sets,
/// as [`marked`] describes.
fn reading(
    paragraph: &Element,
    range: Range<usize>,
    setting: &Setting,
    styles: &Styles,
) -> Option<Reading> {
    let base = paragraph_value(paragraph, setting, styles);
    let (start, end) = if range.start < range.end {
        (range.start, range.end)
    } else if range.start > 0 {
        (range.start - 1, range.start)
    } else {
        (0, 1)
    };
    let mut answer = None;
    for segment in segments(paragraph) {
        let segment_end = segment.start + segment.kind.len();
        if segment.kind.len() == 0 || segment_end <= start || end <= segment.start {
            continue;
        }
        let value = value_at(paragraph, &segment.path, &base, setting, styles);
        if answer.as_ref().is_some_and(|a| *a != value) {
            return None;
        }
        answer = Some(value);
    }
    Some(answer.unwrap_or(base))
}

/// Give a range of the text of the paragraph at a path under the content root
/// a mark, or take it off, writing into the root's automatic styles whatever
/// style that needs and telling `styles` of it.
///
/// A document with no `office:automatic-styles` is given one before its body,
/// as the format orders them, which moves the body one place along: a path
/// held from before the edit is to be taken again.
///
/// # Errors
///
/// The path does not lead to a paragraph, or the content root does not
/// declare a namespace the mark is written in. Each is refused before
/// anything changes.
pub fn format(
    content: &mut Element,
    path: &[usize],
    range: Range<usize>,
    mark: Mark,
    on: bool,
    styles: &mut Styles,
) -> Result<(), Refused> {
    set_text(content, path, range, &Setting::Mark(mark, on), styles)
}

/// Give a range of the text of the paragraph at a path under the content root
/// a setting, as [`format`] gives a mark. A family is named through a font
/// face declaration in `content.xml`: one already there for the family, or
/// one written into `office:font-face-decls`, which a document without them
/// is given before its automatic styles, moving the body along as they do.
///
/// # Errors
///
/// As for [`format`].
pub fn set_text(
    content: &mut Element,
    path: &[usize],
    range: Range<usize>,
    setting: &Setting,
    styles: &mut Styles,
) -> Result<(), Refused> {
    let paragraph = content
        .at(path)
        .filter(|e| is_paragraph(e))
        .ok_or(Refused::NotFound)?;
    if !content.declares(&Ns::Style) || setting.spaces().iter().any(|ns| !content.declares(ns)) {
        return Err(Refused::Namespace);
    }
    let len = text(paragraph).chars().count();
    let end = range.end.min(len);
    let start = range.start.min(end);
    if start == end {
        return Ok(());
    }

    let mut edited = paragraph.clone();
    cut_at(&mut edited, end);
    cut_at(&mut edited, start);
    // What the range holds: every character inside it, and every element of
    // no length strictly inside it. One at either end stays outside a span.
    let selected = segments(&edited)
        .into_iter()
        .filter(|s| match s.kind.len() {
            0 => start < s.start && s.start < end,
            n => start <= s.start && s.start + n <= end,
        })
        .map(|s| s.path)
        .collect();
    let base = paragraph_value(&edited, setting, styles);
    let root = Element {
        attrs: content.attrs.clone(),
        ..element(content.name.clone())
    };
    let (face, declaration) = match setting {
        Setting::Family(family) => font_face(content, family),
        _ => (String::new(), None),
    };
    let mut formatter = Formatter {
        setting: setting.clone(),
        target: setting.target(),
        attributes: setting.attributes(),
        written: setting.written(&face),
        selected,
        automatic: automatic_text_styles(content),
        text: edited.name.prefix.as_deref().unwrap_or("text").to_owned(),
        root,
        styles,
        minted: Vec::new(),
    };
    formatter.within(&mut edited, &mut Vec::new(), &base);
    normalize(&mut edited);
    let minted = formatter.minted;

    // The paragraph first: writing a new automatic-styles container would
    // move the body, and the path with it.
    *content.at_mut(path).ok_or(Refused::NotFound)? = edited;
    if !minted.is_empty() {
        let container = automatic_styles(content);
        container
            .children
            .extend(minted.into_iter().map(Node::Element));
        container.self_closing = false;
    }
    if let Some(declaration) = declaration {
        if let (Some(name), Some(family)) = (
            declaration.attr(&Ns::Style, "name"),
            declaration.attr(&Ns::Svg, "font-family"),
        ) {
            styles.add_font_face(name, family.trim_matches('\''));
        }
        font_face_decls(content)
            .children
            .push(Node::Element(declaration));
    }
    Ok(())
}

/// The name of the font face declaration in `content.xml` for a family: one
/// there already, or a new one, answered beside the name to be written.
fn font_face(content: &Element, family: &str) -> (String, Option<Element>) {
    let declared = content.child(&Ns::Office, "font-face-decls");
    let faces = || declared.into_iter().flat_map(Element::elements);
    if let Some(name) = faces()
        .find(|face| {
            face.attr(&Ns::Svg, "font-family")
                .or_else(|| face.attr(&Ns::Style, "name"))
                .is_some_and(|f| f.trim_matches('\'') == family)
        })
        .and_then(|face| face.attr(&Ns::Style, "name"))
    {
        return (name.to_owned(), None);
    }
    let taken = |name: &str| faces().any(|face| face.attr(&Ns::Style, "name") == Some(name));
    let mut name = family.to_owned();
    let mut number = 1;
    while taken(&name) {
        number += 1;
        name = format!("{family}{number}");
    }
    let mut face = element(content.name_for(&Ns::Style, "font-face"));
    face.set_attr(content.name_for(&Ns::Style, "name"), name.as_str());
    let quoted = if family.contains(' ') {
        format!("'{family}'")
    } else {
        family.to_owned()
    };
    face.set_attr(content.name_for(&Ns::Svg, "font-family"), quoted);
    (name, Some(face))
}

/// The content root's `office:font-face-decls`, written before its automatic
/// styles, or before its body where it has none, where the document has none.
fn font_face_decls(content: &mut Element) -> &mut Element {
    let found = |local: &'static str| move |n: &Node| matches!(n, Node::Element(e) if e.is(&Ns::Office, local));
    let at = if let Some(at) = content.children.iter().position(found("font-face-decls")) {
        at
    } else {
        let before = content
            .children
            .iter()
            .position(|n| {
                found("automatic-styles")(n) || found("master-styles")(n) || found("body")(n)
            })
            .unwrap_or(content.children.len());
        let container = element(content.name_for(&Ns::Office, "font-face-decls"));
        content.children.insert(before, Node::Element(container));
        content.self_closing = false;
        before
    };
    let Node::Element(container) = &mut content.children[at] else {
        unreachable!("found or written as an element");
    };
    container.self_closing = false;
    container
}

/// The name a paragraph, a span or a link is resolved by, as the renderer
/// resolves it.
fn style_name(element: &Element) -> &str {
    element.attr(&Ns::Text, "style-name").unwrap_or("Standard")
}

/// What the paragraph's own style says, which everything in it reads unless
/// a span says otherwise.
fn paragraph_value(paragraph: &Element, setting: &Setting, styles: &Styles) -> Reading {
    setting
        .read(
            &styles
                .resolve(&Family::Paragraph, style_name(paragraph))
                .text,
        )
        .unwrap_or_else(|| setting.unsaid())
}

/// What an element's own style says about the property: a span's or a
/// link's, and nothing for anything else inside a paragraph.
fn own(element: &Element, setting: &Setting, styles: &Styles) -> Option<Reading> {
    if element.is(&Ns::Text, "span") || element.is(&Ns::Text, "a") {
        setting.read(&styles.resolve(&Family::Text, style_name(element)).text)
    } else {
        None
    }
}

/// What the node at a path under a paragraph reads: the nearest container
/// above it that says, or the paragraph.
fn value_at(
    paragraph: &Element,
    path: &[usize],
    base: &Reading,
    setting: &Setting,
    styles: &Styles,
) -> Reading {
    (1..path.len())
        .filter_map(|depth| paragraph.at(&path[..depth]))
        .fold(base.clone(), |value, e| {
            own(e, setting, styles).unwrap_or(value)
        })
}

/// Cut the node whose characters surround an offset in two, so that the
/// offset falls between nodes.
fn cut_at(paragraph: &mut Element, at: usize) {
    let Some(segment) = segments(paragraph)
        .into_iter()
        .find(|s| s.start < at && at < s.start + s.kind.len())
    else {
        return;
    };
    let offset = at - segment.start;
    let Some((parent, index)) = parent_of(paragraph, &segment.path) else {
        return;
    };
    let byte = |t: &str| t.char_indices().nth(offset).map_or(t.len(), |(b, _)| b);
    let second = match (&mut parent.children[index], segment.kind) {
        (Node::Text(t), _) => Node::Text(t.split_off(byte(t))),
        (Node::CData(t), _) => Node::CData(t.split_off(byte(t))),
        (Node::Element(space), Kind::Spaces(count)) => {
            let mut second = space.clone();
            set_count(space, offset);
            set_count(&mut second, count - offset);
            Node::Element(second)
        }
        _ => return,
    };
    parent.children.insert(index + 1, second);
}

/// The text styles `content.xml` holds among its automatic styles.
fn automatic_text_styles(content: &Element) -> Vec<Element> {
    content
        .child(&Ns::Office, "automatic-styles")
        .into_iter()
        .flat_map(Element::elements)
        .filter(|e| e.is(&Ns::Style, "style") && e.attr(&Ns::Style, "family") == Some("text"))
        .cloned()
        .collect()
}

/// The content root's `office:automatic-styles`, written before the body where
/// the document has none.
pub(crate) fn automatic_styles(content: &mut Element) -> &mut Element {
    let found = |local: &'static str| move |n: &Node| matches!(n, Node::Element(e) if e.is(&Ns::Office, local));
    let at = if let Some(at) = content.children.iter().position(found("automatic-styles")) {
        at
    } else {
        let before = content
            .children
            .iter()
            .position(|n| found("master-styles")(n) || found("body")(n))
            .unwrap_or(content.children.len());
        let container = element(content.name_for(&Ns::Office, "automatic-styles"));
        content.children.insert(before, Node::Element(container));
        content.self_closing = false;
        before
    };
    let Node::Element(container) = &mut content.children[at] else {
        unreachable!("found or written as an element");
    };
    container
}

pub(crate) fn element(name: Name) -> Element {
    Element {
        name,
        attrs: Vec::new(),
        children: Vec::new(),
        self_closing: true,
    }
}

/// How much of a node the range holds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Held {
    /// All of it.
    Whole,
    /// Some of what is inside it.
    Part,
    /// None of it.
    Outside,
    /// A comment or a processing instruction, which a run passes over.
    Neutral,
}

/// One range being given a setting.
struct Formatter<'a> {
    setting: Setting,
    /// What the range reads once it is given the setting.
    target: Reading,
    /// What is cleared before the setting is written.
    attributes: Vec<(Ns, &'static str)>,
    /// What writing the setting writes.
    written: Vec<(Ns, &'static str, String)>,
    /// The paths from the paragraph of what the range holds.
    selected: HashSet<Vec<usize>>,
    /// The text styles among the automatic styles, with those written here.
    automatic: Vec<Element>,
    /// The prefix the paragraph spells the text namespace with.
    text: String,
    /// The content root without its children: what a name written into the
    /// styles is spelled by.
    root: Element,
    styles: &'a mut Styles,
    /// The styles written here, to go into the tree.
    minted: Vec<Element>,
}

impl Formatter<'_> {
    fn held(&self, node: &Node, path: &mut Vec<usize>) -> Held {
        match node {
            Node::Comment(_) | Node::ProcessingInstruction(_) => Held::Neutral,
            Node::Element(e) if is_inline_container(e) => {
                let (mut whole, mut some) = (true, false);
                for (index, child) in e.children.iter().enumerate() {
                    path.push(index);
                    match self.held(child, path) {
                        Held::Whole => some = true,
                        Held::Part => (some, whole) = (true, false),
                        Held::Outside => whole = false,
                        Held::Neutral => {}
                    }
                    path.pop();
                }
                match (some, whole) {
                    (true, true) => Held::Whole,
                    (true, false) => Held::Part,
                    _ => Held::Outside,
                }
            }
            _ if self.selected.contains(path) => Held::Whole,
            _ => Held::Outside,
        }
    }

    /// Give the mark to what the range holds under an element whose contents
    /// read `context`. Runs of what it holds whole are wrapped where they read
    /// otherwise; a span it holds whole whose own style says otherwise is
    /// restyled; and whatever it holds part of is gone into.
    fn within(&mut self, element: &mut Element, path: &mut Vec<usize>, context: &Reading) {
        let held: Vec<Held> = (0..element.children.len())
            .map(|index| {
                path.push(index);
                let held = self.held(&element.children[index], path);
                path.pop();
                held
            })
            .collect();
        let mut out = Vec::with_capacity(element.children.len());
        let mut run = Vec::new();
        // Comments after the run so far, which join it if more follows.
        let mut waiting = Vec::new();
        for (index, (node, held)) in std::mem::take(&mut element.children)
            .into_iter()
            .zip(held)
            .enumerate()
        {
            path.push(index);
            let contrary = matches!(&node, Node::Element(e)
                if is_inline_container(e)
                    && own(e, &self.setting, self.styles).is_some_and(|v| v != self.target));
            match (held, node) {
                (Held::Neutral, node) if !run.is_empty() => waiting.push(node),
                (Held::Whole, mut node) if !contrary => {
                    // A container inside a run reads the mark once the run
                    // does; something deeper in it may still say otherwise.
                    if let Node::Element(e) = &mut node
                        && is_inline_container(e)
                    {
                        let target = self.target.clone();
                        self.within(e, path, &target);
                    }
                    run.append(&mut waiting);
                    run.push(node);
                }
                (held, node) => {
                    self.close(&mut run, &mut out, context);
                    out.append(&mut waiting);
                    match (held, node) {
                        (Held::Whole, Node::Element(mut e)) if e.is(&Ns::Text, "span") => {
                            let target = self.target.clone();
                            self.within(&mut e, path, &target);
                            if self.restyle(&mut e, context) {
                                out.extend(e.children);
                            } else {
                                out.push(Node::Element(e));
                            }
                        }
                        // A link whose own style says otherwise: its contents
                        // are wrapped inside it.
                        (Held::Whole, Node::Element(mut e)) => {
                            let inner = own(&e, &self.setting, self.styles)
                                .unwrap_or_else(|| context.clone());
                            self.within(&mut e, path, &inner);
                            out.push(Node::Element(e));
                        }
                        (Held::Part, Node::Element(mut e)) => {
                            let inner = own(&e, &self.setting, self.styles)
                                .unwrap_or_else(|| context.clone());
                            self.within(&mut e, path, &inner);
                            out.push(Node::Element(e));
                        }
                        (_, node) => out.push(node),
                    }
                }
            }
            path.pop();
        }
        self.close(&mut run, &mut out, context);
        out.append(&mut waiting);
        element.children = out;
    }

    /// Give a run the mark where its surroundings do not: a lone span by its
    /// style, anything else by a new span around it.
    fn close(&mut self, run: &mut Vec<Node>, out: &mut Vec<Node>, context: &Reading) {
        if run.is_empty() {
            return;
        }
        if *context != self.target {
            if let [Node::Element(span)] = run.as_mut_slice()
                && span.is(&Ns::Text, "span")
            {
                if own(span, &self.setting, self.styles).as_ref() != Some(&self.target) {
                    // The mark is written, so the span is never left empty.
                    self.restyle(span, context);
                }
            } else {
                let mut style = self.blank(None);
                self.write_mark(&mut style);
                let name = self.name(style);
                let mut span = element(Name::new(&self.text, "span", Ns::Text));
                span.set_attr(Name::new(&self.text, "style-name", Ns::Text), name);
                span.children = std::mem::take(run);
                span.self_closing = false;
                out.push(Node::Element(span));
                return;
            }
        }
        out.append(run);
    }

    /// Change a span's style so that the span reads the mark, in a parent
    /// whose contents read `context`. Answers whether the span is left saying
    /// nothing at all, and should give way to its contents.
    ///
    /// The mark can be said two ways: written into the span's style, or left
    /// out of it where what the span inherits already says it. Left out is
    /// the smaller, and is taken unless the document has a style that says it
    /// the written way and none that says it the other, which is what brings
    /// a span back to the style it had when a mark is put on and taken off.
    fn restyle(&mut self, span: &mut Element, context: &Reading) -> bool {
        let current = span.attr(&Ns::Text, "style-name").map(ToOwned::to_owned);
        let style = match current.as_deref().and_then(|n| self.automatic(n)) {
            Some(existing) => existing.clone(),
            None => self.blank(current.as_deref()),
        };
        let mut written = style.clone();
        self.write_mark(&mut written);
        let mut bare = style;
        if let Some(properties) = bare.child_mut(&Ns::Style, "text-properties") {
            for (ns, local) in &self.attributes {
                properties.remove_attr(ns, local);
            }
        }
        bare.children.retain(|n| {
            !matches!(n, Node::Element(e)
                if e.is(&Ns::Style, "text-properties") && e.attrs.is_empty() && e.children.is_empty())
        });
        // What the span reads with no word of its own on the mark: its
        // style's parent, or with no parent the family's default, which is
        // what a name the document does not have resolves to.
        let parent = bare.attr(&Ns::Style, "parent-style-name");
        let bare_reads = self
            .setting
            .read(
                &self
                    .styles
                    .resolve(&Family::Text, parent.unwrap_or(""))
                    .text,
            )
            .unwrap_or_else(|| context.clone());

        let style = if bare_reads != self.target {
            written
        } else if parent.is_none() && bare.elements().next().is_none() {
            span.remove_attr(&Ns::Text, "style-name");
            return span.attrs.is_empty();
        } else if self.existing(&bare).is_none() && self.existing(&written).is_some() {
            written
        } else {
            bare
        };
        let name = self.name(style);
        match span
            .attrs
            .iter_mut()
            .find(|a| a.name.is(&Ns::Text, "style-name"))
        {
            Some(attr) => attr.value = name,
            None => span.set_attr(Name::new(&self.text, "style-name", Ns::Text), name),
        }
        false
    }

    /// Write the mark into a style's text properties, which it is given
    /// where it has none. An attribute the style has is changed where it
    /// stands, and one the mark does not write is taken out.
    fn write_mark(&self, style: &mut Element) {
        if style.child(&Ns::Style, "text-properties").is_none() {
            let properties = element(self.root.name_for(&Ns::Style, "text-properties"));
            style.children.push(Node::Element(properties));
            style.self_closing = false;
        }
        let Some(properties) = style.child_mut(&Ns::Style, "text-properties") else {
            return;
        };
        for (ns, local) in &self.attributes {
            if !self.written.iter().any(|(n, l, _)| n == ns && l == local) {
                properties.remove_attr(ns, local);
            }
        }
        for (ns, local, value) in &self.written {
            match properties.attrs.iter_mut().find(|a| a.name.is(ns, local)) {
                Some(attr) => attr.value.clone_from(value),
                None => properties.set_attr(self.root.name_for(ns, local), value.as_str()),
            }
        }
    }

    /// A text style that says nothing yet, inheriting from `parent`.
    fn blank(&self, parent: Option<&str>) -> Element {
        let mut style = element(self.root.name_for(&Ns::Style, "style"));
        style.set_attr(self.root.name_for(&Ns::Style, "family"), "text");
        if let Some(parent) = parent {
            style.set_attr(self.root.name_for(&Ns::Style, "parent-style-name"), parent);
        }
        style
    }

    fn automatic(&self, name: &str) -> Option<&Element> {
        self.automatic
            .iter()
            .find(|s| s.attr(&Ns::Style, "name") == Some(name))
    }

    /// The name of an automatic style that says what this one does.
    fn existing(&self, style: &Element) -> Option<&str> {
        let wanted = unnamed(style);
        self.automatic
            .iter()
            .find(|s| unnamed(s) == wanted)
            .and_then(|s| s.attr(&Ns::Style, "name"))
    }

    /// The name of an automatic style that says what this one does, written
    /// into the styles where none is there yet.
    fn name(&mut self, mut style: Element) -> String {
        if let Some(name) = self.existing(&style) {
            return name.to_owned();
        }
        let taken =
            |n: &str| self.styles.style(&Family::Text, n).is_some() || self.automatic(n).is_some();
        let mut number = 1;
        while taken(&format!("T{number}")) {
            number += 1;
        }
        let name = format!("T{number}");
        style.remove_attr(&Ns::Style, "name");
        style.attrs.insert(
            0,
            Attribute {
                name: self.root.name_for(&Ns::Style, "name"),
                value: name.clone(),
            },
        );
        self.styles.add(&style);
        self.automatic.push(style.clone());
        self.minted.push(style);
        name
    }
}

/// A style with its name taken off, to compare with another by what it says.
pub(crate) fn unnamed(style: &Element) -> Element {
    let mut style = style.clone();
    style.remove_attr(&Ns::Style, "name");
    style
}
