//! Blocks laid out at the page's width into pieces: the units a page is
//! filled with.
//!
//! A piece is one line of a paragraph, one row of a table or one picture,
//! with everything drawn in it placed relative to its own top. Pages are
//! filled with whole pieces, so a page breaks between lines and between rows
//! and never through one. The structure tree is built here too, in reading
//! order, because this is where it is known what each thing drawn is.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::HashMap;
use std::num::{NonZeroU16, NonZeroU32};
use std::rc::Rc;

use krilla::tagging::{TableHeaderScope, Tag, TagKind};
use krilla::text::KrillaGlyph;
use odox_core::edit::Description;
use odox_core::{Border, Color, Edges, Measure, TextAlign};

use crate::Refusal;
use crate::fonts::Faces;
use crate::model::{Block, Cell, Figure, List, Paragraph, Style, Table, TextBox};
use crate::tags::{LeafId, NodeId, Tree};
use crate::text::{self, Glyph};

/// The gap between a list label and the text it belongs to.
const LABEL_GAP: f32 = 5.0;
/// The space a table keeps above and below it.
const TABLE_GAP: f32 = 4.0;
/// The space between a text box's edge and its text.
const BOX_PADDING: f32 = 8.0;
/// The edge of a text box, as faint as the window draws it.
const BOX_EDGE: Color = Color {
    r: 0xb0,
    g: 0xb0,
    b: 0xb0,
};
/// Ink, where a document names none: the page is paper.
const INK: Color = Color { r: 0, g: 0, b: 0 };
/// What every link is drawn in, as the window's light palette draws it.
const LINK: Color = Color {
    r: 0x1a,
    g: 0x5f,
    b: 0xb4,
};

/// Something drawn, placed relative to the top of its piece.
#[derive(Clone)]
pub(crate) enum Item {
    /// Glyphs on a baseline.
    Text {
        x: f32,
        baseline: f32,
        face: usize,
        size: f32,
        color: Color,
        glyphs: Vec<KrillaGlyph>,
        text: String,
        leaf: Option<LeafId>,
    },
    /// A filled rectangle behind text: decoration.
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
    },
    /// A straight line: a border or an underline, decoration too.
    Rule {
        from: (f32, f32),
        to: (f32, f32),
        width: f32,
        color: Color,
    },
    /// A picture, which is decoration where it has no leaf.
    Image {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        image: krilla::image::Image,
        leaf: Option<LeafId>,
    },
    /// Nothing drawn, standing for content that is empty: a table cell with
    /// no text in it, which is still a cell to a reader.
    Empty { x: f32, y: f32, leaf: LeafId },
    /// The area of a link that a click follows.
    Link {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        uri: String,
        leaf: LeafId,
    },
}

