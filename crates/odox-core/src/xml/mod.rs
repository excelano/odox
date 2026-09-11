//! The document tree: namespace-qualified names, elements, and the parser and
//! writer that turn a part of a package into one and back.
//!
//! This is the substrate the rest of the crate reads through. It is a faithful
//! tree rather than a model: attribute order, the choice between `<a/>` and
//! `<a></a>`, comments, processing instructions and the whitespace between
//! elements all survive a read and a write, so that a part this crate has no
//! opinion about is returned unchanged.
//!
//! Text is held *unescaped*, and escaped again on the way out in the canonical
//! form. Byte identity across a round trip is therefore not claimed; what is
//! claimed, and what `tests/roundtrip.rs` measures, is that parsing the output
//! gives back an equal tree.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

mod read;
mod write;

/// An XML namespace, resolved from the URI it was declared with.
///
/// ODF spells every one of these with a conventional prefix and an invariant
/// URI. Matching is on this rather than on the prefix, because the prefix is the
/// document's choice and the URI is the format's.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ns {
    /// `urn:oasis:names:tc:opendocument:xmlns:office:1.0`
    Office,
    /// `urn:oasis:names:tc:opendocument:xmlns:text:1.0`
    Text,
    /// `urn:oasis:names:tc:opendocument:xmlns:style:1.0`
    Style,
    /// `urn:oasis:names:tc:opendocument:xmlns:table:1.0`
    Table,
    /// `urn:oasis:names:tc:opendocument:xmlns:drawing:1.0`
    Draw,
    /// `urn:oasis:names:tc:opendocument:xmlns:presentation:1.0`
    Presentation,
    /// `urn:oasis:names:tc:opendocument:xmlns:chart:1.0`
    Chart,
    /// `urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0`, which is
    /// where most of the formatting attributes live.
    Fo,
    /// `urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0`
    Svg,
    /// `urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0`
    Number,
    /// `urn:oasis:names:tc:opendocument:xmlns:meta:1.0`
    Meta,
    /// `urn:oasis:names:tc:opendocument:xmlns:manifest:1.0`
    Manifest,
    /// `urn:oasis:names:tc:opendocument:xmlns:config:1.0`
    Config,
    /// `urn:oasis:names:tc:opendocument:xmlns:of:1.2`, the formula namespace.
    Of,
    /// `http://www.w3.org/1999/xlink`
    Xlink,
    /// `http://purl.org/dc/elements/1.1/`
    Dc,
    /// `urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0`
    Loext,
    /// `urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0`
    Calcext,
    /// A namespace declaration: `xmlns:foo="..."`. Held as an attribute so that
    /// writing a tree re-declares exactly what reading it found.
    Xmlns,
    /// A namespace this crate has no name for. The URI is kept so that the
    /// element is written back into the namespace it came from.
    Other(Box<str>),
    /// No namespace: an unprefixed name with no default declaration in scope.
    None,
}

impl Ns {
    /// The namespace a URI denotes.
    pub fn from_uri(uri: &str) -> Self {
        match uri {
            "urn:oasis:names:tc:opendocument:xmlns:office:1.0" => Self::Office,
            "urn:oasis:names:tc:opendocument:xmlns:text:1.0" => Self::Text,
            "urn:oasis:names:tc:opendocument:xmlns:style:1.0" => Self::Style,
            "urn:oasis:names:tc:opendocument:xmlns:table:1.0" => Self::Table,
            "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" => Self::Draw,
            "urn:oasis:names:tc:opendocument:xmlns:presentation:1.0" => Self::Presentation,
            "urn:oasis:names:tc:opendocument:xmlns:chart:1.0" => Self::Chart,
            "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" => Self::Fo,
            "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" => Self::Svg,
            "urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0" => Self::Number,
            "urn:oasis:names:tc:opendocument:xmlns:meta:1.0" => Self::Meta,
            "urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" => Self::Manifest,
            "urn:oasis:names:tc:opendocument:xmlns:config:1.0" => Self::Config,
            "urn:oasis:names:tc:opendocument:xmlns:of:1.2" => Self::Of,
            "http://www.w3.org/1999/xlink" => Self::Xlink,
            "http://purl.org/dc/elements/1.1/" => Self::Dc,
            "urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0" => Self::Loext,
            "urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0" => Self::Calcext,
            other => Self::Other(other.into()),
        }
    }
}

