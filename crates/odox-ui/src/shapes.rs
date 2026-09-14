//! Drawing ODF's shapes: what a slide is made of, and what a text document or a
//! spreadsheet can carry in a frame.
//!
//! A shape gives its own position and size in the coordinate space of the page
//! it is on, so nothing here lays anything out. What it does is map that space
//! onto a rectangle on screen, resolve the style the shape names, and paint the
//! fill, the outline and the text in that order.
//!
//! **What is drawn is what a fixture proves.** ODF's shape vocabulary is far
//! larger than this: `draw:custom-shape` alone carries a small vector language
//! in `draw:enhanced-geometry`, with formulas and named equations. A shape this
//! cannot draw is left undrawn rather than approximated into something the
//! document does not say, and [`Canvas::shape`] names the ones that are.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{Color32, Mesh, Pos2, Rect, Shape, Stroke, Ui, UiBuilder, pos2, vec2};
use odox_core::{
    Color, Document, Element, Family, Fill, Gradient, GradientStyle, Length, Ns, Properties,
};

use crate::flow::{Flow, Pictures};
use crate::format::{self, Palette};

/// A page, and where on screen it is being drawn.
pub struct Canvas<'a> {
    /// The document, for its styles and its pictures.
    pub document: &'a Document,
    /// Pictures decoded so far.
    pub pictures: &'a mut Pictures,
    /// The rectangle the page occupies on screen.
    pub page: Rect,
    /// Screen points per ODF point.
    pub scale: f32,
    /// The colours to draw in where the document names none.
    pub palette: Palette,
}

/// The `draw:enhanced-geometry` types drawn as the rectangle they occupy.
///
/// A custom shape is a path built from formulas, and this draws none of them.
/// What it does is fill the bounding box of the few whose outline is a rectangle
/// or close enough to one that the difference is a corner — which is every type
/// in the corpus. Anything else is left undrawn, because filling the box of a
/// star or an arrow would put a block on the slide where the document asked for
/// a shape.
const BOXY: [&str; 5] = [
    "rectangle",
    "round-rectangle",
    "flowchart-process",
    "flowchart-alternate-process",
    "flowchart-document",
];