impl Item {
    fn shifted(mut self, dx: f32, dy: f32) -> Self {
        match &mut self {
            Self::Text { x, baseline, .. } => {
                *x += dx;
                *baseline += dy;
            }
            Self::Rect { x, y, .. }
            | Self::Image { x, y, .. }
            | Self::Link { x, y, .. }
            | Self::Empty { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
            Self::Rule { from, to, .. } => {
                from.0 += dx;
                from.1 += dy;
                to.0 += dx;
                to.1 += dy;
            }
        }
        self
    }

    /// The same, drawn as decoration: a repeated table header is the header
    /// a reader has already been read, drawn again.
    fn decoration(mut self) -> Self {
        match &mut self {
            Self::Text { leaf, .. } | Self::Image { leaf, .. } => *leaf = None,
            _ => {}
        }
        self
    }

    /// The top and bottom of what it covers, relative to its piece.
    pub(crate) fn extent(&self) -> (f32, f32) {
        match self {
            Self::Text { baseline, size, .. } => (baseline - size, baseline + size * 0.3),
            Self::Rect { y, height, .. }
            | Self::Image { y, height, .. }
            | Self::Link { y, height, .. } => (*y, y + height),
            Self::Rule { from, to, .. } => (from.1.min(to.1), from.1.max(to.1)),
            Self::Empty { y, .. } => (*y, *y),
        }
    }
}

/// One unit a page is filled with.
#[derive(Clone, Default)]
pub(crate) struct Piece {
    pub height: f32,
    pub items: Vec<Item>,
    /// Space above it, dropped at the top of a page.
    pub before: f32,
    /// Space below it.
    pub after: f32,
    /// Whether the next piece has to be on the same page.
    pub keep_with_next: bool,
    /// Whether it begins a new page.
    pub page_before: bool,
    /// What is drawn again above it where it begins a page: a table's header
    /// rows.
    pub repeat: Option<Rc<Vec<Piece>>>,
    /// The heading it is the first line of, for the outline.
    pub heading: Option<(u8, String)>,
}

/// Lays blocks out and builds the structure tree as it goes.
pub(crate) struct Layout<'a> {
    pub faces: &'a mut Faces,
    pub tree: Tree,
    /// The tallest a piece may be, which a picture is scaled to fit.
    pub page_height: f32,
    /// A page break asked for after the last paragraph, for the next piece.
    break_pending: bool,
    /// The levels of the headings the current one sits under, as the
    /// document numbers them.
    headings: Vec<u8>,
}

impl<'a> Layout<'a> {
    pub(crate) fn new(faces: &'a mut Faces, page_height: f32) -> Self {
        Self {
            faces,
            tree: Tree::default(),
            page_height,
            break_pending: false,
            headings: Vec::new(),
        }
    }

    pub(crate) fn blocks(
        &mut self,
        blocks: &[Block],
        x: f32,
        width: f32,
        parent: Option<NodeId>,
        out: &mut Vec<Piece>,
    ) -> Result<(), Refusal> {
        for block in blocks {
            let first = out.len();
            match block {
                Block::Paragraph(paragraph) => {
                    self.paragraph(paragraph, x, width, parent, None, out)?;
                }
                Block::List(list) => self.list(list, x, width, parent, out)?,
                Block::Table(table) => self.table(table, x, width, parent, out)?,
                Block::Figure(figure) => self.figure(figure, x, width, parent, out),
                Block::TextBox(text_box) => self.text_box(text_box, x, width, parent, out)?,
            }
            if self.break_pending
                && let Some(piece) = out.get_mut(first)
                && !matches!(block, Block::Paragraph(p) if p.page_after)
            {
                piece.page_before = true;
                self.break_pending = false;
            }
            if let Block::Paragraph(paragraph) = block
                && paragraph.page_after
            {
                self.break_pending = true;
            }
        }
        Ok(())
    }

