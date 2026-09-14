//! Parsing a part of a package into an [`Element`] tree.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use quick_xml::events::{BytesStart, Event};
use quick_xml::name::{QName, ResolveResult};
use quick_xml::{NsReader, XmlVersion};

use super::{Attribute, Element, Name, Node, Ns};
use crate::Error;

/// Parse one part of a package.
///
/// `part` names the entry, and is carried into any error so that a window can
/// say which part of the document it could not read.
///
/// # Errors
///
/// The part is not UTF-8, or is not well-formed XML.
pub fn parse(bytes: &[u8], part: &str) -> Result<Element, Error> {
    // OpenDocument requires UTF-8 or UTF-16 and every writer in practice
    // produces UTF-8. Decoding here rather than letting the parser do it is what
    // lets an event borrow the input instead of a scratch buffer, which is the
    // difference between one copy of the part in memory and two.
    let text = std::str::from_utf8(bytes).map_err(|e| Error::Xml {
        part: part.to_owned(),
        detail: format!("not UTF-8: {e}"),
    })?;
    // A UTF-8 byte order mark is legal and the parser does not skip it.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);

    let mut reader = NsReader::from_str(text);
    let config = reader.config_mut();
    // An empty element is kept as one rather than expanded into a start and an
    // end, so that writing the tree does not rewrite `<a/>` as `<a></a>` in
    // every style in the document.
    config.expand_empty_elements = false;
    // Whitespace is significant inside a paragraph, and the whitespace between
    // elements is what makes a part readable after a round trip.
    config.trim_text_start = false;
    config.trim_text_end = false;
    config.check_end_names = true;

    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;

    loop {
        let event = reader.read_event().map_err(|e| Error::Xml {
            part: part.to_owned(),
            detail: e.to_string(),
        })?;
        match event {
            Event::Start(ref start) => {
                let element = element_of(start, &reader, false);
                stack.push(element);
            }
            Event::Empty(ref start) => {
                let element = element_of(start, &reader, true);
                push_node(&mut stack, &mut root, Node::Element(element), part)?;
            }
            Event::End(_) => {
                let done = stack.pop().ok_or_else(|| Error::Xml {
                    part: part.to_owned(),
                    detail: "an end tag with nothing open".to_owned(),
                })?;
                push_node(&mut stack, &mut root, Node::Element(done), part)?;
            }
            Event::Text(ref t) => {
                push_text(&mut stack, &t.xml10_content());
            }
            Event::GeneralRef(ref r) => {
                let text = reference(r, part)?;
                push_text(&mut stack, &text);
            }
            Event::CData(ref c) => {
                let node = Node::CData(c.xml10_content().into_owned());
                push_node(&mut stack, &mut root, node, part)?;
            }
            Event::Comment(ref c) => {
                let node = Node::Comment(c.xml10_content().into_owned());
                push_node(&mut stack, &mut root, node, part)?;
            }
            Event::PI(p) => {
                // The target and its content together, as written, so that the
                // writer can put back the whitespace between them.
                let node = Node::ProcessingInstruction(p.into_inner().into_owned());
                push_node(&mut stack, &mut root, node, part)?;
            }
            // The declaration is not kept: the writer emits the one every part
            // of an OpenDocument package carries. A document type declaration
            // is not kept either, because ODF defines none and writing back
            // something a reader might resolve entities against would be the
            // one place this tree stopped being a faithful copy.
            Event::Decl(_) | Event::DocType(_) => {}
            Event::Eof => break,
        }
    }

    root.ok_or(Error::Xml {
        part: part.to_owned(),
        detail: "no root element".to_owned(),
    })
}

