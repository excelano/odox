//! The paragraphs under a flow's root, as the page editor sees them.
//!
//! A paragraph is named by its path of child indices from the root the flow
//! draws, which is the path [`crate::Flow`] reports it under. The editor's
//! edits become `odox-core` edits on the tree, and an edit that begins an undo
//! step is recorded in [`Editing`] once it has succeeded. A format writes
//! automatic styles, which the document's [`Styles`] is told of as they are
//! written. DESIGN.md §11.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::cell::OnceCell;
use std::ops::Range;

use eframe::egui::Id;
use egui_richedit::{
    Edit, Fragment, Mark, Model, Position, Property, RichEdit, Script, Selection, Value,
};
use odox_core::edit::{self, LineSpacing, TableEdit, html, is_paragraph};
use odox_core::{Element, Kept, ListKind, Ns, Package, Refused, Styles, TextAlign};

use crate::Editing;
use crate::find::{Match, Replaced};
use odox_core::edit::is_block_container;

/// The editor over a flow's paragraphs.
pub type PageEditor = RichEdit<Vec<usize>>;

/// The id the page editor takes the keyboard under. The shell reads it to
/// know that typing on the page still leaves its shortcuts to it.
pub(crate) fn page_editor_id() -> Id {
    Id::unique("odox-page-editor")
}

/// A page editor with nothing selected.
pub fn page_editor() -> PageEditor {
    RichEdit::new(page_editor_id()).with_clipboard(&crate::clipboard::SYSTEM)
}

/// What a button for a kind of paragraph asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    /// A heading of a level, or body text again where the paragraphs already
    /// are one.
    Heading(u8),
    /// Body text.
    Body,
    /// A bulleted list, or no list where the paragraphs already are one.
    Bullets,
    /// A numbered list, or no list where the paragraphs already are one.
    Numbers,
}

/// A picture a person chose to put in, and what they said it shows.
pub struct Picture {
    /// The file's bytes.
    pub bytes: Vec<u8>,
    /// Its media type, from [`odox_core::picture::kind`].
    pub media_type: &'static str,
    /// The extension its part is named with.
    pub extension: &'static str,
    /// How wide and how tall it is drawn, in points.
    pub size: (f32, f32),
    /// What it shows, or `None` for decoration only.
    pub description: Option<String>,
}

/// A copy made here: its paragraphs as [`edit::slice`] carries them, and the
/// HTML written of them for another program.
struct Copied {
    items: Vec<Element>,
    html: String,
}

/// Paragraphs another program put on the clipboard as HTML.
struct Read(Vec<html::Paragraph>);

/// The paragraphs under a root in a content tree, for one frame's editing.
pub struct FlowModel<'a> {
    content: &'a mut Element,
    styles: &'a mut Styles,
    /// Where the flow's root is under the content root.
    root: Vec<usize>,
    editing: &'a mut Editing,
    /// The part of the root the paragraphs are kept to, as a path under it:
    /// on a slide, the one shape being typed into.
    scope: Vec<usize>,
    /// Put in front of every path an undo step remembers, for a view whose
    /// paths mean something only with it: on a deck, which slide.
    tag: Vec<usize>,
    /// Every editable paragraph in drawing order, found when first asked for
    /// and forgotten when an edit changes it.
    order: OnceCell<Vec<Vec<usize>>>,
    /// Whether the tree is the styles part, a page header's or footer's, which
    /// an undo step puts back in its place rather than the content's.
    styles_part: bool,
}

impl<'a> FlowModel<'a> {
    /// The paragraphs under the element at a path in a content tree, whose
    /// document's styles are `styles`.
    pub fn new(
        content: &'a mut Element,
        styles: &'a mut Styles,
        root: Vec<usize>,
        editing: &'a mut Editing,
    ) -> Self {
        Self {
            content,
            styles,
            root,
            editing,
            scope: Vec::new(),
            tag: Vec::new(),
            order: OnceCell::new(),
            styles_part: false,
        }
    }

    /// Have the caret an undo step remembers begin with these indices, which
    /// the view takes off again when it is asked to put the caret back.
    #[must_use]
    pub fn tagged(mut self, tag: Vec<usize>) -> Self {
        self.tag = tag;
        self
    }

    /// Keep to the paragraphs under one element beneath the root, so that
    /// moving and joining never leave it: a slide's label does not run on
    /// into the next shape.
    #[must_use]
    pub fn within(mut self, scope: Vec<usize>) -> Self {
        self.scope = scope;
        self
    }

    /// Edit the styles part rather than the content: the tree given is the
    /// styles part, and undo puts it back there.
    #[must_use]
    pub fn in_styles_part(mut self) -> Self {
        self.styles_part = true;
        self
    }

    /// Remember the tree as it was before an edit, as one step to undo.
    fn record(&mut self, before: Element, caret: Option<Position<Vec<usize>>>) {
        self.record_step(before, Kept::default(), caret);
    }

    /// The same, with the package entries the edit is about to write.
    fn record_step(&mut self, before: Element, parts: Kept, caret: Option<Position<Vec<usize>>>) {
        if self.styles_part {
            self.editing.record_styles_part(Some(before), parts, caret);
        } else {
            self.editing.record_step(before, parts, caret);
        }
    }

    fn root(&self) -> Option<&Element> {
        self.content.at(&self.root)
    }

    fn paragraph_mut(&mut self, path: &[usize]) -> Option<&mut Element> {
        self.content
            .at_mut(&self.root)?
            .at_mut(path)
            .filter(|element| is_paragraph(element))
    }

    fn order(&self) -> &[Vec<usize>] {
        self.order.get_or_init(|| {
            let mut out = self.root().map(paragraph_paths).unwrap_or_default();
            out.retain(|path| path.starts_with(&self.scope));
            out
        })
    }

    fn neighbour(&self, paragraph: &[usize], step: isize) -> Option<Vec<usize>> {
        let order = self.order();
        let index = order.iter().position(|p| p == paragraph)?;
        order.get(index.checked_add_signed(step)?).cloned()
    }

    fn replace(
        &mut self,
        from: &Position<Vec<usize>>,
        to: &Position<Vec<usize>>,
        text: &str,
    ) -> Option<()> {
        let root = self.content.at_mut(&self.root)?;
        match edit::replace_range(
            root,
            (&from.paragraph, from.offset),
            (&to.paragraph, to.offset),
            text,
        ) {
            Ok(()) => Some(()),
            Err(Refused::Structure) => self.clear_between(from, to, text),
            Err(_) => None,
        }
    }

    /// A range that crosses a table or a frame, which no join takes apart:
    /// the selected text goes from every paragraph it covers, each paragraph
    /// stays where it is, and the new text goes in at the start. Nothing to
    /// remove and nothing to put in, as Backspace at the start of a cell, is
    /// refused rather than made an edit of nothing.
    fn clear_between(
        &mut self,
        from: &Position<Vec<usize>>,
        to: &Position<Vec<usize>>,
        text: &str,
    ) -> Option<()> {
        let ranges = self.covered(from, to)?;
        if text.is_empty() && ranges.iter().all(|(_, range)| range.is_empty()) {
            return None;
        }
        for (path, range) in ranges.into_iter().rev() {
            let with = if path == from.paragraph { text } else { "" };
            let paragraph = self.paragraph_mut(&path)?;
            edit::replace(paragraph, range, with);
        }
        Some(())
    }

    /// Each paragraph from one position to another in drawing order, with
    /// the range of its text between them.
    fn covered(
        &self,
        from: &Position<Vec<usize>>,
        to: &Position<Vec<usize>>,
    ) -> Option<Vec<(Vec<usize>, Range<usize>)>> {
        let order = self.order();
        let first = order.iter().position(|p| *p == from.paragraph)?;
        let last = order.iter().position(|p| *p == to.paragraph)?;
        let covered: Vec<Vec<usize>> = order.get(first..=last)?.to_vec();
        Some(
            covered
                .into_iter()
                .map(|path| {
                    let len = self.text(&path).map_or(0, |t| t.chars().count());
                    let start = if path == from.paragraph {
                        from.offset
                    } else {
                        0
                    };
                    let end = if path == to.paragraph { to.offset } else { len };
                    (path, start..end.max(start))
                })
                .collect(),
        )
    }

    /// The paragraphs a selection runs over, in the order they are drawn.
    fn selected_paragraphs(&self, selection: &Selection<Vec<usize>>) -> Option<Vec<Vec<usize>>> {
        let order = self.order();
        let anchor = order
            .iter()
            .position(|p| *p == selection.anchor.paragraph)?;
        let focus = order.iter().position(|p| *p == selection.focus.paragraph)?;
        Some(order[anchor.min(focus)..=anchor.max(focus)].to_vec())
    }

    /// Whether a block is what every paragraph a selection runs over already
    /// is, which is when its button is lit and pressing it takes it off.
    pub fn is_block(&self, selection: &Selection<Vec<usize>>, block: Block) -> bool {
        let Some(paragraphs) = self.selected_paragraphs(selection) else {
            return false;
        };
        paragraphs.iter().all(|paragraph| {
            let path = [self.root.as_slice(), paragraph].concat();
            let (heading, list) = edit::block_state(self.content, &path, self.styles);
            match block {
                Block::Heading(level) => heading == Some(level),
                Block::Body => heading.is_none(),
                Block::Bullets => list == Some(ListKind::Bullet),
                Block::Numbers => list == Some(ListKind::Number),
            }
        })
    }

    /// Whether every paragraph a selection runs over is a heading, which is
    /// what moving up and down the outline needs.
    pub fn all_headings(&self, selection: &Selection<Vec<usize>>) -> bool {
        self.selected_paragraphs(selection)
            .is_some_and(|paragraphs| {
                paragraphs.iter().all(|paragraph| {
                    let path = [self.root.as_slice(), paragraph].concat();
                    edit::block_state(self.content, &path, self.styles)
                        .0
                        .is_some()
                })
            })
    }

    /// Move the list items a selection runs over a level in (`delta` above
    /// zero) or out, as one step to undo: with a caret, only where it stands at
    /// the start of an item, and never in a table, where Tab moves between
    /// cells. Answers whether the keys were the list's.
    ///
    /// # Errors
    ///
    /// The move was refused, and nothing changed: an item to lift out sits in
    /// an item that goes on after its list.
    pub fn list_level(&mut self, editor: &mut PageEditor, delta: i8) -> Result<bool, Refused> {
        let Some(selection) = editor.selection().cloned() else {
            return Ok(false);
        };
        let Some(paragraphs) = self.selected_paragraphs(&selection) else {
            return Ok(false);
        };
        let listed = |paragraph: &Vec<usize>| {
            let path = [self.root.as_slice(), paragraph].concat();
            edit::block_state(self.content, &path, self.styles)
                .1
                .is_some()
        };
        let applies = !self.in_table(&selection)
            && if selection.is_caret() {
                selection.focus.offset == 0 && listed(&selection.focus.paragraph)
            } else {
                paragraphs.len() > 1 && paragraphs.iter().all(listed)
            };
        if !applies {
            return Ok(false);
        }
        let mut refused = None;
        self.over_paragraphs(editor, |model, paragraphs| {
            let paths: Vec<Vec<usize>> = paragraphs
                .iter()
                .map(|paragraph| [model.root.as_slice(), paragraph].concat())
                .collect();
            edit::list_level(model.content, &paths, delta)
                .map_err(|reason| refused = Some(reason))
                .ok()
        });
        refused.map_or(Ok(true), Err)
    }

