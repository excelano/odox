//! `draw:transform`: where a shape sits when a corner and a size cannot say it.
//!
//! Most shapes are placed by `svg:x`, `svg:y`, `svg:width` and `svg:height`, and
//! that is a rectangle with its edges along the page's. A shape that is turned,
//! or leaned over, cannot be described that way, so ODF gives it a list of
//! operations instead — `rotate (-3.14159) translate (28cm 15.75cm)` — and
//! writes no `svg:x` at all.
//!
//! A reader that wants the corner and cannot find it drops the shape. Across the
//! presentation templates `LibreOffice` ships that is 137 shapes in ten
//! documents, most of them the decoration that gives a template its identity, so
//! the list has to be read.
//!
//! # The two things the specification does not spell out
//!
//! The operations apply to a point **left to right**, which is the opposite of
//! the way SVG composes the same syntax, and the angle is in **radians** rather
//! than degrees. Both were settled against the templates: `Bottom Bar White`
//! decorates the bottom edge of its slide with a bar 0.7cm wide and 28cm tall
//! under `rotate (-1.5707963) translate (28cm 15.05cm)`, and only one reading of
//! the two puts it along the bottom.
//!
//! A rotation is counter-clockwise as a person sees it, and the page's y runs
//! downwards, so the matrix is the transpose of the one a mathematics text
//! writes.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use crate::value::Length;

/// An affine placement: where a shape's own coordinates land on the page.
///
/// The six numbers are the usual two-by-three, in the order `matrix()` writes
/// them: `x' = a x + c y + e` and `y' = b x + d y + f`. The two offsets are in
/// points, as every length in this crate is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// x from x.
    pub a: f32,
    /// y from x.
    pub b: f32,
    /// x from y.
    pub c: f32,
    /// y from y.
    pub d: f32,
    /// The x offset, in points.
    pub e: f32,
    /// The y offset, in points.
    pub f: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// The placement that moves nothing.
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// Read a `draw:transform` attribute.
    ///
    /// `None` where nothing in it parsed, so that a shape carrying an attribute
    /// this cannot read is placed by its corner rather than at the page's
    /// origin. An operation this does not know is skipped and the rest is kept:
    /// a shape drawn from three of its four operations is closer to right than a
    /// shape not drawn.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut placement = Self::IDENTITY;
        let mut read = 0;
        let mut rest = text;
        while let Some(open) = rest.find('(') {
            let name = rest[..open].trim().trim_start_matches(')').trim();
            let Some(close) = rest[open..].find(')') else {
                break;
            };
            let arguments = &rest[open + 1..open + close];
            rest = &rest[open + close + 1..];

            let numbers: Vec<f32> = arguments
                .split([',', ' ', '\t', '\n'])
                .filter(|word| !word.is_empty())
                .filter_map(number)
                .collect();
            let Some(step) = step(name, &numbers) else {
                continue;
            };
            // Left to right, so each operation acts on what the ones before it
            // produced: the new step goes outside.
            placement = placement.then(step);
            read += 1;
        }
        (read > 0).then_some(placement)
    }

    /// This placement, and then `outer`.
    #[must_use]
    pub fn then(self, outer: Self) -> Self {
        Self {
            a: outer.a * self.a + outer.c * self.b,
            b: outer.b * self.a + outer.d * self.b,
            c: outer.a * self.c + outer.c * self.d,
            d: outer.b * self.c + outer.d * self.d,
            e: outer.a * self.e + outer.c * self.f + outer.e,
            f: outer.b * self.e + outer.d * self.f + outer.f,
        }
    }

    /// Where a point in the shape's own coordinates lands, in points.
    #[must_use]
    pub fn apply(self, (x, y): (f32, f32)) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// Whether this turns or leans the shape, rather than only moving it.
    ///
    /// A placement that does neither can be reduced to a corner and a size,
    /// which is what most of a renderer wants to be handed.
    #[must_use]
    pub fn is_upright(self) -> bool {
        self.b.abs() < 1e-4 && self.c.abs() < 1e-4 && self.a > 0.0 && self.d > 0.0
    }
}

