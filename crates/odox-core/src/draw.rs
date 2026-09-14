//! `draw:enhanced-geometry`: the outline of a custom shape, worked out.
//!
//! A custom shape does not state its outline as points. It states a *path* in a
//! compact command language, in a coordinate space of its own, whose numbers may
//! be references to named formulas that are themselves arithmetic over the
//! space's edges and over adjustment values a person dragged. A rounded
//! rectangle is `M ?f7 0 X 0 ?f8 L 0 ?f9 Y ?f7 21600 …`, and none of that means
//! anything until the formulas are evaluated.
//!
//! What comes out of here is [`Geometry`]: the same outline as flat polylines in
//! the shape's own space, ready for a renderer to map onto a rectangle. Curves
//! and arcs are flattened here rather than passed on, because the number of
//! segments a curve needs depends on the size of the coordinate space and not on
//! the size of the window, and this is where the space is known.
//!
//! A `draw:path` states its outline the other way, in SVG's path notation in
//! `svg:d`, and comes out of here as the same [`Geometry`]. The two languages
//! have the same shape — move, line, curve, close — and share the pen below.
//!
//! # What is implemented
//!
//! The whole command language except `Q`'s smooth variants, and the formula
//! grammar in full. What is *measured* is narrower: across the twenty-three
//! presentation templates `LibreOffice` ships, the commands that occur are `M`,
//! `L`, `C`, `Z`, `N`, `U`, `X`, `Y` and `V`, and the formulas use the four edge
//! constants, `pi`, and `if`, `sin`, `cos` and `abs`. The arc commands `A`, `B`
//! and `W` occur nowhere in that set and their sweep direction is taken from the
//! specification rather than from a document.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::xml::{Element, Ns};

/// The coordinate space a shape states its outline in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewBox {
    /// The left edge.
    pub x: f32,
    /// The top edge.
    pub y: f32,
    /// How wide.
    pub width: f32,
    /// How tall.
    pub height: f32,
}

impl ViewBox {
    fn parse(text: &str) -> Option<Self> {
        let numbers: Vec<f32> = text
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        let [x, y, width, height] = numbers[..] else {
            return None;
        };
        (width > 0.0 && height > 0.0).then_some(Self {
            x,
            y,
            width,
            height,
        })
    }
}

/// One stroke of the pen: a run of points, and what is done with them.
#[derive(Debug, Clone, PartialEq)]
pub struct SubPath {
    /// The points, in the shape's own coordinate space.
    pub points: Vec<(f32, f32)>,
    /// Whether the last point joins the first.
    pub closed: bool,
    /// Whether the inside is filled. `F` in the path turns it off for the rest
    /// of the path, which is how a shape draws a detail over its own body.
    pub fill: bool,
    /// Whether the outline is drawn. `S` turns it off.
    pub stroke: bool,
}

/// A custom shape's outline.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    /// The space the points are in.
    pub view: ViewBox,
    /// The strokes, in the order they are drawn.
    pub paths: Vec<SubPath>,
}

/// How finely a curve is broken into straight lines.
///
/// Sixteen segments for a whole cubic and one for every six degrees of an arc,
/// which at the sizes ODF's coordinate spaces use — twenty-one thousand six
/// hundred units across, drawn into a few hundred pixels — is below what a
/// screen can show.
const CURVE_SEGMENTS: usize = 16;
const DEGREES_PER_SEGMENT: f32 = 6.0;

impl Geometry {
    /// Read a `draw:path` element, whose outline is SVG path data in `svg:d`.
    ///
    /// `None` where it states no path or no coordinate space.
    ///
    /// **Arcs are drawn as the straight line to where they end.** `A` and `a`
    /// occur in none of the 352 paths across the presentation templates
    /// `LibreOffice` ships, and turning an endpoint-parameterised elliptical arc
    /// into a centre and a sweep is a page of trigonometry to serve nothing that
    /// exists. A line keeps the outline closed and roughly where it belongs,
    /// which is the failure worth having.
    pub fn read_path(path: &Element) -> Option<Self> {
        let view = ViewBox::parse(path.attr(&Ns::Svg, "viewBox")?)?;
        let data = path.attr(&Ns::Svg, "d")?;
        let mut pen = Pen::new();
        pen.svg(data);
        Some(Self {
            view,
            paths: pen.finish(),
        })
    }

