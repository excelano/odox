//! Styles: the property sets a document names, the chain each one inherits
//! through, and the resolution of a name into the properties a renderer wants.
//!
//! ODF keeps formatting in two places that work identically. A *named* style in
//! `styles.xml` is the one a person chose and can see in an application's style
//! list; an *automatic* style in `content.xml` is the one an application
//! generated for a direct formatting change, named `P1`, `T3`, `ce2`. Both are
//! `style:style` elements, both inherit through `style:parent-style-name`, and
//! nothing downstream needs to know which kind it is holding. They are collected
//! into one table here, keyed by family and name, because ODF scopes a style
//! name within its family and a paragraph style and a cell style may share one.
//!
//! Resolution walks the chain from its root down, so that the nearest style
//! wins, beginning at the family's `style:default-style`. The answer is cached,
//! because a spreadsheet asks for the same handful of cell styles once per
//! visible cell per frame.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::value::{Color, Length, Measure, Percent};
use crate::xml::{Element, Ns};

/// Which kind of thing a style applies to.
///
/// ODF calls this the style family, and it is what makes a style name
/// meaningful: `Standard` names a paragraph style and a table style and they are
/// unrelated.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Family {
    /// `paragraph`
    Paragraph,
    /// `text`, which is a character style.
    Text,
    /// `section`
    Section,
    /// `table`
    Table,
    /// `table-column`
    TableColumn,
    /// `table-row`
    TableRow,
    /// `table-cell`
    TableCell,
    /// `graphic`
    Graphic,
    /// `presentation`, the family a placeholder on a slide takes.
    Presentation,
    /// `drawing-page`, which is a slide's own background and transition.
    DrawingPage,
    /// `chart`
    Chart,
    /// `ruby`
    Ruby,
    /// A family this crate has no name for, kept so that its styles are still
    /// collected and still resolve.
    Other(Box<str>),
}

impl Family {
    /// The family an attribute value names.
    pub fn parse(text: &str) -> Self {
        match text {
            "paragraph" => Self::Paragraph,
            "text" => Self::Text,
            "section" => Self::Section,
            "table" => Self::Table,
            "table-column" => Self::TableColumn,
            "table-row" => Self::TableRow,
            "table-cell" => Self::TableCell,
            "graphic" => Self::Graphic,
            "presentation" => Self::Presentation,
            "drawing-page" => Self::DrawingPage,
            "chart" => Self::Chart,
            "ruby" => Self::Ruby,
            other => Self::Other(other.into()),
        }
    }
}

/// How a paragraph's lines sit against its edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    /// Against the start edge, which for a left-to-right document is the left.
    Start,
    /// Against the end edge.
    End,
    /// Centred.
    Center,
    /// Both edges, by stretching the spaces.
    Justify,
}

/// Where a run sits relative to the baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// On it.
    Baseline,
    /// Above it, smaller.
    Super,
    /// Below it, smaller.
    Sub,
}

/// Vertical placement inside a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalAlign {
    /// Against the top.
    Top,
    /// Centred.
    Middle,
    /// Against the bottom.
    Bottom,
}

/// A page or column break asked for before or after a paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Break {
    /// No break.
    Auto,
    /// A new page.
    Page,
    /// A new column.
    Column,
}

/// One edge of a border, as ODF writes all three of its parts in one attribute.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    /// How thick.
    pub width: Length,
    /// What colour.
    pub color: Color,
}

impl Border {
    /// Parse `0.5pt solid #000000`, in any order of the three parts.
    ///
    /// `None` for `none` and for `hidden`, which are how ODF says there is no
    /// border on this edge — a distinction from the attribute being absent,
    /// which means inherit.
    fn parse(text: &str) -> Option<Self> {
        let mut width = None;
        let mut color = None;
        for word in text.split_whitespace() {
            match word {
                "none" | "hidden" => return None,
                _ => {
                    if let Some(length) = Length::parse(word) {
                        width = Some(length);
                    } else if let Some(parsed) = Color::parse(word) {
                        color = Some(parsed);
                    }
                }
            }
        }
        Some(Self {
            // A border with a style and a colour but no width is the hairline
            // every application draws for one.
            width: width.unwrap_or(Length(0.5)),
            color: color.unwrap_or(Color { r: 0, g: 0, b: 0 }),
        })
    }
}

