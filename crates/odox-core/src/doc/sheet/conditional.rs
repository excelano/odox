//! Conditional formats: a style a cell takes while a condition on it holds.
//! DESIGN.md §11.
//!
//! `LibreOffice` writes them twice, as a table's `calcext:conditional-formats`
//! and as `style:map` on the cells' styles; the first is read where a table
//! has it, the second otherwise. A condition compares the cell's value, tests
//! its text, or works out a formula whose references are relative to the
//! condition's base cell. One of any other kind never holds.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::cmp::Ordering;
use std::collections::HashMap;

use super::recalc::held;
use super::{Cell, SheetDocument};
use crate::Family;
use crate::formula::{self, Address, Cells, Expr};
use crate::style::Styles;
use crate::xml::{Element, Ns};

/// The conditional formats of one sheet.
#[derive(Debug, Default)]
pub struct ConditionalFormats {
    formats: Vec<Format>,
}

/// One format: the cells it covers and its conditions, the first that holds
/// giving its style.
#[derive(Debug)]
struct Format {
    ranges: Vec<(Address, Address)>,
    conditions: Vec<Condition>,
}

#[derive(Debug)]
struct Condition {
    test: Test,
    style: String,
    base: Address,
}

#[derive(Debug)]
enum Test {
    Compare(&'static str, String),
    Between(String, String, bool),
    Formula(String),
    Text(TextTest, String),
    Never,
}

#[derive(Debug, Clone, Copy)]
enum TextTest {
    Begins,
    Ends,
    Contains,
    Lacks,
}

impl ConditionalFormats {
    /// Read a table's `calcext:conditional-formats`.
    pub fn read(table: &Element) -> Self {
        let Some(list) = table.child(&Ns::Calcext, "conditional-formats") else {
            return Self::default();
        };
        let formats = list
            .elements()
            .filter(|e| e.is(&Ns::Calcext, "conditional-format"))
            .map(|format| Format {
                ranges: ranges(
                    format
                        .attr(&Ns::Calcext, "target-range-address")
                        .unwrap_or(""),
                ),
                conditions: format
                    .elements()
                    .filter(|e| e.is(&Ns::Calcext, "condition"))
                    .filter_map(|condition| {
                        Some(Condition {
                            test: calcext_test(condition.attr(&Ns::Calcext, "value")?),
                            style: condition.attr(&Ns::Calcext, "apply-style-name")?.to_owned(),
                            base: base(condition.attr(&Ns::Calcext, "base-cell-address")),
                        })
                    })
                    .collect(),
            })
            .collect();
        Self { formats }
    }

    /// Whether the sheet has any.
    pub fn is_empty(&self) -> bool {
        self.formats.is_empty()
    }

    /// The style a cell takes: that of the first condition that holds, of the
    /// first format covering it.
    pub fn style_for(
        &self,
        sheet: usize,
        row: usize,
        column: usize,
        value: &formula::Value,
        cells: &dyn Cells,
    ) -> Option<&str> {
        let at = Address { row, column };
        self.formats
            .iter()
            .filter(|format| {
                format.ranges.iter().any(|(from, to)| {
                    (from.row..=to.row).contains(&row)
                        && (from.column..=to.column).contains(&column)
                })
            })
            .find_map(|format| {
                format
                    .conditions
                    .iter()
                    .find(|condition| holds(condition, sheet, at, value, cells))
                    .map(|condition| condition.style.as_str())
            })
    }

