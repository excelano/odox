//! The window's one view: a document's flow, and its headings beside it.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use eframe::egui::scroll_area::ScrollAreaOutput;
use eframe::egui::{self, Pos2, Rect, Ui};
use egui_richedit::{Model, Selection};
use odox_core::Document;
use odox_core::doc::{PageSetup, TextDocument};
use odox_core::words::Count;
use odox_ui::find::{Match, Options, Replaced, in_paragraphs};
use odox_ui::i18n::{fill, t};
use odox_ui::link_dialog::LinkQuestion;
use odox_ui::note_popup::NoteResponse;
use odox_ui::page_setup::PageSetupQuestion;
use odox_ui::picture_question::PictureQuestion;
use odox_ui::toolbar::{self, Command, MenuState};
use odox_ui::{
    Caret, Editing, Export, Flow, FlowModel, Found, Notice, PageEditor, Pictures, View, Zoom,
    count, fonts, marks, page_editor,
};

const MARGIN: f32 = 24.0;

/// How much of the window a page up or down moves, keeping the rest in sight
/// so the line last read is still there after the move.
const PAGE: f32 = 0.9;

#[derive(Default)]
struct Scroll {
    shown: Option<Shown>,
    settle: Option<f32>,
}

#[derive(Clone, Copy)]
struct Shown {
    scale: f32,
    offset: f32,
    viewport: Rect,
    height: f32,
}

#[derive(Clone, Copy)]
enum Move {
    Page { down: bool },
    End { bottom: bool },
}

#[derive(Clone, Copy)]
struct Anchor {
    fraction: f32,
    at: f32,
}

impl Scroll {
    fn area(&mut self, ui: &Ui, scale: f32) -> (egui::ScrollArea, Option<Anchor>) {
        let area = egui::ScrollArea::vertical().auto_shrink([false, false]);
        let rescaled = self
            .shown
            .filter(|shown| (shown.scale - scale).abs() > f32::EPSILON);
        if let Some(shown) = rescaled {
            self.settle = None;
            let anchor = shown.anchor(ui.input(|input| input.pointer.hover_pos()));
            let estimate = shown.height * scale / shown.scale;
            return (
                area.vertical_scroll_offset(anchor.offset(estimate)),
                Some(anchor),
            );
        }
        match self.settle.take() {
            Some(offset) => (area.vertical_scroll_offset(offset), None),
            None => (area, None),
        }
    }

    fn record(
        &mut self,
        ui: &Ui,
        output: &ScrollAreaOutput<()>,
        scale: f32,
        anchor: Option<Anchor>,
    ) {
        let drawn = output.state.clamped_offset().y;
        let height = output.content_size.y;
        let offset = anchor.map_or(drawn, |anchor| anchor.offset(height));
        if (offset - drawn).abs() > 0.5 {
            self.settle = Some(offset);
            ui.ctx().request_repaint();
        }
        self.shown = Some(Shown {
            scale,
            offset,
            viewport: output.inner_rect,
            height,
        });
    }

    /// How much text there is, under the page, and the paging keys when
    /// `keys` says the caret is not the one taking them.
    fn paging(&mut self, ui: &mut Ui, keys: bool, counted: &Counted) {
        let mut moved = None;
        if keys {
            ui.input_mut(|input| {
                if input.consume_key(egui::Modifiers::NONE, egui::Key::PageUp) {
                    moved = Some(Move::Page { down: false });
                }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::PageDown) {
                    moved = Some(Move::Page { down: true });
                }
                for modifiers in [egui::Modifiers::NONE, egui::Modifiers::COMMAND] {
                    if input.consume_key(modifiers, egui::Key::Home) {
                        moved = Some(Move::End { bottom: false });
                    }
                    if input.consume_key(modifiers, egui::Key::End) {
                        moved = Some(Move::End { bottom: true });
                    }
                }
            });
        }
        egui::Panel::bottom("pages").show(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                counted.show(ui);
            });
        });
        if let Some(moved) = moved {
            self.go(ui.ctx(), moved);
        }
    }

    /// Move the view, settled on the next frame.
    fn go(&mut self, ctx: &egui::Context, moved: Move) {
        let Some(shown) = self.shown else {
            return;
        };
        let step = shown.viewport.height() * PAGE;
        let offset = match moved {
            Move::Page { down: true } => shown.offset + step,
            Move::Page { down: false } => shown.offset - step,
            Move::End { bottom: true } => shown.bottom(),
            Move::End { bottom: false } => 0.0,
        };
        self.settle = Some(offset.clamp(0.0, shown.bottom()));
        ctx.request_repaint();
    }
}

/// How much text the document holds, and the selection when there is one,
/// counted again only when the text or the selection changes.
#[derive(Default)]
struct Counted {
    of: Option<(u64, Option<Selection<Vec<usize>>>)>,
    whole: Count,
    selected: Option<Count>,
}

impl Counted {
    fn update(
        &mut self,
        body: &odox_core::Element,
        revision: u64,
        selection: Option<&Selection<Vec<usize>>>,
    ) {
        let selection = selection.filter(|s| s.anchor != s.focus).cloned();
        let of = Some((revision, selection));
        if self.of == of {
            return;
        }
        let selection = of.as_ref().and_then(|(_, s)| s.as_ref());
        if self.of.as_ref().is_none_or(|(r, _)| *r != revision) {
            self.whole = count::in_paragraphs(body);
        }
        self.selected = selection.map(|s| {
            count::between(
                body,
                (&s.anchor.paragraph, s.anchor.offset),
                (&s.focus.paragraph, s.focus.offset),
            )
        });
        self.of = of;
    }

    fn show(&self, ui: &mut Ui) {
        let (words, characters) = match self.selected {
            Some(selected) => (
                fill(
                    t("Words: {selected} of {total}"),
                    &[
                        ("selected", &selected.words.to_string()),
                        ("total", &self.whole.words.to_string()),
                    ],
                ),
                fill(
                    t("Characters: {selected} of {total}"),
                    &[
                        ("selected", &selected.characters.to_string()),
                        ("total", &self.whole.characters.to_string()),
                    ],
                ),
            ),
            None => (
                fill(
                    t("Words: {words}"),
                    &[("words", &self.whole.words.to_string())],
                ),
                fill(
                    t("Characters: {characters}"),
                    &[("characters", &self.whole.characters.to_string())],
                ),
            ),
        };
        ui.label(characters);
        ui.label(words);
    }
}