    /// Start the numbering again at the item the selection begins in, or carry
    /// its list's numbering on from the list before, as one step to undo.
    /// Answers whether the document changed.
    pub fn numbering(&mut self, editor: &mut PageEditor, restart: bool) -> bool {
        self.over_paragraphs(editor, |model, paragraphs| {
            let path = [model.root.as_slice(), paragraphs.first()?].concat();
            if restart {
                edit::restart_numbering(model.content, &path).ok()
            } else {
                edit::continue_numbering(model.content, &path).ok()
            }
        })
    }

    /// Whether a page break begins the paragraph a selection ends in.
    pub fn page_break_here(&self, selection: &Selection<Vec<usize>>) -> bool {
        let path = [self.root.as_slice(), &selection.focus.paragraph].concat();
        edit::page_break(self.content, &path, self.styles)
    }

    /// Begin a page at the caret, as Ctrl+Enter does: the paragraph split
    /// there and the page begun at its second half, or at the paragraph where
    /// the caret is at its start, as one step to undo with the caret at the
    /// page's first character. Answers whether the document changed.
    pub fn insert_page_break(&mut self, editor: &mut PageEditor) -> bool {
        let Some(selection) = editor.selection().filter(|s| s.is_caret()).cloned() else {
            return false;
        };
        let at = selection.focus;
        let before = self.content.clone();
        let count = self.content.children.len();
        let path = [self.root.as_slice(), &at.paragraph].concat();
        let Ok(begins) = edit::insert_page_break(self.content, &path, at.offset, self.styles)
        else {
            *self.content = before;
            return false;
        };
        if *self.content == before {
            return false;
        }
        self.follow_styles(count);
        let Some(page) = begins
            .strip_prefix(self.root.as_slice())
            .map(<[usize]>::to_vec)
        else {
            return false;
        };
        self.order = OnceCell::new();
        self.editing.touch();
        self.record(
            before,
            Some(Position::new(
                [self.tag.as_slice(), &at.paragraph].concat(),
                at.offset,
            )),
        );
        editor.select(Selection::caret(Position::new(page, 0)));
        true
    }

    /// Take the page break off the paragraph a selection ends in, as one step
    /// to undo. Answers whether the document changed.
    pub fn remove_page_break(&mut self, editor: &mut PageEditor) -> bool {
        let Some(focus) = editor.selection().map(|s| s.focus.paragraph.clone()) else {
            return false;
        };
        self.over_paragraphs(editor, |model, _| {
            let before = model.content.children.len();
            let path = [model.root.as_slice(), &focus].concat();
            edit::remove_page_break(model.content, &path, model.styles).ok()?;
            model.follow_styles(before);
            Some(())
        })
    }

    /// Backspace at the start of a paragraph a page break begins, or Delete at
    /// the end of the one before it: the break comes off, as one step to undo,
    /// before the key would join anything, and the caret stays. Answers
    /// whether the key was taken.
    pub fn remove_break_by_key(&mut self, editor: &mut PageEditor, backspace: bool) -> bool {
        let Some(at) = editor
            .selection()
            .filter(|s| s.is_caret())
            .map(|s| s.focus.clone())
        else {
            return false;
        };
        let broken = if backspace {
            (at.offset == 0).then(|| at.paragraph.clone())
        } else {
            self.text(&at.paragraph)
                .is_some_and(|text| text.chars().count() == at.offset)
                .then(|| self.neighbour(&at.paragraph, 1))
                .flatten()
        };
        let Some(broken) = broken else {
            return false;
        };
        let path = [self.root.as_slice(), &broken].concat();
        if !edit::page_break(self.content, &path, self.styles) {
            return false;
        }
        self.over_paragraphs(editor, |model, _| {
            let before = model.content.children.len();
            let path = [model.root.as_slice(), &broken].concat();
            edit::remove_page_break(model.content, &path, model.styles).ok()?;
            model.follow_styles(before);
            Some(())
        })
    }

    /// Whether any paragraph a selection runs over is a heading.
    pub fn has_heading(&self, selection: &Selection<Vec<usize>>) -> bool {
        self.selected_paragraphs(selection)
            .is_some_and(|paragraphs| {
                paragraphs.iter().any(|paragraph| {
                    let path = [self.root.as_slice(), paragraph].concat();
                    edit::block_state(self.content, &path, self.styles)
                        .0
                        .is_some()
                })
            })
    }

    /// Replace each match of a search with a text, last first so that the
    /// offsets of the ones before hold. The first replacement begins an undo
    /// step when `new_step` says so and the rest join it. Answers how many
    /// were replaced.
    pub fn replace_matches(&mut self, matches: &[&Match], with: &str, new_step: bool) -> Replaced {
        let mut done = Replaced::default();
        for found in matches.iter().rev() {
            let edit = Edit::Replace {
                from: Position::new(found.paragraph.clone(), found.range.start),
                to: Position::new(found.paragraph.clone(), found.range.end),
                text: with,
            };
            if self.apply(edit, new_step && done.replaced == 0).is_some() {
                done.replaced += 1;
            } else {
                done.skipped += 1;
            }
        }
        done
    }

    /// Make the paragraphs a selection runs over a kind of paragraph, or take
    /// them out of it where they already are one, as one step to undo, and put
    /// the selection back over the same paragraphs. Answers whether the
    /// document changed.
    pub fn apply_block(&mut self, editor: &mut PageEditor, block: Block) -> bool {
        let lit = editor
            .selection()
            .is_some_and(|selection| self.is_block(selection, block));
        self.over_paragraphs(editor, |model, paragraphs| match block {
            Block::Heading(_) | Block::Body => {
                let level = match block {
                    Block::Heading(level) if !lit => Some(level),
                    _ => None,
                };
                model.set_headings(paragraphs, level)
            }
            Block::Bullets | Block::Numbers => {
                let kind = match block {
                    Block::Bullets => ListKind::Bullet,
                    _ => ListKind::Number,
                };
                model.set_lists(paragraphs, (!lit).then_some(kind))
            }
        })
    }

    /// Move every heading a selection runs over a level up the outline
    /// (`delta` below zero) or down it, as one step to undo. Answers whether
    /// the document changed.
    pub fn shift_headings(&mut self, editor: &mut PageEditor, delta: i8) -> bool {
        self.over_paragraphs(editor, |model, paragraphs| {
            for paragraph in paragraphs {
                let before = model.content.children.len();
                let path = [model.root.as_slice(), paragraph].concat();
                edit::shift_heading(model.content, &path, delta, model.styles).ok()?;
                model.follow_styles(before);
            }
            Some(())
        })
    }

    /// How every paragraph a selection runs over is aligned, where they are
    /// all aligned alike.
    pub fn alignment(&self, selection: &Selection<Vec<usize>>) -> Option<TextAlign> {
        let paragraphs = self.selected_paragraphs(selection)?;
        let mut aligned = paragraphs.iter().map(|paragraph| {
            let path = [self.root.as_slice(), paragraph].concat();
            edit::alignment(self.content, &path, self.styles)
        });
        let first = aligned.next()?;
        aligned.all(|align| align == first).then_some(first)
    }

    /// Align the paragraphs a selection runs over, as one step to undo, and
    /// put the selection back over them. Answers whether the document changed.
    pub fn apply_alignment(&mut self, editor: &mut PageEditor, align: TextAlign) -> bool {
        self.over_paragraphs(editor, |model, paragraphs| {
            let before = model.content.children.len();
            let paths: Vec<Vec<usize>> = paragraphs
                .iter()
                .map(|paragraph| [model.root.as_slice(), paragraph].concat())
                .collect();
            edit::set_alignment(model.content, &paths, align, model.styles).ok()?;
            model.follow_styles(before);
            Some(())
        })
    }

    /// Move the paragraphs a selection runs over a tab's width in (`step`
    /// above zero) or out, list items a level, as one step to undo. Answers
    /// whether the document changed.
    pub fn apply_indent(&mut self, editor: &mut PageEditor, step: i8) -> bool {
        self.over_paragraphs(editor, |model, paragraphs| {
            let before = model.content.children.len();
            let paths: Vec<Vec<usize>> = paragraphs
                .iter()
                .map(|paragraph| [model.root.as_slice(), paragraph].concat())
                .collect();
            edit::set_indent(model.content, &paths, step, model.styles).ok()?;
            model.follow_styles(before);
            Some(())
        })
    }

    /// Space the lines of the paragraphs a selection runs over, as one step to
    /// undo. Answers whether the document changed.
    pub fn apply_line_spacing(&mut self, editor: &mut PageEditor, spacing: LineSpacing) -> bool {
        self.over_paragraphs(editor, |model, paragraphs| {
            let before = model.content.children.len();
            let paths: Vec<Vec<usize>> = paragraphs
                .iter()
                .map(|paragraph| [model.root.as_slice(), paragraph].concat())
                .collect();
            edit::set_line_spacing(model.content, &paths, spacing, model.styles).ok()?;
            model.follow_styles(before);
            Some(())
        })
    }

    /// The line spacing every paragraph a selection runs over has, where they
    /// all have the same one of those offered.
    pub fn line_spacing(&self, selection: &Selection<Vec<usize>>) -> Option<LineSpacing> {
        let paragraphs = self.selected_paragraphs(selection)?;
        let mut spaced = paragraphs.iter().map(|paragraph| {
            let path = [self.root.as_slice(), paragraph].concat();
            edit::line_spacing(self.content, &path, self.styles)
        });
        let first = spaced.next()?;
        spaced.all(|spacing| spacing == first).then_some(first)?
    }

    /// A change to the paragraphs a selection runs over that adds and removes
    /// none: tried on the tree, put back if it fails or changes nothing, and
    /// otherwise recorded as one step to undo with the selection put back over
    /// the same paragraphs.
    fn over_paragraphs(
        &mut self,
        editor: &mut PageEditor,
        change: impl FnOnce(&mut Self, &[Vec<usize>]) -> Option<()>,
    ) -> bool {
        let Some(selection) = editor.selection().cloned() else {
            return false;
        };
        let Some(paragraphs) = self.selected_paragraphs(&selection) else {
            return false;
        };
        let place = |path: &Vec<usize>, order: &[Vec<usize>]| order.iter().position(|p| p == path);
        let (Some(anchor), Some(focus)) = (
            place(&selection.anchor.paragraph, self.order()),
            place(&selection.focus.paragraph, self.order()),
        ) else {
            return false;
        };
        let before = self.content.clone();
        let root = self.root.clone();
        if change(self, &paragraphs).is_none() || *self.content == before {
            *self.content = before;
            self.root = root;
            return false;
        }
        self.order = OnceCell::new();
        let begins = Position::new(
            [self.tag.as_slice(), &selection.focus.paragraph].concat(),
            selection.focus.offset,
        );
        self.editing.touch();
        self.record(before, Some(begins));
        let order = self.order().to_vec();
        if let (Some(a), Some(f)) = (order.get(anchor), order.get(focus)) {
            editor.select(Selection {
                anchor: Position::new(a.clone(), selection.anchor.offset),
                focus: Position::new(f.clone(), selection.focus.offset),
            });
        }
        true
    }

