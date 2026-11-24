//! The row of buttons over a page being edited. DESIGN.md §11.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, Response, RichText, Ui, vec2};
use egui_richedit::{Mark, Model, Property, Script, Value};
use odox_core::TextAlign;
use odox_core::edit::{LineSpacing, TableEdit};

use crate::color_picker::{self, Kind};
use crate::i18n::{fill, t};
use crate::{Block, FlowModel, PageEditor, fonts};

/// What was pressed on the toolbar over a page.
#[derive(Default)]
pub struct Pressed {
    /// A mark's button, for the editor to toggle over the model.
    pub mark: Option<Mark>,
    /// A family or a size picked, for the editor to give the selection.
    pub value: Option<Value>,
    /// An alignment's button, for the model to apply.
    pub align: Option<TextAlign>,
    /// A kind of paragraph's button, for the model to apply.
    pub block: Option<Block>,
    /// A heading's key to move it up the outline (below zero) or down it.
    pub outline: Option<i8>,
    /// An indent button: a tab's width in, above zero, or out.
    pub indent: Option<i8>,
    /// A line spacing picked.
    pub spacing: Option<LineSpacing>,
    /// The table button, which asks how many rows and columns.
    pub table: bool,
    /// The picture button, which asks for a file.
    pub picture: bool,
}

/// The buttons over a slide's label: the marks and the alignments, none of
/// them live without a caret.
pub fn label(ui: &mut Ui, editor: &PageEditor, model: &FlowModel<'_>) -> Pressed {
    let mut pressed = Pressed::default();
    ui.add_enabled_ui(editor.selection().is_some(), |ui| {
        ui.horizontal_wrapped(|ui| text_buttons(ui, editor, model, &mut pressed));
    });
    pressed
}

/// Raised or lowered, or back on the line where the selection already is.
fn toggled(editor: &PageEditor, model: &FlowModel<'_>, script: Script) -> Value {
    if editor.value(model, Property::Script) == Some(Value::Script(script)) {
        Value::Script(Script::Normal)
    } else {
        Value::Script(script)
    }
}

/// The family and the size the selection has, blank where it differs; the
/// marks, each lit where the whole selection has it; and the alignments, lit
/// where every paragraph the selection runs over is aligned that way.
fn text_buttons(ui: &mut Ui, editor: &PageEditor, model: &FlowModel<'_>, pressed: &mut Pressed) {
    pressed.value = family_box(ui, editor, model).or(size_box(ui, editor, model));
    for (key, larger) in [(Key::CloseBracket, true), (Key::OpenBracket, false)] {
        if ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, key)) {
            pressed.value = stepped(editor.value(model, Property::Size).as_ref(), larger);
        }
    }
    if editor.has_focus(ui.ctx()) {
        for (key, script) in [(Key::P, Script::Super), (Key::B, Script::Sub)] {
            if ui.input_mut(|input| input.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, key)) {
                pressed.value = Some(toggled(editor, model, script));
            }
        }
    }
    ui.separator();
    pressed.mark = mark_buttons(ui, editor, model);
    if let Some(color) = color_picker::button(ui, Kind::Ink) {
        pressed.value = Some(Value::Color(color));
    }
    if let Some(color) = color_picker::button(ui, Kind::Highlight) {
        pressed.value = Some(Value::Highlight(color));
    }
    ui.separator();
    let current = editor
        .selection()
        .and_then(|selection| model.alignment(selection));
    for align in ALIGNMENTS {
        if align_button(ui, current == Some(align), align)
            .on_hover_text(align_name(align))
            .clicked()
        {
            pressed.align = Some(align);
        }
    }
    ui.separator();
    for (step, label, name) in [
        (-1, "\u{21e4}", t("Decrease indent")),
        (1, "\u{21e5}", t("Increase indent")),
    ] {
        if ui.button(label).on_hover_text(name).clicked() {
            pressed.indent = Some(step);
        }
    }
    let current = editor
        .selection()
        .and_then(|selection| model.line_spacing(selection));
    egui::ComboBox::from_id_salt("line-spacing")
        .width(48.0)
        .selected_text(current.map_or("", spacing_label))
        .show_ui(ui, |ui| {
            for spacing in SPACINGS {
                if ui
                    .selectable_label(current == Some(spacing), spacing_label(spacing))
                    .clicked()
                {
                    pressed.spacing = Some(spacing);
                }
            }
        })
        .response
        .on_hover_text(t("Line spacing"));
}