/// The four edges of a box, in the order ODF's shorthand implies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edges<T> {
    /// Left.
    pub left: Option<T>,
    /// Right.
    pub right: Option<T>,
    /// Top.
    pub top: Option<T>,
    /// Bottom.
    pub bottom: Option<T>,
}

/// Four edges, none of them set. Written out rather than derived because
/// deriving it would require the edge's own type to have a default, and neither
/// a length nor a border has one that means anything: the absence of a border is
/// `None`, not a border of zero width.
impl<T> Default for Edges<T> {
    fn default() -> Self {
        Self {
            left: None,
            right: None,
            top: None,
            bottom: None,
        }
    }
}

/// Character formatting.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextProperties {
    /// The family asked for, after a `style:font-name` has been resolved through
    /// the document's font face declarations.
    pub font_family: Option<String>,
    /// Size, absolute or a proportion of the parent's.
    pub size: Option<Measure>,
    /// Bold.
    pub bold: Option<bool>,
    /// Italic or oblique, which are not distinguished here because a renderer
    /// picking a face cannot honour the difference.
    pub italic: Option<bool>,
    /// Underlined, of any line style.
    pub underline: Option<bool>,
    /// Struck through, of any line style.
    pub strike: Option<bool>,
    /// Ink colour.
    pub color: Option<Color>,
    /// Highlight behind the characters.
    pub background: Option<Color>,
    /// Superscript or subscript.
    pub position: Option<Position>,
    /// Drawn in capitals whatever the text says.
    pub uppercase: Option<bool>,
}

/// Paragraph formatting.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParagraphProperties {
    /// Horizontal alignment.
    pub align: Option<TextAlign>,
    /// Space outside the paragraph on each edge.
    pub margin: Edges<Length>,
    /// The first line's extra indent, which is negative for a hanging indent.
    pub text_indent: Option<Length>,
    /// Line spacing, absolute or a proportion of the font size.
    pub line_height: Option<Measure>,
    /// Fill behind the paragraph.
    pub background: Option<Color>,
    /// A break asked for before the paragraph.
    pub break_before: Option<Break>,
    /// A break asked for after it.
    pub break_after: Option<Break>,
    /// Borders, per edge.
    pub border: Edges<Border>,
    /// Space between the border and the text, per edge.
    pub padding: Edges<Length>,
}

/// Cell formatting, which a spreadsheet reads for every visible cell.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CellProperties {
    /// Fill.
    pub background: Option<Color>,
    /// Vertical placement of the text in the cell.
    pub vertical_align: Option<VerticalAlign>,
    /// Borders, per edge.
    pub border: Edges<Border>,
    /// Space between the border and the text, per edge.
    pub padding: Edges<Length>,
    /// Whether a line too long for the cell wraps rather than overflowing.
    pub wrap: Option<bool>,
}

/// What fills a shape or the ground behind a slide.
///
/// A named gradient rather than the gradient itself, because ODF defines each
/// one once in `office:styles` and refers to it by name from every style that
/// uses it — resolving it here would copy it per style. [`Styles::gradient`]
/// looks it up.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Fill {
    /// Nothing is drawn: whatever is behind shows through.
    #[default]
    None,
    /// One colour.
    Solid(Color),
    /// The gradient of this name.
    Gradient(String),
}

/// How a gradient runs, which is the part of it a renderer has to understand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientStyle {
    /// Along an axis, from one edge to the other.
    Linear,
    /// Along an axis, from the middle outwards to both edges.
    Axial,
    /// Out from a point, in circles.
    Radial,
    /// Out from a point, in ellipses.
    Ellipsoidal,
    /// Out from a point, in squares.
    Square,
    /// Out from a point, in rectangles.
    Rectangular,
}

