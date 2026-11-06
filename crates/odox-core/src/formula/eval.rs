//! Working a formula out.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::cmp::Ordering;

use super::{Error, Expr, Op, Reference, Unsupported, Value};

/// The prefix ODF writes before a function Excel added and `OpenFormula`
/// names no other way, `COM.MICROSOFT.CONCAT`.
pub(crate) const MICROSOFT: &str = "COM.MICROSOFT.";

/// Where a formula reads the cells it refers to.
pub trait Cells {
    /// A sheet's index by its name.
    fn sheet(&self, name: &str) -> Option<usize>;

    /// What a cell holds, or comes to where it holds a formula. A formula cell
    /// whose result cannot be trusted answers [`Unsupported`], and so does
    /// everything that reads it.
    ///
    /// # Errors
    ///
    /// The cell holds a formula this crate cannot work out.
    fn value(&self, sheet: usize, row: usize, column: usize) -> Result<Value, Unsupported>;
}

/// Work out a formula standing on a sheet.
///
/// # Errors
///
/// The formula, or a cell it reads, uses something outside the core.
pub fn evaluate(expr: &Expr, sheet: usize, cells: &dyn Cells) -> Result<Value, Unsupported> {
    let context = Context { sheet, cells };
    Ok(match context.scalar(expr)? {
        Value::Number(n) | Value::Date(n) if !n.is_finite() => Value::Error(Error::Number),
        value => value,
    })
}

struct Context<'a> {
    sheet: usize,
    cells: &'a dyn Cells,
}

/// An argument as a function sees it: a value, or the cells of a reference.
enum Arg {
    Scalar(Value),
    Range(Vec<Vec<Value>>),
}

impl Arg {
    fn into_scalar(self) -> Value {
        match self {
            Self::Scalar(value) => value,
            Self::Range(mut rows) if rows.len() == 1 && rows[0].len() == 1 => rows[0].remove(0),
            Self::Range(_) => Value::Error(Error::Value),
        }
    }

    fn cells(&self) -> impl Iterator<Item = &Value> {
        let (scalar, rows) = match self {
            Self::Scalar(value) => (Some(value), None),
            Self::Range(rows) => (None, Some(rows)),
        };
        scalar
            .into_iter()
            .chain(rows.into_iter().flatten().flatten())
    }
}

macro_rules! tryv {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(error) => return Ok(Value::Error(error)),
        }
    };
}

