//! `draw:measure`: a dimension line, worked out from the two points it
//! measures and its style, as `LibreOffice` draws one.
//!
//! Measured on a deck `LibreOffice` drew: the line stands off from the points
//! by `draw:line-distance`, 0.8 cm where the style says nothing, to the left of
//! the way from the first point to the second, which is above a line drawn
//! left to right, and to the right where `draw:placing` is `below`; each point
//! has a guide from `draw:guide-distance`, 0.1 cm, out from it to
//! `draw:guide-overhang`, 0.2 cm, past the line; and the label is centred
//! along the line, turned with it but never upside down, standing on it, or
//! hanging under it where `draw:placing` is `below`. The label is the text the shape holds, the
//! length `LibreOffice` wrote into it in the document's unit.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use crate::draw::Geometry;
use crate::style::GraphicProperties;
use crate::value::Length;

/// A point, in points from the page's top left corner.
pub type Point = (f32, f32);

/// What a dimension line draws, in points on the page.
#[derive(Debug, Clone, PartialEq)]
pub struct Dimension {
    /// The guide out from each point.
    pub guides: [[Point; 2]; 2],
    /// The line, from the end by the first point to the end by the second.
    pub line: [Point; 2],
    /// The unit vector from the first point towards the second.
    pub along: Point,
    /// Whether the label hangs under the line rather than standing on it.
    pub under: bool,
}

const CM: f32 = 72.0 / 2.54;

impl Dimension {
    /// The dimension line between two points, as a style draws it. `None`
    /// where the points are the same, which gives it no direction.
    pub fn new(from: Point, to: Point, style: &GraphicProperties) -> Option<Self> {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = dx.hypot(dy);
        if length <= f32::EPSILON {
            return None;
        }
        let along = (dx / length, dy / length);
        let below = style.placed_below == Some(true);
        let side = if below { -1.0 } else { 1.0 };
        let across = (along.1 * side, -along.0 * side);
        let points = |value: Option<Length>, default: f32| value.map_or(default, Length::points);
        let distance = points(style.line_distance, 0.8 * CM);
        let overhang = points(style.guide_overhang, 0.2 * CM);
        let gap = points(style.guide_distance, 0.1 * CM);
        let off = |point: Point, by: f32| (point.0 + across.0 * by, point.1 + across.1 * by);
        // A line on the other side of the points, by a distance below zero,
        // has guides that stop at it, as `LibreOffice` draws them.
        let reach = if distance < 0.0 {
            distance
        } else {
            distance + overhang
        };
        Some(Self {
            guides: [from, to].map(|point| [off(point, gap), off(point, reach)]),
            line: [off(from, distance), off(to, distance)],
            along,
            under: below,
        })
    }

    /// The box a label `height` points tall is centred in: its top left
    /// corner, and the vectors along its top and its left side. It runs the
    /// length of the line and reads left to right, a line drawn right to left
    /// having its box turned the other way round rather than its label upside
    /// down, and it stands on the line, or hangs from it where the style places
    /// the label below.
    pub fn label(&self, height: f32) -> (Point, Point, Point) {
        let [start, end] = self.line;
        let ((left, right), along) =
            if self.along.0 < 0.0 || (self.along.0.abs() <= f32::EPSILON && self.along.1 > 0.0) {
                ((end, start), (-self.along.0, -self.along.1))
            } else {
                ((start, end), self.along)
            };
        let down = (-along.1, along.0);
        let length = (right.0 - left.0).hypot(right.1 - left.1);
        let corner = if self.under {
            left
        } else {
            (left.0 - down.0 * height, left.1 - down.1 * height)
        };
        (
            corner,
            (along.0 * length, along.1 * length),
            (down.0 * height, down.1 * height),
        )
    }