impl Canvas<'_> {
    /// Fill the page.
    pub fn background(&self, ui: &Ui, fill: &Fill) {
        let page = self.page;
        self.fill(
            ui,
            page,
            fill,
            None,
            &[
                page.left_top(),
                page.right_top(),
                page.right_bottom(),
                page.left_bottom(),
            ],
        );
    }

    /// One shape, at the place on the page the document puts it.
    ///
    /// Drawn: `draw:rect`, `draw:ellipse`, `draw:circle`, `draw:polygon`,
    /// `draw:polyline`, `draw:line`, the boxy custom shapes, a `draw:frame`
    /// holding a picture or a text box, and `draw:g`, which is a group and is
    /// descended into. Left undrawn: `draw:path`, `draw:connector`, `draw:measure`
    /// and every custom shape that is not a box.
    pub fn shape(&mut self, ui: &mut Ui, shape: &Element) {
        if shape.is(&Ns::Draw, "g") {
            for child in shape.elements() {
                self.shape(ui, child);
            }
            return;
        }

        let properties = self.style_of(shape);
        let outline = self.stroke(&properties);

        // A line is the one shape positioned by its two ends rather than by a
        // corner and a size.
        if shape.is(&Ns::Draw, "line") {
            let ends = [self.point(shape, "x1", "y1"), self.point(shape, "x2", "y2")];
            if let ([Some(from), Some(to)], Some(stroke)) = ([ends[0], ends[1]], outline) {
                self.painter(ui)
                    .add(Shape::line_segment([from, to], stroke));
            }
            return;
        }

        let Some(rect) = self.rect(shape) else { return };

        if shape.is(&Ns::Draw, "polygon") || shape.is(&Ns::Draw, "polyline") {
            let points = points(shape, rect);
            if points.len() >= 2 {
                if shape.is(&Ns::Draw, "polygon") {
                    self.fill(
                        ui,
                        rect,
                        &properties.graphic.fill,
                        properties.graphic.opacity,
                        &points,
                    );
                    if let Some(stroke) = outline {
                        self.painter(ui).add(Shape::closed_line(points, stroke));
                    }
                } else if let Some(stroke) = outline {
                    self.painter(ui).add(Shape::line(points, stroke));
                }
            }
            return;
        }

        if shape.is(&Ns::Draw, "ellipse") || shape.is(&Ns::Draw, "circle") {
            let (centre, radius) = (rect.center(), rect.size() / 2.0);
            if let Some(colour) = self.flat(&properties.graphic.fill, properties.graphic.opacity) {
                self.painter(ui)
                    .add(Shape::ellipse_filled(centre, radius, colour));
            }
            if let Some(stroke) = outline {
                self.painter(ui)
                    .add(Shape::ellipse_stroke(centre, radius, stroke));
            }
            return;
        }

        let boxy = shape.is(&Ns::Draw, "rect")
            || shape.is(&Ns::Draw, "frame")
            || (shape.is(&Ns::Draw, "custom-shape")
                && shape
                    .child(&Ns::Draw, "enhanced-geometry")
                    .and_then(|geometry| geometry.attr(&Ns::Draw, "type"))
                    .is_some_and(|kind| BOXY.contains(&kind)));
        if !boxy {
            return;
        }

        let corners = [
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
        ];
        self.fill(
            ui,
            rect,
            &properties.graphic.fill,
            properties.graphic.opacity,
            &corners,
        );
        if let Some(stroke) = outline {
            self.painter(ui)
                .add(Shape::closed_line(corners.to_vec(), stroke));
        }
        self.text(ui, shape, rect);
    }

    /// The paragraphs a shape holds, or the picture it frames.
    fn text(&mut self, ui: &mut Ui, shape: &Element, rect: Rect) {
        let content = shape
            .child(&Ns::Draw, "text-box")
            .or_else(|| shape.child(&Ns::Draw, "image").map(|_| shape));
        let Some(content) = content else { return };
        let content = content.clone();
        let (page, scale, palette) = (self.page, self.scale, self.palette);
        let document = self.document;
        let pictures = &mut *self.pictures;
        ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
            ui.set_clip_rect(rect.intersect(page));
            let mut flow = Flow::new(document, pictures, scale);
            flow.palette = palette;
            flow.blocks(ui, &content, rect.width());
        });
    }

    /// The style a shape names, resolved.
    ///
    /// A shape on a slide names a `presentation` style where it is one of the
    /// slide's own frames and a `graphic` style where it is a drawing; a master
    /// page's decorations are the second kind. Both chains end in the same
    /// properties.
    fn style_of(&self, shape: &Element) -> std::rc::Rc<Properties> {
        if let Some(name) = shape.attr(&Ns::Presentation, "style-name") {
            return self.document.styles.resolve(&Family::Presentation, name);
        }
        let name = shape.attr(&Ns::Draw, "style-name").unwrap_or_default();
        self.document.styles.resolve(&Family::Graphic, name)
    }

    /// Everything is clipped to the page: a master page's decorations are
    /// routinely wider than the slide they decorate.
    fn painter(&self, ui: &Ui) -> eframe::egui::Painter {
        ui.painter()
            .with_clip_rect(self.page.intersect(ui.clip_rect()))
    }

    /// Where a shape sits on screen, from its position and size on the page.
    fn rect(&self, shape: &Element) -> Option<Rect> {
        let at = |local: &str| shape.attr(&Ns::Svg, local).and_then(Length::parse);
        let (x, y) = (at("x")?, at("y")?);
        let width = at("width").map_or(0.0, |w| w.points() * self.scale);
        let height = at("height").map_or(0.0, |h| h.points() * self.scale);
        Some(Rect::from_min_size(
            pos2(
                self.page.left() + x.points() * self.scale,
                self.page.top() + y.points() * self.scale,
            ),
            vec2(width, height),
        ))
    }

    fn point(&self, shape: &Element, x: &str, y: &str) -> Option<Pos2> {
        let at = |local: &str| shape.attr(&Ns::Svg, local).and_then(Length::parse);
        Some(pos2(
            self.page.left() + at(x)?.points() * self.scale,
            self.page.top() + at(y)?.points() * self.scale,
        ))
    }

    fn stroke(&self, properties: &Properties) -> Option<Stroke> {
        let colour = properties.graphic.stroke?;
        let width = properties
            .graphic
            .stroke_width
            .map_or(1.0, |w| w.points() * self.scale)
            .max(1.0);
        Some(Stroke::new(width, format::color32(colour)))
    }

    /// Paint a fill over a shape whose outline is the given points.
    ///
    /// A solid fill is one shape. A gradient is a mesh, because that is how a
    /// colour varies across a triangle in a graphics toolkit: the corners carry
    /// the colour the gradient has at each of them and the interpolation between
    /// them is what draws it.
    fn fill(&self, ui: &Ui, rect: Rect, fill: &Fill, opacity: Option<f32>, points: &[Pos2]) {
        match fill {
            Fill::None => {}
            Fill::Solid(colour) => {
                let colour = alpha(format::color32(*colour), opacity);
                self.painter(ui)
                    .add(Shape::convex_polygon(points.to_vec(), colour, Stroke::NONE));
            }
            Fill::Gradient(name) => {
                let Some(gradient) = self.document.styles.gradient(name) else {
                    return;
                };
                self.painter(ui).add(gradient_mesh(rect, gradient, opacity));
            }
        }
    }

    /// One colour for a fill, where the shape being drawn cannot carry a mesh.
    fn flat(&self, fill: &Fill, opacity: Option<f32>) -> Option<Color32> {
        match fill {
            Fill::None => None,
            Fill::Solid(colour) => Some(alpha(format::color32(*colour), opacity)),
            Fill::Gradient(name) => {
                let gradient = self.document.styles.gradient(name)?;
                Some(alpha(blend(gradient.start, gradient.end, 0.5), opacity))
            }
        }
    }
}