impl Shown {
    fn bottom(&self) -> f32 {
        (self.height - self.viewport.height()).max(0.0)
    }

    fn anchor(&self, pointer: Option<Pos2>) -> Anchor {
        let at = pointer
            .filter(|at| self.viewport.contains(*at))
            .map_or(0.0, |at| at.y - self.viewport.top());
        Anchor {
            fraction: (self.offset + at) / self.height.max(1.0),
            at,
        }
    }
}

impl Anchor {
    fn offset(self, height: f32) -> f32 {
        (self.fraction * height - self.at).max(0.0)
    }
}

/// A text document, open or not.
pub struct TextView {
    document: Option<TextDocument>,
    pictures: Pictures,
    /// The heading the outline asked to be shown, answered on the next frame
    /// while the body is laid out.
    scroll_to: Option<usize>,
    /// The bookmark or heading a link inside the document asked for, answered
    /// on the next frame as `scroll_to` is.
    place: Option<String>,
    /// The caret, in edit mode.
    page: PageEditor,
    /// What a search found.
    found: Found,
    /// What the Format menu draws itself from, and what it last chose.
    menu: MenuState,
    command: Option<Command>,
    /// A blank document was opened, and the caret is to go into its first
    /// paragraph when the page is next drawn.
    begin: bool,
    export: Export,
    scroll: Scroll,
    counted: Counted,
    /// The size asked of a table to put in, while the question is up.
    table_size: Option<(usize, usize)>,
    /// The size the question was answered with, put in on the next frame.
    table_wanted: Option<(usize, usize)>,
    /// What the page asked for as it was drawn, done on the next frame.
    wanted: Wanted,
    /// What a chosen picture shows, while the question is up.
    picture_question: Option<PictureQuestion>,
    /// The picture the question was answered with, put in on the next frame.
    picture_wanted: Option<odox_ui::Picture>,
    /// The page's paper, orientation and margins, while the question is up.
    page_question: Option<PageSetupQuestion>,
    /// What the question was answered with, made on the next frame.
    page_wanted: Option<PageSetup>,
    /// The link question.
    link: LinkQuestion,
    /// The part of the page being typed into.
    region: Region,
    /// The header, or with true the footer, asked about before it and its
    /// text are taken away.
    band_question: Option<bool>,
    notice: Option<Notice>,
}

impl Default for TextView {
    fn default() -> Self {
        Self {
            document: None,
            pictures: Pictures::default(),
            scroll_to: None,
            place: None,
            page: page_editor(),
            found: Found::default(),
            menu: MenuState::offering_blocks(true),
            command: None,
            begin: false,
            export: Export::default(),
            scroll: Scroll::default(),
            counted: Counted::default(),
            table_size: None,
            picture_question: None,
            picture_wanted: None,
            link: LinkQuestion::default(),
            region: Region::Body,
            band_question: None,
            page_question: None,
            page_wanted: None,
            table_wanted: None,
            wanted: Wanted::default(),
            notice: None,
        }
    }
}

impl View for TextView {
    fn open(&mut self, ctx: &egui::Context, bytes: &[u8], path: &Path) -> Result<(), String> {
        let document = TextDocument::read(bytes).map_err(|e| {
            fill(
                t("{file} could not be opened: {reason}"),
                &[
                    (
                        "file",
                        &path.file_name().unwrap_or_default().to_string_lossy(),
                    ),
                    ("reason", &e.to_string()),
                ],
            )
        })?;
        self.pictures.clear();
        self.document = Some(document);
        self.region = Region::Body;
        self.scroll_to = None;
        self.place = None;
        self.page.clear();
        self.found.clear();
        self.counted = Counted::default();
        self.load_fonts(ctx);
        Ok(())
    }

    fn blank(&self, language: &str) -> Vec<u8> {
        odox_core::blank::text(language)
    }

    fn begin(&mut self) {
        self.begin = true;
    }

    fn close(&mut self) {
        self.document = None;
        self.region = Region::Body;
        self.export = Export::default();
        self.pictures.clear();
        self.page.clear();
        self.found.clear();
        self.scroll = Scroll::default();
        self.counted = Counted::default();
    }

    fn is_open(&self) -> bool {
        self.document.is_some()
    }

    fn title(&self) -> Option<String> {
        self.document.as_ref()?.document.meta.title.clone()
    }

    fn document_mut(&mut self) -> Option<&mut Document> {
        Some(&mut self.document.as_mut()?.document)
    }

    fn reindex(&mut self) {
        self.page.document_replaced();
    }

    fn caret(&self) -> Option<Caret> {
        let focus = self.page.selection()?.focus.clone();
        Some(Caret::new(
            [self.region.tag().as_slice(), &focus.paragraph].concat(),
            focus.offset,
        ))
    }

    fn restore_caret(&mut self, caret: Caret) {
        let (region, paragraph) = Region::of(&caret.paragraph);
        if region != self.region {
            self.region = region;
            self.page.clear();
        }
        self.page.select(Selection::caret(Caret::new(
            paragraph.to_vec(),
            caret.offset,
        )));
    }

    fn find(&mut self, query: &str, options: Options) -> usize {
        let matches = self
            .document
            .as_ref()
            .and_then(TextDocument::body)
            .map(|body| in_paragraphs(body, 0, query, options))
            .unwrap_or_default();
        self.found.set(matches);
        self.found.len()
    }

    fn show_match(&mut self, index: usize) {
        self.found.show(index);
    }

    fn menus(&mut self, ui: &mut Ui, _editing: &Editing) {
        ui.menu_button(t("Format"), |ui| {
            if let Some(command) = toolbar::menu(ui, &self.menu) {
                self.command = Some(command);
            }
        });
    }

    fn can_replace(&self, editing: &Editing) -> bool {
        editing.on && self.document.is_some()
    }