/// One of a document's named gradients.
///
/// ODF 1.3 also permits a list of `loext:gradient-stop` children, which
/// `LibreOffice` writes alongside the two colour attributes and which say the same
/// thing for a two-stop gradient. The attributes are read and the stops are not:
/// every gradient in the corpus has exactly two stops that repeat what
/// `draw:start-color` and `draw:end-color` already say, and a renderer that
/// interpolated more of them would be drawing something no fixture can check.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    /// How it runs.
    pub style: GradientStyle,
    /// The colour it begins at.
    pub start: Color,
    /// The colour it ends at.
    pub end: Color,
    /// The direction, in degrees. ODF measures it counter-clockwise from the
    /// direction that runs bottom to top, so 0 is upward and 90 points left.
    pub angle: f32,
    /// How much of each end is the flat colour before the blend begins, as a
    /// proportion of the whole.
    pub border: f32,
    /// Where the centre is, for the styles that have one, as a proportion of the
    /// shape's width and height.
    pub center: (f32, f32),
}

/// How a shape is filled and outlined.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphicProperties {
    /// What fills it.
    pub fill: Fill,
    /// Outline colour, absent when the stroke is `none`.
    pub stroke: Option<Color>,
    /// Outline width.
    pub stroke_width: Option<Length>,
    /// How opaque the fill is, from zero to one.
    pub opacity: Option<f32>,
}

/// Everything a resolved style says, across every family.
///
/// One type rather than one per family, because a paragraph carries character
/// properties, a cell carries paragraph properties, and a renderer asking for a
/// cell's font would otherwise have to resolve three styles and merge them
/// itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Properties {
    /// Character formatting.
    pub text: TextProperties,
    /// Paragraph formatting.
    pub paragraph: ParagraphProperties,
    /// Cell formatting.
    pub cell: CellProperties,
    /// Shape formatting.
    pub graphic: GraphicProperties,
    /// Whether the master page's own background shows through, from a
    /// `drawing-page` style. A presentation's, and `None` where nothing said.
    pub background_visible: Option<bool>,
    /// Whether the master page's decorations are drawn.
    pub background_objects_visible: Option<bool>,
    /// A column's width, from a `table-column` style.
    pub column_width: Option<Length>,
    /// A row's height, from a `table-row` style.
    pub row_height: Option<Length>,
}

/// A page's dimensions and margins, from a `style:page-layout`.
#[derive(Debug, Clone, PartialEq)]
pub struct PageLayout {
    /// The page's width.
    pub width: Length,
    /// Its height.
    pub height: Length,
    /// The margins inside it.
    pub margin: Edges<Length>,
}

impl Default for PageLayout {
    /// US Letter portrait with one-inch margins.
    ///
    /// Reached only by a document that declares no page layout at all, which no
    /// office application produces: the answer normally comes from the document's
    /// own master page and this is never consulted. The size still has to be
    /// something, and it is the paper the producer on this machine writes —
    /// `LibreOffice` 25.2 puts `8.5in` by `11in` in every document it creates here.
    /// A4 would be the other choice and neither is more right.
    fn default() -> Self {
        Self {
            width: Length(612.0),
            height: Length(792.0),
            margin: Edges {
                left: Some(Length(72.0)),
                right: Some(Length(72.0)),
                top: Some(Length(72.0)),
                bottom: Some(Length(72.0)),
            },
        }
    }
}

/// One style as the document declares it.
pub struct Style {
    /// Its name, as everything else refers to it.
    pub name: String,
    /// The name a person sees in an application's style list, where it differs
    /// from the name in the file. ODF encodes a space as `_20_` in a style name
    /// and carries the readable spelling in this attribute.
    pub display_name: Option<String>,
    /// Its family.
    pub family: Family,
    /// The style it inherits from.
    pub parent: Option<String>,
    /// The `style:style` element itself, so that a property this crate does not
    /// model is still reachable.
    pub element: Element,
}

