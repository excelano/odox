//! The window's one view: a deck's slides, and the one being looked at.
//!
//! A slide is drawn at the geometry the document gives: every shape on a page
//! carries its own position and size in the page's coordinate space, so the page
//! is scaled to the space the window has and each shape is put where the document
//! says. What this release draws is the text on a slide. Its shapes — rectangles,
//! lines, connectors — and the background and placeholder geometry a master page
//! contributes are the next release's, and a shape that is not text is drawn as
//! the outline of the space it occupies so that a slide is not silently missing
//! something.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use eframe::egui::{self, Sense, Stroke, StrokeKind, Ui, vec2};
use odox_core::doc::Presentation;
use odox_ui::i18n::{fill, t};
use odox_ui::{Canvas, Flow, Pictures, Viewer, fonts};

/// A presentation, open or not.
#[derive(Default)]
pub struct SlideView {
    document: Option<Presentation>,
    slide: usize,
    pictures: Pictures,
    show_notes: bool,
}

impl Viewer for SlideView {
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
        ctx.set_fonts(fonts::definitions(&fonts::families_used(&parts)));

        self.pictures.clear();
        self.document = Some(document);
        self.slide = 0;
        Ok(())
    }

    fn close(&mut self) {
        self.document = None;
        self.pictures.clear();
    }

    fn is_open(&self) -> bool {
        self.document.is_some()
    }

    fn title(&self) -> Option<String> {
        self.document.as_ref()?.document.meta.title.clone()
    }

    fn central(&mut self, ui: &mut Ui, zoom: f32) {
        let count = self
            .document
            .as_ref()
            .map_or(0, |document| document.slides().len());
        if count == 0 {
            ui.centered_and_justified(|ui| {
                ui.weak(t("This presentation has no slides."));
            });
            return;
        }
        self.step_keys(ui, count);

        // The document and the picture cache are taken as separate borrows of
        // separate fields, which is what lets the renderer hold one while filling
        // the other.
        let slide_index = self.slide;
        let show_notes = self.show_notes;
        let Some(document) = &self.document else {
            return;
        };
        let pictures = &mut self.pictures;
        let slides = document.slides();
        let Some(slide) = slides.get(slide_index) else {
            return;
        };

        if show_notes {
            let notes = slide.notes();
            egui::Panel::bottom("notes")
                .default_size(160.0)
                .resizable(true)
                .show(ui, |ui| {
                    ui.heading(t("Notes"));
                    egui::ScrollArea::vertical().show(ui, |ui| match notes {
                        Some(notes) => {
                            let width = ui.available_width();
                            Flow::new(&document.document, pictures, zoom).blocks(ui, notes, width);
                        }
                        None => {
                            ui.weak(t("This slide has no notes."));
                        }
                    });
                });
        }

        let layout = document.page_layout(slide);
        let available = ui.available_size();
        // The page scaled to fit, keeping its shape, and then the window's own
        // zoom on top of that.
        let fit = (available.x / layout.width.points())
            .min(available.y / layout.height.points())
            .max(0.01)
            * zoom;
        let size = vec2(layout.width.points() * fit, layout.height.points() * fit);

        // Both resolved before the closure, which borrows the document again.
        let background = document.background(slide);
        let decorations = document.background_objects(slide);

        egui::ScrollArea::both().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                let (page, _) = ui.allocate_exact_size(size, Sense::hover());
                let palette = odox_ui::format::Palette::default();
                // Paper under everything. A slide whose background is `none` —
                // which is what a template says when its identity is the shapes
                // rather than the ground — is drawn on paper and not on the
                // window's own colour. `odox_ui::format::Palette` says why.
                ui.painter().rect_filled(page, 2.0, palette.paper);

                let mut canvas = Canvas {
                    document: &document.document,
                    pictures,
                    page,
                    scale: fit,
                    palette,
                };
                // Back to front: the ground, then what the master page draws on
                // every slide, then the slide's own.
                canvas.background(ui, &background);
                for shape in &decorations {
                    canvas.shape(ui, shape);
                }
                for shape in slide.shapes() {
                    canvas.shape(ui, shape);
                }

                // The page's edge last, so a decoration running to the bleed
                // does not paint over it.
                ui.painter().rect_stroke(
                    page,
                    2.0,
                    Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                    StrokeKind::Inside,
                );
            });
        });
    }

    fn side(&mut self, ui: &mut Ui) -> bool {
        let Some(document) = &self.document else {
            return false;
        };
        let slides = document.slides();
        if slides.is_empty() {
            return false;
        }
        ui.heading(t("Slides"));
        ui.separator();
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
                self.slide = index;
            }
        }
        true
    }

    fn view_menu(&mut self, ui: &mut Ui) {
        ui.separator();
        ui.checkbox(&mut self.show_notes, t("Show the speaker's notes"));
    }
}

impl SlideView {
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
        self.slide = slide.min(count.saturating_sub(1));
    }
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