    /// Read a `draw:enhanced-geometry` element.
    ///
    /// `None` where it states no path or no coordinate space, which is a shape
    /// nothing can draw.
    pub fn read(geometry: &Element) -> Option<Self> {
        let view = ViewBox::parse(geometry.attr(&Ns::Svg, "viewBox")?)?;
        let path = geometry.attr(&Ns::Draw, "enhanced-path")?;

        let formulas = Formulas::new(geometry, view);
        let mut pen = Pen::new();
        pen.run(path, &formulas);
        let mut paths = pen.finish();

        // A mirrored shape states its outline once and is drawn flipped.
        let flip_x = geometry.attr(&Ns::Draw, "mirror-horizontal") == Some("true");
        let flip_y = geometry.attr(&Ns::Draw, "mirror-vertical") == Some("true");
        if flip_x || flip_y {
            for path in &mut paths {
                for (x, y) in &mut path.points {
                    if flip_x {
                        *x = view.x + view.width - (*x - view.x);
                    }
                    if flip_y {
                        *y = view.y + view.height - (*y - view.y);
                    }
                }
            }
        }

        Some(Self { view, paths })
    }
}

/// The named formulas of one shape, and the values they are over.
struct Formulas<'a> {
    by_name: HashMap<&'a str, &'a str>,
    modifiers: Vec<f32>,
    view: ViewBox,
    /// Evaluated formulas, kept because a chain of thirty each referring to the
    /// one before is ordinary and re-evaluating it per reference is not.
    known: RefCell<HashMap<String, f32>>,
    depth: Cell<u32>,
}

/// How deep a formula may refer before it is called a cycle.
///
/// No writer produces one; a hand-edited file can, and the alternative to a
/// limit is a window that stops responding.
const MAX_DEPTH: u32 = 64;

impl<'a> Formulas<'a> {
    fn new(geometry: &'a Element, view: ViewBox) -> Self {
        let by_name = geometry
            .elements()
            .filter(|e| e.is(&Ns::Draw, "equation"))
            .filter_map(|e| Some((e.attr(&Ns::Draw, "name")?, e.attr(&Ns::Draw, "formula")?)))
            .collect();
        let modifiers = geometry
            .attr(&Ns::Draw, "modifiers")
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        Self {
            by_name,
            modifiers,
            view,
            known: RefCell::new(HashMap::new()),
            depth: Cell::new(0),
        }
    }

    /// The value of a named formula.
    fn named(&self, name: &str) -> f32 {
        if let Some(value) = self.known.borrow().get(name) {
            return *value;
        }
        if self.depth.get() >= MAX_DEPTH {
            return 0.0;
        }
        let Some(text) = self.by_name.get(name) else {
            return 0.0;
        };
        self.depth.set(self.depth.get() + 1);
        let value = self.eval(text);
        self.depth.set(self.depth.get() - 1);
        self.known.borrow_mut().insert(name.to_owned(), value);
        value
    }

    /// A modifier, which is an adjustment a person dragged and the document
    /// stored.
    fn modifier(&self, index: usize) -> f32 {
        self.modifiers.get(index).copied().unwrap_or(0.0)
    }

    /// One of the constants a formula may name.
    fn constant(&self, name: &str) -> Option<f32> {
        Some(match name {
            "left" => self.view.x,
            "top" => self.view.y,
            "right" => self.view.x + self.view.width,
            "bottom" => self.view.y + self.view.height,
            "width" | "logwidth" => self.view.width,
            "height" | "logheight" => self.view.height,
            "pi" => std::f32::consts::PI,
            // A shape may ask whether it is being stroked or filled and draw
            // differently. Nothing here answers no.
            "hasstroke" | "hasfill" => 1.0,
            "xstretch" | "ystretch" => 0.0,
            _ => return None,
        })
    }

