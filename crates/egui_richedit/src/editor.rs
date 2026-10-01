//! The editor: where the caret is, what the keys and the pointer do to it,
//! and how it and the selection are painted over the application's galleys.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::HashMap;
use std::sync::Arc;

use egui::output::IMEOutput;
use egui::text::{CCursor, CCursorRange, CharIndex};
use egui::text_selection::text_cursor_state::{
    ccursor_next_word, ccursor_previous_word, is_word_char,
};
use egui::text_selection::visuals::{paint_text_cursor, paint_text_selection};
use egui::{
    CursorIcon, Event, EventFilter, Galley, IMEPurpose, Id, ImeEvent, Key, Modifiers, Pos2,
    Response, Sense, Ui, vec2,
};

use crate::{Edit, Mark, Model, OffsetMap, Position, Selection};

/// Keys the editor keeps while it has the focus, rather than letting egui
/// move the focus to another widget with them. Escape is not kept: it is how
/// a person leaves the page.
const FILTER: EventFilter = EventFilter {
    tab: true,
    horizontal_arrows: true,
    vertical_arrows: true,
    escape: false,
};

/// A paragraph as the application laid it out and where it is on screen.
pub struct Laid {
    /// The galley egui made of the paragraph's [`crate::ParagraphJob`].
    pub galley: Arc<Galley>,
    /// The map that came out of the same job.
    pub map: OffsetMap,
    /// Where the galley is painted: the point its rows are positioned from.
    pub origin: Pos2,
}

/// What kind of edit the open undo step is made of. Consecutive edits of one
/// kind share a step; anything else begins one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Group {
    Typing,
    Deleting,
    /// A paste, a cut or a split, each its own step however many edits it
    /// takes.
    Other,
}

/// The editor's state, kept by the application from one frame to the next.
///
/// One per editable surface. It holds the selection, which way Up and Down
/// are aiming, which undo step is open, and where each paragraph was drawn
/// last frame, which is what Up, Down, Home and End move by.
pub struct RichEdit<P> {
    id: Id,
    selection: Option<Selection<P>>,
    /// At a row's wrap point, whether the caret is drawn at the start of the
    /// next row rather than at the end of this one.
    next_row: bool,
    /// The screen x that a run of Up and Down keeps to.
    column: Option<f32>,
    group: Option<Group>,
    /// Last frame's paragraphs.
    placed: HashMap<P, Placed>,
    /// This frame's, as they are reported.
    placing: Placing<P>,
    /// What the next frame owes the caret.
    owed: Owed,
    /// How tall the view the paragraphs are drawn in is, which is how far
    /// Page Up and Page Down move.
    page_height: f32,
    /// The pointer went down on a paragraph and has not come up.
    dragging: bool,
    /// When the caret last moved or the text last changed, which is when its
    /// blink restarts.
    last_interaction: f64,
    /// Marks given or taken off at a caret, for the text typed there next.
    pending: Option<Pending<P>>,
}

/// Marks given or taken off with a caret and nothing selected. The next text
/// typed at that caret takes them; once the caret is anywhere else, or the
/// document has changed, they are forgotten.
struct Pending<P> {
    at: Position<P>,
    marks: Vec<(Mark, bool)>,
}

/// What the caret is owed by the next frame that draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owed {
    Nothing,
    /// It moved, and is scrolled into view where it is drawn.
    Reveal,
    /// It was put somewhere from outside, and takes the keyboard too.
    FocusAndReveal,
}

struct Placed {
    galley: Arc<Galley>,
    map: OffsetMap,
    origin: Pos2,
    /// Where the paragraph came in document order.
    index: usize,
}

/// The paragraphs reported so far this frame.
struct Placing<P> {
    placed: HashMap<P, Placed>,
    /// Whether the selection began in a paragraph already painted and has
    /// not yet ended.
    inside: bool,
}

impl<P> Default for Placing<P> {
    fn default() -> Self {
        Self {
            placed: HashMap::new(),
            inside: false,
        }
    }
}

impl<P: Clone + Eq + std::hash::Hash + std::fmt::Debug> RichEdit<P> {
    /// An editor with no selection, which takes the focus under an id.
    pub fn new(id: Id) -> Self {
        Self {
            id,
            selection: None,
            next_row: false,
            column: None,
            group: None,
            placed: HashMap::new(),
            placing: Placing::default(),
            owed: Owed::Nothing,
            page_height: 0.0,
            dragging: false,
            last_interaction: 0.0,
            pending: None,
        }
    }