    fn replace(&mut self, with: &str, all: bool, editing: &mut Editing) -> Replaced {
        let matches: Vec<Match> = if all {
            self.found.all().to_vec()
        } else {
            self.found.current_match().cloned().into_iter().collect()
        };
        let Some(document) = &mut self.document else {
            return Replaced::default();
        };
        let Some(root) = document.body_path() else {
            return Replaced::default();
        };
        let Document {
            content, styles, ..
        } = &mut document.document;
        let mut model = FlowModel::new(content, styles, root, editing);
        model.replace_matches(&matches.iter().collect::<Vec<_>>(), with, true)
    }

    #[allow(clippy::too_many_lines)]
    fn central(&mut self, ui: &mut Ui, zoom: &mut Zoom, editing: &mut Editing) {
        if let Some(document) = &mut self.document {
            self.export.show(ui.ctx(), document, editing);
        }
        // While a question is up the page is drawn as it is and takes no keys;
        // the caret waits for it to be answered.
        let asked = self.table_size.is_some()
            || self.picture_question.is_some()
            || self.page_question.is_some()
            || self.link.is_up()
            || self.band_question.is_some();
        self.link.ask(ui.ctx());
        self.ask_band(ui.ctx(), editing);
        self.ask_table_size(ui.ctx());
        self.ask_picture(ui.ctx());
        self.ask_page_setup(ui.ctx());
        self.set_page(editing);
        let edit_mode = editing.on && !editing.asking && !self.export.asking() && !asked;
        if !editing.on {
            self.page.clear();
        }
        self.edit_toolbar(ui, editing, edit_mode);
        let Some(document) = &mut self.document else {
            return;
        };
        if document.body().is_none() {
            ui.centered_and_justified(|ui| {
                ui.weak(t("This document has no text in it."));
            });
            return;
        }

        // The page's text width, which is what the document's own lines were
        // written to. Narrower than the window, the flow is drawn on a page the
        // width the document asked for; wider, the page shrinks to fit rather
        // than being cut off.
        zoom.fit(ui.available_width() / (document.text_width() + MARGIN * 2.0));
        let scale = zoom.scale();
        let page = document.text_width() * scale;
        let page_height = document.text_height() * scale;
        let margin = MARGIN * scale;

        // The caret's own Page Up and Page Down move it, and the view with it.
        let keys = !editing.asking
            && !self.export.asking()
            && (!edit_mode || self.page.selection().is_none());
        if let Some(body) = document.body() {
            let selection = (self.region == Region::Body)
                .then(|| self.page.selection())
                .flatten();
            self.counted.update(body, editing.revision(), selection);
        }
        self.scroll.paging(ui, keys, &self.counted);

        changes_bar(ui, &document.document.content, edit_mode, &mut self.wanted);
        let (area, anchor) = self.scroll.area(ui, scale);
        let output = area.show(ui, |ui| {
            // What was typed goes into the tree before it is laid out, so
            // this frame already draws it.
            let region = self.region;
            if edit_mode && let Some((mut model, _)) = region_model(document, region, editing) {
                self.wanted.correct(&mut model);
                if region == Region::Body {
                    self.wanted
                        .apply(&mut self.page, &mut model, &mut self.notice);
                }
                tab_keys(ui, &mut self.page, &mut model, &mut self.notice);
                if region == Region::Body {
                    if let Some(refused) = break_keys(ui, &mut self.page, &mut model) {
                        self.notice = Some(Notice::Problem(refused));
                    }
                } else {
                    swallow_body_keys(ui);
                }
                self.page.input(ui, &mut model);
            }
            let Some(body) = document.body() else {
                return;
            };
            ui.vertical_centered(|ui| {
                let width = page.min(ui.available_width() - margin * 2.0).max(80.0);
                let palette = odox_ui::format::Palette::for_theme(ui.visuals().dark_mode);
                egui::Frame::new()
                    .fill(palette.paper)
                    .inner_margin(margin)
                    .show(ui, |ui| {
                        ui.set_width(width);
                        let gap = (ui.spacing().item_spacing.y * scale).round();
                        ui.spacing_mut().item_spacing.y = gap;
                        // At least one page tall, however little is on
                        // it, shrunk with the page when the window is
                        // narrower.
                        ui.set_min_height(page_height * width / page);
                        let master = document.master_page();
                        let band = |footer: bool| {
                            master
                                .and_then(|m| {
                                    m.child(
                                        &odox_core::Ns::Style,
                                        if footer { "footer" } else { "header" },
                                    )
                                })
                                .filter(|b| {
                                    b.attr(&odox_core::Ns::Style, "display") != Some("false")
                                })
                        };
                        let active = edit_mode.then_some(self.region);
                        let mut wanted = None;
                        if let Some(header) = band(false) {
                            let page = (active == Some(Region::Header)).then_some(&mut self.page);
                            let clickable = active.is_some_and(|region| region != Region::Header);
                            let (clicked, corrected) = band_drawn(
                                ui,
                                &document.document,
                                &mut self.pictures,
                                (header, page),
                                palette,
                                (scale, width),
                                clickable,
                            );
                            if clicked {
                                wanted = Some(Region::Header);
                            }
                            if corrected.is_some() {
                                self.wanted.correction = corrected;
                                ui.ctx().request_repaint();
                            }
                            rule(ui, palette);
                        }
                        let body_rect = ui
                            .scope(|ui| {
                                let mut flow =
                                    Flow::new(&document.document, &mut self.pictures, scale);
                                flow.palette = palette;
                                flow.scroll_to_heading = self.scroll_to.take();
                                flow.scroll_to_place = self.place.take();
                                flow.page =
                                    (active == Some(Region::Body)).then_some(&mut self.page);
                                flow.find = self.found.highlights(0);
                                flow.marks = marks::shown(ui.ctx());
                                flow.blocks(ui, body, width);
                                if self.wanted.take(&mut flow) {
                                    ui.ctx().request_repaint();
                                }
                                if flow.followed.is_some() {
                                    self.place = flow.followed.take();
                                    ui.ctx().request_repaint();
                                }
                            })
                            .response
                            .rect;
                        if active.is_some_and(|region| region != Region::Body)
                            && ui
                                .interact(
                                    body_rect,
                                    egui::Id::unique("body-region"),
                                    egui::Sense::click(),
                                )
                                .clicked()
                        {
                            wanted = Some(Region::Body);
                        }
                        if let Some(footer) = band(true) {
                            rule(ui, palette);
                            let page = (active == Some(Region::Footer)).then_some(&mut self.page);
                            let clickable = active.is_some_and(|region| region != Region::Footer);
                            let (clicked, corrected) = band_drawn(
                                ui,
                                &document.document,
                                &mut self.pictures,
                                (footer, page),
                                palette,
                                (scale, width),
                                clickable,
                            );
                            if clicked {
                                wanted = Some(Region::Footer);
                            }
                            if corrected.is_some() {
                                self.wanted.correction = corrected;
                                ui.ctx().request_repaint();
                            }
                        }
                        if let Some(region) = wanted {
                            self.region = region;
                            self.page.clear();
                            self.begin = true;
                            ui.ctx().request_repaint();
                        }
                    });
            });
        });
        self.scroll.record(ui, &output, scale, anchor);
        self.found.drawn();
    }