    fn eval(&self, text: &str) -> f32 {
        Expression {
            text: text.as_bytes(),
            at: 0,
            formulas: self,
        }
        .expression()
    }
}

/// A formula, read left to right.
struct Expression<'a, 'f> {
    text: &'a [u8],
    at: usize,
    formulas: &'a Formulas<'f>,
}

impl Expression<'_, '_> {
    fn skip(&mut self) {
        while self.at < self.text.len() && self.text[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip();
        self.text.get(self.at).copied()
    }

    fn take(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.at += 1;
            return true;
        }
        false
    }

    fn expression(&mut self) -> f32 {
        let mut value = self.term();
        loop {
            if self.take(b'+') {
                value += self.term();
            } else if self.take(b'-') {
                value -= self.term();
            } else {
                return value;
            }
        }
    }

    fn term(&mut self) -> f32 {
        let mut value = self.factor();
        loop {
            if self.take(b'*') {
                value *= self.factor();
            } else if self.take(b'/') {
                let divisor = self.factor();
                // A formula dividing by zero is a shape somebody edited by hand.
                // Nought is a point that can be drawn; infinity is not.
                value = if divisor == 0.0 { 0.0 } else { value / divisor };
            } else {
                return value;
            }
        }
    }

    fn factor(&mut self) -> f32 {
        if self.take(b'-') {
            return -self.factor();
        }
        if self.take(b'+') {
            return self.factor();
        }
        if self.take(b'(') {
            let value = self.expression();
            self.take(b')');
            return value;
        }
        if self.take(b'?') {
            let name = self.word();
            return self.formulas.named(&name);
        }
        if self.take(b'$') {
            let index = self.word().parse().unwrap_or(0);
            return self.formulas.modifier(index);
        }
        match self.peek() {
            Some(byte) if byte.is_ascii_alphabetic() => {
                let name = self.word();
                if self.take(b'(') {
                    let arguments = self.arguments();
                    return call(&name, &arguments);
                }
                self.formulas.constant(&name).unwrap_or(0.0)
            }
            _ => self.number(),
        }
    }

    fn arguments(&mut self) -> Vec<f32> {
        let mut arguments = Vec::new();
        if self.take(b')') {
            return arguments;
        }
        loop {
            arguments.push(self.expression());
            if !self.take(b',') {
                self.take(b')');
                return arguments;
            }
        }
    }

    /// A name or a run of digits, whichever is under the cursor.
    fn word(&mut self) -> String {
        self.skip();
        let start = self.at;
        while self
            .text
            .get(self.at)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            self.at += 1;
        }
        String::from_utf8_lossy(&self.text[start..self.at]).into_owned()
    }

    fn number(&mut self) -> f32 {
        self.skip();
        let start = self.at;
        while self
            .text
            .get(self.at)
            .is_some_and(|b| b.is_ascii_digit() || *b == b'.')
        {
            self.at += 1;
        }
        if start == self.at {
            // Nothing readable here: step over it so that a malformed formula
            // ends rather than spins.
            self.at += 1;
            return 0.0;
        }
        String::from_utf8_lossy(&self.text[start..self.at])
            .parse()
            .unwrap_or(0.0)
    }
}

fn call(name: &str, arguments: &[f32]) -> f32 {
    let argument = |n: usize| arguments.get(n).copied().unwrap_or(0.0);
    match name {
        "abs" => argument(0).abs(),
        "sqrt" => argument(0).max(0.0).sqrt(),
        // Radians. A formula that means degrees writes the conversion itself,
        // as `sin($0 * (pi/180))`, which is how every one in the corpus does it.
        "sin" => argument(0).sin(),
        "cos" => argument(0).cos(),
        "tan" => argument(0).tan(),
        "atan" => argument(0).atan(),
        "atan2" => argument(0).atan2(argument(1)),
        "min" => argument(0).min(argument(1)),
        "max" => argument(0).max(argument(1)),
        // Greater than nought is true, which is the specification's rule and not
        // the usual one.
        "if" => {
            if argument(0) > 0.0 {
                argument(1)
            } else {
                argument(2)
            }
        }
        _ => 0.0,
    }
}

