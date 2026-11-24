//! The window's one view: a deck's slides, and the one being looked at.
//!
//! A slide is drawn at the geometry the document gives: every shape on a page
//! carries its own position and size in the page's coordinate space, so the page
//! is scaled to the space the window has and each shape is put where the document
//! says. In edit mode the slide's own shapes can be picked, dragged and resized
//! by their corners, which writes the four `svg:` attributes back. DESIGN.md §11.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use eframe::egui::{self, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use egui_richedit::Selection;
use odox_core::doc::{Presentation, Steps};
use odox_core::edit;
use odox_core::{Document, Element, Length, Ns, PageLayout};
use odox_ui::find::{Highlights, Match, Options, Replaced, in_paragraphs};
use odox_ui::i18n::{fill, t};
use odox_ui::link_dialog::LinkQuestion;
use odox_ui::note_popup::{self, NoteContent, NoteResponse};
use odox_ui::picture_question::PictureQuestion;
use odox_ui::toolbar::{self, Command, MenuState};
use odox_ui::{
    Canvas, Caret, Editing, Export, Flow, FlowModel, Found, Notice, PageEditor, Pictures, View,
    Zoom, fonts, marks, page_editor,
};

/// The slides a link may go to: each named slide, as `#name`, with its number
/// and name for a person to read.
fn slide_places(document: &Presentation) -> Vec<(String, String)> {
    document
        .slides()
        .iter()
        .enumerate()
        .filter_map(|(at, slide)| {
            let name = slide.name?;
            let unnamed = name
                .strip_prefix("page")
                .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
            let shown = if unnamed {
                fill(t("Slide {number}"), &[("number", &(at + 1).to_string())])
            } else {
                format!("{}. {name}", at + 1)
            };
            Some((shown, format!("#{name}")))
        })
        .collect()
}

/// A presentation, open or not.
pub struct SlideView {
    document: Option<Presentation>,
    slide: usize,
    pictures: Pictures,
    show_notes: bool,
    /// The shape picked on the slide, by its index among the page's children.
    picked: Option<usize>,
    /// A drag in progress on the picked shape.
    drag: Option<Drag>,
    /// The caret in a label, in edit mode, its paragraphs named by their
    /// path from the page.
    page_editor: PageEditor,
    /// What a search found. A slide's own labels are scope `n` and its notes
    /// scope `slides + n`.
    found: Found,
    /// The slideshow is on: one slide, filling the screen, and nothing else.
    presenting: bool,
    /// How many of the shown slide's builds the show has clicked through.
    step: usize,
    /// What the Format menu draws itself from, and what it last chose.
    menu: MenuState,
    command: Option<Command>,
    /// What the Slide menu last chose.
    slide_command: Option<SlideCommand>,
    /// A comment just put on the slide, whose box opens on the next frame.
    opened_note: Option<usize>,
    /// What was done in a slide comment's box, done on the next frame.
    note_wanted: Option<(usize, NoteResponse)>,
    export: Export,
    /// The link question.
    link: LinkQuestion,
    /// What the Slide menu asked for that is done on the next frame: a
    /// comment put on the slide, or a picture asked for.
    menu_asked: Option<MenuAsked>,
    /// Slide > Text box was chosen: the next drag on the slide draws the
    /// box, or a click puts one there.
    placing: bool,
    /// Where a drag drawing a text box began, on screen.
    place_from: Option<Pos2>,
    /// What a chosen picture shows, while the question is up.
    picture_question: Option<PictureQuestion>,
    notice: Option<Notice>,
}

impl Default for SlideView {
    fn default() -> Self {
        Self {
            document: None,
            slide: 0,
            pictures: Pictures::default(),
            show_notes: false,
            picked: None,
            drag: None,
            page_editor: page_editor(),
            found: Found::default(),
            presenting: false,
            step: 0,
            menu: MenuState::offering_blocks(false),
            command: None,
            slide_command: None,
            opened_note: None,
            note_wanted: None,
            link: LinkQuestion::default(),
            menu_asked: None,
            placing: false,
            place_from: None,
            picture_question: None,
            notice: None,
            export: Export::default(),
        }
    }
}

/// What the Slide menu asked for, applied when the page is next drawn.
#[derive(Clone, Copy)]
enum SlideCommand {
    Add,
    Duplicate,
    Delete,
    Earlier,
    Later,
}

/// A slide's comments: a mark at each, and its box while the mark is under
/// the pointer, editable in edit mode. `opened` is a comment just put on the
/// slide, whose box opens now. Answers what was done in a box.
fn slide_comments(
    ui: &Ui,
    page: Rect,
    scale: f32,
    index: usize,
    slide: &Element,
    editable: bool,
    opened: Option<usize>,
) -> Option<(usize, NoteResponse)> {
    let mut noted = None;
    for (child, (x, y), note) in edit::slide_annotations(slide) {
        let mark = Rect::from_min_size(
            pos2(page.left() + x * scale, page.top() + y * scale),
            vec2(14.0, 12.0) * scale.max(0.5),
        );
        ui.painter()
            .rect_filled(mark, 2.0, egui::Color32::from_rgb(0xf2, 0xb7, 0x05));
        let id = egui::Id::unique(("odox-slide-comment", index, child));
        let header = [
            note.author.unwrap_or_default(),
            note.date.unwrap_or_default().replace('T', " "),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" \u{b7} ");
        let content = NoteContent {
            header,
            text: note.text,
        };
        let wanted = ui.rect_contains_pointer(mark);
        let opened = opened == Some(child);
        let done = note_popup::show(ui, mark, id, &content, wanted, opened, editable);
        if let Some(done) = done {
            noted = Some((child, done));
        }
    }
    noted
}

/// A shape's box on the page, in ODF points.
#[derive(Clone, Copy)]
struct Box {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

/// What a drag took hold of: the shape itself, or one of its corners, named
/// by which fraction of the box across and down it sits at.
#[derive(Clone, Copy)]
enum Grab {
    Whole,
    Corner(f32, f32),
}

/// A drag in progress: where it began on screen, what it took hold of, and
/// the box as it was, so that each frame's geometry comes from the pointer's
/// whole travel rather than from a sum of small steps.
struct Drag {
    shape: usize,
    from: Pos2,
    grab: Grab,
    start: Box,
}

/// The keys and clicks that move through a show, taken in one frame.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy)]
struct ShowKeys {
    forward: bool,
    back: bool,
    next: bool,
    previous: bool,
    first: bool,
    last: bool,
    leave: bool,
}

/// What the Slide menu asked for, done on the next frame.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuAsked {
    Comment,
    Picture,
}