    fn side_at_start(&self) -> bool {
        false
    }

    fn fits(&self) -> bool {
        true
    }

    fn has_formatting_marks(&self) -> bool {
        self.document.is_some()
    }

    fn has_side(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(|document| !document.outline().is_empty())
    }

    fn side(&mut self, ui: &mut Ui) {
        let Some(document) = &self.document else {
            return;
        };
        let outline = document.outline();
        ui.heading(t("Outline"));
        ui.separator();
        for (index, heading) in outline.iter().enumerate() {
            // A heading's level is its indent, which is how an outline reads as
            // the shape of the document rather than as a list of its headings.
            let indent = f32::from(heading.level.saturating_sub(1)) * 10.0;
            ui.horizontal(|ui| {
                ui.add_space(indent);
                let label = if heading.text.trim().is_empty() {
                    t("(untitled)").to_owned()
                } else {
                    heading.text.clone()
                };
                if ui
                    .add(
                        egui::Label::new(label)
                            .truncate()
                            .sense(egui::Sense::click()),
                    )
                    .clicked()
                {
                    self.scroll_to = Some(index);
                }
            });
        }
    }

    fn print(&mut self, path: Option<&Path>) {
        self.export.print(path);
    }

    fn file_menu(&mut self, ui: &mut Ui, path: Option<&Path>) {
        ui.separator();
        self.export.menu(ui, path);
    }

    fn take_notice(&mut self) -> Option<Notice> {
        self.notice.take().or_else(|| self.export.take_notice())
    }

    fn view_menu(&mut self, ui: &mut Ui) {
        let Some(document) = &self.document else {
            return;
        };
        ui.separator();
        if ui.button(t("Copy the document as text")).clicked() {
            if let Some(body) = document.body() {
                ui.ctx().copy_text(body.plain_text());
            }
            ui.close();
        }
    }
}

