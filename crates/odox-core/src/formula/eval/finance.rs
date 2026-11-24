//! Money over time: a loan's payment, a present value, a rate of return.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::{Arg, Context, arity};
use crate::formula::{Error, Expr, Unsupported, Value};

/// How close two guesses at a rate of return have to come, and how many are
/// made, as `LibreOffice` makes them.
const SETTLED: f64 = 1e-7;
const GUESSES: usize = 20;

/// The functions this family works out.
pub(super) const NAMES: &[&str] = &["PMT", "NPV", "IRR"];

impl Context<'_> {
    pub(super) fn finance_function(&self, name: &str, args: &[Expr]) -> Result<Value, Unsupported> {
        Ok(match name {
            "PMT" => {
                arity(args, 3, 5)?;
                let rate = tryv!(self.number_at(args, 0)?);
                let periods = tryv!(self.number_at(args, 1)?);
                let present = tryv!(self.number_at(args, 2)?);
                let future = match args.get(3) {
                    Some(Expr::Missing) | None => 0.0,
                    Some(_) => tryv!(self.number_at(args, 3)?),
                };
                let at_start = match args.get(4) {
                    Some(Expr::Missing) | None => false,
                    Some(_) => tryv!(self.number_at(args, 4)?) != 0.0,
                };
                if periods == 0.0 {
                    return Ok(Value::Error(Error::Argument));
                }
                Value::Number(if rate == 0.0 {
                    -(present + future) / periods
                } else {
                    let grown = (1.0 + rate).powf(periods);
                    let when = if at_start { 1.0 + rate } else { 1.0 };
                    -(rate * (future + present * grown)) / (when * (grown - 1.0))
                })
            }
            "NPV" => {
                if args.len() < 2 {
                    return Err(Unsupported);
                }
                let rate = tryv!(self.number_at(args, 0)?);
                let mut sum = 0.0;
                let mut period = 0;
                for expr in &args[1..] {
                    match self.arg(expr)? {
                        Arg::Scalar(value) => {
                            period += 1;
                            sum += tryv!(self.number(&value)) / (1.0 + rate).powi(period);
                        }
                        range @ Arg::Range(_) => {
                            for value in range.cells() {
                                match value {
                                    Value::Number(n) | Value::Date(n) => {
                                        period += 1;
                                        sum += n / (1.0 + rate).powi(period);
                                    }
                                    Value::Error(error) => return Ok(Value::Error(*error)),
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                Value::Number(sum)
            }
            "IRR" => {
                arity(args, 1, 2)?;
                let mut flows = Vec::new();
                for value in self.arg(&args[0])?.cells() {
                    match value {
                        Value::Number(n) | Value::Date(n) => flows.push(*n),
                        Value::Error(error) => return Ok(Value::Error(*error)),
                        _ => {}
                    }
                }
                let guess = match args.get(1) {
                    Some(_) => tryv!(self.number_at(args, 1)?),
                    None => 0.1,
                };
                match rate_of_return(&flows, guess) {
                    Some(rate) => Value::Number(rate),
                    None => Value::Error(Error::Convergence),
                }
            }
            _ => return Err(Unsupported),
        })
    }
}

/// The rate at which cash flows come to nothing, by Newton's method from a
/// guess; `None` where the guesses do not settle.
fn rate_of_return(flows: &[f64], guess: f64) -> Option<f64> {
    let mut rate = guess;
    for _ in 0..GUESSES {
        let (mut value, mut slope) = (0.0, 0.0);
        for (period, flow) in flows.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let period = period as f64;
            value += flow / (1.0 + rate).powf(period);
            slope -= period * flow / (1.0 + rate).powf(period + 1.0);
        }
        if slope == 0.0 || !slope.is_finite() {
            return None;
        }
        let next = rate - value / slope;
        if (next - rate).abs() < SETTLED {
            return next.is_finite().then_some(next);
        }
        rate = next;
    }
    None
}
