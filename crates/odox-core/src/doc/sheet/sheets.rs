//! Adding, renaming, deleting and moving a workbook's sheets.
//!
//! A sheet is a `table:table` in the spreadsheet body, and what refers to one
//! refers to it by name: a formula's `[$Sheet1.A1]`, a print range's
//! `Sheet1.A1:Sheet1.D9`, a named range's address. So moving a sheet only
//! reorders the tables. Renaming one renames it in every reference that names
//! it, and is refused where a reference cannot be read or where something
//! holds its references in a form this does not rewrite: a chart, a condition.
//! Deleting one is refused while anything outside it points into it, because a
//! formula left pointing at nothing looks right and nothing here evaluates it.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::SheetDocument;
use super::freeze::SETTINGS;
use super::restructure::Blocked;
use crate::doc::refs::{self, Refusal};
use crate::xml::{Attribute, Element, Node, Ns};

/// What an attribute holds, as far as sheet names go.
enum Holds {
    /// A formula, whose references are in brackets.
    Formula,
    /// Addresses standing alone, separated by spaces.
    Addresses,
    /// References in a form this does not read, under a name a window can put
    /// in a sentence.
    Opaque(&'static str),
}

fn holds(attribute: &Attribute) -> Option<Holds> {
    let name = &attribute.name;
    if name.is(&Ns::Table, "formula") || name.is(&Ns::Table, "expression") {
        return Some(Holds::Formula);
    }
    if name.is(&Ns::Table, "condition") {
        return Some(Holds::Opaque("validations"));
    }
    if name.is(&Ns::Calcext, "value") || name.is(&Ns::Calcext, "formula") {
        return Some(Holds::Opaque("conditional formats"));
    }
    let local = name.local.as_ref();
    (local.ends_with("address")
        || local.ends_with("addresses")
        || local == "print-ranges"
        || local == "notify-on-update-of-ranges")
        .then_some(Holds::Addresses)
}

fn blocked(refusal: Refusal) -> Blocked {
    match refusal {
        Refusal::Unreadable(text) => Blocked::Unreadable(text),
        Refusal::Deleted(text) => Blocked::Deleted(text),
    }
}

/// Rename a sheet in every reference under an element.
fn rename_under(element: &mut Element, from: &str, to: &str) -> Result<(), Blocked> {
    if element.is(&Ns::Draw, "object") || element.is(&Ns::Draw, "object-ole") {
        return Err(Blocked::Names("charts and objects"));
    }
    for attribute in &mut element.attrs {
        match holds(attribute) {
            Some(Holds::Formula) => {
                attribute.value =
                    refs::rename_in_formula(&attribute.value, from, to).map_err(blocked)?;
            }
            Some(Holds::Addresses) => {
                attribute.value =
                    refs::rename_in_addresses(&attribute.value, from, to).map_err(blocked)?;
            }
            Some(Holds::Opaque(what)) if attribute.value.contains(from) => {
                return Err(Blocked::Names(what));
            }
            _ => {}
        }
    }
    for child in &mut element.children {
        if let Node::Element(child) = child {
            rename_under(child, from, to)?;
        }
    }
    Ok(())
}

/// The first reference under an element that names a sheet.
fn naming_under(element: &Element, sheet: &str) -> Result<(), Blocked> {
    for attribute in &element.attrs {
        let found = match holds(attribute) {
            Some(Holds::Formula) => refs::naming_in_formula(&attribute.value, sheet),
            Some(Holds::Addresses) => refs::naming_in_addresses(&attribute.value, sheet),
            Some(Holds::Opaque(what)) if attribute.value.contains(sheet) => {
                return Err(Blocked::Names(what));
            }
            _ => None,
        };
        if let Some(reference) = found {
            return Err(Blocked::Deleted(reference));
        }
    }
    element
        .elements()
        .try_for_each(|child| naming_under(child, sheet))
}

impl SheetDocument {
    /// Whether a name may be given to a sheet: not empty, none of the
    /// characters a reference or a spreadsheet reserves, no quote at either
    /// end, and no other sheet's, `except` being the sheet being renamed.
    ///
    /// # Errors
    ///
    /// [`Blocked::Unnamed`] for a name that may not be one, and
    /// [`Blocked::Taken`] for another sheet's.
    pub fn check_sheet_name(&self, name: &str, except: Option<usize>) -> Result<(), Blocked> {
        if name.trim().is_empty()
            || name.contains(['[', ']', '*', '?', ':', '/', '\\'])
            || name.starts_with('\'')
            || name.ends_with('\'')
        {
            return Err(Blocked::Unnamed);
        }
        let taken = self
            .sheets
            .iter()
            .enumerate()
            .any(|(index, sheet)| Some(index) != except && sheet.name == name);
        if taken {
            return Err(Blocked::Taken);
        }
        Ok(())
    }

