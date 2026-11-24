//! Choosing among values, and asking what a value is.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::cmp::Ordering;

use super::{Context, arity, compare, truth};
use crate::formula::{Error, Expr, Unsupported, Value};

/// The functions this family works out.
pub(super) const NAMES: &[&str] = &["IFS", "SWITCH", "ISBLANK", "ISNUMBER", "ISTEXT", "ISERROR"];

impl Context<'_> {
    pub(super) fn logic_function(&self, name: &str, args: &[Expr]) -> Result<Value, Unsupported> {
        Ok(match name {
            "IFS" => {
                if args.is_empty() || !args.len().is_multiple_of(2) {
                    return Err(Unsupported);
                }
                for pair in args.chunks(2) {
                    if tryv!(truth(&self.scalar(&pair[0])?)) {
                        return self.scalar(&pair[1]);
                    }
                }
                Value::Error(Error::NotAvailable)
            }
            "SWITCH" => {
                if args.len() < 3 {
                    return Err(Unsupported);
                }
                let value = self.scalar(&args[0])?;
                if let Value::Error(error) = value {
                    return Ok(Value::Error(error));
                }
                let rest = &args[1..];
                for pair in rest.chunks(2) {
                    let [case, result] = pair else {
                        return self.scalar(&pair[0]);
                    };
                    let case = self.scalar(case)?;
                    let equal = match (&value, &case) {
                        (Value::Text(a), Value::Text(b)) => {
                            compare(
                                &Value::Text(a.clone()),
                                &Value::Text(b.clone()),
                                self.cells.case_sensitive(),
                            ) == Ok(Ordering::Equal)
                        }
                        (Value::Text(_), _) | (_, Value::Text(_)) => false,
                        _ => compare(&value, &case, false) == Ok(Ordering::Equal),
                    };
                    if equal {
                        return self.scalar(result);
                    }
                }
                Value::Error(Error::NotAvailable)
            }
            "ISBLANK" | "ISNUMBER" | "ISTEXT" | "ISERROR" => {
                arity(args, 1, 1)?;
                let value = self.scalar(&args[0])?;
                Value::Boolean(match name {
                    "ISBLANK" => matches!(value, Value::Empty),
                    "ISNUMBER" => matches!(value, Value::Number(_) | Value::Date(_)),
                    "ISTEXT" => matches!(value, Value::Text(_)),
                    _ => matches!(value, Value::Error(_)),
                })
            }
            _ => return Err(Unsupported),
        })
    }
}
