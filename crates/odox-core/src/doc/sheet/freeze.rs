//! Rows and columns that stay where they are while the rest of a sheet
//! scrolls.
//!
//! ODF has no element for a freeze: it is a view setting, which every
//! producer writes the way `LibreOffice` does, into `settings.xml` under the
//! first view, one entry per sheet by name. `VerticalSplitMode` 2 freezes the
//! rows above `VerticalSplitPosition`, and `HorizontalSplitMode` 2 the
//! columns left of `HorizontalSplitPosition`; the `Position…` items say where
//! the scrolled part begins, which for a freeze is where it ends.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::SheetDocument;
use crate::edit::Refused;
use crate::xml::{self, Element, Node, Ns};

/// The part the settings live in.
const SETTINGS: &str = "settings.xml";

/// How many rows and columns of a sheet are frozen, from the top and the
/// left.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Frozen {
    /// The rows above the freeze.
    pub rows: usize,
    /// The columns left of it.
    pub columns: usize,
}

impl SheetDocument {
    /// What a sheet keeps frozen, by the sheet's name.
    pub(super) fn read_frozen(settings: Option<&Element>, name: &str) -> Frozen {
        let Some(entry) = settings.and_then(|root| sheet_entry(root, name)) else {
            return Frozen::default();
        };
        let item = |local: &str| {
            entry
                .elements()
                .find(|e| {
                    e.is(&Ns::Config, "config-item") && e.attr(&Ns::Config, "name") == Some(local)
                })
                .map(item_text)
                .and_then(|text| text.trim().parse::<usize>().ok())
                .unwrap_or(0)
        };
        Frozen {
            rows: if item("VerticalSplitMode") == 2 {
                item("VerticalSplitPosition")
            } else {
                0
            },
            columns: if item("HorizontalSplitMode") == 2 {
                item("HorizontalSplitPosition")
            } else {
                0
            },
        }
    }

    /// Freeze a sheet's top rows and left columns, or with nothing frozen
    /// unfreeze it. The settings are written into the package, where a save
    /// takes them, and are not part of the content an undo goes back over:
    /// a freeze is how the sheet is looked at, as the zoom is.
    ///
    /// # Errors
    ///
    /// The sheet does not exist, or `settings.xml` is there and cannot be
    /// read. Nothing is changed.
    pub fn set_frozen(&mut self, sheet: usize, frozen: Frozen) -> Result<(), Refused> {
        let name = self
            .sheets
            .get(sheet)
            .ok_or(Refused::NotFound)?
            .name
            .clone();
        let package = &mut self.document.package;
        let had = package.part(SETTINGS).is_some();
        let mut root = package
            .optional_xml(SETTINGS)
            .map_err(|_| Refused::Structure)?
            .unwrap_or_else(|| {
                let mut root = Element::new("office", "document-settings", Ns::Office);
                root.declare(&Ns::Office);
                root.set_attr(root.name_for(&Ns::Office, "version"), "1.3");
                root
            });
        if !root.declare(&Ns::Config) {
            return Err(Refused::Namespace);
        }
        let entry = reach_sheet_entry(&mut root, &name);
        let (rows_mode, columns_mode) = (
            if frozen.rows > 0 { "2" } else { "0" },
            if frozen.columns > 0 { "2" } else { "0" },
        );
        for (local, kind, value) in [
            ("VerticalSplitMode", "short", rows_mode.to_owned()),
            ("HorizontalSplitMode", "short", columns_mode.to_owned()),
            ("VerticalSplitPosition", "int", frozen.rows.to_string()),
            ("HorizontalSplitPosition", "int", frozen.columns.to_string()),
            ("PositionBottom", "int", frozen.rows.to_string()),
            ("PositionRight", "int", frozen.columns.to_string()),
        ] {
            set_item(entry, local, kind, &value);
        }
        package.set_xml(SETTINGS, &root);
        if !had {
            package.list(SETTINGS, "text/xml");
        }
        self.reindex();
        Ok(())
    }
}

