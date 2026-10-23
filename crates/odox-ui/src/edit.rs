//! What the shell keeps about a document being edited: whether editing is on,
//! what has changed since the file was read, and how to take it back.
//!
//! Undo is a stack of snapshots of the content tree rather than a log of
//! commands. The tree derives `Clone`, a document of the size these
//! applications open clones in well under a millisecond, and commands would buy
//! redo granularity nobody has asked for. DESIGN.md §11.
//!
//! Each snapshot also holds where the caret stood in that state, where the
//! view has one, so that taking an edit back or putting it back shows the
//! place it happened, and the package entries the edit wrote outside the
//! tree, such as the settings a sheet's freeze is kept in.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use egui_richedit::Position;
use odox_core::{Document, Element, Kept};

/// How many edits can be taken back. Past this the oldest is forgotten.
const DEPTH: usize = 100;

/// Where a caret stood: a paragraph's path under the root the view draws, and
/// a character offset into its text.
pub type Caret = Position<Vec<usize>>;

/// A state of the content tree and where the caret was in it.
struct Snapshot {
    content: Element,
    parts: Kept,
    caret: Option<Caret>,
}

/// The shell's editing state, handed to the view on every frame.
#[derive(Default)]
pub struct Editing {
    /// Whether the window is in edit mode. Outside it a view draws and
    /// selects and never opens an editor.
    pub on: bool,
    /// A question is up over the window, and the keys belong to it: a view
    /// does not open an editor on the Enter that answers it.
    pub asking: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The depth of the undo stack when the document was last read or saved,
    /// which is the state the file on disk holds. `None` once that state can no
    /// longer be reached by undoing, because a new edit was made below it.
    saved_at: Option<usize>,
    /// Counts every change to the content, including the ones an undo step
    /// absorbs, so that whatever is derived from the text can tell it is stale.
    revision: u64,
}

impl Editing {
    /// A document was just read or closed: nothing to undo, nothing changed.
    pub fn reset(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.saved_at = Some(0);
        self.touch();
    }

    /// The content changed, in a way that need not have begun an undo step.
    pub fn touch(&mut self) {
        self.revision += 1;
    }

    /// A number that is different whenever the text may be.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Take a snapshot of the content tree before an edit changes it.
    ///
    /// Every edit calls this first, with the tree as it stands, and then
    /// mutates. A redo history is discarded, because the edit that follows is
    /// a new branch.
    pub fn record(&mut self, content: &Element) {
        self.record_snapshot(content.clone(), None);
    }

    /// The same, for a snapshot already taken, with where the caret stood in
    /// it: an editor that learns only after an edit whether it succeeded
    /// takes the tree before trying, and records it once it has.
    pub fn record_snapshot(&mut self, content: Element, caret: Option<Caret>) {
        self.record_step(content, Kept::default(), caret);
    }

    /// The same, with the package entries the edit is about to write besides
    /// the tree, as they stood before it.
    pub fn record_step(&mut self, content: Element, parts: Kept, caret: Option<Caret>) {
        if self.saved_at.is_some_and(|depth| depth > self.undo.len()) {
            // The saved state was above this point and is now off the line.
            self.saved_at = None;
        }
        self.redo.clear();
        self.touch();
        self.undo.push(Snapshot {
            content,
            parts,
            caret,
        });
        if self.undo.len() > DEPTH {
            self.undo.remove(0);
            self.saved_at = self.saved_at.and_then(|depth| depth.checked_sub(1));
        }
    }

    /// Take the last edit back: the document's tree, and the entries the edit
    /// wrote, go back to how they were. Given where the caret is, so that the
    /// edit can be redone; answers where the caret was.
    pub fn undo(&mut self, document: &mut Document, caret: Option<Caret>) -> Option<Option<Caret>> {
        let previous = self.undo.pop()?;
        let current = Self::put_back(document, previous.content, previous.parts, caret);
        self.redo.push(current);
        self.touch();
        Some(previous.caret)
    }