    /// The selection, or `None` when there is no caret.
    pub fn selection(&self) -> Option<&Selection<P>> {
        self.selection.as_ref()
    }

    /// Put the caret or the selection somewhere. The next frame scrolls it
    /// into view and gives the editor the keyboard.
    pub fn select(&mut self, selection: Selection<P>) {
        self.selection = Some(selection);
        self.group = None;
        self.column = None;
        self.pending = None;
        self.owed = Owed::FocusAndReveal;
    }

    /// Take the caret away.
    pub fn clear(&mut self) {
        self.selection = None;
        self.group = None;
        self.column = None;
        self.dragging = false;
        self.pending = None;
    }

    /// The document was changed or replaced by something other than this
    /// editor, as an undo is. The next edit begins a new undo step, and the
    /// selection is checked against the model before it is used again.
    pub fn document_replaced(&mut self) {
        self.group = None;
        self.column = None;
        self.pending = None;
    }

    /// Whether the selection carries a mark, as a toolbar shows it: `None`
    /// where part of it does, and for a caret what text typed there would
    /// take, a mark given or taken off at the caret included. `None` too when
    /// there is no caret.
    pub fn marked<M: Model<Paragraph = P>>(&self, model: &M, mark: Mark) -> Option<bool> {
        let selection = self.selection.as_ref()?;
        if let Some(on) = self.pending_mark(mark) {
            return Some(on);
        }
        let (from, to) = self.ordered(selection);
        model.marked(&from, &to, mark)
    }

    /// Give the selection a mark, or take it off where all of it has it, in
    /// one undo step; with a caret and nothing selected, give it to or take it
    /// off what is typed next at the caret. Answers whether the document
    /// changed. The editor takes the keyboard back from whatever asked, as a
    /// toolbar's button does.
    pub fn toggle<M: Model<Paragraph = P>>(&mut self, model: &mut M, mark: Mark) -> bool {
        let Some(selection) = self.selection.clone() else {
            return false;
        };
        self.owed = Owed::FocusAndReveal;
        let on = self.marked(model, mark) != Some(true);
        if selection.is_caret() {
            let at = selection.focus;
            let mut marks = match self.pending.take() {
                Some(pending) if pending.at == at => pending.marks,
                _ => Vec::new(),
            };
            marks.retain(|(m, _)| *m != mark);
            if model.marked(&at, &at, mark) != Some(on) {
                marks.push((mark, on));
            }
            self.pending = (!marks.is_empty()).then_some(Pending { at, marks });
            return false;
        }
        let (from, to) = self.ordered(&selection);
        self.group = None;
        let changed = self.format(model, from, to, mark, on, Group::Other);
        self.group = None;
        changed
    }

    /// What a mark given or taken off at the caret says, while the caret is
    /// where it was then.
    fn pending_mark(&self, mark: Mark) -> Option<bool> {
        let selection = self.selection.as_ref().filter(|s| s.is_caret())?;
        let pending = self.pending.as_ref().filter(|p| p.at == selection.focus)?;
        pending
            .marks
            .iter()
            .find(|(m, _)| *m == mark)
            .map(|(_, on)| *on)
    }

    /// Take this frame's events and apply them to the model. Called once a
    /// frame, before the paragraphs are laid out, whether or not the editor
    /// has the keyboard. Answers whether the document changed.
    pub fn input<M: Model<Paragraph = P>>(&mut self, ui: &mut Ui, model: &mut M) -> bool {
        self.placed = std::mem::take(&mut self.placing).placed;

        // Registered every frame, because egui takes the focus from a widget
        // that did not appear in the last one. It senses no pointer, so the
        // paragraphs over it still get theirs.
        ui.interact(ui.clip_rect(), self.id, Sense::focusable_noninteractive());
        self.page_height = ui.clip_rect().height();
        if self.owed == Owed::FocusAndReveal {
            self.owed = Owed::Reveal;
            ui.memory_mut(|memory| memory.request_focus(self.id));
        }
        if !ui.memory(|memory| memory.has_focus(self.id)) {
            self.dragging = false;
            return false;
        }
        ui.memory_mut(|memory| memory.set_focus_lock_filter(self.id, FILTER));
        if self.dragging {
            scroll_toward_pointer(ui);
        }
        self.settle(model);

        let events = ui.input(|input| input.filtered_events(&FILTER));
        let mut changed = false;
        for event in &events {
            let (moved, edited) = self.event(ui, model, event);
            if moved || edited {
                self.last_interaction = ui.input(|input| input.time);
                self.owed = Owed::Reveal;
            }
            changed |= edited;
        }
        changed
    }