/// The state of drawing one path: where the pen is and what it has drawn.
struct Pen {
    done: Vec<SubPath>,
    points: Vec<(f32, f32)>,
    closed: bool,
    fill: bool,
    stroke: bool,
}

impl Pen {
    fn new() -> Self {
        Self {
            done: Vec::new(),
            points: Vec::new(),
            closed: false,
            fill: true,
            stroke: true,
        }
    }

    fn at(&self) -> (f32, f32) {
        self.points.last().copied().unwrap_or((0.0, 0.0))
    }

    /// Finish the run of points in hand and begin another.
    fn brk(&mut self) {
        if self.points.len() >= 2 {
            self.done.push(SubPath {
                points: std::mem::take(&mut self.points),
                closed: self.closed,
                fill: self.fill,
                stroke: self.stroke,
            });
        } else {
            self.points.clear();
        }
        self.closed = false;
    }

    fn finish(mut self) -> Vec<SubPath> {
        self.brk();
        self.done
    }

    /// Walk the path, command by command.
    fn run(&mut self, path: &str, formulas: &Formulas<'_>) {
        let mut tokens = Tokens {
            text: path.as_bytes(),
            at: 0,
            formulas,
            pushed: None,
        };
        let mut command = None;
        loop {
            match tokens.next() {
                Some(Token::Command(letter)) => {
                    command = Some(letter);
                    self.command(letter, &mut tokens);
                }
                // A command's arguments may repeat: `L x y x y x y` is three
                // lines, and the letter is written once.
                Some(Token::Number(first)) => match command {
                    Some(letter) => {
                        tokens.pushed = Some(first);
                        self.command(letter, &mut tokens);
                    }
                    None => return,
                },
                None => return,
            }
        }
    }

    fn command(&mut self, letter: char, tokens: &mut Tokens<'_, '_>) {
        match letter {
            'M' => {
                let point = tokens.point();
                self.brk();
                self.points.push(point);
            }
            'L' => {
                let point = tokens.point();
                self.points.push(point);
            }
            'C' => {
                let (a, b, end) = (tokens.point(), tokens.point(), tokens.point());
                self.cubic(a, b, end);
            }
            'Q' => {
                let (control, end) = (tokens.point(), tokens.point());
                // A quadratic is a cubic whose two controls sit two thirds of
                // the way from each end towards the single one.
                let from = self.at();
                let third = |a: f32, b: f32| a + 2.0 / 3.0 * (b - a);
                self.cubic(
                    (third(from.0, control.0), third(from.1, control.1)),
                    (third(end.0, control.0), third(end.1, control.1)),
                    end,
                );
            }
            'Z' => {
                self.closed = true;
                self.brk();
            }
            'N' => self.brk(),
            'F' => self.fill = false,
            'S' => self.stroke = false,
            'T' | 'U' => {
                let (centre, radii) = (tokens.point(), tokens.point());
                let (from, to) = (tokens.number(), tokens.number());
                if letter == 'U' {
                    self.brk();
                }
                self.arc(centre, radii, from, to);
            }
            'X' | 'Y' => {
                let to = tokens.point();
                self.quadrant(to, letter == 'X');
            }
            'A' | 'B' | 'W' | 'V' => {
                let (corner, opposite) = (tokens.point(), tokens.point());
                let (from, to) = (tokens.point(), tokens.point());
                if letter == 'B' || letter == 'V' {
                    self.brk();
                }
                self.box_arc(corner, opposite, from, to, letter == 'W' || letter == 'V');
            }
            _ => {}
        }
    }

