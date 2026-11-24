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

/// Show the box beside `anchor`, editable where `editable` says. Answers
/// what was done in it, and whether the pointer is over it, which keeps it
/// open.
pub fn show(
    ui: &Ui,
    anchor: Rect,
    id: Id,
    content: &NoteContent,
    editable: bool,
) -> (Option<NoteResponse>, bool) {
    let mut done = None;
    let buffer = id.with("text");
    let area = egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(anchor.right_top() + egui::vec2(6.0, 0.0))
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
    let hovered = area.response.rect.contains(
        ui.input(|input| input.pointer.hover_pos())
            .unwrap_or(egui::Pos2::new(f32::NAN, f32::NAN)),
    );
    let focused = ui.memory(|memory| memory.has_focus(field(id)));
    (done, hovered || focused)
}

fn field(id: Id) -> Id {
    id.with("field")
}
