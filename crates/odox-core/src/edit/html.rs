//! The clipboard's HTML: paragraphs written in the small part of HTML that
//! every program reads, and that part read back.
//!
//! What is written is paragraphs, headings and list items, with bold, italic,
//! underline, strikethrough, line breaks and links in them. What is read is
//! the same, from the tags that say it or from a `style` attribute that does,
//! since a browser copies a document's formatting that way; anything else in
//! what is read, a table, a picture, a colour, gives up its text and nothing
//! more. A paragraph read takes its marks as spans with automatic styles, the
//! ones a mark put on by hand would have, written by [`super::format`].
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::fmt::Write as _;

use super::{Mark, Refused, format, normalize};
use crate::style::{Family, ListKind, Styles, TextProperties};
use crate::xml::{Element, Node, Ns};

/// What a paragraph is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Body text.
    Body,
    /// A heading of a level, one to six.
    Heading(u8),
    /// An item of a list.
    List(ListKind),
}

/// A paragraph as HTML carried it: what it is, and its text in runs, each
/// with the marks it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    /// What it is.
    pub kind: Kind,
    /// Its text, a line break as `'\n'`, in runs of the same marks.
    pub runs: Vec<(String, Vec<Mark>)>,
}

const MARKS: [Mark; 4] = [Mark::Bold, Mark::Italic, Mark::Underline, Mark::Strike];

fn tags(mark: Mark) -> (&'static str, &'static str) {
    match mark {
        Mark::Bold => ("<b>", "</b>"),
        Mark::Italic => ("<i>", "</i>"),
        Mark::Underline => ("<u>", "</u>"),
        Mark::Strike => ("<s>", "</s>"),
    }
}

/// Paragraphs as HTML, each as what it is, with what its spans give it over
/// its own style marked, so that a heading's weight is the heading's and not
/// a bold.
pub fn write(paragraphs: &[(&Element, Kind)], styles: &Styles) -> String {
    let mut out = String::from("<meta charset=\"utf-8\">");
    let mut list: Option<ListKind> = None;
    for (paragraph, kind) in paragraphs {
        let this = match kind {
            Kind::List(kind) => Some(*kind),
            _ => None,
        };
        if this != list {
            if let Some(open) = list {
                out.push_str(list_tags(open).1);
            }
            if let Some(open) = this {
                out.push_str(list_tags(open).0);
            }
            list = this;
        }
        let (open, close) = match kind {
            Kind::Heading(level) => {
                let level = (*level).clamp(1, 6);
                (format!("<h{level}>"), format!("</h{level}>"))
            }
            Kind::List(_) => ("<li>".to_owned(), "</li>".to_owned()),
            Kind::Body => ("<p>".to_owned(), "</p>".to_owned()),
        };
        let name = paragraph
            .attr(&Ns::Text, "style-name")
            .unwrap_or("Standard");
        let base = styles.resolve(&Family::Paragraph, name).text.clone();
        out.push_str(&open);
        inline(paragraph, &base, &base, styles, &mut out);
        out.push_str(&close);
    }
    if let Some(open) = list {
        out.push_str(list_tags(open).1);
    }
    out
}

fn list_tags(kind: ListKind) -> (&'static str, &'static str) {
    match kind {
        ListKind::Bullet => ("<ul>", "</ul>"),
        ListKind::Number => ("<ol>", "</ol>"),
    }
}

fn inline(
    parent: &Element,
    base: &TextProperties,
    inherited: &TextProperties,
    styles: &Styles,
    out: &mut String,
) {
    for child in &parent.children {
        match child {
            Node::Text(text) | Node::CData(text) => marked(&escape(text), base, inherited, out),
            Node::Element(e) if e.is(&Ns::Text, "s") => {
                let spaces = e.attr_usize(&Ns::Text, "c").unwrap_or(1).max(1);
                marked(&"&nbsp;".repeat(spaces), base, inherited, out);
            }
            Node::Element(e) if e.is(&Ns::Text, "tab") => marked("\t", base, inherited, out),
            Node::Element(e) if e.is(&Ns::Text, "line-break") => out.push_str("<br>"),
            Node::Element(e) if e.is(&Ns::Text, "span") => {
                let name = e.attr(&Ns::Text, "style-name").unwrap_or_default();
                let own = styles.resolve(&Family::Text, name).text.over(inherited);
                inline(e, base, &own, styles, out);
            }
            Node::Element(e) if e.is(&Ns::Text, "a") => {
                let href = e.attr(&Ns::Xlink, "href").unwrap_or_default();
                let _ = write!(out, "<a href=\"{}\">", escape(href));
                inline(e, base, inherited, styles, out);
                out.push_str("</a>");
            }
            Node::Element(e) => inline(e, base, inherited, styles, out),
            _ => {}
        }
    }
}