/// The line spacings offered, closest first.
const SPACINGS: [LineSpacing; 4] = [
    LineSpacing::Single,
    LineSpacing::OnePointOneFive,
    LineSpacing::OnePointFive,
    LineSpacing::Double,
];

fn spacing_label(spacing: LineSpacing) -> &'static str {
    match spacing {
        LineSpacing::Single => "1",
        LineSpacing::OnePointOneFive => "1.15",
        LineSpacing::OnePointFive => "1.5",
        LineSpacing::Double => "2",
    }
}

/// The families a list offers: the document's own, then the machine's.
/// Answers the one picked.
fn family_box(ui: &mut Ui, editor: &PageEditor, model: &FlowModel<'_>) -> Option<Value> {
    let current = match editor.value(model, Property::Family) {
        Some(Value::Family(family)) => Some(family),
        _ => None,
    };
    let mut picked = None;
    let mut offer = |ui: &mut Ui, family: &str| {
        if ui
            .selectable_label(current.as_deref() == Some(family), family)
            .clicked()
        {
            fonts::add_family(ui.ctx(), family);
            picked = Some(Value::Family(family.to_owned()));
        }
    };
    egui::ComboBox::from_id_salt("family")
        .width(150.0)
        .height(400.0)
        .selected_text(current.clone().unwrap_or_default())
        .show_ui(ui, |ui| {
            for family in model.document_families() {
                offer(ui, &family);
            }
            ui.separator();
            for family in fonts::system_families() {
                offer(ui, family);
            }
        })
        .response
        .on_hover_text(t("Font"));
    picked
}

/// The sizes offered beside the size box, in points.
const SIZES: [f32; 16] = [
    8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0, 36.0, 48.0, 72.0,
];

/// A size in twentieths of a point, as the editor carries one.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn twentieths(points: f32) -> u32 {
    (points * 20.0).round() as u32
}

/// A size in twentieths of a point as a person reads it, in points.
fn shown(twentieths: u32) -> String {
    let points = format!("{:.2}", f64::from(twentieths) / 20.0);
    points
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

/// A size as a person types it, `12`, `10.5`, `10,5` or `12pt`, between one
/// point and a thousand.
fn typed_size(text: &str) -> Option<u32> {
    let text = text.trim().trim_end_matches("pt").trim().replace(',', ".");
    let points: f32 = text.parse().ok()?;
    (1.0..1000.0).contains(&points).then(|| twentieths(points))
}

/// The next size offered above or below a size, or none where there is no
/// one size to step from.
fn stepped(current: Option<&Value>, larger: bool) -> Option<Value> {
    let Some(&Value::Size(current)) = current else {
        return None;
    };
    let mut sizes = SIZES.into_iter().map(twentieths);
    let next = if larger {
        sizes.find(|&size| size > current)
    } else {
        sizes.rev().find(|&size| size < current)
    };
    next.map(Value::Size)
}

/// A box showing the selection's size, which a size is typed into and Enter
/// gives, with the sizes offered beside it. Answers the size given.
fn size_box(ui: &mut Ui, editor: &PageEditor, model: &FlowModel<'_>) -> Option<Value> {
    let current = match editor.value(model, Property::Size) {
        Some(Value::Size(size)) => Some(size),
        _ => None,
    };
    let id = ui.make_persistent_id("size");
    let mut text = ui
        .data(|data| data.get_temp::<String>(id))
        .unwrap_or_else(|| current.map(shown).unwrap_or_default());
    let response = ui
        .add(egui::TextEdit::singleline(&mut text).desired_width(32.0))
        .on_hover_text(t("Size"));
    let mut given = None;
    if response.has_focus() {
        ui.data_mut(|data| data.insert_temp(id, text.clone()));
    } else {
        ui.data_mut(|data| data.remove::<String>(id));
    }
    if response.lost_focus() && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter))
    {
        given = typed_size(&text).map(Value::Size);
    }
    egui::ComboBox::from_id_salt("sizes")
        .width(0.0)
        .selected_text("")
        .show_ui(ui, |ui| {
            for size in SIZES.map(twentieths) {
                if ui
                    .selectable_label(current == Some(size), shown(size))
                    .clicked()
                {
                    given = Some(Value::Size(size));
                }
            }
        });
    given
}

