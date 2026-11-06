//! A formula as a person types and reads it, and as ODF stores it.
//!
//! Typed, a reference is bare, `A1`, `$A$1`, `A1:B3`, and names its sheet
//! with a dot or an exclamation mark, `Data.B2`, `Data!B2`, `'My sheet'.A1`;
//! a function's arguments are parted by `;` or `,`. Stored, every reference
//! is in brackets with a dot before its cell, `[.A1]`, `[$Data.B2:.B6]`, and
//! arguments are parted by `;`. Read back, a formula is written as
//! `LibreOffice` shows one: `=SUM(A1:A3)`, `=$Data.B2*2`.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::{Unsupported, parse};

/// A typed formula, `=` first, as ODF stores it, `of:=` first.
///
/// # Errors
///
/// The formula cannot be read, or reads as something outside the core.
pub fn to_stored(typed: &str) -> Result<String, Unsupported> {
    let body = typed.trim().strip_prefix('=').ok_or(Unsupported)?;
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::from("of:=");
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '"' {
                    if chars.get(i + 1) == Some(&'"') {
                        i += 2;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            if i >= chars.len() {
                return Err(Unsupported);
            }
            i += 1;
            out.extend(&chars[start..i]);
        } else if c.is_whitespace() {
            i += 1;
        } else if c == ',' || c == ';' {
            out.push(';');
            i += 1;
        } else if let Some((reference, length)) = reference_at(&chars[i..]) {
            out.push_str(&reference);
            i += length;
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            let name: String = chars[start..i].iter().collect::<String>().to_uppercase();
            let called = chars[i..].iter().find(|c| !c.is_whitespace()) == Some(&'(');
            match name.as_str() {
                "TRUE" | "FALSE" if !called => {
                    out.push_str(&name);
                    out.push_str("()");
                }
                _ if called => out.push_str(&name),
                _ => return Err(Unsupported),
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    parse(&out)?;
    Ok(out)
}

/// A stored formula as a person reads it: `of:=SUM([.A1:.A3])` as
/// `=SUM(A1:A3)`.
pub fn to_typed(stored: &str) -> String {
    let body = stored.strip_prefix("of:").unwrap_or(stored);
    let mut out = String::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push('"');
                for c in chars.by_ref() {
                    out.push(c);
                    if c == '"' {
                        break;
                    }
                }
            }
            '[' => {
                let mut inside = String::new();
                let mut quoted = false;
                for c in chars.by_ref() {
                    match c {
                        '\'' => {
                            quoted = !quoted;
                            inside.push(c);
                        }
                        ']' if !quoted => break,
                        _ => inside.push(c),
                    }
                }
                out.push_str(&readable(&inside));
            }
            _ => out.push(c),
        }
    }
    out
}

