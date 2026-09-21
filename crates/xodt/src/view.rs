//! The window's one view: a document's flow, and its headings beside it.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use eframe::egui::{self, Ui};
use odox_core::doc::TextDocument;
use odox_core::{Document, edit};
use odox_ui::i18n::{fill, t};
use odox_ui::{Editing, Flow, ParagraphEditor, ParagraphOutcome, Pictures, View, fonts};

/// A text document, open or not.
#[derive(Default)]
pub struct TextView {
    document: Option<TextDocument>,
    pictures: Pictures,
    /// The heading the outline asked to be shown, answered on the next frame
    /// while the body is laid out.
    scroll_to: Option<usize>,
    /// The paragraph being typed into, while one is.
    editor: Option<ParagraphEditor>,
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
        self.editor = None;
        self.load_fonts(ctx);
        Ok(())
    }

    fn close(&mut self) {
        self.document = None;
        self.pictures.clear();
        self.editor = None;
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
        self.editor = None;
    }

    fn central(&mut self, ui: &mut Ui, zoom: f32, editing: &mut Editing) {
        let edit_mode = editing.on && !editing.asking;
        let mut clicked = None;
        let mut outcome = None;
        let Some(document) = &self.document else {
            return;
        };
        let Some(body) = document.body() else {
            ui.centered_and_justified(|ui| {
                ui.weak(t("This document has no text in it."));
            });
            return;
        };

        // The page's text width, which is what the document's own lines were
        // written to. Narrower than the window, the flow is drawn on a page the
        // width the document asked for; wider, the page shrinks to fit rather
        // than being cut off.
        let page = document.text_width() * zoom;
        let margin = 24.0 * zoom;

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    let width = page.min(ui.available_width() - margin * 2.0).max(80.0);
                    egui::Frame::new()
                        .fill(odox_ui::format::Palette::default().paper)
                        .inner_margin(margin)
                        .show(ui, |ui| {
                            ui.set_width(width);
                            let mut flow = Flow::new(&document.document, &mut self.pictures, zoom);
                            flow.scroll_to_heading = self.scroll_to.take();
                            flow.edit_mode = edit_mode;
                            flow.editor = self.editor.as_mut();
                            flow.blocks(ui, body, width);
                            clicked = flow.clicked.take();
                            outcome = flow.outcome.take();
                        });
                });
            });

        if let Some(outcome) = outcome {
            self.finish_editing(outcome, editing);
        } else if let Some(path) = clicked
            && self.editor.is_none()
        {
            self.begin_editing(path);
        }
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
    /// Open the text box on the paragraph at a path from the body.
    fn begin_editing(&mut self, path: Vec<usize>) {
        let Some(paragraph) = self
            .document
            .as_ref()
            .and_then(TextDocument::body)
            .and_then(|body| body.at(&path))
        else {
            return;
        };
        self.editor = Some(ParagraphEditor::open(path, edit::text(paragraph), None));
    }

    /// Close the text box, writing what was typed where it was kept.
    ///
    /// A join first keeps what was typed, then joins the paragraph onto the
    /// one before it and opens the box again there, with the caret at the
    /// join, so that Backspace at the start of a paragraph reads as it does
    /// in any editor.
    fn finish_editing(&mut self, outcome: ParagraphOutcome, editing: &mut Editing) {
        let Some(editor) = self.editor.take() else {
            return;
        };
        if outcome == ParagraphOutcome::Cancel {
            return;
        }
        let Some(document) = &mut self.document else {
            return;
        };
        let mut recorded = false;
        if editor.changed() {
            editing.record(&document.document.content);
            recorded = true;
            let Some(body) = document.body_mut() else {
                return;
            };
            // What was typed may have become several paragraphs; the one a
            // join wants is the first of them, at the same path.
            let _ = edit::apply(body, &editor.path, &editor.text);
        }
        if outcome == ParagraphOutcome::JoinPrevious {
            if !recorded {
                editing.record(&document.document.content);
            }
            let Some(body) = document.body_mut() else {
                return;
            };
            let previous_len = body
                .at(&editor.path[..editor.path.len() - 1])
                .and_then(|parent| {
                    parent.children[..*editor.path.last()?]
                        .iter()
                        .rev()
                        .find_map(|node| match node {
                            odox_core::Node::Element(e)
                                if e.is(&odox_core::Ns::Text, "p")
                                    || e.is(&odox_core::Ns::Text, "h") =>
                            {
                                Some(edit::text(e).chars().count())
                            }
                            _ => None,
                        })
                });
            if let Ok(joined) = edit::join_with_previous(body, &editor.path)
                && let Some(paragraph) = body.at(&joined)
            {
                self.editor = Some(ParagraphEditor::open(
                    joined,
                    edit::text(paragraph),
                    previous_len,
                ));
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
        ctx.set_fonts(fonts::definitions(&fonts::families_used(&parts)));
    }
}
