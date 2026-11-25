//! Looking values up, conditions over several ranges, and the references a
//! function works out.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::cmp::Ordering;

use super::{
    Arg, Context, Criterion, arity, compare, count_value, number, same_kind, truth, whole,
};
use crate::formula::{Address, Error, Expr, Reference, Unsupported, Value};

/// How a value is looked for in a list: as itself, or in a sorted list as
/// the last at or below it, or at or above it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Search {
    Exact,
    AtOrBelow,
    AtOrAbove,
}

/// The last row and column a sheet can have, counting from zero.
const LAST_ROW: f64 = 1_048_575.0;
const LAST_COLUMN: f64 = 16_383.0;

/// The most cells a reference worked out by a function may cover before it
/// is left to the application that wrote the document.
const MOST_CELLS: f64 = 1_000_000.0;

/// The functions this family works out.
pub(super) const NAMES: &[&str] = &[
    "SUMIFS",
    "COUNTIFS",
    "AVERAGEIFS",
    "INDEX",
    "MATCH",
    "HLOOKUP",
    "XLOOKUP",
    "SUMPRODUCT",
    "OFFSET",
    "INDIRECT",
];

impl Context<'_> {
    pub(super) fn lookup_function(&self, name: &str, args: &[Expr]) -> Result<Value, Unsupported> {
        Ok(match name {
            "SUMIFS" | "COUNTIFS" | "AVERAGEIFS" => self.conditions(name, args)?,
            "INDEX" => self.index(args)?,
            "MATCH" => self.position(args)?,
            "HLOOKUP" => self.horizontal(args)?,
            "XLOOKUP" => self.xlookup(args)?,
            "SUMPRODUCT" => self.sum_product(args)?,
            "OFFSET" | "INDIRECT" => match self.reference_call(name, args)? {
                Ok(reference) => self.intersection(&reference)?,
                Err(error) => Value::Error(error),
            },
            _ => return Err(Unsupported),
        })
    }

    /// An argument worked out cell by cell over the ranges in it, as an
    /// array: `(A1:A4="a")*B1:B4` is four values. A value stands for an
    /// array of one, and an operator joins two arrays cell by cell, or one
    /// with each cell of the other; arrays of other shapes come to `#VALUE!`.
    fn cells_of(&self, expr: &Expr) -> Result<Vec<Vec<Value>>, Unsupported> {
        Ok(match expr {
            Expr::Binary(op, left, right) => {
                let (left, right) = (self.cells_of(left)?, self.cells_of(right)?);
                let shape = |rows: &Vec<Vec<Value>>| (rows.len(), rows.first().map_or(0, Vec::len));
                let (a, b) = (shape(&left), shape(&right));
                let size = if a == (1, 1) { b } else { a };
                if a != b && a != (1, 1) && b != (1, 1) {
                    return Ok(vec![vec![Value::Error(Error::Value)]]);
                }
                let at = |rows: &Vec<Vec<Value>>, r: usize, c: usize| {
                    if shape(rows) == (1, 1) {
                        rows[0][0].clone()
                    } else {
                        rows[r][c].clone()
                    }
                };
                (0..size.0)
                    .map(|r| {
                        (0..size.1)
                            .map(|c| self.combine(*op, at(&left, r, c), at(&right, r, c)))
                            .collect()
                    })
                    .collect()
            }
            Expr::Negate(inner) | Expr::Percent(inner) => {
                let by = if matches!(expr, Expr::Negate(_)) {
                    -1.0
                } else {
                    0.01
                };
                self.cells_of(inner)?
                    .into_iter()
                    .map(|row| {
                        row.into_iter()
                            .map(|value| match self.number(&value) {
                                Ok(n) => Value::Number(n * by),
                                Err(error) => Value::Error(error),
                            })
                            .collect()
                    })
                    .collect()
            }
            _ => match self.arg(expr)? {
                Arg::Range(rows) => rows,
                Arg::Scalar(value) => vec![vec![value]],
            },
        })
    }

    /// The row of a sorted or unsorted table whose first cell is a value,
    /// and that row's cell in a column; `#N/A` where none is. A sorted search
    /// ends at an error where `errors_end`, as `LibreOffice`'s `HLOOKUP` does
    /// and its `VLOOKUP` does not.
    pub(super) fn vertical(
        &self,
        wanted: &Value,
        rows: &[Vec<Value>],
        column: usize,
        (sorted, errors_end): (bool, bool),
    ) -> Value {
        let first: Vec<Value> = rows.iter().map(|row| row[0].clone()).collect();
        let how = if sorted {
            Search::AtOrBelow
        } else {
            Search::Exact
        };
        match self.find(wanted, &first, how, errors_end) {
            Some(at) => rows[at][column].clone(),
            None => Value::Error(Error::NotAvailable),
        }
    }

    /// Where a value is in a list.
    fn find(&self, wanted: &Value, list: &[Value], how: Search, errors_end: bool) -> Option<usize> {
        match how {
            Search::Exact => {
                let exact = Criterion::lookup(wanted, self.matching());
                list.iter().position(|cell| match wanted {
                    Value::Text(_) => *cell != Value::Empty && exact.matches(cell),
                    _ => {
                        same_kind(cell, wanted)
                            && compare(cell, wanted, false) == Ok(Ordering::Equal)
                    }
                })
            }
            Search::AtOrBelow | Search::AtOrAbove => {
                let past = if how == Search::AtOrBelow {
                    Ordering::Greater
                } else {
                    Ordering::Less
                };
                let mut found = None;
                for (at, cell) in list.iter().enumerate() {
                    if errors_end && matches!(cell, Value::Error(_)) {
                        break;
                    }
                    if !same_kind(cell, wanted) {
                        continue;
                    }
                    if compare(cell, wanted, false) == Ok(past) {
                        break;
                    }
                    found = Some(at);
                }
                found
            }
        }
    }

    fn horizontal(&self, args: &[Expr]) -> Result<Value, Unsupported> {
        arity(args, 3, 4)?;
        let wanted = self.scalar(&args[0])?;
        if let Value::Error(error) = wanted {
            return Ok(Value::Error(error));
        }
        let Arg::Range(rows) = self.arg(&args[1])? else {
            return Ok(Value::Error(Error::Value));
        };
        let row = tryv!(self.number_at(args, 2)?).trunc();
        if row < 1.0 || whole(row) > rows.len() {
            return Ok(Value::Error(Error::Argument));
        }
        let sorted = match args.get(3) {
            Some(expr) => tryv!(truth(&self.scalar(expr)?)),
            None => true,
        };
        let width = rows.first().map_or(0, Vec::len);
        let columns: Vec<Vec<Value>> = (0..width)
            .map(|c| rows.iter().map(|row| row[c].clone()).collect())
            .collect();
        Ok(self.vertical(&wanted, &columns, whole(row) - 1, (sorted, true)))
    }

    fn position(&self, args: &[Expr]) -> Result<Value, Unsupported> {
        arity(args, 2, 3)?;
        let wanted = self.scalar(&args[0])?;
        if let Value::Error(error) = wanted {
            return Ok(Value::Error(error));
        }
        let rows = self.cells_of(&args[1])?;
        let Some(list) = line(&rows) else {
            return Ok(Value::Error(Error::NotAvailable));
        };
        let kind = match args.get(2) {
            Some(expr) => tryv!(self.number(&self.scalar(expr)?)),
            None => 1.0,
        };
        let how = if kind > 0.0 {
            Search::AtOrBelow
        } else if kind < 0.0 {
            Search::AtOrAbove
        } else {
            Search::Exact
        };
        Ok(match self.find(&wanted, &list, how, true) {
            Some(at) => count_value(at + 1),
            None => Value::Error(Error::NotAvailable),
        })
    }

    fn xlookup(&self, args: &[Expr]) -> Result<Value, Unsupported> {
        arity(args, 3, 6)?;
        let wanted = self.scalar(&args[0])?;
        if let Value::Error(error) = wanted {
            return Ok(Value::Error(error));
        }
        let given = |at: usize| args.get(at).filter(|expr| !matches!(expr, Expr::Missing));
        if let Some(mode) = given(4)
            && tryv!(self.number(&self.scalar(mode)?)) != 0.0
        {
            return Err(Unsupported);
        }
        let backwards = match given(5) {
            Some(expr) => {
                #[allow(clippy::cast_possible_truncation)]
                let mode = tryv!(self.number(&self.scalar(expr)?)).trunc() as i64;
                match mode {
                    1 => false,
                    -1 => true,
                    _ => return Err(Unsupported),
                }
            }
            None => false,
        };
        let looked = self.cells_of(&args[1])?;
        let returned = self.cells_of(&args[2])?;
        let size = |rows: &Vec<Vec<Value>>| rows.len() * rows.first().map_or(0, Vec::len);
        if size(&looked) != size(&returned) {
            return Ok(Value::Error(Error::ArgumentKind));
        }
        let Some(mut list) = line(&looked) else {
            return Ok(Value::Error(Error::Value));
        };
        if backwards {
            list.reverse();
        }
        let found = self
            .find(&wanted, &list, Search::Exact, false)
            .map(|at| if backwards { list.len() - 1 - at } else { at });
        let Some(at) = found else {
            return Ok(match given(3) {
                Some(expr) => self.scalar(expr)?,
                None => Value::Error(Error::NotAvailable),
            });
        };
        let down = looked.len() > 1 || looked.first().is_some_and(|row| row.len() == 1);
        let cell = if down {
            returned
                .get(at)
                .filter(|row| row.len() == 1)
                .map(|row| row[0].clone())
        } else {
            returned
                .first()
                .filter(|_| returned.len() == 1)
                .and_then(|row| row.get(at).cloned())
        };
        Ok(cell.unwrap_or(Value::Error(Error::Value)))
    }

    fn index(&self, args: &[Expr]) -> Result<Value, Unsupported> {
        arity(args, 2, 3)?;
        let rows = self.cells_of(&args[0])?;
        let first = tryv!(self.number_at(args, 1)?).trunc();
        let second = match args.get(2) {
            Some(expr) => Some(tryv!(self.number(&self.scalar(expr)?)).trunc()),
            None => None,
        };
        let width = rows.first().map_or(0, Vec::len);
        let (row, column) = match second {
            Some(column) => (first, column),
            None if rows.len() == 1 && width > 1 => (1.0, first),
            None if width > 1 => {
                let Expr::Reference(reference) = &args[0] else {
                    return Err(Unsupported);
                };
                let (from, to) = (reference.from.column, reference.to.column);
                if !(from..=to).contains(&self.at.column) {
                    return Ok(Value::Error(Error::Value));
                }
                #[allow(clippy::cast_precision_loss)]
                (first, (self.at.column - from + 1) as f64)
            }
            None => (first, 1.0),
        };
        if row < 1.0 || column < 1.0 {
            return Err(Unsupported);
        }
        Ok(rows
            .get(whole(row) - 1)
            .and_then(|r| r.get(whole(column) - 1))
            .cloned()
            .unwrap_or(Value::Error(Error::Reference)))
    }

    fn conditions(&self, name: &str, args: &[Expr]) -> Result<Value, Unsupported> {
        let (summed, pairs) = if name == "COUNTIFS" {
            (None, args)
        } else {
            (
                Some(args.first().ok_or(Unsupported)?),
                &args[1.min(args.len())..],
            )
        };
        if pairs.is_empty() || pairs.len() % 2 != 0 {
            return Err(Unsupported);
        }
        let summed = match summed {
            Some(expr) => match self.arg(expr)? {
                Arg::Range(rows) => Some(rows),
                Arg::Scalar(_) => return Ok(Value::Error(Error::Value)),
            },
            None => None,
        };
        let mut tests = Vec::new();
        for pair in pairs.chunks(2) {
            let Arg::Range(rows) = self.arg(&pair[0])? else {
                return Ok(Value::Error(Error::Value));
            };
            let criterion = self.scalar(&pair[1])?;
            if let Value::Error(error) = criterion {
                return Ok(Value::Error(error));
            }
            tests.push((rows, Criterion::of(&criterion, self.matching())));
        }
        let shape = |rows: &Vec<Vec<Value>>| (rows.len(), rows.first().map_or(0, Vec::len));
        let wanted_shape = shape(&tests[0].0);
        if tests.iter().any(|(rows, _)| shape(rows) != wanted_shape)
            || summed
                .as_ref()
                .is_some_and(|rows| shape(rows) != wanted_shape)
        {
            return Ok(Value::Error(Error::Argument));
        }
        let (mut sum, mut n) = (0.0, 0usize);
        for r in 0..wanted_shape.0 {
            for c in 0..wanted_shape.1 {
                if !tests.iter().all(|(rows, test)| test.matches(&rows[r][c])) {
                    continue;
                }
                match &summed {
                    None => n += 1,
                    Some(rows) => match &rows[r][c] {
                        value @ (Value::Number(_) | Value::Date(_) | Value::Boolean(_)) => {
                            sum += number(value).unwrap_or(0.0);
                            n += 1;
                        }
                        Value::Error(error) => return Ok(Value::Error(*error)),
                        _ => {}
                    },
                }
            }
        }
        Ok(match name {
            "COUNTIFS" => count_value(n),
            "SUMIFS" => Value::Number(sum),
            _ if n == 0 => Value::Error(Error::DivisionByZero),
            #[allow(clippy::cast_precision_loss)]
            _ => Value::Number(sum / n as f64),
        })
    }

    fn sum_product(&self, args: &[Expr]) -> Result<Value, Unsupported> {
        if args.is_empty() {
            return Err(Unsupported);
        }
        let ranges: Vec<Vec<Vec<Value>>> = args
            .iter()
            .map(|arg| self.cells_of(arg))
            .collect::<Result<_, _>>()?;
        let shape = |rows: &Vec<Vec<Value>>| (rows.len(), rows.first().map_or(0, Vec::len));
        let first = shape(&ranges[0]);
        if ranges.iter().any(|rows| shape(rows) != first) {
            return Ok(Value::Error(Error::Value));
        }
        let mut sum = 0.0;
        for r in 0..first.0 {
            for c in 0..first.1 {
                let mut product = 1.0;
                for rows in &ranges {
                    product *= match &rows[r][c] {
                        Value::Number(n) | Value::Date(n) => *n,
                        Value::Boolean(b) => f64::from(u8::from(*b)),
                        Value::Error(error) => return Ok(Value::Error(*error)),
                        Value::Empty | Value::Text(_) => 0.0,
                    };
                }
                sum += product;
            }
        }
        Ok(Value::Number(sum))
    }

    /// The reference `OFFSET` or `INDIRECT` works out.
    pub(super) fn reference_call(
        &self,
        name: &str,
        args: &[Expr],
    ) -> Result<Result<Reference, Error>, Unsupported> {
        let name = name.strip_prefix(super::MICROSOFT).unwrap_or(name);
        if name == "INDIRECT" {
            arity(args, 1, 2)?;
            if let Some(style) = args.get(1)
                && !matches!(style, Expr::Missing)
                && !match truth(&self.scalar(style)?) {
                    Ok(a1) => a1,
                    Err(error) => return Ok(Err(error)),
                }
            {
                return Err(Unsupported);
            }
            let written = match self.text_at(args, 0)? {
                Ok(text) => text,
                Err(error) => return Ok(Err(error)),
            };
            return Ok(indirect(&written).ok_or(Error::Reference));
        }
        arity(args, 3, 5)?;
        let base = match &args[0] {
            Expr::Reference(base) => base.clone(),
            Expr::Call(name, inner) if super::is_reference_call(name) => {
                match self.reference_call(name, inner)? {
                    Ok(base) => base,
                    Err(error) => return Ok(Err(error)),
                }
            }
            _ => return Err(Unsupported),
        };
        let base = &base;
        let mut moved = [0.0; 4];
        #[allow(clippy::cast_precision_loss)]
        for (at, slot) in moved.iter_mut().enumerate() {
            *slot = match args.get(at + 1) {
                Some(Expr::Missing) | None if at >= 2 => {
                    if at == 2 {
                        (base.to.row - base.from.row + 1) as f64
                    } else {
                        (base.to.column - base.from.column + 1) as f64
                    }
                }
                Some(expr) => match self.number(&self.scalar(expr)?) {
                    Ok(n) => n.trunc(),
                    Err(error) => return Ok(Err(error)),
                },
                None => 0.0,
            };
        }
        let [rows, columns, height, width] = moved;
        #[allow(clippy::cast_precision_loss)]
        let (row, column) = (
            base.from.row as f64 + rows,
            base.from.column as f64 + columns,
        );
        if row < 0.0
            || column < 0.0
            || height < 1.0
            || width < 1.0
            || row + height > LAST_ROW + 1.0
            || column + width > LAST_COLUMN + 1.0
        {
            return Ok(Err(Error::Argument));
        }
        if height * width > MOST_CELLS {
            return Err(Unsupported);
        }
        let from = Address {
            row: whole(row),
            column: whole(column),
        };
        Ok(Ok(Reference {
            sheet: base.sheet.clone(),
            from,
            to: Address {
                row: from.row + whole(height) - 1,
                column: from.column + whole(width) - 1,
            },
        }))
    }
}

/// A one row or one column range as a list.
fn line(rows: &[Vec<Value>]) -> Option<Vec<Value>> {
    match rows {
        [only] => Some(only.clone()),
        _ if rows.iter().all(|row| row.len() == 1) => {
            Some(rows.iter().map(|row| row[0].clone()).collect())
        }
        _ => None,
    }
}

/// A reference written as text, `$Sheet1.B2`, `Sheet1.B2:B4` or `B2`.
fn indirect(written: &str) -> Option<Reference> {
    let written = if written.contains('.') {
        written.to_owned()
    } else {
        written.replacen('!', ".", 1)
    };
    let ends: Vec<String> = written
        .split(':')
        .map(|end| {
            if end.contains('.') {
                end.trim().to_owned()
            } else {
                format!(".{}", end.trim())
            }
        })
        .collect();
    if ends.is_empty() || ends.len() > 2 || ends.iter().any(|end| end.len() < 3) {
        return None;
    }
    match crate::formula::parse(&format!("of:=[{}]", ends.join(":"))).ok()? {
        Expr::Reference(reference) => Some(reference),
        _ => None,
    }
}