impl TextView {
    /// Ask how many rows and columns a table is to have, while the question
    /// is up, and keep the answer for the page to put in.
    fn ask_table_size(&mut self, ctx: &egui::Context) {
        let Some((rows, columns)) = &mut self.table_size else {
            return;
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("insert-table")).show(ctx, |ui| {
            ui.heading(t("Insert a table"));
            ui.add_space(6.0);
            egui::Grid::new(egui::Id::unique("table-size")).show(ui, |ui| {
                ui.label(t("Rows"));
                ui.add(egui::DragValue::new(rows).range(1..=200));
                ui.end_row();
                ui.label(t("Columns"));
                ui.add(egui::DragValue::new(columns).range(1..=20));
                ui.end_row();
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(t("Insert")).clicked()
                    || ui.input(|input| input.key_pressed(egui::Key::Enter))
                {
                    answer = Some(true);
                }
                if ui.button(t("Cancel")).clicked()
                    || ui.input(|input| input.key_pressed(egui::Key::Escape))
                {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                self.table_wanted = Some((*rows, *columns));
                self.table_size = None;
                ctx.request_repaint();
            }
            Some(false) => self.table_size = None,
            None => {}
        }
    }

    /// The toolbar over the page in edit mode, and what it and the Format menu
    /// asked for: carried out here, or asked about first.
    fn edit_toolbar(&mut self, ui: &mut Ui, editing: &mut Editing, edit_mode: bool) {
        let command = if edit_mode { self.command.take() } else { None };
        let body_only = matches!(
            command,
            Some(Command::Comment | Command::Note(_) | Command::PageBreak(_))
        );
        let command = if body_only && self.region != Region::Body {
            None
        } else {
            command
        };
        if let Some(Command::Band { footer, on }) = command {
            self.band(footer, on, editing);
        }
        let region = self.region;
        let Some(document) = self.document.as_mut().filter(|_| edit_mode) else {
            self.command = None;
            self.menu = MenuState::offering_blocks(true);
            return;
        };
        let bands = (
            document.band_path(false).is_some(),
            document.band_path(true).is_some(),
        );
        let text_width = document.text_width();
        if command == Some(Command::PageSetup) {
            self.page_question = Some(PageSetupQuestion::new(document.page_setup()));
        }
        let Some((mut model, package)) = region_model(document, region, editing) else {
            self.region = Region::Body;
            self.page.clear();
            self.menu = MenuState::offering_blocks(true);
            return;
        };
        if std::mem::take(&mut self.begin)
            && let Some(first) = model.first()
        {
            self.page
                .select(Selection::caret(egui_richedit::Position::new(first, 0)));
        }
        let asked = toolbar_commands(
            ui,
            &mut self.page,
            &mut model,
            command,
            self.table_wanted.take(),
            text_width,
        );
        if asked.size {
            self.table_size = Some((2, 3));
        }
        if asked.picture {
            match PictureQuestion::choose(ui.ctx(), text_width) {
                Ok(question) => self.picture_question = question,
                Err(problem) => self.notice = Some(Notice::Problem(problem)),
            }
        }
        if let Some(picture) = self.picture_wanted.take()
            && let Err(refused) = model.insert_picture(&mut self.page, package, &picture)
        {
            self.notice = Some(Notice::Problem(odox_ui::refusal::message(&refused)));
        }
        if let Some(problem) = self.link.act(
            ui,
            &mut model,
            &mut self.page,
            command == Some(Command::Link),
            FlowModel::places,
        ) {
            self.notice = Some(Notice::Problem(problem));
        }
        if asked.notice.is_some() {
            self.notice = asked.notice;
        }
        self.menu = MenuState::of(&self.page, &model, true)
            .with_bands(bands.0, bands.1)
            .in_band(region != Region::Body);
        ui.separator();
    }

    /// Give the page a header or a footer, or take it away, as one step to
    /// undo; one with text in it is asked about first.
    fn band(&mut self, footer: bool, on: bool, editing: &mut Editing) {
        let Some(document) = &mut self.document else {
            return;
        };
        let holds_anything = document
            .band_path(footer)
            .and_then(|path| document.document.styles_part.as_ref()?.at(&path).cloned())
            .is_some_and(|band| {
                let mut children = band.elements();
                let empty_paragraph = children.next().is_some_and(|first| {
                    first.is(&odox_core::Ns::Text, "p")
                        && first.elements().next().is_none()
                        && first.plain_text().trim().is_empty()
                });
                !(empty_paragraph && children.next().is_none())
            });
        if !on && holds_anything && self.band_question.is_none() {
            self.band_question = Some(footer);
            return;
        }
        self.band_question = None;
        let styles = document.document.styles_part.clone();
        let changed = if footer {
            document.set_footer(on)
        } else {
            document.set_header(on)
        };
        match changed {
            Ok(true) => {
                editing.record_styles_part(styles, odox_core::Kept::default(), None);
                let removed = if footer {
                    Region::Footer
                } else {
                    Region::Header
                };
                if !on && self.region == removed {
                    self.region = Region::Body;
                    self.page.clear();
                }
            }
            Ok(false) => {}
            Err(refused) => {
                self.notice = Some(Notice::Problem(odox_ui::refusal::message(&refused)));
            }
        }
    }

    /// Ask before a header or footer with text in it is taken away.
    fn ask_band(&mut self, ctx: &egui::Context, editing: &mut Editing) {
        let Some(footer) = self.band_question else {
            return;
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("band-question")).show(ctx, |ui| {
            ui.label(if footer {
                t("The footer holds text. Take it away with its text?")
            } else {
                t("The header holds text. Take it away with its text?")
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(t("Take it away")).clicked() {
                    answer = Some(true);
                }
                if ui.button(t("Cancel")).clicked()
                    || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => self.band(footer, false, editing),
            Some(false) => self.band_question = None,
            None => {}
        }
    }

    /// Ask what a chosen picture shows, while the question is up, and keep
    /// the answer for the page to put in.
    fn ask_picture(&mut self, ctx: &egui::Context) {
        let Some(question) = &mut self.picture_question else {
            return;
        };
        match question.show(ctx) {
            Some(true) => {
                self.picture_wanted = self.picture_question.take().map(PictureQuestion::picture);
                ctx.request_repaint();
            }
            Some(false) => self.picture_question = None,
            None => {}
        }
    }

    /// Ask about the page's paper, orientation and margins while the
    /// question is up, and keep the answer for the next frame to make.
    fn ask_page_setup(&mut self, ctx: &egui::Context) {
        let Some(question) = &mut self.page_question else {
            return;
        };
        match question.show(ctx) {
            Some(true) => {
                self.page_wanted = self.page_question.take().map(|q| q.setup());
                ctx.request_repaint();
            }
            Some(false) => self.page_question = None,
            None => {}
        }
    }

    /// Make the page the question was answered with, as one step to undo.
    fn set_page(&mut self, editing: &mut Editing) {
        let (Some(setup), Some(document)) = (self.page_wanted.take(), &mut self.document) else {
            return;
        };
        let content = document.document.content.clone();
        let styles = document.document.styles_part.clone();
        match document.set_page_setup(&setup) {
            Ok(true) => {
                let caret = self.page.selection().map(|s| s.focus.clone());
                editing.record_styles(content, styles, caret);
            }
            Ok(false) => {}
            Err(refused) => {
                self.notice = Some(Notice::Problem(odox_ui::refusal::message(&refused)));
            }
        }
    }

    /// Load the faces the document's styles name, once, when it opens.
    ///
    /// egui rebuilds its glyph atlas when the font definitions change, so this
    /// happens on opening a document and never while one is being drawn.
    fn load_fonts(&self, ctx: &egui::Context) {
        let Some(document) = &self.document else {
            return;
        };
        let mut parts = vec![&document.document.content];
        if let Some(styles) = &document.document.styles_part {
            parts.push(styles);
        }
        fonts::install(ctx, &parts);
    }
}

/// Put something in at the caret, answering why it could not be: no caret
/// in the text, or what the document refused.
fn at_caret(
    page: &mut PageEditor,
    put: impl FnOnce(&mut PageEditor) -> Result<(), odox_core::Refused>,
) -> Option<String> {
    if page.selection().is_none() {
        return Some(odox_ui::refusal::no_caret());
    }
    put(page)
        .err()
        .map(|refused| odox_ui::refusal::message(&refused))
}

/// What the window says when a change to a table is refused.
fn table_refused(refused: &odox_core::Refused) -> String {
    match refused {
        odox_core::Refused::Structure => t(
            "This table has merged or repeated cells, so its rows and columns are left as they are.",
        )
        .to_owned(),
        _ => t("The table could not be changed here.").to_owned(),
    }
}

/// Ctrl+Enter begins a page at the caret, and Backspace or Delete across the
/// start of a paragraph a page break begins takes the break off before they
/// would join anything, while the page has the keyboard; Ctrl+Alt+C and
/// Ctrl+Alt+F put a comment and a footnote at the caret. Answers why one of
/// those could not be put in.
fn break_keys(ui: &Ui, page: &mut PageEditor, model: &mut FlowModel<'_>) -> Option<String> {
    if !page.has_focus(ui.ctx()) {
        return None;
    }
    if ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)) {
        model.insert_page_break(page);
    }
    let alt = |key| {
        ui.input_mut(|input| {
            input.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::ALT, key)
        })
    };
    let mut refused = None;
    if alt(egui::Key::C) {
        refused = at_caret(page, |page| model.add_comment(page));
    }
    if alt(egui::Key::F) {
        refused = at_caret(page, |page| {
            model.insert_note(page, odox_core::edit::NoteClass::Footnote)
        })
        .or(refused);
    }
    for (key, backspace) in [(egui::Key::Backspace, true), (egui::Key::Delete, false)] {
        if ui.input(|input| input.key_pressed(key) && input.modifiers.is_none())
            && model.remove_break_by_key(page, backspace)
        {
            ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key));
        }
    }
    refused
}

/// Tab and Shift+Tab, while the page has the keyboard: at the start of a list
/// item, or over several, they move the items a level in or out, and in a
/// table they move from cell to cell rather than typing a tab. Why an item
/// could not move out, or a row could not be put in past the last cell, goes
/// to `notice`.
fn tab_keys(
    ui: &Ui,
    page: &mut PageEditor,
    model: &mut FlowModel<'_>,
    notice: &mut Option<Notice>,
) {
    if !page.has_focus(ui.ctx()) {
        return;
    }
    let tab = ui.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Key {
                key: egui::Key::Tab,
                pressed: true,
                modifiers,
                ..
            } if !modifiers.command => Some(!modifiers.shift),
            _ => None,
        })
    });
    let Some(forward) = tab else {
        return;
    };
    let taken = |ui: &Ui| {
        ui.input_mut(|input| {
            input.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab);
            input.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
        });
    };
    match model.list_level(page, if forward { 1 } else { -1 }) {
        Ok(false) => {}
        Ok(true) => {
            taken(ui);
            return;
        }
        Err(_) => {
            taken(ui);
            *notice = Some(Notice::Problem(
                t("This item cannot move out: the item it is in goes on after its list.")
                    .to_owned(),
            ));
            return;
        }
    }
    let stepped = model.step_cell(page, forward);
    if stepped != Ok(false) {
        taken(ui);
    }
    if let Err(refused) = stepped {
        *notice = Some(Notice::Problem(table_refused(&refused)));
    }
}

