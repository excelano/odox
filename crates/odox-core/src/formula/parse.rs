//! Reading an `OpenFormula` expression into a tree.
//!
//! What is read is the expression as ODF stores it, `of:=SUM([.A1:.B2])`:
//! references in brackets, `;` between a function's arguments, and strings in
//! double quotes with a quote doubled inside. Anything outside the core this
//! reads, a named range, a whole column, an array, is [`Unsupported`], and the
//! cell keeps the result its producer wrote.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::{Address, Expr, Op, Reference, Unsupported};
use crate::formula::Error;

/// Read a formula as ODF writes it, with or without the `of:` namespace.
///
/// # Errors
///
/// The formula is in another namespace, or uses something this does not read.
pub fn parse(formula: &str) -> Result<Expr, Unsupported> {
    let body = formula
        .strip_prefix("of:")
        .unwrap_or(formula)
        .trim_start()
        .strip_prefix('=')
        .ok_or(Unsupported)?;
    let tokens = tokens(body)?;
    let mut parser = Parser { tokens, at: 0 };
    let expr = parser.comparison()?;
    if parser.at == parser.tokens.len() {
        Ok(expr)
    } else {
        Err(Unsupported)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Text(String),
    Reference(Reference),
    Name(String),
    Error(Error),
    Op(&'static str),
    Open,
    Close,
    Separator,
}

fn tokens(source: &str) -> Result<Vec<Token>, Unsupported> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\n' | '\r' => i += 1,
            '(' => {
                out.push(Token::Open);
                i += 1;
            }
            ')' => {
                out.push(Token::Close);
                i += 1;
            }
            ';' => {
                out.push(Token::Separator);
                i += 1;
            }
            '"' => out.push(Token::Text(quoted(&chars, &mut i)?)),
            '[' => out.push(Token::Reference(reference(&bracketed(&chars, &mut i)?)?)),
            '#' => {
                let rest: String = chars[i..].iter().collect();
                let (error, length) = Error::ALL
                    .iter()
                    .find_map(|error| {
                        rest.starts_with(error.code())
                            .then(|| (*error, error.code().chars().count()))
                    })
                    .ok_or(Unsupported)?;
                out.push(Token::Error(error));
                i += length;
            }
            '0'..='9' | '.' => out.push(Token::Number(number(&chars, &mut i)?)),
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
                {
                    i += 1;
                }
                out.push(Token::Name(
                    chars[start..i].iter().collect::<String>().to_uppercase(),
                ));
            }
            _ => {
                let two: String = chars[i..chars.len().min(i + 2)].iter().collect();
                let op = ["<>", "<=", ">="]
                    .into_iter()
                    .find(|op| two == *op)
                    .or_else(|| {
                        ["+", "-", "*", "/", "^", "&", "=", "<", ">", "%"]
                            .into_iter()
                            .find(|op| op.starts_with(c))
                    })
                    .ok_or(Unsupported)?;
                out.push(Token::Op(op));
                i += op.len();
            }
        }
    }
    Ok(out)
}

/// A string from its opening quote, with a doubled quote standing for one.
fn quoted(chars: &[char], i: &mut usize) -> Result<String, Unsupported> {
    let mut text = String::new();
    *i += 1;
    loop {
        match chars.get(*i) {
            Some('"') if chars.get(*i + 1) == Some(&'"') => {
                text.push('"');
                *i += 2;
            }
            Some('"') => {
                *i += 1;
                return Ok(text);
            }
            Some(c) => {
                text.push(*c);
                *i += 1;
            }
            None => return Err(Unsupported),
        }
    }
}

/// What is between a reference's brackets, a quoted sheet name's `]` included.
fn bracketed(chars: &[char], i: &mut usize) -> Result<String, Unsupported> {
    let mut inside = String::new();
    let mut in_quotes = false;
    *i += 1;
    loop {
        match chars.get(*i) {
            Some('\'') => {
                in_quotes = !in_quotes;
                inside.push('\'');
            }
            Some(']') if !in_quotes => {
                *i += 1;
                return Ok(inside);
            }
            Some(c) => inside.push(*c),
            None => return Err(Unsupported),
        }
        *i += 1;
    }
}

/// A number, with a decimal point and an exponent where it has them.
fn number(chars: &[char], i: &mut usize) -> Result<f64, Unsupported> {
    let start = *i;
    while *i < chars.len() && (chars[*i].is_ascii_digit() || chars[*i] == '.') {
        *i += 1;
    }
    if *i < chars.len() && (chars[*i] == 'E' || chars[*i] == 'e') {
        let mut j = *i + 1;
        if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
            j += 1;
        }
        if j < chars.len() && chars[j].is_ascii_digit() {
            *i = j;
            while *i < chars.len() && chars[*i].is_ascii_digit() {
                *i += 1;
            }
        }
    }
    chars[start..*i]
        .iter()
        .collect::<String>()
        .parse()
        .map_err(|_| Unsupported)
}