/// Text, inside the tags of each mark it has that its paragraph's own style
/// does not.
fn marked(text: &str, base: &TextProperties, own: &TextProperties, out: &mut String) {
    let on: Vec<Mark> = MARKS
        .into_iter()
        .filter(|mark| mark.of(own) == Some(true) && mark.of(base) != Some(true))
        .collect();
    for mark in &on {
        out.push_str(tags(*mark).0);
    }
    out.push_str(text);
    for mark in on.iter().rev() {
        out.push_str(tags(*mark).1);
    }
}

/// Rows of cells as an HTML table, each cell's text as it is shown.
pub fn table(rows: &[Vec<String>]) -> String {
    let mut out = String::from("<meta charset=\"utf-8\"><table>");
    for row in rows {
        out.push_str("<tr>");
        for cell in row {
            let _ = write!(out, "<td>{}</td>", escape(cell));
        }
        out.push_str("</tr>");
    }
    out.push_str("</table>");
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The paragraphs in a piece of HTML, as far as this reads it.
pub fn read(html: &str) -> Vec<Paragraph> {
    let mut reader = Reader::default();
    let mut rest = html;
    while !rest.is_empty() {
        let Some(at) = rest.find('<') else {
            reader.text(rest);
            break;
        };
        reader.text(&rest[..at]);
        rest = &rest[at..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |end| &after[end + 3..]);
            continue;
        }
        // A tag left open at the end is not one.
        let Some(end) = tag_end(rest) else {
            break;
        };
        reader.tag(&rest[1..end - 1]);
        rest = &rest[end..];
    }
    reader.flush();
    reader.paragraphs
}

/// Where a tag that begins a string ends, past its `>`, a `>` inside a
/// quoted attribute value not counting.
fn tag_end(text: &str) -> Option<usize> {
    let mut quote = None;
    for (at, c) in text.char_indices().skip(1) {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if q == c => quote = None,
            (None, '>') => return Some(at + 1),
            _ => {}
        }
    }
    None
}

/// An element still open, and what it says of each mark.
struct Open {
    tag: String,
    marks: Vec<(Mark, bool)>,
}

#[derive(Default)]
struct Reader {
    open: Vec<Open>,
    /// Inside an element whose text is not the document's, such as a style
    /// sheet or a script: how deep.
    skipped: usize,
    runs: Vec<(String, Vec<Mark>)>,
    /// A cell was read in the row so far, so the next begins after a tab.
    cell: bool,
    paragraphs: Vec<Paragraph>,
}

const SKIPPED: [&str; 4] = ["head", "style", "script", "title"];
const BLOCKS: [&str; 22] = [
    "p",
    "div",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "li",
    "ul",
    "ol",
    "blockquote",
    "pre",
    "tr",
    "table",
    "section",
    "article",
    "header",
    "footer",
    "dt",
    "dd",
    "hr",
];
const VOID: [&str; 8] = ["br", "hr", "img", "meta", "link", "input", "col", "wbr"];