/// What the toolbar and the Format menu asked for that the view carries on
/// with: the question of a table's size, and a refusal to tell.
struct Asked {
    size: bool,
    picture: bool,
    notice: Option<Notice>,
}

/// A misspelled word's paragraph and characters, and what to put there.
type Correction = (Vec<usize>, std::ops::Range<usize>, String);

/// What the page asked for while it was drawn, which the model, not there
/// then, does on the next frame: a column dragged to a width, and what was
/// done in a comment's or a note's box.
#[derive(Default)]
struct Wanted {
    column: Option<(Vec<usize>, usize, f32)>,
    comment: Option<(Vec<usize>, usize, NoteResponse)>,
    note: Option<(Vec<usize>, NoteResponse)>,
    /// A tracked change to accept or reject, or every one when the id is none.
    decision: Option<(Option<String>, bool)>,
    /// A misspelled word and what the spell check's menu puts in its place.
    correction: Option<Correction>,
}

impl Wanted {
    /// Take what a flow asked for, answering whether it asked anything.
    fn take(&mut self, flow: &mut Flow<'_>) -> bool {
        self.column = flow.resized.take().or(self.column.take());
        self.comment = flow.noted.take().or(self.comment.take());
        self.note = flow.note_done.take().or(self.note.take());
        if let Some((id, accept)) = flow.decided.take() {
            self.decision = Some((Some(id), accept));
        }
        self.correction = flow.corrected.take().or(self.correction.take());
        self.correction.is_some()
            || self.column.is_some()
            || self.comment.is_some()
            || self.note.is_some()
            || self.decision.is_some()
    }

    fn apply(
        &mut self,
        page: &mut PageEditor,
        model: &mut FlowModel<'_>,
        notice: &mut Option<Notice>,
    ) {
        if let Some((id, accept)) = self.decision.take()
            && model.decide(page, id.as_deref(), accept).is_err()
        {
            let said = if accept {
                t("This change cannot be accepted: it is no longer in the document.")
            } else {
                t(
                    "This change cannot be rejected: its marks in the text are missing or stand where its text cannot go back.",
                )
            };
            *notice = Some(Notice::Problem(said.to_owned()));
        }
        if let Some((table, column, width)) = self.column.take()
            && model.set_column_width(&table, column, width).is_err()
        {
            *notice = Some(Notice::Problem(
                t("These columns cannot be resized: their widths are not stated, or together they are under a centimetre.").to_owned(),
            ));
        }
        if let Some((paragraph, index, done)) = self.comment.take()
            && let Err(refused) = model.note(&paragraph, index, &done)
        {
            *notice = Some(Notice::Problem(odox_ui::refusal::message(&refused)));
        }
        if let Some((note, done)) = self.note.take()
            && let Err(refused) = model.note_body(&note, &done)
        {
            *notice = Some(Notice::Problem(odox_ui::refusal::message(&refused)));
        }
    }

    /// Put a word the spell check's menu chose in place of the one marked,
    /// in the part of the page being typed into, where the menu was.
    fn correct(&mut self, model: &mut FlowModel<'_>) {
        if let Some((paragraph, range, word)) = self.correction.take() {
            let found = odox_ui::find::Match {
                scope: 0,
                paragraph,
                range,
            };
            model.replace_matches(&[&found], &word, true);
        }
    }
}

/// The part of the page typed into: the body, or the page's header or footer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Region {
    Body,
    Header,
    Footer,
}

/// What an undo step's caret begins with in a header or a footer, which no
/// path in the body does.
const HEADER: usize = usize::MAX;
const FOOTER: usize = usize::MAX - 1;

impl Region {
    /// What the carets an undo step remembers in this part begin with.
    fn tag(self) -> Vec<usize> {
        match self {
            Self::Body => Vec::new(),
            Self::Header => vec![HEADER],
            Self::Footer => vec![FOOTER],
        }
    }

