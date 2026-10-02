//! The editor driven through egui with no window: events go in as a frame's
//! raw input, and what the model holds afterwards is what is checked.
//!
//! The model is a list of plain strings, which is the point: nothing about the
//! editor may need a richer document than that to work. Bold, one flag a
//! character, is the one formatting it keeps, for the marks.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use egui::output::OutputCommand;
use egui::text::LayoutJob;
use egui::{Context, Event, Id, Key, Modifiers, Pos2, RawInput, Rect, Sense, TextFormat, vec2};
use egui_richedit::{Edit, Laid, Mark, Model, ParagraphJob, Position, RichEdit, Selection};

/// Paragraphs of plain text, named by their index.
struct Plain {
    paragraphs: Vec<String>,
    /// Whether each character of each paragraph is bold.
    bold: Vec<Vec<bool>>,
    /// How many undo steps the edits made so far began.
    steps: usize,
}

impl Plain {
    fn new(paragraphs: &[&str]) -> Self {
        Self {
            paragraphs: paragraphs.iter().map(|&p| p.to_owned()).collect(),
            bold: paragraphs
                .iter()
                .map(|p| vec![false; p.chars().count()])
                .collect(),
            steps: 0,
        }
    }

    /// The bold characters of a paragraph.
    fn bold_text(&self, paragraph: usize) -> String {
        self.paragraphs[paragraph]
            .chars()
            .zip(&self.bold[paragraph])
            .filter(|(_, bold)| **bold)
            .map(|(c, _)| c)
            .collect()
    }

    /// Each paragraph from one position to another, with the range of it
    /// between them.
    fn spans(
        &self,
        from: &Position<usize>,
        to: &Position<usize>,
    ) -> Vec<(usize, std::ops::Range<usize>)> {
        (from.paragraph..=to.paragraph)
            .map(|p| {
                let start = if p == from.paragraph { from.offset } else { 0 };
                let end = if p == to.paragraph {
                    to.offset
                } else {
                    self.bold[p].len()
                };
                (p, start..end)
            })
            .collect()
    }
}

fn byte(text: &str, offset: usize) -> usize {
    text.char_indices()
        .nth(offset)
        .map_or(text.len(), |(i, _)| i)
}

impl Model for Plain {
    type Paragraph = usize;

    fn text(&self, paragraph: &usize) -> Option<String> {
        self.paragraphs.get(*paragraph).cloned()
    }

    fn next(&self, paragraph: &usize) -> Option<usize> {
        (paragraph + 1 < self.paragraphs.len()).then_some(paragraph + 1)
    }

    fn previous(&self, paragraph: &usize) -> Option<usize> {
        paragraph.checked_sub(1)
    }

    fn first(&self) -> Option<usize> {
        (!self.paragraphs.is_empty()).then_some(0)
    }

    fn last(&self) -> Option<usize> {
        self.paragraphs.len().checked_sub(1)
    }

    fn marked(&self, from: &Position<usize>, to: &Position<usize>, mark: Mark) -> Option<bool> {
        if mark != Mark::Bold {
            return Some(false);
        }
        if from == to {
            // Typed text takes the character before it.
            let bold = &self.bold[from.paragraph];
            let at = from.offset.saturating_sub(1);
            return Some(bold.get(at).copied().unwrap_or(false));
        }
        let mut chars = self
            .spans(from, to)
            .into_iter()
            .flat_map(|(p, range)| self.bold[p][range].to_vec());
        let first = chars.next()?;
        chars.all(|b| b == first).then_some(first)
    }

