//! Working a formula out.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::cmp::Ordering;

use super::{Address, Error, Expr, Op, Reference, Unsupported, Value};

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

    /// Whether a criterion's text may hold the wildcards `*` and `?`, as the
    /// document's calculation settings say.
    fn wildcards(&self) -> bool {
        false
    }

    /// Whether text compares with its case, as the document's calculation
    /// settings say; ODF's default is that it does.
    fn case_sensitive(&self) -> bool {
        true
    }
}

/// Work out a formula standing in a cell of a sheet.
///
/// # Errors
///
/// The formula, or a cell it reads, uses something outside the core.
pub fn evaluate(
    expr: &Expr,
    sheet: usize,
    at: Address,
    cells: &dyn Cells,
) -> Result<Value, Unsupported> {
    let context = Context { sheet, at, cells };
    Ok(match context.scalar(expr)? {
        Value::Number(n) | Value::Date(n) if !n.is_finite() => Value::Error(Error::Number),
        value => value,
    })
}

struct Context<'a> {
    sheet: usize,
    at: Address,
    cells: &'a dyn Cells,
}

/// An argument as a function sees it: a value, or the cells of a reference.
enum Arg {
    Scalar(Value),
    Range(Vec<Vec<Value>>),
}

impl Arg {
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
    fn sheet_of(&self, reference: &Reference) -> Option<usize> {
        match &reference.sheet {
            Some(name) => self.cells.sheet(name),
            None => Some(self.sheet),
        }
    }

    fn range(&self, reference: &Reference) -> Result<Vec<Vec<Value>>, Unsupported> {
        let Some(sheet) = self.sheet_of(reference) else {
            return Ok(vec![vec![Value::Error(Error::Reference)]]);
        };
        (reference.from.row..=reference.to.row)
            .map(|row| {
                (reference.from.column..=reference.to.column)
                    .map(|column| self.cells.value(sheet, row, column))
                    .collect()
            })
            .collect()
    }

