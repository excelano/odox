//! Drawing an ODF body: the block renderer all three applications share.
//!
//! ODF's content model is the same in a text document, a spreadsheet cell and a
//! slide's text frame — `text:p`, `text:span`, `text:list`, `table:table`,
//! `draw:frame` — so this is written against that model and not against a format.
//! A text document hands it the body; a spreadsheet hands it a cell; a
//! presentation hands it a frame.
//!
//! What it does not do is paginate. A page layout says how wide a line may be and
//! that width is what it is given; page boxes, widows, floats and columns are
//! typesetting, and a reading view is what this draws.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::HashMap;

use eframe::egui::{
    Align, ColorImage, Context, Pos2, Rect, Sense, Stroke, StrokeKind, TextFormat, TextureHandle,
    TextureOptions, Ui, pos2, text::LayoutJob, text_selection::LabelSelectionState, vec2,
};
use odox_core::{
    Border, Document, Element, Family, Node, Ns, Properties, TextAlign, TextProperties,
};

use crate::format::{self, DEFAULT_SIZE};

/// Pictures already decoded, kept for as long as the document is open.
///
/// A failed decode is remembered as a failure, so that a picture in a format
/// nothing here reads is not decoded again on every frame.
#[derive(Default)]
pub struct Pictures {
    textures: HashMap<String, Option<TextureHandle>>,
}

impl Pictures {
    /// The texture for a picture the document refers to, decoding it once.
    pub fn get(
        &mut self,
        ctx: &Context,
        document: &Document,
        href: &str,
    ) -> Option<&TextureHandle> {
        if !self.textures.contains_key(href) {
            let texture = document
                .picture(href)
                .and_then(decode)
                .map(|image| ctx.load_texture(href, image, TextureOptions::LINEAR));
            self.textures.insert(href.to_owned(), texture);
        }
        self.textures.get(href).and_then(Option::as_ref)
    }

    /// Forget every picture, for a window that has closed its document.
    pub fn clear(&mut self) {
        self.textures.clear();
    }
}

fn decode(bytes: &[u8]) -> Option<ColorImage> {
    let decoded = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [decoded.width() as usize, decoded.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, decoded.as_raw()))
}

/// The state a run of blocks is drawn with.
pub struct Flow<'a> {
    /// The document, for its styles and its pictures.
    pub document: &'a Document,
    /// Pictures decoded so far.
    pub pictures: &'a mut Pictures,
    /// Screen points per ODF point.
    pub zoom: f32,
    /// The colours to draw in where the document names none.
    pub palette: format::Palette,
    /// The heading, counted from zero in document order, to bring into view.
    ///
    /// Set by a panel that lists a document's headings. It is answered while the
    /// body is drawn, because the only moment a heading's position is known is
    /// the moment it is laid out.
    pub scroll_to_heading: Option<usize>,
    /// How many headings have been drawn this pass.
    headings_seen: usize,
}

impl<'a> Flow<'a> {
    /// A flow over a document, drawing at a zoom.
    pub fn new(document: &'a Document, pictures: &'a mut Pictures, zoom: f32) -> Self {
        Self {
            document,
            pictures,
            zoom,
            palette: format::Palette::default(),
            scroll_to_heading: None,
            headings_seen: 0,
        }
    }
}

/// How far each list level is indented, in ODF points.
const LIST_STEP: f32 = 18.0;
/// The space a list label is drawn in, left of its item's text.
const LIST_GUTTER: f32 = 20.0;
/// The gap between a label and the text it belongs to.
const LABEL_GAP: f32 = 5.0;

/// A paragraph's margins and first-line indent, in screen points.
struct Spacing {
    /// Space outside the paragraph's left edge.
    left: f32,
    /// Space outside its right edge.
    right: f32,
    /// The first line's extra indent, negative for a hanging one.
    indent: f32,
    /// Space above the paragraph.
    before: f32,
    /// Space below it.
    after: f32,
}