    /// The four curve commands, which differ only in where their two control
    /// points come from. Returns where a `smooth` curve after this one
    /// continues from, or `None` where the data ran out mid-command.
    fn svg_curve(
        &mut self,
        lower: u8,
        scan: &mut Numbers,
        offset: impl Fn((f32, f32)) -> (f32, f32),
        reflected: Option<(f32, f32)>,
    ) -> Option<(f32, f32)> {
        let here = self.at();
        let (first, second, end) = match lower {
            b'c' => {
                let (a, b, e) = (scan.point()?, scan.point()?, scan.point()?);
                (offset(a), offset(b), offset(e))
            }
            b's' => {
                let (b, e) = (scan.point()?, scan.point()?);
                // With no curve before it the first control sits on the current
                // point, which is what SVG says.
                (reflected.unwrap_or(here), offset(b), offset(e))
            }
            b'q' => {
                let (c, e) = (scan.point()?, scan.point()?);
                let (c, e) = (offset(c), offset(e));
                (quadratic(here, c), quadratic(e, c), e)
            }
            // A smooth quadratic, whose one control point is the last one
            // reflected.
            _ => {
                let e = offset(scan.point()?);
                let c = reflected.unwrap_or(here);
                (quadratic(here, c), quadratic(e, c), e)
            }
        };
        self.cubic(first, second, end);
        Some((2.0 * end.0 - second.0, 2.0 * end.1 - second.1))
    }

    /// Walk SVG path data.
    ///
    /// A command letter is followed by as many argument groups as are written,
    /// and a lower-case letter means its numbers are offsets from where the pen
    /// is. After a `moveto` the implied repeat is a `lineto`, which is SVG's one
    /// irregularity and the reason `implied` exists.
    fn svg(&mut self, data: &str) {
        let mut scan = Numbers {
            text: data.as_bytes(),
            at: 0,
        };
        let mut command = b' ';
        // Where the previous curve's second control point was, reflected, which
        // is what a smooth curve continues from.
        let mut reflected: Option<(f32, f32)> = None;
        let mut start = (0.0, 0.0);

        loop {
            if let Some(letter) = scan.command() {
                command = letter;
            } else if scan.peek_number().is_none() {
                return;
            }
            let lower = command.to_ascii_lowercase();
            let relative = command.is_ascii_lowercase();
            let here = self.at();
            let offset = |point: (f32, f32)| {
                if relative {
                    (here.0 + point.0, here.1 + point.1)
                } else {
                    point
                }
            };

            match lower {
                b'm' => {
                    let Some(to) = scan.point() else { return };
                    let to = offset(to);
                    self.brk();
                    self.points.push(to);
                    start = to;
                    reflected = None;
                    // The pairs after a moveto are lines, not more moves.
                    command = if relative { b'l' } else { b'L' };
                }
                b'l' => {
                    let Some(to) = scan.point() else { return };
                    self.points.push(offset(to));
                    reflected = None;
                }
                b'h' => {
                    let Some(x) = scan.number() else { return };
                    let x = if relative { here.0 + x } else { x };
                    self.points.push((x, here.1));
                    reflected = None;
                }
                b'v' => {
                    let Some(y) = scan.number() else { return };
                    let y = if relative { here.1 + y } else { y };
                    self.points.push((here.0, y));
                    reflected = None;
                }
                b'c' | b's' | b'q' | b't' => {
                    let Some(next) = self.svg_curve(lower, &mut scan, offset, reflected) else {
                        return;
                    };
                    reflected = Some(next);
                }
                b'a' => {
                    // The flags and radii are read and dropped; see `read_path`.
                    for _ in 0..3 {
                        if scan.number().is_none() {
                            return;
                        }
                    }
                    let (Some(_), Some(_)) = (scan.number(), scan.number()) else {
                        return;
                    };
                    let Some(to) = scan.point() else { return };
                    self.points.push(offset(to));
                    reflected = None;
                }
                b'z' => {
                    self.closed = true;
                    self.brk();
                    // A path may carry on after closing, from where the last
                    // sub-path began.
                    self.points.push(start);
                    reflected = None;
                }
                _ => return,
            }
        }
    }

    fn cubic(&mut self, a: (f32, f32), b: (f32, f32), end: (f32, f32)) {
        let from = self.at();
        for step in 1..=CURVE_SEGMENTS {
            #[allow(clippy::cast_precision_loss)]
            let t = step as f32 / CURVE_SEGMENTS as f32;
            let u = 1.0 - t;
            let blend = |p0: f32, p1: f32, p2: f32, p3: f32| {
                u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3
            };
            self.points.push((
                blend(from.0, a.0, b.0, end.0),
                blend(from.1, a.1, b.1, end.1),
            ));
        }
    }

