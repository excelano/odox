//! A text document read into the blocks an export lays out.
//!
//! The reading follows the window's (`odox-ui`'s flow): the same elements are
//! blocks, the same ones pass their text through, a tab is the same four
//! spaces, a footnote is its citation, and a picture anchored in a paragraph
//! comes after it. What the window draws is what the PDF holds, in the same
//! order.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::ops::Range;

use krilla::tagging::ListNumbering;
use odox_core::edit::{self, Description};
use odox_core::{
    Border, Break, Color, Document, Edges, Element, Family, Length, Measure, Node, Ns, Position,
    Properties, TextAlign, TextProperties,
};
use odox_fonts::Variant;

/// The size text is drawn at where a document names none, as in the window.
const DEFAULT_SIZE: f32 = 12.0;
/// How large a superscript or subscript is, as a proportion of its run.
pub(crate) const SCRIPT_SCALE: f32 = 0.58;
/// How large a footnote's citation is, as a proportion of its run.
const CITATION_SCALE: f32 = 0.7;
/// How far each list level is indented.
const LIST_STEP: f32 = 18.0;
/// The space a list label is drawn in, left of its item's text.
const LIST_GUTTER: f32 = 20.0;

/// One thing in the flow.
pub(crate) enum Block {
    Paragraph(Paragraph),
    List(List),
    Table(Table),
    Figure(Figure),
    TextBox(TextBox),
    Note(Note),
}

/// A paragraph or a heading.
pub(crate) struct Paragraph {
    /// The heading level, for a heading.
    pub heading: Option<u8>,
    /// Its characters, as drawn.
    pub text: String,
    /// The runs the text is drawn in, covering it in order.
    pub runs: Vec<Run>,
    /// What a run with an empty paragraph, or a list label, is drawn in.
    pub base: Style,
    /// Where each link goes, as a run's `link` counts them.
    pub links: Vec<String>,
    pub align: TextAlign,
    pub left: f32,
    pub right: f32,
    /// The first line's extra indent.
    pub indent: f32,
    pub before: f32,
    pub after: f32,
    pub line_height: Option<Measure>,
    pub background: Option<Color>,
    pub border: Edges<Border>,
    /// A new page before it, or after it.
    pub page_before: bool,
    pub page_after: bool,
}

impl Paragraph {
    /// A line of text in one style and nothing else: a list label.
    pub(crate) fn plain(text: &str, style: &Style) -> Self {
        Self {
            heading: None,
            text: text.to_owned(),
            runs: vec![Run {
                range: 0..text.len(),
                style: style.clone(),
            }],
            base: style.clone(),
            links: Vec::new(),
            align: TextAlign::Start,
            left: 0.0,
            right: 0.0,
            indent: 0.0,
            before: 0.0,
            after: 0.0,
            line_height: None,
            background: None,
            border: Edges::default(),
            page_before: false,
            page_after: false,
        }
    }
}

/// Characters in one style.
pub(crate) struct Run {
    pub range: Range<usize>,
    pub style: Style,
}

/// How a run is drawn.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Style {
    pub family: String,
    pub variant: Variant,
    /// The size its glyphs are drawn at, after any raising.
    pub size: f32,
    pub color: Option<Color>,
    pub background: Option<Color>,
    pub underline: bool,
    pub strike: bool,
    /// How far above the baseline it sits, in points.
    pub rise: f32,
    /// The link it belongs to, by index into the paragraph's links.
    pub link: Option<usize>,
}

/// A list.
pub(crate) struct List {
    pub numbering: ListNumbering,
    pub items: Vec<Item>,
}

/// One item of a list.
pub(crate) struct Item {
    /// The label, which a list header does not have.
    pub label: Option<String>,
    /// How far the item's blocks are indented from the list's own edge.
    pub indent: f32,
    pub blocks: Vec<Block>,
}

