//! A stored formula moved to another cell: its relative references go with
//! it, and its absolute ones stay.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

/// A stored formula as it reads from a cell `rows` down and `columns` across
/// from where it was written: `of:=[.A1]*[.$B$1]` moved one down is
/// `of:=[.A2]*[.$B$1]`. A reference moved off the sheet's top or left edge is
/// `#REF!`.
pub fn shifted(stored: &str, rows: isize, columns: isize) -> String {
    let mut out = String::with_capacity(stored.len());
    let mut chars = stored.chars();
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
                match moved(&inside, rows, columns) {
                    Some(inside) => {
                        out.push('[');
                        out.push_str(&inside);
                        out.push(']');
                    }
                    None => out.push_str("#REF!"),
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// The inside of a reference moved, or `None` where it leaves the sheet.
fn moved(inside: &str, rows: isize, columns: isize) -> Option<String> {
    inside
        .split(':')
        .map(|end| {
            let dot = end.rfind('.')?;
            let (sheet, cell) = end.split_at(dot + 1);
            Some(format!("{sheet}{}", moved_cell(cell, rows, columns)?))
        })
        .collect::<Option<Vec<_>>>()
        .map(|ends| ends.join(":"))
}

fn moved_cell(cell: &str, rows: isize, columns: isize) -> Option<String> {
    let fixed_column = cell.starts_with('$');
    let rest = cell.trim_start_matches('$');
    let letters: String = rest.chars().take_while(char::is_ascii_alphabetic).collect();
    let rest = &rest[letters.len()..];
    let (fixed_column, fixed_row) = if letters.is_empty() {
        (false, fixed_column)
    } else {
        (fixed_column, rest.starts_with('$'))
    };
    let digits = rest.trim_start_matches('$');
    let number: Option<isize> = if digits.is_empty() {
        None
    } else {
        Some(digits.parse().ok()?)
    };
    let index = if letters.is_empty() {
        None
    } else {
        Some(letters.bytes().try_fold(0isize, |n, b| {
            Some(n * 26 + isize::from(b.to_ascii_uppercase() - b'A') + 1)
        })?)
    };
    if index.is_none() && number.is_none() {
        return None;
    }
    let column = index.map(|index| if fixed_column { index } else { index + columns });
    let row = number.map(|number| if fixed_row { number } else { number + rows });
    if column.is_some_and(|c| c < 1) || row.is_some_and(|r| r < 1) {
        return None;
    }
    let mut out = String::new();
    if let Some(column) = column {
        if fixed_column {
            out.push('$');
        }
        out.push_str(&letters_of(column));
    }
    if let Some(row) = row {
        if fixed_row {
            out.push('$');
        }
        out.push_str(&row.to_string());
    }
    Some(out)
}

fn letters_of(mut column: isize) -> String {
    let mut out = Vec::new();
    while column > 0 {
        let rest = (column - 1) % 26;
        out.push(char::from(b'A' + u8::try_from(rest).unwrap_or(0)));
        column = (column - 1) / 26;
    }
    out.iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::shifted;

    #[test]
    fn relative_references_move_and_absolute_ones_stay() {
        assert_eq!(shifted("of:=SUM([.B:.B])", 3, 0), "of:=SUM([.B:.B])");
        assert_eq!(shifted("of:=SUM([.B:.$C])", 0, 1), "of:=SUM([.C:.$C])");
        assert_eq!(shifted("of:=SUM([.2:.$4])", 1, 0), "of:=SUM([.3:.$4])");
        assert_eq!(shifted("of:=[.A1]*[.$B$1]", 1, 0), "of:=[.A2]*[.$B$1]");
        assert_eq!(shifted("of:=SUM([.A1:.B2])", 0, 2), "of:=SUM([.C1:.D2])");
        assert_eq!(
            shifted("of:=[$Data.$A2]+[.Z1]", 3, 1),
            "of:=[$Data.$A5]+[.AA4]"
        );
        assert_eq!(shifted("of:=\"[.A1]\"&[.A1]", 1, 0), "of:=\"[.A1]\"&[.A2]");
        assert_eq!(shifted("of:=[.A1]+1", -1, 0), "of:=#REF!+1");
    }
}
