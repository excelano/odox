//! Fields put in at the caret: a date, fixed or following, the page number
//! and the page count.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::edit::{Field, follow_dates, insert_field};
use odox_core::{Element, xml};

const NAMESPACES: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0""#;

fn content(body: &str) -> Element {
    let source = format!(
        "<office:document-content {NAMESPACES}><office:body><office:text>{body}</office:text></office:body></office:document-content>"
    );
    xml::parse(source.as_bytes(), "test").expect("a content root")
}

fn written(root: &Element) -> String {
    String::from_utf8(xml::serialize(root.at(&[0, 0]).expect("the text"))).expect("UTF-8")
}

const FIRST: [usize; 3] = [0, 0, 0];
const TODAY: (i64, i64, i64) = (2026, 11, 23);

#[test]
fn date_field_written_fixed_or_following() {
    let mut root = content("<text:p>Dated </text:p>");
    insert_field(&mut root, &FIRST, 6, Field::Date { fixed: true }, TODAY).expect("put in");
    insert_field(&mut root, &FIRST, 0, Field::Date { fixed: false }, TODAY).expect("put in");
    let out = written(&root);
    assert!(
        out.contains(r#"<text:date text:date-value="2026-11-23">2026-11-23</text:date>Dated <text:date text:date-value="2026-11-23" text:fixed="true">2026-11-23</text:date>"#),
        "{out}"
    );
}

#[test]
fn page_fields_are_written_as_libreoffice_writes_them() {
    let mut root = content("<text:p>Page <text:s/>of </text:p>");
    insert_field(&mut root, &FIRST, 9, Field::PageCount, TODAY).expect("put in");
    insert_field(&mut root, &FIRST, 5, Field::PageNumber, TODAY).expect("put in");
    let out = written(&root);
    assert!(
        out.contains(r#"<text:page-number text:select-page="current">1</text:page-number>"#),
        "{out}"
    );
    assert!(
        out.contains("<text:page-count>1</text:page-count>"),
        "{out}"
    );
    assert_eq!(
        root.at(&FIRST).expect("paragraph").plain_text(),
        "Page 1 of 1"
    );
}

#[test]
fn fixed_date_keeps_its_value() {
    let mut root = content(
        r#"<text:p><text:date text:date-value="2020-01-02" text:fixed="true">2020-01-02</text:date> and <text:date text:date-value="2020-01-02">2020-01-02</text:date></text:p>"#,
    );
    assert!(follow_dates(&mut root, TODAY));
    let out = written(&root);
    assert!(
        out.contains(
            r#"<text:date text:date-value="2020-01-02" text:fixed="true">2020-01-02</text:date>"#
        ),
        "{out}"
    );
    assert!(
        out.contains(r#"<text:date text:date-value="2026-11-23">2026-11-23</text:date>"#),
        "{out}"
    );
    assert!(!follow_dates(&mut root, TODAY), "nothing left to change");
}

#[test]
fn following_date_shows_today_without_changing_tree() {
    let root = content(
        r#"<text:p><text:date text:date-value="2020-01-02">2020-01-02</text:date><text:date text:date-value="2020-01-02" text:fixed="true">2020-01-02</text:date></text:p>"#,
    );
    let before = root.clone();
    let styles = odox_core::Styles::collect(Some(&root), None);
    let paragraph = root.at(&FIRST).expect("paragraph");
    let dates: Vec<&Element> = paragraph.elements().collect();
    assert_eq!(
        odox_core::doc::date_shown(dates[0], TODAY, &styles).as_deref(),
        Some("2026-11-23")
    );
    assert_eq!(odox_core::doc::date_shown(dates[1], TODAY, &styles), None);
    assert!(root == before);
}
