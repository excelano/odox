//! The German catalogue can be put in force and says what it should.
//!
//! In its own file rather than beside the pseudolocale's test, because a
//! catalogue is put in force once per process — `potext` keeps it in a
//! `OnceLock`, and a window reads it on every frame without locking — so two
//! languages cannot be asserted in one test binary. An integration test is its
//! own process.
//!
//! What this catches is the state the first German build was seen in: a
//! catalogue compiled in, never chosen, and every window drawing English —
//! which is what a working English window draws, so nothing else says.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_ui::i18n::{LANGUAGES, set_language, t};

#[test]
fn german_is_shipped_and_can_be_chosen() {
    let chosen = set_language("de", LANGUAGES);
    assert_eq!(chosen.as_deref(), Some("de"), "German was not chosen");

    // The fleet's words, from Comma's glossary, so every window agrees.
    assert_eq!(t("File"), "Datei");
    assert_eq!(t("Open…"), "Öffnen …");
    assert_eq!(t("Reload"), "Neu einlesen");
    assert_eq!(
        t("OpenDocument Spreadsheet"),
        "OpenDocument-Tabellendokument"
    );

    // A placeholder is looked up by name at run time and has to survive the
    // catalogue untouched, or the message reaches the window with braces in it.
    let message = t("{file} could not be opened: {reason}");
    assert!(
        message.contains("{file}") && message.contains("{reason}"),
        "{message}"
    );
    assert_ne!(message, "{file} could not be opened: {reason}");

    // A region's locale tag falls back to the language.
    assert_eq!(set_language("de-AT", LANGUAGES).as_deref(), Some("de"));
}