/// What the slide asked for this frame, applied once the document is free.
enum Action {
    Pick(Option<usize>),
    Begin(Drag),
    Move(Pos2),
    End,
    /// A text box to put on the slide, at a place and size in points.
    Place((f32, f32, f32, f32)),
}

/// The size of a corner handle on screen, and how far from a corner a press
/// still takes it.
const HANDLE: f32 = 8.0;

impl View for SlideView {
    fn open(&mut self, ctx: &egui::Context, bytes: &[u8], path: &Path) -> Result<(), String> {
        let document = Presentation::read(bytes).map_err(|e| {
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
        let mut parts = vec![&document.document.content];
        if let Some(styles) = &document.document.styles_part {
            parts.push(styles);
        }
        fonts::install(ctx, &parts);

        self.pictures.clear();
        self.document = Some(document);
        self.stop_placing();
        self.slide = 0;
        self.picked = None;
        self.drag = None;
        self.page_editor.clear();
        self.found.clear();
        self.presenting = false;
        self.export = Export::default();
        Ok(())
    }

    fn blank(&self, language: &str) -> Vec<u8> {
        odox_core::blank::presentation(language)
    }

    fn begin(&mut self) {
        let title = self.document.as_ref().and_then(|document| {
            document
                .slides()
                .get(self.slide)?
                .shapes_indexed()
                .find(|(_, shape)| shape.attr(&Ns::Presentation, "class") == Some("title"))
                .map(|(index, _)| index)
        });
        if let Some(paragraph) = title.and_then(|title| self.empty_placeholder(title)) {
            self.picked = title;
            self.page_editor
                .select(Selection::caret(Caret::new(paragraph, 0)));
        }
    }

    fn close(&mut self) {
        self.document = None;
        self.stop_placing();
        self.pictures.clear();
        self.picked = None;
        self.drag = None;
        self.page_editor.clear();
        self.found.clear();
        self.presenting = false;
        self.export = Export::default();
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
        self.drag = None;
        self.page_editor.document_replaced();
        // An undo can take slides away from under the one in view.
        let last = self
            .document
            .as_ref()
            .map_or(0, |document| document.slides().len().saturating_sub(1));
        self.slide = self.slide.min(last);
    }

    fn caret(&self) -> Option<Caret> {
        let focus = &self.page_editor.selection()?.focus;
        Some(Caret::new(
            [&[self.slide][..], &focus.paragraph].concat(),
            focus.offset,
        ))
    }

    fn find(&mut self, query: &str, options: Options) -> usize {
        let matches = self.document.as_ref().map(|document| {
            let slides = document.slides();
            let mut found = Vec::new();
            for (index, slide) in slides.iter().enumerate() {
                found.extend(in_paragraphs(slide.element, index, query, options));
            }
            for (index, slide) in slides.iter().enumerate() {
                if let Some(notes) = slide.notes() {
                    found.extend(in_paragraphs(notes, slides.len() + index, query, options));
                }
            }
            found
        });
        self.found.set(matches.unwrap_or_default());
        self.found.len()
    }

    fn show_match(&mut self, index: usize) {
        let Some(found) = self.found.show(index).cloned() else {
            return;
        };
        let count = self
            .document
            .as_ref()
            .map_or(0, |document| document.slides().len());
        if found.scope >= count {
            self.show_notes = true;
            self.show_slide(found.scope - count);
        } else {
            self.show_slide(found.scope);
        }
    }

    fn menus(&mut self, ui: &mut Ui, editing: &Editing) {
        ui.menu_button(t("Format"), |ui| {
            if let Some(command) = toolbar::menu(ui, &self.menu) {
                self.command = Some(command);
            }
        });
        let count = self
            .document
            .as_ref()
            .map_or(0, |document| document.slides().len());
        let can = editing.on && !editing.asking;
        ui.menu_button(t("Slide"), |ui| {
            if ui
                .add_enabled(can, egui::Button::new(t("Comment")))
                .clicked()
            {
                self.menu_asked = Some(MenuAsked::Comment);
                ui.close();
            }
            if ui
                .add_enabled(can, egui::Button::new(t("Text box")))
                .clicked()
            {
                self.placing = true;
                ui.close();
            }
            if ui
                .add_enabled(can, egui::Button::new(t("Picture…")))
                .clicked()
            {
                self.menu_asked = Some(MenuAsked::Picture);
                ui.close();
            }
            ui.separator();
            let entries = [
                (t("New slide"), SlideCommand::Add, can),
                (t("Duplicate slide"), SlideCommand::Duplicate, can),
                (t("Delete slide"), SlideCommand::Delete, can && count > 1),
                (
                    t("Move slide earlier"),
                    SlideCommand::Earlier,
                    can && self.slide > 0,
                ),
                (
                    t("Move slide later"),
                    SlideCommand::Later,
                    can && self.slide + 1 < count,
                ),
            ];
            for (name, command, enabled) in entries {
                if ui.add_enabled(enabled, egui::Button::new(name)).clicked() {
                    self.slide_command = Some(command);
                    ui.close();
                }
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
        // Where each scope's paragraphs are rooted, found before the document
        // is changed: the slides borrow it.
        let count = document.slides().len();
        let mut roots: Vec<(usize, usize, Vec<usize>)> = Vec::new();
        for found in &matches {
            if roots.iter().any(|(scope, ..)| *scope == found.scope) {
                continue;
            }
            let (slide, notes) = if found.scope >= count {
                (found.scope - count, true)
            } else {
                (found.scope, false)
            };
            let slides = document.slides();
            let Some(this) = slides.get(slide) else {
                continue;
            };
            let Some(mut root) = document.page_path(this.position) else {
                continue;
            };
            if notes {
                let Some((at, _)) = this
                    .element
                    .elements_indexed()
                    .find(|(_, e)| e.is(&Ns::Presentation, "notes"))
                else {
                    continue;
                };
                root.push(at);
            }
            roots.push((found.scope, slide, root));
        }
        let Document {
            content, styles, ..
        } = &mut document.document;
        let mut total = Replaced::default();
        for (scope, slide, root) in roots {
            let group: Vec<&Match> = matches.iter().filter(|m| m.scope == scope).collect();
            let mut model = FlowModel::new(content, styles, root, editing).tagged(vec![slide]);
            let done = model.replace_matches(&group, with, total.replaced == 0);
            total.replaced += done.replaced;
            total.skipped += done.skipped;
        }
        total
    }

    fn restore_caret(&mut self, caret: Caret) {
        let Some((&slide, within)) = caret.paragraph.split_first() else {
            return;
        };
        self.show_slide(slide);
        self.picked = within.first().copied();
        self.page_editor
            .select(Selection::caret(Caret::new(within.to_vec(), caret.offset)));
    }

    fn fits(&self) -> bool {
        true
    }

    fn presenting(&self) -> bool {
        self.presenting
    }

    #[allow(clippy::too_many_lines)]
    fn central(&mut self, ui: &mut Ui, zoom: &mut Zoom, editing: &mut Editing) {
        let count = self
            .document
            .as_ref()
            .map_or(0, |document| document.slides().len());
        if count == 0 {
            self.presenting = false;
            ui.centered_and_justified(|ui| {
                ui.weak(t("This presentation has no slides."));
            });
            return;
        }
        let asking = self.carry_export(ui.ctx(), editing)
            || self.link.is_up()
            || self.picture_question.is_some();
        self.link.ask(ui.ctx());
        self.ask_picture(ui.ctx(), editing);
        if !asking && self.keys_before_drawing(ui, editing, count) {
            return;
        }
        if let Some(command) = self.slide_command.take() {
            self.change_slides(command, editing);
        }
        if self.menu_asked == Some(MenuAsked::Comment) {
            self.menu_asked = None;
            self.add_comment(editing);
        }
        if let Some((child, done)) = self.note_wanted.take() {
            self.note(child, &done, editing);
        }

        // The document and the picture cache are taken as separate borrows of
        // separate fields, which is what lets the renderer hold one while filling
        // the other.
        let slide_index = self.slide;
        let show_notes = self.show_notes;
        let edit_mode = editing.on && !editing.asking && !asking;
        let picked = self.picked;
        let placing = self.placing;
        let mut place_from = self.place_from;
        let dragging = self.drag.is_some();
        let mut action = None;
        let mut followed = None;
        let mut noted = None;
        let opened = self.opened_note.take();

        if edit_mode {
            self.edit(ui, editing);
        } else {
            self.command = None;
            self.menu = MenuState::offering_blocks(false);
        }

        let Some(document) = &self.document else {
            return;
        };
        let pictures = &mut self.pictures;
        let page_editor = edit_mode.then_some(&mut self.page_editor);
        let slides = document.slides();
        let Some(slide) = slides.get(slide_index) else {
            return;
        };

        if show_notes {
            let notes_at = slides.len() + slide_index;
            notes_panel(
                ui,
                &document.document,
                pictures,
                slide,
                zoom.scale(),
                self.found.highlights(notes_at),
            );
        }

        let layout = document.page_layout(slide);
        zoom.fit(fit(ui.available_size(), &layout));
        let scale = zoom.scale();
        let size = vec2(
            layout.width.points() * scale,
            layout.height.points() * scale,
        );

        // Both resolved before the closure, which borrows the document again.
        let background = document.background(slide);
        let decorations = document.background_objects(slide);
        let fields = document.master_fields(slide, odox_ui::today());

        egui::ScrollArea::both().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                let sense = if edit_mode {
                    Sense::click_and_drag()
                } else {
                    Sense::hover()
                };
                let (page, response) = ui.allocate_exact_size(size, sense);
                let palette = odox_ui::format::Palette::for_theme(ui.visuals().dark_mode);
                // Paper under everything. A slide whose background is `none` —
                // which is what a template says when its identity is the shapes
                // rather than the ground — is drawn on paper and not on the
                // window's own colour. `odox_ui::format::Palette` says why.
                ui.painter().rect_filled(page, 2.0, palette.paper);

                let mut canvas = Canvas::new(&document.document, pictures, page, scale, palette);
                canvas.page_editor = page_editor.filter(|_| !placing);
                canvas.find = self.found.highlights(slide_index);
                canvas.marks = marks::shown(ui.ctx());
                // Back to front: the ground, then what the master page draws on
                // every slide, then the slide's own.
                canvas.background(ui, &background);
                for shape in decorations.iter().copied().chain(&fields) {
                    canvas.shape(ui, shape);
                }
                for (index, shape) in slide.shapes_indexed() {
                    canvas.slide_shape(ui, index, shape);
                }
                followed = canvas.followed.take();
                noted = slide_comments(
                    ui,
                    page,
                    scale,
                    slide_index,
                    slide.element,
                    edit_mode,
                    opened,
                );

                // The page's edge last, so a decoration running to the bleed
                // does not paint over it.
                ui.painter().rect_stroke(
                    page,
                    2.0,
                    Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                    StrokeKind::Inside,
                );

                if edit_mode && placing {
                    action = place(ui, &response, page, scale, &mut place_from);
                } else if edit_mode {
                    action = interact(ui, &response, page, scale, slide, picked, dragging);
                }
            });
        });

        self.place_from = place_from;
        self.found.drawn();
        if noted.is_some() {
            self.note_wanted = noted;
            ui.ctx().request_repaint();
        }
        if let Some(name) = followed {
            self.go_to_slide_named(&name);
        }
        if let Some(action) = action {
            self.act(action, scale, editing);
        }
    }

    fn has_formatting_marks(&self) -> bool {
        self.document.is_some()
    }

    fn has_side(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(|document| !document.slides().is_empty())
    }

    fn side(&mut self, ui: &mut Ui) {
        let Some(document) = &self.document else {
            return;
        };
        let slides = document.slides();
        ui.heading(t("Slides"));
        ui.separator();
        let mut chosen = None;
        for (index, slide) in slides.iter().enumerate() {
            // A slide's first line of text is what it is recognized by; its
            // `draw:name` is usually a number an application generated.
            let heading = slide
                .shapes()
                .map(odox_core::Element::plain_text)
                .find(|text| !text.trim().is_empty())
                .unwrap_or_else(|| slide.name.unwrap_or_default().to_owned());
            let label = format!("{}. {}", index + 1, first_line(&heading));
            if ui.selectable_label(index == self.slide, label).clicked() {
                chosen = Some(index);
            }
        }
        if let Some(index) = chosen {
            self.show_slide(index);
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
        ui.separator();
        ui.checkbox(&mut self.show_notes, t("Show the speaker's notes"));
        ui.separator();
        if ui.button(t("Start the slideshow")).clicked() {
            self.start_show(0);
            ui.close();
        }
        if ui
            .button(t("Start the slideshow from this slide"))
            .clicked()
        {
            self.start_show(self.slide);
            ui.close();
        }
    }
}

impl SlideView {
    /// The toolbar, and what was typed, which goes into the tree before it is
    /// drawn, kept to the one shape the caret is in.
    fn edit(&mut self, ui: &mut Ui, editing: &mut Editing) {
        let Some(document) = &mut self.document else {
            return;
        };
        let Some(root) = document
            .slides()
            .get(self.slide)
            .map(|s| s.position)
            .and_then(|position| document.page_path(position))
        else {
            return;
        };
        let scope = self
            .page_editor
            .selection()
            .and_then(|selection| selection.focus.paragraph.first().copied())
            .into_iter()
            .collect();
        let places = slide_places(document);
        let Document {
            content, styles, ..
        } = &mut document.document;
        let mut model = FlowModel::new(content, styles, root, editing)
            .within(scope)
            .tagged(vec![self.slide]);
        let command = self.command.take();
        let pressed = toolbar::label(ui, &self.page_editor, &model);
        let mark = pressed.mark.or(match command {
            Some(Command::Mark(mark)) => Some(mark),
            _ => None,
        });
        if let Some(mark) = mark {
            self.page_editor.toggle(&mut model, mark);
        }
        if let Some(value) = pressed
            .value
            .or_else(|| command.and_then(|command| command.value(&self.page_editor, &model)))
        {
            self.page_editor.set(&mut model, value);
        }
        if let Some(align) = pressed.align.or(match command {
            Some(Command::Align(align)) => Some(align),
            _ => None,
        }) {
            model.apply_alignment(&mut self.page_editor, align);
        }
        if let Some(step) = pressed.indent.or(match command {
            Some(Command::Indent(step)) => Some(step),
            _ => None,
        }) {
            model.apply_indent(&mut self.page_editor, step);
        }
        if let Some(spacing) = pressed.spacing.or(match command {
            Some(Command::LineSpacing(spacing)) => Some(spacing),
            _ => None,
        }) {
            model.apply_line_spacing(&mut self.page_editor, spacing);
        }
        if let Some(problem) = self.link.act(
            ui,
            &mut model,
            &mut self.page_editor,
            command == Some(Command::Link),
            |_| places,
        ) {
            self.notice = Some(Notice::Problem(problem));
        }
        self.menu = MenuState::of(&self.page_editor, &model, false);
        ui.separator();
        self.page_editor.input(ui, &mut model);
    }

    /// Ask for a picture once Slide > Picture… is chosen, ask what it shows,
    /// and put it on the slide, as one step to undo.
    fn ask_picture(&mut self, ctx: &egui::Context, editing: &mut Editing) {
        if self.menu_asked == Some(MenuAsked::Picture) {
            self.menu_asked = None;
            match PictureQuestion::choose(ctx, f32::MAX) {
                Ok(question) => self.picture_question = question,
                Err(problem) => self.notice = Some(Notice::Problem(problem)),
            }
        }
        let Some(question) = &mut self.picture_question else {
            return;
        };
        let answer = question.show(ctx);
        if answer.is_none() {
            return;
        }
        let Some(question) = self.picture_question.take() else {
            return;
        };
        if answer != Some(true) {
            return;
        }
        let picture = question.picture();
        let Some(document) = &mut self.document else {
            return;
        };
        let Some(position) = document.slides().get(self.slide).map(|s| s.position) else {
            return;
        };
        let before = document.document.content.clone();
        let name = document
            .document
            .package
            .picture_name(&picture.bytes, picture.extension);
        let parts = document
            .document
            .package
            .keep(&[name.as_str(), "META-INF/manifest.xml"]);
        let placed = document.insert_picture(
            position,
            &odox_core::doc::SlidePicture {
                bytes: &picture.bytes,
                media_type: picture.media_type,
                extension: picture.extension,
                size: picture.size,
                description: picture.description.as_deref(),
            },
        );
        match placed {
            Ok(path) => {
                editing.record_step(before, parts, None);
                self.picked = path.last().copied();
                ctx.request_repaint();
            }
            Err(refused) => {
                document.document.content = before;
                document.document.package.restore(parts);
                self.notice = Some(Notice::Problem(refused.to_string()));
            }
        }
    }

    /// Apply what the slide asked for.
    fn act(&mut self, action: Action, scale: f32, editing: &mut Editing) {
        match action {
            Action::Place(rect) => {
                self.placing = false;
                self.place_from = None;
                let Some(document) = &mut self.document else {
                    return;
                };
                let Some(position) = document.slides().get(self.slide).map(|s| s.position) else {
                    return;
                };
                let before = document.document.content.clone();
                let caret = self
                    .page_editor
                    .selection()
                    .map(|selection| selection.focus.clone())
                    .map(|focus| {
                        Caret::new([vec![self.slide], focus.paragraph].concat(), focus.offset)
                    });
                match document.insert_text_box(position, rect) {
                    Ok(path) => {
                        editing.record_snapshot(before, caret);
                        let shape = path.last().copied();
                        self.picked = shape;
                        if let Some(shape) = shape {
                            self.page_editor
                                .select(Selection::caret(Caret::new(vec![shape, 0, 0], 0)));
                        }
                    }
                    Err(refused) => self.notice = Some(Notice::Problem(refused.to_string())),
                }
            }
            Action::Pick(shape) => {
                self.picked = shape;
                self.drag = None;
                // An empty placeholder has its one line at an edge, away from
                // where its hint asks to be clicked.
                if let Some(paragraph) = shape.and_then(|shape| self.empty_placeholder(shape)) {
                    self.page_editor
                        .select(Selection::caret(Caret::new(paragraph, 0)));
                }
            }
            Action::Begin(drag) => {
                if let Some(document) = &self.document {
                    editing.record(&document.document.content);
                }
                self.picked = Some(drag.shape);
                self.drag = Some(drag);
            }
            Action::Move(at) => {
                let Some(drag) = &self.drag else { return };
                let travel = (at - drag.from) / scale;
                let Box {
                    mut x,
                    mut y,
                    mut width,
                    mut height,
                } = drag.start;
                match drag.grab {
                    Grab::Whole => {
                        x += travel.x;
                        y += travel.y;
                    }
                    Grab::Corner(u, v) => {
                        // The grabbed corner follows the pointer and the
                        // opposite one stays; a box is never thinner than a
                        // point, which keeps it findable.
                        if u > 0.5 {
                            width = (width + travel.x).max(1.0);
                        } else {
                            let moved = travel.x.min(width - 1.0);
                            x += moved;
                            width -= moved;
                        }
                        if v > 0.5 {
                            height = (height + travel.y).max(1.0);
                        } else {
                            let moved = travel.y.min(height - 1.0);
                            y += moved;
                            height -= moved;
                        }
                    }
                }
                let shape = drag.shape;
                let Some(document) = &mut self.document else {
                    return;
                };
                let Some(slide) = document.slides().get(self.slide).map(|s| s.position) else {
                    return;
                };
                // A shape that refuses is one that cannot be placed this way,
                // which the box list already leaves out; nothing to say.
                let _ = document.set_geometry(
                    slide,
                    shape,
                    Length(x),
                    Length(y),
                    Length(width),
                    Length(height),
                );
            }
            Action::End => self.drag = None,
        }
    }

    /// What the keys and the modes settle before a slide is drawn. Answers
    /// whether the slideshow has taken the frame, and nothing more is to be
    /// drawn.
    fn keys_before_drawing(&mut self, ui: &mut Ui, editing: &Editing, count: usize) -> bool {
        if self.presenting {
            self.present(ui, count);
            return true;
        }
        if !editing.asking {
            self.start_keys(ui);
        }
        // The arrows and the page keys step through the slides unless
        // something has the keyboard, as the caret in a label does.
        if !ui.ctx().egui_wants_keyboard_input() {
            self.step_keys(ui, count);
        }
        if ui.input(|input| input.key_pressed(Key::Escape)) {
            self.picked = None;
            self.drag = None;
            self.page_editor.clear();
            self.stop_placing();
        }
        if !editing.on {
            self.page_editor.clear();
            self.stop_placing();
        }
        false
    }

    /// Carry an export on. Answers whether its question is up, which the
    /// slide waits on: it is drawn as it is and takes no keys.
    fn carry_export(&mut self, ctx: &egui::Context, editing: &mut Editing) -> bool {
        if let Some(document) = &mut self.document {
            self.export.show(ctx, document, editing);
        }
        self.export.asking()
    }

    /// Put an empty comment near the top left of the slide in view, as one
    /// step to undo, with its box open to be written in.
    fn add_comment(&mut self, editing: &mut Editing) {
        let Some(document) = &mut self.document else {
            return;
        };
        let Some(page) = document
            .slides()
            .get(self.slide)
            .and_then(|slide| document.page_path(slide.position))
        else {
            return;
        };
        let before = document.document.content.clone();
        let corner = 1.0 / 2.54 * 72.0;
        if let Ok(child) = edit::add_slide_annotation(
            &mut document.document.content,
            &page,
            (corner, corner),
            "",
            odox_ui::comments::author().as_deref(),
            &odox_ui::comments::now(),
        ) {
            editing.record_snapshot(before, None);
            self.opened_note = Some(child);
        }
    }

    /// Do what was done in a slide comment's box, as one step to undo.
    fn note(&mut self, child: usize, done: &NoteResponse, editing: &mut Editing) {
        let Some(document) = &mut self.document else {
            return;
        };
        let Some(page) = document
            .slides()
            .get(self.slide)
            .and_then(|slide| document.page_path(slide.position))
        else {
            return;
        };
        let before = document.document.content.clone();
        let content = &mut document.document.content;
        let changed = match done {
            NoteResponse::Saved(text) => edit::set_slide_annotation_text(
                content,
                &page,
                child,
                text,
                odox_ui::comments::author().as_deref(),
                &odox_ui::comments::now(),
            ),
            NoteResponse::Deleted => edit::remove_slide_annotation(content, &page, child),
        };
        if changed.is_ok() {
            editing.record_snapshot(before, None);
        }
    }

    /// Add a slide after the one in view, or duplicate, delete or move that
    /// one, as one step to undo, and follow it: the new slide, the copy, the
    /// slide after a deleted one, the moved slide.
    fn change_slides(&mut self, command: SlideCommand, editing: &mut Editing) {
        let Some(document) = &mut self.document else {
            return;
        };
        let Some(position) = document
            .slides()
            .get(self.slide)
            .map(|slide| slide.position)
        else {
            return;
        };
        let before = document.document.content.clone();
        let done = match command {
            SlideCommand::Add => document.add_slide(position).map(|_| self.slide + 1),
            SlideCommand::Duplicate => document.duplicate_slide(position).map(|_| self.slide + 1),
            SlideCommand::Delete => document
                .delete_slide(position)
                .map(|()| self.slide.min(document.slides().len().saturating_sub(1))),
            SlideCommand::Earlier => document.move_slide(position, false).map(|_| self.slide - 1),
            SlideCommand::Later => document.move_slide(position, true).map(|_| self.slide + 1),
        };
        let Ok(shown) = done else {
            return;
        };
        editing.record_snapshot(before, None);
        self.slide = shown;
        self.picked = None;
        self.drag = None;
        self.page_editor.clear();
        self.found.clear();
    }

    /// The paragraph of a placeholder on the slide in view that holds no text,
    /// as a path from the slide.
    fn empty_placeholder(&self, index: usize) -> Option<Vec<usize>> {
        let document = self.document.as_ref()?;
        let slides = document.slides();
        let (_, shape) = slides
            .get(self.slide)?
            .shapes_indexed()
            .find(|(i, _)| *i == index)?;
        if shape.attr(&Ns::Presentation, "class").is_none() || !shape.plain_text().trim().is_empty()
        {
            return None;
        }
        let (box_at, text_box) = shape
            .elements_indexed()
            .find(|(_, e)| e.is(&Ns::Draw, "text-box"))?;
        let (paragraph_at, _) = text_box
            .elements_indexed()
            .find(|(_, e)| e.is(&Ns::Text, "p"))?;
        Some(vec![index, box_at, paragraph_at])
    }

    /// F5 starts the slideshow at the first slide and Shift+F5 at this one.
    fn start_keys(&mut self, ui: &Ui) {
        let (from_start, from_here) = ui.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::NONE, Key::F5),
                input.consume_key(egui::Modifiers::SHIFT, Key::F5),
            )
        });
        if from_start {
            self.start_show(0);
        } else if from_here {
            self.start_show(self.slide);
        }
    }

    fn start_show(&mut self, from: usize) {
        self.presenting = true;
        self.slide = from;
        self.step = 0;
        self.picked = None;
        self.drag = None;
        self.page_editor.clear();
    }

    /// The slideshow: the slide on a black screen, as large as it will go,
    /// and the keys and the mouse that move through it. Escape ends it.
    fn present(&mut self, ui: &mut Ui, count: usize) {
        let area = ui.max_rect();
        let response = ui.allocate_rect(area, Sense::click());
        let keys = ui.input_mut(|input| {
            let none = egui::Modifiers::NONE;
            let forward = [Key::Space, Key::Enter, Key::ArrowRight, Key::ArrowDown]
                .into_iter()
                .filter(|&key| input.consume_key(none, key))
                .count()
                > 0;
            let back = [Key::Backspace, Key::ArrowLeft, Key::ArrowUp]
                .into_iter()
                .filter(|&key| input.consume_key(none, key))
                .count()
                > 0;
            ShowKeys {
                forward: forward || response.clicked(),
                back: back || response.secondary_clicked(),
                next: input.consume_key(none, Key::PageDown),
                previous: input.consume_key(none, Key::PageUp),
                first: input.consume_key(none, Key::Home),
                last: input.consume_key(none, Key::End),
                leave: input.consume_key(none, Key::Escape),
            }
        });
        if keys.leave {
            self.presenting = false;
            return;
        }
        self.walk_show(keys, count);
        let Some(document) = &self.document else {
            return;
        };
        let slides = document.slides();
        let Some(slide) = slides.get(self.slide) else {
            return;
        };
        let steps = Steps::read(slide.element);
        let layout = document.page_layout(slide);
        let fit = fit(area.size(), &layout);
        let page = Rect::from_center_size(
            area.center(),
            vec2(layout.width.points() * fit, layout.height.points() * fit),
        );
        let palette = odox_ui::format::Palette::for_theme(ui.visuals().dark_mode);
        ui.painter().rect_filled(page, 0.0, palette.paper);
        let mut canvas = Canvas::new(&document.document, &mut self.pictures, page, fit, palette);
        canvas.build = Some((&steps, self.step));
        canvas.background(ui, &document.background(slide));
        for shape in &document.background_objects(slide) {
            canvas.shape(ui, shape);
        }
        for (index, shape) in slide.shapes_indexed() {
            canvas.slide_shape(ui, index, shape);
        }

        // The pointer is in the way of a slide, and is put away when it has
        // been still for a moment.
        let idle = ui.input(|input| input.pointer.time_since_last_movement());
        if idle > 2.0 {
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
        } else {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(2));
        }
    }

    /// Move through a show: forward takes the slide's next build and then
    /// the next slide, back undoes a build and then goes to the slide before
    /// at its last; page up and down, Home and End go from slide to slide.
    fn walk_show(&mut self, keys: ShowKeys, count: usize) {
        let builds = |view: &Self, slide: usize| {
            view.document
                .as_ref()
                .and_then(|document| {
                    document
                        .slides()
                        .get(slide)
                        .map(|slide| Steps::read(slide.element).len())
                })
                .unwrap_or(0)
        };
        let last = count.saturating_sub(1);
        if keys.forward {
            if self.step < builds(self, self.slide) {
                self.step += 1;
            } else if self.slide < last {
                self.show_slide(self.slide + 1);
            }
        }
        if keys.back {
            if self.step > 0 {
                self.step -= 1;
            } else if self.slide > 0 {
                self.show_slide(self.slide - 1);
                self.step = builds(self, self.slide);
            }
        }
        if keys.next {
            self.show_slide((self.slide + 1).min(last));
        }
        if keys.previous && self.slide > 0 {
            self.show_slide(self.slide - 1);
            self.step = builds(self, self.slide);
        }
        if keys.first {
            self.show_slide(0);
        }
        if keys.last {
            self.show_slide(last);
        }
    }

    /// Page up and down, and the arrow keys, move between slides.
    fn step_keys(&mut self, ui: &Ui, count: usize) {
        let mut slide = self.slide;
        ui.input(|input| {
            let forward = input.key_pressed(egui::Key::PageDown)
                || input.key_pressed(egui::Key::ArrowRight)
                || input.key_pressed(egui::Key::ArrowDown);
            let back = input.key_pressed(egui::Key::PageUp)
                || input.key_pressed(egui::Key::ArrowLeft)
                || input.key_pressed(egui::Key::ArrowUp);
            if forward {
                slide = slide.saturating_add(1);
            }
            if back {
                slide = slide.saturating_sub(1);
            }
            if input.key_pressed(egui::Key::Home) {
                slide = 0;
            }
            if input.key_pressed(egui::Key::End) {
                slide = count.saturating_sub(1);
            }
        });
        self.show_slide(slide.min(count.saturating_sub(1)));
    }

    /// Go to the slide a link names, which is the `draw:name` the link
    /// carries after its `#`. A name no slide has leaves the view as it is.
    fn go_to_slide_named(&mut self, name: &str) {
        let Some(index) = self.document.as_ref().and_then(|document| {
            document
                .slides()
                .iter()
                .position(|slide| slide.name == Some(name))
        }) else {
            return;
        };
        self.show_slide(index);
    }

    /// Look at another slide, with nothing picked and no caret. The slide
    /// already shown is left as it is.
    fn show_slide(&mut self, slide: usize) {
        if slide != self.slide {
            self.slide = slide;
            self.step = 0;
            self.picked = None;
            self.drag = None;
            self.page_editor.clear();
            self.stop_placing();
        }
    }

    /// Leave the placing of a text box, which Escape, another slide, another
    /// document and leaving edit mode each do.
    fn stop_placing(&mut self) {
        self.placing = false;
        self.place_from = None;
    }
}