/// Every style in a document, from both of the places they live.
pub struct Styles {
    by_name: HashMap<(Family, String), Style>,
    defaults: HashMap<Family, Element>,
    page_layouts: HashMap<String, PageLayout>,
    master_pages: HashMap<String, Element>,
    lists: HashMap<String, Element>,
    gradients: HashMap<String, Gradient>,
    font_faces: HashMap<String, String>,
    cache: RefCell<HashMap<(Family, String), Rc<Properties>>>,
}

impl Styles {
    /// Collect the styles of a document from its two parts.
    ///
    /// Both are optional because both are optional in the format: a package may
    /// carry all its formatting in automatic styles in `content.xml` and no
    /// `styles.xml` at all.
    pub fn collect(content: Option<&Element>, styles: Option<&Element>) -> Self {
        let mut this = Self {
            by_name: HashMap::new(),
            defaults: HashMap::new(),
            page_layouts: HashMap::new(),
            master_pages: HashMap::new(),
            lists: HashMap::new(),
            gradients: HashMap::new(),
            font_faces: HashMap::new(),
            cache: RefCell::new(HashMap::new()),
        };
        // styles.xml first, so that an automatic style in content.xml with the
        // same family and name as a named one wins. They do not collide in
        // practice, and the order is what decides if they ever do.
        for root in [styles, content].into_iter().flatten() {
            this.collect_from(root);
        }
        this
    }

    fn collect_from(&mut self, root: &Element) {
        for container in root.elements() {
            match () {
                () if container.is(&Ns::Office, "font-face-decls") => {
                    for face in container.elements() {
                        let Some(name) = face.attr(&Ns::Style, "name") else {
                            continue;
                        };
                        // `svg:font-family` is the family a renderer asks the
                        // system for; the declaration's own name is only a
                        // handle that the styles refer to it by, and the two
                        // usually but not always agree.
                        let family = face
                            .attr(&Ns::Svg, "font-family")
                            .unwrap_or(name)
                            .trim_matches('\'')
                            .to_owned();
                        self.font_faces.insert(name.to_owned(), family);
                    }
                }
                () if container.is(&Ns::Office, "styles")
                    || container.is(&Ns::Office, "automatic-styles") =>
                {
                    for element in container.elements() {
                        self.collect_style(element);
                    }
                }
                () if container.is(&Ns::Office, "master-styles") => {
                    for master in container.elements() {
                        if let Some(name) = master.attr(&Ns::Style, "name") {
                            self.master_pages.insert(name.to_owned(), master.clone());
                        }
                    }
                }
                () => {}
            }
        }
    }

    fn collect_style(&mut self, element: &Element) {
        if element.is(&Ns::Style, "style") {
            let Some(name) = element.attr(&Ns::Style, "name") else {
                return;
            };
            let family = Family::parse(element.attr(&Ns::Style, "family").unwrap_or_default());
            let style = Style {
                name: name.to_owned(),
                display_name: element
                    .attr(&Ns::Style, "display-name")
                    .map(ToOwned::to_owned),
                family: family.clone(),
                parent: element
                    .attr(&Ns::Style, "parent-style-name")
                    .map(ToOwned::to_owned),
                element: element.clone(),
            };
            self.by_name.insert((family, name.to_owned()), style);
        } else if element.is(&Ns::Style, "default-style") {
            let family = Family::parse(element.attr(&Ns::Style, "family").unwrap_or_default());
            self.defaults.insert(family, element.clone());
        } else if element.is(&Ns::Style, "page-layout") {
            if let Some(name) = element.attr(&Ns::Style, "name") {
                self.page_layouts
                    .insert(name.to_owned(), page_layout(element));
            }
        } else if element.is(&Ns::Draw, "gradient") {
            if let Some(name) = element.attr(&Ns::Draw, "name") {
                self.gradients.insert(name.to_owned(), gradient(element));
            }
        } else if element.is(&Ns::Text, "list-style")
            && let Some(name) = element.attr(&Ns::Style, "name")
        {
            self.lists.insert(name.to_owned(), element.clone());
        }
    }

