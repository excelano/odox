//! Dates: today, months on, the days of the week, and the time between two
//! days.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::{Context, arity, civil, serial};
use crate::formula::{Error, Expr, Unsupported, Value};

/// The functions this family works out.
pub(super) const NAMES: &[&str] = &[
    "TODAY",
    "NOW",
    "EDATE",
    "EOMONTH",
    "WEEKDAY",
    "NETWORKDAYS",
    "DATEDIF",
];

impl Context<'_> {
    pub(super) fn date_function(&self, name: &str, args: &[Expr]) -> Result<Value, Unsupported> {
        Ok(match name {
            "TODAY" | "NOW" => {
                arity(args, 0, 0)?;
                let now = self.cells.now().ok_or(Unsupported)?;
                if name == "TODAY" {
                    Value::Date(now.floor())
                } else {
                    Value::Date(now)
                }
            }
            "EDATE" | "EOMONTH" => {
                arity(args, 2, 2)?;
                let day = tryv!(self.number_at(args, 0)?).floor();
                let months = tryv!(self.number_at(args, 1)?).trunc();
                let (y, m, d) = civil(day);
                #[allow(clippy::cast_possible_truncation)]
                let total = y * 12 + (m - 1) + months as i64;
                let (y, m) = (total.div_euclid(12), total.rem_euclid(12) + 1);
                let last = days_in_month(y, m);
                Value::Number(if name == "EOMONTH" {
                    serial(y, m, last)
                } else {
                    serial(y, m, d.min(last))
                })
            }
            "WEEKDAY" => self.weekday(args)?,
            "NETWORKDAYS" => self.working_days(args)?,
            "DATEDIF" => self.between(args)?,
            _ => return Err(Unsupported),
        })
    }
}

impl Context<'_> {
    fn weekday(&self, args: &[Expr]) -> Result<Value, Unsupported> {
        arity(args, 1, 2)?;
        let day = tryv!(self.number_at(args, 0)?).floor();
        #[allow(clippy::cast_possible_truncation)]
        let kind = match args.get(1) {
            Some(_) => tryv!(self.number_at(args, 1)?).trunc() as i64,
            None => 1,
        };
        let sunday_first = weekday(day);
        let n = match kind {
            1 => sunday_first + 1,
            2 => (sunday_first + 6) % 7 + 1,
            3 => (sunday_first + 6) % 7,
            11..=17 => (sunday_first - (kind - 10) % 7).rem_euclid(7) + 1,
            _ => return Ok(Value::Error(Error::Argument)),
        };
        #[allow(clippy::cast_precision_loss)]
        Ok(Value::Number(n as f64))
    }

    fn working_days(&self, args: &[Expr]) -> Result<Value, Unsupported> {
        arity(args, 2, 3)?;
        let start = tryv!(self.number_at(args, 0)?).floor();
        let end = tryv!(self.number_at(args, 1)?).floor();
        let mut holidays = Vec::new();
        if let Some(expr) = args.get(2) {
            for value in self.arg(expr)?.cells() {
                match value {
                    Value::Number(n) | Value::Date(n) => holidays.push(n.floor()),
                    Value::Error(error) => return Ok(Value::Error(*error)),
                    _ => {}
                }
            }
        }
        let (low, high, sign) = if start <= end {
            (start, end, 1.0)
        } else {
            (end, start, -1.0)
        };
        let mut n = 0.0;
        let mut day = low;
        while day <= high {
            if !matches!(weekday(day), 0 | 6) && !holidays.contains(&day) {
                n += 1.0;
            }
            day += 1.0;
        }
        Ok(Value::Number(n * sign))
    }

    fn between(&self, args: &[Expr]) -> Result<Value, Unsupported> {
        arity(args, 3, 3)?;
        let start = tryv!(self.number_at(args, 0)?).floor();
        let end = tryv!(self.number_at(args, 1)?).floor();
        let unit = tryv!(self.text_at(args, 2)?).to_lowercase();
        if start > end {
            return Ok(Value::Error(Error::Argument));
        }
        let ((y1, m1, d1), (y2, m2, d2)) = (civil(start), civil(end));
        let months = (y2 - y1) * 12 + (m2 - m1) - i64::from(d2 < d1);
        #[allow(clippy::cast_precision_loss)]
        Ok(Value::Number(match unit.as_str() {
            "d" => end - start,
            "m" => months as f64,
            "y" => (months / 12) as f64,
            "ym" => (months % 12) as f64,
            "md" => {
                let (py, pm) = if d2 >= d1 {
                    (y2, m2)
                } else if m2 == 1 {
                    (y2 - 1, 12)
                } else {
                    (y2, m2 - 1)
                };
                end - serial(py, pm, d1.min(days_in_month(py, pm)))
            }
            "yd" => {
                let mut from = serial(y2, m1, d1.min(days_in_month(y2, m1)));
                if from > end {
                    from = serial(y2 - 1, m1, d1.min(days_in_month(y2 - 1, m1)));
                }
                end - from
            }
            _ => return Ok(Value::Error(Error::Argument)),
        }))
    }
}

/// The day of the week, Sunday nought.
fn weekday(day: f64) -> i64 {
    #[allow(clippy::cast_possible_truncation)]
    let days = day.floor() as i64;
    (days + 6).rem_euclid(7)
}

/// How many days a month has.
fn days_in_month(year: i64, month: i64) -> i64 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    #[allow(clippy::cast_possible_truncation)]
    let days = (serial(next_year, next_month, 1) - serial(year, month, 1)) as i64;
    days
}