/// The inside of a stored reference as a person reads it: the dot before a
/// cell on the formula's own sheet goes, and a range's second end loses a
/// sheet it repeats.
fn readable(inside: &str) -> String {
    inside
        .split(':')
        .enumerate()
        .map(|(index, end)| {
            let end = end.strip_prefix('.').unwrap_or(end);
            if index > 0 {
                end.rsplit_once('.')
                    .map_or(end, |(_, cell)| cell)
                    .to_owned()
            } else {
                end.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(":")
}

/// A typed reference at the start of some text, stored, and how many
/// characters it took.
fn reference_at(chars: &[char]) -> Option<(String, usize)> {
    let (first, mut length) = end_at(chars)?;
    let mut stored = format!("[{}", first.stored());
    if chars.get(length) == Some(&':')
        && let Some((second, more)) = end_at(&chars[length + 1..])
    {
        stored.push(':');
        stored.push_str(
            &Address {
                sheet: second.sheet.filter(|s| Some(s) != first.sheet.as_ref()),
                ..second
            }
            .stored(),
        );
        length += 1 + more;
    }
    stored.push(']');
    let next = chars.get(length);
    if next.is_some_and(|c| c.is_alphanumeric() || *c == '(' || *c == '_') {
        return None;
    }
    Some((stored, length))
}

struct Address {
    sheet: Option<String>,
    cell: String,
}

impl Address {
    fn stored(&self) -> String {
        match &self.sheet {
            Some(sheet) if sheet.chars().all(|c| c.is_alphanumeric() || c == '_') => {
                format!("${sheet}.{}", self.cell)
            }
            Some(sheet) => format!("$'{}'.{}", sheet.replace('\'', "''"), self.cell),
            None => format!(".{}", self.cell),
        }
    }
}

/// One end of a typed reference: an optional sheet and a cell.
fn end_at(chars: &[char]) -> Option<(Address, usize)> {
    let mut i = 0;
    let mut sheet = None;
    if chars.first() == Some(&'\'') {
        let mut name = String::new();
        i = 1;
        loop {
            match chars.get(i)? {
                '\'' if chars.get(i + 1) == Some(&'\'') => {
                    name.push('\'');
                    i += 2;
                }
                '\'' => {
                    i += 1;
                    break;
                }
                c => {
                    name.push(*c);
                    i += 1;
                }
            }
        }
        if !matches!(chars.get(i), Some('.' | '!')) {
            return None;
        }
        sheet = Some(name);
        i += 1;
    } else {
        let start = usize::from(chars.first() == Some(&'$'));
        let mut j = start;
        while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
            j += 1;
        }
        if j > start
            && matches!(chars.get(j), Some('.' | '!'))
            && cell_at(&chars[j + 1..]).is_some()
        {
            sheet = Some(chars[start..j].iter().collect());
            i = j + 1;
        }
    }
    let (cell, length) = cell_at(&chars[i..])?;
    Some((Address { sheet, cell }, i + length))
}

/// A cell address, `A1` or `$A$1`, at the start of some text.
fn cell_at(chars: &[char]) -> Option<(String, usize)> {
    let mut i = usize::from(chars.first() == Some(&'$'));
    let letters = i;
    while i < chars.len() && chars[i].is_ascii_alphabetic() {
        i += 1;
    }
    if i == letters || i - letters > 3 {
        return None;
    }
    if chars.get(i) == Some(&'$') {
        i += 1;
    }
    let digits = i;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits {
        return None;
    }
    let cell: String = chars[..i].iter().collect();
    Some((cell.to_ascii_uppercase(), i))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_formulas_are_stored_as_odf_writes_them() {
        for (typed, expected) in [
            ("=SUM(A1:A3)", "of:=SUM([.A1:.A3])"),
            ("=sum(a1, $B$2)", "of:=SUM([.A1];[.$B$2])"),
            ("=Data.B2*2", "of:=[$Data.B2]*2"),
            ("=Data!B2:B6", "of:=[$Data.B2:.B6]"),
            ("='My sheet'.A1&\"x, y\"", "of:=[$'My sheet'.A1]&\"x, y\""),
            ("=IF(A1>2;TRUE;FALSE)", "of:=IF([.A1]>2;TRUE();FALSE())"),
            ("=LOG10(100)", "of:=LOG10(100)"),
        ] {
            assert_eq!(to_stored(typed).as_deref(), Ok(expected), "{typed}");
        }
    }

    #[test]
    fn what_cannot_be_read_is_refused() {
        for typed in ["=SUM(A1", "=A1+", "=name_of_a_range*2", "=\"open"] {
            assert!(to_stored(typed).is_err(), "{typed}");
        }
    }

    #[test]
    fn stored_formulas_read_back_as_typed() {
        for (written, expected) in [
            ("of:=SUM([.D2:.D4])", "=SUM(D2:D4)"),
            ("of:=[$Data.B2:.B6]", "=$Data.B2:B6"),
            ("of:=[.C2]/SUM([.$C$2:.$C$4])", "=C2/SUM($C$2:$C$4)"),
            ("of:=\"[.A1]\"&[.A1]", "=\"[.A1]\"&A1"),
        ] {
            assert_eq!(to_typed(written), expected);
            assert_eq!(
                to_stored(&to_typed(written)).as_deref(),
                Ok(written),
                "round trip"
            );
        }
    }
}
