//! A text document read into the blocks an export lays out.
//!
//! The reading follows the window's (`odox-ui`'s flow): the same elements are
//! blocks, the same ones pass their text through, a tab goes to the same
//! stop, a footnote is its citation, and a picture anchored in a paragraph
//! comes after it. What the window draws is what the PDF holds, in the same
//! order.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::ops::Range;

use krilla::tagging::ListNumbering;
use odox_core::edit::{self, Description, ListCounters};
use odox_core::{
    Border, Break, Color, Document, Edges, Element, Family, Length, Measure, Node, Ns, Position,
    Properties, SMALL_CAPS, TabStop, TextAlign, TextProperties,
};
use odox_fonts::Variant;

/// The size text is drawn at where a document names none, as in the window.
pub(crate) const DEFAULT_SIZE: f32 = 12.0;
/// How large a footnote's citation is, as a proportion of its run.
const CITATION_SCALE: f32 = 0.7;
const TAB_DISTANCE: f32 = 1.25 / 2.54 * 72.0;
/// How far each list level is indented.
const LIST_STEP: f32 = 18.0;
/// The space a list label is drawn in, left of its item's text.
const LIST_GUTTER: f32 = 20.0;

/// One thing in the flow.
#[allow(clippy::large_enum_variant)]
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
    pub tabs: Vec<TabStop>,
    pub tab_distance: f32,
    /// Whether it reads right to left as a whole, as its style says.
    pub right_to_left: bool,
    /// The footnotes cited in it, for the foot of the page it is on.
    pub footnotes: Vec<Note>,
    /// The page number and page count fields in it, by the characters each
    /// shows, which the pages are filled in with once they are known.
    pub page_fields: Vec<(Range<usize>, PageField)>,
    /// Which paragraph with page fields it is, counting from nought, to find
    /// the page it lands on.
    pub fielded: Option<usize>,
}

/// A field a page fills in: the page number, moved by so many pages, or the
/// page count, each in a numbering format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PageField {
    Number { adjust: i64, format: String },
    Count { format: String },
}

impl PageField {
    /// Read `text:page-number` or `text:page-count`.
    fn read(element: &Element) -> Self {
        let format = element
            .attr(&Ns::Style, "num-format")
            .unwrap_or("1")
            .to_owned();
        if element.is(&Ns::Text, "page-count") {
            return Self::Count { format };
        }
        let select = match element.attr(&Ns::Text, "select-page") {
            Some("previous") => -1,
            Some("next") => 1,
            _ => 0,
        };
        let adjust = element
            .attr(&Ns::Text, "page-adjust")
            .and_then(|n| n.parse::<i64>().ok())
            .unwrap_or(0);
        Self::Number {
            adjust: select + adjust,
            format,
        }
    }

    /// What the field shows on a page of so many: nothing for a page number
    /// moved off the document.
    fn shown(&self, page: usize, pages: usize) -> String {
        match self {
            Self::Number { adjust, format } => {
                let moved = i64::try_from(page).unwrap_or(0) + adjust;
                match usize::try_from(moved) {
                    Ok(n) if (1..=pages).contains(&n) => edit::number_text(n, format),
                    _ => String::new(),
                }
            }
            Self::Count { format } => edit::number_text(pages, format),
        }
    }
}