    /// The covered cells' rectangles, for a caller walking them.
    fn ranges(&self) -> impl Iterator<Item = &(Address, Address)> {
        self.formats.iter().flat_map(|format| &format.ranges)
    }
}

/// The `style:map` conditions of a cell style.
fn style_maps(styles: &Styles, name: &str) -> Vec<Condition> {
    let Some(style) = styles.style(&Family::TableCell, name) else {
        return Vec::new();
    };
    style
        .element
        .elements()
        .filter(|e| e.is(&Ns::Style, "map"))
        .filter_map(|map| {
            Some(Condition {
                test: map_test(map.attr(&Ns::Style, "condition")?),
                style: map.attr(&Ns::Style, "apply-style-name")?.to_owned(),
                base: base(map.attr(&Ns::Style, "base-cell-address")),
            })
        })
        .collect()
}

/// A `calcext:value`: `>10`, `between(1,5)`, `formula-is(...)`,
/// `begins-with("a")` and the like.
fn calcext_test(value: &str) -> Test {
    let value = value.trim();
    for op in ["<=", ">=", "!=", "=", "<", ">"] {
        if let Some(rest) = value.strip_prefix(op) {
            return Test::Compare(op, rest.to_owned());
        }
    }
    let Some((name, inside)) = call(value) else {
        return Test::Never;
    };
    match name {
        "between" | "not-between" => pair(inside).map_or(Test::Never, |(a, b)| {
            Test::Between(a, b, name == "not-between")
        }),
        "formula-is" => Test::Formula(inside.to_owned()),
        "begins-with" => Test::Text(TextTest::Begins, inside.to_owned()),
        "ends-with" => Test::Text(TextTest::Ends, inside.to_owned()),
        "contains-text" => Test::Text(TextTest::Contains, inside.to_owned()),
        "not-contains-text" => Test::Text(TextTest::Lacks, inside.to_owned()),
        _ => Test::Never,
    }
}

/// A `style:condition`: `cell-content()>10`, `cell-content-is-between(1,5)`,
/// `is-true-formula(...)`.
fn map_test(condition: &str) -> Test {
    let condition = condition.trim();
    if let Some(rest) = condition.strip_prefix("cell-content()") {
        return calcext_test(rest);
    }
    let Some((name, inside)) = call(condition) else {
        return Test::Never;
    };
    match name {
        "cell-content-is-between" | "cell-content-is-not-between" => pair(inside)
            .map_or(Test::Never, |(a, b)| {
                Test::Between(a, b, name == "cell-content-is-not-between")
            }),
        "is-true-formula" => Test::Formula(inside.to_owned()),
        _ => Test::Never,
    }
}

/// `name(inside)`, split.
fn call(text: &str) -> Option<(&str, &str)> {
    let open = text.find('(')?;
    let inside = text.get(open + 1..)?.strip_suffix(')')?;
    Some((&text[..open], inside))
}

/// Two arguments at the comma outside quotes, brackets and parentheses.
fn pair(inside: &str) -> Option<(String, String)> {
    let mut depth = 0i32;
    let mut quoted = false;
    for (at, c) in inside.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '(' | '[' if !quoted => depth += 1,
            ')' | ']' if !quoted => depth -= 1,
            ',' | ';' if !quoted && depth == 0 => {
                return Some((inside[..at].to_owned(), inside[at + 1..].to_owned()));
            }
            _ => {}
        }
    }
    None
}

/// `Sheet1.A1:Sheet1.A4`, and several of them with spaces between.
fn ranges(text: &str) -> Vec<(Address, Address)> {
    text.split_whitespace()
        .filter_map(|range| {
            let (from, to) = range.split_once(':').unwrap_or((range, range));
            Some((cell(from)?, cell(to)?))
        })
        .collect()
}

fn base(text: Option<&str>) -> Address {
    text.and_then(cell).unwrap_or(Address { row: 0, column: 0 })
}

/// `Sheet1.A1`, `$'My sheet'.$B$2`: the cell, whatever sheet it names.
fn cell(text: &str) -> Option<Address> {
    let local = text.rsplit('.').next()?.replace('$', "");
    let split = local.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = local.split_at(split);
    if letters.is_empty() {
        return None;
    }
    let column = letters.chars().try_fold(0usize, |n, c| {
        c.is_ascii_alphabetic()
            .then(|| n * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1))
    })?;
    let row: usize = digits.parse().ok()?;
    Some(Address {
        row: row.checked_sub(1)?,
        column: column - 1,
    })
}

