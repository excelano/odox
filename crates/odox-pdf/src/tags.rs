//! The structure tree, built while the document is laid out and handed to
//! krilla once the pages are written.
//!
//! Layout knows what each piece of content is — a heading's line, a list
//! label, a picture — and the order it is read in; only writing a page knows
//! the identifier krilla gives that content. So layout records a content
//! number where each identifier belongs, and the tree is assembled from the
//! numbers once every page has been drawn.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::HashMap;

use krilla::tagging::{Identifier, Node as TagNode, TagGroup, TagKind, TagTree};

/// A group in the tree.
pub(crate) type NodeId = usize;
/// A piece of content on a page, or a link's annotation.
pub(crate) type LeafId = usize;

enum Child {
    Node(NodeId),
    Leaf(LeafId),
}

struct Group {
    kind: TagKind,
    children: Vec<Child>,
}

/// The structure of a document, in reading order.
#[derive(Default)]
pub(crate) struct Tree {
    groups: Vec<Group>,
    roots: Vec<NodeId>,
    leaves: usize,
}

impl Tree {
    /// A new group, under another or at the top.
    pub(crate) fn group(&mut self, parent: Option<NodeId>, kind: impl Into<TagKind>) -> NodeId {
        let id = self.groups.len();
        self.groups.push(Group {
            kind: kind.into(),
            children: Vec::new(),
        });
        match parent {
            Some(parent) => self.groups[parent].children.push(Child::Node(id)),
            None => self.roots.push(id),
        }
        id
    }

    /// A new piece of content, read next in a group.
    pub(crate) fn leaf(&mut self, parent: NodeId) -> LeafId {
        let id = self.leaves;
        self.leaves += 1;
        self.groups[parent].children.push(Child::Leaf(id));
        id
    }

    /// Whether anything drawn is read under a group.
    pub(crate) fn holds_content(&self, id: NodeId) -> bool {
        self.groups[id].children.iter().any(|child| match child {
            Child::Node(node) => self.holds_content(*node),
            Child::Leaf(_) => true,
        })
    }

    /// The tree krilla writes, given the identifier each piece of content was
    /// drawn under. A piece that was never drawn is left out, and so is a
    /// group left with nothing in it.
    pub(crate) fn finish(self, drawn: &HashMap<LeafId, Identifier>, lang: &str) -> TagTree {
        let mut tree = TagTree::new().with_lang(Some(lang.to_owned()));
        for &root in &self.roots {
            if let Some(node) = self.node(root, drawn) {
                tree.push(node);
            }
        }
        tree
    }

    fn node(&self, id: NodeId, drawn: &HashMap<LeafId, Identifier>) -> Option<TagNode> {
        let group = &self.groups[id];
        let children: Vec<TagNode> = group
            .children
            .iter()
            .filter_map(|child| match child {
                Child::Node(node) => self.node(*node, drawn),
                Child::Leaf(leaf) => drawn.get(leaf).map(|id| (*id).into()),
            })
            .collect();
        (!children.is_empty()).then(|| TagGroup::with_children(group.kind.clone(), children).into())
    }
}