    fn apply(&mut self, edit: Edit<'_, usize>, new_step: bool) -> Option<Position<usize>> {
        if new_step {
            self.steps += 1;
        }
        match edit {
            Edit::Replace { from, to, text } => {
                let tail = {
                    let last = self.paragraphs.get(to.paragraph)?;
                    last[byte(last, to.offset)..].to_owned()
                };
                let tail_bold = self.bold[to.paragraph][to.offset..].to_vec();
                let bold = &mut self.bold[from.paragraph];
                let typed_bold = from.offset > 0 && bold[from.offset - 1];
                bold.truncate(from.offset);
                bold.extend(std::iter::repeat_n(typed_bold, text.chars().count()));
                bold.extend(tail_bold);
                let first = self.paragraphs.get_mut(from.paragraph)?;
                first.truncate(byte(first, from.offset));
                first.push_str(text);
                first.push_str(&tail);
                self.paragraphs.drain(from.paragraph + 1..=to.paragraph);
                self.bold.drain(from.paragraph + 1..=to.paragraph);
                Some(Position::new(
                    from.paragraph,
                    from.offset + text.chars().count(),
                ))
            }
            Edit::Split { at } => {
                let paragraph = self.paragraphs.get_mut(at.paragraph)?;
                let second = paragraph.split_off(byte(paragraph, at.offset));
                self.paragraphs.insert(at.paragraph + 1, second);
                let second = self.bold[at.paragraph].split_off(at.offset);
                self.bold.insert(at.paragraph + 1, second);
                Some(Position::new(at.paragraph + 1, 0))
            }
            Edit::Format { from, to, mark, on } => {
                if mark == Mark::Bold {
                    for (p, range) in self.spans(&from, &to) {
                        self.bold[p][range].fill(on);
                    }
                }
                Some(to)
            }
        }
    }
}

/// An editor over a model, and the context it is drawn in.
struct Harness {
    ctx: Context,
    editor: RichEdit<usize>,
    model: Plain,
}

impl Harness {
    fn new(paragraphs: &[&str]) -> Self {
        let mut harness = Self {
            ctx: Context::default(),
            editor: RichEdit::new(Id::new("editor")),
            model: Plain::new(paragraphs),
        };
        // One frame to lay the paragraphs out, which is what Up and Down move
        // through.
        harness.frame(Vec::new());
        harness
    }

    fn caret(&mut self, paragraph: usize, offset: usize) {
        self.editor
            .select(Selection::caret(Position::new(paragraph, offset)));
    }

    /// One frame with these events in it, answering what egui was asked to
    /// put on the clipboard.
    fn frame(&mut self, events: Vec<Event>) -> Option<String> {
        let input = RawInput {
            events,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0))),
            ..RawInput::default()
        };
        let Self { ctx, editor, model } = self;
        let mut output = ctx.run_ui(input, |ui| {
            editor.input(ui, model);
            for (index, text) in model.paragraphs.iter().enumerate() {
                let mut job = ParagraphJob::new(LayoutJob::default());
                job.text(text, TextFormat::default());
                if job.is_empty() {
                    job.atom(" ", 0, TextFormat::default());
                }
                let (job, map) = job.into_parts();
                let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                let (rect, response) =
                    ui.allocate_exact_size(vec2(300.0, galley.size().y), Sense::click_and_drag());
                let laid = Laid {
                    galley,
                    map,
                    origin: rect.min,
                };
                editor.paragraph(ui, &response, &index, laid);
            }
        });
        // Nothing draws here, so the glyph atlas's updates go nowhere, and
        // egui asks that dropping them be said.
        output.textures_delta.clear();
        output
            .platform_output
            .commands
            .into_iter()
            .find_map(|command| match command {
                OutputCommand::CopyText(text) => Some(text),
                _ => None,
            })
    }

    fn keys(&mut self, keys: &[(Key, Modifiers)]) {
        let events = keys
            .iter()
            .map(|&(key, modifiers)| Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            })
            .collect();
        self.frame(events);
    }

    fn key(&mut self, key: Key) {
        self.keys(&[(key, Modifiers::NONE)]);
    }

    fn typed(&mut self, text: &str) {
        self.frame(vec![Event::Text(text.to_owned())]);
    }

    fn focus(&self) -> Position<usize> {
        self.editor.selection().expect("a caret").focus.clone()
    }
}

#[test]
fn typing_goes_in_at_the_caret_and_a_run_of_it_is_one_step() {
    let mut h = Harness::new(&["Hello world"]);
    h.caret(0, 5);
    h.typed(",");
    h.typed(" there");
    assert_eq!(h.model.paragraphs, ["Hello, there world"]);
    assert_eq!(h.focus(), Position::new(0, 12));
    assert_eq!(h.model.steps, 1, "one run of typing");

    h.key(Key::ArrowLeft);
    h.typed("!");
    assert_eq!(h.model.steps, 2, "a move ends the run");
}