    /// Put back the edit last taken back, the same way.
    pub fn redo(&mut self, document: &mut Document, caret: Option<Caret>) -> Option<Option<Caret>> {
        let next = self.redo.pop()?;
        let current = Self::put_back(document, next.content, next.parts, caret);
        self.undo.push(current);
        self.touch();
        Some(next.caret)
    }

    /// Put a state into the document, answering the one it replaced.
    fn put_back(
        document: &mut Document,
        content: Element,
        parts: Kept,
        caret: Option<Caret>,
    ) -> Snapshot {
        Snapshot {
            content: std::mem::replace(&mut document.content, content),
            parts: document.package.restore(parts),
            caret,
        }
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
    use odox_core::{Document, Element, Ns};

    fn tree(text: &str) -> Element {
        let mut e = Element::new("text", "p", Ns::Text);
        e.children.push(odox_core::Node::Text(text.to_owned()));
        e
    }

    /// A document whose content is one paragraph of this text.
    fn document(text: &str) -> Document {
        let mut document = Document::read(
            &odox_core::blank::text("en"),
            &[odox_core::media_type::TEXT],
        )
        .expect("a blank document");
        document.content = tree(text);
        document
    }

    #[test]
    fn undo_returns_to_the_saved_state_and_redo_leaves_it() {
        let mut editing = Editing::default();
        editing.reset();
        assert!(!editing.modified());

        let mut current = document("one");
        editing.record(&current.content);
        current.content = tree("two");
        assert!(editing.modified());

        editing.undo(&mut current, None).expect("something to undo");
        assert_eq!(current.content, tree("one"));
        assert!(!editing.modified(), "undone to what the file holds");

        editing.redo(&mut current, None).expect("something to redo");
        assert_eq!(current.content, tree("two"));
        assert!(editing.modified());
    }

    #[test]
    fn a_new_edit_below_the_saved_state_makes_it_unreachable() {
        let mut editing = Editing::default();
        editing.reset();
        let mut current = document("one");
        editing.record(&current.content);
        current.content = tree("two");
        editing.mark_saved();
        assert!(!editing.modified());

        // Back to "one", then a different second edit: the saved "two" is on
        // a branch that no longer exists.
        editing.undo(&mut current, None).expect("something to undo");
        assert!(editing.modified());
        editing.record(&current.content);
        assert!(
            editing.modified(),
            "the same depth is not the same document"
        );
        assert!(!editing.can_redo(), "the branch the save was on is gone");
    }

    #[test]
    fn undo_and_redo_answer_where_the_caret_was() {
        let mut editing = Editing::default();
        editing.reset();
        let before = super::Caret::new(vec![0], 1);
        let after = super::Caret::new(vec![0], 4);
        editing.record_snapshot(tree("one"), Some(before.clone()));
        let mut current = document("one more");
        let caret = editing
            .undo(&mut current, Some(after.clone()))
            .expect("something to undo");
        assert_eq!(caret, Some(before), "where the edit began");
        let caret = editing.redo(&mut current, None).expect("something to redo");
        assert_eq!(caret, Some(after), "where the caret was when it was undone");
    }

    #[test]
    fn the_entries_an_edit_wrote_go_back_with_it() {
        let mut editing = Editing::default();
        editing.reset();
        let mut current = document("one");
        let kept = current.package.keep(&["settings.xml"]);
        editing.record_step(current.content.clone(), kept, None);
        current.package.set_part("settings.xml", b"<x/>".to_vec());
        editing.undo(&mut current, None).expect("something to undo");
        assert!(
            current.package.part("settings.xml").is_none(),
            "absent again"
        );
        editing.redo(&mut current, None).expect("something to redo");
        assert_eq!(
            current
                .package
                .part("settings.xml")
                .map(|p| p.data.as_slice()),
            Some(&b"<x/>"[..])
        );
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
        let mut current = document("last");
        while editing.undo(&mut current, None).is_some() {
            undone += 1;
        }
        assert_eq!(undone, super::DEPTH);
        assert!(
            editing.modified(),
            "the saved state was forgotten with the oldest snapshots"
        );
    }
}