impl Paragraph {
    /// Show one of its page fields as it comes to on a page of so many, the
    /// runs moving with the characters.
    pub(crate) fn fill_field(&mut self, at: usize, page: usize, pages: usize) {
        let (range, field) = self.page_fields[at].clone();
        let shown = field.shown(page, pages);
        let delta = shown.len().cast_signed() - range.len().cast_signed();
        self.text.replace_range(range.clone(), &shown);
        let moved = |at: usize| at.saturating_add_signed(delta);
        let mut covered = !range.is_empty();
        for run in &mut self.runs {
            if run.range.start == range.start && range.is_empty() {
                run.range.end = moved(run.range.end);
                covered = true;
            } else if run.range.start >= range.end {
                run.range = moved(run.range.start)..moved(run.range.end);
            } else if run.range.end >= range.end {
                run.range.end = moved(run.range.end);
                covered |= run.range.end > range.start;
            }
        }
        if !covered && !shown.is_empty() {
            let place = self
                .runs
                .iter()
                .position(|run| run.range.start > range.start)
                .unwrap_or(self.runs.len());
            self.runs.insert(
                place,
                Run {
                    range: range.start..range.start + shown.len(),
                    style: self.base.clone(),
                },
            );
        }
        for (other, _) in &mut self.page_fields[at + 1..] {
            *other = moved(other.start)..moved(other.end);
        }
        self.page_fields[at].0 = range.start..range.start + shown.len();
    }

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
            tabs: Vec::new(),
            tab_distance: TAB_DISTANCE,
            right_to_left: false,
            footnotes: Vec::new(),
            page_fields: Vec::new(),
            fielded: None,
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
    /// The size of the text it sits in, which its underline and its highlight
    /// are drawn to: its own size where it is not raised.
    pub base_size: f32,
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
    /// Whether it goes at the foot of the page its citation is on, rather
    /// than at the end of the document.
    pub foot: bool,
}

/// Read the blocks under an element, with its path from the content root, and
/// the endnotes cited in them after the rest, in the order they are cited. A
/// footnote goes with the paragraph that cites it, for the foot of its page.
pub(crate) fn read(
    document: &Document,
    root: &Element,
    path: Vec<usize>,
    shape: Option<std::rc::Rc<Properties>>,
) -> Vec<Block> {
    read_notes(document, root, path, shape, false, None)
}

/// [`read`] for a text document's body, whose footnotes go with the
/// paragraphs that cite them, for the foot of their pages. Anywhere else, a
/// slide's text, a header, a footnote is read as an endnote, since there is no
/// page foot for it there. A date that follows the day shows `today`.
pub(crate) fn read_text(
    document: &Document,
    root: &Element,
    path: Vec<usize>,
    today: Option<(i64, i64, i64)>,
) -> Vec<Block> {
    read_notes(document, root, path, None, true, today)
}

fn read_notes(
    document: &Document,
    root: &Element,
    path: Vec<usize>,
    shape: Option<std::rc::Rc<Properties>>,
    at_foot: bool,
    today: Option<(i64, i64, i64)>,
) -> Vec<Block> {
    let mut reader = Reader {
        document,
        path,
        shape,
        notes: Vec::new(),
        at_foot,
        fielded: 0,
        today,
    };
    let mut blocks = Vec::new();
    reader.blocks(root, &mut blocks, &mut ListCounters::default());
    blocks.extend(reader.notes.into_iter().map(Block::Note));
    blocks
}

struct Reader<'a> {
    document: &'a Document,
    path: Vec<usize>,
    /// The style of the shape the text is in, under each paragraph's own.
    shape: Option<std::rc::Rc<Properties>>,
    /// The notes cited so far.
    notes: Vec<Note>,
    /// Whether a footnote goes with the paragraph citing it, rather than to
    /// the end with the endnotes.
    at_foot: bool,
    /// How many paragraphs with page fields have been read.
    fielded: usize,
    /// Today, which a date that follows the day shows, where the document is
    /// printed rather than read for a slide or a band.
    today: Option<(i64, i64, i64)>,
}