#[test]
fn enter_splits_and_backspace_at_the_start_joins() {
    let mut h = Harness::new(&["abcd"]);
    h.caret(0, 2);
    h.key(Key::Enter);
    assert_eq!(h.model.paragraphs, ["ab", "cd"]);
    assert_eq!(h.focus(), Position::new(1, 0));

    h.key(Key::Backspace);
    assert_eq!(h.model.paragraphs, ["abcd"]);
    assert_eq!(h.focus(), Position::new(0, 2));
}

#[test]
fn delete_at_the_end_joins_the_next_paragraph() {
    let mut h = Harness::new(&["ab", "cd"]);
    h.caret(0, 2);
    h.key(Key::Delete);
    assert_eq!(h.model.paragraphs, ["abcd"]);
    assert_eq!(h.focus(), Position::new(0, 2));
}

#[test]
fn shift_enter_is_a_line_break_inside_the_paragraph() {
    let mut h = Harness::new(&["abcd"]);
    h.caret(0, 2);
    h.keys(&[(Key::Enter, Modifiers::SHIFT)]);
    assert_eq!(h.model.paragraphs, ["ab\ncd"]);
}

#[test]
fn arrows_cross_from_one_paragraph_into_the_next() {
    let mut h = Harness::new(&["ab", "cd"]);
    h.caret(0, 2);
    h.key(Key::ArrowRight);
    assert_eq!(h.focus(), Position::new(1, 0));
    h.key(Key::ArrowLeft);
    assert_eq!(h.focus(), Position::new(0, 2));
}

#[test]
fn up_and_down_move_between_paragraphs_by_where_they_were_drawn() {
    let mut h = Harness::new(&["first line", "second line"]);
    h.caret(0, 3);
    h.frame(Vec::new());
    h.key(Key::ArrowDown);
    let below = h.focus();
    assert_eq!(below.paragraph, 1);
    assert!(
        (2..=4).contains(&below.offset),
        "kept to the column: {below:?}"
    );
    h.key(Key::ArrowUp);
    assert_eq!(h.focus(), Position::new(0, 3), "and back to where it began");
}

#[test]
fn a_selection_across_paragraphs_is_replaced_by_what_is_typed() {
    let mut h = Harness::new(&["ab", "cd"]);
    h.caret(0, 1);
    h.keys(&[
        (Key::ArrowRight, Modifiers::SHIFT),
        (Key::ArrowRight, Modifiers::SHIFT),
        (Key::ArrowRight, Modifiers::SHIFT),
    ]);
    let selection = h.editor.selection().expect("a selection").clone();
    assert_eq!(selection.anchor, Position::new(0, 1));
    assert_eq!(selection.focus, Position::new(1, 1));

    h.typed("X");
    assert_eq!(h.model.paragraphs, ["aXd"]);
}

#[test]
fn copy_takes_the_selection_with_paragraphs_on_lines_of_their_own() {
    let mut h = Harness::new(&["one", "two", "three"]);
    h.editor.select(Selection {
        anchor: Position::new(2, 2),
        focus: Position::new(0, 1),
    });
    assert_eq!(h.frame(vec![Event::Copy]).as_deref(), Some("ne\ntwo\nth"));
}

#[test]
fn a_paste_of_several_lines_is_several_paragraphs_in_one_step() {
    let mut h = Harness::new(&["ab"]);
    h.caret(0, 1);
    h.frame(vec![Event::Paste("1\r\n2\n3".to_owned())]);
    assert_eq!(h.model.paragraphs, ["a1", "2", "3b"]);
    assert_eq!(h.focus(), Position::new(2, 1));
    assert_eq!(h.model.steps, 1);
}

#[test]
fn word_steps_with_ctrl() {
    let mut h = Harness::new(&["one two three"]);
    h.caret(0, 0);
    h.keys(&[(Key::ArrowRight, Modifiers::CTRL)]);
    let first = h.focus().offset;
    assert!(first == 3 || first == 4, "past the first word: {first}");
    h.keys(&[(Key::Backspace, Modifiers::CTRL)]);
    assert_eq!(h.model.paragraphs[0].trim_start(), "two three");
}

