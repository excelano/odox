//! The paragraphs under a flow's root, as the page editor sees them.
//!
//! A paragraph is named by its path of child indices from the root the flow
//! draws, which is the path [`crate::Flow`] reports it under. The editor's
//! edits become `odox-core` edits on the tree, and an edit that begins an undo
//! step is recorded in [`Editing`] once it has succeeded. DESIGN.md §11.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::cell::OnceCell;

use eframe::egui::Id;
use egui_richedit::{Edit, Model, Position, RichEdit};
use odox_core::edit::{self, is_paragraph};
use odox_core::{Element, Ns, Refused};

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

/// The paragraphs under a root in a content tree, for one frame's editing.
pub struct FlowModel<'a> {
    content: &'a mut Element,
    /// Where the flow's root is under the content root.
    root: Vec<usize>,
    editing: &'a mut Editing,
    /// Every editable paragraph in drawing order, found when first asked for
    /// and forgotten when an edit changes it.
    order: OnceCell<Vec<Vec<usize>>>,
}

impl<'a> FlowModel<'a> {
    /// The paragraphs under the element at a path in a content tree.
    pub fn new(content: &'a mut Element, root: Vec<usize>, editing: &'a mut Editing) -> Self {
        Self {
            content,
            root,
            editing,
            order: OnceCell::new(),
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
            let mut out = Vec::new();
            if let Some(root) = self.root() {
                blocks(root, &mut Vec::new(), &mut out);
            }
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
        let order = self.order();
        let first = order.iter().position(|p| *p == from.paragraph)?;
        let last = order.iter().position(|p| *p == to.paragraph)?;
        let covered: Vec<Vec<usize>> = order.get(first..=last)?.to_vec();
        let ranges: Vec<(Vec<usize>, usize, usize)> = covered
            .into_iter()
            .map(|path| {
                let len = self.text(&path).map_or(0, |t| t.chars().count());
                let start = if path == from.paragraph {
                    from.offset
                } else {
                    0
                };
                let end = if path == to.paragraph { to.offset } else { len };
                (path, start, end.max(start))
            })
            .collect();
        if text.is_empty() && ranges.iter().all(|(_, start, end)| start == end) {
            return None;
        }
        for (path, start, end) in ranges.into_iter().rev() {
            let with = if path == from.paragraph { text } else { "" };
            let paragraph = self.paragraph_mut(&path)?;
            edit::replace(paragraph, start..end, with);
        }
        Some(())
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
        let before = new_step.then(|| self.content.clone());
        let at = match edit {
            Edit::Replace { from, to, text } => self
                .replace(&from, &to, text)
                .map(|()| Position::new(from.paragraph, from.offset + text.chars().count())),
            Edit::Split { at } => {
                let root = self.content.at_mut(&self.root)?;
                edit::split_at(root, &at.paragraph, at.offset)
                    .ok()
                    .map(|second| Position::new(second, 0))
            }
        };
        if at.is_some() {
            if let Some(before) = before {
                self.editing.record_snapshot(before);
            }
            self.order = OnceCell::new();
        }
        at
    }
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
    } else if is_block_container(element) {
        blocks(element, path, out);
    }
}

#[cfg(test)]
mod tests {
    use super::FlowModel;
    use crate::Editing;
    use egui_richedit::{Edit, Model, Position};
    use odox_core::{Element, Node, Ns};

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
        let mut editing = Editing::default();
        let model = FlowModel::new(&mut content, vec![0, 0], &mut editing);
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
        let mut editing = Editing::default();
        editing.reset();
        let mut model = FlowModel::new(&mut content, vec![0, 0], &mut editing);
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
        let mut editing = Editing::default();
        editing.reset();
        let mut model = FlowModel::new(&mut content, vec![0, 0], &mut editing);
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
        let mut editing = Editing::default();
        editing.reset();
        let mut model = FlowModel::new(&mut content, vec![0, 0], &mut editing);
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
    fn typing_that_runs_on_past_a_save_is_a_change() {
        let mut content = content();
        let mut editing = Editing::default();
        editing.reset();
        let at = |offset| Position::new(vec![0], offset);
        let typed = |offset| Edit::Replace {
            from: at(offset),
            to: at(offset),
            text: "x",
        };
        let mut model = FlowModel::new(&mut content, vec![0, 0], &mut editing);
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
        let mut editing = Editing::default();
        editing.reset();
        let mut model = FlowModel::new(&mut content, vec![0, 0], &mut editing);
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
        while let Some(previous) = editing.undo(&current) {
            current = previous;
            undone += 1;
        }
        assert_eq!(undone, 1);
    }
}
