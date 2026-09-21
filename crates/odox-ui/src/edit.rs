//! What the shell keeps about a document being edited: whether editing is on,
//! what has changed since the file was read, and how to take it back.
//!
//! Undo is a stack of snapshots of the content tree rather than a log of
//! commands. The tree derives `Clone`, a document of the size these
//! applications open clones in well under a millisecond, and commands would buy
//! redo granularity nobody has asked for. DESIGN.md §11.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::Element;

/// How many edits can be taken back. Past this the oldest is forgotten.
const DEPTH: usize = 100;

/// The shell's editing state, handed to the view on every frame.
#[derive(Default)]
pub struct Editing {
    /// Whether the window is in edit mode. Outside it a view draws and
    /// selects and never opens an editor.
    pub on: bool,
    /// A question is up over the window, and the keys belong to it: a view
    /// does not open an editor on the Enter that answers it.
    pub asking: bool,
    undo: Vec<Element>,
    redo: Vec<Element>,
    /// The depth of the undo stack when the document was last read or saved,
    /// which is the state the file on disk holds. `None` once that state can no
    /// longer be reached by undoing, because a new edit was made below it.
    saved_at: Option<usize>,
}

impl Editing {
    /// A document was just read or closed: nothing to undo, nothing changed.
    pub fn reset(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.saved_at = Some(0);
    }

    /// Take a snapshot of the content tree before an edit changes it.
    ///
    /// Every edit calls this first, with the tree as it stands, and then
    /// mutates. A redo history is discarded, because the edit that follows is
    /// a new branch.
    pub fn record(&mut self, content: &Element) {
        if self.saved_at.is_some_and(|depth| depth > self.undo.len()) {
            // The saved state was above this point and is now off the line.
            self.saved_at = None;
        }
        self.redo.clear();
        self.undo.push(content.clone());
        if self.undo.len() > DEPTH {
            self.undo.remove(0);
            self.saved_at = self.saved_at.and_then(|depth| depth.checked_sub(1));
        }
    }

    /// Take the last edit back. Given the tree as it stands, so that the edit
    /// can be redone; answers the tree to put in its place.
    pub fn undo(&mut self, current: &Element) -> Option<Element> {
        let previous = self.undo.pop()?;
        self.redo.push(current.clone());
        Some(previous)
    }

    /// Put back the edit last taken back.
    pub fn redo(&mut self, current: &Element) -> Option<Element> {
        let next = self.redo.pop()?;
        self.undo.push(current.clone());
        Some(next)
    }

    /// Whether there is anything to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is anything to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Whether the document differs from the file it was read from or last
    /// saved to. Undoing back to that state answers false again.
    pub fn modified(&self) -> bool {
        self.saved_at != Some(self.undo.len())
    }

    /// The document was written: what it holds now is what the file holds.
    pub fn mark_saved(&mut self) {
        self.saved_at = Some(self.undo.len());
    }
}

#[cfg(test)]
mod tests {
    use super::Editing;
    use odox_core::{Element, Ns};

    fn tree(text: &str) -> Element {
        let mut e = Element::new("text", "p", Ns::Text);
        e.children.push(odox_core::Node::Text(text.to_owned()));
        e
    }

    #[test]
    fn undo_returns_to_the_saved_state_and_redo_leaves_it() {
        let mut editing = Editing::default();
        editing.reset();
        assert!(!editing.modified());

        let mut current = tree("one");
        editing.record(&current);
        current = tree("two");
        assert!(editing.modified());

        current = editing.undo(&current).expect("something to undo");
        assert_eq!(current, tree("one"));
        assert!(!editing.modified(), "undone to what the file holds");

        current = editing.redo(&current).expect("something to redo");
        assert_eq!(current, tree("two"));
        assert!(editing.modified());
    }

    #[test]
    fn a_new_edit_below_the_saved_state_makes_it_unreachable() {
        let mut editing = Editing::default();
        editing.reset();
        let mut current = tree("one");
        editing.record(&current);
        current = tree("two");
        editing.mark_saved();
        assert!(!editing.modified());

        // Back to "one", then a different second edit: the saved "two" is on
        // a branch that no longer exists.
        current = editing.undo(&current).expect("something to undo");
        assert!(editing.modified());
        editing.record(&current);
        assert!(
            editing.modified(),
            "the same depth is not the same document"
        );
        assert!(!editing.can_redo(), "the branch the save was on is gone");
    }

    #[test]
    fn the_stack_is_bounded() {
        let mut editing = Editing::default();
        editing.reset();
        for i in 0..(super::DEPTH + 10) {
            editing.record(&tree(&i.to_string()));
        }
        assert!(editing.modified());
        let mut undone = 0;
        let mut current = tree("last");
        while let Some(previous) = editing.undo(&current) {
            current = previous;
            undone += 1;
        }
        assert_eq!(undone, super::DEPTH);
        assert!(
            editing.modified(),
            "the saved state was forgotten with the oldest snapshots"
        );
    }
}