    /// The families the document declares, for a list to offer first.
    pub fn document_families(&self) -> Vec<String> {
        self.styles.font_families()
    }

    /// Whether the paragraph a selection ends in is in a table.
    pub fn in_table(&self, selection: &Selection<Vec<usize>>) -> bool {
        let path = [self.root.as_slice(), &selection.focus.paragraph].concat();
        edit::table_at(self.content, &path).is_some()
    }

    /// Put a table of `rows` by `columns` beside the paragraph the caret is
    /// in, as wide as `width` points, as one step to undo, with the caret in
    /// its first cell.
    ///
    /// # Errors
    ///
    /// What the edit refused, with the document left as it was.
    pub fn insert_table(
        &mut self,
        editor: &mut PageEditor,
        rows: usize,
        columns: usize,
        width: f32,
    ) -> Result<(), Refused> {
        self.structural(editor, |content, paragraph, styles| {
            edit::insert_table(content, paragraph, rows, columns, width, styles)
        })
    }

    /// Change the table the caret is in, as one step to undo.
    ///
    /// # Errors
    ///
    /// What the edit refused, with the document left as it was.
    pub fn edit_table(
        &mut self,
        editor: &mut PageEditor,
        change: TableEdit,
    ) -> Result<(), Refused> {
        self.structural(editor, |content, paragraph, styles| {
            edit::edit_table(content, paragraph, change, styles)
        })
    }

    /// Merge the cells a selection runs over, from the cell it starts in to
    /// the cell it ends in, as one step to undo, the caret going to the merged
    /// cell's first paragraph.
    ///
    /// # Errors
    ///
    /// The selection's ends are not in one table, or the rectangle cuts across
    /// a cell merged already.
    pub fn merge_cells(&mut self, editor: &mut PageEditor) -> Result<(), Refused> {
        let anchor = editor
            .selection()
            .map(|s| [self.root.as_slice(), &s.anchor.paragraph].concat())
            .ok_or(Refused::NotFound)?;
        self.structural(editor, |content, focus, _| {
            let (Some(from), Some(to)) = (
                edit::table_at(content, &anchor),
                edit::table_at(content, focus),
            ) else {
                return Err(Refused::NotFound);
            };
            if from.table != to.table {
                return Err(Refused::NotFound);
            }
            let rows = from.row.min(to.row)..from.row.max(to.row) + 1;
            let columns = from.column.min(to.column)..from.column.max(to.column) + 1;
            let (rows, columns) = edit::merge_rectangle(content, &from.table, rows, columns)?;
            edit::merge_cells(content, &from.table, rows, columns)
        })
    }

    /// Split the merged cell the caret is in back into the cells it covers, as
    /// one step to undo.
    ///
    /// # Errors
    ///
    /// The caret is not in a table, or the table has repeated rows or cells.
    pub fn split_cell(&mut self, editor: &mut PageEditor) -> Result<(), Refused> {
        self.structural(editor, |content, focus, _| {
            let cell = (1..focus.len())
                .rev()
                .find(|&depth| {
                    content
                        .at(&focus[..depth])
                        .is_some_and(|e| e.is(&Ns::Table, "table-cell"))
                })
                .map(|depth| focus[..depth].to_vec())
                .ok_or(Refused::NotFound)?;
            edit::split_cell(content, &cell)?;
            Ok(focus.to_vec())
        })
    }

    /// Make every column of the table the caret is in the same width, as one
    /// step to undo.
    ///
    /// # Errors
    ///
    /// The caret is not in a table, or a column's width is not stated.
    pub fn equal_widths(&mut self, editor: &mut PageEditor) -> Result<(), Refused> {
        self.structural(editor, |content, focus, styles| {
            let table = edit::table_at(content, focus)
                .ok_or(Refused::NotFound)?
                .table;
            edit::equal_widths(content, &table, styles)?;
            Ok(focus.to_vec())
        })
    }

    /// Make a column of a table, by its path under the root the flow draws, a
    /// width in points, as a dragged edge does, as one step to undo.
    ///
    /// # Errors
    ///
    /// As [`edit::set_column_width`].
    pub fn set_column_width(
        &mut self,
        table: &[usize],
        column: usize,
        width: f32,
    ) -> Result<(), Refused> {
        let before = self.content.clone();
        let children = self.content.children.len();
        let path = [self.root.as_slice(), table].concat();
        if let Err(refused) =
            edit::set_column_width(self.content, &path, column, width, self.styles)
        {
            *self.content = before;
            return Err(refused);
        }
        if *self.content == before {
            return Ok(());
        }
        self.follow_styles(children);
        self.editing.touch();
        self.record_step(before, Kept::default(), None);
        Ok(())
    }

    /// Put an empty comment on what is selected in the paragraph the
    /// selection ends in, or at the caret, signed and dated now, as one step
    /// to undo, the caret going to it so its box opens to be written in.
    ///
    /// # Errors
    ///
    /// As [`edit::add_annotation`].
    pub fn add_comment(&mut self, editor: &mut PageEditor) -> Result<(), Refused> {
        let selection = editor.selection().cloned().ok_or(Refused::NotFound)?;
        let paragraph = selection.focus.paragraph.clone();
        let range = if selection.anchor.paragraph == paragraph {
            let (a, b) = (selection.anchor.offset, selection.focus.offset);
            a.min(b)..a.max(b)
        } else {
            selection.focus.offset..selection.focus.offset
        };
        let before = self.content.clone();
        let path = [self.root.as_slice(), &paragraph].concat();
        if let Err(refused) = edit::add_annotation(
            self.content,
            &path,
            range.clone(),
            "",
            crate::comments::author().as_deref(),
            &crate::comments::now(),
        ) {
            *self.content = before;
            return Err(refused);
        }
        self.order = OnceCell::new();
        self.editing.touch();
        self.record(
            before,
            Some(Position::new(
                [self.tag.as_slice(), &paragraph].concat(),
                range.start,
            )),
        );
        editor.select(Selection::caret(Position::new(paragraph, range.start)));
        Ok(())
    }

    /// The places in the document a link may go to: each heading, as
    /// `#Heading|outline`, and each bookmark, as `#name`, with what a person
    /// reads for it.
    pub fn places(&self) -> Vec<(String, String)> {
        fn walk(element: &Element, into: &mut Vec<(String, String)>) {
            for child in element.elements() {
                if child.is(&Ns::Text, "tracked-changes") {
                    continue;
                }
                if child.is(&Ns::Text, "h") {
                    let heading = child.plain_text().trim().to_owned();
                    if !heading.trim().is_empty() {
                        into.push((heading.clone(), format!("#{heading}|outline")));
                    }
                }
                if (child.is(&Ns::Text, "bookmark") || child.is(&Ns::Text, "bookmark-start"))
                    && let Some(name) = child.attr(&Ns::Text, "name")
                    && !name.starts_with("__")
                {
                    into.push((name.to_owned(), format!("#{name}")));
                }
                walk(child, into);
            }
        }
        let mut places = Vec::new();
        if let Some(body) = self.content.at(&self.root) {
            walk(body, &mut places);
        }
        places
    }

    /// What a link question starts from at the caret: the link the caret is
    /// in, or the text selected within one paragraph, or nothing at the
    /// caret. `None` where the selection runs across paragraphs.
    pub fn link_here(&self, editor: &PageEditor) -> Option<LinkHere> {
        let selection = editor.selection()?;
        if selection.anchor.paragraph != selection.focus.paragraph {
            return None;
        }
        let paragraph = selection.focus.paragraph.clone();
        let element = self
            .content
            .at(&[self.root.as_slice(), &paragraph].concat())?;
        let (a, b) = (selection.anchor.offset, selection.focus.offset);
        let (low, high) = (a.min(b), a.max(b));
        let in_link = edit::link_at(element, if low == high { b } else { low })
            .filter(|(_, range, _)| range.start <= low && high <= range.end);
        if let Some((link, range, href)) = in_link {
            let text = edit::text(element)
                .chars()
                .skip(range.start)
                .take(range.len())
                .collect();
            return Some(LinkHere {
                paragraph,
                range,
                link: Some(link),
                text,
                href,
            });
        }
        let range = a.min(b)..a.max(b);
        let text = edit::text(element)
            .chars()
            .skip(range.start)
            .take(range.len())
            .collect();
        Some(LinkHere {
            paragraph,
            range,
            link: None,
            text,
            href: String::new(),
        })
    }

    /// Make or change the link a question started from, with its text and
    /// address, as one step to undo; text that differs from what was there
    /// takes its place.
    ///
    /// # Errors
    ///
    /// As [`edit::make_link`] and [`edit::set_link`], and no text at all is
    /// refused.
    pub fn put_link(
        &mut self,
        editor: &mut PageEditor,
        here: &LinkHere,
        text: &str,
        href: &str,
    ) -> Result<(), Refused> {
        if !edit::followable(href) {
            return Err(Refused::Address);
        }
        if text.is_empty() {
            return Err(Refused::Structure);
        }
        if here.link.is_some() && here.text == text && here.href == href {
            return Ok(());
        }
        let path = [self.root.as_slice(), &here.paragraph].concat();
        let still = self
            .content
            .at(&path)
            .map(|paragraph| {
                edit::text(paragraph)
                    .chars()
                    .skip(here.range.start)
                    .take(here.range.len())
                    .collect::<String>()
            })
            .is_some_and(|there| there == here.text);
        if !still {
            return Err(Refused::NotFound);
        }
        let before = self.content.clone();
        let done = match &here.link {
            Some(link) => {
                edit::set_link(self.content, &[path.as_slice(), link].concat(), href, text)
            }
            None => self.wrap_link(&path, here, text, href).map(|_| ()),
        };
        if let Err(refused) = done {
            *self.content = before;
            return Err(refused);
        }
        let end = here.range.start + text.chars().count();
        self.order = OnceCell::new();
        self.editing.touch();
        self.record(
            before,
            Some(Position::new(
                [self.tag.as_slice(), &here.paragraph].concat(),
                here.range.start,
            )),
        );
        editor.select(Selection::caret(Position::new(here.paragraph.clone(), end)));
        Ok(())
    }