    /// The outline of a marker drawn at one end of the line, `width` points
    /// wide: its tip, the middle of the top of its view box, at the end, and
    /// the rest of it running back along the line.
    pub fn marker(&self, outline: &Geometry, at_start: bool, width: f32) -> Vec<Vec<Point>> {
        let view = outline.view;
        if view.width <= 0.0 {
            return Vec::new();
        }
        let scale = width / view.width;
        let (tip, inward) = if at_start {
            (self.line[0], self.along)
        } else {
            (self.line[1], (-self.along.0, -self.along.1))
        };
        let side = (-inward.1, inward.0);
        outline
            .paths
            .iter()
            .map(|path| {
                path.points
                    .iter()
                    .map(|&(x, y)| {
                        let u = (x - (view.x + view.width / 2.0)) * scale;
                        let v = (y - view.y) * scale;
                        (
                            tip.0 + side.0 * u + inward.0 * v,
                            tip.1 + side.1 * u + inward.1 * v,
                        )
                    })
                    .collect()
            })
            .collect()
    }
}

/// A marker's width where its style gives none, as `LibreOffice`'s default
/// style draws one.
pub const MARKER_WIDTH: f32 = 0.3 * CM;

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Point, b: Point) -> bool {
        (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01
    }

    #[test]
    fn a_line_drawn_left_to_right_stands_above_its_points() {
        let dimension = Dimension::new((0.0, 100.0), (200.0, 100.0), &GraphicProperties::default())
            .expect("a direction");
        let distance = 0.8 * CM;
        assert!(near(dimension.line[0], (0.0, 100.0 - distance)));
        assert!(near(dimension.line[1], (200.0, 100.0 - distance)));
        assert!(near(dimension.guides[0][0], (0.0, 100.0 - 0.1 * CM)));
        assert!(near(
            dimension.guides[0][1],
            (0.0, 100.0 - distance - 0.2 * CM)
        ));
        let (corner, along, down) = dimension.label(10.0);
        assert!(near(corner, (0.0, 100.0 - distance - 10.0)), "{corner:?}");
        assert!(near(along, (200.0, 0.0)));
        assert!(near(down, (0.0, 10.0)));
    }

    #[test]
    fn a_line_drawn_right_to_left_keeps_its_label_upright() {
        let dimension = Dimension::new((200.0, 100.0), (0.0, 100.0), &GraphicProperties::default())
            .expect("a direction");
        let distance = 0.8 * CM;
        assert!(near(dimension.line[0], (200.0, 100.0 + distance)));
        let (corner, along, down) = dimension.label(10.0);
        assert!(near(along, (200.0, 0.0)), "it reads left to right");
        assert!(near(down, (0.0, 10.0)));
        assert!(
            near(corner, (0.0, 100.0 + distance - 10.0)),
            "standing on the line, between it and the points"
        );
    }

    #[test]
    fn a_line_placed_below_hangs_its_label_under_it_on_the_other_side() {
        let mut style = GraphicProperties::default();
        style.placed_below = Some(true);
        let dimension = Dimension::new((0.0, 100.0), (200.0, 100.0), &style).expect("a direction");
        let distance = 0.8 * CM;
        assert!(near(dimension.line[0], (0.0, 100.0 + distance)));
        let (corner, _, _) = dimension.label(10.0);
        assert!(near(corner, (0.0, 100.0 + distance)));
    }

    #[test]
    fn a_line_on_the_other_side_has_guides_that_stop_at_it() {
        let mut style = GraphicProperties::default();
        style.line_distance = Some(Length(-CM));
        let dimension = Dimension::new((0.0, 100.0), (200.0, 100.0), &style).expect("a direction");
        assert!(near(dimension.line[0], (0.0, 100.0 + CM)));
        assert!(near(dimension.guides[0][0], (0.0, 100.0 - 0.1 * CM)));
        assert!(near(dimension.guides[0][1], (0.0, 100.0 + CM)));
        let (corner, _, _) = dimension.label(10.0);
        assert!(near(corner, (0.0, 100.0 + CM - 10.0)), "standing on it");
    }
}
