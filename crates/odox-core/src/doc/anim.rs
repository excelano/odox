//! A slide's builds: the shapes and paragraphs its main sequence of effects
//! shows or takes away, click by click. Every effect is taken at its end: what
//! it leaves visible or hidden, never how it moves. DESIGN.md §7.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use crate::xml::{Element, Ns};

/// What an effect acts on: a shape, by its index among the page's children,
/// or one of its paragraphs, by its place among the shape's paragraphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A shape.
    Shape(usize),
    /// A paragraph of a shape.
    Paragraph(usize, usize),
}

/// One effect that shows or hides its target, at the click it comes with.
#[derive(Debug, Clone, Copy)]
struct Effect {
    step: usize,
    target: Target,
    shows: bool,
}

/// A slide's builds, step by step: step nought is the slide as it is entered,
/// and each click after it is one more.
#[derive(Debug, Clone, Default)]
pub struct Steps {
    clicks: usize,
    effects: Vec<Effect>,
}

impl Steps {
    /// Read the main sequence of a `draw:page`. A slide with none has no
    /// steps.
    pub fn read(page: &Element) -> Self {
        let Some(sequence) = page
            .elements()
            .filter(|e| e.is(&Ns::Anim, "par"))
            .find_map(main_sequence)
        else {
            return Self::default();
        };
        let mut steps = Self::default();
        let mut effects = Vec::new();
        collect(sequence, &mut effects);
        for effect in effects {
            if effect.attr(&Ns::Presentation, "node-type") == Some("on-click") {
                steps.clicks += 1;
            }
            let Some(shows) = shows(effect) else {
                continue;
            };
            let mut targets = Vec::new();
            for (id, text_only) in target_ids(effect) {
                let found = match resolve(page, id) {
                    Some(Target::Shape(shape)) if text_only => (0..paragraph_count(page, shape))
                        .map(|paragraph| Target::Paragraph(shape, paragraph))
                        .collect(),
                    Some(target) => vec![target],
                    None => Vec::new(),
                };
                for target in found {
                    if !targets.contains(&target) {
                        targets.push(target);
                    }
                }
            }
            steps
                .effects
                .extend(targets.into_iter().map(|target| Effect {
                    step: steps.clicks,
                    target,
                    shows,
                }));
        }
        steps
    }

    /// How many clicks the slide's builds take.
    pub fn len(&self) -> usize {
        self.clicks
    }

    /// Whether the slide has no builds to click through.
    pub fn is_empty(&self) -> bool {
        self.clicks == 0
    }

    /// Whether a target is seen after `step` clicks. A paragraph is seen only
    /// where its shape is.
    pub fn visible(&self, step: usize, target: Target) -> bool {
        match target {
            Target::Shape(_) => self.own(step, target),
            Target::Paragraph(shape, _) => {
                self.own(step, Target::Shape(shape)) && self.own(step, target)
            }
        }
    }

    /// The paragraphs of a shape, by their place among its paragraphs, that
    /// are not yet seen, or no longer, after `step` clicks.
    pub fn veiled(&self, step: usize, shape: usize) -> Vec<usize> {
        let mut veiled: Vec<usize> = self
            .effects
            .iter()
            .filter_map(|effect| match effect.target {
                Target::Paragraph(of, paragraph) if of == shape => Some(paragraph),
                _ => None,
            })
            .filter(|&paragraph| !self.own(step, Target::Paragraph(shape, paragraph)))
            .collect();
        veiled.sort_unstable();
        veiled.dedup();
        veiled
    }

    /// Whether the effects on a target alone leave it seen: before its first
    /// one it is the opposite of what that one does.
    fn own(&self, step: usize, target: Target) -> bool {
        let mut on = self.effects.iter().filter(|effect| effect.target == target);
        let Some(first) = on.next() else {
            return true;
        };
        let mut seen = !first.shows;
        for effect in std::iter::once(first).chain(on) {
            if effect.step > step {
                break;
            }
            seen = effect.shows;
        }
        seen
    }
}