    /// The part a remembered caret is in, and its path there.
    fn of(path: &[usize]) -> (Self, &[usize]) {
        match path.split_first() {
            Some((&HEADER, rest)) => (Self::Header, rest),
            Some((&FOOTER, rest)) => (Self::Footer, rest),
            _ => (Self::Body, path),
        }
    }
}

/// Draw a page header or footer, with the page editor where it is being typed
/// into; answers whether it was clicked into where it is not.
fn band_drawn(
    ui: &mut Ui,
    document: &odox_core::Document,
    pictures: &mut Pictures,
    (band, page): (&odox_core::Element, Option<&mut PageEditor>),
    palette: odox_ui::format::Palette,
    (scale, width): (f32, f32),
    clickable: bool,
) -> (bool, Option<Correction>) {
    let mut corrected = None;
    let rect = ui
        .scope(|ui| {
            let mut flow = Flow::new(document, pictures, scale);
            flow.palette = palette;
            flow.page = page;
            flow.blocks(ui, band, width);
            corrected = flow.corrected.take();
        })
        .response
        .rect;
    let clicked = clickable
        && ui
            .interact(
                rect,
                egui::Id::unique(("band-region", band.name.local.as_ref())),
                egui::Sense::click(),
            )
            .clicked();
    (clicked, corrected)
}

/// The faint rule between a page's header or footer and its body.
fn rule(ui: &mut Ui, palette: odox_ui::format::Palette) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(1.0, palette.ink.gamma_multiply(0.2)),
    );
}

/// In a header or footer, take the keys that put in a page break, a footnote
/// or a comment, which belong to the body, so that they do nothing there
/// rather than reach the find bar or split the paragraph.
fn swallow_body_keys(ui: &Ui) {
    ui.input_mut(|input| {
        let command = egui::Modifiers::COMMAND;
        let alt = egui::Modifiers::COMMAND | egui::Modifiers::ALT;
        input.consume_key(command, egui::Key::Enter);
        input.consume_key(alt, egui::Key::F);
        input.consume_key(alt, egui::Key::C);
    });
}

/// The model of a part of the page, with the package a picture put in it is
/// kept in: the body in the content, a header or footer in the styles part.
fn region_model<'d>(
    document: &'d mut TextDocument,
    region: Region,
    editing: &'d mut Editing,
) -> Option<(FlowModel<'d>, &'d mut odox_core::Package)> {
    let root = match region {
        Region::Body => document.body_path()?,
        Region::Header => document.band_path(false)?,
        Region::Footer => document.band_path(true)?,
    };
    let Document {
        content,
        styles,
        styles_part,
        package,
        ..
    } = &mut document.document;
    let model = if region == Region::Body {
        FlowModel::new(content, styles, root, editing)
    } else {
        FlowModel::new(styles_part.as_mut()?, styles, root, editing)
            .in_styles_part()
            .tagged(region.tag())
    };
    Some((model, package))
}

/// The bar over a page with tracked changes, saying so, and in edit mode
/// offering to accept or reject them all.
fn changes_bar(ui: &mut Ui, content: &odox_core::Element, edit_mode: bool, wanted: &mut Wanted) {
    if odox_core::edit::changes(content).is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(t("This document has tracked changes."));
        if odox_core::edit::tracking(content) {
            ui.weak(t("Edits made here are not tracked."));
        }
        if edit_mode {
            if ui.button(t("Accept all")).clicked() {
                wanted.decision = Some((None, true));
            }
            if ui.button(t("Reject all")).clicked() {
                wanted.decision = Some((None, false));
            }
        }
    });
    ui.separator();
}

/// What the Format menu asked for at the caret: a restart or a continuation
/// of a list's numbering, a comment, a note, a field, or a page break put in
/// or taken out. Answers why one could not be put in.
fn caret_command(
    page: &mut PageEditor,
    model: &mut FlowModel<'_>,
    command: Option<Command>,
) -> Option<String> {
    match command {
        Some(Command::Numbering(restart)) => {
            model.numbering(page, restart);
        }
        Some(Command::Comment) => return at_caret(page, |page| model.add_comment(page)),
        Some(Command::Note(class)) => {
            return at_caret(page, |page| model.insert_note(page, class));
        }
        Some(Command::Field(field)) => {
            return at_caret(page, |page| model.insert_field(page, field));
        }
        Some(Command::PageBreak(true)) => {
            model.insert_page_break(page);
        }
        Some(Command::PageBreak(false)) => {
            model.remove_page_break(page);
        }
        _ => {}
    }
    None
}

/// The toolbar over the page, and what it or the Format menu asked for: a
/// mark, a kind of paragraph, a table or a change to the one the caret is in.
/// A table the size question was answered with goes in here too.
fn toolbar_commands(
    ui: &mut Ui,
    page: &mut PageEditor,
    model: &mut FlowModel<'_>,
    command: Option<Command>,
    wanted: Option<(usize, usize)>,
    width: f32,
) -> Asked {
    let pressed = toolbar::text(ui, page, model);
    let mark = pressed.mark.or(match command {
        Some(Command::Mark(mark)) => Some(mark),
        _ => None,
    });
    let block = pressed.block.or(match command {
        Some(Command::Block(block)) => Some(block),
        _ => None,
    });
    if let Some(mark) = mark {
        page.toggle(model, mark);
    }
    if let Some(value) = pressed
        .value
        .or_else(|| command.and_then(|command| command.value(page, model)))
    {
        page.set(model, value);
    }
    if let Some(align) = pressed.align.or(match command {
        Some(Command::Align(align)) => Some(align),
        _ => None,
    }) {
        model.apply_alignment(page, align);
    }
    if let Some(block) = block {
        model.apply_block(page, block);
    }
    if let Some(step) = pressed.indent.or(match command {
        Some(Command::Indent(step)) => Some(step),
        _ => None,
    }) {
        model.apply_indent(page, step);
    }
    if let Some(spacing) = pressed.spacing.or(match command {
        Some(Command::LineSpacing(spacing)) => Some(spacing),
        _ => None,
    }) {
        model.apply_line_spacing(page, spacing);
    }
    let refused = caret_command(page, model, command);
    if let Some(delta) = pressed.outline.or(match command {
        Some(Command::Outline(delta)) => Some(delta),
        _ => None,
    }) {
        model.shift_headings(page, delta);
    }
    let reshaped = match command {
        Some(Command::MergeCells) => Some(model.merge_cells(page).map_err(|refused| {
            if refused == odox_core::Refused::NotFound {
                t("Select the cells to merge, from one cell of a table to another.")
            } else {
                t("These cells cannot be merged: the table has repeated rows or cells.")
            }
        })),
        Some(Command::SplitCell) => Some(model.split_cell(page).map_err(|refused| {
            if refused == odox_core::Refused::NotFound {
                t("The caret is not in a table cell.")
            } else {
                t("This cell cannot be split: the table has repeated rows or cells.")
            }
        })),
        Some(Command::EqualWidths) => Some(
            model
                .equal_widths(page)
                .map_err(|_| t("This table does not say how wide its columns are.")),
        ),
        _ => None,
    };
    if let Some(Err(message)) = reshaped {
        return Asked {
            size: false,
            picture: false,
            notice: Some(Notice::Problem(message.to_owned())),
        };
    }
    let changed = match (command, wanted) {
        (Some(Command::Table(change)), _) => Some(model.edit_table(page, change)),
        (_, Some((rows, columns))) => Some(model.insert_table(page, rows, columns, width)),
        _ => None,
    };
    Asked {
        size: pressed.table || command == Some(Command::InsertTable),
        picture: pressed.picture || command == Some(Command::InsertPicture),
        notice: changed
            .and_then(Result::err)
            .map(|refused| table_refused(&refused))
            .or(refused)
            .map(Notice::Problem),
    }
}

