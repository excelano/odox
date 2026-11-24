//! Text functions: formatting a number, reading a number, finding and
//! replacing.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::{Context, Matching, arity, count_value, pattern, whole};
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
                    Value::Boolean(b) => Value::Boolean(b),
                    Value::Empty => Value::Number(0.0),
                    Value::Error(error) => Value::Error(error),
                    Value::Text(_) => Value::Error(Error::Argument),
                }
            }
            "FIND" | "SEARCH" => self.find_text(name, args)?,
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

    /// Where text is found in other text, from a place: by `FIND` with its
    /// case, and by `SEARCH` without it and as the document's calculation
    /// settings read a pattern.
    fn find_text(&self, name: &str, args: &[Expr]) -> Result<Value, Unsupported> {
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
        let from = whole(start) - 1;
        if name == "SEARCH" {
            let matching = Matching {
                whole: false,
                ..self.matching()
            };
            let rest: String = chars[from..].iter().collect();
            let found = pattern(&wanted, matching)
                .ok_or(Unsupported)?
                .find(&rest)
                .map(|m| from + rest[..m.start()].chars().count());
            return Ok(match found {
                Some(at) => count_value(at + 1),
                None => Value::Error(Error::Value),
            });
        }
        if wanted.is_empty() {
            return Ok(Value::Error(Error::Value));
        }
        let needle: Vec<char> = wanted.chars().collect();
        let found = (from..=chars.len().saturating_sub(needle.len()))
            .find(|&at| chars.get(at..at + needle.len()) == Some(&needle[..]));
        Ok(match found {
            Some(at) if chars.len() >= needle.len() => count_value(at + 1),
            _ => Value::Error(Error::Value),
        })
    }
}