impl Spacing {
    fn of(properties: &odox_core::ParagraphProperties, zoom: f32) -> Self {
        let points = |length: Option<odox_core::Length>| {
            length.map_or(0.0, odox_core::Length::points) * zoom
        };
        Self {
            left: points(properties.margin.left),
            right: points(properties.margin.right),
            indent: points(properties.text_indent),
            before: points(properties.margin.top),
            after: points(properties.margin.bottom),
        }
    }
}

/// What a run of text inherits from the text it sits inside.
///
/// One argument rather than four, because a span inside a link inside a
/// paragraph passes all of them down together and they only ever travel as a set.
#[derive(Clone, Copy)]
struct Run<'a> {
    /// The character properties in force.
    inherited: &'a TextProperties,
    /// The size in points those properties resolve to, which is what a relative
    /// size inside this run is relative to.
    size: f32,
    /// What egui draws the run with.
    format: &'a TextFormat,
    /// The colours to use where the document names none.
    palette: format::Palette,
}

/// Where a list's counters stand, one per level.
#[derive(Default)]
struct Counters(Vec<usize>);

impl Counters {
    fn bump(&mut self, level: usize, start: usize) -> usize {
        while self.0.len() <= level {
            self.0.push(0);
        }
        // A level that has not been counted at yet begins at the level style's
        // start value, which is one unless the document says otherwise.
        if self.0[level] == 0 {
            self.0[level] = start;
        } else {
            self.0[level] += 1;
        }
        // Entering a list resets everything below it, which is what makes 2.1
        // follow 1.3.
        self.0.truncate(level + 1);
        self.0[level]
    }

    fn at(&self, level: usize) -> usize {
        self.0.get(level).copied().unwrap_or(1)
    }
}

