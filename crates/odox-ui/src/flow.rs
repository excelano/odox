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
use std::sync::Arc;

use eframe::egui::emath::TSTransform;
use eframe::egui::text_selection::accesskit_text::update_accesskit_for_text_widget;
use eframe::egui::{
    Align, Align2, Color32, ColorImage, Context, CursorIcon, Galley, Pos2, Rect, Response, Role,
    Sense, Stroke, StrokeKind, TextFormat, TextureHandle, TextureOptions, Ui, Vec2, WidgetInfo,
    pos2,
    text::{CCursor, CCursorRange, LayoutJob},
    text_selection::{LabelSelectionState, visuals::paint_text_selection},
    vec2,
};
use egui_richedit::{Laid, OffsetMap, ParagraphJob};
use odox_core::{
    Border, Document, Element, Family, Gradient, Node, Ns, Offset, Properties, SMALL_CAPS, TabKind,
    TextAlign, TextProperties, Tiling, bidi,
    edit::{self, ListCounters},
};

use eframe::egui::epaint::text::ByteRangeExt as _;

use crate::find::Highlights;
use crate::flow_model::PageEditor;
use crate::format::{self, DEFAULT_SIZE};
use crate::i18n::t;
use crate::links;

/// Pictures already decoded, kept for as long as the document is open.
///
/// A failed decode is remembered as a failure, so that a picture in a format
/// nothing here reads is not decoded again on every frame.
#[derive(Default)]
pub struct Pictures {
    textures: HashMap<String, Option<TextureHandle>>,
    tiles: HashMap<String, Option<Tile>>,
    gradients: HashMap<(String, u32), TextureHandle>,
}

/// A picture made ready to fill a shape tile by tile.
pub struct Tile {
    /// One tile, or for tiles whose alternate rows or columns are moved, two
    /// rows or two columns of them, each moved as the second is; or for a
    /// picture placed once, the picture inside a clear border its edge
    /// stretches into.
    pub texture: TextureHandle,
    /// The picture's own size in pixels.
    pub pixels: [usize; 2],
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

    /// The texture that fills a shape with a picture laid as `tiling` says,
    /// built once for each picture and way of laying it.
    pub fn tile(
        &mut self,
        ctx: &Context,
        document: &Document,
        href: &str,
        tiling: &Tiling,
    ) -> Option<&Tile> {
        let key = format!("{href}\n{}\n{:?}", tiling.repeat, tiling.offset);
        if !self.tiles.contains_key(&key) {
            let tile = document.picture(href).and_then(decode).map(|image| {
                let pixels = image.size;
                let (laid, options) = laid(&image, tiling);
                Tile {
                    texture: ctx.load_texture(&key, laid, options),
                    pixels,
                }
            });
            self.tiles.insert(key.clone(), tile);
        }
        self.tiles.get(&key).and_then(Option::as_ref)
    }

    /// A gradient drawn as a picture to stretch over a shape of `size`, for
    /// the styles whose colour does not run straight across it. The picture
    /// depends only on the shape's proportions, and one is kept for each.
    pub fn gradient(
        &mut self,
        ctx: &Context,
        name: &str,
        gradient: &Gradient,
        size: Vec2,
    ) -> &TextureHandle {
        const LONGEST: f32 = 256.0;
        let aspect = (size.x / size.y.max(f32::EPSILON)).clamp(1.0 / LONGEST, LONGEST);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let key = (name.to_owned(), (aspect * 100.0).round() as u32);
        self.gradients.entry(key).or_insert_with(|| {
            let drawn = if aspect >= 1.0 {
                vec2(LONGEST, LONGEST / aspect)
            } else {
                vec2(LONGEST * aspect, LONGEST)
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let [width, height] = [
                drawn.x.ceil().max(2.0) as usize,
                drawn.y.ceil().max(2.0) as usize,
            ];
            let mut image = ColorImage::filled([width, height], Color32::TRANSPARENT);
            #[allow(clippy::cast_precision_loss)]
            for y in 0..height {
                for x in 0..width {
                    let point = ((x as f32 + 0.5), (y as f32 + 0.5));
                    let level = gradient.level(point, (width as f32, height as f32));
                    image.pixels[y * width + x] =
                        format::blend(gradient.start, gradient.end, level);
                }
            }
            ctx.load_texture(format!("gradient {name}"), image, TextureOptions::LINEAR)
        })
    }

    /// Forget every picture, for a window that has closed its document.
    pub fn clear(&mut self) {
        self.textures.clear();
        self.tiles.clear();
        self.gradients.clear();
    }
}

/// The picture as the texture a tiled fill samples: repeating, with a second
/// row or column moved where alternate ones are, or held inside a clear
/// border where the picture is placed once.
fn laid(image: &ColorImage, tiling: &Tiling) -> (ColorImage, TextureOptions) {
    let [width, height] = image.size;
    let at = |x: usize, y: usize| image.pixels[y * width + x];
    let moved = |fraction: f32, length: usize| {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let pixels = (fraction.rem_euclid(1.0) * length as f32).round() as usize;
        pixels % length.max(1)
    };
    if !tiling.repeat {
        let mut framed = ColorImage::filled([width + 2, height + 2], Color32::TRANSPARENT);
        for y in 0..height {
            for x in 0..width {
                framed.pixels[(y + 1) * (width + 2) + x + 1] = at(x, y);
            }
        }
        return (framed, TextureOptions::LINEAR);
    }
    let laid = match tiling.offset {
        None => image.clone(),
        Some(Offset::Rows(fraction)) => {
            let shift = moved(fraction, width);
            let mut laid = ColorImage::filled([width, height * 2], Color32::TRANSPARENT);
            for y in 0..height {
                for x in 0..width {
                    laid.pixels[y * width + x] = at(x, y);
                    laid.pixels[(y + height) * width + x] = at((x + width - shift) % width, y);
                }
            }
            laid
        }
        Some(Offset::Columns(fraction)) => {
            let shift = moved(fraction, height);
            let mut laid = ColorImage::filled([width * 2, height], Color32::TRANSPARENT);
            for y in 0..height {
                for x in 0..width {
                    laid.pixels[y * width * 2 + x] = at(x, y);
                    laid.pixels[y * width * 2 + width + x] = at(x, (y + height - shift) % height);
                }
            }
            laid
        }
    };
    (laid, TextureOptions::LINEAR_REPEAT)
}

fn decode(bytes: &[u8]) -> Option<ColorImage> {
    let decoded = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [decoded.width() as usize, decoded.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, decoded.as_raw()))
}