/// A polygon's points, mapped from the coordinate space it declares onto the
/// rectangle it occupies.
///
/// `draw:points` is in the space `svg:viewBox` sets up, which is a shape's
/// own and has nothing to do with the page's: a polygon 13.5cm wide states
/// its points out of 13501. Without the mapping every polygon collapses into
/// the top left corner.
fn points(shape: &Element, rect: Rect) -> Vec<Pos2> {
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
            Some(pos2(
                rect.left() + (x - left) / width * rect.width(),
                rect.top() + (y - top) / height * rect.height(),
            ))
        })
        .collect()
}

fn alpha(colour: Color32, opacity: Option<f32>) -> Color32 {
    match opacity {
        Some(opacity) if opacity < 1.0 => colour.gamma_multiply(opacity),
        _ => colour,
    }
}

fn blend(from: Color, to: Color, t: f32) -> Color32 {
    let mix = |a: u8, b: u8| {
        let a = f32::from(a);
        let b = f32::from(b);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            (a + (b - a) * t).round().clamp(0.0, 255.0) as u8
        }
    };
    Color32::from_rgb(mix(from.r, to.r), mix(from.g, to.g), mix(from.b, to.b))
}

/// A gradient across a rectangle, as two triangles with coloured corners.
///
/// **Linear and axial are drawn; the four that radiate from a point are not.**
/// A radial gradient needs many more triangles than a rectangle has corners, and
/// no fixture here uses one, so those get the flat average of the two colours —
/// visibly an approximation and not a wrong direction.
fn gradient_mesh(rect: Rect, gradient: &Gradient, opacity: Option<f32>) -> Shape {
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    let mut mesh = Mesh::default();

    match gradient.style {
        GradientStyle::Linear | GradientStyle::Axial => {
            // ODF measures the angle counter-clockwise from the direction that
            // runs bottom to top, and the screen's y grows downward, so the axis
            // is the unit vector below. A point's place along the gradient is its
            // projection onto it, rescaled so that the rectangle's own extent is
            // nought to one.
            let radians = gradient.angle.to_radians();
            let axis = vec2(radians.sin(), -radians.cos());
            let projections: Vec<f32> = corners
                .iter()
                .map(|corner| (*corner - rect.center()).dot(axis))
                .collect();
            let low = projections.iter().copied().fold(f32::MAX, f32::min);
            let high = projections.iter().copied().fold(f32::MIN, f32::max);
            let span = (high - low).max(f32::EPSILON);

            for (corner, projection) in corners.iter().zip(&projections) {
                let mut t = (projection - low) / span;
                // The border is the fraction of the run that stays the start
                // colour before the blend begins.
                let border = gradient.border.clamp(0.0, 0.99);
                t = ((t - border) / (1.0 - border)).clamp(0.0, 1.0);
                // An axial gradient runs out from the middle to both edges, so
                // the two halves of the rectangle each take the whole blend.
                if gradient.style == GradientStyle::Axial {
                    t = (t - 0.5).abs() * 2.0;
                }
                mesh.colored_vertex(
                    *corner,
                    alpha(blend(gradient.start, gradient.end, t), opacity),
                );
            }
        }
        _ => {
            let flat = alpha(blend(gradient.start, gradient.end, 0.5), opacity);
            for corner in corners {
                mesh.colored_vertex(corner, flat);
            }
        }
    }

    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    Shape::mesh(mesh)
}
