//! A column's width and a row's height.
//!
//! A size is a property of the column's or the row's style, and a style is
//! shared by every column or row that names it, so a resize gives the one
//! column or row a style of its own: the one it had with the size changed,
//! named as an automatic style the document already holds wherever one says
//! the same. A column or row inside a repeated run is split out of it first.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::restructure::columns_of;
use super::{CellNames, SheetDocument, is_row_container, reach_row, split_run};
use crate::edit::{Refused, automatic_styles, element, name_for};
use crate::value::Length;
use crate::xml::{Element, Node, Ns};
use crate::{Document, Family};

/// Which of the two a size is for.
#[derive(Clone, Copy)]
enum Axis {
    Column,
    Row,
}

impl Axis {
    fn family(self) -> (Family, &'static str, &'static str, &'static str) {
        match self {
            Self::Column => (
                Family::TableColumn,
                "table-column",
                "table-column-properties",
                "co",
            ),
            Self::Row => (Family::TableRow, "table-row", "table-row-properties", "ro"),
        }
    }
}

impl SheetDocument {
    /// Give a column a width.
    ///
    /// # Errors
    ///
    /// The sheet does not exist, or the content does not declare the style
    /// namespace a width is written in. Nothing is changed.
    pub fn set_column_width(
        &mut self,
        sheet: usize,
        column: usize,
        width: Length,
    ) -> Result<(), Refused> {
        let table_at = self.sheets.get(sheet).ok_or(Refused::NotFound)?.table;
        if !self.document.declares(&Ns::Style) {
            return Err(Refused::Namespace);
        }
        let repeated = self.document.name(&Ns::Table, "number-columns-repeated");
        let column_name = self.document.name(&Ns::Table, "table-column");
        let table = table_mut(&mut self.document, table_at)?;
        let defs = columns_of(table);
        let path = if let Some((steps, first, repeat)) = defs
            .iter()
            .find(|(_, first, repeat)| (*first..first + repeat).contains(&column))
        {
            let (last, above) = steps.split_last().ok_or(Refused::NotFound)?;
            let parent = table.at_mut(above).ok_or(Refused::NotFound)?;
            let at = split_run(parent, *last, column - first, *repeat, &repeated);
            [above, &[at]].concat()
        } else {
            // Past the columns the table defines: the gap is a run of
            // plain columns and the column comes after it.
            let written = defs.last().map_or(0, |(_, first, repeat)| first + repeat);
            let (parent_path, mut at) = if let Some((steps, ..)) = defs.last() {
                let (last, above) = steps.split_last().ok_or(Refused::NotFound)?;
                (above.to_vec(), last + 1)
            } else {
                let first_row = table.children.iter().position(|n| {
                        matches!(n, Node::Element(e) if is_row_container(e) || e.is(&Ns::Table, "table-row"))
                    });
                (Vec::new(), first_row.unwrap_or(table.children.len()))
            };
            let parent = table.at_mut(&parent_path).ok_or(Refused::NotFound)?;
            let gap = column - written;
            if gap > 0 {
                let mut filler = element(column_name.clone());
                if gap > 1 {
                    filler.set_attr(repeated.clone(), gap.to_string());
                }
                parent.children.insert(at, Node::Element(filler));
                at += 1;
            }
            parent
                .children
                .insert(at, Node::Element(element(column_name)));
            parent.self_closing = false;
            [parent_path, vec![at]].concat()
        };
        self.restyle(table_at, &path, Axis::Column, width)?;
        self.reindex();
        Ok(())
    }

    /// Give a row a height, which a spreadsheet then keeps rather than
    /// fitting the row to its text.
    ///
    /// # Errors
    ///
    /// The sheet does not exist, or the content does not declare the style
    /// namespace a height is written in. Nothing is changed.
    pub fn set_row_height(
        &mut self,
        sheet: usize,
        row: usize,
        height: Length,
    ) -> Result<(), Refused> {
        let index = self.sheets.get(sheet).ok_or(Refused::NotFound)?;
        let table_at = index.table;
        let range = index
            .row_range(row)
            .map(|r| (r.path.steps().to_vec(), r.first, r.count));
        let rows_written = index.rows.last().map_or(0, |r| r.first + r.count);
        if !self.document.declares(&Ns::Style) {
            return Err(Refused::Namespace);
        }
        let names = CellNames::of(&self.document);
        let table = table_mut(&mut self.document, table_at)?;
        reach_row(table, range.as_ref(), row, rows_written, &names)?;
        // Where the row now is: found again rather than carried out of the
        // split, which borrows the table.
        let path = row_path(table, row).ok_or(Refused::NotFound)?;
        self.restyle(table_at, &path, Axis::Row, height)?;
        self.reindex();
        Ok(())
    }