    /// Paint a paragraph with the selection and the caret over it, and take
    /// what the pointer does on it. Called for every editable paragraph in
    /// document order, on screen or not, after [`Self::input`].
    ///
    /// `response` is the paragraph's, allocated to sense clicks and, for a
    /// drag to select, drags.
    pub fn paragraph(&mut self, ui: &Ui, response: &Response, paragraph: &P, laid: Laid) {
        let Laid {
            mut galley,
            map,
            origin,
        } = laid;
        let focused = ui.memory(|memory| memory.has_focus(self.id));

        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Text);
        }
        self.pointer(ui, response, paragraph, &galley, &map, origin);

        if let Some(range) = self.selected_here(paragraph, &map)
            && focused
        {
            paint_text_selection(&mut galley, ui.visuals(), &range, None);
        }
        ui.painter()
            .galley(origin, galley.clone(), ui.visuals().text_color());

        if focused
            && let Some(selection) = &self.selection
            && selection.focus.paragraph == *paragraph
        {
            let cursor = CCursor {
                index: CharIndex(map.to_galley(selection.focus.offset)),
                prefer_next_row: self.next_row,
            };
            let caret = galley.pos_from_cursor(cursor).translate(origin.to_vec2());
            let since = ui.input(|input| input.time) - self.last_interaction;
            paint_text_cursor(ui, ui.painter(), caret, since);
            if self.owed == Owed::Reveal {
                self.owed = Owed::Nothing;
                ui.scroll_to_rect(caret.expand(4.0), None);
            }
            // Where an input method draws what is being composed.
            let to_global = ui
                .ctx()
                .layer_transform_to_global(ui.layer_id())
                .unwrap_or_default();
            ui.output_mut(|output| {
                output.ime = Some(IMEOutput {
                    purpose: IMEPurpose::Normal,
                    rect: to_global * response.rect,
                    cursor_rect: to_global * caret,
                    should_interrupt_composition: false,
                });
            });
        }

        let index = self.placing.placed.len();
        self.placing.placed.insert(
            paragraph.clone(),
            Placed {
                galley,
                map,
                origin,
                index,
            },
        );
    }

    /// Drop a selection whose paragraph is gone, and pull one past the end of
    /// its paragraph back to it, as an undo can leave either.
    fn settle<M: Model<Paragraph = P>>(&mut self, model: &M) {
        let Some(selection) = &mut self.selection else {
            return;
        };
        let kept = [&mut selection.anchor, &mut selection.focus]
            .into_iter()
            .all(|position| {
                len(model, &position.paragraph)
                    .map(|len| position.offset = position.offset.min(len))
                    .is_some()
            });
        if !kept {
            self.selection = None;
        }
    }

    /// One event. Answers whether it moved the caret and whether it changed
    /// the document.
    fn event<M: Model<Paragraph = P>>(
        &mut self,
        ui: &Ui,
        model: &mut M,
        event: &Event,
    ) -> (bool, bool) {
        match event {
            Event::Text(text) | Event::Ime(ImeEvent::Commit(text))
                if !text.is_empty() && text != "\n" && text != "\r" =>
            {
                (false, self.type_text(model, text))
            }
            Event::Paste(text) => (false, self.paste(model, text)),
            Event::Copy => {
                if let Some(text) = self.selected_text(model) {
                    ui.ctx().copy_text(text);
                }
                (false, false)
            }
            Event::Cut => {
                let Some(text) = self.selected_text(model) else {
                    return (false, false);
                };
                ui.ctx().copy_text(text);
                self.group = None;
                let cut = self.delete_selection(model, Group::Other);
                self.group = None;
                (false, cut)
            }
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => match shortcut(*key).filter(|_| modifiers.command) {
                Some(mark) => (false, self.toggle(model, mark)),
                None => self.key(model, *key, *modifiers),
            },
            _ => (false, false),
        }
    }

    fn key<M: Model<Paragraph = P>>(
        &mut self,
        model: &mut M,
        key: Key,
        modifiers: Modifiers,
    ) -> (bool, bool) {
        let Some(selection) = self.selection.clone() else {
            return (false, false);
        };
        // Alt on a Mac, Ctrl elsewhere, and egui's own text box takes either.
        let word = modifiers.alt || modifiers.ctrl;
        let extend = modifiers.shift;
        match key {
            Key::ArrowLeft | Key::ArrowRight => {
                let right = key == Key::ArrowRight;
                let to = if !selection.is_caret() && !extend {
                    // An arrow on a selection goes to its near end.
                    let (from, to) = self.ordered(&selection);
                    if right { to } else { from }
                } else if modifiers.mac_cmd {
                    self.row_edge(model, &selection.focus, right)
                } else if right {
                    step_right(model, &selection.focus, word)
                } else {
                    step_left(model, &selection.focus, word)
                };
                self.next_row = false;
                self.move_to(to, extend);
                (true, false)
            }
            Key::A if modifiers.command => {
                let (Some(first), Some(last)) = (model.first(), model.last()) else {
                    return (false, false);
                };
                self.move_to(Position::new(first, 0), false);
                self.move_to(edge(model, last, true), true);
                (true, false)
            }
            Key::Home | Key::End if modifiers.command => {
                let end = key == Key::End;
                let paragraph = if end { model.last() } else { model.first() };
                let Some(paragraph) = paragraph else {
                    return (false, false);
                };
                self.next_row = false;
                self.move_to(edge(model, paragraph, end), extend);
                (true, false)
            }
            Key::Home | Key::End => {
                let to = self.row_edge(model, &selection.focus, key == Key::End);
                self.move_to(to, extend);
                (true, false)
            }
            Key::PageUp | Key::PageDown => {
                let Some(to) = self.page(&selection.focus, key == Key::PageDown) else {
                    return (false, false);
                };
                let column = self.column;
                self.move_to(to, extend);
                self.column = column;
                (true, false)
            }
            Key::ArrowUp | Key::ArrowDown => {
                let to = self.vertical(model, &selection.focus, key == Key::ArrowDown);
                // The column `vertical` aimed at, kept across the run, which
                // `move_to` would forget.
                let column = self.column;
                self.move_to(to, extend);
                self.column = column;
                (true, false)
            }
            Key::Backspace | Key::Delete => {
                if !selection.is_caret() {
                    return (false, self.delete_selection(model, Group::Deleting));
                }
                let at = selection.focus;
                let (from, to) = if key == Key::Backspace {
                    (step_left(model, &at, word), at)
                } else {
                    let to = step_right(model, &at, word);
                    (at, to)
                };
                if from == to {
                    return (false, false);
                }
                let edit = Edit::Replace { from, to, text: "" };
                (false, self.apply(model, edit, Group::Deleting))
            }
            Key::Enter if modifiers.shift => (false, self.type_text(model, "\n")),
            Key::Enter => {
                self.group = None;
                // A selection the model will not delete is not split in the
                // middle of either.
                let changed = self.delete_selection(model, Group::Other);
                if !changed && !selection.is_caret() {
                    return (false, false);
                }
                let mut changed = changed;
                let at = self.selection.clone().map(|s| s.focus);
                if let Some(at) = at {
                    changed |= self.apply(model, Edit::Split { at }, Group::Other);
                }
                self.group = None;
                (false, changed)
            }
            Key::Tab if modifiers.is_none() => (false, self.type_text(model, "\t")),
            _ => (false, false),
        }
    }

    /// Put the caret at a position, or take the selection's focus there.
    fn move_to(&mut self, to: Position<P>, extend: bool) {
        self.group = None;
        self.column = None;
        match (&mut self.selection, extend) {
            (Some(selection), true) => selection.focus = to,
            _ => self.selection = Some(Selection::caret(to)),
        }
    }

    /// Replace the selection with text, as typing does, and give what was
    /// typed the marks given or taken off at the caret.
    fn type_text<M: Model<Paragraph = P>>(&mut self, model: &mut M, text: &str) -> bool {
        let Some(selection) = &self.selection else {
            return false;
        };
        let (from, to) = self.ordered(selection);
        let marks: Vec<(Mark, bool)> = Mark::ALL
            .into_iter()
            .filter_map(|mark| self.pending_mark(mark).map(|on| (mark, on)))
            .collect();
        let edit = Edit::Replace {
            from: from.clone(),
            to,
            text,
        };
        if !self.apply(model, edit, Group::Typing) {
            return false;
        }
        if let Some(end) = self.selection.as_ref().map(|s| s.focus.clone()) {
            for (mark, on) in marks {
                self.format(model, from.clone(), end.clone(), mark, on, Group::Typing);
            }
        }
        true
    }

    /// Paste text, each of its lines after the first a paragraph of its own,
    /// in one undo step. It stops at the first part the model refuses, so a
    /// selection that cannot be replaced is left as it was.
    fn paste<M: Model<Paragraph = P>>(&mut self, model: &mut M, text: &str) -> bool {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        self.group = None;
        let mut changed = false;
        for (index, line) in text.split('\n').enumerate() {
            if index > 0 {
                let Some(at) = self.selection.clone().map(|s| s.focus) else {
                    break;
                };
                if !self.apply(model, Edit::Split { at }, Group::Other) {
                    break;
                }
                changed = true;
            }
            if index == 0 || !line.is_empty() {
                let Some(selection) = &self.selection else {
                    break;
                };
                let (from, to) = self.ordered(selection);
                let edit = Edit::Replace {
                    from,
                    to,
                    text: line,
                };
                if !self.apply(model, edit, Group::Other) {
                    break;
                }
                changed = true;
            }
        }
        self.group = None;
        changed
    }

    /// Delete what is selected, if anything is.
    fn delete_selection<M: Model<Paragraph = P>>(&mut self, model: &mut M, group: Group) -> bool {
        let Some(selection) = self.selection.as_ref().filter(|s| !s.is_caret()) else {
            return false;
        };
        let (from, to) = self.ordered(selection);
        self.apply(model, Edit::Replace { from, to, text: "" }, group)
    }

    /// Hand an edit to the model, beginning an undo step unless it continues
    /// the kind of edit before it, and put the caret where the model says.
    fn apply<M: Model<Paragraph = P>>(
        &mut self,
        model: &mut M,
        edit: Edit<'_, P>,
        group: Group,
    ) -> bool {
        let new_step = self.group != Some(group);
        let Some(at) = model.apply(edit, new_step) else {
            return false;
        };
        self.group = Some(group);
        self.selection = Some(Selection::caret(at));
        self.next_row = false;
        self.column = None;
        self.pending = None;
        true
    }

    /// Hand a format to the model, as [`Self::apply`] does an edit, leaving
    /// the selection where it is.
    fn format<M: Model<Paragraph = P>>(
        &mut self,
        model: &mut M,
        from: Position<P>,
        to: Position<P>,
        mark: Mark,
        on: bool,
        group: Group,
    ) -> bool {
        let new_step = self.group != Some(group);
        let edit = Edit::Format { from, to, mark, on };
        if model.apply(edit, new_step).is_none() {
            return false;
        }
        self.group = Some(group);
        true
    }

    /// The selection's two ends, the earlier first. Across paragraphs, which
    /// is earlier is the order they were drawn in last frame.
    fn ordered(&self, selection: &Selection<P>) -> (Position<P>, Position<P>) {
        let Selection { anchor, focus } = selection;
        let anchor_first = if anchor.paragraph == focus.paragraph {
            anchor.offset <= focus.offset
        } else {
            match (
                self.placed.get(&anchor.paragraph),
                self.placed.get(&focus.paragraph),
            ) {
                (Some(a), Some(f)) => a.index < f.index,
                _ => true,
            }
        };
        if anchor_first {
            (anchor.clone(), focus.clone())
        } else {
            (focus.clone(), anchor.clone())
        }
    }

    /// The selected text, paragraphs separated by newlines. `None` for a
    /// caret.
    fn selected_text<M: Model<Paragraph = P>>(&self, model: &M) -> Option<String> {
        let selection = self.selection.as_ref().filter(|s| !s.is_caret())?;
        let (from, to) = self.ordered(selection);
        let mut out = String::new();
        let mut paragraph = from.paragraph;
        let mut start = from.offset;
        loop {
            let text = model.text(&paragraph)?;
            if paragraph == to.paragraph {
                out.extend(
                    text.chars()
                        .skip(start)
                        .take(to.offset.saturating_sub(start)),
                );
                return Some(out);
            }
            out.extend(text.chars().skip(start));
            out.push('\n');
            paragraph = model.next(&paragraph)?;
            start = 0;
        }
    }

    /// The start or the end of the row a position is drawn on. Without last
    /// frame's layout of its paragraph, the paragraph's own start or end.
    fn row_edge<M: Model<Paragraph = P>>(
        &mut self,
        model: &M,
        at: &Position<P>,
        end: bool,
    ) -> Position<P> {
        let Some(placed) = self.placed.get(&at.paragraph) else {
            return edge(model, at.paragraph.clone(), end);
        };
        let cursor = CCursor {
            index: CharIndex(placed.map.to_galley(at.offset)),
            prefer_next_row: self.next_row,
        };
        let edge = if end {
            placed.galley.cursor_end_of_row(&cursor)
        } else {
            placed.galley.cursor_begin_of_row(&cursor)
        };
        // The end of a wrapped row and the start of the next are the same
        // offset; which row the caret is drawn on is what was asked for.
        self.next_row = !end;
        Position::new(at.paragraph.clone(), placed.map.to_model(edge.index.0))
    }

    /// The position a view's height above or below another, keeping to the
    /// column, in whichever paragraph last frame drew nearest there. `None`
    /// without last frame's layout of the paragraph the caret is in.
    fn page(&mut self, at: &Position<P>, down: bool) -> Option<Position<P>> {
        let placed = self.placed.get(&at.paragraph)?;
        let cursor = CCursor {
            index: CharIndex(placed.map.to_galley(at.offset)),
            prefer_next_row: self.next_row,
        };
        let caret = placed
            .galley
            .pos_from_cursor(cursor)
            .translate(placed.origin.to_vec2());
        let x = self.column.unwrap_or_else(|| caret.center().x);
        let y = caret.center().y
            + if down {
                self.page_height
            } else {
                -self.page_height
            };
        let distance = |placed: &Placed| {
            let top = placed.origin.y + placed.galley.rect.top();
            let bottom = placed.origin.y + placed.galley.rect.bottom();
            (top - y).max(y - bottom).max(0.0)
        };
        let (paragraph, there) = self
            .placed
            .iter()
            .min_by(|(_, a), (_, b)| distance(a).total_cmp(&distance(b)))?;
        let landed = there
            .galley
            .cursor_from_pos(vec2(x - there.origin.x, y - there.origin.y));
        let to = Position::new(paragraph.clone(), there.map.to_model(landed.index.0));
        self.column = Some(x);
        self.next_row = landed.prefer_next_row;
        Some(to)
    }

    /// The position one row up or down from another, keeping to the column
    /// a run of Up and Down began in, and crossing into the paragraph before
    /// or after at the first or last row.
    fn vertical<M: Model<Paragraph = P>>(
        &mut self,
        model: &M,
        at: &Position<P>,
        down: bool,
    ) -> Position<P> {
        let neighbour = if down {
            model.next(&at.paragraph)
        } else {
            model.previous(&at.paragraph)
        };
        let edge_of_this = || edge(model, at.paragraph.clone(), down);
        let Some(placed) = self.placed.get(&at.paragraph) else {
            return neighbour.map_or_else(edge_of_this, |p| Position::new(p, 0));
        };
        let galley = &placed.galley;
        let cursor = CCursor {
            index: CharIndex(placed.map.to_galley(at.offset)),
            prefer_next_row: self.next_row,
        };
        let x = self
            .column
            .unwrap_or_else(|| placed.origin.x + galley.pos_from_cursor(cursor).center().x);
        self.column = Some(x);

        let row = galley.layout_from_cursor(cursor).row;
        let last_row = galley.rows.len().saturating_sub(1);
        if (down && row < last_row) || (!down && row > 0) {
            let h = Some(x - placed.origin.x);
            let (moved, _) = if down {
                galley.cursor_down_one_row(&cursor, h)
            } else {
                galley.cursor_up_one_row(&cursor, h)
            };
            self.next_row = moved.prefer_next_row;
            return Position::new(at.paragraph.clone(), placed.map.to_model(moved.index.0));
        }

        let Some(neighbour) = neighbour else {
            return edge_of_this();
        };
        let Some(there) = self.placed.get(&neighbour) else {
            return edge(model, neighbour, !down);
        };
        let target_row = if down {
            there.galley.rows.first()
        } else {
            there.galley.rows.last()
        };
        let y = target_row.map_or(0.0, |row| row.rect().center().y);
        let landed = there.galley.cursor_from_pos(vec2(x - there.origin.x, y));
        self.next_row = landed.prefer_next_row;
        Position::new(neighbour.clone(), there.map.to_model(landed.index.0))
    }

    /// What the pointer does on a paragraph: a press puts the caret down,
    /// with Shift extends the selection, a drag extends it through whichever
    /// paragraph the pointer is over, a double-click takes a word and a
    /// triple-click the paragraph.
    fn pointer(
        &mut self,
        ui: &Ui,
        response: &Response,
        paragraph: &P,
        galley: &Galley,
        map: &OffsetMap,
        origin: Pos2,
    ) {
        let at_pointer = |pos: Pos2| galley.cursor_from_pos(pos - origin);
        let (pressed, down, shift, pos) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.modifiers.shift,
                input.pointer.interact_pos(),
            )
        });

        // A press and its release can arrive in one frame, a quick click on a
        // slow frame, and then the button is no longer down on anything; the
        // click says where it was. A paragraph that leaves drags to what is
        // under it takes the click alone, so a press that becomes a drag puts
        // no caret down.
        let placed = if response.sense.senses_drag() {
            pressed && (response.is_pointer_button_down_on() || response.clicked())
        } else {
            response.clicked()
        };
        if placed && let Some(pos) = pos {
            let cursor = at_pointer(pos);
            let to = Position::new(paragraph.clone(), map.to_model(cursor.index.0));
            match &mut self.selection {
                Some(selection) if shift => selection.focus = to,
                _ => self.selection = Some(Selection::caret(to)),
            }
            self.next_row = cursor.prefer_next_row;
            self.group = None;
            self.column = None;
            // A paragraph that does not sense drags leaves them to what is
            // under it, as a slide does to move the shape.
            self.dragging = response.sense.senses_drag();
            self.last_interaction = ui.input(|input| input.time);
            ui.memory_mut(|memory| memory.request_focus(self.id));
        } else if self.dragging && !down {
            self.dragging = false;
        } else if self.dragging
            && let Some(pos) = pos
            // The paragraph the pointer is over, and not merely level with:
            // cells in a table row share their height.
            && response.rect.contains(pos)
            && let Some(selection) = &mut self.selection
        {
            let cursor = at_pointer(pos);
            selection.focus = Position::new(paragraph.clone(), map.to_model(cursor.index.0));
            self.next_row = cursor.prefer_next_row;
            self.last_interaction = ui.input(|input| input.time);
        }

        // egui takes the focus from a widget on any click it does not hover,
        // and the editor's own is under the paragraphs, so a click ending on
        // one would take the keyboard from the caret it just put down. The
        // paragraph is the editor's, and the focus comes back to it.
        if response.clicked() {
            ui.memory_mut(|memory| memory.request_focus(self.id));
        }

        if (response.double_clicked() || response.triple_clicked())
            && let Some(pos) = pos
        {
            let (start, end) = if response.triple_clicked() {
                (0, map.model_len())
            } else {
                let (start, end) = word_around(&galley.job.text, at_pointer(pos).index.0);
                (map.to_model(start), map.to_model(end))
            };
            self.selection = Some(Selection {
                anchor: Position::new(paragraph.clone(), start),
                focus: Position::new(paragraph.clone(), end),
            });
            self.next_row = false;
        }
    }

    /// The part of the selection in this paragraph, in galley characters.
    ///
    /// Paragraphs are painted in document order, so the first end of the
    /// selection met is its start, and every paragraph until the other end is
    /// selected whole.
    fn selected_here(&mut self, paragraph: &P, map: &OffsetMap) -> Option<CCursorRange> {
        let selection = self.selection.as_ref().filter(|s| !s.is_caret())?;
        let (anchor, focus) = (&selection.anchor, &selection.focus);
        let (start, end) = match (
            anchor.paragraph == *paragraph,
            focus.paragraph == *paragraph,
        ) {
            (true, true) => (
                anchor.offset.min(focus.offset),
                anchor.offset.max(focus.offset),
            ),
            (true, false) | (false, true) => {
                let at = if anchor.paragraph == *paragraph {
                    anchor.offset
                } else {
                    focus.offset
                };
                self.placing.inside = !self.placing.inside;
                if self.placing.inside {
                    (at, map.model_len())
                } else {
                    (0, at)
                }
            }
            (false, false) if self.placing.inside => (0, map.model_len()),
            (false, false) => return None,
        };
        Some(CCursorRange::two(
            CCursor::new(map.to_galley(start)),
            CCursor::new(map.to_galley(end)),
        ))
    }
}