#[cfg(test)]
mod tests {
    use super::{Editing, TextDocument, TextView};
    use odox_core::{Node, Ns};

    fn with_header(words: &str) -> (TextView, Editing) {
        let mut document =
            TextDocument::read(&odox_core::blank::text("en-US")).expect("a blank document");
        document.set_header(true).expect("on");
        if !words.is_empty() {
            let path = document.band_path(false).expect("a header");
            let header = document
                .document
                .styles_part
                .as_mut()
                .and_then(|part| part.at_mut(&path))
                .expect("the header");
            let paragraph = header.child_mut(&Ns::Text, "p").expect("its paragraph");
            paragraph.children.push(Node::Text(words.to_owned()));
            paragraph.self_closing = false;
        }
        let view = TextView {
            document: Some(document),
            ..TextView::default()
        };
        let mut editing = Editing::default();
        editing.reset();
        (view, editing)
    }

    fn has_header(view: &TextView) -> bool {
        view.document
            .as_ref()
            .is_some_and(|document| document.band_path(false).is_some())
    }

    #[test]
    fn turning_header_off_asks_when_not_empty() {
        let (mut view, mut editing) = with_header("Words");
        view.band(false, false, &mut editing);
        assert_eq!(view.band_question, Some(false));
        assert!(has_header(&view), "nothing is taken away before the answer");
        view.band(false, false, &mut editing);
        assert!(!has_header(&view));
        assert!(editing.can_undo());
    }

    #[test]
    fn an_empty_header_is_taken_away_without_asking() {
        let (mut view, mut editing) = with_header("");
        view.band(false, false, &mut editing);
        assert_eq!(view.band_question, None);
        assert!(!has_header(&view));
    }

    #[test]
    fn a_remembered_caret_takes_the_page_to_its_part() {
        use odox_ui::View;
        let (mut view, _) = with_header("Words");
        view.restore_caret(odox_ui::Caret::new(vec![super::HEADER, 0], 2));
        assert_eq!(view.region, super::Region::Header);
        let caret = view.caret().expect("a caret");
        assert_eq!(caret.paragraph, vec![super::HEADER, 0]);
        view.restore_caret(odox_ui::Caret::new(vec![3], 1));
        assert_eq!(view.region, super::Region::Body);
        assert_eq!(view.caret().expect("a caret").paragraph, vec![3]);
    }

    #[test]
    fn a_header_with_a_picture_and_no_text_is_asked_about() {
        let (mut view, mut editing) = with_header("");
        let document = view.document.as_mut().expect("open");
        let path = document.band_path(false).expect("a header");
        let header = document
            .document
            .styles_part
            .as_mut()
            .and_then(|part| part.at_mut(&path))
            .expect("the header");
        let paragraph = header.child_mut(&Ns::Text, "p").expect("its paragraph");
        paragraph
            .children
            .push(Node::Element(odox_core::Element::new(
                "draw",
                "frame",
                Ns::Draw,
            )));
        paragraph.self_closing = false;
        view.band(false, false, &mut editing);
        assert_eq!(view.band_question, Some(false));
    }

    #[test]
    fn a_comment_asked_for_with_no_caret_says_so() {
        let mut document =
            TextDocument::read(&odox_core::blank::text("en-US")).expect("a blank document");
        let root = document.body_path().expect("a body");
        let mut editing = Editing::default();
        editing.reset();
        let before = document.document.content.clone();
        let odox_core::Document {
            content, styles, ..
        } = &mut document.document;
        let mut model = odox_ui::FlowModel::new(content, styles, root, &mut editing);
        let mut page = super::page_editor();
        for command in [
            odox_ui::toolbar::Command::Comment,
            odox_ui::toolbar::Command::Note(odox_core::edit::NoteClass::Footnote),
            odox_ui::toolbar::Command::Field(odox_core::edit::Field::PageNumber),
        ] {
            assert_eq!(
                super::caret_command(&mut page, &mut model, Some(command)),
                Some(odox_ui::refusal::no_caret())
            );
        }
        drop(model);
        assert!(document.document.content == before, "nothing was changed");
    }

    #[test]
    fn a_refused_insert_says_why() {
        let mut page = super::page_editor();
        page.select(egui_richedit::Selection::caret(odox_ui::Caret::new(
            vec![0],
            0,
        )));
        assert_eq!(
            super::at_caret(&mut page, |_| Err(odox_core::Refused::Namespace)),
            Some(odox_ui::refusal::message(&odox_core::Refused::Namespace))
        );
        assert_eq!(super::at_caret(&mut page, |_| Ok(())), None);
    }
}