    /// A paragraph's lines, with a list label drawn in the margin of its
    /// first one where it has one.
    #[allow(clippy::too_many_lines)]
    fn paragraph(
        &mut self,
        paragraph: &Paragraph,
        x: f32,
        width: f32,
        parent: Option<NodeId>,
        label: Option<(&str, NodeId)>,
        out: &mut Vec<Piece>,
    ) -> Result<(), Refusal> {
        let level = paragraph
            .heading
            .map(|level| reading_level(&mut self.headings, level));
        let kind: TagKind = match level {
            Some(level) => Tag::Hn(
                NonZeroU16::new(u16::from(level)).unwrap_or(NonZeroU16::MIN),
                Some(paragraph.text.trim().to_owned()),
            )
            .into(),
            None => Tag::P.into(),
        };
        let node = self.tree.group(parent, kind);
        let glyphs = text::shape(paragraph, self.faces)?;

        let body = (width - paragraph.left - paragraph.right).max(1.0);
        let first_width = (body - paragraph.indent).max(1.0);
        let lines = text::lines(&paragraph.text, &glyphs, first_width, body);
        let count = lines.len();
        let mut link_nodes: HashMap<usize, NodeId> = HashMap::new();

        for (number, line) in lines.into_iter().enumerate() {
            let first = number == 0;
            let last = number + 1 == count;
            let left = x + paragraph.left + if first { paragraph.indent } else { 0.0 };
            let available = if first { first_width } else { body };
            let in_line: Vec<&Glyph> = glyphs
                .iter()
                .filter(|g| line.contains(&g.cluster))
                .collect();
            let (ascent, descent, natural) = self.metrics(paragraph, &in_line);
            let height = match paragraph.line_height {
                None => natural,
                Some(Measure::Relative(percent)) => natural * percent.fraction(),
                Some(Measure::Absolute(length)) => length.points(),
            };
            let baseline = ascent + (height - ascent - descent) / 2.0;

            let line_width = text::width(&paragraph.text, &glyphs, line.clone());
            let ends_hard = paragraph.text[line.clone()].ends_with('\n');
            let trimmed = text::trimmed(&paragraph.text, line.clone());
            let spaces = in_line
                .iter()
                .filter(|g| g.cluster < trimmed && paragraph.text[g.cluster..].starts_with(' '))
                .count();
            let (start, stretch) = match paragraph.align {
                TextAlign::Center => (left + (available - line_width) / 2.0, 0.0),
                TextAlign::End => (left + available - line_width, 0.0),
                #[allow(clippy::cast_precision_loss)]
                TextAlign::Justify if !last && !ends_hard && spaces > 0 => {
                    (left, (available - line_width).max(0.0) / spaces as f32)
                }
                _ => (left, 0.0),
            };

            let mut piece = Piece {
                height,
                ..Piece::default()
            };
            if let Some(fill) = paragraph.background {
                piece.items.push(Item::Rect {
                    x: x + paragraph.left,
                    y: 0.0,
                    width: body,
                    height,
                    color: fill,
                });
            }
            borders(
                &mut piece.items,
                &paragraph.border,
                (x + paragraph.left, 0.0, body, height),
                (first, last),
            );
            if first && let Some((label, label_node)) = label {
                self.label(paragraph, label, label_node, start, baseline, &mut piece)?;
            }

            let mut decorations = Vec::new();
            let mut pen = start;
            for fragment in fragments(&in_line) {
                let style = &paragraph.runs[fragment[0].run].style;
                let begin = fragment[0].cluster;
                let end = in_line
                    .iter()
                    .map(|g| g.cluster)
                    .find(|&c| c > fragment[fragment.len() - 1].cluster)
                    .unwrap_or(line.end);
                let shown = paragraph.text[begin..end].trim_end_matches('\n');
                let mut krilla = Vec::new();
                let mut advance = 0.0;
                for (index, glyph) in fragment.iter().enumerate() {
                    // Every glyph of a cluster stands for the whole of it.
                    let next = fragment[index..]
                        .iter()
                        .map(|g| g.cluster)
                        .find(|&c| c != glyph.cluster)
                        .unwrap_or(end);
                    let stretched = if stretch > 0.0
                        && glyph.cluster < trimmed
                        && paragraph.text[glyph.cluster..].starts_with(' ')
                    {
                        glyph.advance + stretch
                    } else {
                        glyph.advance
                    };
                    advance += stretched;
                    krilla.push(KrillaGlyph::new(
                        krilla::text::GlyphId::new(u32::from(glyph.id)),
                        stretched / style.size,
                        glyph.dx / style.size,
                        glyph.dy / style.size,
                        0.0,
                        (glyph.cluster - begin)..(next.min(begin + shown.len()) - begin),
                        None,
                    ));
                }
                let color = if style.link.is_some() {
                    LINK
                } else {
                    style.color.unwrap_or(INK)
                };
                let link = style
                    .link
                    .and_then(|index| paragraph.links.get(index).map(|uri| (index, uri)))
                    .filter(|(_, uri)| is_external(uri));
                let owner = match link {
                    Some((index, _)) => *link_nodes
                        .entry(index)
                        .or_insert_with(|| self.tree.group(Some(node), Tag::Link)),
                    None => node,
                };
                if let Some(fill) = style.background {
                    decorations.push(Item::Rect {
                        x: pen,
                        y: baseline - style.size,
                        width: advance,
                        height: style.size * 1.2,
                        color: fill,
                    });
                }
                let line_y = baseline - style.rise;
                let stroke = (style.size * 0.06).max(0.5);
                if style.underline || link.is_some() {
                    piece.items.push(Item::Rule {
                        from: (pen, line_y + style.size * 0.12),
                        to: (pen + advance, line_y + style.size * 0.12),
                        width: stroke,
                        color,
                    });
                }
                if style.strike {
                    piece.items.push(Item::Rule {
                        from: (pen, line_y - style.size * 0.3),
                        to: (pen + advance, line_y - style.size * 0.3),
                        width: stroke,
                        color,
                    });
                }
                if !shown.is_empty() {
                    let leaf = self.tree.leaf(owner);
                    piece.items.push(Item::Text {
                        x: pen,
                        baseline: line_y,
                        face: fragment[0].face,
                        size: style.size,
                        color,
                        glyphs: krilla,
                        text: shown.to_owned(),
                        leaf: Some(leaf),
                    });
                }
                if let Some((_, uri)) = link {
                    let leaf = self.tree.leaf(owner);
                    piece.items.push(Item::Link {
                        x: pen,
                        y: 0.0,
                        width: advance.max(1.0),
                        height,
                        uri: uri.clone(),
                        leaf,
                    });
                }
                pen += advance;
            }
            decorations.append(&mut piece.items);
            piece.items = decorations;

            if first {
                piece.before = paragraph.before;
                piece.keep_with_next = paragraph.heading.is_some();
                piece.page_before = paragraph.page_before;
                piece.heading = level.map(|level| (level, paragraph.text.trim().to_owned()));
            }
            if last {
                piece.after = paragraph.after;
            } else if paragraph.heading.is_some() {
                piece.keep_with_next = true;
            }
            out.push(piece);
        }
        Ok(())
    }