/// A flow laid out upright where nothing of it is seen and drawn turned onto
/// a shape: the pointer is taken on the shape, and the page editor sees it
/// turned back into the upright layout.
#[derive(Clone)]
pub struct Turned {
    /// The shape's own response, which takes the clicks the upright
    /// paragraphs cannot.
    pub response: Response,
    /// Where the pointer is, turned back into the upright layout.
    pub pointer: Option<Pos2>,
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
    /// A place in the document to bring into view: a bookmark's name, or a
    /// heading's text written `Name|outline`. Answered while the body is drawn,
    /// for the same reason `scroll_to_heading` is.
    pub scroll_to_place: Option<String>,
    /// A place in the document that a link just followed asked for, for the
    /// view to set `scroll_to_place` to on the next frame.
    pub followed: Option<String>,
    /// How many headings have been drawn this pass.
    headings_seen: usize,
    /// The page editor, which puts a caret in every paragraph under the root
    /// and paints them. Outside edit mode there is none.
    pub page: Option<&'a mut PageEditor>,
    /// The matches of a search, drawn behind the text they are in.
    pub find: Option<Highlights<'a>>,
    /// Whether text in the page can be dragged over to select it. Off on a
    /// slide in edit mode, where a drag moves the shape instead.
    pub selectable: bool,
    /// The style of the shape the flow is drawn in, under each paragraph's
    /// own, as [`Properties::in_shape`] lays them.
    pub under: Option<std::rc::Rc<Properties>>,
    /// Where the flow is laid out upright and drawn turned onto a shape.
    pub turned: Option<Turned>,
    /// Where the flow is, as a path of child indices from the root, or from
    /// wherever [`Self::start_at`] said the root sits.
    path: Vec<usize>,
    /// Inside a frame anchored in a paragraph, which is reached by a clone
    /// and not by a path, or in the second and later copies of a repeated
    /// cell, whose copies share one path: nothing in either can be edited in
    /// place.
    detached: bool,
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
            scroll_to_place: None,
            followed: None,
            headings_seen: 0,
            page: None,
            find: None,
            selectable: true,
            under: None,
            turned: None,
            path: Vec::new(),
            detached: false,
        }
    }

    /// Name where the root this flow draws sits under something larger, so
    /// that every path it reports and matches is from that larger root: a
    /// slide's label is drawn from the shape and edited from the page.
    pub fn start_at(&mut self, prefix: Vec<usize>) {
        self.path = prefix;
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
    /// Whether the run's characters are the paragraph's own text, which the
    /// caret moves through, or a field's value, which it steps over.
    counted: bool,
}