/// The first view's entry for a sheet, by name.
fn sheet_entry<'a>(root: &'a Element, name: &str) -> Option<&'a Element> {
    root.child(&Ns::Office, "settings")?
        .elements()
        .find(|e| named(e, "config-item-set", "ooo:view-settings"))?
        .elements()
        .find(|e| named(e, "config-item-map-indexed", "Views"))?
        .child(&Ns::Config, "config-item-map-entry")?
        .elements()
        .find(|e| named(e, "config-item-map-named", "Tables"))?
        .elements()
        .find(|e| named(e, "config-item-map-entry", name))
}

/// The same entry, written where it is missing along with whatever holds it.
fn reach_sheet_entry<'a>(root: &'a mut Element, name: &str) -> &'a mut Element {
    let settings = reach(root, &Ns::Office, "settings", None);
    let view = reach(
        settings,
        &Ns::Config,
        "config-item-set",
        Some("ooo:view-settings"),
    );
    let views = reach(view, &Ns::Config, "config-item-map-indexed", Some("Views"));
    let first = reach(views, &Ns::Config, "config-item-map-entry", None);
    let tables = reach(first, &Ns::Config, "config-item-map-named", Some("Tables"));
    reach(tables, &Ns::Config, "config-item-map-entry", Some(name))
}

/// A child by its element name and, where given, its `config:name`, added at
/// the end where there is none.
fn reach<'a>(parent: &'a mut Element, ns: &Ns, local: &str, name: Option<&str>) -> &'a mut Element {
    let found = parent.children.iter().position(|n| {
        matches!(n, Node::Element(e) if e.is(ns, local)
            && name.is_none_or(|name| e.attr(&Ns::Config, "name") == Some(name)))
    });
    let at = found.unwrap_or_else(|| {
        let mut child = Element {
            name: parent.name_for(ns, local),
            attrs: Vec::new(),
            children: Vec::new(),
            self_closing: true,
        };
        if let Some(name) = name {
            child.set_attr(parent.name_for(&Ns::Config, "name"), name);
        }
        parent.children.push(Node::Element(child));
        parent.self_closing = false;
        parent.children.len() - 1
    });
    match &mut parent.children[at] {
        Node::Element(child) => child,
        _ => unreachable!("found or written as an element"),
    }
}

/// Give a settings entry's item a value, writing the item where it is
/// missing.
fn set_item(entry: &mut Element, local: &str, kind: &str, value: &str) {
    let item = reach(entry, &Ns::Config, "config-item", Some(local));
    if item.attr(&Ns::Config, "type").is_none() {
        let type_name = item.name_for(&Ns::Config, "type");
        item.set_attr(type_name, kind);
    }
    item.children = vec![Node::Text(value.to_owned())];
    item.self_closing = false;
}

fn named(element: &Element, local: &str, name: &str) -> bool {
    element.is(&Ns::Config, local) && element.attr(&Ns::Config, "name") == Some(name)
}

fn item_text(item: &Element) -> String {
    item.children
        .iter()
        .filter_map(|n| match n {
            Node::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// The settings part of a package, parsed, where it has one that reads.
pub(super) fn settings_of(package: &crate::package::Package) -> Option<Element> {
    package
        .part(SETTINGS)
        .and_then(|part| xml::parse(&part.data, SETTINGS).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(file: &str) -> SheetDocument {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/libreoffice")
                .join(file),
        )
        .expect("the corpus sheet");
        SheetDocument::read(&bytes).expect("it reads")
    }

    #[test]
    fn a_freeze_is_written_read_back_and_survives_a_save() {
        let mut document = sheet("calc.ods");
        assert_eq!(document.sheets()[0].frozen, Frozen::default());
        let frozen = Frozen {
            rows: 1,
            columns: 2,
        };
        document.set_frozen(0, frozen).expect("frozen");
        assert_eq!(document.sheets()[0].frozen, frozen);
        let bytes = document.document.write().expect("written");
        let again = SheetDocument::read(&bytes).expect("read again");
        assert_eq!(again.sheets()[0].frozen, frozen);
    }

    #[test]
    fn unfreezing_leaves_nothing_frozen() {
        let mut document = sheet("calc.ods");
        document
            .set_frozen(
                0,
                Frozen {
                    rows: 3,
                    columns: 0,
                },
            )
            .expect("frozen");
        document.set_frozen(0, Frozen::default()).expect("unfrozen");
        assert_eq!(document.sheets()[0].frozen, Frozen::default());
    }
}
