//! The window's one view: a document's flow, and its headings beside it.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use eframe::egui::scroll_area::ScrollAreaOutput;
use eframe::egui::{self, Pos2, Rect, Ui};
use egui_richedit::{Model, Selection};
use odox_core::Document;
use odox_core::doc::TextDocument;
use odox_ui::find::{Match, Replaced, in_paragraphs};
use odox_ui::i18n::{fill, t};
use odox_ui::toolbar::{self, Command, MenuState};
use odox_ui::{
    Caret, Editing, Export, Flow, FlowModel, Found, Notice, PageEditor, Pictures, View, Zoom,
    fonts, page_editor,
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

    /// The Page up and Page down buttons under the page, and the keys when
    /// `keys` says the caret is not the one taking them.
    fn paging(&mut self, ui: &mut Ui, keys: bool) {
        let (up, down) = self.room();
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
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(up, egui::Button::new(t("Page up")))
                    .clicked()
                {
                    moved = Some(Move::Page { down: false });
                }
                if ui
                    .add_enabled(down, egui::Button::new(t("Page down")))
                    .clicked()
                {
                    moved = Some(Move::Page { down: true });
                }
            });
        });
        if let Some(moved) = moved {
            self.go(ui.ctx(), moved);
        }
    }

    /// Whether the view can move up, and whether it can move down.
    fn room(&self) -> (bool, bool) {
        self.shown.map_or((false, false), |shown| {
            (shown.offset > 0.5, shown.offset < shown.bottom() - 0.5)
        })
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
    /// The size asked of a table to put in, while the question is up.
    table_size: Option<(usize, usize)>,
    /// The size the question was answered with, put in on the next frame.
    table_wanted: Option<(usize, usize)>,
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
            table_size: None,
            table_wanted: None,
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
        self.scroll_to = None;
        self.place = None;
        self.page.clear();
        self.found.clear();
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
        self.export = Export::default();
        self.pictures.clear();
        self.page.clear();
        self.found.clear();
        self.scroll = Scroll::default();
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
        self.page
            .selection()
            .map(|selection| selection.focus.clone())
    }

    fn restore_caret(&mut self, caret: Caret) {
        self.page.select(Selection::caret(caret));
    }

    fn find(&mut self, query: &str) -> usize {
        let matches = self
            .document
            .as_ref()
            .and_then(TextDocument::body)
            .map(|body| in_paragraphs(body, 0, query))
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

    fn central(&mut self, ui: &mut Ui, zoom: &mut Zoom, editing: &mut Editing) {
        if let Some(document) = &mut self.document {
            self.export.show(ui.ctx(), document, editing);
        }
        // While a question is up the page is drawn as it is and takes no keys;
        // the caret waits for it to be answered.
        let asked = self.table_size.is_some();
        self.ask_table_size(ui.ctx());
        let edit_mode = editing.on && !editing.asking && !self.export.asking() && !asked;
        if !editing.on {
            self.page.clear();
        }
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

        if edit_mode && let Some(root) = document.body_path() {
            let text_width = document.text_width();
            let Document {
                content, styles, ..
            } = &mut document.document;
            let mut model = FlowModel::new(content, styles, root, editing);
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
                self.command.take(),
                self.table_wanted.take(),
                text_width,
            );
            if asked.size {
                self.table_size = Some((2, 3));
            }
            if asked.notice.is_some() {
                self.notice = asked.notice;
            }
            self.menu = MenuState::of(&self.page, &model, true);
            ui.separator();
        } else {
            self.command = None;
            self.menu = MenuState::offering_blocks(true);
        }

        // The caret's own Page Up and Page Down move it, and the view with it.
        let keys = !editing.asking
            && !self.export.asking()
            && (!edit_mode || self.page.selection().is_none());
        self.scroll.paging(ui, keys);

        let (area, anchor) = self.scroll.area(ui, scale);
        let output = area.show(ui, |ui| {
            // What was typed goes into the tree before it is laid out, so
            // this frame already draws it.
            if edit_mode && let Some(root) = document.body_path() {
                let Document {
                    content, styles, ..
                } = &mut document.document;
                let mut model = FlowModel::new(content, styles, root, editing);
                table_keys(ui, &mut self.page, &mut model, &mut self.notice);
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
                        let mut flow = Flow::new(&document.document, &mut self.pictures, scale);
                        flow.palette = palette;
                        flow.scroll_to_heading = self.scroll_to.take();
                        flow.scroll_to_place = self.place.take();
                        flow.page = edit_mode.then_some(&mut self.page);
                        flow.find = self.found.highlights(0);
                        flow.blocks(ui, body, width);
                        if flow.followed.is_some() {
                            self.place = flow.followed.take();
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

/// Tab and Shift+Tab in a table move from cell to cell rather than typing a
/// tab, while the page has the keyboard. Why a row could not be put in past
/// the last cell goes to `notice`.
fn table_keys(
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
    let stepped = model.step_cell(page, forward);
    if stepped != Ok(false) {
        ui.input_mut(|input| {
            input.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab);
            input.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
        });
    }
    if let Err(refused) = stepped {
        *notice = Some(Notice::Problem(table_refused(&refused)));
    }
}

/// What the toolbar and the Format menu asked for that the view carries on
/// with: the question of a table's size, and a refusal to tell.
struct Asked {
    size: bool,
    notice: Option<Notice>,
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
    if let Some(align) = pressed.align.or(match command {
        Some(Command::Align(align)) => Some(align),
        _ => None,
    }) {
        model.apply_alignment(page, align);
    }
    if let Some(block) = block {
        model.apply_block(page, block);
    }
    let changed = match (command, wanted) {
        (Some(Command::Table(change)), _) => Some(model.edit_table(page, change)),
        (_, Some((rows, columns))) => Some(model.insert_table(page, rows, columns, width)),
        _ => None,
    };
    Asked {
        size: pressed.table || command == Some(Command::InsertTable),
        notice: changed
            .and_then(Result::err)
            .map(|refused| Notice::Problem(table_refused(&refused))),
    }
}