impl Flow<'_> {
    /// Draw every block under an element, in order.
    ///
    /// `width` is the space available in screen points, and is what a paragraph
    /// wraps to.
    pub fn blocks(&mut self, ui: &mut Ui, parent: &Element, width: f32) {
        let mut counters = Counters::default();
        self.blocks_with(ui, parent, width, &mut counters);
    }

    fn blocks_with(&mut self, ui: &mut Ui, parent: &Element, width: f32, counters: &mut Counters) {
        for element in parent.elements() {
            match () {
                () if element.is(&Ns::Text, "p") || element.is(&Ns::Text, "h") => {
                    self.paragraph(ui, element, width, None);
                }
                () if element.is(&Ns::Text, "list") => {
                    // Numbering belongs to a list, not to the body: a second
                    // list starts at one again unless it says it is continuing
                    // the one before it, which is what `text:continue-numbering`
                    // and `text:continue-list` are for.
                    let continues = element.attr(&Ns::Text, "continue-numbering") == Some("true")
                        || element.attr(&Ns::Text, "continue-list").is_some();
                    if !continues {
                        *counters = Counters::default();
                    }
                    self.list(ui, element, width, 0, None, counters);
                }
                () if element.is(&Ns::Table, "table") => self.table(ui, element, width),
                () if element.is(&Ns::Draw, "frame") => self.frame(ui, element, width),
                () if element.is(&Ns::Text, "soft-page-break") => self.page_break(ui, width),
                () if is_block_container(element) => {
                    self.blocks_with(ui, element, width, counters);
                }
                () => {}
            }
        }
    }

    /// The resolved style of a paragraph or heading.
    fn style_of(&self, element: &Element, family: &Family) -> std::rc::Rc<Properties> {
        let name = element
            .attr(&Ns::Text, "style-name")
            .or_else(|| element.attr(&Ns::Table, "style-name"))
            .unwrap_or("Standard");
        self.document.styles.resolve(family, name)
    }

    /// One paragraph or heading, with an optional list label drawn in its margin.
    fn paragraph(&mut self, ui: &mut Ui, element: &Element, width: f32, label: Option<&str>) {
        let properties = self.style_of(element, &Family::Paragraph);
        let zoom = self.zoom;
        let size = format::size_of(&properties.text, DEFAULT_SIZE);

        let Spacing {
            left,
            right,
            indent,
            before,
            after,
        } = Spacing::of(&properties.paragraph, zoom);

        // The first line's indent may be negative — a hanging indent — and the
        // text then starts left of the rest of the paragraph, which is where a
        // list label goes.
        let body_width = (width - left - right).max(1.0);
        let wrap = (body_width - indent.max(0.0)).max(1.0);

        if before > 0.0 {
            ui.add_space(before);
        }

        let base = format::text_format(&properties.text, DEFAULT_SIZE, zoom, self.palette);
        let mut job = LayoutJob {
            wrap: eframe::egui::text::TextWrapping {
                max_width: wrap,
                ..Default::default()
            },
            halign: match properties.paragraph.align.unwrap_or(TextAlign::Start) {
                TextAlign::Center => Align::Center,
                TextAlign::End => Align::Max,
                _ => Align::Min,
            },
            justify: properties.paragraph.align == Some(TextAlign::Justify),
            ..LayoutJob::default()
        };
        let mut frames = Vec::new();
        let run = Run {
            inherited: &properties.text,
            size,
            format: &base,
            palette: self.palette,
        };
        self.runs(element, &run, &mut job, &mut frames);

        // An empty paragraph is a blank line and has to take its height, which an
        // empty layout job would not.
        if job.text.is_empty() {
            job.append(" ", 0.0, base.clone());
        }
        if let Some(height) = format::line_height(properties.paragraph.line_height, size) {
            for section in &mut job.sections {
                section.format.line_height = Some(height * zoom);
            }
        }

        let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
        let height = galley.size().y;
        // Click and drag, and not merely hover: the selection plugin begins a
        // selection only on a response whose sense includes drag, which is what
        // `Label` adds to its own when it is selectable, and the click half is
        // what lets a double-click take a word and a triple-click a line.
        // Without the drag the pointer reaches the scroll area instead and
        // nothing is selected — measured, not read.
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());

        if element.is(&Ns::Text, "h") {
            if self.scroll_to_heading == Some(self.headings_seen) {
                ui.scroll_to_rect(rect, Some(Align::TOP));
            }
            self.headings_seen += 1;
        }

        if let Some(fill) = properties.paragraph.background {
            ui.painter().rect_filled(rect, 0.0, format::color32(fill));
        }
        paint_borders(ui, rect, &properties.paragraph.border, zoom);

        // The anchor the galley was laid out around: its rows are positioned
        // relative to this, which is what makes a centred paragraph centre each
        // of its rows rather than its block.
        let anchor = match properties.paragraph.align.unwrap_or(TextAlign::Start) {
            TextAlign::Center => rect.left() + left + indent.max(0.0) + wrap / 2.0,
            TextAlign::End => rect.left() + left + indent.max(0.0) + wrap,
            _ => rect.left() + left + indent.max(0.0),
        };
        // Through egui's selection plugin rather than the painter, which is what
        // lets a person drag across the page and press Ctrl+C. The hook paints
        // the galley itself, at the same anchor the painter would have taken,
        // so alignment is untouched. It is called for every paragraph and not
        // only the visible ones: the plugin drops a selection whose ends it did
        // not see this frame, so a paragraph skipped for being scrolled off
        // would end a selection the moment it left the window.
        LabelSelectionState::label_text_selection(
            ui,
            &response,
            pos2(anchor, rect.top()),
            galley,
            base.color,
            Stroke::NONE,
        );

        if let Some(label) = label {
            let mut label_job = LayoutJob::default();
            label_job.append(label, 0.0, base.clone());
            let label_galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(label_job));
            // Ending just before the text begins, in the gutter the list opened
            // for it. Painting left of the allocated rectangle is deliberate: the
            // gutter is space the caller reserved and nothing else draws there.
            let x = rect.left() + left - label_galley.size().x - LABEL_GAP * zoom;
            ui.painter()
                .galley(pos2(x, rect.top()), label_galley, base.color);
        }

        for frame in frames {
            self.frame(ui, &frame, body_width);
        }
        if after > 0.0 {
            ui.add_space(after);
        }
    }

    /// Append the text of a paragraph's children to a layout job.
    ///
    /// Anything that is not text is either resolved into characters — ODF spells
    /// out runs of spaces, tabs and line breaks rather than writing them
    /// literally — or collected to be drawn after the paragraph, which is what
    /// happens to a picture anchored inside one.
    fn runs(
        &self,
        parent: &Element,
        run: &Run<'_>,
        job: &mut LayoutJob,
        frames: &mut Vec<Element>,
    ) {
        let Run {
            inherited,
            size,
            format,
            palette,
        } = *run;
        for child in &parent.children {
            match child {
                Node::Text(text) | Node::CData(text) => job.append(text, 0.0, format.clone()),
                Node::Comment(_) | Node::ProcessingInstruction(_) => {}
                Node::Element(element) => {
                    if element.is(&Ns::Text, "s") {
                        let count = element.attr_usize(&Ns::Text, "c").unwrap_or(1).min(256);
                        job.append(&" ".repeat(count), 0.0, format.clone());
                    } else if element.is(&Ns::Text, "tab") {
                        // egui lays out a tab as a glyph rather than advancing to
                        // a stop, so the paragraph's tab stops are approximated
                        // by a fixed advance. A document whose layout depends on
                        // tab stops is one §6 of DESIGN.md names.
                        job.append("    ", 0.0, format.clone());
                    } else if element.is(&Ns::Text, "line-break") {
                        job.append("\n", 0.0, format.clone());
                    } else if element.is(&Ns::Text, "span") {
                        let style = self.style_of(element, &Family::Text);
                        let merged = merge(inherited, &style.text);
                        let inner_size = format::size_of(&merged, size);
                        let inner = format::text_format(&merged, size, self.zoom, palette);
                        let run = Run {
                            inherited: &merged,
                            size: inner_size,
                            format: &inner,
                            palette,
                        };
                        self.runs(element, &run, job, frames);
                    } else if element.is(&Ns::Text, "a") {
                        let style = self.style_of(element, &Family::Text);
                        let merged = merge(inherited, &style.text);
                        let inner_size = format::size_of(&merged, size);
                        let inner = format::link_format(&merged, size, self.zoom, palette);
                        let run = Run {
                            inherited: &merged,
                            size: inner_size,
                            format: &inner,
                            palette,
                        };
                        self.runs(element, &run, job, frames);
                    } else if element.is(&Ns::Draw, "frame") {
                        frames.push(element.clone());
                    } else if element.is(&Ns::Text, "note") {
                        // The citation is part of the text; the body is a
                        // footnote and belongs at the foot of a page this view
                        // does not have.
                        if let Some(citation) = element.child(&Ns::Text, "note-citation") {
                            let mut raised = format.clone();
                            raised.font_id.size *= 0.7;
                            raised.valign = Align::TOP;
                            job.append(&citation.plain_text(), 0.0, raised);
                        }
                    } else if is_inline_passthrough(element) {
                        self.runs(element, run, job, frames);
                    }
                }
            }
        }
    }

    /// A list, and the lists nested inside it.
    fn list(
        &mut self,
        ui: &mut Ui,
        element: &Element,
        width: f32,
        level: usize,
        inherited_style: Option<&str>,
        counters: &mut Counters,
    ) {
        let style_name = element
            .attr(&Ns::Text, "style-name")
            .or(inherited_style)
            .unwrap_or_default()
            .to_owned();
        // The indent is absolute rather than relative, because a nested list is
        // drawn by a recursive call and not inside an indented region: a level
        // knows how deep it is and puts itself there.
        #[allow(clippy::cast_precision_loss)]
        let indent = LIST_STEP * self.zoom * (level as f32 + 1.0);
        let gutter = LIST_GUTTER * self.zoom;

        for item in element.elements() {
            let numbered = item.is(&Ns::Text, "list-item");
            if !numbered && !item.is(&Ns::Text, "list-header") {
                continue;
            }
            let label = if numbered {
                let level_style = self.level_style(&style_name, level);
                let start = level_style
                    .as_ref()
                    .and_then(|s| s.attr_usize(&Ns::Text, "start-value"))
                    .unwrap_or(1);
                let number = counters.bump(level, start);
                Self::label(level_style.as_ref(), level, number, counters)
            } else {
                // A list header is the paragraph a list may begin with, and it
                // is not numbered and does not count.
                String::new()
            };

            let mut first = true;
            for block in item.elements() {
                if block.is(&Ns::Text, "list") {
                    self.list(ui, block, width, level + 1, Some(&style_name), counters);
                    continue;
                }
                let on_this_block = if first && !label.is_empty() {
                    Some(label.as_str())
                } else {
                    None
                };
                if block.is(&Ns::Text, "p") || block.is(&Ns::Text, "h") {
                    first = false;
                    // The label is drawn in the gutter this space opens, left
                    // of where the text begins, so that a wide label crowds the
                    // indent rather than the first word.
                    ui.horizontal_top(|ui| {
                        ui.add_space(indent + gutter);
                        self.paragraph(ui, block, width - indent - gutter, on_this_block);
                    });
                } else if block.is(&Ns::Table, "table") {
                    first = false;
                    ui.horizontal_top(|ui| {
                        ui.add_space(indent + gutter);
                        self.table(ui, block, width - indent - gutter);
                    });
                } else if block.is(&Ns::Draw, "frame") {
                    first = false;
                    ui.horizontal_top(|ui| {
                        ui.add_space(indent + gutter);
                        self.frame(ui, block, width - indent - gutter);
                    });
                }
            }
        }
    }

    /// The level style of a list, which says whether the level is numbered or
    /// bulleted and how.
    fn level_style(&self, list_style: &str, level: usize) -> Option<Element> {
        let style = self.document.styles.list_style(list_style)?;
        style
            .elements()
            .find(|e| e.attr_usize(&Ns::Text, "level") == Some(level + 1))
            .cloned()
    }

    /// The text drawn in front of a list item.
    fn label(
        level_style: Option<&Element>,
        level: usize,
        number: usize,
        counters: &Counters,
    ) -> String {
        let Some(style) = level_style else {
            // A list whose style the document did not write, which happens in a
            // document assembled by something that left the style behind.
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
            let prefix = style.attr(&Ns::Text, "num-prefix").unwrap_or_default();
            let suffix = style.attr(&Ns::Text, "num-suffix").unwrap_or_default();
            // `text:display-levels` is how 1.2.3 is written: the level's own
            // number preceded by its ancestors'.
            let display = style
                .attr_usize(&Ns::Text, "display-levels")
                .unwrap_or(1)
                .max(1);
            let mut numbers = Vec::new();
            let first = (level + 1).saturating_sub(display);
            for ancestor in first..level {
                numbers.push(number_text(counters.at(ancestor), format));
            }
            numbers.push(number_text(number, format));
            return format!("{prefix}{}{suffix}", numbers.join("."));
        }
        // A level drawn with an image, which is a picture this does not fetch.
        "\u{2022}".to_owned()
    }

    /// A table, drawn as a grid of cells with the document's own widths.
    fn table(&mut self, ui: &mut Ui, table: &Element, width: f32) {
        let columns = column_widths(self.document, table, width, self.zoom);
        if columns.is_empty() {
            return;
        }
        ui.add_space(4.0 * self.zoom);
        self.rows(ui, table, &columns);
        ui.add_space(4.0 * self.zoom);
    }

    fn rows(&mut self, ui: &mut Ui, parent: &Element, columns: &[f32]) {
        for element in parent.elements() {
            if element.is(&Ns::Table, "table-row") {
                self.row(ui, element, columns);
            } else if element.is(&Ns::Table, "table-header-rows")
                || element.is(&Ns::Table, "table-rows")
                || element.is(&Ns::Table, "table-row-group")
            {
                self.rows(ui, element, columns);
            }
        }
    }

    fn row(&mut self, ui: &mut Ui, row: &Element, columns: &[f32]) {
        // The backgrounds and the borders have to be painted behind the text, and
        // the row's height is only known once the text is laid out, so the shapes
        // are reserved now and filled in after.
        let reserved = ui.painter().add(eframe::egui::Shape::Noop);
        let mut painted = Vec::new();

        let response = ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let mut column = 0usize;
            for cell in row.elements() {
                let covered = cell.is(&Ns::Table, "covered-table-cell");
                if !covered && !cell.is(&Ns::Table, "table-cell") {
                    continue;
                }
                let repeat = cell
                    .attr_usize(&Ns::Table, "number-columns-repeated")
                    .unwrap_or(1)
                    .max(1);
                let spanned = cell
                    .attr_usize(&Ns::Table, "number-columns-spanned")
                    .unwrap_or(1)
                    .max(1);
                for _ in 0..repeat {
                    // The width covers every column the cell spans; the position
                    // advances by one. ODF writes a `table:covered-table-cell`
                    // for each further column a span reaches, so advancing by the
                    // span here as well would count those columns twice and push
                    // the rest of the row off the end of the table.
                    let width: f32 = columns
                        .iter()
                        .skip(column)
                        .take(spanned)
                        .sum::<f32>()
                        .max(8.0);
                    if !covered {
                        let properties = self.style_of(cell, &Family::TableCell);
                        let padding = properties
                            .cell
                            .padding
                            .left
                            .map_or(2.0, odox_core::Length::points)
                            * self.zoom;
                        let inner = ui
                            .allocate_ui_with_layout(
                                vec2(width, 0.0),
                                eframe::egui::Layout::top_down(Align::Min),
                                |ui| {
                                    ui.add_space(padding);
                                    ui.set_min_width(width);
                                    ui.set_max_width(width);
                                    let content = (width - padding * 2.0).max(8.0);
                                    self.blocks(ui, cell, content);
                                    ui.add_space(padding);
                                },
                            )
                            .response
                            .rect;
                        painted.push((inner, properties));
                    }
                    column += 1;
                }
            }
        });

        let row_rect = response.response.rect;
        let mut shapes = Vec::new();
        for (rect, properties) in painted {
            let cell_rect = Rect::from_min_max(
                pos2(rect.left(), row_rect.top()),
                pos2(rect.right(), row_rect.bottom()),
            );
            if let Some(fill) = properties.cell.background {
                shapes.push(eframe::egui::Shape::rect_filled(
                    cell_rect,
                    0.0,
                    format::color32(fill),
                ));
            }
            for (edge, from, to) in edges(cell_rect) {
                if let Some(border) = edge_of(&properties.cell.border, edge) {
                    shapes.push(eframe::egui::Shape::line_segment(
                        [from, to],
                        Stroke::new(
                            (border.width.points() * self.zoom).max(1.0),
                            format::color32(border.color),
                        ),
                    ));
                }
            }
        }
        ui.painter().set(reserved, eframe::egui::Shape::Vec(shapes));
    }

    /// A frame: a box with a picture, a text box or an object in it.
    fn frame(&mut self, ui: &mut Ui, frame: &Element, width: f32) {
        let zoom = self.zoom;
        let declared = |local: &str| {
            frame
                .attr(&Ns::Svg, local)
                .and_then(odox_core::Length::parse)
                .map(|l| l.points() * zoom)
        };

        if let Some(image) = frame.child(&Ns::Draw, "image") {
            let href = image
                .attr(&Ns::Xlink, "href")
                .unwrap_or_default()
                .to_owned();
            let found = self
                .pictures
                .get(ui.ctx(), self.document, &href)
                .map(|texture| (texture.id(), texture.size()));
            if let Some((id, [pixels_wide, pixels_high])) = found {
                #[allow(clippy::cast_precision_loss)]
                let aspect = if pixels_wide == 0 {
                    1.0
                } else {
                    pixels_high as f32 / pixels_wide as f32
                };
                let w = declared("width").unwrap_or(width).min(width);
                let h = declared("height").unwrap_or(w * aspect);
                let size = vec2(w, h);
                ui.add(eframe::egui::Image::new((id, size)).fit_to_exact_size(size));
                return;
            }
        }

        // A text box draws the blocks inside it; anything else — an embedded
        // object, a chart, a formula — is a box the size the document asked for,
        // so that the page does not silently lose the space it occupied.
        if let Some(box_) = frame.child(&Ns::Draw, "text-box") {
            let w = declared("width").unwrap_or(width).min(width);
            ui.allocate_ui_with_layout(
                vec2(w, 0.0),
                eframe::egui::Layout::top_down(Align::Min),
                |ui| {
                    ui.set_max_width(w);
                    eframe::egui::Frame::group(ui.style()).show(ui, |ui| {
                        self.blocks(ui, box_, w - 16.0 * zoom);
                    });
                },
            );
            return;
        }

        let w = declared("width").unwrap_or(width).min(width);
        let h = declared("height").unwrap_or(48.0 * zoom);
        let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
        ui.painter().rect_stroke(
            rect,
            2.0,
            Stroke::new(1.0, self.palette.ink.gamma_multiply(0.3)),
            StrokeKind::Inside,
        );
    }

    /// Where the document says a page ended.
    ///
    /// Not a page: this view does not paginate, and drawing the break the
    /// producer recorded is how a reader sees that the document has them.
    fn page_break(&mut self, ui: &mut Ui, width: f32) {
        ui.add_space(8.0 * self.zoom);
        let (rect, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
        ui.painter().hline(
            rect.x_range(),
            rect.center().y,
            Stroke::new(1.0, self.palette.ink.gamma_multiply(0.3)),
        );
        ui.add_space(8.0 * self.zoom);
    }
}

