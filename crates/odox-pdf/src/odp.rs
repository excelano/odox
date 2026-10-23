//! A presentation laid out a page per slide, at the slide's size.
//!
//! A slide is drawn as the window draws it (`odox-ui`'s shapes): its ground,
//! the master page's decorations, then its own shapes back to front, each
//! filled, outlined and labelled, and a shape the window leaves undrawn is
//! left out here too. What is drawn and what is read are in different orders:
//! drawing goes back to front, and reading goes title first and then the
//! slide's own shapes in the order the document lists them. Everything from
//! the master page is decoration. A slide left out of a show is left out of
//! the export. DESIGN.md §12.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use krilla::paint::{LinearGradient, Paint, SpreadMethod, Stop};
use krilla::tagging::Tag;
use odox_core::doc::{Presentation, Slide};
use odox_core::draw::Geometry;
use odox_core::edit::{self, Description};
use odox_core::{
    Anchor, Color, Document, Element, Family, Fill, GradientStyle, Length, Ns, Properties,
    Transform,
};

use crate::fonts::Faces;
use crate::layout::{Ground, Item, Layout, Segment};
use crate::model::{self, Block};
use crate::tags::NodeId;
use crate::{Exported, Laid, Options, Page, Refusal};

/// The figures on the slides that are shown which say nothing in place of
/// their picture, as paths from the content root.
pub(crate) fn undescribed(presentation: &Presentation) -> Vec<Vec<usize>> {
    let document = &presentation.document;
    let mut found = Vec::new();
    for slide in shown(presentation) {
        let Some(page) = presentation.page_path(slide.position) else {
            continue;
        };
        for (index, shape) in slide.shapes_indexed() {
            let path = [page.clone(), vec![index]].concat();
            collect_undescribed(document, shape, path, &mut found);
        }
    }
    found
}

fn collect_undescribed(
    document: &Document,
    shape: &Element,
    path: Vec<usize>,
    into: &mut Vec<Vec<usize>>,
) {
    if shape.is(&Ns::Draw, "g") {
        for (index, child) in shape.elements_indexed() {
            collect_undescribed(document, child, [path.clone(), vec![index]].concat(), into);
        }
        return;
    }
    if edit::is_figure(shape) {
        if picture(document, shape).is_some()
            && edit::description(shape, &document.styles) == Description::Missing
        {
            into.push(path);
        }
        return;
    }
    if placement(shape).is_some_and(Placement::is_upright)
        && let Some((content, content_path)) = label(shape, &path)
    {
        model::undescribed(&model::read(document, content, content_path, None), into);
    }
}

/// The slides that are shown.
fn shown(presentation: &Presentation) -> impl Iterator<Item = Slide<'_>> {
    presentation
        .slides()
        .into_iter()
        .filter(|slide| !presentation.is_hidden(slide))
}

/// Export a presentation as PDF/UA-1.
pub(crate) fn export(presentation: &Presentation, options: &Options) -> Result<Exported, Refusal> {
    let missing = undescribed(presentation);
    if !missing.is_empty() {
        return Err(Refusal::Undescribed(missing));
    }
    let document = &presentation.document;
    let mut faces = Faces::new();
    let laid = lay_out(presentation, options, &mut faces)?;
    crate::write(
        laid,
        faces,
        &crate::About {
            title: crate::title(document, options),
            language: crate::language(document, "graphic")
                .unwrap_or_else(|| options.language.clone()),
            creator: "Odox Deck",
        },
    )
}