/// The slide's shapes as things to take hold of: the picked one outlined with
/// its corner handles, and what the pointer did this frame.
fn interact(
    ui: &Ui,
    response: &egui::Response,
    page: Rect,
    fit: f32,
    slide: &odox_core::doc::Slide<'_>,
    picked: Option<usize>,
    dragging: bool,
) -> Option<Action> {
    // The slide's own shapes that can be taken hold of, front
    // first, as boxes on screen. A shape placed by a transform, a
    // line placed by its ends, a group and a connector state no
    // corner and are not among them.
    let boxes: Vec<(usize, Rect)> = slide
        .shapes_indexed()
        .filter_map(|(index, shape)| {
            let geometry = box_of(shape)?;
            Some((index, on_screen(page, fit, geometry)))
        })
        .collect();
    let shown = picked.and_then(|index| boxes.iter().find(|(i, _)| *i == index));
    if let Some((_, rect)) = shown {
        let colour = ui.visuals().selection.stroke.color;
        ui.painter()
            .rect_stroke(*rect, 0.0, Stroke::new(1.5, colour), StrokeKind::Outside);
        for corner in corners(*rect) {
            ui.painter().rect_filled(
                Rect::from_center_size(corner, vec2(HANDLE, HANDLE)),
                0.0,
                colour,
            );
        }
    }

    let under = |at: Pos2| {
        boxes
            .iter()
            .rev()
            .find(|(_, rect)| rect.expand(2.0).contains(at))
            .map(|(index, _)| *index)
    };
    // A drag begins where the button went down, not where the pointer was
    // when it had moved far enough to count as a drag: a press on a handle
    // is a press on the handle.
    let pressed = ui
        .input(|input| input.pointer.press_origin())
        .or_else(|| response.interact_pointer_pos());
    if response.drag_started()
        && let Some(at) = pressed
    {
        // A corner of the picked shape first, then whatever shape
        // is under the pointer, then nothing.
        let grabbed = shown.and_then(|(index, rect)| {
            corners(*rect)
                .into_iter()
                .zip([(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)])
                .find(|(corner, _)| corner.distance(at) <= HANDLE)
                .map(|(_, (u, v))| (*index, Grab::Corner(u, v)))
        });
        let grabbed = grabbed.or_else(|| under(at).map(|index| (index, Grab::Whole)));
        return Some(match grabbed {
            Some((shape, grab)) => {
                let start = slide
                    .shapes_indexed()
                    .find(|(i, _)| *i == shape)
                    .and_then(|(_, shape)| box_of(shape));
                match start {
                    Some(start) => Action::Begin(Drag {
                        shape,
                        from: at,
                        grab,
                        start,
                    }),
                    None => Action::Pick(None),
                }
            }
            None => Action::Pick(None),
        });
    } else if dragging
        && response.dragged()
        && let Some(at) = response.interact_pointer_pos()
    {
        return Some(Action::Move(at));
    } else if dragging && response.drag_stopped() {
        return Some(Action::End);
    } else if response.clicked()
        && let Some(at) = response.interact_pointer_pos()
    {
        return Some(Action::Pick(under(at)));
    }
    None
}