/// What is inside a reference's brackets: `.A1`, `$Sheet.$A$1`,
/// `'A sheet'.A1:.B2`.
fn reference(inside: &str) -> Result<Reference, Unsupported> {
    let mut parts = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (at, c) in inside.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            ':' if !quoted => {
                parts.push(&inside[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&inside[start..]);
    match parts.as_slice() {
        [one] => {
            let (sheet, address) = address(one)?;
            Ok(Reference {
                sheet,
                from: address,
                to: address,
            })
        }
        [first, last] => {
            let (sheet, from) = address(first)?;
            let (_, to) = address(last)?;
            Ok(Reference {
                sheet,
                from: Address {
                    row: from.row.min(to.row),
                    column: from.column.min(to.column),
                },
                to: Address {
                    row: from.row.max(to.row),
                    column: from.column.max(to.column),
                },
            })
        }
        _ => Err(Unsupported),
    }
}

/// One end of a reference: the sheet it names, if any, and the cell.
fn address(text: &str) -> Result<(Option<String>, Address), Unsupported> {
    let mut quoted = false;
    let mut dot = None;
    for (at, c) in text.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            '.' if !quoted => dot = Some(at),
            _ => {}
        }
    }
    let dot = dot.ok_or(Unsupported)?;
    let sheet = text[..dot].trim_start_matches('$');
    let sheet = if sheet.is_empty() {
        None
    } else if let Some(inner) = sheet.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        Some(inner.replace("''", "'"))
    } else {
        Some(sheet.to_owned())
    };
    let cell: String = text[dot + 1..].chars().filter(|c| *c != '$').collect();
    let letters: String = cell.chars().take_while(char::is_ascii_alphabetic).collect();
    let digits = &cell[letters.len()..];
    if letters.is_empty() || digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(Unsupported);
    }
    let column = letters
        .to_ascii_uppercase()
        .bytes()
        .try_fold(0usize, |n, b| {
            n.checked_mul(26)?.checked_add(usize::from(b - b'A') + 1)
        })
        .ok_or(Unsupported)?
        - 1;
    let row = digits.parse::<usize>().map_err(|_| Unsupported)?;
    Ok((
        sheet,
        Address {
            row: row.checked_sub(1).ok_or(Unsupported)?,
            column,
        },
    ))
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).cloned();
        self.at += 1;
        token
    }

    fn op(&mut self, ops: &[&'static str]) -> Option<&'static str> {
        match self.peek() {
            Some(Token::Op(op)) if ops.contains(op) => {
                let op = *op;
                self.at += 1;
                Some(op)
            }
            _ => None,
        }
    }

    fn binary(
        &mut self,
        ops: &[&'static str],
        next: fn(&mut Self) -> Result<Expr, Unsupported>,
    ) -> Result<Expr, Unsupported> {
        let mut left = next(self)?;
        while let Some(op) = self.op(ops) {
            let right = next(self)?;
            left = Expr::Binary(Op::of(op), Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn comparison(&mut self) -> Result<Expr, Unsupported> {
        self.binary(&["=", "<>", "<", "<=", ">", ">="], Self::join)
    }

    fn join(&mut self) -> Result<Expr, Unsupported> {
        self.binary(&["&"], Self::sum)
    }

    fn sum(&mut self) -> Result<Expr, Unsupported> {
        self.binary(&["+", "-"], Self::product)
    }

    fn product(&mut self) -> Result<Expr, Unsupported> {
        self.binary(&["*", "/"], Self::power)
    }

    fn power(&mut self) -> Result<Expr, Unsupported> {
        self.binary(&["^"], Self::unary)
    }

    fn unary(&mut self) -> Result<Expr, Unsupported> {
        match self.op(&["-", "+"]) {
            Some("-") => Ok(Expr::Negate(Box::new(self.unary()?))),
            Some(_) => self.unary(),
            None => self.percent(),
        }
    }

    fn percent(&mut self) -> Result<Expr, Unsupported> {
        let mut expr = self.primary()?;
        while self.op(&["%"]).is_some() {
            expr = Expr::Percent(Box::new(expr));
        }
        Ok(expr)
    }

    fn primary(&mut self) -> Result<Expr, Unsupported> {
        match self.next().ok_or(Unsupported)? {
            Token::Number(n) => Ok(Expr::Number(n)),
            Token::Text(text) => Ok(Expr::Text(text)),
            Token::Reference(reference) => Ok(Expr::Reference(reference)),
            Token::Error(error) => Ok(Expr::Error(error)),
            Token::Open => {
                let inner = self.comparison()?;
                match self.next() {
                    Some(Token::Close) => Ok(inner),
                    _ => Err(Unsupported),
                }
            }
            Token::Name(name) => {
                if self.next() != Some(Token::Open) {
                    return Err(Unsupported);
                }
                let mut arguments = Vec::new();
                if self.peek() == Some(&Token::Close) {
                    self.at += 1;
                    return Ok(Expr::Call(name, arguments));
                }
                loop {
                    if matches!(self.peek(), Some(Token::Separator | Token::Close)) {
                        arguments.push(Expr::Missing);
                    } else {
                        arguments.push(self.comparison()?);
                    }
                    match self.next() {
                        Some(Token::Separator) => {}
                        Some(Token::Close) => break,
                        _ => return Err(Unsupported),
                    }
                }
                Ok(Expr::Call(name, arguments))
            }
            _ => Err(Unsupported),
        }
    }
}