/// The slides that are shown, a page each.
fn lay_out(
    presentation: &Presentation,
    options: &Options,
    faces: &mut Faces,
) -> Result<Laid, Refusal> {
    let document = &presentation.document;
    let mut layout = Layout::new(faces, 0.0);
    let mut pages = Vec::new();
    let mut headings = Vec::new();
    for (number, slide) in presentation.slides().iter().enumerate() {
        if presentation.is_hidden(slide) {
            continue;
        }
        let size = presentation.page_layout(slide);
        let (width, height) = (size.width.points(), size.height.points());
        layout.page_height = height;
        let mut page = Page::new(width, height);
        let mut slides = Slides {
            layout: &mut layout,
            document,
        };

        let whole = Placement {
            origin: (0.0, 0.0),
            x: (width, 0.0),
            y: (0.0, height),
        };
        let ground = slides.ground(&presentation.background(slide), None, whole.bounds());
        if let Some(item) = slides.outline(&whole.corners(), true, ground, None, None) {
            page.items.push((0.0, item));
        }

        // Decoration: laid out under a group of its own that nothing drawn
        // is read under, so that the tree drops it.
        let decoration = slides.layout.tree.group(None, Tag::Div);
        for shape in presentation.background_objects(slide) {
            for item in slides.shape(shape, decoration, false)? {
                if !matches!(item, Item::Link { .. } | Item::Empty { .. }) {
                    page.items.push((0.0, item.decoration()));
                }
            }
        }

        let section = slides.layout.tree.group(None, Tag::Section);
        let shapes: Vec<&Element> = slide.shapes().collect();
        let is_title = |shape: &Element| shape.attr(&Ns::Presentation, "class") == Some("title");
        let mut drawn: Vec<Vec<Item>> = vec![Vec::new(); shapes.len()];
        let mut title = None;
        let reading = (0..shapes.len())
            .filter(|&at| is_title(shapes[at]))
            .chain((0..shapes.len()).filter(|&at| !is_title(shapes[at])));
        for at in reading {
            let heading = is_title(shapes[at]) && title.is_none();
            drawn[at] = slides.shape(shapes[at], section, heading)?;
            if heading {
                title = label_text(document, shapes[at]).filter(|text| !text.is_empty());
            }
        }
        page.items
            .extend(drawn.into_iter().flatten().map(|item| (0.0, item)));

        let title = title.unwrap_or_else(|| {
            options
                .untitled_slide
                .replace("{number}", &(number + 1).to_string())
        });
        headings.push((1, title, pages.len(), 0.0));
        pages.push(page);
    }
    if pages.is_empty() {
        return Err(Refusal::Empty);
    }
    Ok(Laid {
        pages,
        tree: layout.tree,
        headings,
    })
}

/// The text a shape's label holds, as one line: what a slide's title is
/// called in the outline.
fn label_text(document: &Document, shape: &Element) -> Option<String> {
    let (content, _) = label(shape, &[])?;
    let words: Vec<String> = model::read(document, content, Vec::new(), None)
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph(paragraph) => Some(paragraph.text.trim().to_owned()),
            _ => None,
        })
        .filter(|text| !text.is_empty())
        .collect();
    Some(words.join(" "))
}

/// Where a shape's own box lands on the slide: a corner and the two vectors
/// along the edges that meet there, which is a rectangle square to the page
/// unless `draw:transform` turns or leans it.
#[derive(Clone, Copy)]
struct Placement {
    origin: (f32, f32),
    x: (f32, f32),
    y: (f32, f32),
}

impl Placement {
    /// A point of the box, in the fractions of it across and down.
    fn across(self, u: f32, v: f32) -> (f32, f32) {
        (
            self.origin.0 + self.x.0 * u + self.y.0 * v,
            self.origin.1 + self.x.1 * u + self.y.1 * v,
        )
    }

    fn corners(self) -> [(f32, f32); 4] {
        [
            self.across(0.0, 0.0),
            self.across(1.0, 0.0),
            self.across(1.0, 1.0),
            self.across(0.0, 1.0),
        ]
    }

