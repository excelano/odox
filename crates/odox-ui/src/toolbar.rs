//! The row of buttons over a page being edited. DESIGN.md §11.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{Key, KeyboardShortcut, Modifiers, RichText, Ui};
use egui_richedit::{Mark, Model};

use crate::i18n::t;
use crate::{Block, FlowModel, PageEditor};

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
        ui.horizontal(|ui| pressed = mark_buttons(ui, editor, model));
    });
    pressed
}

/// What was pressed on a text document's toolbar.
#[derive(Default)]
pub struct Pressed {
    /// A mark's button, for the editor to toggle over the model.
    pub mark: Option<Mark>,
    /// A kind of paragraph's button, for the model to apply.
    pub block: Option<Block>,
}

/// The marks, and after them the kinds of paragraph: body text, three
/// levels of heading, a bulleted and a numbered list. Each is lit where every
/// paragraph the selection runs over is one, and pressing a lit one takes it
/// off.
pub fn text(ui: &mut Ui, editor: &PageEditor, model: &FlowModel<'_>) -> Pressed {
    let mut pressed = Pressed::default();
    ui.add_enabled_ui(editor.selection().is_some(), |ui| {
        ui.horizontal(|ui| {
            pressed.mark = mark_buttons(ui, editor, model);
            ui.separator();
            for (block, label, name) in [
                (Block::Body, "\u{b6}", t("Body text")),
                (Block::Heading(1), "H1", t("Heading 1")),
                (Block::Heading(2), "H2", t("Heading 2")),
                (Block::Heading(3), "H3", t("Heading 3")),
                (Block::Bullets, "\u{2022}", t("Bulleted list")),
                (Block::Numbers, "1.", t("Numbered list")),
            ] {
                let lit = editor
                    .selection()
                    .is_some_and(|selection| model.is_block(selection, block));
                if ui
                    .selectable_label(lit, label)
                    .on_hover_text(name)
                    .clicked()
                {
                    pressed.block = Some(block);
                }
            }
        });
    });
    pressed
}

fn mark_buttons<M: Model<Paragraph = Vec<usize>>>(
    ui: &mut Ui,
    editor: &PageEditor,
    model: &M,
) -> Option<Mark> {
    let mut pressed = None;
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
    pressed
}