    /// A style by family and name.
    pub fn style(&self, family: &Family, name: &str) -> Option<&Style> {
        self.by_name.get(&(family.clone(), name.to_owned()))
    }

    /// A gradient by name, as a fill refers to one.
    pub fn gradient(&self, name: &str) -> Option<&Gradient> {
        self.gradients.get(name)
    }

    /// A list style by name, as the `text:list-style-name` of a list refers to
    /// one.
    pub fn list_style(&self, name: &str) -> Option<&Element> {
        self.lists.get(name)
    }

    /// A master page by name.
    pub fn master_page(&self, name: &str) -> Option<&Element> {
        self.master_pages.get(name)
    }

    /// The page layout a master page points at.
    pub fn page_layout_of(&self, master_page: &str) -> Option<&PageLayout> {
        let master = self.master_pages.get(master_page)?;
        let layout = master.attr(&Ns::Style, "page-layout-name")?;
        self.page_layouts.get(layout)
    }

    /// The resolved properties of a style, with its whole inheritance chain and
    /// its family's default applied.
    ///
    /// A name that is not in the document resolves to the family's default,
    /// which is what an application does with a dangling style reference: the
    /// paragraph is shown rather than refused.
    pub fn resolve(&self, family: &Family, name: &str) -> Rc<Properties> {
        let key = (family.clone(), name.to_owned());
        if let Some(cached) = self.cache.borrow().get(&key) {
            return Rc::clone(cached);
        }

        let mut properties = Properties::default();
        if let Some(default) = self.defaults.get(family) {
            properties.apply(default, &self.font_faces);
        }
        // Root first, so that the style asked for is applied last and wins.
        for style in self.chain(family, name).into_iter().rev() {
            properties.apply(&style.element, &self.font_faces);
        }

        let properties = Rc::new(properties);
        self.cache.borrow_mut().insert(key, Rc::clone(&properties));
        properties
    }

    /// The style and its ancestors, nearest first.
    ///
    /// A cycle in the chain — which no writer produces and a hand-edited file
    /// can — stops at the style it returns to rather than hanging.
    fn chain(&self, family: &Family, name: &str) -> Vec<&Style> {
        let mut chain: Vec<&Style> = Vec::new();
        let mut next: Option<&str> = Some(name);
        while let Some(current) = next {
            if chain.iter().any(|s| s.name == current) {
                break;
            }
            let Some(style) = self.style(family, current) else {
                break;
            };
            next = style.parent.as_deref();
            chain.push(style);
        }
        chain
    }

    /// The family the properties of a cell's text come from: a cell style's
    /// `style:parent-style-name` chain carries the paragraph and text
    /// properties, so a cell resolves in one call.
    pub fn font_family(&self, declared: &str) -> String {
        self.font_faces
            .get(declared)
            .cloned()
            .unwrap_or_else(|| declared.to_owned())
    }
}

fn gradient(element: &Element) -> Gradient {
    let color = |local: &str, fallback: Color| {
        element
            .attr(&Ns::Draw, local)
            .and_then(Color::parse)
            .unwrap_or(fallback)
    };
    let proportion = |local: &str| {
        element
            .attr(&Ns::Draw, local)
            .and_then(Percent::parse)
            .map_or(0.0, Percent::fraction)
    };
    Gradient {
        style: match element.attr(&Ns::Draw, "style") {
            Some("axial") => GradientStyle::Axial,
            Some("radial") => GradientStyle::Radial,
            Some("ellipsoid") => GradientStyle::Ellipsoidal,
            Some("square") => GradientStyle::Square,
            Some("rectangular") => GradientStyle::Rectangular,
            _ => GradientStyle::Linear,
        },
        start: color("start-color", Color { r: 0, g: 0, b: 0 }),
        end: color(
            "end-color",
            Color {
                r: 0xff,
                g: 0xff,
                b: 0xff,
            },
        ),
        // Written as `270deg`, and occasionally as a bare tenth of a degree by
        // producers older than the unit.
        angle: element.attr(&Ns::Draw, "angle").map_or(0.0, parse_angle),
        border: proportion("border"),
        center: (proportion("cx"), proportion("cy")),
    }
}