    /// The ascent, descent and natural height of a line: the largest of the
    /// faces and sizes in it, or the paragraph's own where it is empty.
    fn metrics(&mut self, paragraph: &Paragraph, glyphs: &[&Glyph]) -> (f32, f32, f32) {
        let mut ascent: f32 = 0.0;
        let mut descent: f32 = 0.0;
        let mut natural: f32 = 0.0;
        let mut measure = |face: &crate::fonts::Face, style: &Style| {
            ascent = ascent.max(face.ascent * style.size + style.rise.max(0.0));
            descent = descent.max(face.descent * style.size - style.rise.min(0.0));
            natural = natural.max(face.line(style.size));
        };
        if glyphs.is_empty() {
            if let Ok(face) = self
                .faces
                .request(&paragraph.base.family, paragraph.base.variant)
            {
                measure(self.faces.get(face), &paragraph.base);
            }
        } else {
            for glyph in glyphs {
                measure(self.faces.get(glyph.face), &paragraph.runs[glyph.run].style);
            }
        }
        (ascent, descent, natural.max(ascent + descent))
    }

    /// A list label, ending just before the text begins.
    fn label(
        &mut self,
        paragraph: &Paragraph,
        label: &str,
        node: NodeId,
        text_start: f32,
        baseline: f32,
        piece: &mut Piece,
    ) -> Result<(), Refusal> {
        let style = Style {
            link: None,
            ..paragraph.base.clone()
        };
        let shaped = Paragraph {
            heading: None,
            text: label.to_owned(),
            runs: vec![crate::model::Run {
                range: 0..label.len(),
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
        };
        let glyphs = text::shape(&shaped, self.faces)?;
        let width: f32 = glyphs.iter().map(|g| g.advance).sum();
        let Some(face) = glyphs.first().map(|g| g.face) else {
            return Ok(());
        };
        let leaf = self.tree.leaf(node);
        piece.items.push(Item::Text {
            x: text_start - width - LABEL_GAP,
            baseline,
            face,
            size: style.size,
            color: style.color.unwrap_or(INK),
            glyphs: glyphs
                .iter()
                .enumerate()
                .map(|(index, g)| {
                    let next = glyphs[index..]
                        .iter()
                        .map(|n| n.cluster)
                        .find(|&c| c != g.cluster)
                        .unwrap_or(label.len());
                    KrillaGlyph::new(
                        krilla::text::GlyphId::new(u32::from(g.id)),
                        g.advance / style.size,
                        g.dx / style.size,
                        g.dy / style.size,
                        0.0,
                        g.cluster..next.max(g.cluster),
                        None,
                    )
                })
                .collect(),
            text: label.to_owned(),
            leaf: Some(leaf),
        });
        Ok(())
    }

    fn list(
        &mut self,
        list: &List,
        x: f32,
        width: f32,
        parent: Option<NodeId>,
        out: &mut Vec<Piece>,
    ) -> Result<(), Refusal> {
        let node = self.tree.group(parent, Tag::L(list.numbering));
        for item in &list.items {
            let item_node = self.tree.group(Some(node), Tag::LI);
            let label_node = item
                .label
                .as_ref()
                .map(|_| self.tree.group(Some(item_node), Tag::Lbl));
            let body = self.tree.group(Some(item_node), Tag::LBody);
            let inner_x = x + item.indent;
            let inner_width = (width - item.indent).max(1.0);
            let mut first = true;
            for block in &item.blocks {
                match block {
                    Block::Paragraph(paragraph) => {
                        let label = match (&item.label, label_node) {
                            (Some(text), Some(node)) if first => Some((text.as_str(), node)),
                            _ => None,
                        };
                        self.paragraph(paragraph, inner_x, inner_width, Some(body), label, out)?;
                    }
                    // A nested list places itself: its indent is absolute.
                    Block::List(nested) => self.list(nested, x, width, Some(body), out)?,
                    other => self.blocks(
                        std::slice::from_ref(other),
                        inner_x,
                        inner_width,
                        Some(body),
                        out,
                    )?,
                }
                if !matches!(block, Block::List(_)) {
                    first = false;
                }
            }
        }
        Ok(())
    }

    fn figure(
        &mut self,
        figure: &Figure,
        x: f32,
        width: f32,
        parent: Option<NodeId>,
        out: &mut Vec<Piece>,
    ) {
        let (pixels_wide, pixels_high) = figure.image.size();
        #[allow(clippy::cast_precision_loss)]
        let aspect = if pixels_wide == 0 {
            1.0
        } else {
            pixels_high as f32 / pixels_wide as f32
        };
        let mut w = figure.width.unwrap_or(width).min(width);
        let mut h = figure.height.unwrap_or(w * aspect);
        if h > self.page_height {
            w *= self.page_height / h;
            h = self.page_height;
        }
        let leaf = match &figure.description {
            Description::Text(alt) => {
                let node = self.tree.group(parent, Tag::Figure(Some(alt.clone())));
                Some(self.tree.leaf(node))
            }
            Description::Decorative | Description::Missing => None,
        };
        out.push(Piece {
            height: h,
            items: vec![Item::Image {
                x,
                y: 0.0,
                width: w,
                height: h,
                image: figure.image.clone(),
                leaf,
            }],
            ..Piece::default()
        });
    }

    fn text_box(
        &mut self,
        text_box: &TextBox,
        x: f32,
        width: f32,
        parent: Option<NodeId>,
        out: &mut Vec<Piece>,
    ) -> Result<(), Refusal> {
        let outer = text_box.width.unwrap_or(width).min(width);
        let node = self.tree.group(parent, Tag::Div);
        let first = out.len();
        self.blocks(
            &text_box.blocks,
            x + BOX_PADDING,
            (outer - 2.0 * BOX_PADDING).max(1.0),
            Some(node),
            out,
        )?;
        // The box's edge, drawn piece by piece so that it follows the box
        // across a page: the sides of every piece and the gaps below them,
        // the top above the first and the bottom below the last.
        let last = out.len().saturating_sub(1);
        let half = BOX_PADDING / 2.0;
        for (index, piece) in out.iter_mut().enumerate().skip(first) {
            let top = if index == first { -half } else { 0.0 };
            let bottom = piece.height + if index == last { half } else { piece.after };
            let mut edge = |from: (f32, f32), to: (f32, f32)| {
                piece.items.push(Item::Rule {
                    from,
                    to,
                    width: 0.75,
                    color: BOX_EDGE,
                });
            };
            edge((x, top), (x, bottom));
            edge((x + outer, top), (x + outer, bottom));
            if index == first {
                edge((x, top), (x + outer, top));
            }
            if index == last {
                edge((x, bottom), (x + outer, bottom));
            }
        }
        if let Some(piece) = out.get_mut(first) {
            piece.before += BOX_PADDING;
        }
        if out.len() > first
            && let Some(piece) = out.last_mut()
        {
            piece.after += BOX_PADDING;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn table(
        &mut self,
        table: &Table,
        x: f32,
        width: f32,
        parent: Option<NodeId>,
        out: &mut Vec<Piece>,
    ) -> Result<(), Refusal> {
        let columns = column_widths(&table.columns, width);
        if columns.is_empty() {
            return Ok(());
        }
        let node = self.tree.group(parent, Tag::Table);
        let mut header: Vec<Piece> = Vec::new();
        let mut repeat: Option<Rc<Vec<Piece>>> = None;
        let first = out.len();
        for (index, row) in table.rows.iter().enumerate() {
            let is_header = index < table.header_rows;
            let row_node = self.tree.group(Some(node), Tag::TR);
            let mut piece = self.row(row, &columns, x, is_header, row_node)?;
            if is_header {
                piece.keep_with_next = true;
                header.push(piece.clone());
            } else {
                if repeat.is_none() && !header.is_empty() {
                    repeat = Some(Rc::new(
                        header
                            .iter()
                            .map(|piece| Piece {
                                items: piece.items.iter().cloned().map(Item::decoration).collect(),
                                ..piece.clone()
                            })
                            .collect(),
                    ));
                }
                piece.repeat.clone_from(&repeat);
            }
            out.push(piece);
        }
        if let Some(piece) = out.get_mut(first) {
            piece.before += TABLE_GAP;
        }
        if out.len() > first
            && let Some(piece) = out.last_mut()
        {
            piece.after += TABLE_GAP;
        }
        Ok(())
    }

    /// One row of a table: every cell laid out at its width, the row as tall
    /// as its tallest cell.
    fn row(
        &mut self,
        row: &[Cell],
        columns: &[f32],
        x: f32,
        header: bool,
        node: NodeId,
    ) -> Result<Piece, Refusal> {
        let mut laid: Vec<(f32, f32, &Cell, Vec<Item>, f32)> = Vec::new();
        for cell in row {
            let left = x + columns.iter().take(cell.column).sum::<f32>();
            let width: f32 = columns
                .iter()
                .skip(cell.column)
                .take(cell.span)
                .sum::<f32>()
                .max(8.0);
            let spans = |count: usize| {
                NonZeroU32::new(u32::try_from(count).unwrap_or(1)).filter(|n| n.get() > 1)
            };
            let kind: TagKind = if header {
                Tag::TH(TableHeaderScope::Column)
                    .with_col_span(spans(cell.span))
                    .with_row_span(spans(cell.rows))
                    .into()
            } else {
                Tag::TD
                    .with_col_span(spans(cell.span))
                    .with_row_span(spans(cell.rows))
                    .into()
            };
            let cell_node = self.tree.group(Some(node), kind);
            let mut pieces = Vec::new();
            let inner = (width - 2.0 * cell.padding).max(8.0);
            let pending = std::mem::take(&mut self.break_pending);
            self.blocks(
                &cell.blocks,
                left + cell.padding,
                inner,
                Some(cell_node),
                &mut pieces,
            )?;
            self.break_pending = pending;
            let mut items = Vec::new();
            if !self.tree.holds_content(cell_node) {
                let leaf = self.tree.leaf(cell_node);
                items.push(Item::Empty {
                    x: left + cell.padding,
                    y: cell.padding,
                    leaf,
                });
            }
            let mut y = cell.padding;
            for (index, piece) in pieces.into_iter().enumerate() {
                if index > 0 {
                    y += piece.before;
                }
                items.extend(piece.items.into_iter().map(|item| item.shifted(0.0, y)));
                y += piece.height + piece.after;
            }
            laid.push((left, width, cell, items, y + cell.padding));
        }
        let height = laid
            .iter()
            .map(|(.., height)| *height)
            .fold(0.0_f32, f32::max)
            .max(1.0);
        let mut piece = Piece {
            height,
            ..Piece::default()
        };
        for (left, width, cell, ..) in &laid {
            if let Some(fill) = cell.background {
                piece.items.push(Item::Rect {
                    x: *left,
                    y: 0.0,
                    width: *width,
                    height,
                    color: fill,
                });
            }
        }
        for (.., items, _) in &mut laid {
            piece.items.append(items);
        }
        for (left, width, cell, ..) in &laid {
            borders(
                &mut piece.items,
                &cell.border,
                (*left, 0.0, *width, height),
                (true, true),
            );
        }
        Ok(piece)
    }
}

/// The level a heading is read at, given the levels of the headings above
/// it: its depth among them, so that the first is a level 1 and none skips a
/// level, which PDF/UA asks of every document. A document that numbers its
/// headings without gaps keeps its own numbers.
fn reading_level(above: &mut Vec<u8>, own: u8) -> u8 {
    while above.last().is_some_and(|&level| level >= own) {
        above.pop();
    }
    above.push(own);
    u8::try_from(above.len()).unwrap_or(u8::MAX)
}

/// The glyphs of a line in runs that are drawn together: the same run and the
/// same face.
fn fragments<'g>(glyphs: &[&'g Glyph]) -> Vec<Vec<&'g Glyph>> {
    let mut out: Vec<Vec<&Glyph>> = Vec::new();
    for &glyph in glyphs {
        match out.last_mut() {
            Some(last) if last[0].run == glyph.run && last[0].face == glyph.face => {
                last.push(glyph);
            }
            _ => out.push(vec![glyph]),
        }
    }
    out
}

/// Whether a link leaves the document, which is what a link annotation is
/// written for. A link to a place inside it is read as its text.
fn is_external(uri: &str) -> bool {
    uri.contains("://") || uri.starts_with("mailto:")
}

/// The borders of a box: the sides always, and the top and bottom only where
/// the box begins and ends.
fn borders(
    items: &mut Vec<Item>,
    border: &Edges<Border>,
    (x, y, width, height): (f32, f32, f32, f32),
    (top, bottom): (bool, bool),
) {
    let mut rule = |edge: Option<Border>, from: (f32, f32), to: (f32, f32)| {
        if let Some(edge) = edge {
            items.push(Item::Rule {
                from,
                to,
                width: edge.width.points().max(0.25),
                color: edge.color,
            });
        }
    };
    rule(border.left, (x, y), (x, y + height));
    rule(border.right, (x + width, y), (x + width, y + height));
    if top {
        rule(border.top, (x, y), (x + width, y));
    }
    if bottom {
        rule(border.bottom, (x, y + height), (x + width, y + height));
    }
}

/// Each column's width: the declared ones, scaled down together where they
/// are wider than the page, and the space left shared among the rest, as the
/// window shares it.
fn column_widths(declared: &[Option<f32>], width: f32) -> Vec<f32> {
    if declared.is_empty() {
        return Vec::new();
    }
    let total: f32 = declared.iter().filter_map(|w| *w).sum();
    let unstated = declared.iter().filter(|w| w.is_none()).count();
    let factor = if total > width && total > 0.0 {
        width / total
    } else {
        1.0
    };
    #[allow(clippy::cast_precision_loss)]
    let share = if unstated == 0 {
        0.0
    } else {
        ((width - total * factor) / unstated as f32).max(16.0)
    };
    declared
        .iter()
        .map(|w| w.map_or(share, |points| points * factor))
        .collect()
}

/// Cut a piece taller than a page at a height: what lies wholly above stays,
/// what does not moves to a second piece, and what crosses the cut — a
/// border, a background — is cut with it.
pub(crate) fn split(piece: Piece, at: f32) -> (Piece, Piece) {
    let mut above = Piece {
        height: at,
        items: Vec::new(),
        before: piece.before,
        heading: piece.heading,
        repeat: piece.repeat.clone(),
        page_before: piece.page_before,
        ..Piece::default()
    };
    let mut below = Piece {
        after: piece.after,
        keep_with_next: piece.keep_with_next,
        repeat: piece.repeat,
        ..Piece::default()
    };
    // The cut moves up to the top of the first thing that would be cut
    // through and cannot be: a line of text, a picture.
    let cut = piece
        .items
        .iter()
        .filter(|item| matches!(item, Item::Text { .. } | Item::Image { .. }))
        .map(Item::extent)
        .filter(|(top, bottom)| *top < at && *bottom > at && *top > 0.0)
        .map(|(top, _)| top)
        .fold(at, f32::min);
    for item in piece.items {
        let (top, bottom) = item.extent();
        if bottom <= cut {
            above.items.push(item);
        } else if top >= cut
            || matches!(
                item,
                Item::Text { .. } | Item::Image { .. } | Item::Link { .. }
            )
        {
            below.items.push(item.shifted(0.0, -cut));
        } else {
            let (upper, lower) = cut_item(item, cut);
            above.items.push(upper);
            below.items.push(lower);
        }
    }
    above.height = cut;
    below.height = (piece.height - cut).max(0.0);
    (above, below)
}

/// A rectangle or a rule crossing a cut, as the part above it and the part
/// below, the lower one moved up to begin at the top.
fn cut_item(item: Item, at: f32) -> (Item, Item) {
    match item {
        Item::Rect {
            x,
            y,
            width,
            height,
            color,
        } => (
            Item::Rect {
                x,
                y,
                width,
                height: at - y,
                color,
            },
            Item::Rect {
                x,
                y: 0.0,
                width,
                height: y + height - at,
                color,
            },
        ),
        Item::Rule {
            from,
            to,
            width,
            color,
        } => (
            Item::Rule {
                from,
                to: (to.0, at),
                width,
                color,
            },
            Item::Rule {
                from: (from.0, 0.0),
                to: (to.0, to.1 - at),
                width,
                color,
            },
        ),
        other => (other.clone(), other.shifted(0.0, -at)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(levels: &[u8]) -> Vec<u8> {
        let mut above = Vec::new();
        levels
            .iter()
            .map(|&level| reading_level(&mut above, level))
            .collect()
    }

    #[test]
    fn headings_numbered_without_gaps_keep_their_numbers() {
        assert_eq!(read(&[1, 2, 3, 2, 1, 2]), [1, 2, 3, 2, 1, 2]);
    }

    #[test]
    fn a_document_that_begins_below_level_one_or_skips_a_level_is_read_without_the_gap() {
        assert_eq!(read(&[2, 3, 2]), [1, 2, 1]);
        assert_eq!(read(&[1, 3, 4, 2]), [1, 2, 3, 2]);
    }

    #[test]
    fn declared_columns_wider_than_the_page_are_scaled_and_the_rest_share_what_is_left() {
        assert_eq!(
            column_widths(&[Some(100.0), Some(300.0)], 200.0),
            [50.0, 150.0]
        );
        assert_eq!(
            column_widths(&[Some(100.0), None, None], 300.0),
            [100.0, 100.0, 100.0]
        );
    }
}