/// The alignments, in the order the toolbar and the menu offer them.
const ALIGNMENTS: [TextAlign; 4] = [
    TextAlign::Start,
    TextAlign::Center,
    TextAlign::End,
    TextAlign::Justify,
];

fn align_name(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Start => t("Align left"),
        TextAlign::Center => t("Center"),
        TextAlign::End => t("Align right"),
        TextAlign::Justify => t("Justify"),
    }
}

/// A button drawn as lines of text set the way an alignment sets them.
fn align_button(ui: &mut Ui, lit: bool, align: TextAlign) -> Response {
    let height = ui.spacing().interact_size.y;
    let response = ui.add(egui::Button::selectable(lit, "").min_size(vec2(height, height)));
    let color = ui
        .style()
        .interact_selectable(&response, lit)
        .fg_stroke
        .color;
    let area = response.rect.shrink2(vec2(height * 0.22, height * 0.25));
    let stroke = egui::Stroke::new(1.5, color);
    for (line, full) in [(0.0, true), (1.0, false), (2.0, true), (3.0, false)] {
        let y = area.top() + area.height() * line / 3.0;
        let width = if full || align == TextAlign::Justify {
            area.width()
        } else {
            area.width() * 0.6
        };
        let left = match align {
            TextAlign::Start | TextAlign::Justify => area.left(),
            TextAlign::Center => area.center().x - width / 2.0,
            TextAlign::End => area.right() - width,
        };
        ui.painter()
            .line_segment([egui::pos2(left, y), egui::pos2(left + width, y)], stroke);
    }
    response
}