/// An ODF angle in degrees.
///
/// `270deg` is the spelling ODF 1.2 introduced. Before it the attribute was a
/// plain number in tenths of a degree, which some producers still write, so a
/// value with no unit is read that way.
fn parse_angle(text: &str) -> f32 {
    let text = text.trim();
    match text.strip_suffix("deg") {
        Some(degrees) => degrees.trim().parse().unwrap_or(0.0),
        None => text.parse::<f32>().unwrap_or(0.0) / 10.0,
    }
}

fn page_layout(element: &Element) -> PageLayout {
    let mut layout = PageLayout::default();
    if let Some(properties) = element.child(&Ns::Style, "page-layout-properties") {
        if let Some(width) = properties
            .attr(&Ns::Fo, "page-width")
            .and_then(Length::parse)
        {
            layout.width = width;
        }
        if let Some(height) = properties
            .attr(&Ns::Fo, "page-height")
            .and_then(Length::parse)
        {
            layout.height = height;
        }
        let mut margin = Edges::default();
        read_edges(properties, "margin", &mut margin, Length::parse);
        // An edge the layout does not name keeps the default rather than
        // becoming nothing, because a page layout that sets only its top margin
        // is not asking for the other three to be zero.
        layout.margin.left = margin.left.or(layout.margin.left);
        layout.margin.right = margin.right.or(layout.margin.right);
        layout.margin.top = margin.top.or(layout.margin.top);
        layout.margin.bottom = margin.bottom.or(layout.margin.bottom);
    }
    layout
}

/// Read ODF's edge shorthand: `fo:margin` sets all four, and
/// `fo:margin-left` and its siblings override one each.
fn read_edges<T: Copy>(
    properties: &Element,
    base: &str,
    into: &mut Edges<T>,
    parse: impl Fn(&str) -> Option<T>,
) {
    if let Some(all) = properties.attr(&Ns::Fo, base).and_then(&parse) {
        *into = Edges {
            left: Some(all),
            right: Some(all),
            top: Some(all),
            bottom: Some(all),
        };
    }
    if let Some(v) = properties
        .attr(&Ns::Fo, &format!("{base}-left"))
        .and_then(&parse)
    {
        into.left = Some(v);
    }
    if let Some(v) = properties
        .attr(&Ns::Fo, &format!("{base}-right"))
        .and_then(&parse)
    {
        into.right = Some(v);
    }
    if let Some(v) = properties
        .attr(&Ns::Fo, &format!("{base}-top"))
        .and_then(&parse)
    {
        into.top = Some(v);
    }
    if let Some(v) = properties
        .attr(&Ns::Fo, &format!("{base}-bottom"))
        .and_then(&parse)
    {
        into.bottom = Some(v);
    }
}

impl Properties {
    /// Apply one style's property elements over what is already here.
    ///
    /// Only a property the element states is changed; everything else keeps the
    /// value it inherited, which is what makes walking a chain root first give
    /// the right answer.
    fn apply(&mut self, style: &Element, font_faces: &HashMap<String, String>) {
        for properties in style.elements() {
            if properties.is(&Ns::Style, "text-properties") {
                self.text.apply(properties, font_faces);
            } else if properties.is(&Ns::Style, "paragraph-properties") {
                self.paragraph.apply(properties);
            } else if properties.is(&Ns::Style, "table-cell-properties") {
                self.cell.apply(properties);
            } else if properties.is(&Ns::Style, "graphic-properties")
                || properties.is(&Ns::Style, "drawing-page-properties")
            {
                self.graphic.apply(properties);
                // Two of a slide's own switches over what its master gives it.
                // They live on the same element as the fill and are read here so
                // that they inherit through the style chain like everything else.
                if let Some(visible) = properties
                    .attr(&Ns::Presentation, "background-visible")
                    .and_then(crate::value::boolean)
                {
                    self.background_visible = Some(visible);
                }
                if let Some(visible) = properties
                    .attr(&Ns::Presentation, "background-objects-visible")
                    .and_then(crate::value::boolean)
                {
                    self.background_objects_visible = Some(visible);
                }
            } else if properties.is(&Ns::Style, "table-column-properties") {
                if let Some(width) = properties
                    .attr(&Ns::Style, "column-width")
                    .and_then(Length::parse)
                {
                    self.column_width = Some(width);
                }
            } else if properties.is(&Ns::Style, "table-row-properties")
                && let Some(height) = properties
                    .attr(&Ns::Style, "row-height")
                    .and_then(Length::parse)
            {
                self.row_height = Some(height);
            }
        }
    }
}

