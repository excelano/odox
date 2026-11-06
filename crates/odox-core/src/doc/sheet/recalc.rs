//! Working formulas out again after their cells change. DESIGN.md §11.
//!
//! A document opens with the results its producer wrote, and they are trusted:
//! nothing is recalculated until a value changes. Then every formula that
//! reads the changed cells, and every formula that reads those, is worked out
//! again in the order they depend on each other, and its result is written in
//! the kind of value the cell already held and shown in its number format. A
//! formula that cannot be worked out here keeps its old result and is answered
//! as stale, and so is everything that reads it; one that cannot even be read
//! might read anything, so it is stale whenever anything changes.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use super::{Cell, SheetDocument, Value, Writing};
use crate::formula::{self, Cells, Error, Expr, Reference, Unsupported};
use crate::xml::Ns;

/// A cell by sheet, row and column, all counting from zero.
pub type Place = (usize, usize, usize);

/// What a recalculation did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Recalculated {
    /// The formula cells given a new result.
    pub updated: Vec<Place>,
    /// The formula cells whose result could not be worked out and may now be
    /// out of date.
    pub stale: Vec<Place>,
}

struct Formula {
    parsed: Result<Expr, Unsupported>,
    reads: Vec<(usize, Reference)>,
}

impl SheetDocument {
    /// Work out again every formula that reads, directly or through other
    /// formulas, one of the cells that changed.
    pub fn recalculate(&mut self, changed: &[Place]) -> Recalculated {
        if changed.is_empty() {
            return Recalculated::default();
        }
        let formulas = self.formulas();
        let mut dirty: HashSet<Place> = formulas
            .iter()
            .filter(|(_, formula)| formula.parsed.is_err())
            .map(|(place, _)| *place)
            .collect();
        dirty.extend(changed.iter().filter(|place| formulas.contains_key(place)));
        let mut frontier: Vec<Place> = changed.to_vec();
        while let Some((sheet, row, column)) = frontier.pop() {
            for (place, formula) in &formulas {
                if dirty.contains(place) {
                    continue;
                }
                let reads = formula
                    .reads
                    .iter()
                    .any(|(s, reference)| *s == sheet && reference.contains(row, column));
                if reads {
                    dirty.insert(*place);
                    frontier.push(*place);
                }
            }
        }
        self.work_out(&formulas, &dirty)
    }

    /// Work out every formula in the workbook again, as after its rows or
    /// columns moved.
    pub fn recalculate_all(&mut self) -> Recalculated {
        let formulas = self.formulas();
        let dirty = formulas.keys().copied().collect();
        self.work_out(&formulas, &dirty)
    }

    fn work_out(
        &mut self,
        formulas: &HashMap<Place, Formula>,
        dirty: &HashSet<Place>,
    ) -> Recalculated {
        let book = Book {
            document: self,
            formulas,
            dirty,
            known: RefCell::new(HashMap::new()),
            visiting: RefCell::new(HashSet::new()),
        };
        let mut places: Vec<Place> = dirty.iter().copied().collect();
        places.sort_unstable();
        let results: Vec<(Place, Result<formula::Value, Unsupported>)> = places
            .into_iter()
            .map(|(sheet, row, column)| ((sheet, row, column), book.value(sheet, row, column)))
            .collect();
        let mut done = Recalculated::default();
        for (place, result) in results {
            match result {
                Ok(value) if self.put_result(place, &value).is_ok() => done.updated.push(place),
                _ => done.stale.push(place),
            }
        }
        self.reindex();
        done
    }

    /// Every formula cell in the workbook, read. A formula in a cell the
    /// document writes once for several, which reads differently in each, is
    /// not read.
    fn formulas(&self) -> HashMap<Place, Formula> {
        let mut out = HashMap::new();
        for (sheet_index, sheet) in self.sheets.iter().enumerate() {
            for range in &sheet.rows {
                let Some(row) = self.row_element(sheet, range.first) else {
                    continue;
                };
                let mut column = 0usize;
                for cell in row.elements() {
                    let covered = cell.is(&Ns::Table, "covered-table-cell");
                    if !covered && !cell.is(&Ns::Table, "table-cell") {
                        continue;
                    }
                    let repeat = cell
                        .attr_usize(&Ns::Table, "number-columns-repeated")
                        .unwrap_or(1)
                        .max(1);
                    if let Some(written) = cell.attr(&Ns::Table, "formula") {
                        let parsed = if repeat > 1 || range.count > 1 {
                            Err(Unsupported)
                        } else {
                            formula::parse(written)
                        };
                        let reads = parsed
                            .as_ref()
                            .map(|expr| self.references(expr, sheet_index))
                            .unwrap_or_default();
                        for offset in 0..repeat.min(1024) {
                            for down in 0..range.count.min(1024) {
                                out.insert(
                                    (sheet_index, range.first + down, column + offset),
                                    Formula {
                                        parsed: parsed.clone(),
                                        reads: reads.clone(),
                                    },
                                );
                            }
                        }
                    }
                    column += repeat;
                }
            }
        }
        out
    }

