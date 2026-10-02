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
use egui_richedit::{Edit, Mark, Model, Position, RichEdit, Selection};
use odox_core::edit::{self, is_paragraph};
use odox_core::{Element, ListKind, Ns, Refused, Styles};

use crate::Editing;
use crate::flow::is_block_container;

/// The editor over a flow's paragraphs.
pub type PageEditor = RichEdit<Vec<usize>>;

/// The id the page editor takes the keyboard under. The shell reads it to
/// know that typing on the page still leaves its shortcuts to it.
pub(crate) fn page_editor_id() -> Id {
    Id::new("odox-page-editor")
}

/// A page editor with nothing selected.
pub fn page_editor() -> PageEditor {
    RichEdit::new(page_editor_id())
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

    /// Make the paragraphs a selection runs over a kind of paragraph, or take
    /// them out of it where they already are one, as one step to undo, and put
    /// the selection back over the same paragraphs. Answers whether the
    /// document changed.
    pub fn apply_block(&mut self, editor: &mut PageEditor, block: Block) -> bool {
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
        let lit = self.is_block(&selection, block);
        let before = self.content.clone();
        let done = match block {
            Block::Heading(_) | Block::Body => {
                let level = match block {
                    Block::Heading(level) if !lit => Some(level),
                    _ => None,
                };
                self.set_headings(&paragraphs, level)
            }
            Block::Bullets | Block::Numbers => {
                let kind = match block {
                    Block::Bullets => ListKind::Bullet,
                    _ => ListKind::Number,
                };
                self.set_lists(&paragraphs, (!lit).then_some(kind))
            }
        };
        if done.is_none() || *self.content == before {
            *self.content = before;
            return false;
        }
        self.order = OnceCell::new();
        let begins = Position::new(
            [self.tag.as_slice(), &selection.focus.paragraph].concat(),
            selection.focus.offset,
        );
        self.editing.touch();
        self.editing.record_snapshot(before, Some(begins));
        let order = self.order().to_vec();
        if let (Some(a), Some(f)) = (order.get(anchor), order.get(focus)) {
            editor.select(Selection {
                anchor: Position::new(a.clone(), selection.anchor.offset),
                focus: Position::new(f.clone(), selection.focus.offset),
            });
        }
        true
    }

    fn set_headings(&mut self, paragraphs: &[Vec<usize>], level: Option<u8>) -> Option<()> {
        let automatic =
            |content: &Element| content.child(&Ns::Office, "automatic-styles").is_some();
        for paragraph in paragraphs {
            let had = automatic(self.content);
            let path = [self.root.as_slice(), paragraph].concat();
            edit::set_heading(self.content, &path, level, self.styles).ok()?;
            self.follow_styles(had);
        }
        Some(())
    }

    fn set_lists(&mut self, paragraphs: &[Vec<usize>], kind: Option<ListKind>) -> Option<()> {
        let had = self
            .content
            .child(&Ns::Office, "automatic-styles")
            .is_some();
        let paths: Vec<Vec<usize>> = paragraphs
            .iter()
            .map(|paragraph| [self.root.as_slice(), paragraph].concat())
            .collect();
        edit::set_list(self.content, &paths, kind, self.styles).ok()?;
        self.follow_styles(had);
        Some(())
    }

    /// Styles written into a document that had nowhere to keep them go before
    /// its body, and move the body, and the root, one along.
    fn follow_styles(&mut self, had: bool) {
        if !had
            && self
                .content
                .child(&Ns::Office, "automatic-styles")
                .is_some()
            && let Some(first) = self.root.first_mut()
        {
            *first += 1;
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
        if at_end_of_heading {
            let path = [self.root.as_slice(), &second].concat();
            // A document that cannot be given the paragraph style keeps the
            // heading, which is a split all the same.
            let _ = edit::set_heading(self.content, &path, None, self.styles);
        }
        Some(second)
    }

    /// Give the text from one position to another a mark, or take it off,
    /// paragraph by paragraph.
    fn format(
        &mut self,
        from: &Position<Vec<usize>>,
        to: &Position<Vec<usize>>,
        mark: edit::Mark,
        on: bool,
    ) -> Option<()> {
        let automatic =
            |content: &Element| content.child(&Ns::Office, "automatic-styles").is_some();
        for (path, range) in self.covered(from, to)? {
            if range.is_empty() {
                continue;
            }
            let had = automatic(self.content);
            let mut at = self.root.clone();
            at.extend(&path);
            edit::format(self.content, &at, range, mark, on, self.styles).ok()?;
            // Styles written into a document that had nowhere to keep them go
            // before its body, and move the body, and the root, one along.
            if !had
                && automatic(self.content)
                && let Some(first) = self.root.first_mut()
            {
                *first += 1;
            }
        }
        Some(())
    }
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
            Edit::Replace { from, .. } | Edit::Format { from, .. } => from,
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
            Edit::Format { from, to, mark, on } => {
                self.format(&from, &to, core_mark(mark), on).map(|()| to)
            }
        };
        if at.is_some() {
            self.editing.touch();
            if let Some(before) = before {
                self.editing.record_snapshot(before, Some(begins));
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

#[cfg(test)]
mod tests {
    use super::FlowModel;
    use crate::Editing;
    use egui_richedit::{Edit, Model, Position};
    use odox_core::{Element, Node, Ns, Styles};

    fn element(prefix: &str, local: &str, ns: Ns, children: Vec<Node>) -> Node {
        let mut e = Element::new(prefix, local, ns);
        e.children = children;
        Node::Element(e)
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
        let current = content.clone();
        let (_, caret) = editing.undo(&current, None).expect("a step to undo");
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
        let mut current = content.clone();
        while let Some(previous) = editing.undo(&current, None).map(|(tree, _)| tree) {
            current = previous;
            undone += 1;
        }
        assert_eq!(undone, 1);
    }
}