/// The mark a key toggles with Ctrl, or Command on a Mac. Strikethrough has
/// no shortcut that people share, so it has none.
fn shortcut(key: Key) -> Option<Mark> {
    match key {
        Key::B => Some(Mark::Bold),
        Key::I => Some(Mark::Italic),
        Key::U => Some(Mark::Underline),
        _ => None,
    }
}

/// How many characters a paragraph holds, or `None` when it is gone.
fn len<M: Model>(model: &M, paragraph: &M::Paragraph) -> Option<usize> {
    model.text(paragraph).map(|text| text.chars().count())
}

/// The start of a paragraph, or its end.
fn edge<M: Model>(model: &M, paragraph: M::Paragraph, end: bool) -> Position<M::Paragraph> {
    let offset = if end {
        len(model, &paragraph).unwrap_or(0)
    } else {
        0
    };
    Position::new(paragraph, offset)
}

/// One character or one word left, into the end of the paragraph before at
/// the start of this one.
fn step_left<M: Model>(
    model: &M,
    at: &Position<M::Paragraph>,
    word: bool,
) -> Position<M::Paragraph> {
    if at.offset == 0 {
        return model
            .previous(&at.paragraph)
            .map_or_else(|| at.clone(), |p| edge(model, p, true));
    }
    let offset = if word {
        let text = model.text(&at.paragraph).unwrap_or_default();
        ccursor_previous_word(&text, CCursor::new(at.offset))
            .index
            .0
    } else {
        at.offset - 1
    };
    Position::new(at.paragraph.clone(), offset)
}