    /// The smallest upright rectangle the box fits inside, as `(x, y, width,
    /// height)`.
    fn bounds(self) -> (f32, f32, f32, f32) {
        let corners = self.corners();
        let (mut left, mut top) = (f32::MAX, f32::MAX);
        let (mut right, mut bottom) = (f32::MIN, f32::MIN);
        for (x, y) in corners {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
        (left, top, right - left, bottom - top)
    }

    fn is_upright(self) -> bool {
        self.x.1.abs() < 0.01 && self.y.0.abs() < 0.01 && self.x.0 >= 0.0 && self.y.1 >= 0.0
    }
}

/// Where a shape's box is, from its corner and size or from the transform
/// that places it instead.
fn placement(shape: &Element) -> Option<Placement> {
    let at = |local: &str| shape.attr(&Ns::Svg, local).and_then(Length::parse);
    let width = at("width").map_or(0.0, Length::points);
    let height = at("height").map_or(0.0, Length::points);
    if let Some(transform) = shape
        .attr(&Ns::Draw, "transform")
        .and_then(Transform::parse)
    {
        let left = at("x").map_or(0.0, Length::points);
        let top = at("y").map_or(0.0, Length::points);
        let corner =
            |u: f32, v: f32| transform.apply((width.mul_add(u, left), height.mul_add(v, top)));
        let origin = corner(0.0, 0.0);
        let x = corner(1.0, 0.0);
        let y = corner(0.0, 1.0);
        return Some(Placement {
            origin,
            x: (x.0 - origin.0, x.1 - origin.1),
            y: (y.0 - origin.0, y.1 - origin.1),
        });
    }
    Some(Placement {
        origin: (at("x")?.points(), at("y")?.points()),
        x: (width, 0.0),
        y: (0.0, height),
    })
}

fn point(shape: &Element, x: &str, y: &str) -> Option<(f32, f32)> {
    let at = |local: &str| shape.attr(&Ns::Svg, local).and_then(Length::parse);
    Some((at(x)?.points(), at(y)?.points()))
}

/// What a shape is labelled with, and its path: the paragraphs of a frame's
/// text box, or a drawing shape's own.
fn label<'a>(shape: &'a Element, path: &[usize]) -> Option<(&'a Element, Vec<usize>)> {
    if let Some((index, text_box)) = shape
        .elements_indexed()
        .find(|(_, e)| e.is(&Ns::Draw, "text-box"))
    {
        return Some((text_box, [path, &[index]].concat()));
    }
    (shape.child(&Ns::Text, "p").is_some() || shape.child(&Ns::Text, "list").is_some())
        .then(|| (shape, path.to_vec()))
}

/// A frame's picture: the first of the ones it offers that can be embedded,
/// in the producer's order.
fn picture(document: &Document, frame: &Element) -> Option<krilla::image::Image> {
    frame
        .elements()
        .filter(|child| child.is(&Ns::Draw, "image"))
        .find_map(|image| model::decode(document.picture(image.attr(&Ns::Xlink, "href")?)?))
}

/// The style a shape names, resolved: a `presentation` style for one of the
/// slide's own placeholders, a `graphic` style for a drawing.
fn style_of(document: &Document, shape: &Element) -> std::rc::Rc<Properties> {
    if let Some(name) = shape.attr(&Ns::Presentation, "style-name") {
        return document.styles.resolve(&Family::Presentation, name);
    }
    let name = shape.attr(&Ns::Draw, "style-name").unwrap_or_default();
    document.styles.resolve(&Family::Graphic, name)
}

/// A slide's shapes turned into what is drawn for them.
struct Slides<'l, 'f, 'd> {
    layout: &'l mut Layout<'f>,
    document: &'d Document,
}

