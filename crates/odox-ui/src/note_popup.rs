//! The box a comment, or a note, is read and edited in: who wrote it and when
//! above its text, beside the mark it belongs to.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{self, Id, Rect, Ui};

use crate::i18n::t;

/// What the box shows.
pub struct NoteContent {
    /// The line over the text: who and when.
    pub header: String,
    /// The text, a line to a paragraph.
    pub text: String,
}

/// What was done in the box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoteResponse {
    /// The text was changed to this and saved.
    Saved(String),
    /// The comment is to go.
    Deleted,
}

/// Show the box beside its mark, `anchor`, while it is wanted: while `wanted`
/// says so (the mark is under the pointer, the caret is at it), while the
/// pointer is over the box or its text has the keyboard, and, from when
/// `opened` asks for it, until it is saved, deleted or the pointer is pressed
/// outside it. Editable where `editable` says. Answers what was done in it.
pub fn show(
    ui: &Ui,
    anchor: Rect,
    id: Id,
    content: &NoteContent,
    wanted: bool,
    opened: bool,
    editable: bool,
) -> Option<NoteResponse> {
    let (open, pinned) = (id.with("open"), id.with("pinned"));
    let memory = |key| {
        ui.ctx()
            .data(|data| data.get_temp::<bool>(key))
            .unwrap_or(false)
    };
    if opened {
        ui.ctx().data_mut(|data| data.insert_temp(pinned, true));
    }
    if !(wanted || memory(open) || memory(pinned)) {
        return None;
    }
    let mut done = None;
    let buffer = id.with(("text", content.text.as_str()));
    let area = egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(anchor.right_top())
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(260.0);
                ui.weak(&content.header);
                if editable {
                    let mut text = ui
                        .ctx()
                        .data(|data| data.get_temp::<String>(buffer))
                        .unwrap_or_else(|| content.text.clone());
                    ui.add(
                        egui::TextEdit::multiline(&mut text)
                            .id(field(id))
                            .desired_rows(3)
                            .desired_width(240.0),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(text != content.text, egui::Button::new(t("Save")))
                            .clicked()
                        {
                            done = Some(NoteResponse::Saved(text.trim_end().to_owned()));
                        }
                        if ui.button(t("Delete")).clicked() {
                            done = Some(NoteResponse::Deleted);
                        }
                    });
                    if done.is_some() {
                        ui.ctx().data_mut(|data| data.remove::<String>(buffer));
                    } else {
                        ui.ctx().data_mut(|data| data.insert_temp(buffer, text));
                    }
                } else {
                    ui.label(&content.text);
                }
            });
        });
    let over = area.response.rect.union(anchor);
    let pointer = ui.input(|input| input.pointer.hover_pos());
    let hovered = pointer.is_some_and(|at| over.contains(at));
    let focused = ui.memory(|memory| memory.has_focus(field(id)));
    let pressed_outside = ui.input(|input| input.pointer.any_pressed()) && !hovered;
    ui.ctx().data_mut(|data| {
        data.insert_temp(open, done.is_none() && (hovered || focused));
        if done.is_some() || (pressed_outside && !opened) {
            data.insert_temp(pinned, false);
        }
    });
    done
}

fn field(id: Id) -> Id {
    id.with("field")
}

/// Show a tracked change's box beside it while `wanted` says so or the
/// pointer is over the box: who made it and when, and in edit mode Accept
/// and Reject. Answers `Some(true)` for Accept and `Some(false)` for Reject.
pub fn change(
    ui: &Ui,
    anchor: Rect,
    id: Id,
    content: &NoteContent,
    wanted: bool,
    editable: bool,
) -> Option<bool> {
    let open = id.with("open");
    let was_open = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(open))
        .unwrap_or(false);
    if !(wanted || was_open) {
        return None;
    }
    let mut decided = None;
    let area = egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(anchor.left_bottom())
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(260.0);
                ui.weak(&content.header);
                if !content.text.is_empty() {
                    ui.label(&content.text);
                }
                if editable {
                    ui.horizontal(|ui| {
                        if ui.button(t("Accept")).clicked() {
                            decided = Some(true);
                        }
                        if ui.button(t("Reject")).clicked() {
                            decided = Some(false);
                        }
                    });
                }
            });
        });
    let over = area.response.rect.union(anchor);
    let hovered = ui
        .input(|input| input.pointer.hover_pos())
        .is_some_and(|at| over.contains(at));
    ui.ctx()
        .data_mut(|data| data.insert_temp(open, decided.is_none() && hovered));
    decided
}
