//! A page's header and footer: which one a page shows, and its fields filled
//! in for that page, as a PDF prints them. DESIGN.md §12.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::Value;
use crate::style::Styles;
use crate::xml::{Element, Node, Ns};

/// What a header's or footer's fields stand for on one page.
pub struct PageFields<'a> {
    /// The page's number, from one.
    pub page: usize,
    /// How many pages there are.
    pub pages: usize,
    /// The sheet the page prints, for a spreadsheet.
    pub sheet: Option<&'a str>,
    /// The document's title, and its file's name.
    pub title: &'a str,
    /// Today, as year, month and day, for a date that is not fixed.
    pub today: (i64, i64, i64),
}

/// The header, or the footer, a master page gives a page by its number from
/// one: the first page's own where the master gives the first page one, a
/// left page's where the page is even and the master gives left pages one,
/// and the master's own otherwise. `None` where the master has none, or turns
/// it off. A first or left one with `style:display="false"` is the master's
/// own, which is how `LibreOffice` writes "same content".
pub fn page_band(master: &Element, footer: bool, page: usize) -> Option<&Element> {
    let base = if footer { "footer" } else { "header" };
    let shown = |element: &&Element| element.attr(&Ns::Style, "display") != Some("false");
    let own = |suffix: &str| {
        master
            .child(&Ns::Style, &format!("{base}{suffix}"))
            .filter(shown)
    };
    let main = own("")?;
    let first = own("-first").filter(|_| page == 1);
    let left = own("-left").filter(|_| page.is_multiple_of(2));
    Some(first.or(left).unwrap_or(main))
}

/// A copy of a header or footer with each field put in as what it stands for
/// on a page: the page's number and the count, the sheet, the title and the
/// file's name, and a date that is not fixed as today in the field's data
/// style. A field not known here keeps the text it was saved with.
pub fn with_page_fields(band: &Element, fields: &PageFields<'_>, styles: &Styles) -> Element {
    let mut band = band.clone();
    fill(&mut band, fields, styles);
    band
}

fn fill(element: &mut Element, fields: &PageFields<'_>, styles: &Styles) {
    for node in &mut element.children {
        let Node::Element(child) = node else {
            continue;
        };
        let shown = if child.name.ns == Ns::Text {
            match &*child.name.local {
                "page-number" => Some(fields.page.to_string()),
                "page-count" => Some(fields.pages.to_string()),
                "sheet-name" => fields.sheet.map(ToOwned::to_owned),
                "title" | "file-name" => Some(fields.title.to_owned()),
                "date" if child.attr(&Ns::Text, "fixed") != Some("true") => {
                    Some(today(child, fields.today, styles))
                }
                _ => None,
            }
        } else {
            None
        };
        match shown {
            Some(text) => *node = Node::Text(text),
            None => fill(child, fields, styles),
        }
    }
}

fn today(field: &Element, (year, month, day): (i64, i64, i64), styles: &Styles) -> String {
    let iso = format!("{year:04}-{month:02}-{day:02}");
    field
        .attr(&Ns::Style, "data-style-name")
        .and_then(|name| styles.data_styles().format(name, &Value::Date(iso.clone())))
        .unwrap_or(iso)
}
