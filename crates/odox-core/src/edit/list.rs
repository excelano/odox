//! A list item's level, and where a numbered list's count starts: Tab and
//! Shift+Tab, and restarting or continuing the numbering. DESIGN.md §11.
//!
//! An item a level in is an item of a `text:list` inside the item before it,
//! as `LibreOffice` writes one; an item with no item before it is put inside
//! an item of its own that holds no paragraph. A restart is the item's
//! `text:start-value`, and a list that carries on from the one before says
//! `text:continue-numbering`.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::block::declares;
use super::format::element;
use super::{Refused, is_paragraph};
use crate::xml::{Element, Node, Ns};

/// The deepest level ODF numbers a list item at.
const DEEPEST: usize = 10;

/// Move the items holding paragraphs a level in (`delta` above zero) or out,
/// each with the items inside it. An item already at the tenth level, or at
/// the first on the way out, stays where it is.
///
/// # Errors
///
/// A path does not lead to a paragraph, the content root does not declare the
/// text namespace, or an item to lift out sits in an item that goes on after
/// its list, which would put the item after text that follows it. Each is
/// refused before anything changes.
pub fn list_level(content: &mut Element, paths: &[Vec<usize>], delta: i8) -> Result<(), Refused> {
    for path in paths {
        content
            .at(path)
            .filter(|e| is_paragraph(e))
            .ok_or(Refused::NotFound)?;
    }
    declares(content, &[Ns::Text])?;
    let mut items: Vec<Vec<usize>> = paths
        .iter()
        .filter_map(|path| item_of(content, path))
        .collect();
    items.sort();
    items.dedup();
    let outermost: Vec<&Vec<usize>> = items
        .iter()
        .filter(|item| {
            !items
                .iter()
                .any(|other| other.len() < item.len() && item.starts_with(other))
        })
        .collect();
    if delta < 0
        && outermost
            .iter()
            .any(|item| lifts_out_of_order(content, item))
    {
        return Err(Refused::Structure);
    }
    let order = paragraphs(content);
    let mut ordinals: Vec<usize> = outermost
        .iter()
        .filter_map(|item| order.iter().position(|p| p.starts_with(item)))
        .collect();
    ordinals.sort_unstable();
    for ordinal in ordinals {
        let Some(item) = paragraphs(content)
            .get(ordinal)
            .and_then(|path| item_of(content, path))
        else {
            continue;
        };
        if delta > 0 {
            indent(content, &item);
        } else if delta < 0 {
            outdent(content, &item);
        }
    }
    Ok(())
}

/// The paths of the paragraphs under an element, in document order.
fn paragraphs(root: &Element) -> Vec<Vec<usize>> {
    fn walk(element: &Element, path: &mut Vec<usize>, into: &mut Vec<Vec<usize>>) {
        for (index, child) in element.elements_indexed() {
            path.push(index);
            if is_paragraph(child) {
                into.push(path.clone());
            } else {
                walk(child, path, into);
            }
            path.pop();
        }
    }
    let mut into = Vec::new();
    walk(root, &mut Vec::new(), &mut into);
    into
}

fn is_item(element: &Element) -> bool {
    element.is(&Ns::Text, "list-item") || element.is(&Ns::Text, "list-header")
}

/// The item a paragraph is held in, where it is in one.
fn item_of(content: &Element, paragraph: &[usize]) -> Option<Vec<usize>> {
    let (_, item) = paragraph.split_last()?;
    is_item(content.at(item)?).then(|| item.to_vec())
}

/// How many lists an item is inside.
fn level(content: &Element, item: &[usize]) -> usize {
    (0..item.len())
        .filter(|&depth| {
            content
                .at(&item[..depth])
                .is_some_and(|e| e.is(&Ns::Text, "list"))
        })
        .count()
}

/// A new, empty element of a name the content root spells.
fn fresh(content: &Element, local: &str) -> Element {
    let mut made = element(content.name_for(&Ns::Text, local));
    made.self_closing = false;
    made
}

/// Put nodes into an item's own list of items a level in: the list it ends
/// with where it has one, and otherwise `list`, made for the purpose.
fn append_to_sublist(item: &mut Element, mut list: Element, nodes: Vec<Node>) {
    if let Some(Node::Element(last)) = item
        .children
        .iter_mut()
        .rev()
        .find(|n| matches!(n, Node::Element(_)))
        && last.is(&Ns::Text, "list")
    {
        last.children.extend(nodes);
        last.self_closing = false;
        return;
    }
    list.children = nodes;
    item.children.push(Node::Element(list));
    item.self_closing = false;
}