    /// Give the column or row at `path` under a table a style that is its
    /// own with the size changed.
    fn restyle(
        &mut self,
        table_at: usize,
        path: &[usize],
        axis: Axis,
        size: Length,
    ) -> Result<(), Refused> {
        let (family, family_name, properties_name, prefix) = axis.family();
        let Document {
            content, styles, ..
        } = &mut self.document;
        let current = table_mut_in(content, table_at)?
            .at(path)
            .and_then(|e| e.attr(&Ns::Table, "style-name"))
            .map(ToOwned::to_owned);
        let automatic = current.as_deref().and_then(|name| {
            content
                .child(&Ns::Office, "automatic-styles")?
                .elements()
                .find(|s| {
                    s.is(&Ns::Style, "style")
                        && s.attr(&Ns::Style, "family") == Some(family_name)
                        && s.attr(&Ns::Style, "name") == Some(name)
                })
                .cloned()
        });
        let mut style = if let Some(mut copy) = automatic {
            copy.remove_attr(&Ns::Style, "name");
            copy
        } else {
            let mut style = element(content.name_for(&Ns::Style, "style"));
            style.set_attr(content.name_for(&Ns::Style, "family"), family_name);
            style
        };
        let at = if let Some(at) = style
            .children
            .iter()
            .position(|n| matches!(n, Node::Element(e) if e.is(&Ns::Style, properties_name)))
        {
            at
        } else {
            style.children.push(Node::Element(element(
                content.name_for(&Ns::Style, properties_name),
            )));
            style.self_closing = false;
            style.children.len() - 1
        };
        if let Node::Element(properties) = &mut style.children[at] {
            let written = format!("{:.4}in", size.points() / 72.0);
            match axis {
                Axis::Column => {
                    properties.set_attr(content.name_for(&Ns::Style, "column-width"), written);
                }
                Axis::Row => {
                    properties.set_attr(content.name_for(&Ns::Style, "row-height"), written);
                    properties.set_attr(
                        content.name_for(&Ns::Style, "use-optimal-row-height"),
                        "false",
                    );
                }
            }
        }
        let (name, fresh) = name_for(content, styles, style, &family, prefix);
        let name_attr = content.name_for(&Ns::Table, "style-name");
        table_mut_in(content, table_at)?
            .at_mut(path)
            .ok_or(Refused::NotFound)?
            .set_attr(name_attr, name);
        if let Some(fresh) = fresh {
            let container = automatic_styles(content);
            container.children.push(Node::Element(fresh));
            container.self_closing = false;
        }
        Ok(())
    }
}

fn table_mut(document: &mut Document, table_at: usize) -> Result<&mut Element, Refused> {
    table_mut_in(&mut document.content, table_at)
}

fn table_mut_in(content: &mut Element, table_at: usize) -> Result<&mut Element, Refused> {
    content
        .child_mut(&Ns::Office, "body")
        .and_then(|body| body.child_mut(&Ns::Office, "spreadsheet"))
        .and_then(|sheet| sheet.at_mut(&[table_at]))
        .ok_or(Refused::NotFound)
}

/// The path under a table of the row element that holds one row alone.
fn row_path(table: &Element, row: usize) -> Option<Vec<usize>> {
    fn walk(parent: &Element, path: &mut Vec<usize>, at: &mut usize, row: usize) -> bool {
        for (index, child) in parent.elements_indexed() {
            path.push(index);
            if child.is(&Ns::Table, "table-row") {
                let repeat = child
                    .attr_usize(&Ns::Table, "number-rows-repeated")
                    .unwrap_or(1)
                    .max(1);
                if (*at..*at + repeat).contains(&row) {
                    return true;
                }
                *at += repeat;
            } else if is_row_container(child) && walk(child, path, at, row) {
                return true;
            }
            path.pop();
        }
        false
    }
    let mut path = Vec::new();
    walk(table, &mut path, &mut 0, row).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> SheetDocument {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/libreoffice/calc.ods"),
        )
        .expect("the corpus sheet");
        SheetDocument::read(&bytes).expect("it reads")
    }

    #[test]
    fn a_column_inside_a_repeated_run_is_split_out_and_given_its_width() {
        let mut document = sheet();
        let before: Vec<_> = (0..10)
            .map(|c| document.sheets()[0].column_width(c))
            .collect();
        document
            .set_column_width(0, 3, Length(100.0))
            .expect("resized");
        let sheet = &document.sheets()[0];
        let width = sheet.column_width(3).expect("a width").points();
        assert!((width - 100.0).abs() < 0.01, "{width}");
        for column in (0..10).filter(|&c| c != 3) {
            assert_eq!(
                sheet.column_width(column),
                before[column],
                "column {column}"
            );
        }
    }

    #[test]
    fn a_column_past_the_defined_ones_is_reached() {
        let mut document = sheet();
        document
            .set_column_width(0, 60, Length(30.0))
            .expect("resized");
        let width = document.sheets()[0]
            .column_width(60)
            .expect("a width")
            .points();
        assert!((width - 30.0).abs() < 0.01);
    }

    #[test]
    fn a_row_gets_a_height_and_its_neighbours_keep_theirs() {
        let mut document = sheet();
        let sheet_index = &document.sheets()[0];
        let above = document.row_height(sheet_index, 1);
        let below = document.row_height(sheet_index, 3);
        document
            .set_row_height(0, 2, Length(40.0))
            .expect("resized");
        let sheet = &document.sheets()[0];
        let height = document.row_height(sheet, 2).expect("a height").points();
        assert!((height - 40.0).abs() < 0.01);
        assert_eq!(document.row_height(sheet, 1), above);
        assert_eq!(document.row_height(sheet, 3), below);
        assert_eq!(
            document.cell(sheet, 2, 0).map(|c| c.text()).as_deref(),
            Some("Nut M6"),
            "the row's cells are where they were"
        );
    }

    #[test]
    fn two_columns_given_one_width_share_a_style() {
        let mut document = sheet();
        document
            .set_column_width(0, 1, Length(90.0))
            .expect("resized");
        document
            .set_column_width(0, 4, Length(90.0))
            .expect("resized");
        let automatic = document
            .document
            .content
            .child(&Ns::Office, "automatic-styles")
            .expect("automatic styles");
        let ninety = automatic
            .elements()
            .filter(|s| {
                s.attr(&Ns::Style, "family") == Some("table-column")
                    && s.elements()
                        .any(|p| p.attr(&Ns::Style, "column-width") == Some("1.2500in"))
            })
            .count();
        assert_eq!(ninety, 1);
    }
}