/// An expression of a condition worked out at a cell, its relative references
/// moved there from the condition's base cell.
fn work_out(
    expression: &str,
    base: Address,
    sheet: usize,
    at: Address,
    cells: &dyn Cells,
) -> Option<formula::Value> {
    let offset = |to: usize, from: usize| {
        isize::try_from(to)
            .ok()?
            .checked_sub(isize::try_from(from).ok()?)
    };
    let stored = formula::shifted(
        &format!("of:={expression}"),
        offset(at.row, base.row)?,
        offset(at.column, base.column)?,
    );
    let parsed: Expr = formula::parse(&stored).ok()?;
    formula::evaluate(&parsed, sheet, at, cells).ok()
}

fn holds(
    condition: &Condition,
    sheet: usize,
    at: Address,
    value: &formula::Value,
    cells: &dyn Cells,
) -> bool {
    let worked = |expression: &str| work_out(expression, condition.base, sheet, at, cells);
    match &condition.test {
        Test::Never => false,
        Test::Formula(expression) => match worked(expression) {
            Some(formula::Value::Boolean(b)) => b,
            Some(formula::Value::Number(n)) => n != 0.0,
            _ => false,
        },
        Test::Compare(op, expression) => {
            let Some(other) = worked(expression) else {
                return false;
            };
            compare(value, &other).is_some_and(|order| match *op {
                "=" => order.is_eq(),
                "!=" => order.is_ne(),
                "<" => order.is_lt(),
                ">" => order.is_gt(),
                "<=" => order.is_le(),
                _ => order.is_ge(),
            })
        }
        Test::Between(low, high, outside) => {
            let (Some(low), Some(high)) = (worked(low), worked(high)) else {
                return false;
            };
            let (low, high) = if compare(&low, &high).is_some_and(Ordering::is_gt) {
                (high, low)
            } else {
                (low, high)
            };
            let (Some(from_low), Some(from_high)) = (compare(value, &low), compare(value, &high))
            else {
                return false;
            };
            (from_low.is_ge() && from_high.is_le()) != *outside
        }
        Test::Text(kind, expression) => {
            let Some(formula::Value::Text(wanted)) = worked(expression) else {
                return false;
            };
            let text = match value {
                formula::Value::Text(text) => text.to_lowercase(),
                formula::Value::Number(n) | formula::Value::Date(n) => n.to_string(),
                formula::Value::Boolean(b) => if *b { "TRUE" } else { "FALSE" }.to_lowercase(),
                formula::Value::Empty | formula::Value::Error(_) => return false,
            };
            let wanted = wanted.to_lowercase();
            match kind {
                TextTest::Begins => text.starts_with(&wanted),
                TextTest::Ends => text.ends_with(&wanted),
                TextTest::Contains => text.contains(&wanted),
                TextTest::Lacks => !text.contains(&wanted),
            }
        }
    }
}

/// Two values in order, numbers by number and text by text without regard to
/// case; an empty cell, an error or a number against text has none.
fn compare(a: &formula::Value, b: &formula::Value) -> Option<Ordering> {
    use formula::Value::{Boolean, Date, Number, Text};
    let number = |value: &formula::Value| match value {
        Number(n) | Date(n) => Some(*n),
        Boolean(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    };
    match (a, b) {
        (Text(a), Text(b)) => Some(a.to_lowercase().cmp(&b.to_lowercase())),
        _ => number(a)?.partial_cmp(&number(b)?),
    }
}

/// A workbook's cells as their results stand, for a condition to read.
struct Standing<'a> {
    document: &'a SheetDocument,
}

