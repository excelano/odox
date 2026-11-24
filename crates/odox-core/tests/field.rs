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
    let styles = odox_core::Styles::collect(Some(&root), None);
    assert!(follow_dates(&mut root, None, TODAY, &styles));
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
    assert!(
        !follow_dates(&mut root, None, TODAY, &styles),
        "nothing left to change"
    );
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

#[test]
fn a_date_saved_today_keeps_its_time_and_one_from_another_day_its_style() {
    let source = format!(
        r#"<office:document-content {NAMESPACES} xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:number="urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0"><office:automatic-styles><number:date-style style:name="N30"><number:month/><number:text>/</number:text><number:day/><number:text>/</number:text><number:year/></number:date-style></office:automatic-styles><office:body><office:text><text:p><text:date style:data-style-name="N30" text:date-value="2026-11-23T10:00:00">11/23/26</text:date> <text:date style:data-style-name="N30" text:date-value="2020-01-02T10:00:00">1/2/20</text:date></text:p></office:text></office:body></office:document-content>"#
    );
    let mut root = xml::parse(source.as_bytes(), "test").expect("parsed");
    let styles = odox_core::Styles::collect(Some(&root), None);
    assert!(follow_dates(&mut root, None, TODAY, &styles));
    let out = String::from_utf8(xml::serialize(&root)).expect("UTF-8");
    assert!(
        out.contains(r#"text:date-value="2026-11-23T10:00:00">11/23/26</text:date>"#),
        "{out}"
    );
    assert!(
        out.contains(r#"text:date-value="2026-11-23">11/23/26</text:date>"#),
        "{out}"
    );
}

#[test]
fn a_spreadsheets_dates_are_left_alone() {
    let source = format!(
        r#"<office:document-content {NAMESPACES}><office:body><office:spreadsheet><text:p><text:date text:date-value="2020-01-02">2020-01-02</text:date></text:p></office:spreadsheet></office:body></office:document-content>"#
    );
    let mut root = xml::parse(source.as_bytes(), "test").expect("parsed");
    let styles = odox_core::Styles::collect(Some(&root), None);
    assert!(!follow_dates(&mut root, None, TODAY, &styles));
}

#[test]
fn a_following_date_in_a_header_follows_too() {
    let mut root = content("<text:p>Body</text:p>");
    let source = format!(
        r#"<office:document-styles {NAMESPACES} xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"><office:master-styles><style:master-page style:name="Standard"><style:header><text:p><text:date text:date-value="2020-01-02">2020-01-02</text:date></text:p></style:header></style:master-page></office:master-styles></office:document-styles>"#
    );
    let mut styles_part = xml::parse(source.as_bytes(), "styles").expect("a styles root");
    let styles = odox_core::Styles::collect(Some(&root), Some(&styles_part));
    assert!(follow_dates(
        &mut root,
        Some(&mut styles_part),
        TODAY,
        &styles
    ));
    let out = String::from_utf8(xml::serialize(&styles_part)).expect("UTF-8");
    assert!(
        out.contains(r#"<text:date text:date-value="2026-11-23">2026-11-23</text:date>"#),
        "{out}"
    );
}

#[test]
fn a_date_in_a_tracked_deletion_is_left_as_it_was_deleted() {
    let mut root = content(
        r#"<text:tracked-changes><text:changed-region text:id="c1"><text:deletion><office:change-info><dc:date xmlns:dc="http://purl.org/dc/elements/1.1/">2020-01-02T10:00:00</dc:date></office:change-info><text:p><text:date text:date-value="2020-01-02">2020-01-02</text:date></text:p></text:deletion></text:changed-region></text:tracked-changes><text:p>Body</text:p>"#,
    );
    let styles = odox_core::Styles::collect(Some(&root), None);
    assert!(!follow_dates(&mut root, None, TODAY, &styles));
}

#[test]
fn text_typed_after_a_field_goes_after_it() {
    let mut root = content("<text:p>of <text:page-count>4</text:page-count></text:p>");
    let paragraph = root.at_mut(&FIRST).expect("the paragraph");
    odox_core::edit::replace(paragraph, 4..4, " pages");
    let out = written(&root);
    assert!(
        out.contains("<text:page-count>4</text:page-count> pages</text:p>"),
        "{out}"
    );
}

#[test]
fn text_typed_before_a_field_goes_before_it() {
    let mut root = content("<text:p>of <text:page-count>4</text:page-count> pages</text:p>");
    let paragraph = root.at_mut(&FIRST).expect("the paragraph");
    odox_core::edit::replace(paragraph, 3..3, "all ");
    let out = written(&root);
    assert!(
        out.contains("of all <text:page-count>4</text:page-count>"),
        "{out}"
    );
}

#[test]
fn a_field_is_one_character_of_the_text_and_goes_when_it_is_deleted() {
    let mut root = content("<text:p>of <text:page-count>4</text:page-count> pages</text:p>");
    let paragraph = root.at_mut(&FIRST).expect("the paragraph");
    assert_eq!(odox_core::edit::text(paragraph), "of \u{fffc} pages");
    odox_core::edit::replace(paragraph, 3..4, "");
    let out = written(&root);
    assert!(!out.contains("page-count"), "{out}");
    assert_eq!(
        odox_core::edit::text(root.at(&FIRST).expect("the paragraph")),
        "of  pages"
    );
}

#[test]
fn the_field_character_is_never_written_as_text() {
    let mut root = content("<text:p>of </text:p>");
    let paragraph = root.at_mut(&FIRST).expect("the paragraph");
    odox_core::edit::replace(paragraph, 3..3, "a\u{fffc}b");
    assert_eq!(odox_core::edit::text(paragraph), "of ab");
}

#[test]
fn a_link_after_a_field_is_found_where_it_stands() {
    let root = content(
        r#"<text:p><text:page-count>4</text:page-count><text:a xmlns:xlink="http://www.w3.org/1999/xlink" xlink:href="https://example.com">link</text:a></text:p>"#,
    );
    let paragraph = root.at(&FIRST).expect("the paragraph");
    let links = odox_core::edit::links(paragraph);
    assert_eq!(links[0].0, 1..5);
    assert_eq!(
        odox_core::edit::link_at(paragraph, 2).map(|(_, range, _)| range),
        Some(1..5)
    );
}

#[test]
fn a_bookmark_still_takes_no_room() {
    let root = content(r#"<text:p>a<text:bookmark text:name="b"/>c</text:p>"#);
    assert_eq!(
        odox_core::edit::text(root.at(&FIRST).expect("the paragraph")),
        "ac"
    );
}