/// A child style over its parent: a property the child does not state is the
/// parent's.
fn merge(parent: &TextProperties, child: &TextProperties) -> TextProperties {
    TextProperties {
        font_family: child
            .font_family
            .clone()
            .or_else(|| parent.font_family.clone()),
        size: child.size.or(parent.size),
        bold: child.bold.or(parent.bold),
        italic: child.italic.or(parent.italic),
        underline: child.underline.or(parent.underline),
        strike: child.strike.or(parent.strike),
        color: child.color.or(parent.color),
        background: child.background.or(parent.background),
        position: child.position.or(parent.position),
        uppercase: child.uppercase.or(parent.uppercase),
    }
}

/// Which edge of a cell a border belongs to.
#[derive(Clone, Copy)]
enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

fn edges(rect: Rect) -> [(Edge, Pos2, Pos2); 4] {
    [
        (Edge::Left, rect.left_top(), rect.left_bottom()),
        (Edge::Right, rect.right_top(), rect.right_bottom()),
        (Edge::Top, rect.left_top(), rect.right_top()),
        (Edge::Bottom, rect.left_bottom(), rect.right_bottom()),
    ]
}

fn edge_of(borders: &odox_core::Edges<Border>, edge: Edge) -> Option<Border> {
    match edge {
        Edge::Left => borders.left,
        Edge::Right => borders.right,
        Edge::Top => borders.top,
        Edge::Bottom => borders.bottom,
    }
}