impl Reader<'_> {
    fn blocks(&mut self, parent: &Element, into: &mut Vec<Block>, counters: &mut ListCounters) {
        for (index, element) in parent.elements_indexed() {
            self.path.push(index);
            if element.is(&Ns::Text, "p") || element.is(&Ns::Text, "h") {
                self.paragraph(element, into);
            } else if element.is(&Ns::Text, "list") {
                let continues = element.attr(&Ns::Text, "continue-numbering") == Some("true")
                    || element.attr(&Ns::Text, "continue-list").is_some();
                if !continues {
                    *counters = ListCounters::default();
                }
                self.list(element, 0, None, counters, into);
            } else if element.is(&Ns::Table, "table") {
                into.push(Block::Table(self.table(element)));
            } else if element.is(&Ns::Draw, "frame") {
                self.frame(element, into);
            } else if edit::is_block_container(element)
                && !(element.is(&Ns::Text, "section")
                    && element.attr(&Ns::Text, "display") == Some("none"))
            {
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
        let hidden_field = element
            .descendant(&Ns::Text, "hidden-paragraph")
            .is_some_and(|field| field.attr(&Ns::Text, "is-hidden") == Some("true"));
        if properties.text.hidden == Some(true) || hidden_field {
            return;
        }
        let properties = match &self.shape {
            Some(shape) => std::rc::Rc::new(properties.in_shape(shape)),
            None => properties,
        };
        let size = size_of(&properties.text, DEFAULT_SIZE);
        let base = style(&properties.text, size, None);
        let mut text = Text {
            text: String::new(),
            runs: Vec::new(),
            links: Vec::new(),
            page_fields: Vec::new(),
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
        let mut footnotes = Vec::new();
        let mut frames = Vec::new();
        for (element, path) in anchored {
            if element.is(&Ns::Text, "note") {
                let outer = std::mem::replace(&mut self.path, path);
                let note = self.note(&element);
                self.path = outer;
                match note {
                    Some(note) if note.foot && self.at_foot => footnotes.push(note),
                    Some(note) => self.notes.push(note),
                    None => {}
                }
            } else {
                frames.push((element, path));
            }
        }
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
            tabs: p.tab_stops.clone().unwrap_or_default(),
            tab_distance: p.tab_distance.map_or(TAB_DISTANCE, Length::points),
            right_to_left: p.right_to_left == Some(true),
            footnotes,
            fielded: (!text.page_fields.is_empty()).then(|| {
                self.fielded += 1;
                self.fielded - 1
            }),
            page_fields: text.page_fields,
        }));
        for (element, path) in frames {
            let outer = std::mem::replace(&mut self.path, path);
            self.frame(&element, into);
            self.path = outer;
        }
    }

    /// A note's body and its citation.
    fn note(&mut self, note: &Element) -> Option<Note> {
        let citation = note
            .child(&Ns::Text, "note-citation")
            .map(Element::plain_text)
            .unwrap_or_default();
        let (index, body) = note
            .elements_indexed()
            .find(|(_, e)| e.is(&Ns::Text, "note-body"))?;
        self.path.push(index);
        let mut blocks = Vec::new();
        self.blocks(body, &mut blocks, &mut ListCounters::default());
        self.path.pop();
        Some(Note {
            citation,
            blocks,
            foot: note.attr(&Ns::Text, "note-class") != Some("endnote"),
        })
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
                Node::Text(t) | Node::CData(t) => {
                    text.push(&t.replace('\t', ""), &current, inherited);
                }
                Node::Comment(_) | Node::ProcessingInstruction(_) => {}
                Node::Element(element) => {
                    path.push(index);
                    if element.is(&Ns::Text, "s") {
                        let count = element.attr_usize(&Ns::Text, "c").unwrap_or(1);
                        text.push(&" ".repeat(count.min(256)), &current, inherited);
                    } else if element.is(&Ns::Text, "tab") {
                        text.push("\t", &current, inherited);
                    } else if element.is(&Ns::Text, "line-break") {
                        text.push("\n", &current, inherited);
                    } else if element.is(&Ns::Text, "span") || element.is(&Ns::Text, "a") {
                        let own = self.style_of(element, &Family::Text);
                        let merged = own.text.over(inherited);
                        if merged.hidden == Some(true) {
                            path.pop();
                            continue;
                        }
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
                            raised.rise = size * Position::SUPER.rise;
                            text.push(&citation.plain_text(), &raised, inherited);
                        }
                        anchored.push((element.clone(), path.clone()));
                    } else if element.is(&Ns::Text, "page-number")
                        || element.is(&Ns::Text, "page-count")
                    {
                        let start = text.text.len();
                        self.runs(element, inherited, size, link, text, anchored, path);
                        text.page_fields
                            .push((start..text.text.len(), PageField::read(element)));
                    } else if let Some(shown) = self.today.and_then(|today| {
                        odox_core::doc::date_shown(element, today, &self.document.styles)
                    }) {
                        text.push(&shown, &current, inherited);
                    } else if element.is(&Ns::Text, "ruby-text") {
                        text.push(&edit::ruby_shown(element), &current, inherited);
                    } else if edit::is_inline_passthrough(element)
                        && !(element.is(&Ns::Text, "hidden-text")
                            && element.attr(&Ns::Text, "is-hidden") == Some("true"))
                    {
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
        counters: &mut ListCounters,
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
                counters.count(item, level, level_style.as_ref());
                counters.label(level, |at| self.level_style(&style_name, at))
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
                    self.blocks(cell, &mut blocks, &mut ListCounters::default());
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
            self.blocks(text_box, &mut blocks, &mut ListCounters::default());
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
    page_fields: Vec<(Range<usize>, PageField)>,
}

impl Text {
    fn push(&mut self, characters: &str, style: &Style, properties: &TextProperties) {
        for (piece, small) in properties.cased(characters) {
            if small {
                let mut smaller = style.clone();
                smaller.size *= SMALL_CAPS;
                self.append(&piece, &smaller);
            } else {
                self.append(&piece, style);
            }
        }
    }

    fn append(&mut self, shown: &str, style: &Style) {
        // A character that draws nothing and that no face need have, which a
        // PDF/UA file may not hold as a glyph, is left out.
        let shown: String = shown
            .chars()
            .filter(|&c| c == '\n' || c == '\t' || !(c.is_control() || is_invisible(c)))
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

pub(crate) fn size_of(properties: &TextProperties, inherited: f32) -> f32 {
    properties
        .size
        .map_or(inherited, |measure| measure.resolve(inherited))
}

pub(crate) fn style(properties: &TextProperties, size: f32, link: Option<usize>) -> Style {
    let (scale, rise) = properties.position.map_or((1.0, 0.0), |position| {
        (position.scale, size * position.rise)
    });
    Style {
        family: properties.font_family.clone().unwrap_or_default(),
        variant: Variant {
            bold: properties.bold.unwrap_or(false),
            italic: properties.italic.unwrap_or(false),
        },
        size: size * scale,
        base_size: size,
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
pub(crate) fn decode(bytes: &[u8]) -> Option<krilla::image::Image> {
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

/// The figures among blocks that say nothing in place of their picture.
pub(crate) fn undescribed(blocks: &[Block], into: &mut Vec<Vec<usize>>) {
    for block in blocks {
        match block {
            Block::Figure(figure) if figure.description == Description::Missing => {
                into.push(figure.path.clone());
            }
            Block::List(list) => {
                for item in &list.items {
                    undescribed(&item.blocks, into);
                }
            }
            Block::Table(table) => {
                for cell in table.rows.iter().flatten() {
                    undescribed(&cell.blocks, into);
                }
            }
            Block::TextBox(text_box) => undescribed(&text_box.blocks, into),
            Block::Note(note) => undescribed(&note.blocks, into),
            Block::Paragraph(paragraph) => {
                for note in &paragraph.footnotes {
                    undescribed(&note.blocks, into);
                }
            }
            _ => {}
        }
    }
}

/// Whether any paragraph among blocks has page fields.
pub(crate) fn has_page_fields(blocks: &[Block]) -> bool {
    let mut any = false;
    each_paragraph(blocks, &mut |paragraph| any |= paragraph.fielded.is_some());
    any
}

/// Fill every page field among blocks: a page number with the page it was
/// found on, and every page count with the count. A page number found on no
/// page keeps what it shows.
pub(crate) fn fill_page_fields(
    blocks: &mut [Block],
    page_of: &std::collections::HashMap<(usize, usize), usize>,
    pages: usize,
) {
    each_paragraph_mut(blocks, &mut |paragraph| {
        let Some(fielded) = paragraph.fielded else {
            return;
        };
        for at in 0..paragraph.page_fields.len() {
            let page = page_of.get(&(fielded, at)).copied();
            let count = matches!(paragraph.page_fields[at].1, PageField::Count { .. });
            if let Some(page) = page.or(count.then_some(1)) {
                paragraph.fill_field(at, page, pages);
            }
        }
    });
}

fn each_paragraph(blocks: &[Block], each: &mut impl FnMut(&Paragraph)) {
    for block in blocks {
        match block {
            Block::Paragraph(paragraph) => {
                each(paragraph);
                for note in &paragraph.footnotes {
                    each_paragraph(&note.blocks, each);
                }
            }
            Block::List(list) => {
                for item in &list.items {
                    each_paragraph(&item.blocks, each);
                }
            }
            Block::Table(table) => {
                for cell in table.rows.iter().flatten() {
                    each_paragraph(&cell.blocks, each);
                }
            }
            Block::TextBox(text_box) => each_paragraph(&text_box.blocks, each),
            Block::Note(note) => each_paragraph(&note.blocks, each),
            _ => {}
        }
    }
}

fn each_paragraph_mut(blocks: &mut [Block], each: &mut impl FnMut(&mut Paragraph)) {
    for block in blocks {
        match block {
            Block::Paragraph(paragraph) => {
                each(paragraph);
                for note in &mut paragraph.footnotes {
                    each_paragraph_mut(&mut note.blocks, each);
                }
            }
            Block::List(list) => {
                for item in &mut list.items {
                    each_paragraph_mut(&mut item.blocks, each);
                }
            }
            Block::Table(table) => {
                for cell in table.rows.iter_mut().flatten() {
                    each_paragraph_mut(&mut cell.blocks, each);
                }
            }
            Block::TextBox(text_box) => each_paragraph_mut(&mut text_box.blocks, each),
            Block::Note(note) => each_paragraph_mut(&mut note.blocks, each),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use odox_core::Package;
    use odox_core::doc::TextDocument;

    use super::*;

    /// The paragraphs a text document whose body is `body` prints, with the
    /// automatic styles `styles`.
    fn printed(styles: &str, body: &str) -> Vec<String> {
        let ns = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.3""#;
        let content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content {ns}><office:automatic-styles>{styles}</office:automatic-styles><office:body><office:text>{body}</office:text></office:body></office:document-content>"#
        );
        let mut package = Package::read(&odox_core::blank::text("en")).expect("a package");
        package.set_part("content.xml", content.into_bytes());
        let bytes = package.write().expect("written");
        let document = TextDocument::read(&bytes).expect("a text document");
        let blocks = read(
            &document.document,
            document.body().expect("a body"),
            document.body_path().expect("a path"),
            None,
        );
        blocks
            .into_iter()
            .filter_map(|block| match block {
                Block::Paragraph(paragraph) => Some(paragraph.text),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn hidden_text_hidden_paragraphs_and_hidden_sections_are_not_printed() {
        let styles = r#"<style:style style:name="T1" style:family="text"><style:text-properties text:display="none"/></style:style><style:style style:name="P1" style:family="paragraph"><style:text-properties text:display="none"/></style:style>"#;
        let body = concat!(
            r#"<text:p>shown <text:span text:style-name="T1">secret</text:span>end</text:p>"#,
            r#"<text:p text:style-name="P1">a hidden paragraph</text:p>"#,
            r#"<text:p><text:hidden-paragraph text:condition="ooow:1" text:is-hidden="true"/>hidden by a field</text:p>"#,
            r#"<text:p><text:hidden-paragraph text:condition="ooow:0" text:is-hidden="false"/>kept by a field</text:p>"#,
            r#"<text:p>a <text:hidden-text text:condition="ooow:1" text:string-value="field" text:is-hidden="true">field</text:hidden-text>and <text:hidden-text text:condition="ooow:0" text:string-value="shown" text:is-hidden="false">shown</text:hidden-text></text:p>"#,
            r#"<text:section text:name="S1" text:display="none"><text:p>in a hidden section</text:p></text:section>"#,
            r#"<text:section text:name="S2"><text:p>in a section</text:p></text:section>"#,
        );
        assert_eq!(
            printed(styles, body),
            [
                "shown end",
                "kept by a field",
                "a and shown",
                "in a section"
            ]
        );
    }

    #[test]
    fn ruby_is_printed_after_its_base_in_parentheses() {
        let body = r#"<text:p><text:ruby><text:ruby-base>漢字</text:ruby-base><text:ruby-text>かんじ</text:ruby-text></text:ruby>です</text:p>"#;
        assert_eq!(printed("", body), ["漢字(かんじ)です"]);
    }
}