    fn wrap_link(
        &mut self,
        path: &[usize],
        here: &LinkHere,
        text: &str,
        href: &str,
    ) -> Result<Vec<usize>, Refused> {
        let (from, to, with) = edit::difference(&here.text, text);
        if from != to || !with.is_empty() {
            let paragraph = self.content.at_mut(path).ok_or(Refused::NotFound)?;
            let start = here.range.start;
            edit::replace(paragraph, start + from..start + to, &with);
        }
        let end = here.range.start + text.chars().count();
        edit::make_link(self.content, (path, here.range.start), (path, end), href)
    }

    /// Take off the link a question started from, its text staying, as one
    /// step to undo.
    ///
    /// # Errors
    ///
    /// There is no link there.
    pub fn take_link(&mut self, here: &LinkHere) -> Result<(), Refused> {
        let link = here.link.as_ref().ok_or(Refused::NotFound)?;
        let before = self.content.clone();
        let path = [self.root.as_slice(), &here.paragraph, link].concat();
        if self
            .content
            .at(&path)
            .and_then(|a| a.attr(&Ns::Xlink, "href"))
            != Some(&here.href)
        {
            return Err(Refused::NotFound);
        }
        if let Err(refused) = edit::remove_link(self.content, &path) {
            *self.content = before;
            return Err(refused);
        }
        self.order = OnceCell::new();
        self.editing.touch();
        self.record(
            before,
            Some(Position::new(
                [self.tag.as_slice(), &here.paragraph].concat(),
                here.range.start,
            )),
        );
        Ok(())
    }

    /// Put an empty footnote or endnote at the caret, as one step to undo,
    /// the caret staying at it so its box opens to be written in.
    ///
    /// # Errors
    ///
    /// As [`edit::insert_note`].
    pub fn insert_note(
        &mut self,
        editor: &mut PageEditor,
        class: edit::NoteClass,
    ) -> Result<(), Refused> {
        let caret = editor
            .selection()
            .map(|s| s.focus.clone())
            .ok_or(Refused::NotFound)?;
        let before = self.content.clone();
        let path = [self.root.as_slice(), &caret.paragraph].concat();
        if let Err(refused) =
            edit::insert_note(self.content, &path, caret.offset, class, self.styles)
        {
            *self.content = before;
            return Err(refused);
        }
        self.order = OnceCell::new();
        self.editing.touch();
        self.record(
            before,
            Some(Position::new(
                [self.tag.as_slice(), &caret.paragraph].concat(),
                caret.offset,
            )),
        );
        editor.select(Selection::caret(caret));
        Ok(())
    }

    /// Put a field in at the caret, as one step to undo, a date being today's.
    ///
    /// # Errors
    ///
    /// As [`edit::insert_field`].
    pub fn insert_field(
        &mut self,
        editor: &mut PageEditor,
        field: edit::Field,
    ) -> Result<(), Refused> {
        let caret = editor
            .selection()
            .map(|s| s.focus.clone())
            .ok_or(Refused::NotFound)?;
        let before = self.content.clone();
        let path = [self.root.as_slice(), &caret.paragraph].concat();
        if let Err(refused) =
            edit::insert_field(self.content, &path, caret.offset, field, crate::today())
        {
            *self.content = before;
            return Err(refused);
        }
        self.order = OnceCell::new();
        self.editing.touch();
        self.record(
            before,
            Some(Position::new(
                [self.tag.as_slice(), &caret.paragraph].concat(),
                caret.offset,
            )),
        );
        editor.select(Selection::caret(caret));
        Ok(())
    }

    /// Do what was done in a note's box, the note by its path under the root
    /// the flow draws: its new text, or its removal, as one step to undo.
    ///
    /// # Errors
    ///
    /// There is no note there.
    pub fn note_body(
        &mut self,
        note: &[usize],
        done: &crate::note_popup::NoteResponse,
    ) -> Result<(), Refused> {
        let before = self.content.clone();
        let path = [self.root.as_slice(), note].concat();
        let changed = match done {
            crate::note_popup::NoteResponse::Saved(text) => {
                edit::set_note_text(self.content, &path, text)
            }
            crate::note_popup::NoteResponse::Deleted => edit::remove_note(self.content, &path),
        };
        if let Err(refused) = changed {
            *self.content = before;
            return Err(refused);
        }
        self.order = OnceCell::new();
        self.editing.touch();
        self.record(before, None);
        Ok(())
    }

    /// Accept or reject a tracked change, or every one when `id` is none, as
    /// one step to undo. The caret is let go, since the paragraphs it was
    /// counted among may have moved.
    ///
    /// # Errors
    ///
    /// There is no such change, or a deletion cannot be put back.
    pub fn decide(
        &mut self,
        editor: &mut PageEditor,
        id: Option<&str>,
        accept: bool,
    ) -> Result<(), Refused> {
        let before = self.content.clone();
        let changed = match (id, accept) {
            (Some(id), true) => edit::accept(self.content, id),
            (Some(id), false) => edit::reject(self.content, id),
            (None, true) => edit::accept_all(self.content),
            (None, false) => edit::reject_all(self.content),
        };
        if let Err(refused) = changed {
            *self.content = before;
            return Err(refused);
        }
        editor.clear();
        self.order = OnceCell::new();
        self.editing.touch();
        self.record(before, None);
        Ok(())
    }

    /// Do what was done in a comment's box: its new text, signed and dated
    /// now, or its removal, as one step to undo.
    ///
    /// # Errors
    ///
    /// There is no such comment.
    pub fn note(
        &mut self,
        paragraph: &[usize],
        index: usize,
        done: &crate::note_popup::NoteResponse,
    ) -> Result<(), Refused> {
        let before = self.content.clone();
        let path = [self.root.as_slice(), paragraph].concat();
        let changed = match done {
            crate::note_popup::NoteResponse::Saved(text) => edit::set_annotation_text(
                self.content,
                &path,
                index,
                text,
                crate::comments::author().as_deref(),
                &crate::comments::now(),
            ),
            crate::note_popup::NoteResponse::Deleted => {
                edit::remove_annotation(self.content, &path, index)
            }
        };
        if let Err(refused) = changed {
            *self.content = before;
            return Err(refused);
        }
        self.editing.touch();
        self.record(before, None);
        Ok(())
    }

    /// Tab and Shift+Tab in a table: select what the next cell or the one
    /// before holds, and past the last cell put in a row and go to its first.
    /// Answers whether the caret was in a table, and so whether the key was
    /// this one's.
    ///
    /// # Errors
    ///
    /// Past the last cell, a row could not be put in, as a table with merged
    /// or repeated cells refuses one; the key was still this one's.
    pub fn step_cell(&mut self, editor: &mut PageEditor, forward: bool) -> Result<bool, Refused> {
        let Some(selection) = editor.selection().cloned() else {
            return Ok(false);
        };
        let path = [self.root.as_slice(), &selection.focus.paragraph].concat();
        if edit::table_at(self.content, &path).is_none() {
            return Ok(false);
        }
        if let Some(next) = edit::step_cell(self.content, &path, forward) {
            if let Some(paragraph) = next
                .strip_prefix(self.root.as_slice())
                .map(<[usize]>::to_vec)
            {
                let length = self.text(&paragraph).map_or(0, |t| t.chars().count());
                editor.select(Selection {
                    anchor: Position::new(paragraph.clone(), 0),
                    focus: Position::new(paragraph, length),
                });
            }
            return Ok(true);
        }
        if forward {
            self.structural(editor, |content, paragraph, styles| {
                let mut caret = edit::edit_table(content, paragraph, TableEdit::RowBelow, styles)?;
                while let Some(previous) = edit::table_at(content, &caret)
                    .filter(|place| place.column > 0)
                    .and_then(|_| edit::step_cell(content, &caret, false))
                {
                    caret = previous;
                }
                Ok(caret)
            })?;
        }
        Ok(true)
    }

    /// An edit to the tree around the caret's paragraph that answers where the
    /// caret goes: tried on the tree, put back if refused, and otherwise
    /// recorded as one step to undo with the caret where it said.
    fn structural(
        &mut self,
        editor: &mut PageEditor,
        change: impl FnOnce(&mut Element, &[usize], &mut Styles) -> Result<Vec<usize>, Refused>,
    ) -> Result<(), Refused> {
        self.structural_step(editor, Kept::default(), change)
    }

    /// The same, for an edit that also writes package entries, kept as they
    /// stood before it in `parts`.
    fn structural_step(
        &mut self,
        editor: &mut PageEditor,
        parts: Kept,
        change: impl FnOnce(&mut Element, &[usize], &mut Styles) -> Result<Vec<usize>, Refused>,
    ) -> Result<(), Refused> {
        let selection = editor.selection().cloned().ok_or(Refused::NotFound)?;
        let before = self.content.clone();
        let children = self.content.children.len();
        let root = self.root.clone();
        let path = [root.as_slice(), &selection.focus.paragraph].concat();
        let caret = match change(self.content, &path, self.styles) {
            Ok(caret) => caret,
            Err(refused) => {
                *self.content = before;
                return Err(refused);
            }
        };
        if *self.content == before {
            return Ok(());
        }
        let paragraph = caret
            .strip_prefix(root.as_slice())
            .map(<[usize]>::to_vec)
            .ok_or(Refused::NotFound)?;
        self.follow_styles(children);
        self.order = OnceCell::new();
        let begins = Position::new(
            [self.tag.as_slice(), &selection.focus.paragraph].concat(),
            selection.focus.offset,
        );
        self.editing.touch();
        self.record_step(before, parts, Some(begins));
        editor.select(Selection::caret(Position::new(paragraph, 0)));
        Ok(())
    }

    /// Put a picture in beside the paragraph the caret is in, kept in the
    /// package, as one step to undo that takes the package's entries back too,
    /// with the caret in the paragraph after it.
    ///
    /// # Errors
    ///
    /// What the edit refused, with the document and the package left as they
    /// were.
    pub fn insert_picture(
        &mut self,
        editor: &mut PageEditor,
        package: &mut Package,
        picture: &Picture,
    ) -> Result<(), Refused> {
        const MANIFEST: &str = "META-INF/manifest.xml";
        let href = package.picture_name(&picture.bytes, picture.extension);
        let parts = package.keep(&[href.as_str(), MANIFEST]);
        self.structural_step(editor, parts, |content, paragraph, styles| {
            let new = edit::NewPicture {
                href: &href,
                media_type: picture.media_type,
                size: picture.size,
                description: picture.description.as_deref(),
            };
            edit::insert_picture(content, paragraph, &new, styles)
        })?;
        package.add_picture(&picture.bytes, picture.extension, picture.media_type);
        Ok(())
    }

    /// What a paragraph is, as HTML says it.
    fn kind_of(&self, paragraph: &[usize]) -> html::Kind {
        let path = [self.root.as_slice(), paragraph].concat();
        match edit::block_state(self.content, &path, self.styles) {
            (Some(level), _) => html::Kind::Heading(level),
            (None, Some(list)) => html::Kind::List(list),
            (None, None) => html::Kind::Body,
        }
    }