/// While a text box is being placed: a drag draws its outline and makes it, a
/// click makes one of ten centimetres by two where it was. Answers the box,
/// in points on the slide.
fn place(
    ui: &Ui,
    response: &egui::Response,
    page: Rect,
    scale: f32,
    from: &mut Option<Pos2>,
) -> Option<Action> {
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    let inside = |at: Pos2| at.clamp(page.min, page.max);
    let to_slide = |at: Pos2| ((at.x - page.min.x) / scale, (at.y - page.min.y) / scale);
    let (slide_width, slide_height) = (page.width() / scale, page.height() / scale);
    let click = |at: Pos2| {
        let (width, height) = ((CM * 10.0).min(slide_width), (CM * 2.0).min(slide_height));
        let (x, y) = to_slide(inside(at));
        Action::Place((
            x.min(slide_width - width).max(0.0),
            y.min(slide_height - height).max(0.0),
            width,
            height,
        ))
    };
    if response.drag_started() {
        *from = ui
            .input(|input| input.pointer.press_origin())
            .or_else(|| response.interact_pointer_pos());
    }
    if let (Some(start), Some(now)) = (*from, response.interact_pointer_pos())
        && response.dragged()
    {
        ui.painter().rect_stroke(
            Rect::from_two_pos(inside(start), inside(now)),
            0.0,
            Stroke::new(1.0, ui.visuals().selection.stroke.color),
            StrokeKind::Inside,
        );
    }
    if response.drag_stopped()
        && let (Some(start), Some(end)) = (from.take(), response.interact_pointer_pos())
    {
        let (a, b) = (to_slide(inside(start)), to_slide(inside(end)));
        let (x, y) = (a.0.min(b.0), a.1.min(b.1));
        let (width, height) = ((a.0 - b.0).abs(), (a.1 - b.1).abs());
        if width > 4.0 && height > 4.0 {
            return Some(Action::Place((x, y, width, height)));
        }
        return Some(click(start));
    }
    if response.clicked()
        && let Some(at) = response.interact_pointer_pos()
    {
        return Some(click(at));
    }
    None
}