#[test]
fn nothing_is_taken_without_the_focus() {
    let mut h = Harness::new(&["ab"]);
    h.typed("x");
    h.key(Key::Backspace);
    assert_eq!(h.model.paragraphs, ["ab"]);
    assert_eq!(h.model.steps, 0);
}

#[test]
fn a_selection_left_past_the_end_by_an_outside_change_is_pulled_back() {
    let mut h = Harness::new(&["abcdef"]);
    h.caret(0, 6);
    h.model.paragraphs[0].truncate(2);
    h.editor.document_replaced();
    h.typed("!");
    assert_eq!(h.model.paragraphs, ["ab!"]);
}

#[test]
fn a_run_of_downs_keeps_its_column_through_a_short_line() {
    let mut h = Harness::new(&["a longer first line", "ab", "a longer third line"]);
    h.caret(0, 12);
    h.frame(Vec::new());
    h.key(Key::ArrowDown);
    assert_eq!(h.focus(), Position::new(1, 2), "the short line's end");
    h.key(Key::ArrowDown);
    let below = h.focus();
    assert_eq!(below.paragraph, 2);
    assert!(
        (11..=13).contains(&below.offset),
        "back at the column: {below:?}"
    );
}

/// A model that refuses any replace across paragraphs, as an application
/// whose paragraphs sit in different containers does.
struct Refusing(Plain);

impl Model for Refusing {
    type Paragraph = usize;

    fn text(&self, paragraph: &usize) -> Option<String> {
        self.0.text(paragraph)
    }

    fn next(&self, paragraph: &usize) -> Option<usize> {
        self.0.next(paragraph)
    }

    fn previous(&self, paragraph: &usize) -> Option<usize> {
        self.0.previous(paragraph)
    }

    fn first(&self) -> Option<usize> {
        self.0.first()
    }

    fn last(&self) -> Option<usize> {
        self.0.last()
    }

    fn marked(&self, from: &Position<usize>, to: &Position<usize>, mark: Mark) -> Option<bool> {
        self.0.marked(from, to, mark)
    }

    fn apply(&mut self, edit: Edit<'_, usize>, new_step: bool) -> Option<Position<usize>> {
        match &edit {
            Edit::Replace { from, to, .. } if from.paragraph != to.paragraph => None,
            _ => self.0.apply(edit, new_step),
        }
    }
}

#[test]
fn a_selection_the_model_refuses_is_left_whole_by_paste_and_enter() {
    let ctx = Context::default();
    let mut editor = RichEdit::new(Id::new("editor"));
    let mut model = Refusing(Plain::new(&["ab", "cd"]));
    let selection = Selection {
        anchor: Position::new(0, 1),
        focus: Position::new(1, 1),
    };
    for event in [
        Event::Paste("x\ny".to_owned()),
        Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
    ] {
        editor.select(selection.clone());
        let input = RawInput {
            events: vec![event],
            ..RawInput::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            editor.input(ui, &mut model);
        });
        output.textures_delta.clear();
        assert_eq!(model.0.paragraphs, ["ab", "cd"]);
    }
}

#[test]
fn a_click_whose_release_comes_a_frame_later_keeps_the_caret() {
    let mut h = Harness::new(&["Hello world"]);
    let at = Pos2::new(1.0, 5.0);
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    h.frame(vec![Event::PointerMoved(at)]);
    h.frame(vec![button(true)]);
    h.frame(vec![button(false)]);
    h.frame(Vec::new());
    h.typed(">");
    assert_eq!(h.model.paragraphs, [">Hello world"]);
}

#[test]
fn a_drag_past_the_last_paragraph_takes_its_end_and_past_the_first_takes_its_start() {
    let mut h = Harness::new(&["Hello", "world", "again"]);
    let button = |at: Pos2, pressed| Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let start = Pos2::new(20.0, 40.0);
    h.frame(vec![Event::PointerMoved(start)]);
    h.frame(vec![button(start, true)]);
    h.frame(vec![Event::PointerMoved(Pos2::new(20.0, 390.0))]);
    h.frame(Vec::new());
    assert_eq!(h.focus(), Position::new(2, 5), "below everything");
    h.frame(vec![Event::PointerMoved(Pos2::new(20.0, -30.0))]);
    h.frame(Vec::new());
    assert_eq!(h.focus(), Position::new(0, 0), "above everything");
}

