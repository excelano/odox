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
/// A translation is added by putting its `.po` beside the others and naming it
/// here. German was written against Comma's glossary, so the fleet says *Datei*,
/// *Öffnen …* and *OpenDocument-Tabellendokument* the same way in every window.
///
/// The catalogues live inside this crate because `include_str!` reaching above a
/// crate root compiles locally and fails in `cargo package`, which copies only
/// what is under the crate directory: the tarball would carry no catalogue and
/// the publish would die verifying a build that cannot compile. flyleaf lost a
/// release tag to exactly that. `cargo package -p odox-ui --list` is the check.
#[cfg(not(debug_assertions))]
pub const LANGUAGES: &[(&str, &str)] = &[("de", include_str!("../po/de.po"))];

/// The same, plus the pseudolocale, which a release does not carry.
///
/// `en-x-pseudo` translates nothing and changes everything: every message comes
/// back accented, bracketed and 40% longer. Running a window in it shows a
/// string that never went through [`t`], a sentence the catalogue never saw, and
/// a label built to the width of English — the last being what German will hit,
/// found without anybody reading German. `po/pseudo.sh` writes it and says the
/// rest.
///
/// ```text
/// POTEXT_LANG=en-x-pseudo cargo run -p xods -- corpus/libreoffice/calc.ods
/// ```
#[cfg(debug_assertions)]
pub const LANGUAGES: &[(&str, &str)] = &[
    ("de", include_str!("../po/de.po")),
    ("en-x-pseudo", include_str!("../po/en-x-pseudo.po")),
];

/// Mark a message for translation without looking it up here.
///
/// gettext's `N_`, which exists for the case this suite has: a message that has
/// to be a literal somewhere the catalogue is not yet in force — a `const`, or a
/// `Product` built before `run` activates one — and is looked up through [`t`]
/// where it is finally shown. Without a marker the extractor never sees the
/// string and the catalogue never carries it; `po/update-po.sh` names this
/// function as a keyword alongside `t`.
pub const fn mark(message: &'static str) -> &'static str {
    message
}

/// Read the desktop's language and put a catalogue in force, once, at startup.
pub fn start() {
    activate(LANGUAGES);
}

#[cfg(test)]
mod tests {
    use super::{LANGUAGES, set_language, t};

    /// Would catch the whole mechanism being inert.
    ///
    /// Every string a person reads goes through [`t`], and until something loads
    /// a catalogue every one of them comes back as the English it was written
    /// in — which is also what a correctly working English window looks like, so
    /// nothing about the running application says whether translation works at
    /// all. The pseudolocale is the one catalogue that is always here, so it is
    /// what answers.
    #[test]
    fn a_catalogue_that_is_shipped_can_be_put_in_force() {
        let chosen = set_language("en-x-pseudo", LANGUAGES);
        assert_eq!(
            chosen.as_deref(),
            Some("en-x-pseudo"),
            "the pseudolocale was not chosen"
        );
        assert_ne!(
            t("File"),
            "File",
            "the catalogue was chosen and nothing came out of it"
        );
        assert!(t("File").starts_with('['), "{}", t("File"));
        // A placeholder is looked up by name at run time and has to survive the
        // catalogue untouched, or the message reaches the window with braces in it.
        assert!(t("{file} could not be opened: {reason}").contains("{file}"));
        assert!(t("{file} could not be opened: {reason}").contains("{reason}"));
    }
}
