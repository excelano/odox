//! Why an edit was refused, in the words a person reads, for each kind of
//! refusal the library answers. A view with something more particular to say
//! says that, and this where it has not.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::Refused;

use crate::i18n::t;

/// The sentence for a refusal.
pub fn message(refused: &Refused) -> String {
    match refused {
        Refused::Covered => t("This cell is covered by the one that spans it."),
        Refused::Namespace => {
            t("This document has no declaration for what the change would write, so it is left as it was.")
        }
        Refused::NotFound => t("There is nothing here to do that to."),
        Refused::Structure => {
            t("This would cut across a table, a cell or a frame, so nothing was changed.")
        }
        Refused::LastOne => t("A presentation keeps at least one slide."),
        Refused::Address => {
            t("A link goes to a web page, a mail address or a place in the document.")
        }
    }
    .to_owned()
}

/// What is said when something that goes at the caret is asked for with no
/// caret in the text.
pub fn no_caret() -> String {
    t("Put the caret in the text first.").to_owned()
}

#[cfg(test)]
mod tests {
    use super::message;
    use odox_core::Refused;

    #[test]
    fn each_refusal_has_a_sentence_of_its_own() {
        let all = [
            Refused::Covered,
            Refused::Namespace,
            Refused::NotFound,
            Refused::Structure,
            Refused::LastOne,
            Refused::Address,
        ];
        let said: Vec<String> = all.iter().map(message).collect();
        for (refused, sentence) in all.iter().zip(&said) {
            assert_ne!(
                *sentence,
                refused.to_string(),
                "not the library's own words"
            );
        }
        let mut distinct = said.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), said.len());
    }
}