/// An item a level in: into the item before it, or into an item of its own
/// where there is none before it.
fn indent(content: &mut Element, item: &[usize]) {
    if level(content, item) >= DEEPEST {
        return;
    }
    let Some((&at, list_path)) = item.split_last() else {
        return;
    };
    let empty = fresh(content, "list-item");
    let list = fresh(content, "list");
    let Some(holder) = content.at_mut(list_path) else {
        return;
    };
    let before = holder.children[..at]
        .iter()
        .rposition(|n| matches!(n, Node::Element(e) if is_item(e)));
    let moved = holder.children.remove(at);
    if let Some(Node::Element(previous)) = before.map(|before| &mut holder.children[before]) {
        append_to_sublist(previous, list, vec![moved]);
    } else {
        let mut wrapper = empty;
        append_to_sublist(&mut wrapper, list, vec![moved]);
        holder.children.insert(at, Node::Element(wrapper));
    }
}

/// Whether lifting an item out would put it after text that comes after it:
/// the item whose list it is in goes on, past that list, with more.
fn lifts_out_of_order(content: &Element, item: &[usize]) -> bool {
    let [.., list_at, _] = *item else {
        return false;
    };
    let owner = &item[..item.len() - 2];
    content.at(owner).is_some_and(|owner| {
        is_item(owner)
            && owner.children[list_at + 1..]
                .iter()
                .any(|n| matches!(n, Node::Element(_)))
    })
}

/// An item a level out, after the item whose list it was in, taking the items
/// after it in that list with it as its own.
fn outdent(content: &mut Element, item: &[usize]) {
    let [.., owner_at, list_at, at] = *item else {
        return;
    };
    let outer = &item[..item.len() - 3];
    let owner = &item[..item.len() - 2];
    let list = &item[..item.len() - 1];
    let fits = content.at(outer).is_some_and(|e| e.is(&Ns::Text, "list"))
        && content.at(owner).is_some_and(is_item)
        && content.at(list).is_some_and(|e| e.is(&Ns::Text, "list"));
    if !fits {
        return;
    }
    let sublist = fresh(content, "list");
    let Some(inner) = content.at_mut(list) else {
        return;
    };
    let mut taken: Vec<Node> = inner.children.drain(at..).collect();
    let Node::Element(mut lifted) = taken.remove(0) else {
        return;
    };
    let followers = taken;
    let emptied = !inner
        .children
        .iter()
        .any(|n| matches!(n, Node::Element(e) if is_item(e)));
    if followers
        .iter()
        .any(|n| matches!(n, Node::Element(e) if is_item(e)))
    {
        append_to_sublist(&mut lifted, sublist, followers);
    }
    let Some(owner_element) = content.at_mut(owner) else {
        return;
    };
    if emptied {
        owner_element.children.remove(list_at);
    }
    let owner_gone = !owner_element
        .children
        .iter()
        .any(|n| matches!(n, Node::Element(_)));
    let Some(outer_list) = content.at_mut(outer) else {
        return;
    };
    if owner_gone {
        outer_list.children[owner_at] = Node::Element(lifted);
    } else {
        outer_list
            .children
            .insert(owner_at + 1, Node::Element(lifted));
    }
}

/// Start the numbering again at one at the item holding a paragraph, as
/// `LibreOffice`'s Restart Numbering writes it.
///
/// # Errors
///
/// The path does not lead to a paragraph in a list, or leads to one in a
/// list's header, which is not numbered.
pub fn restart_numbering(content: &mut Element, path: &[usize]) -> Result<(), Refused> {
    let item = item_of(content, path).ok_or(Refused::NotFound)?;
    if !content
        .at(&item)
        .is_some_and(|e| e.is(&Ns::Text, "list-item"))
    {
        return Err(Refused::Structure);
    }
    let start = content.name_for(&Ns::Text, "start-value");
    let item = content.at_mut(&item).ok_or(Refused::NotFound)?;
    item.set_attr(start, "1");
    Ok(())
}

/// Carry the numbering of the list holding a paragraph on from the list
/// before it, taking back a restart at its item or at the list's first.
///
/// # Errors
///
/// The path does not lead to a paragraph in a list.
pub fn continue_numbering(content: &mut Element, path: &[usize]) -> Result<(), Refused> {
    let item = item_of(content, path).ok_or(Refused::NotFound)?;
    let outermost = (0..item.len())
        .find(|&depth| {
            content
                .at(&item[..depth])
                .is_some_and(|e| e.is(&Ns::Text, "list"))
        })
        .ok_or(Refused::NotFound)?;
    let continues = content.name_for(&Ns::Text, "continue-numbering");
    let list = content
        .at_mut(&item[..outermost])
        .ok_or(Refused::NotFound)?;
    list.set_attr(continues, "true");
    if let Some(Node::Element(first)) = list
        .children
        .iter_mut()
        .find(|n| matches!(n, Node::Element(e) if is_item(e)))
    {
        first.remove_attr(&Ns::Text, "start-value");
    }
    if let Some(own) = content.at_mut(&item) {
        own.remove_attr(&Ns::Text, "start-value");
    }
    Ok(())
}
