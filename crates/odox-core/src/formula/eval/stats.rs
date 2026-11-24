//! Statistics over the numbers among a function's arguments.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::{Arg, Context, arity, numbers};
use crate::formula::{Error, Expr, Unsupported, Value};

/// The functions this family works out.
pub(super) const NAMES: &[&str] = &["MEDIAN", "STDEV", "RANK"];

impl Context<'_> {
    pub(super) fn stats_function(&self, name: &str, args: &[Expr]) -> Result<Value, Unsupported> {
        let all =
            || -> Result<Vec<Arg>, Unsupported> { args.iter().map(|a| self.arg(a)).collect() };
        Ok(match name {
            "MEDIAN" => {
                let mut found = tryv!(numbers(&all()?, Error::Value));
                if found.is_empty() {
                    return Ok(Value::Error(Error::Number));
                }
                found.sort_by(f64::total_cmp);
                let half = found.len() / 2;
                Value::Number(if found.len() % 2 == 1 {
                    found[half]
                } else {
                    f64::midpoint(found[half - 1], found[half])
                })
            }
            "STDEV" => {
                let found = tryv!(numbers(&all()?, Error::ArgumentKind));
                if found.len() < 2 {
                    return Ok(Value::Error(Error::DivisionByZero));
                }
                #[allow(clippy::cast_precision_loss)]
                let n = found.len() as f64;
                let mean = found.iter().sum::<f64>() / n;
                let squares: f64 = found.iter().map(|x| (x - mean).powi(2)).sum();
                Value::Number((squares / (n - 1.0)).sqrt())
            }
            "RANK" => {
                arity(args, 2, 3)?;
                let x = tryv!(self.number_at(args, 0)?);
                let mut list = Vec::new();
                for value in self.arg(&args[1])?.cells() {
                    match value {
                        Value::Number(n) | Value::Date(n) => list.push(*n),
                        Value::Error(error) => return Ok(Value::Error(*error)),
                        _ => {}
                    }
                }
                let ascending = match args.get(2) {
                    Some(_) => tryv!(self.number_at(args, 2)?) != 0.0,
                    None => false,
                };
                if !list.contains(&x) {
                    return Ok(Value::Error(Error::NotAvailable));
                }
                let before = list
                    .iter()
                    .filter(|&&n| if ascending { n < x } else { n > x })
                    .count();
                #[allow(clippy::cast_precision_loss)]
                Value::Number((before + 1) as f64)
            }
            _ => return Err(Unsupported),
        })
    }
}