/// A table.
pub(crate) struct Table {
    /// Each column's declared width, where it has one.
    pub columns: Vec<Option<f32>>,
    /// How many of the rows are header rows, which come first.
    pub header_rows: usize,
    pub rows: Vec<Vec<Cell>>,
}

/// A cell of a table.
pub(crate) struct Cell {
    pub column: usize,
    /// How many columns, and how many rows, it covers.
    pub span: usize,
    pub rows: usize,
    pub blocks: Vec<Block>,
    pub background: Option<Color>,
    pub border: Edges<Border>,
    pub padding: f32,
}

/// A picture.
pub(crate) struct Figure {
    pub image: krilla::image::Image,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub description: Description,
    /// Where its frame is, from the content root.
    pub path: Vec<usize>,
}

/// A frame of text drawn in a box.
pub(crate) struct TextBox {
    pub width: Option<f32>,
    pub blocks: Vec<Block>,
}

/// A footnote or an endnote, with the citation the text calls it by.
pub(crate) struct Note {
    pub citation: String,
    pub blocks: Vec<Block>,
}

/// Read the blocks under an element, with its path from the content root, and
/// the notes cited in them after the rest, in the order they are cited.
///
/// A PDF has pages and could hold a footnote at the foot of one, but laying
/// one out there means taking room from the page the line citing it is on;
/// the notes are gathered at the end of the document instead.
pub(crate) fn read(document: &Document, root: &Element, path: Vec<usize>) -> Vec<Block> {
    let mut reader = Reader {
        document,
        path,
        notes: Vec::new(),
    };
    let mut blocks = Vec::new();
    reader.blocks(root, &mut blocks, &mut Counters::default());
    blocks.extend(reader.notes.into_iter().map(Block::Note));
    blocks
}

struct Reader<'a> {
    document: &'a Document,
    path: Vec<usize>,
    /// The notes cited so far.
    notes: Vec<Note>,
}

/// Where a list's counters stand, one per level.
#[derive(Default)]
struct Counters(Vec<usize>);

impl Counters {
    fn bump(&mut self, level: usize, start: usize) -> usize {
        while self.0.len() <= level {
            self.0.push(0);
        }
        if self.0[level] == 0 {
            self.0[level] = start;
        } else {
            self.0[level] += 1;
        }
        self.0.truncate(level + 1);
        self.0[level]
    }

    fn at(&self, level: usize) -> usize {
        self.0.get(level).copied().unwrap_or(1)
    }
}