/// The characters an entity or character reference stands for.
///
/// quick-xml reports `&amp;` and `&#10;` as events of their own rather than
/// unescaping them into the surrounding text, so resolving them is this reader's
/// job. ODF declares no document type and so has no entities beyond the five XML
/// defines; a reference to anything else is given back as the characters that
/// spelled it, which is both lossless and visible.
///
/// # Errors
///
/// A character reference names no character: `&#0;`, or a number past Unicode.
fn reference(r: &quick_xml::events::BytesRef<'_>, part: &str) -> Result<String, Error> {
    let name = r.xml10_content();
    let resolved = if r.is_char_ref() {
        r.resolve_char_ref().map_err(|e| Error::Xml {
            part: part.to_owned(),
            detail: e.to_string(),
        })?
    } else {
        match &*name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => None,
        }
    };
    Ok(match resolved {
        Some(c) => c.to_string(),
        None => format!("&{name};"),
    })
}

/// Text outside the root element — the newline after the declaration — has
/// nowhere to live in a tree with one root, and is dropped. Inside, it is
/// appended to the open element, merged with any text already there so that a
/// run split by an entity reference comes back as one node.
fn push_text(stack: &mut [Element], text: &str) {
    if let Some(open) = stack.last_mut() {
        if let Some(Node::Text(existing)) = open.children.last_mut() {
            existing.push_str(text);
        } else {
            open.children.push(Node::Text(text.to_owned()));
        }
    }
}

fn push_node(
    stack: &mut [Element],
    root: &mut Option<Element>,
    node: Node,
    part: &str,
) -> Result<(), Error> {
    if let Some(open) = stack.last_mut() {
        open.children.push(node);
        return Ok(());
    }
    match node {
        Node::Element(e) if root.is_none() => {
            *root = Some(e);
            Ok(())
        }
        Node::Element(_) => Err(Error::Xml {
            part: part.to_owned(),
            detail: "a second root element".to_owned(),
        }),
        // A comment or a processing instruction beside the root element is
        // legal and carries nothing this crate reads.
        _ => Ok(()),
    }
}

fn element_of(start: &BytesStart<'_>, reader: &NsReader<&[u8]>, self_closing: bool) -> Element {
    let name = resolved_name(reader, start.name(), false);
    let mut attrs = Vec::new();
    for attr in start.attributes() {
        let Ok(attr) = attr else { continue };
        // Normalizing is what the specification asks of an attribute value:
        // entity references resolved, and a literal tab or newline turned into a
        // space. The writer re-escapes the ones that have to survive, which is
        // why `write.rs` spells them as character references.
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .unwrap_or_default()
            .into_owned();
        attrs.push(Attribute {
            name: resolved_name(reader, attr.key, true),
            value,
        });
    }
    Element {
        name,
        attrs,
        children: Vec::new(),
        self_closing,
    }
}

fn resolved_name(reader: &NsReader<&[u8]>, qname: QName<'_>, is_attribute: bool) -> Name {
    let prefix = qname.prefix().map(|p| p.as_ref().into());
    let local: Box<str> = qname.local_name().as_ref().into();

    // A namespace declaration is an attribute whose own prefix is `xmlns`, or
    // whose whole name is. The resolver will not place it in a namespace, and
    // it has to be kept so that writing the tree re-declares what reading it
    // found.
    if prefix.as_deref() == Some("xmlns") {
        return Name {
            prefix,
            local,
            ns: Ns::Xmlns,
        };
    }
    if prefix.is_none() && &*local == "xmlns" {
        return Name {
            prefix: None,
            local,
            ns: Ns::Xmlns,
        };
    }

    let (resolved, _) = if is_attribute {
        reader.resolver().resolve_attribute(qname)
    } else {
        reader.resolver().resolve_element(qname)
    };
    let ns = match resolved {
        ResolveResult::Bound(uri) => Ns::from_uri(uri.as_ref()),
        // An unprefixed attribute is in no namespace by definition, and a
        // prefix with nothing in scope to bind it is a broken document that is
        // still worth showing.
        ResolveResult::Unbound | ResolveResult::Unknown(_) => Ns::None,
    };
    Name { prefix, local, ns }
}