/// The marks and the alignments, and after them the kinds of paragraph: body
/// text, three levels of heading, a bulleted and a numbered list. Each kind is
/// lit where every paragraph the selection runs over is one, and pressing a
/// lit one takes it off.
pub fn text(ui: &mut Ui, editor: &PageEditor, model: &FlowModel<'_>) -> Pressed {
    let mut pressed = Pressed::default();
    ui.add_enabled_ui(editor.selection().is_some(), |ui| {
        ui.horizontal_wrapped(|ui| {
            text_buttons(ui, editor, model, &mut pressed);
            ui.separator();
            for (block, label) in [
                (Block::Body, "\u{b6}"),
                (Block::Heading(1), "H1"),
                (Block::Heading(2), "H2"),
                (Block::Heading(3), "H3"),
                (Block::Bullets, "\u{2022}"),
                (Block::Numbers, "1."),
            ] {
                let lit = editor
                    .selection()
                    .is_some_and(|selection| model.is_block(selection, block));
                if ui
                    .selectable_label(lit, label)
                    .on_hover_text(block_name(block))
                    .clicked()
                {
                    pressed.block = Some(block);
                }
            }
            if editor.has_focus(ui.ctx())
                && editor
                    .selection()
                    .is_some_and(|selection| model.has_heading(selection))
            {
                for (key, delta) in [(Key::ArrowLeft, -1), (Key::ArrowRight, 1)] {
                    if ui.input_mut(|input| input.consume_key(OUTLINE, key)) {
                        pressed.outline = Some(delta);
                    }
                }
            }
            ui.separator();
            pressed.table = ui
                .button(t("Table"))
                .on_hover_text(t("Insert a table"))
                .clicked();
            pressed.picture = ui
                .button(t("Picture"))
                .on_hover_text(t("Insert a picture"))
                .clicked();
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
            Some(key) => format!("{name} ({})", shortcut(ui.ctx(), key)),
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

/// A key with Ctrl, or Command on a Mac, as a menu shows it: egui spells a
/// key by its name in the long form, `Ctrl+CloseBracket`, and a person reads
/// its symbol, `Ctrl+]`.
fn shortcut(ctx: &egui::Context, key: Key) -> String {
    ctx.format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, key))
        .replace(key.name(), key.symbol_or_name())
}

/// What the Format menu asks of the page: one of the buttons' commands.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Bold, italic, underline or strikethrough.
    Mark(Mark),
    /// The next size offered, larger or not.
    Size(bool),
    /// An alignment.
    Align(TextAlign),
    /// A kind of paragraph.
    Block(Block),
    /// Superscript or subscript, or back on the line.
    Script(Script),
    /// A tab's width in, above zero, or out.
    Indent(i8),
    /// A line spacing.
    LineSpacing(LineSpacing),
    /// Start a numbered list again at the item, or carry it on from the list
    /// before when not.
    Numbering(bool),
    /// Move the headings selected up the outline (below zero) or down it.
    Outline(i8),
    /// Ask how large a table to put in.
    InsertTable,
    /// Ask for a picture to put in.
    InsertPicture,
    /// Change the table the caret is in.
    Table(TableEdit),
    /// Ask about the page's paper, orientation and margins.
    PageSetup,
}

impl Command {
    /// The value the command gives the selection, where it gives one.
    pub fn value(self, editor: &PageEditor, model: &FlowModel<'_>) -> Option<Value> {
        match self {
            Self::Size(larger) => stepped(editor.value(model, Property::Size).as_ref(), larger),
            Self::Script(script) => Some(toggled(editor, model, script)),
            _ => None,
        }
    }
}

/// The keys that, with Left and Right, move a heading up and down the outline.
const OUTLINE: Modifiers = Modifiers::ALT.plus(Modifiers::SHIFT);

/// The levels of heading the Format menu offers, which are all ODF has.
const LEVELS: std::ops::RangeInclusive<u8> = 1..=10;

/// Every kind of paragraph the menu lights.
fn every_block() -> impl Iterator<Item = Block> {
    [Block::Body, Block::Bullets, Block::Numbers]
        .into_iter()
        .chain(LEVELS.map(Block::Heading))
}

fn block_name(block: Block) -> String {
    match block {
        Block::Body => t("Body text").to_owned(),
        Block::Heading(level) => fill(t("Heading {level}"), &[("level", &level.to_string())]),
        Block::Bullets => t("Bulleted list").to_owned(),
        Block::Numbers => t("Numbered list").to_owned(),
    }
}

/// What the Format menu draws itself from: what the page said the last time it
/// was drawn, because a menu is drawn before the page is.
#[derive(Default)]
pub struct MenuState {
    enabled: bool,
    blocks: bool,
    in_table: bool,
    marks: Vec<Mark>,
    align: Option<TextAlign>,
    kinds: Vec<Block>,
    script: Option<Script>,
    spacing: Option<LineSpacing>,
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
            in_table: blocks && model.in_table(selection),
            marks: Mark::ALL
                .into_iter()
                .filter(|mark| editor.marked(model, *mark) == Some(true))
                .collect(),
            align: model.alignment(selection),
            spacing: model.line_spacing(selection),
            script: match editor.value(model, Property::Script) {
                Some(Value::Script(script)) => Some(script),
                _ => None,
            },
            kinds: if blocks {
                every_block()
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

/// The kinds of paragraph in the Format menu: body text, a heading of any
/// level, the two lists, and moving headings up and down the outline.
fn blocks_menu(ui: &mut Ui, state: &MenuState) -> Option<Command> {
    let mut chosen = None;
    let mut offer = |ui: &mut Ui, block: Block| {
        let button = egui::Button::new(block_name(block)).selected(state.kinds.contains(&block));
        if ui.add_enabled(state.enabled, button).clicked() {
            chosen = Some(Command::Block(block));
            ui.close();
        }
    };
    offer(ui, Block::Body);
    ui.add_enabled_ui(state.enabled, |ui| {
        ui.menu_button(t("Heading"), |ui| {
            for level in LEVELS {
                offer(ui, Block::Heading(level));
            }
        });
    });
    offer(ui, Block::Bullets);
    offer(ui, Block::Numbers);
    ui.add_enabled_ui(state.enabled, |ui| {
        ui.menu_button(t("Lists"), |ui| {
            for (restart, name) in [
                (true, t("Restart numbering")),
                (false, t("Continue numbering")),
            ] {
                if ui.button(name).clicked() {
                    chosen = Some(Command::Numbering(restart));
                    ui.close();
                }
            }
        });
    });
    ui.separator();
    for (delta, name, key) in [
        (-1, t("Promote heading"), Key::ArrowLeft),
        (1, t("Demote heading"), Key::ArrowRight),
    ] {
        let keys = ui
            .ctx()
            .format_shortcut(&KeyboardShortcut::new(OUTLINE, key));
        let button = egui::Button::new(name).shortcut_text(keys);
        if ui.add_enabled(state.enabled, button).clicked() {
            chosen = Some(Command::Outline(delta));
            ui.close();
        }
    }
    chosen
}

/// The alignments, the indent steps and the line spacings in the Format menu.
fn paragraph_menu(ui: &mut Ui, state: &MenuState) -> Option<Command> {
    let mut chosen = None;
    for align in ALIGNMENTS {
        let button = egui::Button::new(align_name(align)).selected(state.align == Some(align));
        if ui.add_enabled(state.enabled, button).clicked() {
            chosen = Some(Command::Align(align));
            ui.close();
        }
    }
    ui.separator();
    for (step, name) in [(1, t("Increase indent")), (-1, t("Decrease indent"))] {
        if ui
            .add_enabled(state.enabled, egui::Button::new(name))
            .clicked()
        {
            chosen = Some(Command::Indent(step));
            ui.close();
        }
    }
    ui.add_enabled_ui(state.enabled, |ui| {
        ui.menu_button(t("Line spacing"), |ui| {
            for spacing in SPACINGS {
                let button = egui::Button::new(spacing_label(spacing))
                    .selected(state.spacing == Some(spacing));
                if ui.add(button).clicked() {
                    chosen = Some(Command::LineSpacing(spacing));
                    ui.close();
                }
            }
        });
    });
    chosen
}

/// The Format menu's entries: the marks, each with its shortcut, a size larger
/// and smaller, the alignments, and where the page has them the kinds of
/// paragraph. Lit where the whole selection is one,
/// and not live without a caret. Answers the command chosen.
pub fn menu(ui: &mut Ui, state: &MenuState) -> Option<Command> {
    let mut chosen = None;
    for mark in Mark::ALL {
        let (name, key) = mark_facts(mark);
        let mut button = egui::Button::new(name).selected(state.marks.contains(&mark));
        if let Some(key) = key {
            button = button.shortcut_text(shortcut(ui.ctx(), key));
        }
        if ui.add_enabled(state.enabled, button).clicked() {
            chosen = Some(Command::Mark(mark));
            ui.close();
        }
    }
    for (script, name, key) in [
        (Script::Super, t("Superscript"), Key::P),
        (Script::Sub, t("Subscript"), Key::B),
    ] {
        let keys = ui.ctx().format_shortcut(&KeyboardShortcut::new(
            Modifiers::COMMAND | Modifiers::SHIFT,
            key,
        ));
        let button = egui::Button::new(name)
            .selected(state.script == Some(script))
            .shortcut_text(keys);
        if ui.add_enabled(state.enabled, button).clicked() {
            chosen = Some(Command::Script(script));
            ui.close();
        }
    }
    ui.separator();
    for (larger, name, key) in [
        (true, t("Larger"), Key::CloseBracket),
        (false, t("Smaller"), Key::OpenBracket),
    ] {
        let button = egui::Button::new(name).shortcut_text(shortcut(ui.ctx(), key));
        if ui.add_enabled(state.enabled, button).clicked() {
            chosen = Some(Command::Size(larger));
            ui.close();
        }
    }
    ui.separator();
    chosen = paragraph_menu(ui, state).or(chosen);
    if state.blocks {
        ui.separator();
        chosen = blocks_menu(ui, state).or(chosen);
        ui.separator();
        ui.add_enabled_ui(state.enabled, |ui| {
            ui.menu_button(t("Table"), |ui| {
                if ui.button(t("Insert a table…")).clicked() {
                    chosen = Some(Command::InsertTable);
                    ui.close();
                }
                ui.separator();
                for (edit, name) in [
                    (TableEdit::RowAbove, t("Insert row above")),
                    (TableEdit::RowBelow, t("Insert row below")),
                    (TableEdit::ColumnLeft, t("Insert column left")),
                    (TableEdit::ColumnRight, t("Insert column right")),
                    (TableEdit::DeleteRow, t("Delete row")),
                    (TableEdit::DeleteColumn, t("Delete column")),
                    (TableEdit::DeleteTable, t("Delete table")),
                ] {
                    if matches!(edit, TableEdit::DeleteRow) {
                        ui.separator();
                    }
                    if ui
                        .add_enabled(state.in_table, egui::Button::new(name))
                        .clicked()
                    {
                        chosen = Some(Command::Table(edit));
                        ui.close();
                    }
                }
            });
        });
        if ui
            .add_enabled(state.enabled, egui::Button::new(t("Insert a picture…")))
            .clicked()
        {
            chosen = Some(Command::InsertPicture);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(state.enabled, egui::Button::new(t("Page setup…")))
            .clicked()
        {
            chosen = Some(Command::PageSetup);
            ui.close();
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Editing;
    use crate::flow_model::{page_editor, page_editor_id};
    use eframe::egui::{Context, Event, RawInput};
    use egui_richedit::{Position, Selection};
    use odox_core::Styles;

    fn outline_pressed(page_has_keyboard: bool) -> Option<i8> {
        let mut content = odox_core::xml::parse(
            br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:text><text:h text:outline-level="2">Two</text:h></office:text></office:body></office:document-content>"#,
            "test",
        )
        .expect("a document");
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        let mut editor = page_editor();
        editor.select(Selection::caret(Position::new(vec![0], 1)));
        let model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        let ctx = Context::default();
        if page_has_keyboard {
            ctx.memory_mut(|memory| memory.request_focus(page_editor_id()));
        }
        let input = RawInput {
            events: vec![Event::Key {
                key: Key::ArrowLeft,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: OUTLINE,
            }],
            ..RawInput::default()
        };
        let mut pressed = None;
        let mut output = ctx.run_ui(input, |ui| pressed = text(ui, &editor, &model).outline);
        output.textures_delta.clear();
        pressed
    }

    #[test]
    fn outline_keys_are_the_pages_only_while_it_has_the_keyboard() {
        assert_eq!(outline_pressed(true), Some(-1));
        assert_eq!(outline_pressed(false), None);
    }
}
