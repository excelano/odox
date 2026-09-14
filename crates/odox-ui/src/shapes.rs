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

use eframe::egui::{
    Color32, Mesh, Pos2, Rect, Shape, Stroke, Ui, UiBuilder, epaint::Vertex, pos2, vec2,
};
use odox_core::draw::Geometry;
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

impl Canvas<'_> {
    /// Fill the page.
    pub fn background(&mut self, ui: &Ui, fill: &Fill) {
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
    /// `draw:polyline`, `draw:line`, `draw:custom-shape` — whose outline is
    /// worked out by [`Geometry`] — a `draw:frame` holding a picture or a text
    /// box, and `draw:g`, which is a group and is descended into. Left undrawn:
    /// `draw:path`, `draw:connector` and `draw:measure`.
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
                        &properties.graphic.fill(),
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
            if let Some(colour) = self.flat(&properties.graphic.fill(), properties.graphic.opacity)
            {
                self.painter(ui)
                    .add(Shape::ellipse_filled(centre, radius, colour));
            }
            if let Some(stroke) = outline {
                self.painter(ui)
                    .add(Shape::ellipse_stroke(centre, radius, stroke));
            }
            return;
        }

        if shape.is(&Ns::Draw, "custom-shape") {
            // The outline is a path in a space of the shape's own, and the
            // formulas in it have to be evaluated before there are any points.
            if let Some(geometry) = shape
                .child(&Ns::Draw, "enhanced-geometry")
                .and_then(Geometry::read)
            {
                self.geometry(ui, &geometry, rect, &properties, outline);
            }
            self.text(ui, shape, rect);
            return;
        }

        if !shape.is(&Ns::Draw, "rect") && !shape.is(&Ns::Draw, "frame") {
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
            &properties.graphic.fill(),
            properties.graphic.opacity,
            &corners,
        );
        if let Some(stroke) = outline {
            self.painter(ui)
                .add(Shape::closed_line(corners.to_vec(), stroke));
        }
        self.text(ui, shape, rect);
    }

    /// A custom shape's outline, mapped from its own coordinate space onto the
    /// rectangle it occupies.
    fn geometry(
        &mut self,
        ui: &Ui,
        geometry: &Geometry,
        rect: Rect,
        properties: &Properties,
        outline: Option<Stroke>,
    ) {
        let view = geometry.view;
        let place = |(x, y): (f32, f32)| {
            pos2(
                rect.left() + (x - view.x) / view.width * rect.width(),
                rect.top() + (y - view.y) / view.height * rect.height(),
            )
        };
        for stroke in &geometry.paths {
            let points: Vec<Pos2> = stroke.points.iter().copied().map(place).collect();
            if points.len() < 2 {
                continue;
            }
            if stroke.fill {
                self.fill(
                    ui,
                    rect,
                    &properties.graphic.fill(),
                    properties.graphic.opacity,
                    &points,
                );
            }
            if stroke.stroke
                && let Some(pen) = outline
            {
                let shape = if stroke.closed {
                    Shape::closed_line(points, pen)
                } else {
                    Shape::line(points, pen)
                };
                self.painter(ui).add(shape);
            }
        }
    }

    /// The paragraphs a shape holds, or the picture it frames.
    fn text(&mut self, ui: &mut Ui, shape: &Element, rect: Rect) {
        // A frame around a picture is handed over whole, because the renderer
        // finds a frame among a parent's children and here the shape is the
        // frame itself. A text box is the ordinary case and its paragraphs are
        // its children.
        let picture = shape.child(&Ns::Draw, "image").is_some();
        let content = if picture {
            shape.clone()
        } else {
            match shape.child(&Ns::Draw, "text-box") {
                Some(box_) => box_.clone(),
                None => return,
            }
        };
        let (page, scale, palette) = (self.page, self.scale, self.palette);
        let document = self.document;
        let pictures = &mut *self.pictures;
        ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
            ui.set_clip_rect(rect.intersect(page));
            let mut flow = Flow::new(document, pictures, scale);
            flow.palette = palette;
            if picture {
                flow.frame(ui, &content, rect.width());
            } else {
                flow.blocks(ui, &content, rect.width());
            }
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

    /// Paint a fill inside an outline.
    ///
    /// Always a mesh, and always triangulated. A graphics toolkit fills a closed
    /// path by cutting it into triangles, and the obvious way — a fan from the
    /// first point, which is what `Shape::convex_polygon` does — is right only
    /// for a convex outline. An arrow, a callout and a puzzle piece are none of
    /// them convex, and a fan across one paints outside it. Colour varies over a
    /// mesh by varying at its corners, so a gradient costs nothing more than
    /// asking for the colour at each.
    fn fill(&mut self, ui: &Ui, rect: Rect, fill: &Fill, opacity: Option<f32>, points: &[Pos2]) {
        // The reference is copied out so that the picture cache can be filled
        // while the document is being read from.
        let document = self.document;

        if let Fill::Image(name) = fill {
            let Some(href) = document.styles.fill_image(name) else {
                return;
            };
            let Some(texture) = self
                .pictures
                .get(ui.ctx(), document, href)
                .map(eframe::egui::TextureHandle::id)
            else {
                return;
            };
            // Stretched over the shape's own rectangle: each corner takes the
            // corner of the picture that the corner of the rectangle is at.
            let tint = alpha(Color32::WHITE, opacity);
            let mut mesh = Mesh::with_texture(texture);
            for point in points {
                mesh.vertices.push(Vertex {
                    pos: *point,
                    uv: pos2(
                        (point.x - rect.left()) / rect.width().max(f32::EPSILON),
                        (point.y - rect.top()) / rect.height().max(f32::EPSILON),
                    ),
                    color: tint,
                });
            }
            for [a, b, c] in triangulate(points) {
                mesh.add_triangle(a, b, c);
            }
            self.painter(ui).add(Shape::mesh(mesh));
            return;
        }

        let gradient = match fill {
            Fill::None | Fill::Image(_) => return,
            Fill::Solid(_) => None,
            Fill::Gradient(name) => match document.styles.gradient(name) {
                Some(gradient) => Some(gradient),
                None => return,
            },
        };
        let flat = match fill {
            Fill::Solid(colour) => Some(alpha(format::color32(*colour), opacity)),
            _ => None,
        };

        let mut mesh = Mesh::default();
        for point in points {
            let colour = flat.unwrap_or_else(|| {
                gradient.map_or(Color32::TRANSPARENT, |gradient| {
                    alpha(gradient_colour(*point, rect, gradient), opacity)
                })
            });
            mesh.colored_vertex(*point, colour);
        }
        for [a, b, c] in triangulate(points) {
            mesh.add_triangle(a, b, c);
        }
        self.painter(ui).add(Shape::mesh(mesh));
    }

    /// One colour for a fill, where the shape being drawn cannot carry a mesh.
    fn flat(&self, fill: &Fill, opacity: Option<f32>) -> Option<Color32> {
        match fill {
            // Nothing to draw, and a picture that has no room in an ellipse,
            // which is drawn as an ellipse rather than as a mesh.
            Fill::None | Fill::Image(_) => None,
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

/// The colour a gradient has at one point of the rectangle it fills.
///
/// **Linear and axial run in the direction the document gives; the four that
/// radiate from a point do not.** A radial gradient's colour depends on the
/// distance from a centre, which this could compute — and no fixture uses one,
/// so it would be a direction invented rather than measured. Those get the flat
/// average of the two colours, which is visibly an approximation.
fn gradient_colour(point: Pos2, rect: Rect, gradient: &Gradient) -> Color32 {
    match gradient.style {
        GradientStyle::Linear | GradientStyle::Axial => {}
        _ => return blend(gradient.start, gradient.end, 0.5),
    }

    // ODF measures the angle counter-clockwise from the direction that runs
    // bottom to top, and the screen's y grows downward, so the axis is the unit
    // vector below. A point's place along the gradient is its projection onto
    // it, rescaled so that the rectangle's own extent is nought to one.
    let radians = gradient.angle.to_radians();
    let axis = vec2(radians.sin(), -radians.cos());
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    let projections = corners.map(|corner| (corner - rect.center()).dot(axis));
    let low = projections.iter().copied().fold(f32::MAX, f32::min);
    let high = projections.iter().copied().fold(f32::MIN, f32::max);
    let span = (high - low).max(f32::EPSILON);

    let mut t = ((point - rect.center()).dot(axis) - low) / span;
    // The border is the fraction of the run that stays the start colour before
    // the blend begins.
    let border = gradient.border.clamp(0.0, 0.99);
    t = ((t - border) / (1.0 - border)).clamp(0.0, 1.0);
    // An axial gradient runs out from the middle to both edges, so each half of
    // the rectangle takes the whole blend.
    if gradient.style == GradientStyle::Axial {
        t = (t - 0.5).abs() * 2.0;
    }
    blend(gradient.start, gradient.end, t)
}

/// Cut a closed outline into triangles, by clipping ears.
///
/// The standard method, and the reason for it is above [`Canvas::fill`]: the
/// cheap alternative is right only for convex outlines and ODF's shapes are
/// routinely not. An outline it cannot cut — one that crosses itself, which a
/// hand-edited document can hold — falls back to the fan, which is wrong in the
/// way the fan is always wrong rather than in a new way.
fn triangulate(points: &[Pos2]) -> Vec<[u32; 3]> {
    let count = points.len();
    if count < 3 {
        return Vec::new();
    }
    let fan = || -> Vec<[u32; 3]> {
        (1..count - 1)
            .map(|i| {
                [
                    0,
                    u32::try_from(i).unwrap_or(0),
                    u32::try_from(i + 1).unwrap_or(0),
                ]
            })
            .collect()
    };

    // Twice the signed area, whose sign is which way round the outline goes.
    let area: f32 = (0..count)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % count]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    let winding = if area >= 0.0 { 1.0 } else { -1.0 };

    let cross = |a: Pos2, b: Pos2, c: Pos2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let inside = |a: Pos2, b: Pos2, c: Pos2, p: Pos2| {
        cross(a, b, p) * winding >= 0.0
            && cross(b, c, p) * winding >= 0.0
            && cross(c, a, p) * winding >= 0.0
    };

    let mut remaining: Vec<usize> = (0..count).collect();
    let mut triangles = Vec::with_capacity(count);
    let mut stuck = 0;
    while remaining.len() > 3 {
        if stuck > remaining.len() {
            return fan();
        }
        let mut clipped = false;
        for position in 0..remaining.len() {
            let corner = [
                remaining[(position + remaining.len() - 1) % remaining.len()],
                remaining[position],
                remaining[(position + 1) % remaining.len()],
            ];
            let ear = corner.map(|index| points[index]);
            // A reflex corner is not an ear, and neither is one whose triangle
            // has another corner of the outline inside it.
            if cross(ear[0], ear[1], ear[2]) * winding <= 0.0 {
                continue;
            }
            if remaining
                .iter()
                .filter(|other| !corner.contains(other))
                .any(|other| inside(ear[0], ear[1], ear[2], points[*other]))
            {
                continue;
            }
            triangles.push(corner.map(|index| u32::try_from(index).unwrap_or(0)));
            remaining.remove(position);
            clipped = true;
            stuck = 0;
            break;
        }
        if !clipped {
            stuck += 1;
        }
    }
    if remaining.len() == 3 {
        triangles.push([
            u32::try_from(remaining[0]).unwrap_or(0),
            u32::try_from(remaining[1]).unwrap_or(0),
            u32::try_from(remaining[2]).unwrap_or(0),
        ]);
    }
    triangles
}

#[cfg(test)]
mod tests {
    use super::triangulate;
    use eframe::egui::{Pos2, pos2};

    /// Twice the area a run of triangles covers, and twice the area the outline
    /// encloses. Equal means the triangles cover the shape and nothing else.
    fn areas(points: &[Pos2]) -> (f32, f32) {
        let cross =
            |a: Pos2, b: Pos2, c: Pos2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        let triangles: f32 = triangulate(points)
            .iter()
            .map(|[a, b, c]| {
                cross(
                    points[*a as usize],
                    points[*b as usize],
                    points[*c as usize],
                )
                .abs()
            })
            .sum();
        let outline: f32 = (0..points.len())
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % points.len()]);
                a.x * b.y - b.x * a.y
            })
            .sum::<f32>()
            .abs();
        (triangles, outline)
    }

    /// Would catch the fan: an L covers three quarters of its bounding box, and
    /// a fan from the first corner covers the whole of it.
    #[test]
    fn a_concave_outline_is_cut_into_the_shape_and_not_its_hull() {
        let l = [
            pos2(0.0, 0.0),
            pos2(2.0, 0.0),
            pos2(2.0, 1.0),
            pos2(1.0, 1.0),
            pos2(1.0, 2.0),
            pos2(0.0, 2.0),
        ];
        let (triangles, outline) = areas(&l);
        assert!(
            (triangles - outline).abs() < 1e-3,
            "{triangles} against {outline}"
        );
        // Three of the four unit squares, twice over.
        assert!((outline - 6.0).abs() < 1e-3, "{outline}");
    }

    /// A cross has four reflex corners and is where a careless ear test fails.
    #[test]
    fn a_cross_is_cut_correctly_too() {
        let cross = [
            pos2(1.0, 0.0),
            pos2(2.0, 0.0),
            pos2(2.0, 1.0),
            pos2(3.0, 1.0),
            pos2(3.0, 2.0),
            pos2(2.0, 2.0),
            pos2(2.0, 3.0),
            pos2(1.0, 3.0),
            pos2(1.0, 2.0),
            pos2(0.0, 2.0),
            pos2(0.0, 1.0),
            pos2(1.0, 1.0),
        ];
        let (triangles, outline) = areas(&cross);
        assert!(
            (triangles - outline).abs() < 1e-3,
            "{triangles} against {outline}"
        );
    }

    /// The same outline the other way round: the winding must not decide whether
    /// it works, because a mirrored shape arrives reversed.
    #[test]
    fn winding_does_not_matter() {
        let mut l = vec![
            pos2(0.0, 0.0),
            pos2(2.0, 0.0),
            pos2(2.0, 1.0),
            pos2(1.0, 1.0),
            pos2(1.0, 2.0),
            pos2(0.0, 2.0),
        ];
        l.reverse();
        let (triangles, outline) = areas(&l);
        assert!(
            (triangles - outline).abs() < 1e-3,
            "{triangles} against {outline}"
        );
    }

    /// A convex outline is the ordinary case and must still come out whole.
    #[test]
    fn a_square_is_two_triangles() {
        let square = [
            pos2(0.0, 0.0),
            pos2(1.0, 0.0),
            pos2(1.0, 1.0),
            pos2(0.0, 1.0),
        ];
        assert_eq!(triangulate(&square).len(), 2);
        let (triangles, outline) = areas(&square);
        assert!((triangles - outline).abs() < 1e-4);
    }
}