    /// Add an empty sheet after another, in that sheet's table style where
    /// the sheet is shown, and answer the new sheet's index.
    ///
    /// # Errors
    ///
    /// The sheet does not exist, or the name may not be given.
    pub fn add_sheet(&mut self, after: usize, name: &str) -> Result<usize, Blocked> {
        let index = self.sheets.get(after).ok_or(Blocked::NotFound)?;
        self.check_sheet_name(name, None)?;
        let (position, shown) = (index.table, index.visible);
        let style = shown
            .then(|| {
                self.table(index)
                    .and_then(|table| table.attr(&Ns::Table, "style-name"))
                    .map(ToOwned::to_owned)
            })
            .flatten();
        let name_of = |local: &str| self.document.name(&Ns::Table, local);
        let mut table = Element {
            name: name_of("table"),
            attrs: Vec::new(),
            children: Vec::new(),
            self_closing: false,
        };
        table.set_attr(name_of("name"), name);
        if let Some(style) = style {
            table.set_attr(name_of("style-name"), style);
        }
        let empty = |local: &str| Element {
            name: name_of(local),
            attrs: Vec::new(),
            children: Vec::new(),
            self_closing: true,
        };
        let mut row = empty("table-row");
        row.children.push(Node::Element(empty("table-cell")));
        row.self_closing = false;
        table.children.push(Node::Element(empty("table-column")));
        table.children.push(Node::Element(row));
        self.spreadsheet_mut()?
            .children
            .insert(position + 1, Node::Element(table));
        self.reindex();
        Ok(after + 1)
    }

    /// Rename a sheet, and every reference that names it.
    ///
    /// Nothing is changed unless all of it can be.
    ///
    /// # Errors
    ///
    /// The sheet does not exist, the name may not be given, or a reference
    /// cannot be renamed.
    pub fn rename_sheet(&mut self, sheet: usize, name: &str) -> Result<(), Blocked> {
        let index = self.sheets.get(sheet).ok_or(Blocked::NotFound)?;
        let (old, position) = (index.name.clone(), index.table);
        if old == name {
            return Ok(());
        }
        self.check_sheet_name(name, Some(sheet))?;
        let mut content = self.document.content.clone();
        let spreadsheet = spreadsheet_in(&mut content).ok_or(Blocked::NotFound)?;
        rename_under(spreadsheet, &old, name)?;
        let name_attr = spreadsheet.name_for(&Ns::Table, "name");
        spreadsheet
            .at_mut(&[position])
            .ok_or(Blocked::NotFound)?
            .set_attr(name_attr, name);
        self.document.content = content;
        self.rename_in_settings(&old, name);
        self.reindex();
        Ok(())
    }

    /// Delete a sheet.
    ///
    /// # Errors
    ///
    /// The sheet does not exist, it is the last one shown, or something
    /// outside it refers into it.
    pub fn delete_sheet(&mut self, sheet: usize) -> Result<(), Blocked> {
        let index = self.sheets.get(sheet).ok_or(Blocked::NotFound)?;
        let (name, position) = (index.name.clone(), index.table);
        let shown_others = self
            .sheets
            .iter()
            .enumerate()
            .any(|(other, s)| other != sheet && s.visible);
        if !shown_others {
            return Err(Blocked::Last);
        }
        let spreadsheet = self
            .document
            .body_of("spreadsheet")
            .ok_or(Blocked::NotFound)?;
        for (at, child) in spreadsheet.elements_indexed() {
            if at != position {
                naming_under(child, &name)?;
            }
        }
        self.spreadsheet_mut()?.children.remove(position);
        self.reindex();
        Ok(())
    }