fn paint_borders(ui: &Ui, rect: Rect, borders: &odox_core::Edges<Border>, zoom: f32) {
    for (edge, from, to) in edges(rect) {
        if let Some(border) = edge_of(borders, edge) {
            ui.painter().line_segment(
                [from, to],
                Stroke::new(
                    (border.width.points() * zoom).max(1.0),
                    format::color32(border.color),
                ),
            );
        }
    }
}

/// The width of each of a table's columns in screen points.
///
/// A column style usually gives one. Where none does, the available width is
/// divided evenly, which is what a producer that wrote no widths meant.
fn column_widths(document: &Document, table: &Element, width: f32, zoom: f32) -> Vec<f32> {
    let mut declared = Vec::new();
    collect_columns(document, table, &mut declared);
    if declared.is_empty() {
        return Vec::new();
    }
    let total: f32 = declared.iter().filter_map(|w| *w).sum();
    let unstated = declared.iter().filter(|w| w.is_none()).count();

    // A table wider than the space is scaled down rather than clipped, which is
    // what a reading view owes a document written for a wider page.
    let scaled = total * zoom;
    let factor = if scaled > width && scaled > 0.0 {
        width / scaled
    } else {
        1.0
    };
    #[allow(clippy::cast_precision_loss)]
    let share = if unstated == 0 {
        0.0
    } else {
        ((width - scaled * factor) / unstated as f32).max(16.0)
    };
    declared
        .into_iter()
        .map(|w| w.map_or(share, |points| points * zoom * factor))
        .collect()
}