    /// A run of points along an ellipse, from one angle to another in degrees.
    fn arc(&mut self, centre: (f32, f32), radii: (f32, f32), from: f32, to: f32) {
        // A sweep that would be nothing or negative is the long way round, which
        // is what `0 360` means and what every full ellipse in the corpus says.
        let mut sweep = to - from;
        if sweep <= 0.0 {
            sweep += 360.0;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = ((sweep / DEGREES_PER_SEGMENT).ceil() as usize).max(2);
        for step in 0..=steps {
            #[allow(clippy::cast_precision_loss)]
            let angle = (from + sweep * step as f32 / steps as f32).to_radians();
            self.points.push((
                centre.0 + radii.0 * angle.cos(),
                centre.1 + radii.1 * angle.sin(),
            ));
        }
    }

    /// A quarter of an ellipse from where the pen is to a point.
    ///
    /// `X` leaves horizontally and arrives vertically, `Y` the other way round.
    /// Between them they are how every rounded corner in ODF is written.
    fn quadrant(&mut self, to: (f32, f32), x_first: bool) {
        let from = self.at();
        let centre = if x_first {
            (to.0, from.1)
        } else {
            (from.0, to.1)
        };
        let radii = ((to.0 - from.0).abs(), (to.1 - from.1).abs());
        if radii.0 == 0.0 || radii.1 == 0.0 {
            self.points.push(to);
            return;
        }
        let angle_of = |p: (f32, f32)| (p.1 - centre.1).atan2(p.0 - centre.0).to_degrees();
        let (start, end) = (angle_of(from), angle_of(to));
        // The quarter that joins the two, taken the short way.
        let mut sweep = end - start;
        while sweep > 180.0 {
            sweep -= 360.0;
        }
        while sweep < -180.0 {
            sweep += 360.0;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = ((sweep.abs() / DEGREES_PER_SEGMENT).ceil() as usize).max(2);
        for step in 1..=steps {
            #[allow(clippy::cast_precision_loss)]
            let angle = (start + sweep * step as f32 / steps as f32).to_radians();
            self.points.push((
                centre.0 + radii.0 * angle.cos(),
                centre.1 + radii.1 * angle.sin(),
            ));
        }
    }

    /// An arc of the ellipse that fills a box, between two points on it.
    ///
    /// **The sweep direction here is the specification's and not a measurement.**
    /// `V` occurs four times in the twenty-three templates surveyed and `A`, `B`
    /// and `W` occur in none of them, so nothing in the corpus tells these apart.
    fn box_arc(
        &mut self,
        corner: (f32, f32),
        opposite: (f32, f32),
        from: (f32, f32),
        to: (f32, f32),
        clockwise: bool,
    ) {
        let centre = (
            f32::midpoint(corner.0, opposite.0),
            f32::midpoint(corner.1, opposite.1),
        );
        let radii = (
            (opposite.0 - corner.0).abs() / 2.0,
            (opposite.1 - corner.1).abs() / 2.0,
        );
        if radii.0 == 0.0 || radii.1 == 0.0 {
            self.points.push(to);
            return;
        }
        let angle_of = |p: (f32, f32)| {
            ((p.1 - centre.1) / radii.1)
                .atan2((p.0 - centre.0) / radii.0)
                .to_degrees()
        };
        let (start, end) = (angle_of(from), angle_of(to));
        let sweep = if clockwise { start - end } else { end - start };
        let sweep = if sweep <= 0.0 { sweep + 360.0 } else { sweep };
        let (a, b) = if clockwise {
            (start, start - sweep)
        } else {
            (start, start + sweep)
        };
        self.arc_between(centre, radii, a, b);
    }

    fn arc_between(&mut self, centre: (f32, f32), radii: (f32, f32), from: f32, to: f32) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = (((to - from).abs() / DEGREES_PER_SEGMENT).ceil() as usize).max(2);
        for step in 0..=steps {
            #[allow(clippy::cast_precision_loss)]
            let angle = (from + (to - from) * step as f32 / steps as f32).to_radians();
            self.points.push((
                centre.0 + radii.0 * angle.cos(),
                centre.1 + radii.1 * angle.sin(),
            ));
        }
    }
}

/// A quadratic curve's single control point, as one of a cubic's two: two
/// thirds of the way from the end towards it.
fn quadratic(end: (f32, f32), control: (f32, f32)) -> (f32, f32) {
    (
        end.0 + 2.0 / 3.0 * (control.0 - end.0),
        end.1 + 2.0 / 3.0 * (control.1 - end.1),
    )
}

/// SVG path data, read one number or command at a time.
///
/// SVG separates numbers with whitespace, with a comma, or with nothing at all
/// where the next one begins unambiguously: `0-571` is two numbers and so is
/// `.5.5`. That is why this is a scanner and not a `split_whitespace`.
struct Numbers<'a> {
    text: &'a [u8],
    at: usize,
}