impl TextProperties {
    fn apply(&mut self, p: &Element, font_faces: &HashMap<String, String>) {
        // `style:font-name` points at a font face declaration and `fo:font-family`
        // names a family directly. A style may carry both, and the declaration is
        // the more specific of the two.
        if let Some(name) = p.attr(&Ns::Fo, "font-family") {
            self.font_family = Some(name.trim_matches('\'').to_owned());
        }
        if let Some(name) = p.attr(&Ns::Style, "font-name") {
            self.font_family = Some(
                font_faces
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| name.to_owned()),
            );
        }
        if let Some(size) = p.attr(&Ns::Fo, "font-size").and_then(Measure::parse) {
            self.size = Some(size);
        }
        if let Some(weight) = p.attr(&Ns::Fo, "font-weight") {
            // A numeric weight is the CSS scale, where 600 and above reads as
            // bold to anything that has two faces to choose between.
            self.bold = Some(match weight {
                "normal" => false,
                "bold" => true,
                other => other.parse::<u32>().is_ok_and(|n| n >= 600),
            });
        }
        if let Some(style) = p.attr(&Ns::Fo, "font-style") {
            self.italic = Some(style != "normal");
        }
        if let Some(line) = p.attr(&Ns::Style, "text-underline-style") {
            self.underline = Some(line != "none");
        }
        if let Some(line) = p.attr(&Ns::Style, "text-line-through-style") {
            self.strike = Some(line != "none");
        }
        if let Some(color) = p.attr(&Ns::Fo, "color") {
            self.color = Color::parse(color);
        }
        if let Some(color) = p.attr(&Ns::Fo, "background-color") {
            self.background = Color::parse(color);
        }
        if let Some(position) = p.attr(&Ns::Style, "text-position") {
            // The attribute is a vertical offset and optionally a size, as in
            // `super 58%` or `-33% 58%`. Only the direction is read: a renderer
            // that placed the glyph at the stated offset and scaled it by the
            // stated amount would be doing typesetting, and what is wanted here
            // is the distinction between superscript and subscript.
            let first = position.split_whitespace().next().unwrap_or_default();
            self.position = Some(match first {
                "super" => Position::Super,
                "sub" => Position::Sub,
                _ => match Percent::parse(first) {
                    Some(percent) if percent.0 > 0.0 => Position::Super,
                    Some(percent) if percent.0 < 0.0 => Position::Sub,
                    _ => Position::Baseline,
                },
            });
        }
        if let Some(transform) = p.attr(&Ns::Fo, "text-transform") {
            self.uppercase = Some(transform == "uppercase");
        }
    }
}