/// A centimetre in points.
const CM: f32 = 72.0 / 2.54;

fn fit(space: Vec2, layout: &PageLayout) -> f32 {
    (space.x / layout.width.points())
        .min(space.y / layout.height.points())
        .max(0.01)
}

/// The speaker's notes under the slide.
fn notes_panel(
    ui: &mut Ui,
    document: &Document,
    pictures: &mut Pictures,
    slide: &odox_core::doc::Slide<'_>,
    zoom: f32,
    find: Option<Highlights<'_>>,
) {
    let notes = slide.notes();
    egui::Panel::bottom("notes")
        .default_size(160.0)
        .resizable(true)
        .show(ui, |ui| {
            ui.heading(t("Notes"));
            egui::ScrollArea::vertical().show(ui, |ui| match notes {
                Some(notes) => {
                    let width = ui.available_width();
                    let mut flow = Flow::new(document, pictures, zoom);
                    flow.find = find;
                    flow.marks = marks::shown(ui.ctx());
                    flow.blocks(ui, notes, width);
                }
                None => {
                    ui.weak(t("This slide has no notes."));
                }
            });
        });
}

/// A shape's box in the page's points, where it states one: a corner and a
/// size and no transform. A shape placed by `draw:transform` states its place
/// as operations and is not moved by writing a corner.
fn box_of(shape: &Element) -> Option<Box> {
    if shape.attr(&Ns::Draw, "transform").is_some() || shape.is(&Ns::Draw, "g") {
        return None;
    }
    let at = |local: &str| shape.attr(&Ns::Svg, local).and_then(Length::parse);
    Some(Box {
        x: at("x")?.points(),
        y: at("y")?.points(),
        width: at("width")?.points(),
        height: at("height")?.points(),
    })
}