impl Numbers<'_> {
    fn skip(&mut self) {
        while self
            .text
            .get(self.at)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b',')
        {
            self.at += 1;
        }
    }

    /// The next command letter, if one is there.
    fn command(&mut self) -> Option<u8> {
        self.skip();
        let byte = *self.text.get(self.at)?;
        if byte.is_ascii_alphabetic() && !matches!(byte, b'e' | b'E') {
            self.at += 1;
            return Some(byte);
        }
        None
    }

    fn peek_number(&mut self) -> Option<u8> {
        self.skip();
        self.text
            .get(self.at)
            .copied()
            .filter(|b| b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.'))
    }

    fn number(&mut self) -> Option<f32> {
        self.peek_number()?;
        let start = self.at;
        if matches!(self.text.get(self.at), Some(b'-' | b'+')) {
            self.at += 1;
        }
        let mut seen_point = false;
        while let Some(byte) = self.text.get(self.at) {
            match byte {
                b'0'..=b'9' => self.at += 1,
                // A second point begins the next number: `.5.5` is two.
                b'.' if !seen_point => {
                    seen_point = true;
                    self.at += 1;
                }
                b'e' | b'E' => {
                    self.at += 1;
                    if matches!(self.text.get(self.at), Some(b'-' | b'+')) {
                        self.at += 1;
                    }
                }
                _ => break,
            }
        }
        String::from_utf8_lossy(&self.text[start..self.at])
            .parse()
            .ok()
    }

    fn point(&mut self) -> Option<(f32, f32)> {
        Some((self.number()?, self.number()?))
    }
}

enum Token {
    Command(char),
    Number(f32),
}

/// The path, read one token at a time, with every reference already resolved.
struct Tokens<'a, 'f> {
    text: &'a [u8],
    at: usize,
    formulas: &'a Formulas<'f>,
    // Set when a repeated argument group put a number back.
    pushed: Option<f32>,
}

impl Tokens<'_, '_> {
    fn next(&mut self) -> Option<Token> {
        if let Some(number) = self.pushed.take() {
            return Some(Token::Number(number));
        }
        while self
            .text
            .get(self.at)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b',')
        {
            self.at += 1;
        }
        let byte = *self.text.get(self.at)?;
        if byte.is_ascii_alphabetic() {
            self.at += 1;
            return Some(Token::Command(char::from(byte)));
        }
        Some(Token::Number(self.value()))
    }

    /// One number, which may be written as a reference to a formula or to a
    /// modifier rather than as digits.
    fn value(&mut self) -> f32 {
        let mut expression = Expression {
            text: self.text,
            at: self.at,
            formulas: self.formulas,
        };
        // A path's numbers are single values and never arithmetic, so a factor
        // is the whole of what may appear — and `?f7` and `$0` are factors.
        let value = expression.factor();
        self.at = expression.at;
        value
    }

    fn number(&mut self) -> f32 {
        match self.next() {
            Some(Token::Number(number)) => number,
            // A command short of its arguments: nought keeps the pen somewhere
            // rather than ending the shape.
            _ => 0.0,
        }
    }

    fn point(&mut self) -> (f32, f32) {
        (self.number(), self.number())
    }
}
