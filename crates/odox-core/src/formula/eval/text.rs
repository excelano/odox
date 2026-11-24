//! Text functions: formatting a number, reading a number, finding and
//! replacing.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::{Context, arity, count_value, whole};
use crate::data_style::DataStyle;
use crate::formula::{Error, Expr, Unsupported, Value};

/// The functions this family works out.
pub(super) const NAMES: &[&str] = &["TEXT", "VALUE", "FIND", "SEARCH", "SUBSTITUTE", "REPLACE"];

impl Context<'_> {
    pub(super) fn text_function(&self, name: &str, args: &[Expr]) -> Result<Value, Unsupported> {
        Ok(match name {
            "TEXT" => {
                arity(args, 2, 2)?;
                let value = self.converted(self.scalar(&args[0])?);
                let code = tryv!(self.text_at(args, 1)?);
                match value {
                    Value::Number(x) | Value::Date(x) => {
                        let style = DataStyle::from_code(&code).ok_or(Unsupported)?;
                        let shown = style
                            .format(&crate::doc::Value::Number(x), |_| None)
                            .ok_or(Unsupported)?;
                        Value::Text(shown)
                    }
                    Value::Boolean(_) | Value::Empty => return Err(Unsupported),
                    Value::Text(written) => Value::Text(written),
                    Value::Error(error) => Value::Error(error),
                }
            }
            "VALUE" => {
                arity(args, 1, 1)?;
                match self.converted(self.scalar(&args[0])?) {
                    Value::Number(x) | Value::Date(x) => Value::Number(x),
                    Value::Error(error) => Value::Error(error),
                    _ => Value::Error(Error::Argument),
                }
            }
            "FIND" | "SEARCH" => {
                arity(args, 2, 3)?;
                let wanted = tryv!(self.text_at(args, 0)?);
                let within = tryv!(self.text_at(args, 1)?);
                let start = match args.get(2) {
                    Some(_) => tryv!(self.number_at(args, 2)?).trunc(),
                    None => 1.0,
                };
                let chars: Vec<char> = within.chars().collect();
                if start < 1.0 || whole(start) > chars.len().max(1) {
                    return Ok(Value::Error(Error::Value));
                }
                let fold = |text: &str| -> Vec<char> {
                    if name == "SEARCH" {
                        text.to_lowercase().chars().collect()
                    } else {
                        text.chars().collect()
                    }
                };
                let (haystack, needle) = (fold(&within), fold(&wanted));
                let from = whole(start) - 1;
                let found = (from..=haystack.len().saturating_sub(needle.len()))
                    .find(|&at| haystack.get(at..at + needle.len()) == Some(&needle[..]));
                match found {
                    Some(at) if haystack.len() >= needle.len() => count_value(at + 1),
                    _ => Value::Error(Error::Value),
                }
            }
            "SUBSTITUTE" => {
                arity(args, 3, 4)?;
                let source = tryv!(self.text_at(args, 0)?);
                let old = tryv!(self.text_at(args, 1)?);
                let new = tryv!(self.text_at(args, 2)?);
                if old.is_empty() {
                    return Ok(Value::Text(source));
                }
                if args.len() < 4 {
                    return Ok(Value::Text(source.replace(&old, &new)));
                }
                let nth = tryv!(self.number_at(args, 3)?).trunc();
                if nth < 1.0 {
                    return Ok(Value::Error(Error::Argument));
                }
                match source.match_indices(&old).nth(whole(nth) - 1) {
                    Some((at, _)) => Value::Text(format!(
                        "{}{new}{}",
                        &source[..at],
                        &source[at + old.len()..]
                    )),
                    None => Value::Text(source),
                }
            }
            "REPLACE" => {
                arity(args, 4, 4)?;
                let source: Vec<char> = tryv!(self.text_at(args, 0)?).chars().collect();
                let (start, n) = (
                    tryv!(self.number_at(args, 1)?).trunc(),
                    tryv!(self.number_at(args, 2)?).trunc(),
                );
                let new = tryv!(self.text_at(args, 3)?);
                if start < 1.0 || n < 0.0 {
                    return Ok(Value::Error(Error::Argument));
                }
                let from = (whole(start) - 1).min(source.len());
                let to = (from + whole(n)).min(source.len());
                let mut out: String = source[..from].iter().collect();
                out.push_str(&new);
                out.extend(&source[to..]);
                Value::Text(out)
            }
            _ => return Err(Unsupported),
        })
    }
}