/// One character or one word right, into the start of the paragraph after at
/// the end of this one.
fn step_right<M: Model>(
    model: &M,
    at: &Position<M::Paragraph>,
    word: bool,
) -> Position<M::Paragraph> {
    let text = model.text(&at.paragraph).unwrap_or_default();
    let len = text.chars().count();
    if at.offset >= len {
        return model
            .next(&at.paragraph)
            .map_or_else(|| at.clone(), |p| Position::new(p, 0));
    }
    let offset = if word {
        ccursor_next_word(&text, CCursor::new(at.offset)).index.0
    } else {
        at.offset + 1
    };
    Position::new(at.paragraph.clone(), offset.min(len))
}

/// The word a character index falls in or beside, as a range of indices.
fn word_around(text: &str, index: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    let index = index.min(chars.len());
    let mut start = index;
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = index;
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }
    (start, end)
}

/// A drag that has left the view scrolls it, faster the further out the
/// pointer is, so that a selection can be taken on to where the pointer cannot
/// reach.
fn scroll_toward_pointer(ui: &Ui) {
    const SPEED: f32 = 0.2;
    const SLOWEST: f32 = 2.0;
    const FASTEST: f32 = 30.0;
    let Some(pointer) = ui.input(|input| input.pointer.latest_pos()) else {
        return;
    };
    let view = ui.clip_rect();
    let past = (pointer.y - view.bottom()).max(0.0) - (view.top() - pointer.y).max(0.0);
    if past == 0.0 {
        return;
    }
    let step = (past.abs() * SPEED).clamp(SLOWEST, FASTEST);
    ui.scroll_with_delta(vec2(0.0, -step.copysign(past)));
    ui.ctx().request_repaint();
}