/// One operation of the list, as a placement of its own.
fn step(name: &str, numbers: &[f32]) -> Option<Transform> {
    let at = |i: usize| numbers.get(i).copied();
    Some(match name {
        "translate" => Transform {
            e: at(0)?,
            f: at(1).unwrap_or(0.0),
            ..Transform::IDENTITY
        },
        "scale" => {
            let x = at(0)?;
            Transform {
                a: x,
                d: at(1).unwrap_or(x),
                ..Transform::IDENTITY
            }
        }
        // Counter-clockwise on the page, whose y runs downwards.
        "rotate" => {
            let (sin, cos) = at(0)?.sin_cos();
            Transform {
                a: cos,
                b: -sin,
                c: sin,
                d: cos,
                ..Transform::IDENTITY
            }
        }
        "skewX" => Transform {
            c: at(0)?.tan(),
            ..Transform::IDENTITY
        },
        "skewY" => Transform {
            b: at(0)?.tan(),
            ..Transform::IDENTITY
        },
        "matrix" => Transform {
            a: at(0)?,
            b: at(1)?,
            c: at(2)?,
            d: at(3)?,
            e: at(4)?,
            f: at(5)?,
        },
        _ => return None,
    })
}

/// One argument. A translation's is a length and everything else's is a bare
/// number, and a length with no unit is already in points, so both go through
/// the same reader.
fn number(word: &str) -> Option<f32> {
    Length::parse(word).map_or_else(|| word.parse().ok(), |length| Some(length.points()))
}

#[cfg(test)]
mod tests {
    use super::Transform;

    fn near(left: (f32, f32), right: (f32, f32)) -> bool {
        (left.0 - right.0).abs() < 0.01 && (left.1 - right.1).abs() < 0.01
    }

    #[test]
    fn a_half_turn_puts_the_far_corner_where_the_near_one_was() {
        // Leaf White 2's decoration: 8.834cm by 15.086cm, turned about and moved
        // to the bottom right corner of a 28cm by 15.75cm slide.
        let placed = Transform::parse("rotate (-3.14159265358979) translate (28cm 15.75cm)")
            .expect("a readable transform");
        let corner = |x: f32, y: f32| {
            let (x, y) = placed.apply((x * 28.3465, y * 28.3465));
            (x / 28.3465, y / 28.3465)
        };
        assert!(
            near(corner(0.0, 0.0), (28.0, 15.75)),
            "{:?}",
            corner(0.0, 0.0)
        );
        assert!(
            near(corner(8.834, 15.086), (19.166, 0.664)),
            "{:?}",
            corner(8.834, 15.086)
        );
    }

    #[test]
    fn a_quarter_turn_lays_a_tall_bar_along_the_bottom() {
        // Bottom Bar White: 0.7cm wide and 28cm tall, which is only a bottom bar
        // if the operations apply left to right.
        let placed = Transform::parse("rotate (-1.5707963267949) translate (28cm 15.05cm)")
            .expect("a readable transform");
        let corner = |x: f32, y: f32| {
            let (x, y) = placed.apply((x * 28.3465, y * 28.3465));
            (x / 28.3465, y / 28.3465)
        };
        assert!(
            near(corner(0.0, 0.0), (28.0, 15.05)),
            "{:?}",
            corner(0.0, 0.0)
        );
        assert!(
            near(corner(0.7, 28.0), (0.0, 15.75)),
            "{:?}",
            corner(0.7, 28.0)
        );
    }

    #[test]
    fn a_translation_alone_is_upright_and_a_turn_is_not() {
        let moved = Transform::parse("translate (2cm 3cm)").expect("a readable transform");
        assert!(moved.is_upright());
        assert!(near(moved.apply((0.0, 0.0)), (56.693, 85.039)));
        let turned = Transform::parse("rotate (0.5) translate (0cm 0cm)").expect("readable");
        assert!(!turned.is_upright());
    }

    #[test]
    fn an_operation_with_no_name_this_knows_leaves_the_others_standing() {
        let placed = Transform::parse("wobble (3) translate (1cm 0cm)").expect("the translation");
        assert!(near(placed.apply((0.0, 0.0)), (28.3465, 0.0)));
        assert!(Transform::parse("wobble (3)").is_none());
    }
}