    /// The references a formula reads, each with the sheet it is on.
    fn references(&self, expr: &Expr, sheet: usize) -> Vec<(usize, Reference)> {
        let mut out = Vec::new();
        let mut walk = vec![expr];
        while let Some(expr) = walk.pop() {
            match expr {
                Expr::Reference(reference) => {
                    let on = match &reference.sheet {
                        Some(name) => self.sheet_index(name),
                        None => Some(sheet),
                    };
                    if let Some(on) = on {
                        out.push((on, reference.clone()));
                    }
                }
                Expr::Negate(inner) | Expr::Percent(inner) => walk.push(inner),
                Expr::Binary(_, left, right) => {
                    walk.push(left);
                    walk.push(right);
                }
                Expr::Call(_, arguments) => walk.extend(arguments),
                _ => {}
            }
        }
        out
    }

    fn sheet_index(&self, name: &str) -> Option<usize> {
        self.sheets
            .iter()
            .position(|sheet| sheet.name == name)
            .or_else(|| {
                self.sheets
                    .iter()
                    .position(|sheet| sheet.name.eq_ignore_ascii_case(name))
            })
    }

    /// Write a formula's result into its cell, keeping the formula: as the
    /// kind of value the cell held where the result is a number, and shown in
    /// the cell's number format.
    fn put_result(
        &mut self,
        (sheet, row, column): Place,
        result: &formula::Value,
    ) -> Result<(), crate::Refused> {
        let index = self.sheets.get(sheet).ok_or(crate::Refused::NotFound)?;
        let held = self
            .cell(index, row, column)
            .map_or(Value::Empty, |cell| cell.value());
        let (value, writing) = match result {
            formula::Value::Empty => (Value::Number(0.0), Writing::Result),
            formula::Value::Number(n) => (
                match held {
                    Value::Date(_) => Value::Date(iso_date(*n)),
                    Value::Time(_) => Value::Time(duration(*n)),
                    Value::Percentage(_) => Value::Percentage(*n),
                    Value::Currency(_, code) => Value::Currency(*n, code),
                    _ => Value::Number(*n),
                },
                Writing::Result,
            ),
            formula::Value::Date(n) => (Value::Date(iso_date(*n)), Writing::Result),
            formula::Value::Text(text) => (Value::Text(text.clone()), Writing::Result),
            formula::Value::Boolean(b) => (Value::Boolean(*b), Writing::Result),
            formula::Value::Error(error) => (Value::Text(error.code().to_owned()), Writing::Error),
        };
        let shown = match (&value, writing) {
            (_, Writing::Error) | (Value::Text(_), _) => value.cached_text(),
            _ => self
                .data_style_name(index, row, column)
                .and_then(|name| self.document.styles.data_styles().format(name, &value))
                .unwrap_or_else(|| match &value {
                    Value::Number(n) => formula::spelled(*n),
                    other => other.cached_text(),
                }),
        };
        self.write_cell(sheet, row, column, &value, &shown, writing)
    }
}

/// The workbook as a formula reads it: a cell to be worked out again is, once,
/// and every other cell answers what it holds.
struct Book<'a> {
    document: &'a SheetDocument,
    formulas: &'a HashMap<Place, Formula>,
    dirty: &'a HashSet<Place>,
    known: RefCell<HashMap<Place, Result<formula::Value, Unsupported>>>,
    visiting: RefCell<HashSet<Place>>,
}