fn collect_columns(document: &Document, parent: &Element, into: &mut Vec<Option<f32>>) {
    for element in parent.elements() {
        if element.is(&Ns::Table, "table-column") {
            let repeat = element
                .attr_usize(&Ns::Table, "number-columns-repeated")
                .unwrap_or(1)
                .clamp(1, 1024);
            let width = element
                .attr(&Ns::Table, "style-name")
                .map(|name| document.styles.resolve(&Family::TableColumn, name))
                .and_then(|p| p.column_width)
                .map(odox_core::Length::points);
            for _ in 0..repeat {
                into.push(width);
            }
        } else if element.is(&Ns::Table, "table-columns")
            || element.is(&Ns::Table, "table-header-columns")
            || element.is(&Ns::Table, "table-column-group")
        {
            collect_columns(document, element, into);
        }
    }
}

/// A number in the format a list level asks for.
fn number_text(number: usize, format: &str) -> String {
    match format.chars().next() {
        Some('a') => alphabetic(number, b'a'),
        Some('A') => alphabetic(number, b'A'),
        Some('i') => roman(number).to_lowercase(),
        Some('I') => roman(number),
        // An empty format is a level that shows no number, which ODF uses for a
        // list whose label is only its prefix and suffix.
        None => String::new(),
        _ => number.to_string(),
    }
}