    /// A reference where one value is wanted: its one cell, or the cell of a
    /// column on the formula's row or of a row in the formula's column.
    fn intersection(&self, reference: &Reference) -> Result<Value, Unsupported> {
        let Some(sheet) = self.sheet_of(reference) else {
            return Ok(Value::Error(Error::Reference));
        };
        let (from, to) = (reference.from, reference.to);
        let row = if from.row == to.row {
            Some(from.row)
        } else {
            (from.row..=to.row)
                .contains(&self.at.row)
                .then_some(self.at.row)
        };
        let column = if from.column == to.column {
            Some(from.column)
        } else {
            (from.column..=to.column)
                .contains(&self.at.column)
                .then_some(self.at.column)
        };
        match (row, column) {
            (Some(row), Some(column)) if from.row == to.row || from.column == to.column => {
                self.cells.value(sheet, row, column)
            }
            _ => Ok(Value::Error(Error::Value)),
        }
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
            Expr::Reference(reference) => self.intersection(reference)?,
            Expr::Negate(inner) => Value::Number(-tryv!(number(&self.scalar(inner)?))),
            Expr::Percent(inner) => Value::Number(tryv!(number(&self.scalar(inner)?)) / 100.0),
            Expr::Binary(op, left, right) => {
                let (left, right) = (self.scalar(left)?, self.scalar(right)?);
                binary(*op, &left, &right, self.cells.case_sensitive())
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
            "SUM" => Value::Number(
                tryv!(numbers(&all()?, Error::Value))
                    .iter()
                    .fold(0.0, |a, b| a + b),
            ),
            "PRODUCT" => {
                let found = tryv!(numbers(&all()?, Error::Value));
                Value::Number(if found.is_empty() {
                    0.0
                } else {
                    found.iter().product()
                })
            }
            "AVERAGE" => {
                let found = tryv!(numbers(&all()?, Error::Value));
                if found.is_empty() {
                    Value::Error(Error::DivisionByZero)
                } else {
                    #[allow(clippy::cast_precision_loss)]
                    Value::Number(found.iter().sum::<f64>() / found.len() as f64)
                }
            }
            "MIN" | "MAX" => {
                if args.is_empty() {
                    return Ok(Value::Error(Error::ArgumentMissing));
                }
                let pick = if name == "MIN" { f64::min } else { f64::max };
                Value::Number(
                    tryv!(numbers(&all()?, Error::ArgumentKind))
                        .into_iter()
                        .reduce(pick)
                        .unwrap_or(0.0),
                )
            }
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
                Value::Number(approx(tryv!(num(0)?)).floor())
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
                    false,
                )
            }
            "SQRT" => {
                count(1, 1)?;
                let x = tryv!(num(0)?);
                if x < 0.0 {
                    Value::Error(Error::Argument)
                } else {
                    Value::Number(x.sqrt())
                }
            }
            "CONCATENATE" => {
                let mut out = String::new();
                for arg in args {
                    out.push_str(&tryv!(text(&self.scalar(arg)?)));
                }
                Value::Text(out)
            }
            "CONCAT" => {
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
                    return Ok(Value::Error(Error::Argument));
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
                    return Ok(Value::Error(Error::Argument));
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
                Value::Text(tryv!(txt(0)?).replace('ß', "ẞ").to_uppercase())
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
                if !(0.0..=32767.0).contains(&y) {
                    return Ok(Value::Error(Error::Argument));
                }
                let months = y * 12.0 + (m - 1.0);
                let (y, m) = ((months / 12.0).floor(), months.rem_euclid(12.0) + 1.0);
                #[allow(clippy::cast_possible_truncation)]
                let first = serial(y as i64, m as i64, 1);
                let date = first + d - 1.0;
                if date < GREGORIAN {
                    return Ok(Value::Error(Error::Value));
                }
                Value::Date(date)
            }
            "YEAR" | "MONTH" | "DAY" => {
                count(1, 1)?;
                let (y, m, d) = civil(tryv!(num(0)?));
                #[allow(clippy::cast_precision_loss)]
                Value::Number(match name {
                    "YEAR" => (if y > 0 { y } else { y - 1 }).clamp(-32768, 32767) as f64,
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
                    return Ok(Value::Error(Error::Argument));
                }
                let column = whole(column) - 1;
                if rows.first().is_none_or(|row| column >= row.len()) {
                    return Ok(Value::Error(Error::Argument));
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
                    let exact = Criterion::of(&wanted, self.cells.wildcards());
                    rows.iter().find(|row| {
                        same_kind(&row[0], &wanted)
                            && match &wanted {
                                Value::Text(_) => exact.matches(&row[0]),
                                _ => compare(&row[0], &wanted, false) == Ok(Ordering::Equal),
                            }
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
                let criterion = Criterion::of(&self.scalar(&args[1])?, self.cells.wildcards());
                let values = match args.get(2) {
                    Some(Expr::Reference(reference)) => {
                        let from = reference.from;
                        let width = range.first().map_or(1, Vec::len);
                        self.range(&Reference {
                            sheet: reference.sheet.clone(),
                            from,
                            to: Address {
                                row: from.row + range.len().max(1) - 1,
                                column: from.column + width.max(1) - 1,
                            },
                        })?
                    }
                    Some(_) => return Ok(Value::Error(Error::ArgumentKind)),
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
        Value::Text(t) => t
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .ok_or(Error::Value),
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

/// A number as a spreadsheet writes it into text, measured against
/// `LibreOffice`: an integer below 2^53 in full, and otherwise fifteen
/// significant digits, in decimals from a hundred-trillionth up to a
/// quadrillion and beyond that with an exponent of at least three digits,
/// `1E+020`, `1.5E-015`.
fn spelled(n: f64) -> String {
    const EXACT: f64 = 9_007_199_254_740_992.0;
    let n = n + 0.0;
    if n.fract() == 0.0 && n.abs() < EXACT {
        return format!("{n:.0}");
    }
    let (digits, exponent) = fifteen_digits(n.abs());
    let digits = digits.trim_end_matches('0');
    let sign = if n < 0.0 { "-" } else { "" };
    if !(-14..15).contains(&exponent) {
        let (first, rest) = digits.split_at(1);
        let point = if rest.is_empty() { "" } else { "." };
        let exponent_sign = if exponent < 0 { '-' } else { '+' };
        return format!(
            "{sign}{first}{point}{rest}E{exponent_sign}{:03}",
            exponent.unsigned_abs()
        );
    }
    if exponent < 0 {
        let zeros = "0".repeat(exponent.unsigned_abs() as usize - 1);
        return format!("{sign}0.{zeros}{digits}");
    }
    let point = exponent.unsigned_abs() as usize + 1;
    if digits.len() <= point {
        format!("{sign}{digits:0<point$}")
    } else {
        format!("{sign}{}.{}", &digits[..point], &digits[point..])
    }
}

/// A positive number's first fifteen significant digits, the last rounded
/// half away from zero, and the power of ten of the first.
fn fifteen_digits(x: f64) -> (String, i32) {
    let written = format!("{x:.16e}");
    let (mantissa, exponent) = written.split_once('e').unwrap_or((&written, "0"));
    let mut exponent: i32 = exponent.parse().unwrap_or(0);
    let all: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    let mut digits = all[..15].to_vec();
    if all[15] >= b'5' {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, b'1');
                digits.pop();
                exponent += 1;
                break;
            }
            i -= 1;
            if digits[i] == b'9' {
                digits[i] = b'0';
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    (String::from_utf8(digits).unwrap_or_default(), exponent)
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
/// out, and in a reference only the cells that hold one. Text written or
/// worked out is the error given, even where it reads as a number.
fn numbers(args: &[Arg], text: Error) -> Result<Vec<f64>, Error> {
    let mut out = Vec::new();
    for arg in args {
        match arg {
            Arg::Scalar(Value::Empty) => {}
            Arg::Scalar(Value::Text(_)) => return Err(text),
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

fn binary(op: Op, left: &Value, right: &Value, case: bool) -> Value {
    let arithmetic = |f: fn(f64, f64) -> Value| match (number(left), number(right)) {
        (Ok(a), Ok(b)) => f(a, b),
        (Err(error), _) | (_, Err(error)) => Value::Error(error),
    };
    match op {
        Op::Add => arithmetic(|a, b| Value::Number(sum(a, b))),
        Op::Subtract => arithmetic(|a, b| Value::Number(sum(a, -b))),
        Op::Multiply => arithmetic(|a, b| Value::Number(a * b)),
        Op::Divide => arithmetic(|a, b| {
            if b == 0.0 {
                Value::Error(Error::DivisionByZero)
            } else {
                Value::Number(a / b)
            }
        }),
        Op::Power => arithmetic(|a, b| {
            let n = if a < 0.0 && b.fract() != 0.0 && odd_root(b) {
                -(-a).powf(b)
            } else {
                a.powf(b)
            };
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
        _ => match compare(left, right, case) {
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

/// Whether a power is the reciprocal of an odd whole number, as `1/3` is,
/// which takes a root of a negative number.
fn odd_root(power: f64) -> bool {
    let root = approx(1.0 / power);
    #[allow(clippy::cast_possible_truncation)]
    let odd = root.fract() == 0.0 && root.abs() < 1e15 && (root as i64) % 2 != 0;
    odd
}

fn is_text(value: &Value) -> bool {
    matches!(value, Value::Text(_))
}

fn same_kind(a: &Value, b: &Value) -> bool {
    is_text(a) == is_text(b) && !matches!(a, Value::Empty | Value::Error(_))
}

/// Two values in order: numbers before text, an empty cell as zero beside a
/// number and as empty text beside text, text in alphabetical order with its
/// case deciding only between the same letters and only where `case` asks,
/// and numbers that differ only past the fifteenth significant digit the
/// same, as a spreadsheet compares them.
fn compare(left: &Value, right: &Value, case: bool) -> Result<Ordering, Error> {
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
            let order = a.to_lowercase().cmp(&b.to_lowercase());
            Ok(if order == Ordering::Equal && case {
                b.cmp(&a)
            } else {
                order
            })
        }
        (Value::Text(_), _) => Ok(Ordering::Greater),
        (_, Value::Text(_)) => Ok(Ordering::Less),
        _ => {
            let (a, b) = (number(left)?, number(right)?);
            if near(a, b) {
                return Ok(Ordering::Equal);
            }
            Ok(a.partial_cmp(&b).unwrap_or(Ordering::Equal))
        }
    }
}

/// A criterion of `SUMIF` and `COUNTIF`: a value, or a comparison written as
/// text, `">2"`, `"<>apple"`, `"ap*"` where the document allows wildcards.
struct Criterion {
    op: Op,
    against: Value,
    /// The text after the comparison, which a text cell is compared with.
    written: String,
    wildcards: bool,
}

impl Criterion {
    fn of(value: &Value, wildcards: bool) -> Self {
        let Value::Text(written) = value else {
            return Self {
                op: Op::Equal,
                against: value.clone(),
                written: text(value).unwrap_or_default(),
                wildcards: false,
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
            Ok(n) if n.is_finite() => Value::Number(n),
            _ => Value::Text(rest.to_owned()),
        };
        Self {
            op,
            against,
            written: rest.to_owned(),
            wildcards,
        }
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
        if let Value::Text(t) = cell
            && matches!(self.op, Op::Equal | Op::NotEqual)
        {
            let fold = str::to_lowercase;
            let same = if self.wildcards {
                glob(&fold(&self.written), &fold(t))
            } else {
                fold(t) == fold(&self.written)
            };
            return same == (self.op == Op::Equal);
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
/// number of decimal places, which may be negative. The scaled number is
/// first taken to fifteen significant digits, so that `1.005` rounds as it is
/// written and not as binary holds it, and `0.4999999999` still rounds down.
fn round(x: f64, places: f64, how: &str) -> f64 {
    if !x.is_finite() || places > 308.0 {
        return x;
    }
    if places < -308.0 {
        return 0.0;
    }
    let factor = 10f64.powf(places.abs());
    let scaled = if places >= 0.0 {
        x * factor
    } else {
        x / factor
    };
    if !scaled.is_finite() {
        return x;
    }
    let scaled = approx(scaled);
    let rounded = match how {
        "ROUNDUP" => scaled.abs().ceil().copysign(scaled),
        "ROUNDDOWN" => scaled.trunc(),
        _ => scaled.round(),
    };
    let back = if places >= 0.0 {
        rounded / factor
    } else {
        rounded * factor
    };
    back + 0.0
}

/// A number taken to fifteen significant digits, the most a spreadsheet
/// shows, which is how it rounds and compares.
pub(crate) fn approx(x: f64) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    format!("{x:.14e}").parse().unwrap_or(x)
}

/// Whether two numbers are the same but for the last bits of binary.
fn near(a: f64, b: f64) -> bool {
    let difference = (a - b).abs();
    if difference == 0.0 {
        return true;
    }
    let tolerance = 2f64.powi(-48);
    difference < a.abs() * tolerance && difference < b.abs() * tolerance
}

/// A sum that is zero where its two terms cancel but for the last bits of
/// binary, as `0.3 - 0.1 - 0.2` is.
fn sum(a: f64, b: f64) -> f64 {
    if ((a < 0.0 && b > 0.0) || (a > 0.0 && b < 0.0)) && near(a, -b) {
        return 0.0;
    }
    a + b
}

/// Whether text matches a pattern in which `*` stands for any run of
/// characters, `?` for any one, and `~` makes the next one plain.
fn glob(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, t));
                p += 1;
                continue;
            }
            Some('?') => {
                p += 1;
                t += 1;
                continue;
            }
            Some('~') if pattern.get(p + 1) == Some(&text[t]) => {
                p += 2;
                t += 1;
                continue;
            }
            Some(&c) if c == text[t] => {
                p += 1;
                t += 1;
                continue;
            }
            _ => {}
        }
        match star {
            Some((at, from)) => {
                p = at + 1;
                t = from + 1;
                star = Some((at, from + 1));
            }
            None => return false,
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

/// The first day of the Gregorian calendar, 15 October 1582, as days from 30
/// December 1899: `DATE` refuses a day before it, and a day before it is
/// shown in the Julian calendar.
pub(crate) const GREGORIAN: f64 = -115_858.0;

/// A date as ODF writes it, `2026-11-05`, where a year before the first is
/// written with a minus and counted without a year zero, as `LibreOffice`
/// writes one: the year -60 of the proleptic calendar is `-0061`.
pub(crate) fn iso_day((year, month, day): (i64, i64, i64)) -> String {
    if year > 0 {
        format!("{year:04}-{month:02}-{day:02}")
    } else {
        format!("-{:04}-{month:02}-{day:02}", 1 - year)
    }
}

/// A date written as [`iso_day`] writes it, read back.
pub(crate) fn read_iso_day(text: &str) -> Option<(i64, i64, i64)> {
    let (before, text) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let mut fields = text.splitn(3, '-');
    let mut next = |width: usize| fields.next()?.get(..width)?.parse::<i64>().ok();
    let (year, month, day) = (next(4)?, next(2)?, next(2)?);
    Some((if before { 1 - year } else { year }, month, day))
}

/// The date a count of days from 30 December 1899 falls on in the Julian
/// calendar, by Richards' method.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn julian(serial: f64) -> (i64, i64, i64) {
    let serial = if serial.is_finite() { serial } else { 0.0 };
    let day_number = serial.clamp(-1e9, 1e9).floor() as i64 + 2_415_019;
    let c = day_number + 32_082;
    let d = (4 * c + 3).div_euclid(1461);
    let e = c - (1461 * d).div_euclid(4);
    let m = (5 * e + 2).div_euclid(153);
    let day = e - (153 * m + 2).div_euclid(5) + 1;
    (d - 4800 + m / 10, m + 3 - 12 * (m / 10), day)
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