impl Cells for Book<'_> {
    fn sheet(&self, name: &str) -> Option<usize> {
        self.document.sheet_index(name)
    }

    fn value(
        &self,
        sheet: usize,
        row: usize,
        column: usize,
    ) -> Result<formula::Value, Unsupported> {
        let place = (sheet, row, column);
        if !self.dirty.contains(&place) {
            let Some(index) = self.document.sheets.get(sheet) else {
                return Ok(formula::Value::Error(Error::Reference));
            };
            return Ok(self
                .document
                .cell(index, row, column)
                .map_or(formula::Value::Empty, |cell| held(&cell)));
        }
        if let Some(known) = self.known.borrow().get(&place) {
            return known.clone();
        }
        if !self.visiting.borrow_mut().insert(place) {
            return Ok(formula::Value::Error(Error::Circular));
        }
        let result = match self.formulas.get(&place).map(|f| &f.parsed) {
            Some(Ok(expr)) => formula::evaluate(expr, sheet, self),
            _ => Err(Unsupported),
        };
        self.visiting.borrow_mut().remove(&place);
        self.known.borrow_mut().insert(place, result.clone());
        result
    }
}

/// What a cell holds, as a formula reads it.
fn held(cell: &Cell<'_>) -> formula::Value {
    if cell.covered {
        return formula::Value::Empty;
    }
    if cell.element.attr(&Ns::Calcext, "value-type") == Some("error") {
        let shown = cell.text();
        return formula::Value::Error(
            Error::ALL
                .into_iter()
                .find(|error| error.code() == shown)
                .unwrap_or(Error::Value),
        );
    }
    match cell.value() {
        Value::Empty => formula::Value::Empty,
        Value::Number(n) | Value::Percentage(n) | Value::Currency(n, _) => {
            formula::Value::Number(n)
        }
        Value::Date(text) => {
            date_serial(&text).map_or(formula::Value::Text(text), formula::Value::Date)
        }
        Value::Time(text) => {
            duration_days(&text).map_or(formula::Value::Text(text), formula::Value::Number)
        }
        Value::Boolean(b) => formula::Value::Boolean(b),
        Value::Text(text) => formula::Value::Text(text),
    }
}

/// `2026-11-05` or `2026-11-05T14:30:00` as days from 30 December 1899.
pub(super) fn date_serial(text: &str) -> Option<f64> {
    let (date, time) = text.split_once('T').unwrap_or((text, ""));
    let mut fields = date.splitn(3, '-').map(|f| f.parse::<i64>().ok());
    let (year, month, day) = (fields.next()??, fields.next()??, fields.next()??);
    let mut serial = formula::eval_serial(year, month, day);
    if !time.is_empty() {
        let mut clock = time.split(':').map(|f| f.parse::<f64>().unwrap_or(0.0));
        let (h, m, s) = (
            clock.next().unwrap_or(0.0),
            clock.next().unwrap_or(0.0),
            clock.next().unwrap_or(0.0),
        );
        serial += (h * 3600.0 + m * 60.0 + s) / 86_400.0;
    }
    Some(serial)
}

/// `PT14H05M09S` as a fraction of a day.
pub(super) fn duration_days(text: &str) -> Option<f64> {
    let rest = text.strip_prefix('P')?;
    let (days, time) = rest.split_once('T').unwrap_or((rest, ""));
    let days: f64 = days.strip_suffix('D').map_or(Ok(0.0), str::parse).ok()?;
    let mut seconds = 0.0;
    let mut digits = String::new();
    for c in time.chars() {
        if c.is_ascii_digit() || c == '.' {
            digits.push(c);
            continue;
        }
        let n: f64 = digits.parse().ok()?;
        seconds += n * match c {
            'H' => 3600.0,
            'M' => 60.0,
            'S' => 1.0,
            _ => return None,
        };
        digits.clear();
    }
    Some(days + seconds / 86_400.0)
}

/// Days from 30 December 1899 as a date cell holds them, with the time of
/// day where there is one.
pub(super) fn iso_date(serial: f64) -> String {
    let (year, month, day) = formula::eval_civil(serial);
    let seconds = ((serial - serial.floor()) * 86_400.0).round();
    if seconds == 0.0 {
        format!("{year:04}-{month:02}-{day:02}")
    } else {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let seconds = seconds as u64;
        format!(
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    }
}

/// A fraction of a day as the duration a time cell holds.
pub(super) fn duration(days: f64) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let seconds = (days.abs() * 86_400.0).round() as u64;
    format!(
        "PT{:02}H{:02}M{:02}S",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}
