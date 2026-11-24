//! A page break put in by hand and taken out again.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::edit::{insert_page_break, page_break, remove_page_break, text};
use odox_core::{Element, Styles, xml};

const NAMESPACES: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0""#;

fn content(body: &str) -> Element {
    let source = format!(
        "<office:document-content {NAMESPACES}><office:automatic-styles/><office:body><office:text>{body}</office:text></office:body></office:document-content>"
    );
    xml::parse(source.as_bytes(), "test").expect("a content root")
}

#[test]
fn ctrl_enter_mid_paragraph_splits_and_breaks() {
    let mut root = content("<text:p>Before after</text:p>");
    let mut styles = Styles::collect(Some(&root), None);
    let first = vec![1, 0, 0];
    let second = insert_page_break(&mut root, &first, 7, &mut styles).expect("broken");
    assert_eq!(second, vec![1, 0, 1]);
    assert_eq!(text(root.at(&first).expect("first")), "Before ");
    assert_eq!(text(root.at(&second).expect("second")), "after");
    assert!(!page_break(&root, &first, &styles));
    assert!(page_break(&root, &second, &styles));
    let written = String::from_utf8(xml::serialize(&root)).expect("UTF-8");
    assert!(written.contains(r#"fo:break-before="page""#), "{written}");
}

#[test]
fn ctrl_enter_at_start_breaks_before() {
    let mut root = content("<text:p>One</text:p><text:p>Two</text:p>");
    let mut styles = Styles::collect(Some(&root), None);
    let two = vec![1, 0, 1];
    let at = insert_page_break(&mut root, &two, 0, &mut styles).expect("broken");
    assert_eq!(at, two);
    assert_eq!(
        root.at(&[1, 0]).expect("the body").children.len(),
        2,
        "no split"
    );
    assert!(page_break(&root, &two, &styles));
}

#[test]
fn remove_break_clears_break_before() {
    let mut root = content("<text:p>One</text:p><text:p>Two</text:p>");
    let mut styles = Styles::collect(Some(&root), None);
    let two = vec![1, 0, 1];
    insert_page_break(&mut root, &two, 0, &mut styles).expect("broken");
    assert!(page_break(&root, &two, &styles));
    remove_page_break(&mut root, &two, &mut styles).expect("joined");
    assert!(!page_break(&root, &two, &styles));
    let written = String::from_utf8(xml::serialize(&root)).expect("UTF-8");
    let name = root
        .at(&two)
        .and_then(|p| p.attr(&odox_core::Ns::Text, "style-name"))
        .expect("a style");
    let own = written
        .split(&format!(r#"style:name="{name}""#))
        .nth(1)
        .and_then(|rest| rest.split("</style:style>").next())
        .unwrap_or_default();
    assert!(!own.contains("break-before"), "{own}");
}

#[test]
fn refuses_a_break_in_a_table_cell_and_changes_nothing() {
    let source = format!(
        r#"<office:document-content {NAMESPACES} xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"><office:automatic-styles/><office:body><office:text><table:table><table:table-row><table:table-cell><text:p>Cell text</text:p></table:table-cell></table:table-row></table:table></office:text></office:body></office:document-content>"#
    );
    let mut root = xml::parse(source.as_bytes(), "test").expect("a content root");
    let before = root.clone();
    let mut styles = Styles::collect(Some(&root), None);
    assert_eq!(
        insert_page_break(&mut root, &[1, 0, 0, 0, 0, 0], 4, &mut styles),
        Err(odox_core::Refused::Structure)
    );
    assert!(root == before);
}

#[test]
fn removing_an_inherited_break_writes_one_style() {
    let named = xml::parse(
        format!(r#"<office:document-styles {NAMESPACES}><office:styles><style:style style:name="Chapter" style:family="paragraph"><style:paragraph-properties fo:break-before="page"/></style:style></office:styles></office:document-styles>"#).as_bytes(),
        "test",
    )
    .expect("styles");
    let mut root = content(r#"<text:p>One</text:p><text:p text:style-name="Chapter">Two</text:p>"#);
    let mut styles = Styles::collect(Some(&root), Some(&named));
    let two = vec![1, 0, 1];
    remove_page_break(&mut root, &two, &mut styles).expect("removed");
    assert!(!page_break(&root, &two, &styles));
    let automatic = root.at(&[0]).expect("automatic styles").elements().count();
    assert_eq!(automatic, 1, "one style, and none left unused");
}