impl Reader<'_> {
    fn blocks(&mut self, parent: &Element, into: &mut Vec<Block>, counters: &mut Counters) {
        for (index, element) in parent.elements_indexed() {
            self.path.push(index);
            if element.is(&Ns::Text, "p") || element.is(&Ns::Text, "h") {
                self.paragraph(element, into);
            } else if element.is(&Ns::Text, "list") {
                let continues = element.attr(&Ns::Text, "continue-numbering") == Some("true")
                    || element.attr(&Ns::Text, "continue-list").is_some();
                if !continues {
                    *counters = Counters::default();
                }
                self.list(element, 0, None, counters, into);
            } else if element.is(&Ns::Table, "table") {
                into.push(Block::Table(self.table(element)));
            } else if element.is(&Ns::Draw, "frame") {
                self.frame(element, into);
            } else if edit::is_block_container(element) {
                self.blocks(element, into, counters);
            }
            self.path.pop();
        }
    }

    fn style_of(&self, element: &Element, family: &Family) -> std::rc::Rc<Properties> {
        let name = element
            .attr(&Ns::Text, "style-name")
            .or_else(|| element.attr(&Ns::Table, "style-name"))
            .unwrap_or("Standard");
        self.document.styles.resolve(family, name)
    }

    fn paragraph(&mut self, element: &Element, into: &mut Vec<Block>) {
        let properties = self.style_of(element, &Family::Paragraph);
        let size = size_of(&properties.text, DEFAULT_SIZE);
        let base = style(&properties.text, size, None);
        let mut text = Text {
            text: String::new(),
            runs: Vec::new(),
            links: Vec::new(),
        };
        let mut anchored = Vec::new();
        self.runs(
            element,
            &properties.text,
            size,
            None,
            &mut text,
            &mut anchored,
            &mut self.path.clone(),
        );
        let points = |length: Option<Length>| length.map_or(0.0, Length::points);
        let p = &properties.paragraph;
        into.push(Block::Paragraph(Paragraph {
            heading: edit::heading_level(element),
            text: text.text,
            runs: text.runs,
            base,
            links: text.links,
            align: p.align.unwrap_or(TextAlign::Start),
            left: points(p.margin.left),
            right: points(p.margin.right),
            indent: points(p.text_indent),
            before: points(p.margin.top),
            after: points(p.margin.bottom),
            line_height: p.line_height,
            background: p.background,
            border: p.border,
            page_before: p.break_before == Some(Break::Page),
            page_after: p.break_after == Some(Break::Page),
        }));
        for (element, path) in anchored {
            let outer = std::mem::replace(&mut self.path, path);
            if element.is(&Ns::Text, "note") {
                self.note(&element);
            } else {
                self.frame(&element, into);
            }
            self.path = outer;
        }
    }

    /// A note's body, read into the notes for the end of the document.
    fn note(&mut self, note: &Element) {
        let citation = note
            .child(&Ns::Text, "note-citation")
            .map(Element::plain_text)
            .unwrap_or_default();
        let Some((index, body)) = note
            .elements_indexed()
            .find(|(_, e)| e.is(&Ns::Text, "note-body"))
        else {
            return;
        };
        self.path.push(index);
        let mut blocks = Vec::new();
        self.blocks(body, &mut blocks, &mut Counters::default());
        self.path.pop();
        self.notes.push(Note { citation, blocks });
    }

    /// Append the text of a paragraph's children, collecting the frames
    /// anchored in it and the notes cited in it, each with its path.
    #[allow(clippy::too_many_arguments)]
    fn runs(
        &self,
        parent: &Element,
        inherited: &TextProperties,
        size: f32,
        link: Option<usize>,
        text: &mut Text,
        anchored: &mut Vec<(Element, Vec<usize>)>,
        path: &mut Vec<usize>,
    ) {
        let current = style(inherited, size, link);
        for (index, child) in parent.children.iter().enumerate() {
            match child {
                Node::Text(t) | Node::CData(t) => text.push(t, &current, inherited),
                Node::Comment(_) | Node::ProcessingInstruction(_) => {}
                Node::Element(element) => {
                    path.push(index);
                    if element.is(&Ns::Text, "s") {
                        let count = element.attr_usize(&Ns::Text, "c").unwrap_or(1);
                        text.push(&" ".repeat(count.min(256)), &current, inherited);
                    } else if element.is(&Ns::Text, "tab") {
                        text.push("    ", &current, inherited);
                    } else if element.is(&Ns::Text, "line-break") {
                        text.push("\n", &current, inherited);
                    } else if element.is(&Ns::Text, "span") || element.is(&Ns::Text, "a") {
                        let own = self.style_of(element, &Family::Text);
                        let merged = own.text.over(inherited);
                        let inner = size_of(&merged, size);
                        let link = if element.is(&Ns::Text, "a") {
                            text.links.push(
                                element
                                    .attr(&Ns::Xlink, "href")
                                    .unwrap_or_default()
                                    .to_owned(),
                            );
                            Some(text.links.len() - 1)
                        } else {
                            link
                        };
                        self.runs(element, &merged, inner, link, text, anchored, path);
                    } else if element.is(&Ns::Draw, "frame") {
                        anchored.push((element.clone(), path.clone()));
                    } else if element.is(&Ns::Text, "note") {
                        if let Some(citation) = element.child(&Ns::Text, "note-citation") {
                            let mut raised = current.clone();
                            raised.size = size * CITATION_SCALE;
                            raised.rise = size * 0.33;
                            text.push(&citation.plain_text(), &raised, inherited);
                        }
                        anchored.push((element.clone(), path.clone()));
                    } else if edit::is_inline_passthrough(element) {
                        self.runs(element, inherited, size, link, text, anchored, path);
                    }
                    path.pop();
                }
            }
        }
    }

    fn list(
        &mut self,
        element: &Element,
        level: usize,
        inherited_style: Option<&str>,
        counters: &mut Counters,
        into: &mut Vec<Block>,
    ) {
        let style_name = element
            .attr(&Ns::Text, "style-name")
            .or(inherited_style)
            .unwrap_or_default()
            .to_owned();
        #[allow(clippy::cast_precision_loss)]
        let indent = LIST_STEP * (level as f32 + 1.0) + LIST_GUTTER;
        let level_style = self.level_style(&style_name, level);
        let mut list = List {
            numbering: numbering(level_style.as_ref()),
            items: Vec::new(),
        };
        for (item_index, item) in element.elements_indexed() {
            let numbered = item.is(&Ns::Text, "list-item");
            if !numbered && !item.is(&Ns::Text, "list-header") {
                continue;
            }
            self.path.push(item_index);
            let label = numbered.then(|| {
                let start = level_style
                    .as_ref()
                    .and_then(|s| s.attr_usize(&Ns::Text, "start-value"))
                    .unwrap_or(1);
                let number = counters.bump(level, start);
                label(level_style.as_ref(), level, number, counters)
            });
            let mut blocks = Vec::new();
            for (block_index, block) in item.elements_indexed() {
                self.path.push(block_index);
                if block.is(&Ns::Text, "list") {
                    self.list(block, level + 1, Some(&style_name), counters, &mut blocks);
                } else if block.is(&Ns::Text, "p") || block.is(&Ns::Text, "h") {
                    self.paragraph(block, &mut blocks);
                } else if block.is(&Ns::Table, "table") {
                    blocks.push(Block::Table(self.table(block)));
                } else if block.is(&Ns::Draw, "frame") {
                    self.frame(block, &mut blocks);
                }
                self.path.pop();
            }
            list.items.push(Item {
                label,
                indent,
                blocks,
            });
            self.path.pop();
        }
        into.push(Block::List(list));
    }

    fn level_style(&self, list_style: &str, level: usize) -> Option<Element> {
        let style = self.document.styles.list_style(list_style)?;
        style
            .elements()
            .find(|e| e.attr_usize(&Ns::Text, "level") == Some(level + 1))
            .cloned()
    }

    fn table(&mut self, table: &Element) -> Table {
        let mut columns = Vec::new();
        self.columns(table, &mut columns);
        let mut out = Table {
            columns,
            header_rows: 0,
            rows: Vec::new(),
        };
        self.rows(table, false, &mut out);
        out
    }

    fn columns(&self, parent: &Element, into: &mut Vec<Option<f32>>) {
        for element in parent.elements() {
            if element.is(&Ns::Table, "table-column") {
                let repeat = element
                    .attr_usize(&Ns::Table, "number-columns-repeated")
                    .unwrap_or(1)
                    .clamp(1, 1024);
                let width = element
                    .attr(&Ns::Table, "style-name")
                    .map(|name| self.document.styles.resolve(&Family::TableColumn, name))
                    .and_then(|p| p.column_width)
                    .map(Length::points);
                into.extend(std::iter::repeat_n(width, repeat));
            } else if element.is(&Ns::Table, "table-columns")
                || element.is(&Ns::Table, "table-header-columns")
                || element.is(&Ns::Table, "table-column-group")
            {
                self.columns(element, into);
            }
        }
    }

    fn rows(&mut self, parent: &Element, header: bool, table: &mut Table) {
        for (index, element) in parent.elements_indexed() {
            self.path.push(index);
            if element.is(&Ns::Table, "table-row") {
                let row = self.row(element);
                // A header row only counts as one while every row before it
                // is one too: a header in the middle of a table is drawn
                // where it is and read as an ordinary row.
                if header && table.header_rows == table.rows.len() {
                    table.header_rows += 1;
                }
                table.rows.push(row);
            } else if element.is(&Ns::Table, "table-header-rows") {
                self.rows(element, true, table);
            } else if element.is(&Ns::Table, "table-rows")
                || element.is(&Ns::Table, "table-row-group")
            {
                self.rows(element, header, table);
            }
            self.path.pop();
        }
    }

    fn row(&mut self, row: &Element) -> Vec<Cell> {
        let mut cells = Vec::new();
        let mut column = 0;
        for (index, cell) in row.elements_indexed() {
            let covered = cell.is(&Ns::Table, "covered-table-cell");
            if !covered && !cell.is(&Ns::Table, "table-cell") {
                continue;
            }
            let repeat = cell
                .attr_usize(&Ns::Table, "number-columns-repeated")
                .unwrap_or(1)
                .clamp(1, 1024);
            let span = cell
                .attr_usize(&Ns::Table, "number-columns-spanned")
                .unwrap_or(1)
                .max(1);
            let rows = cell
                .attr_usize(&Ns::Table, "number-rows-spanned")
                .unwrap_or(1)
                .max(1);
            for _ in 0..repeat {
                if !covered {
                    let properties = self.style_of(cell, &Family::TableCell);
                    self.path.push(index);
                    let mut blocks = Vec::new();
                    self.blocks(cell, &mut blocks, &mut Counters::default());
                    self.path.pop();
                    cells.push(Cell {
                        column,
                        span,
                        rows,
                        blocks,
                        background: properties.cell.background,
                        border: properties.cell.border,
                        padding: properties.cell.padding.left.map_or(2.0, Length::points),
                    });
                }
                column += 1;
            }
        }
        cells
    }

    /// A frame: a picture, or a box of text. Anything else — an object with
    /// no picture of itself, a chart, a formula — draws nothing, where the
    /// window leaves an empty box.
    fn frame(&mut self, frame: &Element, into: &mut Vec<Block>) {
        let declared = |local: &str| {
            frame
                .attr(&Ns::Svg, local)
                .and_then(Length::parse)
                .map(Length::points)
        };
        if edit::is_figure(frame) {
            let image = frame
                .elements()
                .filter(|child| child.is(&Ns::Draw, "image"))
                .find_map(|image| {
                    let bytes = self.document.picture(image.attr(&Ns::Xlink, "href")?)?;
                    decode(bytes)
                });
            if let Some(image) = image {
                into.push(Block::Figure(Figure {
                    image,
                    width: declared("width"),
                    height: declared("height"),
                    description: edit::description(frame, &self.document.styles),
                    path: self.path.clone(),
                }));
            }
            return;
        }
        if let Some((index, text_box)) = frame
            .elements_indexed()
            .find(|(_, e)| e.is(&Ns::Draw, "text-box"))
        {
            self.path.push(index);
            let mut blocks = Vec::new();
            self.blocks(text_box, &mut blocks, &mut Counters::default());
            self.path.pop();
            into.push(Block::TextBox(TextBox {
                width: declared("width"),
                blocks,
            }));
        }
    }
}

