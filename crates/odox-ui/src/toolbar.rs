//! The row of buttons over a page being edited. DESIGN.md §11.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{Key, KeyboardShortcut, Modifiers, RichText, Ui};
use egui_richedit::{Mark, Model};

use crate::PageEditor;
use crate::i18n::t;

/// Bold, italic, underline and strikethrough, each lit where the whole
/// selection has it, and none of them live without a caret. Answers the mark
/// whose button was pressed, for the editor to toggle over the model.
pub fn marks<M: Model<Paragraph = Vec<usize>>>(
    ui: &mut Ui,
    editor: &PageEditor,
    model: &M,
) -> Option<Mark> {
    let mut pressed = None;
    ui.add_enabled_ui(editor.selection().is_some(), |ui| {
        ui.horizontal(|ui| {
            for mark in Mark::ALL {
                let lit = editor.marked(model, mark) == Some(true);
                let (label, name, key) = match mark {
                    Mark::Bold => (RichText::new(t("B")).strong(), t("Bold"), Some(Key::B)),
                    Mark::Italic => (RichText::new(t("I")).italics(), t("Italic"), Some(Key::I)),
                    Mark::Underline => (
                        RichText::new(t("U")).underline(),
                        t("Underline"),
                        Some(Key::U),
                    ),
                    Mark::Strike => (
                        RichText::new(t("S")).strikethrough(),
                        t("Strikethrough"),
                        None,
                    ),
                };
                let hint = match key {
                    Some(key) => format!(
                        "{name} ({})",
                        ui.ctx()
                            .format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, key))
                    ),
                    None => name.to_owned(),
                };
                if ui
                    .selectable_label(lit, label)
                    .on_hover_text(hint)
                    .clicked()
                {
                    pressed = Some(mark);
                }
            }
        });
    });
    pressed
}