#[test]
fn a_drag_stays_in_the_paragraph_it_is_over_when_another_sits_beside_it() {
    // Two paragraphs side by side, as two cells of a table row are.
    let ctx = Context::default();
    let mut editor = RichEdit::new(Id::new("editor"));
    let mut model = Plain::new(&["left cell text", "right"]);
    let mut frame = |events: Vec<Event>| {
        let input = RawInput {
            events,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0))),
            ..RawInput::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            editor.input(ui, &mut model);
            ui.horizontal_top(|ui| {
                for (index, text) in model.paragraphs.iter().enumerate() {
                    let mut job = ParagraphJob::new(LayoutJob::default());
                    job.text(text, TextFormat::default());
                    let (job, map) = job.into_parts();
                    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                    let (rect, response) = ui
                        .allocate_exact_size(vec2(150.0, galley.size().y), Sense::click_and_drag());
                    let laid = Laid {
                        galley,
                        map,
                        origin: rect.min,
                    };
                    editor.paragraph(ui, &response, &index, laid);
                }
            });
        });
        output.textures_delta.clear();
    };
    let button = |x: f32, pressed| Event::PointerButton {
        pos: Pos2::new(x, 5.0),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(vec![Event::PointerMoved(Pos2::new(2.0, 5.0))]);
    frame(vec![button(2.0, true)]);
    for x in [10.0, 20.0, 30.0] {
        frame(vec![Event::PointerMoved(Pos2::new(x, 5.0))]);
    }
    frame(vec![button(30.0, false)]);
    let selection = editor.selection().expect("a selection").clone();
    assert_eq!(selection.anchor.paragraph, 0);
    assert_eq!(selection.focus.paragraph, 0, "the drag stayed in its cell");
    assert!(selection.focus.offset > 0);
}

#[test]
fn ctrl_a_selects_the_whole_document() {
    let mut h = Harness::new(&["one", "two", "three"]);
    h.caret(1, 1);
    h.keys(&[(Key::A, Modifiers::COMMAND)]);
    assert_eq!(
        h.frame(vec![Event::Copy]).as_deref(),
        Some("one\ntwo\nthree")
    );
}

#[test]
fn ctrl_home_and_end_go_to_the_document_s_ends_and_shift_selects() {
    let mut h = Harness::new(&["one", "two", "three"]);
    h.caret(1, 1);
    h.keys(&[(Key::End, Modifiers::COMMAND)]);
    assert_eq!(h.focus(), Position::new(2, 5));
    h.keys(&[(Key::Home, Modifiers::COMMAND | Modifiers::SHIFT)]);
    let selection = h.editor.selection().expect("a selection").clone();
    assert_eq!(selection.anchor, Position::new(2, 5));
    assert_eq!(selection.focus, Position::new(0, 0));
}

#[test]
fn page_down_moves_a_view_s_height_and_page_up_comes_back() {
    let lines: Vec<String> = (0..200).map(|i| format!("line {i}")).collect();
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let mut h = Harness::new(&lines);
    h.caret(0, 2);
    h.frame(Vec::new());
    h.key(Key::PageDown);
    let below = h.focus();
    // The harness's view is 400 points tall, and a line is a little over 16.
    assert!(
        (18..=30).contains(&below.paragraph),
        "a view down: {below:?}"
    );
    assert_eq!(below.offset, 2, "kept to the column");
    h.frame(Vec::new());
    h.key(Key::PageUp);
    assert_eq!(h.focus(), Position::new(0, 2));
}