/// A paragraph's text and runs as they are gathered.
struct Text {
    text: String,
    runs: Vec<Run>,
    links: Vec<String>,
}

impl Text {
    fn push(&mut self, characters: &str, style: &Style, properties: &TextProperties) {
        let shown: String = if properties.uppercase.unwrap_or(false) {
            characters.to_uppercase()
        } else {
            characters.to_owned()
        };
        // A character that draws nothing and that no face need have, which a
        // PDF/UA file may not hold as a glyph, is left out.
        let shown: String = shown
            .chars()
            .filter(|&c| c == '\n' || !(c.is_control() || is_invisible(c)))
            .collect();
        if shown.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(&shown);
        let end = self.text.len();
        match self.runs.last_mut() {
            Some(last) if last.style == *style && last.range.end == start => last.range.end = end,
            _ => self.runs.push(Run {
                range: start..end,
                style: style.clone(),
            }),
        }
    }
}

/// The format characters that shape to nothing: joiners, marks and the byte
/// order mark.
fn is_invisible(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}')
}

fn size_of(properties: &TextProperties, inherited: f32) -> f32 {
    properties
        .size
        .map_or(inherited, |measure| measure.resolve(inherited))
}

fn style(properties: &TextProperties, size: f32, link: Option<usize>) -> Style {
    let (scale, rise) = match properties.position {
        Some(Position::Super) => (SCRIPT_SCALE, size * 0.33),
        Some(Position::Sub) => (SCRIPT_SCALE, -size * 0.08),
        _ => (1.0, 0.0),
    };
    Style {
        family: properties.font_family.clone().unwrap_or_default(),
        variant: Variant {
            bold: properties.bold.unwrap_or(false),
            italic: properties.italic.unwrap_or(false),
        },
        size: size * scale,
        color: properties.color,
        background: properties.background,
        underline: properties.underline.unwrap_or(false),
        strike: properties.strike.unwrap_or(false),
        rise,
        link,
    }
}