impl Cells for Standing<'_> {
    fn sheet(&self, name: &str) -> Option<usize> {
        self.document.sheet_index(name)
    }

    fn year(&self) -> Option<i64> {
        self.document.this_year
    }

    fn value(
        &self,
        sheet: usize,
        row: usize,
        column: usize,
    ) -> Result<formula::Value, formula::Unsupported> {
        let Some(index) = self.document.sheets.get(sheet) else {
            return Ok(formula::Value::Error(formula::Error::Reference));
        };
        Ok(self
            .document
            .cell(index, row, column)
            .map_or(formula::Value::Empty, |cell| held(&cell)))
    }
}

impl SheetDocument {
    /// Work out which style every conditional format gives each cell it
    /// covers, for every sheet.
    pub(super) fn apply_conditions(&mut self) {
        let applied: Vec<HashMap<(usize, usize), String>> = (0..self.sheets.len())
            .map(|sheet| self.conditions_of(sheet))
            .collect();
        for (sheet, applied) in self.sheets.iter_mut().zip(applied) {
            sheet.conditional = applied;
        }
    }

    /// The names of the cell styles that carry a `style:map`.
    fn mapped_styles(&self) -> std::collections::HashSet<&str> {
        let parts = [
            Some(&self.document.content),
            self.document.styles_part.as_ref(),
        ];
        parts
            .into_iter()
            .flatten()
            .flat_map(|part| {
                ["automatic-styles", "styles"]
                    .into_iter()
                    .filter_map(|local| part.child(&Ns::Office, local))
            })
            .flat_map(Element::elements)
            .filter(|style| {
                style.attr(&Ns::Style, "family") == Some("table-cell")
                    && style.child(&Ns::Style, "map").is_some()
            })
            .filter_map(|style| style.attr(&Ns::Style, "name"))
            .collect()
    }

    fn conditions_of(&self, sheet: usize) -> HashMap<(usize, usize), String> {
        let mut applied = HashMap::new();
        let Some(index) = self.sheets.get(sheet) else {
            return applied;
        };
        let Some(table) = self
            .document
            .body_of("spreadsheet")
            .and_then(|body| body.children.get(index.table))
            .and_then(|node| match node {
                crate::xml::Node::Element(table) => Some(table),
                _ => None,
            })
        else {
            return applied;
        };
        let cells = Standing { document: self };
        let value = |row: usize, column: usize| {
            self.cell(index, row, column)
                .map_or(formula::Value::Empty, |cell| held(&cell))
        };
        let formats = ConditionalFormats::read(table);
        if !formats.is_empty() {
            for (from, to) in formats.ranges() {
                for row in from.row..=to.row.min(index.used_rows.saturating_sub(1)) {
                    for column in from.column..=to.column.min(index.used_columns.saturating_sub(1))
                    {
                        if applied.contains_key(&(row, column)) {
                            continue;
                        }
                        if let Some(style) =
                            formats.style_for(sheet, row, column, &value(row, column), &cells)
                        {
                            applied.insert((row, column), style.to_owned());
                        }
                    }
                }
            }
            return applied;
        }
        let mapped = self.mapped_styles();
        if mapped.is_empty() {
            return applied;
        }
        let mut maps: HashMap<String, Vec<Condition>> = HashMap::new();
        for row in 0..index.used_rows {
            for column in 0..index.used_columns {
                let cell = self.cell(index, row, column);
                let Some(name) = cell.as_ref().and_then(Cell::style_name).or_else(|| {
                    index
                        .columns
                        .get(column)
                        .and_then(|c| c.default_cell_style.as_deref())
                }) else {
                    continue;
                };
                if !mapped.contains(name) {
                    continue;
                }
                let found = maps
                    .entry(name.to_owned())
                    .or_insert_with(|| style_maps(&self.document.styles, name));
                if found.is_empty() {
                    continue;
                }
                let at = Address { row, column };
                let value = value(row, column);
                if let Some(condition) = found
                    .iter()
                    .find(|condition| holds(condition, sheet, at, &value, &cells))
                {
                    applied.insert((row, column), condition.style.clone());
                }
            }
        }
        applied
    }
}
