//! The messages the window draws, in the language the desktop asks for.
//!
//! `potext::catalog!` declares the catalogue and its lookups **in this crate**,
//! which is the point of it being a macro: a single global inside `potext` would
//! be one catalogue shared by everything linked against it.
//!
//! Every string a person reads in any of the three applications is here rather
//! than in the application crates, which is why there is one catalogue for the
//! suite and not four. An application contributes its own name, and a name is
//! not translated.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

pub use potext::fill;

potext::catalog!();

/// Every language the suite ships, as a tag and that language's catalogue.
///
/// Empty until a translation exists. The mechanism is wired from the first
/// release because retrofitting `t` onto strings already written is the
/// expensive half, and adding a language to this list is the cheap one.
pub const LANGUAGES: &[(&str, &str)] = &[];

/// Read the desktop's language and put a catalogue in force, once, at startup.
pub fn start() {
    activate(LANGUAGES);
}