/// A name as it appeared, and the namespace it resolved to.
///
/// The prefix is kept alongside the resolved namespace because writing is a
/// faithful re-emission: a document that spells the text namespace `t:` rather
/// than `text:` is written back its own way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name {
    /// The prefix as written, without the colon. `None` for an unprefixed name.
    pub prefix: Option<Box<str>>,
    /// The part after the colon.
    pub local: Box<str>,
    /// Where the prefix pointed.
    pub ns: Ns,
}

impl Name {
    /// Whether this is the given ODF name.
    pub fn is(&self, ns: &Ns, local: &str) -> bool {
        self.ns == *ns && &*self.local == local
    }
}

/// An attribute and its value, with the value unescaped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    /// The attribute's name.
    pub name: Name,
    /// Its value, unescaped.
    pub value: String,
}

/// One node of a parsed part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// An element and everything under it.
    Element(Element),
    /// Character data, unescaped. Whitespace is significant inside a paragraph
    /// and is never trimmed here.
    Text(String),
    /// `<![CDATA[...]]>`, kept as its own kind so that it is written back as one.
    CData(String),
    /// `<!-- ... -->`.
    Comment(String),
    /// `<?target content?>`, excluding the XML declaration.
    ProcessingInstruction(String),
}

/// An element: its name, its attributes in the order they were written, and its
/// children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    /// The element's name.
    pub name: Name,
    /// Attributes in document order, including any namespace declarations.
    pub attrs: Vec<Attribute>,
    /// Children in document order.
    pub children: Vec<Node>,
    /// True when the element was written `<a/>`. Only consulted when there are
    /// no children, and exists so that a round trip does not rewrite every
    /// empty element in the document.
    pub self_closing: bool,
}

impl Element {
    /// A new element with no attributes and no children.
    pub fn new(prefix: &str, local: &str, ns: Ns) -> Self {
        Self {
            name: Name {
                prefix: if prefix.is_empty() {
                    None
                } else {
                    Some(prefix.into())
                },
                local: local.into(),
                ns,
            },
            attrs: Vec::new(),
            children: Vec::new(),
            self_closing: true,
        }
    }

    /// Whether this element has the given ODF name.
    pub fn is(&self, ns: &Ns, local: &str) -> bool {
        self.name.is(ns, local)
    }

    /// The value of an attribute, by namespace and local name.
    pub fn attr(&self, ns: &Ns, local: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.name.is(ns, local))
            .map(|a| a.value.as_str())
    }

    /// The value of an attribute, read as an integer. Absent, or present and
    /// unreadable, are the same answer: ODF's own default applies either way and
    /// a document that says `table:number-columns-repeated="many"` is not a
    /// reason to refuse the document.
    pub fn attr_usize(&self, ns: &Ns, local: &str) -> Option<usize> {
        self.attr(ns, local)?.trim().parse().ok()
    }

    /// Every child element, in order.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            _ => None,
        })
    }

    /// The first child element with the given name.
    pub fn child(&self, ns: &Ns, local: &str) -> Option<&Element> {
        self.elements().find(|e| e.is(ns, local))
    }

    /// The first descendant element with the given name, depth first.
    pub fn descendant(&self, ns: &Ns, local: &str) -> Option<&Element> {
        for e in self.elements() {
            if e.is(ns, local) {
                return Some(e);
            }
            if let Some(found) = e.descendant(ns, local) {
                return Some(found);
            }
        }
        None
    }

    /// Every character of text under this element, with ODF's own whitespace
    /// elements turned back into the characters they stand for.
    ///
    /// `text:s` is a run of spaces, `text:tab` a tab and `text:line-break` a
    /// newline, because ODF collapses literal runs of whitespace and spells out
    /// what it means instead. This is what a spreadsheet cell's displayed string
    /// and a search over a document both read.
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        self.write_plain_text(&mut out);
        out
    }

    fn write_plain_text(&self, out: &mut String) {
        for child in &self.children {
            match child {
                Node::Text(t) | Node::CData(t) => out.push_str(t),
                Node::Element(e) if e.is(&Ns::Text, "s") => {
                    let n = e.attr_usize(&Ns::Text, "c").unwrap_or(1);
                    for _ in 0..n {
                        out.push(' ');
                    }
                }
                Node::Element(e) if e.is(&Ns::Text, "tab") => out.push('\t'),
                Node::Element(e) if e.is(&Ns::Text, "line-break") => out.push('\n'),
                // A note's body is not part of the text it hangs off, and
                // neither is the deleted half of a tracked change.
                Node::Element(e) if e.is(&Ns::Text, "note") => {}
                Node::Element(e) => e.write_plain_text(out),
                Node::Comment(_) | Node::ProcessingInstruction(_) => {}
            }
        }
    }
}

pub use read::parse;
pub use write::serialize;