    /// Put paragraphs read from HTML in at a position, as one edit, and answer
    /// where the caret goes. They go in as a copy made here does, the ones
    /// between the first and the last in the style of the paragraph pasted
    /// into, and each that stands as a paragraph of its own, as
    /// [`edit::paste_fragment`] says which do, is then made the heading or the
    /// list item the HTML said it was.
    fn paste_read(
        &mut self,
        at: &Position<Vec<usize>>,
        read: &[html::Paragraph],
        new_step: bool,
    ) -> Option<Position<Vec<usize>>> {
        let new_step = new_step || !self.editing.modified();
        let (original, root) = (self.content.clone(), self.root.clone());
        let style = self
            .root()?
            .at(&at.paragraph)
            .filter(|paragraph| paragraph.is(&Ns::Text, "p"))
            .and_then(|paragraph| paragraph.attr(&Ns::Text, "style-name"))
            .map(ToOwned::to_owned);
        // Which of them stand alone rather than merging into the paragraph
        // pasted into.
        let length = self.text(&at.paragraph).map_or(0, |t| t.chars().count());
        let in_list = matches!(self.kind_of(&at.paragraph), html::Kind::List(_));
        let start = usize::from(in_list || at.offset > 0);
        let end = if !in_list && at.offset >= length {
            read.len()
        } else {
            read.len().saturating_sub(1)
        };
        let alone = start..end.max(start);
        let pasted = self.put_read(at, read, style);
        let Some((path, offset)) = pasted else {
            *self.content = original;
            self.root = root;
            self.order = OnceCell::new();
            return None;
        };
        self.order = OnceCell::new();
        let first = self.order().iter().position(|p| *p == at.paragraph)?;
        for index in alone.clone() {
            if let html::Kind::Heading(level) = read[index].kind {
                let paragraph = self.order().get(first + index)?.clone();
                self.set_headings(&[paragraph], Some(level));
            }
        }
        // A run of items of one kind is one list. Wrapping paragraphs in a
        // list moves them in the tree and leaves their order, so each run is
        // found again by its place in that order.
        let mut index = alone.start;
        while index < alone.end {
            let html::Kind::List(kind) = read[index].kind else {
                index += 1;
                continue;
            };
            let end = (index..alone.end)
                .take_while(|&i| read[i].kind == html::Kind::List(kind))
                .last()
                .unwrap_or(index);
            self.order = OnceCell::new();
            let run = self.order().get(first + index..=first + end)?.to_vec();
            self.set_lists(&run, Some(kind));
            index = end + 1;
        }
        self.order = OnceCell::new();
        let path = if read.len() > 1 {
            self.order().get(first + read.len() - 1)?.clone()
        } else {
            path
        };
        self.editing.touch();
        if new_step {
            let begins = Position::new([self.tag.as_slice(), &at.paragraph].concat(), at.offset);
            self.record(original, Some(begins));
        }
        Some(Position::new(path, offset))
    }

    fn put_read(
        &mut self,
        at: &Position<Vec<usize>>,
        read: &[html::Paragraph],
        style: Option<String>,
    ) -> Option<(Vec<usize>, usize)> {
        let before = self.content.children.len();
        let mut items = html::paragraphs(self.content, read, self.styles).ok()?;
        self.follow_styles(before);
        if let Some(style) = style {
            let name = self.content.name_for(&Ns::Text, "style-name");
            for item in &mut items {
                item.set_attr(name.clone(), style.clone());
            }
        }
        let root = self.content.at_mut(&self.root)?;
        edit::paste_fragment(root, &at.paragraph, at.offset, &items).ok()
    }

    fn set_headings(&mut self, paragraphs: &[Vec<usize>], level: Option<u8>) -> Option<()> {
        for paragraph in paragraphs {
            let before = self.content.children.len();
            let path = [self.root.as_slice(), paragraph].concat();
            edit::set_heading(self.content, &path, level, self.styles).ok()?;
            self.follow_styles(before);
        }
        Some(())
    }

    fn set_lists(&mut self, paragraphs: &[Vec<usize>], kind: Option<ListKind>) -> Option<()> {
        let before = self.content.children.len();
        let paths: Vec<Vec<usize>> = paragraphs
            .iter()
            .map(|paragraph| [self.root.as_slice(), paragraph].concat())
            .collect();
        edit::set_list(self.content, &paths, kind, self.styles).ok()?;
        self.follow_styles(before);
        Some(())
    }

    /// Containers written into a document that had none go before its body,
    /// and move the body, and the root, along by as many as were written.
    fn follow_styles(&mut self, before: usize) {
        let written = self.content.children.len().saturating_sub(before);
        if let Some(first) = self.root.first_mut() {
            *first += written;
        }
    }

    /// Split a paragraph in two. What follows a heading's last character is
    /// body text and not another heading, as it is in every word processor.
    fn split(&mut self, at: &Position<Vec<usize>>) -> Option<Vec<usize>> {
        let at_end_of_heading = self
            .root()
            .and_then(|root| root.at(&at.paragraph))
            .is_some_and(|paragraph| {
                edit::heading_level(paragraph).is_some()
                    && at.offset >= edit::text(paragraph).chars().count()
            });
        let root = self.content.at_mut(&self.root)?;
        let second = edit::split_at(root, &at.paragraph, at.offset).ok()?;
        let path = [self.root.as_slice(), &second].concat();
        if at_end_of_heading {
            // A document that cannot be given the paragraph style keeps the
            // heading, which is a split all the same.
            let _ = edit::set_heading(self.content, &path, None, self.styles);
        }
        if edit::page_break(self.content, &path, self.styles) {
            let before = self.content.children.len();
            let _ = edit::remove_page_break(self.content, &path, self.styles);
            self.follow_styles(before);
        }
        Some(second)
    }

    /// Give the text from one position to another a setting, paragraph by
    /// paragraph.
    fn restyle(
        &mut self,
        from: &Position<Vec<usize>>,
        to: &Position<Vec<usize>>,
        setting: &edit::Setting,
    ) -> Option<()> {
        for (path, range) in self.covered(from, to)? {
            if range.is_empty() {
                continue;
            }
            let before = self.content.children.len();
            let mut at = self.root.clone();
            at.extend(&path);
            edit::set_text(self.content, &at, range, setting, self.styles).ok()?;
            self.follow_styles(before);
        }
        Some(())
    }
}

/// What the editor asks a value for, as the library sets it.
fn core_setting(value: Value) -> edit::Setting {
    match value {
        Value::Family(family) => edit::Setting::Family(family),
        #[allow(clippy::cast_precision_loss)]
        Value::Size(twentieths) => edit::Setting::Size(twentieths as f32 / 20.0),
        Value::Color(color) => edit::Setting::Color(color.map(core_color)),
        Value::Highlight(color) => edit::Setting::Highlight(color.map(core_color)),
        Value::Script(script) => edit::Setting::Script(match script {
            Script::Normal => edit::Script::Normal,
            Script::Super => edit::Script::Super,
            Script::Sub => edit::Script::Sub,
        }),
    }
}

fn core_color([r, g, b]: [u8; 3]) -> odox_core::Color {
    odox_core::Color { r, g, b }
}

fn editor_color(color: odox_core::Color) -> [u8; 3] {
    [color.r, color.g, color.b]
}

/// A value the library read, as the editor shows it.
fn editor_value(setting: edit::Setting) -> Option<Value> {
    match setting {
        edit::Setting::Family(family) => Some(Value::Family(family)),
        edit::Setting::Size(points) => Some(Value::Size(twentieths(points))),
        edit::Setting::Color(color) => Some(Value::Color(color.map(editor_color))),
        edit::Setting::Highlight(color) => Some(Value::Highlight(color.map(editor_color))),
        edit::Setting::Script(script) => Some(Value::Script(match script {
            edit::Script::Normal => Script::Normal,
            edit::Script::Super => Script::Super,
            edit::Script::Sub => Script::Sub,
        })),
        edit::Setting::Mark(..) => None,
    }
}

/// A size in points, in twentieths of a point.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn twentieths(points: f32) -> u32 {
    (points.max(0.0) * 20.0).round() as u32
}

/// The library's name for a mark.
fn core_mark(mark: Mark) -> edit::Mark {
    match mark {
        Mark::Bold => edit::Mark::Bold,
        Mark::Italic => edit::Mark::Italic,
        Mark::Underline => edit::Mark::Underline,
        Mark::Strike => edit::Mark::Strike,
    }
}

