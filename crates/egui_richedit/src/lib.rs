//! Rich-text editing for egui, over paragraphs the application lays out and
//! owns itself.
//!
//! egui's `TextEdit` edits one string drawn in one format. A document editor
//! draws paragraphs of styled runs among lists, tables and pictures of its own,
//! and wants a caret that moves through all of them as if the page were one
//! surface. This crate is that caret. The application keeps its document and
//! its layout; it describes each paragraph's text as a [`ParagraphJob`] and
//! implements [`Model`] so that an edit lands in its own document. A
//! [`RichEdit`] turns keys, the pointer and the clipboard into [`Edit`]s, and
//! paints the selection and the caret over the application's galleys.
//!
//! # A frame
//!
//! 1. [`RichEdit::input`], before anything is laid out, takes this frame's
//!    events and applies them to the model, so the layout that follows already
//!    shows what was typed.
//! 2. Then, for every editable paragraph in document order, whether or not it
//!    is on screen: build its [`ParagraphJob`], lay it out, allocate a
//!    response that senses clicks and drags, and hand all of it to
//!    [`RichEdit::paragraph`], which paints it.
//!
//! # Offsets
//!
//! A [`Position`] is a paragraph and a character offset (characters, not
//! bytes) into the model's text of that paragraph. What is drawn need not be
//! that text: a tab drawn as spaces, a footnote's citation that the model does
//! not count, a field's current value. [`ParagraphJob::atom`] records each such
//! piece, and the caret steps over it whole.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::must_use_candidate)]

mod editor;
mod job;

pub use editor::{Laid, RichEdit};
pub use job::{OffsetMap, ParagraphJob};

use std::any::Any;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

/// Where a caret can stand: a paragraph, and a character offset into the
/// model's text of it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Position<P> {
    /// The paragraph, as the model names it.
    pub paragraph: P,
    /// Characters from the start of the paragraph's text.
    pub offset: usize,
}

impl<P> Position<P> {
    /// A position in a paragraph.
    pub fn new(paragraph: P, offset: usize) -> Self {
        Self { paragraph, offset }
    }
}

/// A selection: where it began and where it has been taken to. The two are
/// the same position when the selection is a caret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection<P> {
    /// Where the selection began, which stays put while it is extended.
    pub anchor: Position<P>,
    /// Where the selection ends, which is where the caret is drawn.
    pub focus: Position<P>,
}

impl<P: Clone + PartialEq> Selection<P> {
    /// A caret: a selection of nothing, at a position.
    pub fn caret(at: Position<P>) -> Self {
        Self {
            anchor: at.clone(),
            focus: at,
        }
    }

    /// Whether the selection selects nothing.
    pub fn is_caret(&self) -> bool {
        self.anchor == self.focus
    }
}

/// A change the editor asks the model to make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit<'a, P> {
    /// Replace the text from one position to another with new text. The two
    /// may be in different paragraphs, `from` first in document order; the
    /// model then joins what is left of the first and the last. The new text
    /// never holds a paragraph break: `'\n'` in it is a line break inside the
    /// paragraph, and `'\t'` a tab.
    Replace {
        /// Where the replaced text begins.
        from: Position<P>,
        /// Where it ends.
        to: Position<P>,
        /// What goes in its place.
        text: &'a str,
    },
    /// Split a paragraph in two at a position, the second half becoming the
    /// paragraph after it.
    Split {
        /// Where the paragraph is split.
        at: Position<P>,
    },
    /// Give the text from one position to another a mark, or take it off.
    /// The two may be in different paragraphs, `from` first in document
    /// order; no paragraph is joined or split.
    Format {
        /// Where the formatted text begins.
        from: Position<P>,
        /// Where it ends.
        to: Position<P>,
        /// Which mark.
        mark: Mark,
        /// Given, or taken off.
        on: bool,
    },
}

/// Formatting a range of text is given or has taken off. The set is fixed, so
/// that neither the editor nor the model needs a style system to agree on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mark {
    /// Bold.
    Bold,
    /// Italic.
    Italic,
    /// Underlined.
    Underline,
    /// Struck through.
    Strike,
}