impl Slides<'_, '_, '_> {
    /// One shape, read under a group: its fill, its outline and its label or
    /// picture, in the order they are drawn. The first paragraph of a
    /// heading's label is the slide's title.
    #[allow(clippy::too_many_lines)]
    fn shape(
        &mut self,
        shape: &Element,
        parent: NodeId,
        heading: bool,
    ) -> Result<Vec<Item>, Refusal> {
        let mut items = Vec::new();
        if shape.is(&Ns::Draw, "g") {
            for child in shape.elements() {
                items.extend(self.shape(child, parent, false)?);
            }
            return Ok(items);
        }

        let properties = style_of(self.document, shape);
        let stroke = properties.graphic.stroke.map(|color| {
            let width = properties
                .graphic
                .stroke_width
                .map_or(1.0, Length::points)
                .max(0.5);
            (color, width)
        });
        let fill = properties.graphic.fill();
        let opacity = properties.graphic.opacity;
        // A shape with nothing to read that says what it shows is a figure,
        // and its outlines are what is read; any other outline is decoration.
        // An empty paragraph is nothing to read: `LibreOffice` writes one
        // into every shape.
        let figure = match edit::description(shape, &self.document.styles) {
            Description::Text(alt)
                if !edit::is_figure(shape)
                    && label_text(self.document, shape).is_none_or(|text| text.is_empty()) =>
            {
                Some(self.layout.tree.group(Some(parent), Tag::Figure(Some(alt))))
            }
            _ => None,
        };

        if shape.is(&Ns::Draw, "line") {
            if let (Some(from), Some(to)) = (point(shape, "x1", "y1"), point(shape, "x2", "y2")) {
                items.extend(self.outline(&[from, to], false, None, stroke, figure));
            }
            return Ok(items);
        }

        if shape.is(&Ns::Draw, "connector") {
            let (Some(from), Some(to)) = (point(shape, "x1", "y1"), point(shape, "x2", "y2"))
            else {
                return Ok(items);
            };
            let place = Placement {
                origin: (from.0.min(to.0), from.1.min(to.1)),
                x: ((to.0 - from.0).abs(), 0.0),
                y: (0.0, (to.1 - from.1).abs()),
            };
            match Geometry::read_path(shape) {
                Some(mut geometry) => {
                    geometry.refit();
                    self.geometry(&geometry, place, None, stroke, figure, &mut items);
                }
                None => {
                    items.extend(self.outline(&[from, to], false, None, stroke, figure));
                }
            }
            items.extend(self.text(shape, place, parent, heading)?);
            return Ok(items);
        }

        let Some(place) = placement(shape) else {
            return Ok(items);
        };
        let bounds = place.bounds();
        let ground = |slides: &Self| slides.ground(&fill, opacity, bounds);

        if shape.is(&Ns::Draw, "polygon") || shape.is(&Ns::Draw, "polyline") {
            let points = points(shape, place);
            if points.len() >= 2 {
                let closed = shape.is(&Ns::Draw, "polygon");
                let ground = if closed { ground(self) } else { None };
                items.extend(self.outline(&points, closed, ground, stroke, figure));
            }
            return Ok(items);
        }

        if shape.is(&Ns::Draw, "ellipse") || shape.is(&Ns::Draw, "circle") {
            let ground = ground(self);
            let span = (bounds.1, bounds.1 + bounds.3);
            items.extend(self.shaped(ellipse(place), span, ground, stroke, figure));
        } else if shape.is(&Ns::Draw, "path") {
            if let Some(geometry) = Geometry::read_path(shape) {
                let ground = ground(self);
                self.geometry(&geometry, place, ground, stroke, figure, &mut items);
            }
        } else if shape.is(&Ns::Draw, "custom-shape") {
            if let Some(geometry) = shape
                .child(&Ns::Draw, "enhanced-geometry")
                .and_then(Geometry::read)
            {
                let ground = ground(self);
                self.geometry(&geometry, place, ground, stroke, figure, &mut items);
            }
        } else if shape.is(&Ns::Draw, "rect") || shape.is(&Ns::Draw, "frame") {
            let ground = ground(self);
            items.extend(self.outline(&place.corners(), true, ground, stroke, figure));
        } else {
            return Ok(items);
        }
        items.extend(self.text(shape, place, parent, heading)?);
        Ok(items)
    }

    /// A shape's label, or the picture it frames.
    ///
    /// A turned shape keeps its picture and loses its label, as in the
    /// window: a paragraph drawn upright inside a box that is not would say
    /// something the document does not.
    fn text(
        &mut self,
        shape: &Element,
        place: Placement,
        parent: NodeId,
        heading: bool,
    ) -> Result<Vec<Item>, Refusal> {
        let (x, y, width, height) = place.bounds();
        if edit::is_figure(shape) {
            let Some(image) = picture(self.document, shape) else {
                return Ok(Vec::new());
            };
            let leaf = match edit::description(shape, &self.document.styles) {
                Description::Text(alt) => {
                    let node = self.layout.tree.group(Some(parent), Tag::Figure(Some(alt)));
                    Some(self.layout.tree.leaf(node))
                }
                Description::Decorative | Description::Missing => None,
            };
            return Ok(vec![if place.is_upright() {
                Item::Image {
                    x,
                    y,
                    width,
                    height,
                    image,
                    leaf,
                }
            } else {
                Item::Placed {
                    origin: place.origin,
                    across: place.x,
                    down: place.y,
                    image,
                    leaf,
                }
            }]);
        }
        let Some((content, path)) = label(shape, &[]) else {
            return Ok(Vec::new());
        };
        if !place.is_upright() {
            return Ok(Vec::new());
        }
        let style = style_of(self.document, shape);
        let mut blocks = model::read(
            self.document,
            content,
            path,
            Some(std::rc::Rc::clone(&style)),
        );
        if heading && let Some(Block::Paragraph(paragraph)) = blocks.first_mut() {
            paragraph.heading = Some(1);
        }
        let mut pieces = Vec::new();
        self.layout
            .blocks(&blocks, x, width.max(1.0), Some(parent), &mut pieces)?;

        let mut placed = Vec::new();
        let mut at = 0.0;
        for piece in pieces {
            at += piece.before;
            placed.push((at, piece.items));
            at += piece.height + piece.after;
        }
        let anchor = style.graphic.text_anchor.unwrap_or(Anchor::Top);
        let top = y + (height - at.min(height)) * anchor.share();
        Ok(placed
            .into_iter()
            .flat_map(|(below, items)| {
                items
                    .into_iter()
                    .map(move |item| item.shifted(0.0, top + below))
            })
            .collect())
    }

    /// A custom shape's or a path's outlines, mapped from their own space
    /// onto the box. A connector's route is given no ground at all: it has no
    /// area, whatever its style says.
    fn geometry(
        &mut self,
        geometry: &Geometry,
        place: Placement,
        ground: Option<Ground>,
        stroke: Option<(Color, f32)>,
        figure: Option<NodeId>,
        into: &mut Vec<Item>,
    ) {
        let view = geometry.view;
        for sub in &geometry.paths {
            let points: Vec<(f32, f32)> = sub
                .points
                .iter()
                .map(|&(x, y)| place.across((x - view.x) / view.width, (y - view.y) / view.height))
                .collect();
            if points.len() < 2 {
                continue;
            }
            let filled = ground.clone().filter(|_| sub.fill);
            let pen = stroke.filter(|_| sub.stroke);
            into.extend(self.outline(&points, sub.closed, filled, pen, figure));
        }
    }

    /// An outline, filled and stroked as it is given, and read as part of a
    /// figure where it is one.
    fn shaped(
        &mut self,
        outline: Vec<Segment>,
        span: (f32, f32),
        ground: Option<Ground>,
        stroke: Option<(Color, f32)>,
        figure: Option<NodeId>,
    ) -> Option<Item> {
        if ground.is_none() && stroke.is_none() {
            return None;
        }
        Some(Item::Shape {
            outline,
            span,
            ground,
            stroke,
            leaf: figure.map(|node| self.layout.tree.leaf(node)),
        })
    }

    /// A run of points as an outline, closed or open. Only a closed one is
    /// filled.
    fn outline(
        &mut self,
        points: &[(f32, f32)],
        closed: bool,
        ground: Option<Ground>,
        stroke: Option<(Color, f32)>,
        figure: Option<NodeId>,
    ) -> Option<Item> {
        let (&(x, y), rest) = points.split_first()?;
        let mut outline = vec![Segment::Move(x, y)];
        outline.extend(rest.iter().map(|&(x, y)| Segment::Line(x, y)));
        if closed {
            outline.push(Segment::Close);
        }
        let span = points
            .iter()
            .fold((f32::MAX, f32::MIN), |(top, bottom), &(_, y)| {
                (top.min(y), bottom.max(y))
            });
        self.shaped(outline, span, ground.filter(|_| closed), stroke, figure)
    }

    /// What fills a shape, over the rectangle a gradient or a picture runs
    /// across.
    fn ground(
        &self,
        fill: &Fill,
        opacity: Option<f32>,
        (x, y, width, height): (f32, f32, f32, f32),
    ) -> Option<Ground> {
        let opacity = opacity.unwrap_or(1.0).clamp(0.0, 1.0);
        match fill {
            Fill::None => None,
            Fill::Solid(color) => Some(Ground::Paint(crate::rgb(*color).into(), opacity)),
            Fill::Image(name) => {
                let href = self.document.styles.fill_image(name)?;
                let image = model::decode(self.document.picture(href)?)?;
                Some(Ground::Image(image, (x, y, width, height), opacity))
            }
            Fill::Gradient(name) => {
                let gradient = self.document.styles.gradient(name)?;
                let paint: Paint = match gradient.style {
                    GradientStyle::Linear | GradientStyle::Axial => {
                        // As the window runs it: along the direction ODF
                        // gives, across the rectangle's own extent, the start
                        // colour held for the border before the blend begins.
                        let radians = gradient.angle.to_radians();
                        let axis = (radians.sin(), -radians.cos());
                        let centre = (x + width / 2.0, y + height / 2.0);
                        let projections = [
                            (x, y),
                            (x + width, y),
                            (x + width, y + height),
                            (x, y + height),
                        ]
                        .map(|(cx, cy)| (cx - centre.0) * axis.0 + (cy - centre.1) * axis.1);
                        let low = projections.iter().copied().fold(f32::MAX, f32::min);
                        let high = projections.iter().copied().fold(f32::MIN, f32::max);
                        let border = gradient.border.clamp(0.0, 0.99);
                        let stop = |offset: f32, color: Color| Stop {
                            offset: krilla::num::NormalizedF32::new(offset.clamp(0.0, 1.0))
                                .unwrap_or(krilla::num::NormalizedF32::ZERO),
                            color: crate::rgb(color).into(),
                            opacity: krilla::num::NormalizedF32::ONE,
                        };
                        let stops = if gradient.style == GradientStyle::Axial {
                            vec![
                                stop(0.0, gradient.end),
                                stop(border, gradient.end),
                                stop(border + (1.0 - border) / 2.0, gradient.start),
                                stop(1.0, gradient.end),
                            ]
                        } else {
                            vec![
                                stop(0.0, gradient.start),
                                stop(border, gradient.start),
                                stop(1.0, gradient.end),
                            ]
                        };
                        LinearGradient {
                            x1: centre.0 + axis.0 * low,
                            y1: centre.1 + axis.1 * low,
                            x2: centre.0 + axis.0 * high,
                            y2: centre.1 + axis.1 * high,
                            transform: krilla::geom::Transform::identity(),
                            spread_method: SpreadMethod::Pad,
                            stops,
                            anti_alias: false,
                        }
                        .into()
                    }
                    // The window draws the gradients that radiate from a point
                    // as the even mix of their two colours.
                    _ => crate::rgb(blend(gradient.start, gradient.end)).into(),
                };
                Some(Ground::Paint(paint, opacity))
            }
        }
    }
}