impl Model for FlowModel<'_> {
    type Paragraph = Vec<usize>;

    fn text(&self, paragraph: &Vec<usize>) -> Option<String> {
        self.root()?
            .at(paragraph)
            .filter(|element| is_paragraph(element))
            .map(edit::text)
    }

    fn next(&self, paragraph: &Vec<usize>) -> Option<Vec<usize>> {
        self.neighbour(paragraph, 1)
    }

    fn previous(&self, paragraph: &Vec<usize>) -> Option<Vec<usize>> {
        self.neighbour(paragraph, -1)
    }

    fn first(&self) -> Option<Vec<usize>> {
        self.order().first().cloned()
    }

    fn last(&self) -> Option<Vec<usize>> {
        self.order().last().cloned()
    }

    fn marked(
        &self,
        from: &Position<Vec<usize>>,
        to: &Position<Vec<usize>>,
        mark: Mark,
    ) -> Option<bool> {
        let root = self.root()?;
        let mark = core_mark(mark);
        let ranges: Vec<_> = self
            .covered(from, to)?
            .into_iter()
            .filter(|(_, range)| !range.is_empty())
            .collect();
        if ranges.is_empty() {
            let at = from.offset..from.offset;
            return edit::marked(root.at(&from.paragraph)?, at, mark, self.styles);
        }
        let mut answer = None;
        for (path, range) in ranges {
            let value = edit::marked(root.at(&path)?, range, mark, self.styles)?;
            if answer.is_some_and(|a| a != value) {
                return None;
            }
            answer = Some(value);
        }
        answer
    }

    fn value(
        &self,
        from: &Position<Vec<usize>>,
        to: &Position<Vec<usize>>,
        property: Property,
    ) -> Option<Value> {
        let root = self.root()?;
        let property = match property {
            Property::Family => edit::TextProperty::Family,
            Property::Size => edit::TextProperty::Size,
            Property::Color => edit::TextProperty::Color,
            Property::Highlight => edit::TextProperty::Highlight,
            Property::Script => edit::TextProperty::Script,
        };
        let ranges: Vec<_> = self
            .covered(from, to)?
            .into_iter()
            .filter(|(_, range)| !range.is_empty())
            .collect();
        let ranges = if ranges.is_empty() {
            vec![(from.paragraph.clone(), from.offset..from.offset)]
        } else {
            ranges
        };
        let mut answer = None;
        for (path, range) in ranges {
            let value = editor_value(edit::text_value(
                root.at(&path)?,
                range,
                property,
                self.styles,
            )?)?;
            if answer.as_ref().is_some_and(|a| *a != value) {
                return None;
            }
            answer = Some(value);
        }
        answer
    }

    fn fragment(&self, from: &Position<Vec<usize>>, to: &Position<Vec<usize>>) -> Option<Fragment> {
        let root = self.root()?;
        let mut items = Vec::new();
        let mut kinds = Vec::new();
        for (path, range) in self.covered(from, to)? {
            let Some(paragraph) = root.at(&path) else {
                continue;
            };
            items.push(edit::slice(paragraph, range));
            kinds.push(self.kind_of(&path));
        }
        if items.is_empty() {
            return None;
        }
        let paired: Vec<(&Element, html::Kind)> = items.iter().zip(kinds).collect();
        let html = html::write(&paired, self.styles);
        Some(Fragment::new(Copied { items, html }))
    }

    fn html(&self, fragment: &Fragment) -> Option<String> {
        fragment.get::<Copied>().map(|copied| copied.html.clone())
    }

    fn fragment_from_html(&self, text: &str) -> Option<Fragment> {
        let read = html::read(text);
        (!read.is_empty()).then(|| Fragment::new(Read(read)))
    }

    fn paste_fragment(
        &mut self,
        at: &Position<Vec<usize>>,
        fragment: &Fragment,
        new_step: bool,
    ) -> Option<Position<Vec<usize>>> {
        let Some(Copied { items, .. }) = fragment.get::<Copied>() else {
            let Read(read) = fragment.get::<Read>()?;
            return self.paste_read(at, read, new_step);
        };
        let new_step = new_step || !self.editing.modified();
        let before = new_step.then(|| self.content.clone());
        let root = self.content.at_mut(&self.root)?;
        let (path, offset) = edit::paste_fragment(root, &at.paragraph, at.offset, items).ok()?;
        self.editing.touch();
        if let Some(before) = before {
            let begins = Position::new([self.tag.as_slice(), &at.paragraph].concat(), at.offset);
            self.record(before, Some(begins));
        }
        self.order = OnceCell::new();
        Some(Position::new(path, offset))
    }

    fn apply(
        &mut self,
        edit: Edit<'_, Vec<usize>>,
        new_step: bool,
    ) -> Option<Position<Vec<usize>>> {
        // An edit to a document that matches its file always begins a step,
        // whatever the editor thinks: typing that runs on past a save is a
        // change the save does not hold, and has to be one a close asks about.
        let new_step = new_step || !self.editing.modified();
        // Taken before trying, because only trying says whether the edit is
        // made. A refusal is decided before anything in the tree changes.
        // Where the edit begins is where an undo of it puts the caret.
        let before = new_step.then(|| self.content.clone());
        let begins = match &edit {
            Edit::Replace { from, .. } | Edit::Format { from, .. } | Edit::Set { from, .. } => from,
            Edit::Split { at } => at,
        };
        let begins = Position::new(
            [self.tag.as_slice(), &begins.paragraph].concat(),
            begins.offset,
        );
        let at = match edit {
            Edit::Replace { from, to, text } => self
                .replace(&from, &to, text)
                .map(|()| Position::new(from.paragraph, from.offset + text.chars().count())),
            Edit::Split { at } => self.split(&at).map(|second| Position::new(second, 0)),
            Edit::Format { from, to, mark, on } => self
                .restyle(&from, &to, &edit::Setting::Mark(core_mark(mark), on))
                .map(|()| to),
            Edit::Set { from, to, value } => {
                self.restyle(&from, &to, &core_setting(value)).map(|()| to)
            }
        };
        if at.is_some() {
            self.editing.touch();
            if let Some(before) = before {
                self.record(before, Some(begins));
            }
            self.order = OnceCell::new();
        }
        at
    }
}

/// Every paragraph under a root, in the order [`crate::Flow`] draws them, by
/// the paths it reports them under.
pub(crate) fn paragraph_paths(root: &Element) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    blocks(root, &mut Vec::new(), &mut out);
    out
}

/// The paragraphs among a run of blocks, in the order [`crate::Flow`] draws
/// them, and under the same paths.
fn blocks(parent: &Element, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
    for (index, element) in parent.elements_indexed() {
        path.push(index);
        block(element, path, out);
        path.pop();
    }
}

fn block(element: &Element, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
    if is_paragraph(element) {
        out.push(path.clone());
    } else if element.is(&Ns::Text, "list") {
        for (item_index, item) in element.elements_indexed() {
            if item.is(&Ns::Text, "list-item") || item.is(&Ns::Text, "list-header") {
                path.push(item_index);
                blocks(item, path, out);
                path.pop();
            }
        }
    } else if element.is(&Ns::Table, "table")
        || element.is(&Ns::Table, "table-header-rows")
        || element.is(&Ns::Table, "table-rows")
        || element.is(&Ns::Table, "table-row-group")
    {
        blocks(element, path, out);
    } else if element.is(&Ns::Table, "table-row") {
        for (cell_index, cell) in element.elements_indexed() {
            if cell.is(&Ns::Table, "table-cell") {
                path.push(cell_index);
                blocks(cell, path, out);
                path.pop();
            }
        }
    } else if element.is(&Ns::Draw, "frame") {
        if let Some((box_index, text_box)) = element
            .elements_indexed()
            .find(|(_, e)| e.is(&Ns::Draw, "text-box"))
        {
            path.push(box_index);
            blocks(text_box, path, out);
            path.pop();
        }
    } else if element.name.ns == Ns::Draw {
        // A drawing shape keeps its label as paragraphs of its own, where a
        // frame keeps them in a text box.
        blocks(element, path, out);
    } else if is_block_container(element) {
        blocks(element, path, out);
    }
}

/// Where a link question starts: a paragraph, the characters of the link or
/// the selection there, the link's path from the paragraph where there is
/// one, and its text and address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkHere {
    /// The paragraph, as the page editor names it.
    pub paragraph: Vec<usize>,
    /// The characters the link covers, or the selection.
    pub range: std::ops::Range<usize>,
    /// The link's path from the paragraph, where the caret is in one.
    pub link: Option<Vec<usize>>,
    /// Its text.
    pub text: String,
    /// Its address, empty for a new one.
    pub href: String,
}

#[cfg(test)]
mod tests {
    use super::{FlowModel, html};
    use crate::Editing;
    use egui_richedit::{Edit, Model, Position};
    use odox_core::{Document, Element, Node, Ns, Styles};

    fn element(prefix: &str, local: &str, ns: Ns, children: Vec<Node>) -> Node {
        let mut e = Element::new(prefix, local, ns);
        e.children = children;
        Node::Element(e)
    }

    /// A document holding this content, for an undo to go back over.
    fn holding(content: &Element) -> Document {
        let mut document = Document::read(
            &odox_core::blank::text("en"),
            &[odox_core::media_type::TEXT],
        )
        .expect("a blank document");
        document.content = content.clone();
        document
    }

    fn p(text: &str) -> Node {
        element("text", "p", Ns::Text, vec![Node::Text(text.to_owned())])
    }

    /// `office:text` holding a paragraph, a list of one item, a table of one
    /// cell, and a paragraph, under a content root.
    fn content() -> Element {
        let list = element(
            "text",
            "list",
            Ns::Text,
            vec![element("text", "list-item", Ns::Text, vec![p("item")])],
        );
        let table = element(
            "table",
            "table",
            Ns::Table,
            vec![element(
                "table",
                "table-row",
                Ns::Table,
                vec![element("table", "table-cell", Ns::Table, vec![p("cell")])],
            )],
        );
        let body = element(
            "office",
            "text",
            Ns::Office,
            vec![p("one"), list, table, p("two")],
        );
        let mut root = Element::new("office", "document-content", Ns::Office);
        root.children = vec![element("office", "body", Ns::Office, vec![body])];
        root
    }

    #[test]
    fn paragraphs_follow_one_another_through_lists_and_tables() {
        let mut content = content();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        let model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        let mut at = vec![0];
        let mut seen = vec![model.text(&at).expect("the first")];
        while let Some(next) = model.next(&at) {
            seen.push(model.text(&next).expect("a paragraph"));
            assert_eq!(model.previous(&next), Some(at));
            at = next;
        }
        assert_eq!(seen, ["one", "item", "cell", "two"]);
    }

    #[test]
    fn a_selection_over_a_whole_table_takes_it_and_joins_the_ends() {
        let mut content = content();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        // From inside "item" to inside "two", over the table's one cell.
        let at = model.apply(
            Edit::Replace {
                from: Position::new(vec![1, 0, 0], 2),
                to: Position::new(vec![3], 1),
                text: "X",
            },
            true,
        );
        assert_eq!(at, Some(Position::new(vec![1, 0, 0], 3)));
        assert_eq!(model.text(&vec![1, 0, 0]).as_deref(), Some("itXwo"));
        assert_eq!(
            model.next(&vec![1, 0, 0]),
            None,
            "the table and \"two\" are gone"
        );
    }

    #[test]
    fn a_selection_out_of_a_cell_takes_the_text_and_leaves_the_cell() {
        let mut content = content();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        // From inside "cell" to inside "two".
        let at = model.apply(
            Edit::Replace {
                from: Position::new(vec![2, 0, 0, 0], 2),
                to: Position::new(vec![3], 1),
                text: "",
            },
            true,
        );
        assert_eq!(at, Some(Position::new(vec![2, 0, 0, 0], 2)));
        assert_eq!(model.text(&vec![2, 0, 0, 0]).as_deref(), Some("ce"));
        assert_eq!(model.text(&vec![3]).as_deref(), Some("wo"));
    }

    #[test]
    fn backspace_at_a_list_item_joins_it_to_the_paragraph_before() {
        let mut content = content();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        let at = model.apply(
            Edit::Replace {
                from: Position::new(vec![0], 3),
                to: Position::new(vec![1, 0, 0], 0),
                text: "",
            },
            true,
        );
        assert_eq!(at, Some(Position::new(vec![0], 3)));
        assert_eq!(model.text(&vec![0]).as_deref(), Some("oneitem"));
        // The list had one item, and went with it.
        assert_eq!(model.text(&vec![1, 0, 0, 0]).as_deref(), Some("cell"));
    }