impl Reader {
    fn tag(&mut self, inside: &str) {
        let (closing, inside) = match inside.strip_prefix('/') {
            Some(rest) => (true, rest),
            None => (false, inside),
        };
        let inside = inside.trim_end_matches('/');
        let name_end = inside
            .find(|c: char| c.is_whitespace())
            .unwrap_or(inside.len());
        let tag = inside[..name_end].to_ascii_lowercase();
        if tag.is_empty() || tag.starts_with(['!', '?']) {
            return;
        }
        if SKIPPED.contains(&tag.as_str()) {
            if closing {
                self.skipped = self.skipped.saturating_sub(1);
            } else {
                self.skipped += 1;
            }
            return;
        }
        if self.skipped > 0 {
            return;
        }
        if tag == "br" {
            self.push("\n");
            return;
        }
        if matches!(tag.as_str(), "td" | "th") && !closing {
            if self.cell {
                self.push("\t");
            }
            self.cell = true;
        }
        let block = BLOCKS.contains(&tag.as_str());
        if block {
            self.flush();
        }
        if tag == "tr" {
            self.cell = false;
        }
        if closing {
            if let Some(at) = self.open.iter().rposition(|open| open.tag == tag) {
                self.open.truncate(at);
            }
        } else if !VOID.contains(&tag.as_str()) {
            let marks = marks_of(&tag, &inside[name_end..]);
            self.open.push(Open { tag, marks });
        }
    }

    fn text(&mut self, raw: &str) {
        if self.skipped > 0 || raw.is_empty() {
            return;
        }
        let decoded = decode(raw);
        let pre = self.open.iter().any(|open| open.tag == "pre");
        if pre {
            self.push(&decoded.replace('\u{a0}', " "));
            return;
        }
        // A run of whitespace is one space, and none where a line begins.
        let mut words = String::new();
        for c in decoded.chars() {
            if c.is_whitespace() && c != '\u{a0}' {
                let previous = words
                    .chars()
                    .last()
                    .or_else(|| self.runs.last().and_then(|(text, _)| text.chars().last()));
                if previous.is_some_and(|p| !matches!(p, ' ' | '\n' | '\t')) {
                    words.push(' ');
                }
                continue;
            }
            words.push(if c == '\u{a0}' { ' ' } else { c });
        }
        if !words.is_empty() {
            self.push(&words);
        }
    }

    /// Add text to the paragraph being read, in the marks open now.
    fn push(&mut self, text: &str) {
        let marks = self.marks();
        match self.runs.last_mut() {
            Some((last, last_marks)) if *last_marks == marks => last.push_str(text),
            _ => self.runs.push((text.to_owned(), marks)),
        }
    }

    fn marks(&self) -> Vec<Mark> {
        MARKS
            .into_iter()
            .filter(|mark| {
                self.open
                    .iter()
                    .rev()
                    .find_map(|open| {
                        open.marks
                            .iter()
                            .find(|(m, _)| m == mark)
                            .map(|(_, on)| *on)
                    })
                    .unwrap_or(false)
            })
            .collect()
    }

    /// What the innermost heading or list item open says the paragraph is.
    fn kind(&self) -> Kind {
        for (at, open) in self.open.iter().enumerate().rev() {
            if let [b'h', level @ b'1'..=b'6'] = open.tag.as_bytes() {
                return Kind::Heading(level - b'0');
            }
            if open.tag == "li" {
                let ordered = self.open[..at]
                    .iter()
                    .rev()
                    .find(|o| o.tag == "ul" || o.tag == "ol")
                    .is_some_and(|o| o.tag == "ol");
                return Kind::List(if ordered {
                    ListKind::Number
                } else {
                    ListKind::Bullet
                });
            }
        }
        Kind::Body
    }

    /// End the paragraph being read, if it holds any text.
    fn flush(&mut self) {
        if let Some((last, _)) = self.runs.last_mut() {
            let kept = last.trim_end_matches([' ', '\n']).len();
            last.truncate(kept);
        }
        self.runs.retain(|(text, _)| !text.is_empty());
        if self.runs.is_empty() {
            return;
        }
        let kind = self.kind();
        let runs = std::mem::take(&mut self.runs);
        self.paragraphs.push(Paragraph { kind, runs });
    }
}