impl Flow<'_> {
    /// Draw every block under an element, in order.
    ///
    /// `width` is the space available in screen points, and is what a paragraph
    /// wraps to.
    pub fn blocks(&mut self, ui: &mut Ui, parent: &Element, width: f32) {
        let mut counters = ListCounters::default();
        self.blocks_with(ui, parent, width, &mut counters);
    }

    fn blocks_with(
        &mut self,
        ui: &mut Ui,
        parent: &Element,
        width: f32,
        counters: &mut ListCounters,
    ) {
        for (index, element) in parent.elements_indexed() {
            self.path.push(index);
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
                        *counters = ListCounters::default();
                    }
                    self.list(ui, element, width, 0, None, counters);
                }
                () if element.is(&Ns::Table, "table") => self.table(ui, element, width),
                () if element.is(&Ns::Draw, "frame") => self.frame(ui, element, width),
                () if element.is(&Ns::Text, "soft-page-break") => self.page_break(ui, width),
                () if edit::is_block_container(element) => {
                    self.blocks_with(ui, element, width, counters);
                }
                () => {}
            }
            self.path.pop();
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

    /// A paragraph's style, over the shape's it is drawn in where there is one.
    fn paragraph_style(&self, element: &Element) -> std::rc::Rc<Properties> {
        let properties = self.style_of(element, &Family::Paragraph);
        match &self.under {
            Some(shape) => std::rc::Rc::new(properties.in_shape(shape)),
            None => properties,
        }
    }

    /// Colour the matches of a search in a paragraph, and scroll to the current
    /// one when a search has just moved to it.
    fn show_matches(&self, ui: &Ui, galley: &mut Arc<Galley>, map: &OffsetMap, origin: Pos2) {
        let Some(find) = self.find.as_ref().filter(|_| !self.detached) else {
            return;
        };
        let hits = find.within(&self.path);
        if hits.is_empty() {
            return;
        }
        let at = |offset: usize| CCursor::new(map.to_galley(offset.min(map.model_len())));
        for (range, current) in hits {
            let mut visuals = ui.visuals().clone();
            visuals.selection.bg_fill = format::match_fill(current);
            visuals.selection.stroke.color = Color32::BLACK;
            let cursors = CCursorRange::two(at(range.start), at(range.end));
            paint_text_selection(galley, &visuals, &cursors, None);
            if current && find.reveal() {
                let start = galley.pos_from_cursor(at(range.start));
                let end = galley.pos_from_cursor(at(range.end));
                ui.scroll_to_rect(
                    start.union(end).translate(origin.to_vec2()),
                    Some(Align::Center),
                );
            }
        }
    }

    /// A link under the pointer shows where it goes, and is followed by a click
    /// where the text is being read and by Ctrl and a click where it is being
    /// edited, a plain click there being how the caret is put down.
    #[allow(clippy::too_many_arguments)]
    fn follow_link(
        &mut self,
        ui: &Ui,
        response: &Response,
        links: &[(std::ops::Range<usize>, &str)],
        galley: &Galley,
        map: &OffsetMap,
        origin: Pos2,
        editing: bool,
    ) {
        let Some(pointer) = response
            .hover_pos()
            .or_else(|| response.interact_pointer_pos())
        else {
            return;
        };
        let cursor = galley.cursor_from_pos(pointer - origin);
        let caret = galley.pos_from_cursor(cursor).center().x + origin.x;
        let character = if pointer.x < caret {
            cursor.index.0.saturating_sub(1)
        } else {
            cursor.index.0
        };
        let offset = map.to_model(character);
        let Some(href) = links
            .iter()
            .find(|(range, _)| range.contains(&offset))
            .map(|&(_, href)| href)
        else {
            return;
        };
        let Some(target) = links::target(href) else {
            return;
        };
        let reaching = !editing || ui.input(|input| input.modifiers.command);
        if reaching {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if response.hovered() {
            response.clone().on_hover_text(href);
        }
        if reaching && response.clicked() {
            match target {
                links::Target::External(address) => links::open(ui.ctx(), address),
                links::Target::Within(place) => self.followed = Some(place.to_owned()),
            }
        }
    }

    /// One paragraph or heading, with an optional list label drawn in its margin.
    fn paragraph(&mut self, ui: &mut Ui, element: &Element, width: f32, label: Option<&str>) {
        let properties = self.paragraph_style(element);
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

        let palette = self.palette;
        if let Some(fill) = properties.paragraph.background {
            self.palette = palette.on(format::color32(fill));
        }
        let base = format::text_format(&properties.text, DEFAULT_SIZE, zoom, self.palette);

        let (mut job, frames) = self.layout_job(element, &properties, &base, size, wrap);
        let ground = self.palette;
        self.palette = palette;
        let leaders = place_tabs(ui, &mut job, &properties.paragraph, indent.max(0.0), zoom);
        let (job, map) = job.into_parts();

        let right_to_left = properties.paragraph.right_to_left == Some(true);
        let written = bidi::needed(&job.text, right_to_left).then(|| job.clone());
        let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
        // A paragraph with text that reads right to left is drawn from a
        // second layout, each of its lines in the order it reads, which egui
        // does not do; what is drawn then no longer lines up with what is
        // written character for character, so it is read, not edited.
        let visual = written.map(|written| in_visual_order(ui, &written, right_to_left));
        let height = visual.as_ref().unwrap_or(&galley).size().y;
        let edited = !self.detached && self.page.is_some() && visual.is_none();
        // Click and drag, and not merely hover: the selection plugin begins a
        // selection only on a response whose sense includes drag, which is what
        // `Label` adds to its own when it is selectable, and the click half is
        // what lets a double-click take a word and a triple-click a line.
        // Without the drag the pointer reaches the scroll area instead and
        // nothing is selected — measured, not read. The page editor selects
        // the same way, and on a slide in edit mode, where a drag moves the
        // shape, it takes clicks alone.
        let sense = if self.selectable {
            Sense::click_and_drag()
        } else {
            Sense::click()
        };
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);

        if element.is(&Ns::Text, "h") {
            if self.scroll_to_heading == Some(self.headings_seen) {
                ui.scroll_to_rect(rect, Some(Align::TOP));
            }
            self.headings_seen += 1;
        }
        if let Some(place) = &self.scroll_to_place
            && holds_place(element, place)
        {
            ui.scroll_to_rect(rect, Some(Align::TOP));
        }

        if let Some(fill) = properties.paragraph.background {
            ui.painter().rect_filled(rect, 0.0, format::color32(fill));
        }
        paint_borders(ui, rect, &properties.paragraph.border, zoom, ground);

        // The anchor the galley was laid out around: its rows are positioned
        // relative to this, which is what makes a centred paragraph centre each
        // of its rows rather than its block.
        let anchor = match properties.paragraph.align.unwrap_or(TextAlign::Start) {
            TextAlign::Center => rect.left() + left + indent.max(0.0) + wrap / 2.0,
            TextAlign::End => rect.left() + left + indent.max(0.0) + wrap,
            _ => rect.left() + left + indent.max(0.0),
        };
        // Through the page editor in edit mode, and through egui's selection
        // plugin otherwise, rather than the painter: either is what lets a
        // person drag across the page and press Ctrl+C. Both paint the galley
        // themselves, at the same anchor the painter would have taken, so
        // alignment is untouched. Both are called for every paragraph and not
        // only the visible ones: the plugin drops a selection whose ends it did
        // not see this frame, and the editor moves Up and Down through where
        // each paragraph was drawn.
        let origin = pos2(anchor, rect.top());
        paint_leaders(ui, &leaders, origin, &base);
        if let Some(visual) = visual {
            ui.painter().galley(origin, visual, base.color);
            expose(ui, &response, element, &galley, origin, false);
            if self.page.is_some() && !self.detached {
                response.on_hover_text(t("Text that reads right to left cannot be edited yet."));
            }
        } else {
            self.written_order(ui, element, &response, (galley, map), origin, &base, edited);
        }

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

        // Reached by a clone rather than by a path, so nothing inside is
        // edited in place.
        let was_detached = self.detached;
        self.detached = true;
        for frame in frames {
            self.frame(ui, &frame, body_width);
        }
        self.detached = was_detached;
        if after > 0.0 {
            ui.add_space(after);
        }
    }

    /// Draw a paragraph laid out in the order it is written, through the page
    /// editor in edit mode and egui's selection otherwise, with its links.
    #[allow(clippy::too_many_arguments)]
    fn written_order(
        &mut self,
        ui: &mut Ui,
        element: &Element,
        response: &Response,
        (galley, map): (Arc<Galley>, OffsetMap),
        origin: Pos2,
        base: &TextFormat,
        edited: bool,
    ) {
        let mut galley = galley;
        self.show_matches(ui, &mut galley, &map, origin);
        let readable = Arc::clone(&galley);
        let links = edit::links(element);
        let hit = (!links.is_empty()).then(|| (Arc::clone(&galley), map.clone()));
        if let Some(page) = self.page.as_deref_mut().filter(|_| edited) {
            let laid = Laid {
                galley,
                map,
                origin,
            };
            match &self.turned {
                Some(turned) => {
                    let mut moved = turned.response.clone();
                    moved.rect = response.rect;
                    moved.interact_rect = response.interact_rect;
                    page.paragraph_moved(ui, &moved, &self.path, laid, turned.pointer);
                }
                None => page.paragraph(ui, response, &self.path, laid),
            }
        } else {
            LabelSelectionState::label_text_selection(
                ui,
                response,
                origin,
                galley,
                base.color,
                Stroke::NONE,
            );
        }

        expose(ui, response, element, &readable, origin, edited);

        if let Some((galley, map)) = hit {
            self.follow_link(ui, response, &links, &galley, &map, origin, edited);
        }
    }

    /// The layout job for a paragraph, and the frames anchored inside it,
    /// which are drawn after it.
    fn layout_job(
        &self,
        element: &Element,
        properties: &Properties,
        base: &TextFormat,
        size: f32,
        wrap: f32,
    ) -> (ParagraphJob, Vec<Element>) {
        let job = LayoutJob {
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
        let mut job = ParagraphJob::new(job);
        let mut frames = Vec::new();
        let run = Run {
            inherited: &properties.text,
            size,
            format: base,
            palette: self.palette,
            counted: true,
        };
        self.runs(element, &run, &mut job, &mut frames);

        // An empty paragraph is a blank line and has to take its height, which an
        // empty layout job would not.
        if job.is_empty() {
            job.atom(" ", 0, base.clone());
        }
        if let Some(measure) = properties.paragraph.line_height {
            // A proportional line height is a proportion of each run's own
            // size, so a span set larger than its paragraph takes a taller
            // line; an absolute one is the same for every run.
            for format in job.formats_mut() {
                let own = format.font_id.size / self.zoom;
                format.line_height = Some(measure.resolve(own) * self.zoom);
            }
        }
        (job, frames)
    }

    /// Append the text of a paragraph's children to a layout job.
    ///
    /// Anything that is not text is either resolved into characters — ODF spells
    /// out runs of spaces, tabs and line breaks rather than writing them
    /// literally — or collected to be drawn after the paragraph, which is what
    /// happens to a picture anchored inside one.
    ///
    /// Each piece is recorded against the characters [`edit::text`] gives the
    /// paragraph, so that a caret placed in what is drawn lands in the text an
    /// edit changes: where the two differ, as a tab drawn as spaces or a field
    /// drawn as its value, the piece is an atom the caret steps over.
    fn runs(
        &self,
        parent: &Element,
        run: &Run<'_>,
        job: &mut ParagraphJob,
        frames: &mut Vec<Element>,
    ) {
        let Run {
            inherited,
            size,
            format,
            palette,
            counted,
        } = *run;
        let mut small = format.clone();
        small.font_id.size *= SMALL_CAPS;
        let put = |job: &mut ParagraphJob, shown: &str, model_len: usize| {
            let same_length = shown.chars().count() == model_len;
            for (piece, drawn_small) in inherited.cased(shown) {
                let format = if drawn_small { &small } else { format };
                if !counted {
                    job.atom(&piece, 0, format.clone());
                } else if same_length {
                    job.text(&piece, format.clone());
                } else {
                    job.atom(&piece, model_len, format.clone());
                }
            }
        };
        for child in &parent.children {
            match child {
                Node::Text(text) | Node::CData(text) => put(job, text, text.chars().count()),
                Node::Comment(_) | Node::ProcessingInstruction(_) => {}
                Node::Element(element) => {
                    if element.is(&Ns::Text, "s") {
                        let count = element.attr_usize(&Ns::Text, "c").unwrap_or(1);
                        put(job, &" ".repeat(count.min(256)), count);
                    } else if element.is(&Ns::Text, "tab") {
                        if counted {
                            job.tab(format.clone());
                        } else {
                            job.atom("    ", 0, format.clone());
                        }
                    } else if element.is(&Ns::Text, "line-break") {
                        put(job, "\n", 1);
                    } else if element.is(&Ns::Text, "span") {
                        let style = self.style_of(element, &Family::Text);
                        let merged = style.text.over(inherited);
                        let inner_size = format::size_of(&merged, size);
                        let inner = format::text_format(&merged, size, self.zoom, palette);
                        let run = Run {
                            inherited: &merged,
                            size: inner_size,
                            format: &inner,
                            palette,
                            counted,
                        };
                        self.runs(element, &run, job, frames);
                    } else if element.is(&Ns::Text, "a") {
                        let style = self.style_of(element, &Family::Text);
                        let merged = style.text.over(inherited);
                        let inner_size = format::size_of(&merged, size);
                        let inner = format::link_format(&merged, size, self.zoom, palette);
                        let run = Run {
                            inherited: &merged,
                            size: inner_size,
                            format: &inner,
                            palette,
                            counted,
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
                            job.atom(&citation.plain_text(), 0, raised);
                        }
                    } else if element.is(&Ns::Text, "ruby-text") {
                        job.atom(&edit::ruby_shown(element), 0, format.clone());
                    } else if edit::is_inline_passthrough(element) {
                        // A mark's contents are the paragraph's text; a field's
                        // are its value, which the paragraph's text does not
                        // hold.
                        let run = Run {
                            counted: counted && edit::holds_text(element),
                            ..*run
                        };
                        self.runs(element, &run, job, frames);
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
        counters: &mut ListCounters,
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

        for (item_index, item) in element.elements_indexed() {
            let numbered = item.is(&Ns::Text, "list-item");
            if !numbered && !item.is(&Ns::Text, "list-header") {
                continue;
            }
            self.path.push(item_index);
            let label = if numbered {
                let level_style = self.level_style(&style_name, level);
                counters.count(item, level, level_style.as_ref());
                counters.label(level, |at| self.level_style(&style_name, at))
            } else {
                // A list header is the paragraph a list may begin with, and it
                // is not numbered and does not count.
                String::new()
            };

            let mut first = true;
            for (block_index, block) in item.elements_indexed() {
                self.path.push(block_index);
                if block.is(&Ns::Text, "list") {
                    self.list(ui, block, width, level + 1, Some(&style_name), counters);
                    self.path.pop();
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
                self.path.pop();
            }
            self.path.pop();
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
        for (index, element) in parent.elements_indexed() {
            self.path.push(index);
            if element.is(&Ns::Table, "table-row") {
                self.row(ui, element, columns);
            } else if element.is(&Ns::Table, "table-header-rows")
                || element.is(&Ns::Table, "table-rows")
                || element.is(&Ns::Table, "table-row-group")
            {
                self.rows(ui, element, columns);
            }
            self.path.pop();
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
            for (cell_index, cell) in row.elements_indexed() {
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
                for copy in 0..repeat {
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
                                    self.path.push(cell_index);
                                    let was_detached = self.detached;
                                    self.detached |= copy > 0;
                                    let palette = self.palette;
                                    if let Some(fill) = properties.cell.background {
                                        self.palette = palette.on(format::color32(fill));
                                    }
                                    ui.horizontal_top(|ui| {
                                        ui.add_space(padding);
                                        ui.vertical(|ui| {
                                            ui.set_max_width(content);
                                            self.blocks(ui, cell, content);
                                        });
                                    });
                                    self.palette = palette;
                                    self.detached = was_detached;
                                    self.path.pop();
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
            let mut palette = self.palette;
            if let Some(fill) = properties.cell.background {
                let fill = format::color32(fill);
                shapes.push(eframe::egui::Shape::rect_filled(cell_rect, 0.0, fill));
                palette = palette.on(fill);
            }
            for (from, to, stroke) in
                border_lines(cell_rect, &properties.cell.border, self.zoom, palette)
            {
                shapes.push(eframe::egui::Shape::line_segment([from, to], stroke));
            }
        }
        ui.painter().set(reserved, eframe::egui::Shape::Vec(shapes));
    }

    /// A frame: a box with a picture, a text box or an object in it.
    ///
    /// Public because a slide reaches it directly. Everywhere else a frame is
    /// found among a parent's children by [`Self::blocks`], but a shape on a
    /// slide *is* the frame, and asking `blocks` to draw it would look inside it
    /// for blocks and find a `draw:image`, which is not one.
    pub fn frame(&mut self, ui: &mut Ui, frame: &Element, width: f32) {
        let zoom = self.zoom;
        let declared = |local: &str| {
            frame
                .attr(&Ns::Svg, local)
                .and_then(odox_core::Length::parse)
                .map(|l| l.points() * zoom)
        };

        // A frame states its picture more than once where the producer had more
        // than one rendering of it — an SVG and then a PNG of the same drawing,
        // which is how the presentation templates carry their decorations — in
        // the producer's order of preference. The first one that decodes is the
        // answer, so a reader of two formats still draws a template that
        // prefers a third.
        let found = frame
            .elements()
            .filter(|child| child.is(&Ns::Draw, "image"))
            .find_map(|image| {
                let href = image.attr(&Ns::Xlink, "href")?.to_owned();
                self.pictures
                    .get(ui.ctx(), self.document, &href)
                    .map(|texture| (texture.id(), texture.size()))
            });
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

        // A text box draws the blocks inside it; anything else — an embedded
        // object, a chart, a formula — is a box the size the document asked for,
        // so that the page does not silently lose the space it occupied.
        if let Some((box_index, box_)) = frame
            .elements_indexed()
            .find(|(_, e)| e.is(&Ns::Draw, "text-box"))
        {
            let w = declared("width").unwrap_or(width).min(width);
            ui.allocate_ui_with_layout(
                vec2(w, 0.0),
                eframe::egui::Layout::top_down(Align::Min),
                |ui| {
                    ui.set_max_width(w);
                    eframe::egui::Frame::group(ui.style()).show(ui, |ui| {
                        self.path.push(box_index);
                        self.blocks(ui, box_, w - 16.0 * zoom);
                        self.path.pop();
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

fn paint_borders(
    ui: &Ui,
    rect: Rect,
    borders: &odox_core::Edges<Border>,
    zoom: f32,
    palette: format::Palette,
) {
    for (from, to, stroke) in border_lines(rect, borders, zoom, palette) {
        ui.painter().line_segment([from, to], stroke);
    }
}

/// The lines a box's borders are drawn as, a pure black one in the ink of
/// the ground it is on.
fn border_lines(
    rect: Rect,
    borders: &odox_core::Edges<Border>,
    zoom: f32,
    palette: format::Palette,
) -> impl Iterator<Item = (Pos2, Pos2, Stroke)> {
    edges(rect).into_iter().filter_map(move |(edge, from, to)| {
        let border = edge_of(borders, edge)?;
        let stroke = Stroke::new(
            (border.width.points() * zoom).max(1.0),
            format::ink_or(border.color, palette),
        );
        Some((from, to, stroke))
    })
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

/// Tell assistive technology what a paragraph is: its text, and a heading's
/// level. Where the page editor draws the paragraph it has already said the
/// text and where the caret is, and only the role is put right.
fn expose(
    ui: &Ui,
    response: &Response,
    element: &Element,
    galley: &Galley,
    origin: Pos2,
    edited: bool,
) {
    let heading = edit::heading_level(element);
    if !edited {
        let to_global = ui
            .ctx()
            .layer_transform_to_global(ui.layer_id())
            .unwrap_or_default();
        response.widget_info(|| {
            WidgetInfo::new(if heading.is_some() {
                Role::Heading
            } else {
                Role::Paragraph
            })
        });
        update_accesskit_for_text_widget(
            ui.ctx(),
            response.id,
            None,
            to_global * TSTransform::from_translation(origin.to_vec2()),
            galley,
        );
    }
    if let Some(level) = heading {
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_role(Role::Heading);
            node.set_level(usize::from(level));
        });
    }
}

/// Whether a paragraph is what an internal link names: a bookmark in it, by
/// name, or a heading by its text, which `LibreOffice` writes `Name|outline`.
fn holds_place(element: &Element, place: &str) -> bool {
    if let Some(name) = place.strip_suffix("|outline") {
        return element.is(&Ns::Text, "h") && element.plain_text().trim() == name.trim();
    }
    element.elements().any(|e| {
        let marks = e.is(&Ns::Text, "bookmark") || e.is(&Ns::Text, "bookmark-start");
        (marks && e.attr(&Ns::Text, "name") == Some(place)) || holds_place(e, place)
    })
}

/// The gap a tab with a leader leaves, from where the tab begins to where the
/// text after it does, on the row whose top is `top`, in galley coordinates.
struct Leader {
    fill: char,
    from: f32,
    to: f32,
    top: f32,
}

/// The default tab distance where no style gives one: 1.25 cm.
const TAB_DISTANCE: f32 = 1.25 / 2.54 * 72.0;

/// Give each tab in a paragraph the room that takes the text after it to its
/// stop: the next stop the paragraph sets past where the tab stands, or the
/// next of the evenly spaced default ones. A left stop starts the text there,
/// a right one ends it there, a centre one centres it and a decimal one puts
/// its decimal character there. Each tab is measured on the paragraph laid out
/// with the tabs before it already placed. Stops count from the left margin,
/// and the galley starts `offset` right of it. Answers the gaps to fill with
/// leaders.
fn place_tabs(
    ui: &Ui,
    job: &mut ParagraphJob,
    paragraph: &odox_core::ParagraphProperties,
    offset: f32,
    zoom: f32,
) -> Vec<Leader> {
    let mut leaders = Vec::new();
    let stops = paragraph.tab_stops.clone().unwrap_or_default();
    let distance = paragraph
        .tab_distance
        .map_or(TAB_DISTANCE, odox_core::Length::points)
        * zoom;
    for tab in 0..job.tab_count() {
        let Some(start) = job.tab_char(tab) else {
            continue;
        };
        job.set_tab_advance(tab, 0.0);
        let galley = ui
            .ctx()
            .fonts_mut(|fonts| fonts.layout_job(job.layout_job().clone()));
        let at = |index: usize| galley.pos_from_cursor(CCursor::new(index));
        let here = at(start);
        let (x, top) = (here.min.x, here.min.y);
        let space = at(start + 1).min.x - x;
        let text: Vec<char> = galley.text().chars().collect();
        let end = job
            .tab_char(tab + 1)
            .filter(|&next| (at(next).min.y - top).abs() < 0.5)
            .unwrap_or_else(|| {
                (start + 1..text.len())
                    .find(|&i| text[i] == '\n' || (at(i).min.y - top).abs() > 0.5)
                    .unwrap_or(text.len())
            });
        let after = x + space;
        let segment = at(end).min.x.max(after) - after;
        let from_margin = x + space + offset;
        let stop = stops
            .iter()
            .find(|stop| stop.position.points() * zoom > from_margin + 0.5);
        let (target, kind, leader) = if let Some(stop) = stop {
            (
                stop.position.points() * zoom - offset,
                stop.kind,
                stop.leader,
            )
        } else {
            let next = ((from_margin / distance).floor() + 1.0) * distance;
            (next - offset, TabKind::Left, None)
        };
        let reach = match kind {
            TabKind::Left => 0.0,
            TabKind::Right => segment,
            TabKind::Center => segment / 2.0,
            TabKind::Char(mark) => (start + 1..end)
                .find(|&i| text[i] == mark)
                .map_or(segment, |i| at(i).min.x - after),
        };
        let room = (target - reach - after).max(0.0);
        job.set_tab_advance(tab, room);
        if let Some(fill) = leader.filter(|_| room > 0.0) {
            leaders.push(Leader {
                fill,
                from: after,
                to: after + room,
                top,
            });
        }
    }
    leaders
}

/// Fill the gaps tabs with leaders leave, in the paragraph's own font.
fn paint_leaders(ui: &Ui, leaders: &[Leader], origin: Pos2, format: &TextFormat) {
    for leader in leaders {
        let one = leader.fill.to_string();
        let width = ui.ctx().fonts_mut(|fonts| {
            fonts
                .layout_no_wrap(one.clone(), format.font_id.clone(), format.color)
                .size()
                .x
        });
        if width <= 0.0 {
            continue;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = ((leader.to - leader.from) / width).floor().max(0.0) as usize;
        if count == 0 {
            continue;
        }
        let filled = one.repeat(count);
        #[allow(clippy::cast_precision_loss)]
        let used = width * count as f32;
        ui.painter().text(
            origin + vec2(leader.to - used, leader.top),
            Align2::LEFT_TOP,
            filled,
            format.font_id.clone(),
            format.color,
        );
    }
}

/// A paragraph laid out with each of its lines in the order it reads, as
/// the Unicode bidirectional algorithm puts it, every line a line of its own.
///
/// egui shapes each section on its own and turns one whose script reads right
/// to left round, so a stretch read right to left is given in pieces in the
/// order they are drawn: a word in the order it is written, which the shaper
/// turns, and the spaces and marks between words turned here, a bracket facing
/// the other way. Where the lines break is worked out here too: egui breaks
/// a line by where its glyphs stand, and a word turned round puts them out of
/// order.
pub fn in_visual_order(ui: &Ui, written: &LayoutJob, right_to_left: bool) -> Arc<Galley> {
    let text = written.text.as_str();
    let paragraph = bidi::Paragraph::new(text, right_to_left);
    let mut job = LayoutJob {
        wrap: eframe::egui::text::TextWrapping {
            max_width: f32::INFINITY,
            ..written.wrap.clone()
        },
        halign: written.halign,
        first_row_min_height: written.first_row_min_height,
        ..LayoutJob::default()
    };
    let format_at = |at: usize| {
        written
            .sections
            .iter()
            .find(|section| section.byte_range.as_usize().contains(&at))
            .or(written.sections.last())
            .map(|section| section.format.clone())
            .unwrap_or_default()
    };
    for (index, line) in lines(ui, written).into_iter().enumerate() {
        if index > 0 {
            section(&mut job, "\n", format_at(line.start));
        }
        for run in paragraph.line(line) {
            if run.right_to_left {
                for piece in pieces(text, run.range).into_iter().rev() {
                    append(&mut job, written, piece, true);
                }
            } else {
                append(&mut job, written, run.range, false);
            }
        }
    }
    if let (Some(lead), Some(first)) = (written.sections.first(), job.sections.first_mut()) {
        first.leading_space = lead.leading_space;
    }
    ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
}

/// Where a paragraph's lines break, as the bytes of each: after a space,
/// where the next word no longer fits the width the paragraph wraps to, and at
/// each line break written in it. A word wider than the line has one to itself.
fn lines(ui: &Ui, written: &LayoutJob) -> Vec<std::ops::Range<usize>> {
    let text = written.text.as_str();
    let width = |range: std::ops::Range<usize>| {
        if range.is_empty() {
            return 0.0;
        }
        let mut job = LayoutJob::default();
        for piece in pieces(text, range) {
            append(&mut job, written, piece, false);
        }
        ui.ctx()
            .fonts_mut(|fonts| fonts.layout_job(job))
            .rect
            .width()
    };
    let most = written.wrap.max_width;
    let lead = written.sections.first().map_or(0.0, |s| s.leading_space);
    let mut lines = Vec::new();
    let mut start = 0;
    for hard in text.split_inclusive('\n') {
        let end = start + hard.len();
        let body = start..end - usize::from(hard.ends_with('\n'));
        let mut line = body.start;
        let mut used = if lines.is_empty() { lead } else { 0.0 };
        let mut at = body.start;
        while at < body.end {
            let word_end = text[at..body.end]
                .find(char::is_whitespace)
                .map_or(body.end, |offset| at + offset);
            let space_end = text[word_end..body.end]
                .find(|c: char| !c.is_whitespace())
                .map_or(body.end, |offset| word_end + offset);
            let word = width(at..word_end);
            if at > line && used + word > most {
                lines.push(line..line + text[line..at].trim_end().len());
                line = at;
                used = 0.0;
            }
            used += word + width(word_end..space_end);
            at = space_end;
        }
        lines.push(line..body.end);
        start = end;
    }
    lines
}

/// A stretch that reads right to left, cut into its words and what lies
/// between them, in the order they are written.
fn pieces(text: &str, range: std::ops::Range<usize>) -> Vec<std::ops::Range<usize>> {
    let worded = |character: char| bidi::is_right_to_left(character) || bidi::is_mark(character);
    let mut pieces: Vec<(std::ops::Range<usize>, bool)> = Vec::new();
    for (offset, character) in text[range.clone()].char_indices() {
        let at = range.start + offset;
        let end = at + character.len_utf8();
        let kind = worded(character);
        match pieces.last_mut() {
            Some((piece, last)) if *last == kind => piece.end = end,
            _ => pieces.push((at..end, kind)),
        }
    }
    pieces.into_iter().map(|(piece, _)| piece).collect()
}

/// Append a stretch of a paragraph's written text to a job, in the formats
/// its sections give it: in the order it is written, or turned round and its
/// brackets mirrored where it reads right to left and is not a word the shaper
/// turns itself.
fn append(job: &mut LayoutJob, written: &LayoutJob, range: std::ops::Range<usize>, turned: bool) {
    let text = written.text.as_str();
    let mut parts: Vec<(std::ops::Range<usize>, &TextFormat)> = written
        .sections
        .iter()
        .filter_map(|section| {
            let own = section.byte_range.as_usize();
            let start = own.start.max(range.start);
            let end = own.end.min(range.end);
            (start < end).then_some((start..end, &section.format))
        })
        .collect();
    let word = text[range.clone()].chars().any(bidi::is_right_to_left);
    if turned {
        parts.reverse();
    }
    for (part, format) in parts {
        let piece = &text[part];
        if turned && !word {
            let mirrored: String = piece.chars().rev().map(bidi::mirrored).collect();
            section(job, &mirrored, format.clone());
        } else {
            section(job, piece, format.clone());
        }
    }
}

/// Add text to a job as a section of its own, which egui shapes on its own:
/// `LayoutJob::append` runs it into the section before when the format is the
/// same, and the two would be turned round as one.
fn section(job: &mut LayoutJob, text: &str, format: TextFormat) {
    let start = job.text.len();
    job.text.push_str(text);
    job.sections.push(eframe::egui::text::LayoutSection {
        leading_space: 0.0,
        byte_range: eframe::egui::epaint::text::ByteIndex(start)
            ..eframe::egui::epaint::text::ByteIndex(job.text.len()),
        format,
    });
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Flow, Pictures};
    use crate::format::{self, DEFAULT_SIZE};
    use odox_core::{Document, Element, Family, edit, media_type};

    const ANY: &[&str] = &[
        media_type::TEXT,
        media_type::TEXT_WEB,
        media_type::SPREADSHEET,
        media_type::PRESENTATION,
    ];

    fn documents(directory: &Path, into: &mut Vec<Document>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
            if path.is_dir() {
                documents(&path, into);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("odt" | "ods" | "odp")
            ) {
                let bytes = std::fs::read(&path).expect("a corpus document reads");
                into.push(Document::read(&bytes, ANY).expect("a corpus document parses"));
            }
        }
    }

    fn paragraphs<'a>(element: &'a Element, into: &mut Vec<&'a Element>) {
        for child in element.elements() {
            if edit::is_paragraph(child) {
                into.push(child);
            }
            paragraphs(child, into);
        }
    }

    /// The caret is placed in what is drawn and an edit changes what the tree
    /// holds, so for every paragraph in the corpus the two have to agree on
    /// how many characters there are.
    #[test]
    fn what_is_drawn_maps_onto_the_text_an_edit_changes() {
        let mut corpus = Vec::new();
        documents(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"),
            &mut corpus,
        );
        assert!(!corpus.is_empty(), "the corpus is where it was");
        let mut checked = 0;
        for document in &corpus {
            let mut pictures = Pictures::default();
            let flow = Flow::new(document, &mut pictures, 1.0);
            let mut found = Vec::new();
            paragraphs(&document.content, &mut found);
            for paragraph in found {
                let properties = flow.style_of(paragraph, &Family::Paragraph);
                let base = format::text_format(&properties.text, DEFAULT_SIZE, 1.0, flow.palette);
                let size = format::size_of(&properties.text, DEFAULT_SIZE);
                let (job, _) = flow.layout_job(paragraph, &properties, &base, size, 400.0);
                let (_, map) = job.into_parts();
                let text = edit::text(paragraph);
                assert_eq!(map.model_len(), text.chars().count(), "{text:?}");
                checked += 1;
            }
        }
        assert!(checked > 0);
    }
}