/// The even mix of two colours.
fn blend(from: Color, to: Color) -> Color {
    #[allow(clippy::cast_possible_truncation)]
    let mix = |a: u8, b: u8| ((u16::from(a) + u16::from(b)) / 2) as u8;
    Color {
        r: mix(from.r, to.r),
        g: mix(from.g, to.g),
        b: mix(from.b, to.b),
    }
}

/// A polygon's points, mapped from the space `svg:viewBox` sets up onto the
/// box it occupies.
fn points(shape: &Element, place: Placement) -> Vec<(f32, f32)> {
    let view: Vec<f32> = shape
        .attr(&Ns::Svg, "viewBox")
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    let [left, top, width, height] = view[..] else {
        return Vec::new();
    };
    if width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }
    shape
        .attr(&Ns::Draw, "points")
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|pair| {
            let (x, y) = pair.split_once(',')?;
            let x: f32 = x.trim().parse().ok()?;
            let y: f32 = y.trim().parse().ok()?;
            Some(place.across((x - left) / width, (y - top) / height))
        })
        .collect()
}

/// The ellipse inscribed in a box, as four curves, which a turned box maps
/// as it maps any other point.
fn ellipse(place: Placement) -> Vec<Segment> {
    // How far along a tangent a quarter circle's control points sit.
    const KAPPA: f32 = 0.552_284_8;
    let at = |u: f32, v: f32| place.across(0.5 + u / 2.0, 0.5 + v / 2.0);
    let start = at(1.0, 0.0);
    let mut outline = vec![Segment::Move(start.0, start.1)];
    for [(a, b), (c, d), (e, f)] in [
        [(1.0, KAPPA), (KAPPA, 1.0), (0.0, 1.0)],
        [(-KAPPA, 1.0), (-1.0, KAPPA), (-1.0, 0.0)],
        [(-1.0, -KAPPA), (-KAPPA, -1.0), (0.0, -1.0)],
        [(KAPPA, -1.0), (1.0, -KAPPA), (1.0, 0.0)],
    ] {
        let (one, two, end) = (at(a, b), at(c, d), at(e, f));
        outline.push(Segment::Cubic(one.0, one.1, two.0, two.1, end.0, end.1));
    }
    outline.push(Segment::Close);
    outline
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn fixture() -> Presentation {
        let bytes = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/export.odp"),
        )
        .expect("the export fixture");
        Presentation::read(&bytes).expect("it reads")
    }

    fn options() -> Options {
        Options {
            title: "Untitled".to_owned(),
            language: "en-US".to_owned(),
            untitled_slide: "Slide {number}".to_owned(),
        }
    }

    fn names(presentation: &Presentation, paths: &[Vec<usize>]) -> Vec<String> {
        paths
            .iter()
            .filter_map(|path| presentation.document.content.at(path))
            .filter_map(|frame| frame.attr(&Ns::Draw, "name"))
            .map(ToOwned::to_owned)
            .collect()
    }

    #[test]
    fn only_a_picture_on_a_shown_slide_that_says_nothing_is_asked_about() {
        let presentation = fixture();
        assert_eq!(
            names(&presentation, &undescribed(&presentation)),
            ["Undescribed"]
        );
    }

    #[test]
    fn a_hidden_slide_is_left_out_and_an_untitled_one_is_named_by_its_number() {
        let mut presentation = fixture();
        for path in undescribed(&presentation) {
            let Document {
                content, styles, ..
            } = &mut presentation.document;
            edit::set_decorative(content, &path, styles).expect("marked");
        }
        let mut faces = Faces::new();
        let laid = lay_out(&presentation, &options(), &mut faces).expect("laid out");
        assert_eq!(laid.pages.len(), 3);
        let titles: Vec<&str> = laid.headings.iter().map(|h| h.1.as_str()).collect();
        assert_eq!(titles, ["Exporting slides", "Pictures", "Slide 3"]);
    }

    #[test]
    fn every_slide_hidden_is_nothing_to_export() {
        let mut presentation = fixture();
        let hidden = presentation
            .slides()
            .iter()
            .find(|slide| presentation.is_hidden(slide))
            .and_then(|slide| slide.style_name().map(ToOwned::to_owned))
            .expect("the fixture hides a slide");
        let positions: Vec<usize> = presentation.slides().iter().map(|s| s.position).collect();
        for position in positions {
            let name = presentation.document.name(&Ns::Draw, "style-name");
            presentation
                .page_mut(position)
                .expect("a slide")
                .set_attr(name, &hidden);
        }
        let mut faces = Faces::new();
        assert!(matches!(
            lay_out(&presentation, &options(), &mut faces),
            Err(Refusal::Empty)
        ));
    }
}