#[test]
fn a_paragraph_that_leaves_drags_takes_a_caret_on_a_click_and_not_on_a_drag() {
    let ctx = Context::default();
    let mut editor = RichEdit::new(Id::new("editor"));
    let mut model = Plain::new(&["a label on a slide"]);
    let mut frame = |events: Vec<Event>| {
        let input = RawInput {
            events,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0))),
            ..RawInput::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            editor.input(ui, &mut model);
            let mut job = ParagraphJob::new(LayoutJob::default());
            job.text(&model.paragraphs[0], TextFormat::default());
            let (job, map) = job.into_parts();
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
            let (rect, response) =
                ui.allocate_exact_size(vec2(300.0, galley.size().y), Sense::click());
            let laid = Laid {
                galley,
                map,
                origin: rect.min,
            };
            editor.paragraph(ui, &response, &0, laid);
        });
        output.textures_delta.clear();
        editor.selection().cloned()
    };
    let button = |x: f32, pressed| Event::PointerButton {
        pos: Pos2::new(x, 5.0),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(vec![Event::PointerMoved(Pos2::new(5.0, 5.0))]);
    frame(vec![button(5.0, true)]);
    for x in [40.0, 80.0, 120.0] {
        frame(vec![Event::PointerMoved(Pos2::new(x, 5.0))]);
    }
    assert_eq!(
        frame(vec![button(120.0, false)]),
        None,
        "a drag is not a click"
    );

    frame(vec![Event::PointerMoved(Pos2::new(5.0, 5.0))]);
    frame(vec![button(5.0, true)]);
    let clicked = frame(vec![button(5.0, false)]).expect("a click puts a caret down");
    assert_eq!(clicked.focus.paragraph, 0);
}

fn command(key: Key) -> (Key, Modifiers) {
    (key, Modifiers::COMMAND)
}

#[test]
fn a_mark_on_a_selection_is_one_step_and_keeps_the_selection() {
    let mut h = Harness::new(&["one two", "three"]);
    h.caret(0, 4);
    // "two", the break, and "th".
    let mut keys = vec![(Key::ArrowRight, Modifiers::SHIFT); 6];
    keys.push(command(Key::B));
    h.keys(&keys);
    assert_eq!(h.model.bold_text(0), "two");
    assert_eq!(h.model.bold_text(1), "th");
    assert_eq!(h.model.steps, 1);
    let selection = h.editor.selection().expect("a selection").clone();
    assert!(!selection.is_caret(), "the selection stays");
    assert_eq!(h.editor.marked(&h.model, Mark::Bold), Some(true));

    // All of it bold: the mark comes off.
    h.keys(&[command(Key::B)]);
    assert_eq!(h.model.bold_text(0), "");
    assert_eq!(h.model.steps, 2);
}

#[test]
fn a_mixed_selection_takes_the_mark_throughout() {
    let mut h = Harness::new(&["abcd"]);
    h.caret(0, 0);
    h.keys(&[(Key::ArrowRight, Modifiers::SHIFT), command(Key::B)]);
    assert_eq!(h.model.bold_text(0), "a");
    h.caret(0, 0);
    h.keys(&[(Key::End, Modifiers::SHIFT)]);
    assert_eq!(h.editor.marked(&h.model, Mark::Bold), None);
    h.keys(&[command(Key::B)]);
    assert_eq!(h.model.bold_text(0), "abcd");
}

#[test]
fn a_mark_at_a_caret_goes_to_what_is_typed_there_next() {
    let mut h = Harness::new(&["ab"]);
    h.caret(0, 1);
    h.keys(&[command(Key::B)]);
    assert_eq!(h.model.steps, 0, "nothing changed yet");
    assert_eq!(h.editor.marked(&h.model, Mark::Bold), Some(true));
    h.typed("x");
    h.typed("y");
    assert_eq!(h.model.paragraphs, ["axyb"]);
    assert_eq!(h.model.bold_text(0), "xy");
    assert_eq!(h.model.steps, 1, "the typing and its mark are one step");
}

#[test]
fn a_mark_at_a_caret_is_forgotten_when_the_caret_moves_or_is_toggled_back() {
    let mut h = Harness::new(&["abc"]);
    h.caret(0, 1);
    h.keys(&[command(Key::B)]);
    h.key(Key::ArrowRight);
    assert_eq!(h.editor.marked(&h.model, Mark::Bold), Some(false));
    h.typed("x");
    assert_eq!(h.model.bold_text(0), "");

    h.keys(&[command(Key::B), command(Key::B)]);
    assert_eq!(h.editor.marked(&h.model, Mark::Bold), Some(false));
    h.typed("y");
    assert_eq!(h.model.bold_text(0), "");
}