    #[test]
    fn a_tagged_model_remembers_the_caret_under_its_tag() {
        let mut content = content();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut model =
            FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing).tagged(vec![7]);
        model.apply(
            Edit::Split {
                at: Position::new(vec![0], 1),
            },
            true,
        );
        let caret = editing
            .undo(&mut holding(&content), None)
            .expect("a step to undo");
        assert_eq!(caret, Some(Position::new(vec![7, 0], 1)));
    }

    fn heading_document() -> Element {
        odox_core::xml::parse(
            br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"><office:body><office:text><text:h text:outline-level="1" text:style-name="H">Title</text:h><text:p text:style-name="Body">a</text:p><text:p text:style-name="Body">b</text:p></office:text></office:body></office:document-content>"#,
            "test",
        )
        .expect("a document")
    }

    #[test]
    fn only_headings_move_on_the_outline() {
        let mut content = heading_document();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        let model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        let caret =
            |paragraph: usize| egui_richedit::Selection::caret(Position::new(vec![paragraph], 0));
        assert!(model.all_headings(&caret(0)));
        assert!(!model.all_headings(&caret(1)));
        let over_both = egui_richedit::Selection {
            anchor: Position::new(vec![0], 1),
            focus: Position::new(vec![1], 0),
        };
        assert!(!model.all_headings(&over_both));
    }

    #[test]
    fn enter_at_the_end_of_a_heading_starts_body_text() {
        let mut content = heading_document();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        let at = model.apply(
            Edit::Split {
                at: Position::new(vec![0], 5),
            },
            true,
        );
        assert_eq!(at, Some(Position::new(vec![1], 0)));
        let second = content.at(&[0, 0, 1]).expect("the new paragraph");
        assert!(second.is(&Ns::Text, "p"));
        assert_eq!(second.attr(&Ns::Text, "style-name"), Some("Body"));
        assert!(
            content
                .at(&[0, 0, 0])
                .expect("the heading")
                .is(&Ns::Text, "h")
        );
    }

    #[test]
    fn replacing_every_match_is_one_step_and_the_offsets_hold() {
        let mut content = heading_document();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let matches = crate::find::in_paragraphs(
            content.at(&[0, 0]).expect("the body"),
            0,
            "t",
            crate::find::Options::default(),
        );
        assert_eq!(matches.len(), 2, "the two t of Title");
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        let done = model.replace_matches(&matches.iter().collect::<Vec<_>>(), "TT", true);
        assert_eq!(done.replaced, 2);
        assert_eq!(model.text(&vec![0]).as_deref(), Some("TTiTTle"));
        let mut undone = 0;
        let mut current = holding(&content);
        while editing.undo(&mut current, None).is_some() {
            undone += 1;
        }
        assert_eq!(undone, 1);
    }

    #[test]
    fn a_copy_keeps_spans_and_a_paste_puts_them_back_in_one_step() {
        let mut content = odox_core::xml::parse(
            br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:text><text:p>one <text:span text:style-name="T1">two</text:span> three</text:p><text:p>x</text:p></office:text></office:body></office:document-content>"#,
            "test",
        )
        .expect("a document");
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        let fragment = model
            .fragment(&Position::new(vec![0], 3), &Position::new(vec![0], 7))
            .expect("a copy");
        let after = model
            .paste_fragment(&Position::new(vec![1], 1), &fragment, true)
            .expect("it goes in");
        assert_eq!(after, Position::new(vec![1], 5));
        let written = String::from_utf8(odox_core::xml::serialize(&content)).expect("UTF-8");
        assert!(
            written.contains(
                r#"<text:p>x<text:s/><text:span text:style-name="T1">two</text:span></text:p>"#
            ),
            "{written}"
        );
        assert!(editing.undo(&mut holding(&content), None).is_some());
        assert!(!editing.can_undo(), "one step");
    }

    #[test]
    fn enter_in_the_middle_of_a_heading_leaves_two_headings() {
        let mut content = heading_document();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        model.apply(
            Edit::Split {
                at: Position::new(vec![0], 2),
            },
            true,
        );
        assert!(
            content
                .at(&[0, 0, 1])
                .expect("the second half")
                .is(&Ns::Text, "h")
        );
    }

    #[test]
    fn typing_that_runs_on_past_a_save_is_a_change() {
        let mut content = content();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let at = |offset| Position::new(vec![0], offset);
        let typed = |offset| Edit::Replace {
            from: at(offset),
            to: at(offset),
            text: "x",
        };
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        model.apply(typed(0), true);
        model.editing.mark_saved();
        // The same run of typing, as far as the editor knows.
        model.apply(typed(1), false);
        assert!(
            editing.modified(),
            "what was typed after the save is unsaved"
        );
    }

    #[test]
    fn an_edit_that_begins_a_step_is_recorded_once_it_succeeds() {
        let mut content = content();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        let at = model.apply(
            Edit::Split {
                at: Position::new(vec![0], 1),
            },
            true,
        );
        assert_eq!(at, Some(Position::new(vec![1], 0)));
        assert_eq!(model.text(&vec![0]).as_deref(), Some("o"));
        assert_eq!(model.text(&vec![1]).as_deref(), Some("ne"));

        // Backspace at the start of a table's cell: nothing to join and
        // nothing to remove, which is refused and is not a step.
        let refused = model.apply(
            Edit::Replace {
                from: Position::new(vec![2, 0, 0], 4),
                to: Position::new(vec![3, 0, 0, 0], 0),
                text: "",
            },
            true,
        );
        assert_eq!(refused, None);
        assert!(editing.can_undo());
        let mut undone = 0;
        let mut current = holding(&content);
        while editing.undo(&mut current, None).is_some() {
            undone += 1;
        }
        assert_eq!(undone, 1);
    }

    fn blank_text() -> odox_core::doc::TextDocument {
        odox_core::doc::TextDocument::read(&odox_core::blank::text("en")).expect("a blank document")
    }

    #[test]
    fn html_from_elsewhere_pastes_its_kinds_and_marks_as_one_step() {
        let mut document = blank_text();
        let root = document.body_path().expect("a body");
        let mut editing = Editing::default();
        editing.reset();
        let odox_core::Document {
            content, styles, ..
        } = &mut document.document;
        let mut model = FlowModel::new(content, styles, root, &mut editing);
        let first = model.first().expect("a paragraph");
        model.apply(
            Edit::Replace {
                from: Position::new(first.clone(), 0),
                to: Position::new(first.clone(), 0),
                text: "ab",
            },
            true,
        );
        let fragment = model
            .fragment_from_html(
                "<p>x<b>bold</b></p><h1>Head</h1><ul><li>one</li><li>two</li></ul><p>tail</p>",
            )
            .expect("read");
        let at = model
            .paste_fragment(&Position::new(first.clone(), 1), &fragment, true)
            .expect("pasted");
        let mut texts = Vec::new();
        let mut paragraph = model.first();
        while let Some(path) = paragraph {
            texts.push(model.text(&path).unwrap_or_default());
            paragraph = model.next(&path);
        }
        assert_eq!(texts, ["axbold", "Head", "one", "two", "tailb"]);
        assert_eq!(at.offset, 4, "after \"tail\"");
        assert_eq!(
            model.marked(
                &Position::new(first.clone(), 2),
                &Position::new(first.clone(), 6),
                egui_richedit::Mark::Bold
            ),
            Some(true)
        );
        let kind = |model: &FlowModel<'_>, n: usize| {
            let mut path = model.first().expect("first");
            for _ in 0..n {
                path = model.next(&path).expect("next");
            }
            model.kind_of(&path)
        };
        assert_eq!(kind(&model, 1), html::Kind::Heading(1));
        assert_eq!(
            kind(&model, 2),
            html::Kind::List(odox_core::ListKind::Bullet)
        );
        assert_eq!(
            kind(&model, 3),
            html::Kind::List(odox_core::ListKind::Bullet)
        );
        assert_eq!(kind(&model, 4), html::Kind::Body);

        let last = {
            let mut path = model.first().expect("first");
            while let Some(next) = model.next(&path) {
                path = next;
            }
            path
        };
        let copied = model
            .fragment(
                &Position::new(model.first().expect("first"), 0),
                &Position::new(last, 5),
            )
            .expect("a copy");
        let written = model.html(&copied).expect("html");
        assert!(
            written.contains("<p>ax<b>bold</b></p><h1>Head</h1>"),
            "{written}"
        );
        assert!(
            written.contains("<ul><li>one</li><li>two</li></ul><p>tailb</p>"),
            "{written}"
        );

        let mut steps = 0;
        let mut current = holding(&document.document.content);
        while editing.undo(&mut current, None).is_some() {
            steps += 1;
        }
        assert_eq!(steps, 2, "the typing and the paste");
    }

    #[test]
    fn shifting_headings_moves_each_selected_heading_and_undo_restores_tree() {
        let mut content = odox_core::xml::parse(
            br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"><office:automatic-styles/><office:body><office:text><text:h text:outline-level="1">One</text:h><text:h text:outline-level="2">Two</text:h><text:p>Body</text:p></office:text></office:body></office:document-content>"#,
            "test",
        )
        .expect("a document");
        let original = content.clone();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut editor = super::page_editor();
        editor.select(egui_richedit::Selection {
            anchor: Position::new(vec![0], 0),
            focus: Position::new(vec![2], 2),
        });
        let mut model = FlowModel::new(&mut content, &mut styles, vec![1, 0], &mut editing);
        assert!(model.shift_headings(&mut editor, 1));
        let level = |content: &Element, index: usize| {
            content
                .at(&[1, 0, index])
                .and_then(odox_core::edit::heading_level)
        };
        assert_eq!(
            (level(&content, 0), level(&content, 1), level(&content, 2)),
            (Some(2), Some(3), None)
        );
        let mut current = holding(&content);
        let mut undone = 0;
        while editing.undo(&mut current, None).is_some() {
            undone += 1;
        }
        assert_eq!(undone, 1);
        assert_eq!(current.content, original);
    }

    #[test]
    fn tab_at_an_items_start_nests_it_and_undo_restores_tree() {
        let mut content = odox_core::xml::parse(
            br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:text><text:list><text:list-item><text:p>One</text:p></text:list-item><text:list-item><text:p>Two</text:p></text:list-item></text:list></office:text></office:body></office:document-content>"#,
            "test",
        )
        .expect("a document");
        let original = content.clone();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut editor = super::page_editor();
        editor.select(egui_richedit::Selection::caret(Position::new(
            vec![0, 1, 0],
            1,
        )));
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        assert_eq!(
            model.list_level(&mut editor, 1),
            Ok(false),
            "not at the item's start"
        );
        editor.select(egui_richedit::Selection::caret(Position::new(
            vec![0, 1, 0],
            0,
        )));
        assert_eq!(model.list_level(&mut editor, 1), Ok(true));
        let nested = content
            .at(&[0, 0, 0, 0])
            .and_then(|one| one.child(&Ns::Text, "list"))
            .is_some();
        assert!(nested, "Two went into One");
        let mut current = holding(&content);
        let mut undone = 0;
        while editing.undo(&mut current, None).is_some() {
            undone += 1;
        }
        assert_eq!(undone, 1);
        assert_eq!(current.content, original);
    }

    #[test]
    fn tab_in_a_table_cell_is_the_tables_even_at_a_list_items_start() {
        let mut content = odox_core::xml::parse(
            br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"><office:body><office:text><table:table><table:table-column/><table:table-row><table:table-cell><text:list><text:list-item><text:p>One</text:p></text:list-item><text:list-item><text:p>Two</text:p></text:list-item></text:list></table:table-cell></table:table-row></table:table></office:text></office:body></office:document-content>"#,
            "test",
        )
        .expect("a document");
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut editor = super::page_editor();
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        let two = model.order()[1].clone();
        editor.select(egui_richedit::Selection::caret(Position::new(two, 0)));
        assert_eq!(model.list_level(&mut editor, 1), Ok(false));
    }

    fn broken() -> Element {
        odox_core::xml::parse(
            br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"><office:automatic-styles><style:style style:name="P1" style:family="paragraph"><style:paragraph-properties fo:break-before="page"/></style:style></office:automatic-styles><office:body><office:text><text:p>One</text:p><text:p text:style-name="P1">Two</text:p></office:text></office:body></office:document-content>"#,
            "test",
        )
        .expect("a document")
    }

    #[test]
    fn backspace_at_start_of_broken_paragraph_removes_break_first() {
        for (caret, backspace) in [
            (Position::new(vec![1], 0), true),
            (Position::new(vec![0], 3), false),
        ] {
            let mut content = broken();
            let mut styles = Styles::collect(Some(&content), None);
            let mut editing = Editing::default();
            editing.reset();
            let mut editor = super::page_editor();
            editor.select(egui_richedit::Selection::caret(caret.clone()));
            let mut model = FlowModel::new(&mut content, &mut styles, vec![1, 0], &mut editing);
            assert!(model.remove_break_by_key(&mut editor, backspace));
            assert_eq!(model.text(&vec![1]).as_deref(), Some("Two"), "not joined");
            assert_eq!(
                editor.selection().map(|s| s.focus.clone()),
                Some(caret),
                "the caret stays"
            );
            assert!(
                !model.remove_break_by_key(&mut editor, backspace),
                "the next press joins"
            );
        }
    }

    #[test]
    fn enter_after_a_page_break_does_not_break_again() {
        let mut content = broken();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut model = FlowModel::new(&mut content, &mut styles, vec![1, 0], &mut editing);
        model.apply(
            Edit::Split {
                at: Position::new(vec![1], 3),
            },
            true,
        );
        let broken_at = |model: &FlowModel<'_>, index: usize| {
            model.page_break_here(&egui_richedit::Selection::caret(Position::new(
                vec![index],
                0,
            )))
        };
        assert!(broken_at(&model, 1));
        assert!(!broken_at(&model, 2), "the new paragraph carries no break");
    }

    #[test]
    fn a_second_ctrl_enter_changes_nothing_and_records_nothing() {
        let mut content = broken();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut editor = super::page_editor();
        editor.select(egui_richedit::Selection::caret(Position::new(vec![1], 0)));
        let mut model = FlowModel::new(&mut content, &mut styles, vec![1, 0], &mut editing);
        assert!(!model.insert_page_break(&mut editor));
        assert!(!editing.modified());
    }

    #[test]
    fn ctrl_enter_breaks_the_page_at_the_caret_and_undo_restores_tree() {
        let mut content = odox_core::xml::parse(
            br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"><office:body><office:text><text:p>Before after</text:p></office:text></office:body></office:document-content>"#,
            "test",
        )
        .expect("a document");
        let original = content.clone();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut editor = super::page_editor();
        editor.select(egui_richedit::Selection::caret(Position::new(vec![0], 7)));
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        assert!(model.insert_page_break(&mut editor));
        let caret = editor.selection().map(|s| s.focus.clone());
        assert_eq!(model.text(&vec![1]).as_deref(), Some("after"));
        assert_eq!(caret.as_ref().map(|c| c.offset), Some(0));
        let page = caret.expect("a caret").paragraph;
        assert!(model.page_break_here(&egui_richedit::Selection::caret(Position::new(page, 0))));
        let mut current = holding(&content);
        let mut undone = 0;
        while editing.undo(&mut current, None).is_some() {
            undone += 1;
        }
        assert_eq!(undone, 1);
        assert_eq!(current.content, original);
    }

    #[test]
    fn merging_a_selection_of_cells_is_one_step_and_undo_restores_tree() {
        let mut content = odox_core::xml::parse(
            br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"><office:body><office:text><table:table><table:table-column/><table:table-column/><table:table-row><table:table-cell><text:p>a</text:p></table:table-cell><table:table-cell><text:p>b</text:p></table:table-cell></table:table-row><table:table-row><table:table-cell><text:p>c</text:p></table:table-cell><table:table-cell><text:p>d</text:p></table:table-cell></table:table-row></table:table></office:text></office:body></office:document-content>"#,
            "test",
        )
        .expect("a document");
        let original = content.clone();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut editor = super::page_editor();
        editor.select(egui_richedit::Selection {
            anchor: Position::new(vec![0, 2, 0, 0], 0),
            focus: Position::new(vec![0, 3, 1, 0], 1),
        });
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        model.merge_cells(&mut editor).expect("merged");
        assert_eq!(model.text(&vec![0, 2, 0, 3]).as_deref(), Some("d"));
        let mut current = holding(&content);
        let mut undone = 0;
        while editing.undo(&mut current, None).is_some() {
            undone += 1;
        }
        assert_eq!(undone, 1);
        assert_eq!(current.content, original);
    }

    #[test]
    fn splitting_a_cell_not_merged_records_nothing() {
        let mut content = odox_core::xml::parse(
            br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"><office:body><office:text><table:table><table:table-column/><table:table-row><table:table-cell><text:p>a</text:p></table:table-cell></table:table-row></table:table></office:text></office:body></office:document-content>"#,
            "test",
        )
        .expect("a document");
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut editor = super::page_editor();
        editor.select(egui_richedit::Selection::caret(Position::new(
            vec![0, 1, 0, 0],
            0,
        )));
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        model.split_cell(&mut editor).expect("nothing to split");
        assert!(!editing.modified());
    }
    #[test]
    fn a_selection_becomes_a_link_that_is_found_again_and_taken_off() {
        let mut content = content();
        for (prefix, uri) in [
            ("text", "urn:oasis:names:tc:opendocument:xmlns:text:1.0"),
            ("xlink", "http://www.w3.org/1999/xlink"),
        ] {
            content.set_attr(odox_core::xml::Name::new("xmlns", prefix, Ns::Xmlns), uri);
        }
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        let mut editor = super::page_editor();
        editor.select(egui_richedit::Selection {
            anchor: Position::new(vec![0], 0),
            focus: Position::new(vec![0], 3),
        });
        let here = model.link_here(&editor).expect("a selection");
        assert_eq!((here.text.as_str(), here.link.is_none()), ("one", true));
        model
            .put_link(&mut editor, &here, "one", "https://example.com")
            .expect("made");
        let here = model.link_here(&editor).expect("the caret");
        assert_eq!(here.href, "https://example.com");
        assert!(here.link.is_some());
        model.take_link(&here).expect("taken off");
        assert!(model.link_here(&editor).expect("the caret").link.is_none());
        assert_eq!(model.text(&vec![0]).as_deref(), Some("one"));
        assert!(
            model
                .put_link(&mut editor, &here, "one", "file:///etc")
                .is_err()
        );
    }

    fn linked() -> Element {
        let mut content = content();
        for (prefix, uri) in [
            ("text", "urn:oasis:names:tc:opendocument:xmlns:text:1.0"),
            ("xlink", "http://www.w3.org/1999/xlink"),
        ] {
            content.set_attr(odox_core::xml::Name::new("xmlns", prefix, Ns::Xmlns), uri);
        }
        let mut link = Element::new("text", "a", Ns::Text);
        link.set_attr(
            odox_core::xml::Name::new("xlink", "href", Ns::Xlink),
            "https://a.example",
        );
        link.children = vec![Node::Text("here".to_owned())];
        let paragraph = content.at_mut(&[0, 0, 0]).expect("the first paragraph");
        paragraph.children = vec![
            Node::Text("click ".to_owned()),
            Node::Element(link),
            Node::Text(" now".to_owned()),
        ];
        content
    }

    #[test]
    fn a_selection_beside_a_link_is_a_new_link_not_that_one() {
        let mut content = linked();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        let model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        let mut editor = super::page_editor();
        for (anchor, focus) in [(0, 6), (14, 10)] {
            editor.select(egui_richedit::Selection {
                anchor: Position::new(vec![0], anchor),
                focus: Position::new(vec![0], focus),
            });
            let here = model.link_here(&editor).expect("a selection");
            assert!(here.link.is_none(), "{anchor}..{focus}: {here:?}");
        }
        editor.select(egui_richedit::Selection {
            anchor: Position::new(vec![0], 7),
            focus: Position::new(vec![0], 9),
        });
        assert!(model.link_here(&editor).expect("inside").link.is_some());
    }

    #[test]
    fn a_question_left_behind_by_an_edit_is_refused() {
        let mut content = linked();
        let mut styles = Styles::collect(Some(&content), None);
        let mut editing = Editing::default();
        editing.reset();
        let mut model = FlowModel::new(&mut content, &mut styles, vec![0, 0], &mut editing);
        let mut editor = super::page_editor();
        editor.select(egui_richedit::Selection::caret(Position::new(vec![0], 8)));
        let mut here = model.link_here(&editor).expect("in the link");
        here.text = "elsewhere".to_owned();
        assert!(
            model
                .put_link(&mut editor, &here, "there", "https://b.example")
                .is_err()
        );
        assert_eq!(model.text(&vec![0]).as_deref(), Some("click here now"));
    }

    fn with_header() -> odox_core::doc::TextDocument {
        let mut document =
            odox_core::doc::TextDocument::read(&odox_core::blank::text("en-US")).expect("a blank");
        document.set_header(true).expect("on");
        document
    }

    fn type_in_header(document: &mut odox_core::doc::TextDocument, editing: &mut Editing) {
        let path = document.band_path(false).expect("a header");
        let Document {
            styles_part,
            styles,
            ..
        } = &mut document.document;
        let part = styles_part.as_mut().expect("styles");
        let mut model = FlowModel::new(part, styles, path, editing).in_styles_part();
        model.apply(
            Edit::Replace {
                from: Position::new(vec![0], 0),
                to: Position::new(vec![0], 0),
                text: "Head",
            },
            true,
        );
    }

    fn header_text(document: &odox_core::doc::TextDocument) -> String {
        let path = document.band_path(false).expect("a header");
        document
            .document
            .styles_part
            .as_ref()
            .and_then(|part| part.at(&path))
            .map(Element::plain_text)
            .unwrap_or_default()
    }

    #[test]
    fn typing_in_header_changes_styles_part_only() {
        let mut document = with_header();
        let content = document.document.content.clone();
        let mut editing = Editing::default();
        editing.reset();
        type_in_header(&mut document, &mut editing);
        assert_eq!(header_text(&document), "Head");
        assert_eq!(document.document.content, content);
        assert!(editing.modified());
    }

    #[test]
    fn undo_restores_header() {
        let mut document = with_header();
        let content = document.document.content.clone();
        let mut editing = Editing::default();
        editing.reset();
        type_in_header(&mut document, &mut editing);
        editing.undo(&mut document.document, None).expect("a step");
        assert_eq!(header_text(&document), "");
        assert_eq!(document.document.content, content);
        editing.redo(&mut document.document, None).expect("a step");
        assert_eq!(header_text(&document), "Head");
    }
}
