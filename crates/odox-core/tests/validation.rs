//! Validity rules as LibreOffice writes them, in
//! `corpus/libreoffice/validation.ods`: on Sheet1, column A takes a whole
//! number from 1 to 10, B a decimal of nought or more with a warning, C one
//! of red, green or blue and refuses an empty cell, D five characters or
//! fewer with information, E a date in 2026, F a time between nine and six,
//! and G a positive whole number with no message shown.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use odox_core::doc::{SheetDocument, Value, Verdict};

fn fixture() -> SheetDocument {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/validation.ods"),
    )
    .expect("the fixture");
    SheetDocument::read(&bytes).expect("it reads")
}

fn verdict(document: &SheetDocument, column: usize, value: &Value) -> Verdict {
    document
        .validation(0, 0, column)
        .expect("a rule")
        .check(value)
}

fn allowed(verdict: &Verdict) -> bool {
    matches!(verdict, Verdict::Allowed)
}

#[test]
fn whole_number_between_refuses_outside() {
    let document = fixture();
    assert!(allowed(&verdict(&document, 0, &Value::Number(7.0))));
    assert!(allowed(&verdict(&document, 0, &Value::Number(10.0))));
    let Verdict::Refused(message) = verdict(&document, 0, &Value::Number(11.0)) else {
        panic!("eleven is refused");
    };
    assert_eq!(message.title, "Out of range");
    assert_eq!(message.text, "Type a whole number from 1 to 10.");
    assert!(matches!(
        verdict(&document, 0, &Value::Number(2.5)),
        Verdict::Refused(_)
    ));
    assert!(matches!(
        verdict(&document, 0, &Value::Text("five".to_owned())),
        Verdict::Refused(_)
    ));
}

#[test]
fn decimal_allows_fraction() {
    let document = fixture();
    assert!(allowed(&verdict(&document, 1, &Value::Number(2.5))));
}

#[test]
fn warning_message_type() {
    let document = fixture();
    let Verdict::Warned(message) = verdict(&document, 1, &Value::Number(-1.0)) else {
        panic!("a negative price is warned about");
    };
    assert_eq!(message.text, "A price is not usually negative.");
    assert!(matches!(
        verdict(&document, 3, &Value::Text("toolong".to_owned())),
        Verdict::Informed(_)
    ));
}

#[test]
fn list_refuses_value_not_listed() {
    let document = fixture();
    assert!(allowed(&verdict(
        &document,
        2,
        &Value::Text("green".to_owned())
    )));
    assert!(matches!(
        verdict(&document, 2, &Value::Text("pink".to_owned())),
        Verdict::Refused(_)
    ));
}

#[test]
fn text_length() {
    let document = fixture();
    assert!(allowed(&verdict(
        &document,
        3,
        &Value::Text("abcde".to_owned())
    )));
    assert!(!allowed(&verdict(
        &document,
        3,
        &Value::Text("abcdef".to_owned())
    )));
}

#[test]
fn empty_allowed_unless_forbidden() {
    let document = fixture();
    assert!(allowed(&verdict(&document, 0, &Value::Empty)));
    assert!(matches!(
        verdict(&document, 2, &Value::Empty),
        Verdict::Refused(_)
    ));
}

#[test]
fn dates_and_times_between() {
    let document = fixture();
    assert!(allowed(&verdict(
        &document,
        4,
        &Value::Date("2026-03-01".to_owned())
    )));
    assert!(!allowed(&verdict(
        &document,
        4,
        &Value::Date("2027-03-01".to_owned())
    )));
    assert!(allowed(&verdict(
        &document,
        5,
        &Value::Time("PT10H00M00S".to_owned())
    )));
    assert!(!allowed(&verdict(
        &document,
        5,
        &Value::Time("PT20H00M00S".to_owned())
    )));
}

#[test]
fn a_rule_whose_message_is_not_shown_allows() {
    let document = fixture();
    assert!(allowed(&verdict(&document, 6, &Value::Number(-3.0))));
}

#[test]
fn a_cell_with_no_rule_has_none() {
    let document = fixture();
    assert!(document.validation(0, 1, 3).is_none());
    assert!(document.validation(0, 9, 0).is_none());
}

#[test]
fn document_is_unchanged_after_open() {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/validation.ods"),
    )
    .expect("the fixture");
    let document = SheetDocument::read(&bytes).expect("it reads");
    let _ = document
        .validation(0, 0, 0)
        .map(|rule| rule.check(&Value::Number(50.0)));
    let plain = odox_core::doc::Document::read(&bytes, odox_core::media_type::SPREADSHEET_ANY)
        .expect("it reads");
    assert!(document.document.content == plain.content);
}

/// The fixture with one of its rules' attributes set.
fn with_rule(rule: &str, attributes: &[(&str, &str)]) -> SheetDocument {
    let mut document = fixture();
    fn find<'a>(
        element: &'a mut odox_core::Element,
        rule: &str,
    ) -> Option<&'a mut odox_core::Element> {
        if element.is(&odox_core::Ns::Table, "content-validation")
            && element.attr(&odox_core::Ns::Table, "name") == Some(rule)
        {
            return Some(element);
        }
        element.children.iter_mut().find_map(|child| match child {
            odox_core::xml::Node::Element(child) => find(child, rule),
            _ => None,
        })
    }
    let element = find(&mut document.document.content, rule).expect("the rule");
    for (local, value) in attributes {
        let name = element.name_for(&odox_core::Ns::Table, local);
        element.set_attr(name, *value);
    }
    document
}

#[test]
fn a_list_of_quoted_numbers_takes_the_numbers() {
    let document = with_rule(
        "val3",
        &[("condition", r#"of:cell-content-is-in-list("1";"2";"3")"#)],
    );
    assert!(allowed(&verdict(&document, 2, &Value::Number(2.0))));
    assert!(!allowed(&verdict(&document, 2, &Value::Number(4.0))));
}

#[test]
fn a_list_can_care_about_case() {
    let document = with_rule("val3", &[("case-sensitive", "true")]);
    assert!(allowed(&verdict(
        &document,
        2,
        &Value::Text("red".to_owned())
    )));
    assert!(!allowed(&verdict(
        &document,
        2,
        &Value::Text("RED".to_owned())
    )));
}

#[test]
fn a_date_typed_as_text_is_read_as_a_date_or_let_in() {
    let document = fixture();
    assert!(allowed(&verdict(
        &document,
        4,
        &Value::Text("2026-03-01".to_owned())
    )));
    assert!(allowed(&verdict(
        &document,
        4,
        &Value::Text("3/1/2026".to_owned())
    )));
    assert!(!allowed(&verdict(
        &document,
        4,
        &Value::Text("2027-03-01".to_owned())
    )));
    assert!(allowed(&verdict(
        &document,
        4,
        &Value::Text("no date".to_owned())
    )));
    assert!(allowed(&verdict(
        &document,
        5,
        &Value::Text("10:30".to_owned())
    )));
}

#[test]
fn a_whole_number_is_whole_within_rounding() {
    let document = fixture();
    assert!(allowed(&verdict(&document, 0, &Value::Number(5.0 + 1e-15))));
}