/// What an element says of the marks: by its tag, then by a `style`
/// attribute, which a browser writes and which may say the opposite, as a
/// `<b style="font-weight:normal">` around a whole document does.
fn marks_of(tag: &str, attributes: &str) -> Vec<(Mark, bool)> {
    let mut marks = Vec::new();
    match tag {
        "b" | "strong" => marks.push((Mark::Bold, true)),
        "i" | "em" | "cite" | "var" => marks.push((Mark::Italic, true)),
        "u" | "ins" => marks.push((Mark::Underline, true)),
        "s" | "strike" | "del" => marks.push((Mark::Strike, true)),
        _ => {}
    }
    let Some(style) = attribute(attributes, "style") else {
        return marks;
    };
    for declaration in style.split(';') {
        let Some((property, value)) = declaration.split_once(':') else {
            continue;
        };
        let value = value.trim().to_ascii_lowercase();
        let mut set = |mark: Mark, on: bool| {
            marks.retain(|(m, _)| *m != mark);
            marks.push((mark, on));
        };
        match property.trim().to_ascii_lowercase().as_str() {
            "font-weight" => set(
                Mark::Bold,
                value == "bold"
                    || value == "bolder"
                    || value.parse::<u16>().is_ok_and(|weight| weight >= 600),
            ),
            "font-style" => set(Mark::Italic, value == "italic" || value == "oblique"),
            "text-decoration" | "text-decoration-line" => {
                if value.contains("underline") {
                    set(Mark::Underline, true);
                }
                if value.contains("line-through") {
                    set(Mark::Strike, true);
                }
                if value == "none" {
                    set(Mark::Underline, false);
                    set(Mark::Strike, false);
                }
            }
            _ => {}
        }
    }
    marks
}

/// The value of an attribute in the text after a tag's name.
fn attribute(attributes: &str, name: &str) -> Option<String> {
    let lower = attributes.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let at = from + found;
        from = at + name.len();
        let before = lower[..at].chars().last();
        if before.is_some_and(|c| !c.is_whitespace()) {
            continue;
        }
        let rest = attributes[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let value = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or_default(),
            _ => rest.split_whitespace().next().unwrap_or_default(),
        };
        return Some(decode(value));
    }
    None
}

