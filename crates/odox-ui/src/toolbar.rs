//! The row of buttons over a page being edited. DESIGN.md §11.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText, Ui};
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

/// A mark's name, and the key that toggles it with Ctrl, or Command on a Mac.
/// Strikethrough has no shortcut that people share, so it has none.
fn mark_facts(mark: Mark) -> (&'static str, Option<Key>) {
    match mark {
        Mark::Bold => (t("Bold"), Some(Key::B)),
        Mark::Italic => (t("Italic"), Some(Key::I)),
        Mark::Underline => (t("Underline"), Some(Key::U)),
        Mark::Strike => (t("Strikethrough"), None),
    }
}

fn mark_buttons<M: Model<Paragraph = Vec<usize>>>(
    ui: &mut Ui,
    editor: &PageEditor,
    model: &M,
) -> Option<Mark> {
    let mut pressed = None;
    for mark in Mark::ALL {
        let lit = editor.marked(model, mark) == Some(true);
        let (name, key) = mark_facts(mark);
        let label = match mark {
            Mark::Bold => RichText::new(t("B")).strong(),
            Mark::Italic => RichText::new(t("I")).italics(),
            Mark::Underline => RichText::new(t("U")).underline(),
            Mark::Strike => RichText::new(t("S")).strikethrough(),
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

/// What the Format menu asks of the page: one of the buttons' commands.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Bold, italic, underline or strikethrough.
    Mark(Mark),
    /// A kind of paragraph.
    Block(Block),
}

/// The kinds of paragraph, in the order the toolbar and the menu offer them.
const BLOCKS: [Block; 6] = [
    Block::Body,
    Block::Heading(1),
    Block::Heading(2),
    Block::Heading(3),
    Block::Bullets,
    Block::Numbers,
];

fn block_name(block: Block) -> &'static str {
    match block {
        Block::Body => t("Body text"),
        Block::Heading(1) => t("Heading 1"),
        Block::Heading(2) => t("Heading 2"),
        Block::Heading(_) => t("Heading 3"),
        Block::Bullets => t("Bulleted list"),
        Block::Numbers => t("Numbered list"),
    }
}

/// What the Format menu draws itself from: what the page said the last time it
/// was drawn, because a menu is drawn before the page is.
#[derive(Default)]
pub struct MenuState {
    enabled: bool,
    blocks: bool,
    marks: Vec<Mark>,
    kinds: Vec<Block>,
}

impl MenuState {
    /// The state of a page over a model. `blocks` says whether the page has
    /// kinds of paragraph to offer, which a slide's labels do not.
    pub fn of(editor: &PageEditor, model: &FlowModel<'_>, blocks: bool) -> Self {
        let Some(selection) = editor.selection() else {
            return Self {
                blocks,
                ..Self::default()
            };
        };
        Self {
            enabled: true,
            blocks,
            marks: Mark::ALL
                .into_iter()
                .filter(|mark| editor.marked(model, *mark) == Some(true))
                .collect(),
            kinds: if blocks {
                BLOCKS
                    .into_iter()
                    .filter(|block| model.is_block(selection, *block))
                    .collect()
            } else {
                Vec::new()
            },
        }
    }

    /// A menu for a view that has paragraphs to offer or not, before its page
    /// has been drawn once.
    pub fn offering_blocks(blocks: bool) -> Self {
        Self {
            blocks,
            ..Self::default()
        }
    }
}

/// The Format menu's entries: the marks, each with its shortcut, and where the
/// page has them the kinds of paragraph. Lit where the whole selection is one,
/// and not live without a caret. Answers the command chosen.
pub fn menu(ui: &mut Ui, state: &MenuState) -> Option<Command> {
    let mut chosen = None;
    for mark in Mark::ALL {
        let (name, key) = mark_facts(mark);
        let mut button = egui::Button::new(name).selected(state.marks.contains(&mark));
        if let Some(key) = key {
            button = button.shortcut_text(
                ui.ctx()
                    .format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, key)),
            );
        }
        if ui.add_enabled(state.enabled, button).clicked() {
            chosen = Some(Command::Mark(mark));
            ui.close();
        }
    }
    if state.blocks {
        ui.separator();
        for block in BLOCKS {
            let button =
                egui::Button::new(block_name(block)).selected(state.kinds.contains(&block));
            if ui.add_enabled(state.enabled, button).clicked() {
                chosen = Some(Command::Block(block));
                ui.close();
            }
        }
    }
    chosen
}