/// A picture in a format krilla embeds: PNG, JPEG, GIF or WebP, by its first
/// bytes.
fn decode(bytes: &[u8]) -> Option<krilla::image::Image> {
    let data: krilla::Data = std::sync::Arc::new(bytes.to_vec()).into();
    let image = match bytes {
        [0x89, b'P', b'N', b'G', ..] => krilla::image::Image::from_png(data, true),
        [0xFF, 0xD8, ..] => krilla::image::Image::from_jpeg(data, true),
        [b'G', b'I', b'F', ..] => krilla::image::Image::from_gif(data, true),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => krilla::image::Image::from_webp(data, true),
        _ => return None,
    };
    image.ok()
}

/// How a list level numbers its items, as a PDF reader is told.
fn numbering(level_style: Option<&Element>) -> ListNumbering {
    let Some(style) = level_style else {
        return ListNumbering::Disc;
    };
    if !style.is(&Ns::Text, "list-level-style-number") {
        return ListNumbering::Disc;
    }
    match style
        .attr(&Ns::Style, "num-format")
        .unwrap_or("1")
        .chars()
        .next()
    {
        Some('a') => ListNumbering::LowerAlpha,
        Some('A') => ListNumbering::UpperAlpha,
        Some('i') => ListNumbering::LowerRoman,
        Some('I') => ListNumbering::UpperRoman,
        None => ListNumbering::None,
        _ => ListNumbering::Decimal,
    }
}

