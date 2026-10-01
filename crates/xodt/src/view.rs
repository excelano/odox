//! The window's one view: a document's flow, and its headings beside it.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use eframe::egui::{self, Ui};
use egui_richedit::Selection;
use odox_core::Document;
use odox_core::doc::TextDocument;
use odox_ui::find::in_paragraphs;
use odox_ui::i18n::{fill, t};
use odox_ui::{
    Caret, Editing, Flow, FlowModel, Found, PageEditor, Pictures, View, fonts, page_editor,
};

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

    fn close(&mut self) {
        self.document = None;
        self.pictures.clear();
        self.page.clear();
        self.found.clear();
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

    fn central(&mut self, ui: &mut Ui, zoom: f32, editing: &mut Editing) {
        // While a question is up the page is drawn as it is and takes no keys;
        // the caret waits for it to be answered.
        let edit_mode = editing.on && !editing.asking;
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
        let page = document.text_width() * zoom;
        let margin = 24.0 * zoom;

        if edit_mode && let Some(root) = document.body_path() {
            let Document {
                content, styles, ..
            } = &mut document.document;
            let mut model = FlowModel::new(content, styles, root, editing);
            if let Some(mark) = odox_ui::toolbar::marks(ui, &self.page, &model) {
                self.page.toggle(&mut model, mark);
            }
            ui.separator();
        }

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // What was typed goes into the tree before it is laid out, so
                // this frame already draws it.
                if edit_mode && let Some(root) = document.body_path() {
                    let Document {
                        content, styles, ..
                    } = &mut document.document;
                    let mut model = FlowModel::new(content, styles, root, editing);
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
                            let mut flow = Flow::new(&document.document, &mut self.pictures, zoom);
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
        self.found.drawn();
    }

    fn side(&mut self, ui: &mut Ui) -> bool {
        let Some(document) = &self.document else {
            return false;
        };
        let outline = document.outline();
        if outline.is_empty() {
            return false;
        }
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
        true
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
        ctx.set_fonts(fonts::definitions(&fonts::families_used(&parts)));
    }
}