    /// Move a sheet to where another is, the sheets between moving over by
    /// one.
    ///
    /// # Errors
    ///
    /// Either sheet does not exist.
    pub fn move_sheet(&mut self, sheet: usize, to: usize) -> Result<(), Blocked> {
        let from = self.sheets.get(sheet).ok_or(Blocked::NotFound)?.table;
        let target = self.sheets.get(to).ok_or(Blocked::NotFound)?.table;
        if from == target {
            return Ok(());
        }
        let spreadsheet = self.spreadsheet_mut()?;
        let table = spreadsheet.children.remove(from);
        // Before the target when moving left; after it, which the removal has
        // made the same index, when moving right.
        spreadsheet.children.insert(target, table);
        self.reindex();
        Ok(())
    }

    fn spreadsheet_mut(&mut self) -> Result<&mut Element, Blocked> {
        spreadsheet_in(&mut self.document.content).ok_or(Blocked::NotFound)
    }

    /// Carry a sheet's view settings, its freeze among them, to its new name.
    fn rename_in_settings(&mut self, from: &str, to: &str) {
        let package = &mut self.document.package;
        let Ok(Some(mut root)) = package.optional_xml(SETTINGS) else {
            return;
        };
        rename_settings(&mut root, from, to);
        package.set_xml(SETTINGS, &root);
    }
}

/// A sheet's name in the settings under an element: the entry that holds its
/// view settings, and the active sheet where it is that one.
fn rename_settings(element: &mut Element, from: &str, to: &str) {
    if element.is(&Ns::Config, "config-item-map-entry")
        && element.attr(&Ns::Config, "name") == Some(from)
    {
        let name = element.name_for(&Ns::Config, "name");
        element.set_attr(name, to);
    }
    if element.is(&Ns::Config, "config-item")
        && element.attr(&Ns::Config, "name") == Some("ActiveTable")
    {
        for child in &mut element.children {
            if let Node::Text(text) = child
                && text == from
            {
                to.clone_into(text);
            }
        }
    }
    for child in &mut element.children {
        if let Node::Element(child) = child {
            rename_settings(child, from, to);
        }
    }
}

fn spreadsheet_in(content: &mut Element) -> Option<&mut Element> {
    content
        .child_mut(&Ns::Office, "body")
        .and_then(|body| body.child_mut(&Ns::Office, "spreadsheet"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> SheetDocument {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/libreoffice/export.ods"),
        )
        .expect("the export fixture");
        SheetDocument::read(&bytes).expect("it reads")
    }

    fn names(document: &SheetDocument) -> Vec<&str> {
        document.sheets().iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn a_sheet_is_added_after_another_and_is_empty() {
        let mut document = fixture();
        let before = names(&document).len();
        let at = document.add_sheet(0, "New one").expect("added");
        assert_eq!(at, 1);
        assert_eq!(names(&document)[1], "New one");
        assert_eq!(names(&document).len(), before + 1);
        let sheet = &document.sheets()[1];
        assert_eq!((sheet.used_rows, sheet.used_columns), (0, 0));
        assert!(sheet.visible);
        assert_eq!(document.add_sheet(0, "New one"), Err(Blocked::Taken));
        assert_eq!(document.add_sheet(0, "a:b"), Err(Blocked::Unnamed));
    }

    #[test]
    fn a_rename_follows_the_print_range_and_the_formulas() {
        let mut document = fixture();
        let summary = names(&document)
            .iter()
            .position(|n| *n == "Summary")
            .expect("a summary");
        document.add_sheet(0, "Uses").expect("added");
        let uses = 1;
        let summary = summary + 1;
        document
            .set_cell(uses, 0, 0, &crate::doc::Value::Text("x".to_owned()))
            .expect("written");
        // A formula on another sheet that names the one being renamed.
        let table = document.sheets()[uses].table;
        let cell = spreadsheet_in(&mut document.document.content)
            .and_then(|s| s.at_mut(&[table]))
            .and_then(|t| t.child_mut(&Ns::Table, "table-row"))
            .and_then(|r| r.child_mut(&Ns::Table, "table-cell"))
            .expect("the cell");
        let formula = cell.name_for(&Ns::Table, "formula");
        cell.set_attr(formula, "of:=[$Summary.B2]*2");
        document.reindex();

        document
            .rename_sheet(summary, "Quarter totals")
            .expect("renamed");
        assert_eq!(names(&document)[summary], "Quarter totals");
        let sheet = &document.sheets()[uses];
        assert_eq!(
            document
                .cell(sheet, 0, 0)
                .and_then(|c| c.formula().map(ToOwned::to_owned)),
            Some("[$'Quarter totals'.B2]*2".to_owned())
        );
        let summary_sheet = &document.sheets()[summary];
        assert_eq!(
            document
                .table(summary_sheet)
                .and_then(|t| t.attr(&Ns::Table, "print-ranges")),
            Some("'Quarter totals'.A1:'Quarter totals'.C4")
        );
    }

    #[test]
    fn a_renamed_sheet_keeps_its_freeze() {
        let mut document = fixture();
        let frozen = super::super::Frozen {
            rows: 1,
            columns: 1,
        };
        document.set_frozen(0, frozen).expect("frozen");
        document.rename_sheet(0, "Renamed").expect("renamed");
        assert_eq!(document.sheets()[0].frozen, frozen);
    }

    #[test]
    fn a_sheet_a_formula_points_into_is_not_deleted() {
        let mut document = fixture();
        document.add_sheet(0, "Uses").expect("added");
        let table = document.sheets()[1].table;
        let cell = spreadsheet_in(&mut document.document.content)
            .and_then(|s| s.at_mut(&[table]))
            .and_then(|t| t.child_mut(&Ns::Table, "table-row"))
            .and_then(|r| r.child_mut(&Ns::Table, "table-cell"))
            .expect("the cell");
        let formula = cell.name_for(&Ns::Table, "formula");
        cell.set_attr(formula, "of:=[$Summary.B2]*2");
        document.reindex();
        let summary = names(&document)
            .iter()
            .position(|n| *n == "Summary")
            .expect("a summary");
        assert_eq!(
            document.delete_sheet(summary),
            Err(Blocked::Deleted("$Summary.B2".to_owned()))
        );
        document
            .delete_sheet(1)
            .expect("the one with the formula goes");
        document
            .delete_sheet(summary - 1)
            .expect("and then this one");
        assert!(!names(&document).contains(&"Summary"));
    }

    #[test]
    fn the_last_shown_sheet_stays() {
        let mut document = fixture();
        while document.sheets().iter().filter(|s| s.visible).count() > 1 {
            let at = document
                .sheets()
                .iter()
                .position(|s| s.visible)
                .expect("a shown sheet");
            document.delete_sheet(at).expect("deleted");
        }
        let last = document
            .sheets()
            .iter()
            .position(|s| s.visible)
            .expect("one left");
        assert_eq!(document.delete_sheet(last), Err(Blocked::Last));
    }

    #[test]
    fn a_sheet_moves_both_ways() {
        let mut document = fixture();
        let start: Vec<String> = names(&document).iter().map(|n| (*n).to_owned()).collect();
        let last = start.len() - 1;
        document.move_sheet(0, last).expect("moved right");
        assert_eq!(names(&document)[last], start[0]);
        assert_eq!(names(&document)[0], start[1]);
        document.move_sheet(last, 0).expect("moved back");
        assert_eq!(
            names(&document),
            start.iter().map(String::as_str).collect::<Vec<_>>()
        );
    }
}