/// The main sequence under a timing root.
fn main_sequence(root: &Element) -> Option<&Element> {
    if root.attr(&Ns::Presentation, "node-type") != Some("timing-root") {
        return None;
    }
    root.elements().find(|e| {
        e.is(&Ns::Anim, "seq") && e.attr(&Ns::Presentation, "node-type") == Some("main-sequence")
    })
}

/// The effects under a container, in the order they run: each element that
/// says it starts on a click, with the one before it, or after it.
fn collect<'a>(container: &'a Element, effects: &mut Vec<&'a Element>) {
    for element in container.elements() {
        match element.attr(&Ns::Presentation, "node-type") {
            Some("on-click" | "with-previous" | "after-previous") => effects.push(element),
            _ => collect(element, effects),
        }
    }
}

/// Whether an effect leaves its target shown or hidden: what it sets the
/// visibility to last, or else what its class says. An emphasis or a motion
/// does neither.
fn shows(effect: &Element) -> Option<bool> {
    let mut set = None;
    visibility(effect, &mut set);
    set.or(match effect.attr(&Ns::Presentation, "preset-class") {
        Some("entrance") => Some(true),
        Some("exit") => Some(false),
        _ => None,
    })
}

fn visibility(element: &Element, set: &mut Option<bool>) {
    for child in element.elements() {
        if child.is(&Ns::Anim, "set")
            && child.attr(&Ns::Smil, "attributeName") == Some("visibility")
        {
            match child.attr(&Ns::Smil, "to") {
                Some("visible") => *set = Some(true),
                Some("hidden") => *set = Some(false),
                _ => {}
            }
        }
        visibility(child, set);
    }
}

/// The ids an effect names as its targets, each with whether the effect
/// acts on its text alone, which `anim:sub-item="text"` says.
fn target_ids(effect: &Element) -> Vec<(&str, bool)> {
    let mut ids = Vec::new();
    let mut stack = vec![effect];
    while let Some(element) = stack.pop() {
        if let Some(id) = element.attr(&Ns::Smil, "targetElement") {
            ids.push((id, element.attr(&Ns::Anim, "sub-item") == Some("text")));
        }
        stack.extend(element.elements());
    }
    ids
}

fn has_id(element: &Element, id: &str) -> bool {
    element.attrs.iter().any(|attribute| {
        attribute.value == id
            && (attribute.name.is(&Ns::Draw, "id")
                || attribute.name.is(&Ns::Text, "id")
                || attribute.name.local.as_ref() == "id"
                    && attribute.name.prefix.as_deref() == Some("xml"))
    })
}

/// How many paragraphs the shape at a page index holds; a group, whose
/// members are each drawn alone, holds none of its own.
fn paragraph_count(page: &Element, shape: usize) -> usize {
    let mut paragraphs = Vec::new();
    if let Some((_, shape)) = page.elements_indexed().find(|(i, _)| *i == shape)
        && !shape.is(&Ns::Draw, "g")
    {
        paragraphs_of(shape, &mut paragraphs);
    }
    paragraphs.len()
}

/// The shape or paragraph on a page that an id names. A group is found
/// whole; its members are each drawn alone, and nothing inside it is found.
fn resolve(page: &Element, id: &str) -> Option<Target> {
    page.elements_indexed()
        .filter(|(_, e)| !e.is(&Ns::Presentation, "notes") && !e.is(&Ns::Anim, "par"))
        .find_map(|(index, shape)| {
            if has_id(shape, id) {
                return Some(Target::Shape(index));
            }
            if shape.is(&Ns::Draw, "g") {
                return None;
            }
            let mut paragraphs = Vec::new();
            paragraphs_of(shape, &mut paragraphs);
            paragraphs
                .iter()
                .position(|paragraph| has_id(paragraph, id))
                .map(|paragraph| Target::Paragraph(index, paragraph))
        })
}

/// A shape's paragraphs and headings, in the order they are drawn.
fn paragraphs_of<'a>(element: &'a Element, paragraphs: &mut Vec<&'a Element>) {
    for child in element.elements() {
        if child.is(&Ns::Text, "p") || child.is(&Ns::Text, "h") {
            paragraphs.push(child);
        } else {
            paragraphs_of(child, paragraphs);
        }
    }
}