impl Context<'_> {
    fn range(&self, reference: &Reference) -> Result<Vec<Vec<Value>>, Unsupported> {
        let sheet = match &reference.sheet {
            Some(name) => match self.cells.sheet(name) {
                Some(sheet) => sheet,
                None => return Ok(vec![vec![Value::Error(Error::Reference)]]),
            },
            None => self.sheet,
        };
        (reference.from.row..=reference.to.row)
            .map(|row| {
                (reference.from.column..=reference.to.column)
                    .map(|column| self.cells.value(sheet, row, column))
                    .collect()
            })
            .collect()
    }

    fn arg(&self, expr: &Expr) -> Result<Arg, Unsupported> {
        match expr {
            Expr::Reference(reference) => Ok(Arg::Range(self.range(reference)?)),
            other => Ok(Arg::Scalar(self.scalar(other)?)),
        }
    }

    fn scalar(&self, expr: &Expr) -> Result<Value, Unsupported> {
        Ok(match expr {
            Expr::Number(n) => Value::Number(*n),
            Expr::Text(text) => Value::Text(text.clone()),
            Expr::Error(error) => Value::Error(*error),
            Expr::Missing => Value::Empty,
            Expr::Reference(reference) => Arg::Range(self.range(reference)?).into_scalar(),
            Expr::Negate(inner) => Value::Number(-tryv!(number(&self.scalar(inner)?))),
            Expr::Percent(inner) => Value::Number(tryv!(number(&self.scalar(inner)?)) / 100.0),
            Expr::Binary(op, left, right) => {
                let (left, right) = (self.scalar(left)?, self.scalar(right)?);
                binary(*op, &left, &right)
            }
            Expr::Call(name, arguments) => self.call(name, arguments)?,
        })
    }

    #[allow(clippy::too_many_lines)]
    fn call(&self, name: &str, args: &[Expr]) -> Result<Value, Unsupported> {
        let name = name.strip_prefix(MICROSOFT).unwrap_or(name);
        let count = |low: usize, high: usize| {
            if (low..=high).contains(&args.len()) {
                Ok(())
            } else {
                Err(Unsupported)
            }
        };
        let num = |index: usize| -> Result<Result<f64, Error>, Unsupported> {
            Ok(number(&self.scalar(&args[index])?))
        };
        let txt = |index: usize| -> Result<Result<String, Error>, Unsupported> {
            Ok(text(&self.scalar(&args[index])?))
        };
        let all =
            || -> Result<Vec<Arg>, Unsupported> { args.iter().map(|a| self.arg(a)).collect() };
        Ok(match name {
            "TRUE" => {
                count(0, 0)?;
                Value::Boolean(true)
            }
            "FALSE" => {
                count(0, 0)?;
                Value::Boolean(false)
            }
            "SUM" => Value::Number(tryv!(numbers(&all()?)).iter().sum()),
            "PRODUCT" => {
                let found = tryv!(numbers(&all()?));
                Value::Number(if found.is_empty() {
                    0.0
                } else {
                    found.iter().product()
                })
            }
            "AVERAGE" => {
                let found = tryv!(numbers(&all()?));
                if found.is_empty() {
                    Value::Error(Error::DivisionByZero)
                } else {
                    #[allow(clippy::cast_precision_loss)]
                    Value::Number(found.iter().sum::<f64>() / found.len() as f64)
                }
            }
            "MIN" => Value::Number(
                tryv!(numbers(&all()?))
                    .into_iter()
                    .reduce(f64::min)
                    .unwrap_or(0.0),
            ),
            "MAX" => Value::Number(
                tryv!(numbers(&all()?))
                    .into_iter()
                    .reduce(f64::max)
                    .unwrap_or(0.0),
            ),
            "COUNT" => {
                let found = all()?;
                let n = found
                    .iter()
                    .map(|arg| match arg {
                        Arg::Scalar(value) => usize::from(number(value).is_ok()),
                        Arg::Range(_) => arg
                            .cells()
                            .filter(|v| {
                                matches!(v, Value::Number(_) | Value::Date(_) | Value::Boolean(_))
                            })
                            .count(),
                    })
                    .sum::<usize>();
                count_value(n)
            }
            "COUNTA" => {
                let n = all()?
                    .iter()
                    .flat_map(Arg::cells)
                    .filter(|v| !matches!(v, Value::Empty))
                    .count();
                count_value(n)
            }
            "COUNTBLANK" => {
                count(1, 1)?;
                let n = self
                    .arg(&args[0])?
                    .cells()
                    .filter(|v| matches!(v, Value::Empty) || *v == &Value::Text(String::new()))
                    .count();
                count_value(n)
            }
            "IF" => {
                count(1, 3)?;
                let condition = tryv!(truth(&self.scalar(&args[0])?));
                if condition {
                    match args.get(1) {
                        Some(expr) => self.scalar(expr)?,
                        None => Value::Boolean(true),
                    }
                } else {
                    match args.get(2) {
                        Some(expr) => self.scalar(expr)?,
                        None => Value::Boolean(false),
                    }
                }
            }
            "IFERROR" => {
                count(2, 2)?;
                match self.scalar(&args[0])? {
                    Value::Error(_) => self.scalar(&args[1])?,
                    value => value,
                }
            }
            "AND" | "OR" => {
                let mut found = Vec::new();
                for arg in all()? {
                    match arg {
                        Arg::Scalar(value) => found.push(tryv!(truth(&value))),
                        Arg::Range(_) => {
                            for value in arg.cells() {
                                match value {
                                    Value::Error(error) => return Ok(Value::Error(*error)),
                                    Value::Number(n) | Value::Date(n) => found.push(*n != 0.0),
                                    Value::Boolean(b) => found.push(*b),
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                if found.is_empty() {
                    Value::Error(Error::Value)
                } else if name == "AND" {
                    Value::Boolean(found.iter().all(|b| *b))
                } else {
                    Value::Boolean(found.iter().any(|b| *b))
                }
            }
            "NOT" => {
                count(1, 1)?;
                Value::Boolean(!tryv!(truth(&self.scalar(&args[0])?)))
            }
            "ROUND" | "ROUNDUP" | "ROUNDDOWN" => {
                count(1, 2)?;
                let x = tryv!(num(0)?);
                let places = if args.len() > 1 {
                    tryv!(num(1)?).trunc()
                } else {
                    0.0
                };
                Value::Number(round(x, places, name))
            }
            "ABS" => {
                count(1, 1)?;
                Value::Number(tryv!(num(0)?).abs())
            }
            "INT" => {
                count(1, 1)?;
                Value::Number(tryv!(num(0)?).floor())
            }
            "MOD" => {
                count(2, 2)?;
                let (a, b) = (tryv!(num(0)?), tryv!(num(1)?));
                if b == 0.0 {
                    Value::Error(Error::DivisionByZero)
                } else {
                    Value::Number(a - b * (a / b).floor())
                }
            }
            "POWER" => {
                count(2, 2)?;
                binary(
                    Op::Power,
                    &Value::Number(tryv!(num(0)?)),
                    &Value::Number(tryv!(num(1)?)),
                )
            }
            "SQRT" => {
                count(1, 1)?;
                let x = tryv!(num(0)?);
                if x < 0.0 {
                    Value::Error(Error::Number)
                } else {
                    Value::Number(x.sqrt())
                }
            }
            "CONCATENATE" | "CONCAT" => {
                let mut out = String::new();
                for arg in all()? {
                    for value in arg.cells() {
                        out.push_str(&tryv!(text(value)));
                    }
                }
                Value::Text(out)
            }
            "LEFT" | "RIGHT" => {
                count(1, 2)?;
                let source = tryv!(txt(0)?);
                let n = if args.len() > 1 { tryv!(num(1)?) } else { 1.0 };
                if n < 0.0 {
                    return Ok(Value::Error(Error::Value));
                }
                let n = whole(n);
                let chars: Vec<char> = source.chars().collect();
                let n = n.min(chars.len());
                Value::Text(if name == "LEFT" {
                    chars[..n].iter().collect()
                } else {
                    chars[chars.len() - n..].iter().collect()
                })
            }
            "MID" => {
                count(3, 3)?;
                let source = tryv!(txt(0)?);
                let (start, n) = (tryv!(num(1)?), tryv!(num(2)?));
                if start < 1.0 || n < 0.0 {
                    return Ok(Value::Error(Error::Value));
                }
                Value::Text(
                    source
                        .chars()
                        .skip(whole(start) - 1)
                        .take(whole(n))
                        .collect(),
                )
            }
            "LEN" => {
                count(1, 1)?;
                count_value(tryv!(txt(0)?).chars().count())
            }
            "UPPER" => {
                count(1, 1)?;
                Value::Text(tryv!(txt(0)?).to_uppercase())
            }
            "LOWER" => {
                count(1, 1)?;
                Value::Text(tryv!(txt(0)?).to_lowercase())
            }
            "TRIM" => {
                count(1, 1)?;
                Value::Text(
                    tryv!(txt(0)?)
                        .split(' ')
                        .filter(|w| !w.is_empty())
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            }
            "DATE" => {
                count(3, 3)?;
                let (y, m, d) = (
                    tryv!(num(0)?).trunc(),
                    tryv!(num(1)?).trunc(),
                    tryv!(num(2)?).trunc(),
                );
                let y = if (0.0..30.0).contains(&y) {
                    y + 2000.0
                } else if (30.0..100.0).contains(&y) {
                    y + 1900.0
                } else {
                    y
                };
                let months = y * 12.0 + (m - 1.0);
                let (y, m) = ((months / 12.0).floor(), months.rem_euclid(12.0) + 1.0);
                #[allow(clippy::cast_possible_truncation)]
                let first = serial(y as i64, m as i64, 1);
                Value::Date(first + d - 1.0)
            }
            "YEAR" | "MONTH" | "DAY" => {
                count(1, 1)?;
                let (y, m, d) = civil(tryv!(num(0)?));
                #[allow(clippy::cast_precision_loss)]
                Value::Number(match name {
                    "YEAR" => y as f64,
                    "MONTH" => m as f64,
                    _ => d as f64,
                })
            }
            "VLOOKUP" => {
                count(3, 4)?;
                let wanted = self.scalar(&args[0])?;
                if let Value::Error(error) = wanted {
                    return Ok(Value::Error(error));
                }
                let Arg::Range(rows) = self.arg(&args[1])? else {
                    return Ok(Value::Error(Error::Value));
                };
                let column = tryv!(num(2)?).trunc();
                if column < 1.0 {
                    return Ok(Value::Error(Error::Value));
                }
                let column = whole(column) - 1;
                if rows.first().is_none_or(|row| column >= row.len()) {
                    return Ok(Value::Error(Error::Reference));
                }
                let sorted = match args.get(3) {
                    Some(expr) => tryv!(truth(&self.scalar(expr)?)),
                    None => true,
                };
                let found = if sorted {
                    let mut last = None;
                    for row in &rows {
                        if !same_kind(&row[0], &wanted) {
                            continue;
                        }
                        if compare(&row[0], &wanted, false) == Ok(Ordering::Greater) {
                            break;
                        }
                        last = Some(row);
                    }
                    last
                } else {
                    rows.iter().find(|row| {
                        same_kind(&row[0], &wanted)
                            && compare(&row[0], &wanted, false) == Ok(Ordering::Equal)
                    })
                };
                match found {
                    Some(row) => row[column].clone(),
                    None => Value::Error(Error::NotAvailable),
                }
            }
            "SUMIF" | "COUNTIF" | "AVERAGEIF" => {
                count(2, if name == "COUNTIF" { 2 } else { 3 })?;
                let Arg::Range(range) = self.arg(&args[0])? else {
                    return Ok(Value::Error(Error::Value));
                };
                let criterion = Criterion::of(&self.scalar(&args[1])?);
                let values = match args.get(2) {
                    Some(expr) => match self.arg(expr)? {
                        Arg::Range(values) => values,
                        Arg::Scalar(_) => return Ok(Value::Error(Error::Value)),
                    },
                    None => range.clone(),
                };
                let mut sum = 0.0;
                let mut n = 0usize;
                for (r, row) in range.iter().enumerate() {
                    for (c, cell) in row.iter().enumerate() {
                        if !criterion.matches(cell) {
                            continue;
                        }
                        if name == "COUNTIF" {
                            n += 1;
                            continue;
                        }
                        match values.get(r).and_then(|row| row.get(c)) {
                            Some(Value::Number(x) | Value::Date(x)) => {
                                sum += x;
                                n += 1;
                            }
                            Some(Value::Error(error)) => return Ok(Value::Error(*error)),
                            _ => {}
                        }
                    }
                }
                match name {
                    "COUNTIF" => count_value(n),
                    "SUMIF" => Value::Number(sum),
                    _ if n == 0 => Value::Error(Error::DivisionByZero),
                    #[allow(clippy::cast_precision_loss)]
                    _ => Value::Number(sum / n as f64),
                }
            }
            _ => return Err(Unsupported),
        })
    }
}

#[allow(clippy::cast_precision_loss)]
fn count_value(n: usize) -> Value {
    Value::Number(n as f64)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn whole(n: f64) -> usize {
    n.trunc().clamp(0.0, 1e9) as usize
}

/// A value as a number: an empty cell is zero, true is one, and text that
/// reads as a number is that number.
fn number(value: &Value) -> Result<f64, Error> {
    match value {
        Value::Empty => Ok(0.0),
        Value::Number(n) | Value::Date(n) => Ok(*n),
        Value::Boolean(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Value::Text(t) => t.trim().parse().map_err(|_| Error::Value),
        Value::Error(error) => Err(*error),
    }
}

/// A value as text, a number spelled as briefly as it reads exactly.
fn text(value: &Value) -> Result<String, Error> {
    match value {
        Value::Empty => Ok(String::new()),
        Value::Number(n) | Value::Date(n) => Ok(spelled(*n)),
        Value::Boolean(b) => Ok(if *b { "1" } else { "0" }.to_owned()),
        Value::Text(t) => Ok(t.clone()),
        Value::Error(error) => Err(*error),
    }
}

/// A number written with up to fifteen significant digits, as a spreadsheet
/// writes one into text.
pub(crate) fn spelled(n: f64) -> String {
    if n.fract().abs() < f64::EPSILON && n.abs() < 1e15 {
        return format!("{n:.0}");
    }
    let digits = 14.0 - n.abs().log10().floor().clamp(-300.0, 300.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let places = digits.clamp(0.0, 20.0) as usize;
    let written = format!("{n:.places$}");
    if written.contains('.') {
        written
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    } else {
        written
    }
}

/// A value as a condition: a number is true unless it is zero.
fn truth(value: &Value) -> Result<bool, Error> {
    match value {
        Value::Empty => Ok(false),
        Value::Number(n) | Value::Date(n) => Ok(*n != 0.0),
        Value::Boolean(b) => Ok(*b),
        Value::Text(t) => match t.to_uppercase().as_str() {
            "TRUE" => Ok(true),
            "FALSE" => Ok(false),
            _ => Err(Error::Value),
        },
        Value::Error(error) => Err(*error),
    }
}

/// The numbers among a function's arguments: every number written or worked
/// out, and in a reference only the cells that hold one.
fn numbers(args: &[Arg]) -> Result<Vec<f64>, Error> {
    let mut out = Vec::new();
    for arg in args {
        match arg {
            Arg::Scalar(Value::Empty) => {}
            Arg::Scalar(value) => out.push(number(value)?),
            Arg::Range(_) => {
                for value in arg.cells() {
                    match value {
                        Value::Number(n) | Value::Date(n) => out.push(*n),
                        Value::Boolean(b) => out.push(if *b { 1.0 } else { 0.0 }),
                        Value::Error(error) => return Err(*error),
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(out)
}

fn binary(op: Op, left: &Value, right: &Value) -> Value {
    let arithmetic = |f: fn(f64, f64) -> Value| match (number(left), number(right)) {
        (Ok(a), Ok(b)) => f(a, b),
        (Err(error), _) | (_, Err(error)) => Value::Error(error),
    };
    match op {
        Op::Add => arithmetic(|a, b| Value::Number(a + b)),
        Op::Subtract => arithmetic(|a, b| Value::Number(a - b)),
        Op::Multiply => arithmetic(|a, b| Value::Number(a * b)),
        Op::Divide => arithmetic(|a, b| {
            if b == 0.0 {
                Value::Error(Error::DivisionByZero)
            } else {
                Value::Number(a / b)
            }
        }),
        Op::Power => arithmetic(|a, b| {
            let n = a.powf(b);
            if n.is_finite() {
                Value::Number(n)
            } else {
                Value::Error(Error::Number)
            }
        }),
        Op::Join => match (text(left), text(right)) {
            (Ok(a), Ok(b)) => Value::Text(a + &b),
            (Err(error), _) | (_, Err(error)) => Value::Error(error),
        },
        _ => match compare(left, right, true) {
            Ok(ordering) => Value::Boolean(match op {
                Op::Equal => ordering == Ordering::Equal,
                Op::NotEqual => ordering != Ordering::Equal,
                Op::Less => ordering == Ordering::Less,
                Op::LessOrEqual => ordering != Ordering::Greater,
                Op::Greater => ordering == Ordering::Greater,
                _ => ordering != Ordering::Less,
            }),
            Err(error) => Value::Error(error),
        },
    }
}

fn is_text(value: &Value) -> bool {
    matches!(value, Value::Text(_))
}

fn same_kind(a: &Value, b: &Value) -> bool {
    is_text(a) == is_text(b) && !matches!(a, Value::Empty | Value::Error(_))
}

/// Two values in order: numbers before text, an empty cell as zero beside a
/// number and as empty text beside text, and text with its case or without.
fn compare(left: &Value, right: &Value, with_case: bool) -> Result<Ordering, Error> {
    if let Value::Error(error) = left {
        return Err(*error);
    }
    if let Value::Error(error) = right {
        return Err(*error);
    }
    let as_text = |value: &Value| match value {
        Value::Text(t) => Some(t.clone()),
        Value::Empty => Some(String::new()),
        _ => None,
    };
    match (left, right) {
        (Value::Text(_), Value::Text(_) | Value::Empty) | (Value::Empty, Value::Text(_)) => {
            let (a, b) = (
                as_text(left).unwrap_or_default(),
                as_text(right).unwrap_or_default(),
            );
            Ok(if with_case {
                a.cmp(&b)
            } else {
                a.to_lowercase().cmp(&b.to_lowercase())
            })
        }
        (Value::Text(_), _) => Ok(Ordering::Greater),
        (_, Value::Text(_)) => Ok(Ordering::Less),
        _ => {
            let (a, b) = (number(left)?, number(right)?);
            Ok(a.partial_cmp(&b).unwrap_or(Ordering::Equal))
        }
    }
}

/// A criterion of `SUMIF` and `COUNTIF`: a value, or a comparison written as
/// text, `">2"`, `"<>apple"`.
struct Criterion {
    op: Op,
    against: Value,
}

impl Criterion {
    fn of(value: &Value) -> Self {
        let Value::Text(written) = value else {
            return Self {
                op: Op::Equal,
                against: value.clone(),
            };
        };
        let (op, rest) = [
            ("<>", Op::NotEqual),
            ("<=", Op::LessOrEqual),
            (">=", Op::GreaterOrEqual),
            ("<", Op::Less),
            (">", Op::Greater),
            ("=", Op::Equal),
        ]
        .into_iter()
        .find_map(|(prefix, op)| written.strip_prefix(prefix).map(|rest| (op, rest)))
        .unwrap_or((Op::Equal, written.as_str()));
        let against = match rest.trim().parse::<f64>() {
            Ok(n) => Value::Number(n),
            Err(_) => Value::Text(rest.to_owned()),
        };
        Self { op, against }
    }

    fn matches(&self, cell: &Value) -> bool {
        if matches!(cell, Value::Error(_)) {
            return false;
        }
        if matches!(cell, Value::Empty) {
            return match &self.against {
                Value::Text(t) => match self.op {
                    Op::Equal => t.is_empty(),
                    Op::NotEqual => !t.is_empty(),
                    _ => false,
                },
                _ => self.op == Op::NotEqual,
            };
        }
        if is_text(cell) != is_text(&self.against) {
            return self.op == Op::NotEqual;
        }
        let Ok(ordering) = compare(cell, &self.against, false) else {
            return false;
        };
        match self.op {
            Op::Equal => ordering == Ordering::Equal,
            Op::NotEqual => ordering != Ordering::Equal,
            Op::Less => ordering == Ordering::Less,
            Op::LessOrEqual => ordering != Ordering::Greater,
            Op::Greater => ordering == Ordering::Greater,
            _ => ordering != Ordering::Less,
        }
    }
}

/// Round half away from zero, up away from zero, or down toward it, to a
/// number of decimal places, which may be negative. The product is first
/// rounded to nine places, so that `1.005` rounds as it is written and not as
/// binary holds it.
fn round(x: f64, places: f64, how: &str) -> f64 {
    let factor = 10f64.powf(places);
    let scaled = (x * factor * 1e9).round() / 1e9;
    let rounded = match how {
        "ROUNDUP" => scaled.abs().ceil().copysign(scaled),
        "ROUNDDOWN" => scaled.trunc(),
        _ => scaled.round(),
    };
    rounded / factor
}

/// The days from 30 December 1899 to a date.
#[allow(clippy::cast_precision_loss)]
pub(crate) fn serial(year: i64, month: i64, day: i64) -> f64 {
    (days_from_civil(year, month, day) - days_from_civil(1899, 12, 30)) as f64
}

/// The date a count of days from 30 December 1899 falls on.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn civil(serial: f64) -> (i64, i64, i64) {
    let serial = if serial.is_finite() { serial } else { 0.0 };
    civil_from_days(serial.clamp(-1e9, 1e9).floor() as i64 + days_from_civil(1899, 12, 30))
}

/// Days since 1 January 1970, by Howard Hinnant's method.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}