/// `a`, `b`, … `z`, `aa`, which is the spreadsheet column rule and ODF's.
fn alphabetic(number: usize, first: u8) -> String {
    let mut n = number;
    let mut out = Vec::new();
    while n > 0 {
        let remainder = (n - 1) % 26;
        out.push(first + u8::try_from(remainder).unwrap_or(0));
        n = (n - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn roman(number: usize) -> String {
    const VALUES: [(usize, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    // Beyond what Roman numerals reach, the number itself is more use than a
    // line of Ms.
    if number == 0 || number > 3999 {
        return number.to_string();
    }
    let mut left = number;
    let mut out = String::new();
    for (value, numeral) in VALUES {
        while left >= value {
            out.push_str(numeral);
            left -= value;
        }
    }
    out
}

/// Whether an element holds blocks on the body's behalf rather than being one.
fn is_block_container(element: &Element) -> bool {
    element.name.ns == Ns::Text
        && matches!(
            &*element.name.local,
            "section"
                | "index-body"
                | "index-title"
                | "table-of-content"
                | "illustration-index"
                | "table-index"
                | "object-index"
                | "user-index"
                | "alphabetical-index"
                | "bibliography"
                | "tracked-changes"
                | "deletion"
        )
}

/// Whether an element is a wrapper around text rather than text of its own.
///
/// A bookmark, a reference mark and a change mark each sit inside a paragraph,
/// carry no characters, and may have text inside them that does belong to the
/// paragraph.
fn is_inline_passthrough(element: &Element) -> bool {
    element.name.ns == Ns::Text
        && matches!(
            &*element.name.local,
            "bookmark"
                | "bookmark-start"
                | "bookmark-end"
                | "reference-mark"
                | "reference-mark-start"
                | "reference-mark-end"
                | "span"
                | "bibliography-mark"
                | "ruby"
                | "ruby-base"
                | "meta"
                | "meta-field"
                | "change-start"
                | "change-end"
                | "page-number"
                | "page-count"
                | "title"
                | "subject"
                | "author-name"
                | "author-initials"
                | "chapter"
                | "file-name"
                | "sheet-name"
                | "date"
                | "time"
                | "creator"
                | "description"
                | "keywords"
                | "sequence"
                | "bookmark-ref"
                | "sequence-ref"
                | "reference-ref"
                | "variable-get"
                | "variable-set"
                | "user-field-get"
                | "placeholder"
                | "conditional-text"
                | "hidden-text"
                | "text-input"
        )
}