/// Where a box lands on the screen.
fn on_screen(page: Rect, scale: f32, geometry: Box) -> Rect {
    Rect::from_min_size(
        pos2(
            page.left() + geometry.x * scale,
            page.top() + geometry.y * scale,
        ),
        vec2(geometry.width * scale, geometry.height * scale),
    )
}

/// The four corners, going round from the top left.
fn corners(rect: Rect) -> [Pos2; 4] {
    [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ]
}

/// The first line of a slide's text, for a list that has one row per slide.
fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim();
    if line.chars().count() > 48 {
        let short: String = line.chars().take(47).collect();
        return format!("{short}\u{2026}");
    }
    line.to_owned()
}

#[cfg(test)]
mod tests {
    use super::{Action, CM, Editing, Presentation, ShowKeys, SlideView};

    fn shapes(view: &SlideView) -> usize {
        view.document
            .as_ref()
            .and_then(|document| {
                document
                    .slides()
                    .get(view.slide)
                    .map(|s| s.shapes().count())
            })
            .unwrap_or(0)
    }

    #[test]
    fn undo_removes_inserted_shape() {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/libreoffice/deck.odp"),
        )
        .expect("the deck");
        let mut view = SlideView {
            document: Some(Presentation::read(&bytes).expect("it reads")),
            ..SlideView::default()
        };
        let mut editing = Editing::default();
        editing.reset();
        let before = shapes(&view);
        view.placing = true;
        view.act(
            Action::Place((36.0, 36.0, CM * 10.0, CM * 2.0)),
            1.0,
            &mut editing,
        );
        assert_eq!(shapes(&view), before + 1);
        assert!(!view.placing);
        assert!(view.picked.is_some());
        let document = view.document.as_mut().expect("open");
        editing.undo(&mut document.document, None).expect("a step");
        assert_eq!(shapes(&view), before);
    }

    fn open_deck() -> SlideView {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/libreoffice/deck.odp"),
        )
        .expect("the deck");
        SlideView {
            document: Some(Presentation::read(&bytes).expect("it reads")),
            ..SlideView::default()
        }
    }

    #[test]
    fn placing_stops_on_another_slide() {
        let mut view = open_deck();
        view.placing = true;
        view.show_slide(1);
        assert!(!view.placing);
    }

    #[test]
    fn undo_removes_an_inserted_picture_and_its_part() {
        let mut view = open_deck();
        let mut editing = Editing::default();
        editing.reset();
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../packaging/icons/xodt-rounded-512.png"),
        )
        .expect("a picture");
        let document = view.document.as_mut().expect("open");
        let position = document.slides()[0].position;
        let before = document.document.content.clone();
        let name = document.document.package.picture_name(&bytes, "png");
        let parts = document
            .document
            .package
            .keep(&[name.as_str(), "META-INF/manifest.xml"]);
        document
            .insert_picture(
                position,
                &odox_core::doc::SlidePicture {
                    bytes: &bytes,
                    media_type: "image/png",
                    extension: "png",
                    size: (50.0, 50.0),
                    description: Some("An icon"),
                },
            )
            .expect("put in");
        editing.record_step(before, parts, None);
        assert!(document.document.package.part(&name).is_some());
        editing.undo(&mut document.document, None).expect("a step");
        assert!(document.document.package.part(&name).is_none());
        editing.redo(&mut document.document, None).expect("a step");
        assert!(document.document.package.part(&name).is_some());
    }

    #[test]
    fn a_show_clicks_through_builds_before_the_next_slide() {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/libreoffice/builds.odp"),
        )
        .expect("the fixture");
        let mut view = SlideView {
            document: Some(Presentation::read(&bytes).expect("it reads")),
            ..SlideView::default()
        };
        view.start_show(0);
        let forward = ShowKeys {
            forward: true,
            back: false,
            next: false,
            previous: false,
            first: false,
            last: false,
            leave: false,
        };
        let back = ShowKeys {
            forward: false,
            back: true,
            ..forward
        };
        for step in 1..=5 {
            view.walk_show(forward, 2);
            assert_eq!((view.slide, view.step), (0, step));
        }
        view.walk_show(forward, 2);
        assert_eq!((view.slide, view.step), (1, 0));
        view.walk_show(back, 2);
        assert_eq!((view.slide, view.step), (0, 5));
        view.walk_show(back, 2);
        assert_eq!((view.slide, view.step), (0, 4));
    }
}
