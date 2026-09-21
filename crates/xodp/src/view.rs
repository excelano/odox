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

use eframe::egui::{self, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use odox_core::doc::Presentation;
use odox_core::{Document, Element, Length, Ns};
use odox_ui::i18n::{fill, t};
use odox_ui::{Canvas, Editing, Flow, Pictures, View, fonts};

/// A presentation, open or not.
#[derive(Default)]
pub struct SlideView {
    document: Option<Presentation>,
    slide: usize,
    pictures: Pictures,
    show_notes: bool,
    /// The shape picked on the slide, by its index among the page's children.
    picked: Option<usize>,
    /// A drag in progress on the picked shape.
    drag: Option<Drag>,
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

/// What the slide asked for this frame, applied once the document is free.
enum Action {
    Pick(Option<usize>),
    Begin(Drag),
    Move(Pos2),
    End,
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
        ctx.set_fonts(fonts::definitions(&fonts::families_used(&parts)));

        self.pictures.clear();
        self.document = Some(document);
        self.slide = 0;
        self.picked = None;
        self.drag = None;
        Ok(())
    }

    fn close(&mut self) {
        self.document = None;
        self.pictures.clear();
        self.picked = None;
        self.drag = None;
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
    }

    fn central(&mut self, ui: &mut Ui, zoom: f32, editing: &mut Editing) {
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
        if ui.input(|input| input.key_pressed(Key::Escape)) {
            self.picked = None;
            self.drag = None;
        }

        // The document and the picture cache are taken as separate borrows of
        // separate fields, which is what lets the renderer hold one while filling
        // the other.
        let slide_index = self.slide;
        let show_notes = self.show_notes;
        let edit_mode = editing.on && !editing.asking;
        let picked = self.picked;
        let dragging = self.drag.is_some();
        let mut action = None;
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
                let sense = if edit_mode {
                    Sense::click_and_drag()
                } else {
                    Sense::hover()
                };
                let (page, response) = ui.allocate_exact_size(size, sense);
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

                if edit_mode {
                    action = interact(ui, &response, page, fit, slide, picked, dragging);
                }
            });
        });

        if let Some(action) = action {
            self.act(action, fit, editing);
        }
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
            if ui.selectable_label(index == self.slide, label).clicked() && index != self.slide {
                self.slide = index;
                self.picked = None;
                self.drag = None;
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
    /// Apply what the slide asked for.
    fn act(&mut self, action: Action, scale: f32, editing: &mut Editing) {
        match action {
            Action::Pick(shape) => {
                self.picked = shape;
                self.drag = None;
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
        let slide = slide.min(count.saturating_sub(1));
        if slide != self.slide {
            self.slide = slide;
            self.picked = None;
            self.drag = None;
        }
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