/// Text with its character references read: the five XML names, `&nbsp;`,
/// and numbered ones. Any other is left as written.
fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest.find(';').filter(|&end| end <= 10);
        let decoded = end.and_then(|end| {
            let name = &rest[1..end];
            match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ => name
                    .strip_prefix("#x")
                    .or_else(|| name.strip_prefix("#X"))
                    .map_or_else(
                        || name.strip_prefix('#').and_then(|n| n.parse().ok()),
                        |hex| u32::from_str_radix(hex, 16).ok(),
                    )
                    .and_then(char::from_u32),
            }
        });
        if let (Some(c), Some(end)) = (decoded, end) {
            out.push(c);
            rest = &rest[end + 1..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

/// Paragraphs read from HTML, as `text:p` elements of the document whose
/// content root this is, each run given its marks by the automatic styles a
/// mark put on by hand would have. The styles are written into the content's
/// automatic styles and told to `styles`; a document with none is given them
/// before its body, as [`format`] does.
///
/// # Errors
///
/// The content root does not declare a namespace a mark is written in.
/// Nothing is changed.
pub fn paragraphs(
    content: &mut Element,
    read: &[Paragraph],
    styles: &mut Styles,
) -> Result<Vec<Element>, Refused> {
    let automatic = content
        .child(&Ns::Office, "automatic-styles")
        .cloned()
        .unwrap_or_else(|| format::element(content.name_for(&Ns::Office, "automatic-styles")));
    let known = automatic.children.len();
    // The paragraphs are marked under a root of their own, which carries the
    // document's namespaces and its automatic styles, so that a style is used
    // again or named past them as it would be in the document.
    let mut scratch = Element {
        attrs: content.attrs.clone(),
        children: vec![Node::Element(automatic)],
        ..format::element(content.name.clone())
    };
    let name = content.name_for(&Ns::Text, "p");
    for paragraph in read {
        let mut element = format::element(name.clone());
        let text: String = paragraph
            .runs
            .iter()
            .map(|(text, _)| text.as_str())
            .collect();
        element.children.push(Node::Text(text));
        normalize(&mut element);
        element.self_closing = element.children.is_empty();
        scratch.children.push(Node::Element(element));
    }
    for (index, paragraph) in read.iter().enumerate() {
        let mut at = 0;
        for (text, marks) in &paragraph.runs {
            let length = text.chars().count();
            for mark in marks {
                format(
                    &mut scratch,
                    &[index + 1],
                    at..at + length,
                    *mark,
                    true,
                    styles,
                )?;
            }
            at += length;
        }
    }
    let mut children = scratch.children.into_iter();
    let Some(Node::Element(automatic)) = children.next() else {
        return Err(Refused::NotFound);
    };
    let minted: Vec<Node> = automatic.children.into_iter().skip(known).collect();
    let paragraphs = children
        .filter_map(|node| match node {
            Node::Element(element) => Some(element),
            _ => None,
        })
        .collect();
    if !minted.is_empty() {
        let container = format::automatic_styles(content);
        container.children.extend(minted);
        container.self_closing = false;
    }
    Ok(paragraphs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(paragraph: &Paragraph) -> Vec<(&str, Vec<Mark>)> {
        paragraph
            .runs
            .iter()
            .map(|(text, marks)| (text.as_str(), marks.clone()))
            .collect()
    }

    #[test]
    fn the_subset_is_read_with_its_marks_and_kinds() {
        let read = read(
            "<html><head><style>p{color:red}</style></head><body><!--StartFragment-->\
             <h2>A  heading</h2><p>Plain <b>bold <i>both</i></b> &amp; <u>under</u><br>next</p>\
             <ol><li>one</li><li><s>two</s></li></ol><ul><li>dot</li></ul>\
             <!--EndFragment--></body></html>",
        );
        let kinds: Vec<Kind> = read.iter().map(|p| p.kind).collect();
        assert_eq!(
            kinds,
            [
                Kind::Heading(2),
                Kind::Body,
                Kind::List(ListKind::Number),
                Kind::List(ListKind::Number),
                Kind::List(ListKind::Bullet),
            ]
        );
        assert_eq!(runs(&read[0]), [("A heading", vec![])]);
        assert_eq!(
            runs(&read[1]),
            [
                ("Plain ", vec![]),
                ("bold ", vec![Mark::Bold]),
                ("both", vec![Mark::Bold, Mark::Italic]),
                (" & ", vec![]),
                ("under", vec![Mark::Underline]),
                ("\nnext", vec![]),
            ]
        );
        assert_eq!(runs(&read[3]), [("two", vec![Mark::Strike])]);
    }

    #[test]
    fn a_browsers_styles_say_what_its_tags_do_and_may_unsay_them() {
        let read = read(
            "<b style=\"font-weight:normal;\" id=\"docs-internal-guid\"><p>\
             <span style=\"font-weight:700\">heavy</span>\
             <span style=\"font-style: italic; text-decoration: underline\">slant</span>\
             light</p></b>",
        );
        assert_eq!(
            runs(&read[0]),
            [
                ("heavy", vec![Mark::Bold]),
                ("slant", vec![Mark::Italic, Mark::Underline]),
                ("light", vec![]),
            ]
        );
    }

    #[test]
    fn a_table_read_gives_its_rows_with_the_cells_tab_separated() {
        let read =
            read("<table><tr><td>a</td><td>1</td></tr><tr><th>b</th><td>2</td></tr></table>");
        let texts: Vec<&str> = read.iter().map(|p| p.runs[0].0.as_str()).collect();
        assert_eq!(texts, ["a\t1", "b\t2"]);
    }

    #[test]
    fn what_is_not_html_is_read_without_harm() {
        for text in ["<", "<aé", "a & b", "&#xFFFFFFFF;", "<p>é<", "<<>>", "&amp"] {
            let _ = read(text);
        }
        assert_eq!(read("a &amp b")[0].runs[0].0, "a &amp b");
    }

    #[test]
    fn a_table_is_written_with_its_text_escaped() {
        assert_eq!(
            table(&[vec!["a<b".to_owned(), "&".to_owned()]]),
            "<meta charset=\"utf-8\"><table><tr><td>a&lt;b</td><td>&amp;</td></tr></table>"
        );
    }
}