/// The text drawn in front of a list item, as the window draws it.
fn label(
    level_style: Option<&Element>,
    level: usize,
    number: usize,
    counters: &Counters,
) -> String {
    let Some(style) = level_style else {
        return "\u{2022}".to_owned();
    };
    if style.is(&Ns::Text, "list-level-style-bullet") {
        return style
            .attr(&Ns::Text, "bullet-char")
            .unwrap_or("\u{2022}")
            .to_owned();
    }
    if style.is(&Ns::Text, "list-level-style-number") {
        let format = style.attr(&Ns::Style, "num-format").unwrap_or("1");
        let prefix = style.attr(&Ns::Style, "num-prefix").unwrap_or_default();
        let suffix = style.attr(&Ns::Style, "num-suffix").unwrap_or_default();
        let display = style
            .attr_usize(&Ns::Text, "display-levels")
            .unwrap_or(1)
            .max(1);
        let mut numbers = Vec::new();
        let first = (level + 1).saturating_sub(display);
        for ancestor in first..level {
            numbers.push(odox_core::edit::number_text(counters.at(ancestor), format));
        }
        numbers.push(odox_core::edit::number_text(number, format));
        return format!("{prefix}{}{suffix}", numbers.join("."));
    }
    "\u{2022}".to_owned()
}
