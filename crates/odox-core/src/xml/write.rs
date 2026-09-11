//! Turning an [`Element`] tree back into the bytes of a part.
//!
//! Written as strings rather than through a serializer, which is what
//! `waddle-core` already does for the same reason: the output of an office
//! package is structural, the escaping rules are four characters wide, and a
//! writer that is a loop over a tree is a writer whose output can be read.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::{Element, Name, Node};

/// Serialize a part, declaration included.
pub fn serialize(root: &Element) -> Vec<u8> {
    let mut out = String::with_capacity(4096);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    element(&mut out, root);
    out.into_bytes()
}

fn element(out: &mut String, e: &Element) {
    out.push('<');
    name(out, &e.name);
    for attr in &e.attrs {
        out.push(' ');
        name(out, &attr.name);
        out.push_str("=\"");
        escape_attribute(out, &attr.value);
        out.push('"');
    }
    if e.children.is_empty() && e.self_closing {
        out.push_str("/>");
        return;
    }
    out.push('>');
    for child in &e.children {
        match child {
            Node::Element(child) => element(out, child),
            Node::Text(t) => escape_text(out, t),
            Node::CData(t) => {
                out.push_str("<![CDATA[");
                out.push_str(t);
                out.push_str("]]>");
            }
            Node::Comment(t) => {
                out.push_str("<!--");
                out.push_str(t);
                out.push_str("-->");
            }
            Node::ProcessingInstruction(t) => {
                out.push_str("<?");
                out.push_str(t);
                out.push_str("?>");
            }
        }
    }
    out.push_str("</");
    name(out, &e.name);
    out.push('>');
}

fn name(out: &mut String, n: &Name) {
    if let Some(prefix) = &n.prefix {
        out.push_str(prefix);
        out.push(':');
    }
    out.push_str(&n.local);
}

fn escape_text(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            // Not required by the specification outside `]]>`, and written
            // anyway because every office application writes it and a part that
            // differs from the one beside it only in this is a part somebody has
            // to diff twice.
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

fn escape_attribute(out: &mut String, value: &str) {
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // A literal tab, newline or carriage return in an attribute value
            // is normalized to a space by any conforming parser, so the
            // character has to be written as a reference to survive being read
            // back. A cell's formula and a presentation's animation parameters
            // both carry newlines.
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            _ => out.push(c),
        }
    }
}