impl Mark {
    /// Every mark, in the order a toolbar shows them.
    pub const ALL: [Self; 4] = [Self::Bold, Self::Italic, Self::Underline, Self::Strike];
}

/// A copy of part of the document that keeps what a string cannot: the
/// formatting and the kinds of paragraph. Only the model that made one can read
/// it, so it is whatever that model likes behind a type the editor never sees.
#[derive(Clone)]
pub struct Fragment(Arc<dyn Any + Send + Sync>);

impl Fragment {
    /// Wrap whatever a model carries a copy in.
    pub fn new<T: Any + Send + Sync>(value: T) -> Self {
        Self(Arc::new(value))
    }

    /// What was wrapped, if it is a `T`.
    pub fn get<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }
}

/// The document, as the editor sees it. The application implements this over
/// whatever it keeps its document in.
pub trait Model {
    /// Names a paragraph. An edit may change what any paragraph is called, so
    /// the editor holds only the positions the model hands back.
    type Paragraph: Clone + Eq + Hash + Debug;

    /// The paragraph's text, with a line break as `'\n'` and a tab as `'\t'`.
    /// `None` when the model has no such paragraph.
    fn text(&self, paragraph: &Self::Paragraph) -> Option<String>;

    /// The editable paragraph after this one in document order.
    fn next(&self, paragraph: &Self::Paragraph) -> Option<Self::Paragraph>;

    /// The editable paragraph before this one in document order.
    fn previous(&self, paragraph: &Self::Paragraph) -> Option<Self::Paragraph>;

    /// The first editable paragraph, `None` when there is none.
    fn first(&self) -> Option<Self::Paragraph>;

    /// The last editable paragraph, `None` when there is none.
    fn last(&self) -> Option<Self::Paragraph>;

    /// Whether the text from one position to another carries a mark: `Some`
    /// when all of it says the same, `None` when it differs. Two positions
    /// that are the same answer for the text typed there would take its
    /// formatting from. A model that keeps no formatting leaves this as it
    /// is, which says nothing is marked, and refuses [`Edit::Format`].
    fn marked(
        &self,
        from: &Position<Self::Paragraph>,
        to: &Position<Self::Paragraph>,
        mark: Mark,
    ) -> Option<bool> {
        let _ = (from, to, mark);
        Some(false)
    }

    /// A copy of the text from one position to another that keeps what the
    /// plain text of it cannot, for pasting back into this model. The editor
    /// holds it beside the plain text it put on the clipboard and uses it only
    /// while the clipboard still holds that text. A model that keeps no
    /// formatting leaves this as it is and pastes are plain.
    fn fragment(
        &self,
        from: &Position<Self::Paragraph>,
        to: &Position<Self::Paragraph>,
    ) -> Option<Fragment> {
        let _ = (from, to);
        None
    }

    /// Put a [`Fragment`] this model made in at a position, as one edit, and
    /// answer where the caret stands after it. `new_step` is as for
    /// [`Self::apply`]. `None` refuses it, and the paste is plain.
    fn paste_fragment(
        &mut self,
        at: &Position<Self::Paragraph>,
        fragment: &Fragment,
        new_step: bool,
    ) -> Option<Position<Self::Paragraph>> {
        let _ = (at, fragment, new_step);
        None
    }

    /// Make an edit, and answer where the caret stands after it: after the
    /// replacing text, at the start of the second half of a split, or for a
    /// format at the end of the formatted text. `None` refuses the edit and
    /// leaves the document as it was.
    ///
    /// `new_step` says whether the edit begins a new undo step or continues
    /// the one before it: the editor groups a run of typing into one step. An
    /// application that keeps undo as snapshots takes one when `new_step` is
    /// true and the edit succeeds.
    fn apply(
        &mut self,
        edit: Edit<'_, Self::Paragraph>,
        new_step: bool,
    ) -> Option<Position<Self::Paragraph>>;
}
