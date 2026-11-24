//! A list item's level and a numbered list's restarts, compared with the
//! structure LibreOffice writes for the same lists in `lists.odt`.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use odox_core::doc::TextDocument;
use odox_core::edit::{continue_numbering, list_level, restart_numbering, text};
use odox_core::{Element, Ns, xml};

/// A list's structure in a line: its items, what each holds, and the numbering
/// attributes, without styles or whitespace.
fn shape(element: &Element) -> String {
    let inner = || element.elements().map(shape).collect::<Vec<_>>().join(" ");
    if element.is(&Ns::Text, "list") {
        let continues = if element.attr(&Ns::Text, "continue-numbering") == Some("true") {
            "+"
        } else {
            ""
        };
        format!("list{continues}[{}]", inner())
    } else if element.is(&Ns::Text, "list-item") {
        let start = element
            .attr(&Ns::Text, "start-value")
            .map_or(String::new(), |v| format!("={v}"));
        format!("item{start}[{}]", inner())
    } else {
        text(element)
    }
}

fn libreoffice() -> Vec<String> {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/lists.odt"),
    )
    .expect("the lists fixture");
    let document = TextDocument::read(&bytes).expect("it reads");
    document
        .body()
        .expect("a body")
        .elements()
        .filter(|e| e.is(&Ns::Text, "list"))
        .map(shape)
        .collect()
}

const NAMESPACES: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0""#;

fn body(inner: &str) -> Element {
    let source = format!(
        "<office:document-content {NAMESPACES}><office:body><office:text>{inner}</office:text></office:body></office:document-content>"
    );
    xml::parse(source.as_bytes(), "test").expect("a content root")
}

/// The paths of the paragraphs under the body, in document order.
fn paragraphs(content: &Element) -> Vec<Vec<usize>> {
    fn walk(element: &Element, path: &mut Vec<usize>, into: &mut Vec<Vec<usize>>) {
        for (index, child) in element.elements_indexed() {
            path.push(index);
            if child.is(&Ns::Text, "p") || child.is(&Ns::Text, "h") {
                into.push(path.clone());
            } else {
                walk(child, path, into);
            }
            path.pop();
        }
    }
    let mut into = Vec::new();
    walk(content, &mut Vec::new(), &mut into);
    into
}

fn lists(content: &Element) -> Vec<String> {
    content
        .at(&[0, 0])
        .expect("the body")
        .elements()
        .filter(|e| e.is(&Ns::Text, "list"))
        .map(shape)
        .collect()
}

fn item(text: &str) -> String {
    format!("<text:list-item><text:p>{text}</text:p></text:list-item>")
}

#[test]
fn tab_nests_item_in_previous_item() {
    let items: String = ["One", "Two", "Two a", "Two b", "Three"].map(item).concat();
    let mut content = body(&format!("<text:list>{items}</text:list>"));
    let all = paragraphs(&content);
    list_level(&mut content, &all[2..4], 1).expect("in a level");
    assert_eq!(lists(&content)[0], libreoffice()[0]);
}

#[test]
fn tab_on_first_item_nests_under_empty_item() {
    let items: String = ["One", "Two"].map(item).concat();
    let mut content = body(&format!("<text:list>{items}</text:list>"));
    let all = paragraphs(&content);
    list_level(&mut content, &all[..1], 1).expect("in a level");
    assert_eq!(lists(&content), ["list[item[list[item[One]]] item[Two]]"]);
}

#[test]
fn shift_tab_lifts_item_and_its_children() {
    let nested = format!(
        "<text:list>{}<text:list-item><text:p>Two</text:p><text:list>{}<text:list-item><text:p>Two b</text:p><text:list>{}</text:list></text:list-item>{}</text:list></text:list-item>{}</text:list>",
        item("One"),
        item("Two a"),
        item("Two b i"),
        item("Two c"),
        item("Three")
    );
    let mut content = body(&nested);
    let all = paragraphs(&content);
    list_level(&mut content, &all[3..4], -1).expect("out a level");
    assert_eq!(
        lists(&content),
        [
            "list[item[One] item[Two list[item[Two a]]] item[Two b list[item[Two b i] item[Two c]]] item[Three]]"
        ]
    );
    let all = paragraphs(&content);
    let before = lists(&content);
    list_level(&mut content, &all[..1], -1).expect("nothing to do");
    assert_eq!(lists(&content), before, "the first level stays");
}

#[test]
fn level_capped_at_ten() {
    fn nested(level: usize) -> String {
        let inner = if level < 10 {
            nested(level + 1)
        } else {
            String::new()
        };
        format!(
            "<text:list>{}<text:list-item><text:p>Level {level}</text:p>{inner}</text:list-item></text:list>",
            item("Before")
        )
    }
    let deep = nested(1);
    let mut content = body(&deep);
    let all = paragraphs(&content);
    let before = lists(&content);
    list_level(&mut content, &all[all.len() - 1..], 1).expect("nothing to do");
    assert_eq!(lists(&content), before);
}

#[test]
fn restart_writes_start_value() {
    let items: String = ["Again one", "Again two"].map(item).concat();
    let mut content = body(&format!("<text:list>{items}</text:list>"));
    let all = paragraphs(&content);
    restart_numbering(&mut content, &all[0]).expect("restarted");
    assert_eq!(lists(&content)[0], libreoffice()[1]);
}

#[test]
fn continue_joins_previous_list() {
    let mut content = body(&format!(
        "<text:list>{}</text:list><text:p>Between</text:p><text:list>{}</text:list>",
        item("Before"),
        item("Then three")
    ));
    let all = paragraphs(&content);
    continue_numbering(&mut content, &all[2]).expect("continued");
    assert_eq!(lists(&content)[1], libreoffice()[2]);
}

#[test]
fn an_item_of_two_paragraphs_moves_once() {
    let mut content = body(&format!(
        "<text:list>{}<text:list-item><text:p>B1</text:p><text:p>B2</text:p></text:list-item></text:list>",
        item("A")
    ));
    let all = paragraphs(&content);
    list_level(&mut content, &all[1..3], 1).expect("in a level");
    assert_eq!(lists(&content), ["list[item[A list[item[B1 B2]]]]"]);
}

#[test]
fn refuses_lifting_out_of_an_item_that_goes_on_after_its_list_and_changes_nothing() {
    let mut content = body(&format!(
        "<text:list><text:list-item><text:p>A</text:p><text:list>{}</text:list><text:p>A2</text:p></text:list-item>{}</text:list>",
        item("B"),
        item("C")
    ));
    let before = content.clone();
    let all = paragraphs(&content);
    assert_eq!(
        list_level(&mut content, &all[1..2], -1),
        Err(odox_core::Refused::Structure)
    );
    assert!(content == before);
}

#[test]
fn continue_takes_back_a_restart() {
    let mut content = body(&format!(
        "<text:list>{}</text:list><text:p>Between</text:p><text:list><text:list-item text:start-value=\"1\"><text:p>Then</text:p></text:list-item></text:list>",
        item("Before")
    ));
    let all = paragraphs(&content);
    continue_numbering(&mut content, &all[2]).expect("continued");
    assert_eq!(lists(&content)[1], "list+[item[Then]]");
}

#[test]
fn refuses_restarting_at_a_list_header() {
    let mut content =
        body("<text:list><text:list-header><text:p>Head</text:p></text:list-header></text:list>");
    let before = content.clone();
    let all = paragraphs(&content);
    assert_eq!(
        restart_numbering(&mut content, &all[0]),
        Err(odox_core::Refused::Structure)
    );
    assert!(content == before);
}
