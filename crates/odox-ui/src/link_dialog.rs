//! The question that makes or changes a link: its text, its address, typed
//! or picked from the places in the document, and, for a link already there,
//! taking it off.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{self, Key};

use crate::i18n::t;
use crate::{FlowModel, LinkHere, PageEditor};

/// What the link question was answered with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkAnswer {
    /// Make or change the link: its text and its address.
    Set(String, String),
    /// Take the link off.
    Remove,
    /// Leave things as they were.
    Cancel,
}

/// The link question, while it is up.
pub struct LinkDialog {
    text: String,
    address: String,
    /// The places in the document a link may go to: what a person reads in
    /// the list, and the address.
    places: Vec<(String, String)>,
    editing: bool,
    opened: bool,
}

impl LinkDialog {
    /// A question starting from a link's text and address, which are empty
    /// for a new one, offering the document's places; `editing` where a link
    /// is already there to change or take off.
    pub fn new(
        text: String,
        address: String,
        places: Vec<(String, String)>,
        editing: bool,
    ) -> Self {
        Self {
            text,
            address,
            places,
            editing,
            opened: true,
        }
    }

    /// Draw the question, answering once it is answered.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<LinkAnswer> {
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("link-question")).show(ctx, |ui| {
            ui.heading(if self.editing {
                t("Edit link")
            } else {
                t("Insert link")
            });
            egui::Grid::new("link-fields")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label(t("Text"));
                    ui.add(egui::TextEdit::singleline(&mut self.text).desired_width(320.0));
                    ui.end_row();
                    ui.label(t("Address"));
                    let address = ui.add(
                        egui::TextEdit::singleline(&mut self.address)
                            .hint_text("https://")
                            .desired_width(320.0),
                    );
                    if std::mem::take(&mut self.opened) {
                        address.request_focus();
                    }
                    ui.end_row();
                    if !self.places.is_empty() {
                        ui.label(t("Place in the document"));
                        let chosen = self
                            .places
                            .iter()
                            .find(|(_, href)| *href == self.address)
                            .map_or_else(String::new, |(shown, _)| shown.clone());
                        egui::ComboBox::from_id_salt("link-place")
                            .selected_text(chosen)
                            .width(320.0)
                            .show_ui(ui, |ui| {
                                for (shown, href) in &self.places {
                                    ui.selectable_value(&mut self.address, href.clone(), shown);
                                }
                            });
                        ui.end_row();
                    }
                });
            let followable = odox_core::edit::followable(self.address.trim());
            if !self.address.trim().is_empty() && !followable {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    t("A link goes to a web page, a mail address or a place in the document."),
                );
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let ready = followable && !self.text.is_empty();
                let entered = ui.input(|i| i.key_pressed(Key::Enter));
                if ui.add_enabled(ready, egui::Button::new(t("OK"))).clicked() || (ready && entered)
                {
                    answer = Some(LinkAnswer::Set(
                        self.text.clone(),
                        self.address.trim().to_owned(),
                    ));
                }
                if ui.button(t("Cancel")).clicked()
                    || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape))
                {
                    answer = Some(LinkAnswer::Cancel);
                }
                if self.editing && ui.button(t("Remove link")).clicked() {
                    answer = Some(LinkAnswer::Remove);
                }
            });
        });
        answer
    }
}

/// The link question in a window: up while it is asked, and its answer until
/// the page makes it.
#[derive(Default)]
pub struct LinkQuestion {
    asking: Option<(LinkDialog, LinkHere)>,
    answered: Option<(LinkHere, LinkAnswer)>,
}

impl LinkQuestion {
    /// Whether the question is up.
    pub fn is_up(&self) -> bool {
        self.asking.is_some()
    }

    /// Draw the question while it is up, keeping its answer.
    pub fn ask(&mut self, ctx: &egui::Context) {
        let Some((dialog, _)) = &mut self.asking else {
            return;
        };
        if let Some(answer) = dialog.show(ctx)
            && let Some((_, here)) = self.asking.take()
        {
            self.answered = Some((here, answer));
            ctx.request_repaint();
        }
    }

    /// Put the question up where it was asked for, by Ctrl+K or by `wanted`,
    /// and make what it was answered with. Answers what went wrong, for the
    /// window to say.
    pub fn act<'m>(
        &mut self,
        ui: &egui::Ui,
        model: &mut FlowModel<'m>,
        editor: &mut PageEditor,
        wanted: bool,
        places: impl FnOnce(&FlowModel<'m>) -> Vec<(String, String)>,
    ) -> Option<String> {
        let mut problem = None;
        let ctrl_k = editor.has_focus(ui.ctx())
            && ui.input_mut(|i| {
                i.consume_shortcut(&egui::KeyboardShortcut::new(
                    egui::Modifiers::COMMAND,
                    Key::K,
                ))
            });
        if (wanted || ctrl_k) && editor.selection().is_none() {
            problem = Some(t("Put the caret where the link goes.").to_owned());
        } else if wanted || ctrl_k {
            match model.link_here(editor) {
                Some(here) => {
                    let dialog = LinkDialog::new(
                        here.text.clone(),
                        here.href.clone(),
                        places(model),
                        here.link.is_some(),
                    );
                    self.asking = Some((dialog, here));
                }
                None => problem = Some(t("A link is made within one paragraph.").to_owned()),
            }
        }
        if let Some((here, answer)) = self.answered.take() {
            let done = match answer {
                LinkAnswer::Set(text, href) => model.put_link(editor, &here, &text, &href),
                LinkAnswer::Remove => model.take_link(&here),
                LinkAnswer::Cancel => Ok(()),
            };
            if let Err(refused) = done {
                problem = Some(refused_link(&refused));
            }
        }
        problem
    }
}

/// Why a link was not made, changed or taken off, in words.
fn refused_link(refused: &odox_core::Refused) -> String {
    match refused {
        odox_core::Refused::Address => {
            t("A link goes to a web page, a mail address or a place in the document.").to_owned()
        }
        odox_core::Refused::Structure => {
            t("A link cannot run into another link, and needs text to show.").to_owned()
        }
        odox_core::Refused::NotFound => {
            t("The text changed while the question was up. Ask again.").to_owned()
        }
        other => crate::refusal::message(other),
    }
}