impl ParagraphProperties {
    fn apply(&mut self, p: &Element) {
        if let Some(align) = p.attr(&Ns::Fo, "text-align") {
            self.align = match align {
                // `left` and `right` are the writing-direction-independent
                // spellings' siblings, and for a left-to-right document they are
                // the same thing. A right-to-left document would need the
                // direction to tell them apart, which is what §6 of DESIGN.md
                // says this release does not do.
                "start" | "left" => Some(TextAlign::Start),
                "end" | "right" => Some(TextAlign::End),
                "center" => Some(TextAlign::Center),
                "justify" => Some(TextAlign::Justify),
                _ => self.align,
            };
        }
        read_edges(p, "margin", &mut self.margin, Length::parse);
        read_edges(p, "padding", &mut self.padding, Length::parse);
        read_edges(p, "border", &mut self.border, Border::parse);
        if let Some(indent) = p.attr(&Ns::Fo, "text-indent").and_then(Length::parse) {
            self.text_indent = Some(indent);
        }
        if let Some(height) = p.attr(&Ns::Fo, "line-height") {
            self.line_height = Measure::parse(height);
        }
        if let Some(color) = p.attr(&Ns::Fo, "background-color") {
            self.background = Color::parse(color);
        }
        if let Some(before) = p.attr(&Ns::Fo, "break-before") {
            self.break_before = Some(parse_break(before));
        }
        if let Some(after) = p.attr(&Ns::Fo, "break-after") {
            self.break_after = Some(parse_break(after));
        }
    }
}

fn parse_break(text: &str) -> Break {
    match text {
        "page" => Break::Page,
        "column" => Break::Column,
        _ => Break::Auto,
    }
}

impl CellProperties {
    fn apply(&mut self, p: &Element) {
        if let Some(color) = p.attr(&Ns::Fo, "background-color") {
            self.background = Color::parse(color);
        }
        if let Some(align) = p.attr(&Ns::Style, "vertical-align") {
            self.vertical_align = match align {
                "top" => Some(VerticalAlign::Top),
                "middle" => Some(VerticalAlign::Middle),
                // `bottom`, and `automatic`, which is the fourth value ODF
                // defines and means bottom for a cell: it is what a spreadsheet
                // shows for a cell nobody has set.
                _ => Some(VerticalAlign::Bottom),
            };
        }
        read_edges(p, "border", &mut self.border, Border::parse);
        read_edges(p, "padding", &mut self.padding, Length::parse);
        if let Some(wrap) = p.attr(&Ns::Fo, "wrap-option") {
            self.wrap = Some(wrap == "wrap");
        }
    }
}

impl GraphicProperties {
    fn apply(&mut self, p: &Element) {
        // `draw:fill` says which kind, and the value each kind needs is a
        // separate attribute — so a style that switches a shape from a colour to
        // a gradient carries both, and reading the colour without the kind gets
        // the old answer.
        match p.attr(&Ns::Draw, "fill") {
            // Solid, and the case of no `draw:fill` at all: a colour on its own
            // still sets one, which is how a style that only changes the shade
            // of an already-solid shape is written. Neither states a colour
            // every time, and the one it does not state is the one inherited.
            Some("solid") | None => {
                if let Some(color) = p.attr(&Ns::Draw, "fill-color").and_then(Color::parse) {
                    self.fill = Fill::Solid(color);
                }
            }
            Some("gradient") => {
                if let Some(name) = p.attr(&Ns::Draw, "fill-gradient-name") {
                    self.fill = Fill::Gradient(name.to_owned());
                }
            }
            // `none`, and the bitmap and hatch fills this does not draw. They
            // share an arm because they share an outcome: leaving an inherited
            // colour in place would fill the shape with something the document
            // did not ask for and call it the pattern.
            Some(_) => self.fill = Fill::None,
        }
        match p.attr(&Ns::Draw, "stroke") {
            Some("none") => self.stroke = None,
            _ => {
                if let Some(color) = p.attr(&Ns::Svg, "stroke-color").and_then(Color::parse) {
                    self.stroke = Some(color);
                }
            }
        }
        if let Some(width) = p.attr(&Ns::Svg, "stroke-width").and_then(Length::parse) {
            self.stroke_width = Some(width);
        }
        if let Some(opacity) = p.attr(&Ns::Draw, "opacity").and_then(Percent::parse) {
            self.opacity = Some(opacity.fraction().clamp(0.0, 1.0));
        }
    }
}
