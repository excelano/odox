//! Where a formula's cell references go when rows or columns are inserted or
//! deleted.
//!
//! A formula in an `OpenDocument` spreadsheet is text such as
//! `of:=[.C2]/SUM([.$C$2:.$C$4])`, and every reference in it is inside square
//! brackets: `[.B2]`, `[.$A$1:.B9]`, `[$Sheet2.A1]`, `[$'My sheet'.A1:.B2]`, a
//! whole column `[.A:.A]` or a whole row `[.1:.1]`. Nothing else in a formula
//! names a cell, so shifting these and leaving the rest of the text alone is
//! the whole of moving a formula, as long as a reference is one this module can
//! read. Anything it cannot read is refused rather than guessed at: a wrong
//! formula looks right and nothing here can evaluate one to find out.
//!
//! The rule is the one every spreadsheet uses. An insert moves what is at or
//! after the line, each end of a range on its own, so an insert inside a range
//! grows it and one just below it does not. A delete moves what is after the
//! deleted rows up, takes the ends of a range that were inside to the nearest
//! row left, and refuses a reference to a cell that would be gone.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::fmt;

/// Which way a sheet is being changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Rows are inserted or deleted.
    Row,
    /// Columns are inserted or deleted.
    Column,
}

/// What happens at a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `count` rows or columns are put in before the one at `at`.
    Insert(usize),
    /// `count` rows or columns are taken out from the one at `at`.
    Delete(usize),
}

/// A change to one sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The sheet's name.
    pub sheet: String,
    /// Rows or columns.
    pub axis: Axis,
    /// The first row or column, counting from zero, that is affected.
    pub at: usize,
    /// Inserted or deleted, and how many.
    pub kind: Kind,
}

/// Why a reference was not moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The text between the brackets is not a form this module reads.
    Unreadable(String),
    /// A reference to a cell, or a range wholly inside what is deleted.
    Deleted(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(text) => write!(f, "[{text}] cannot be read"),
            Self::Deleted(text) => write!(f, "[{text}] would be deleted"),
        }
    }
}

/// One end of a reference, with the text around its numbers kept so that it
/// can be written back the way it was.
#[derive(Debug)]
struct End {
    /// Everything up to and including the dot: the sheet, if one is named.
    sheet_text: String,
    sheet: Option<String>,
    column: Option<Part>,
    row: Option<Part>,
}

/// A column or a row number with its `$`.
#[derive(Debug, Clone, Copy)]
struct Part {
    absolute: bool,
    index: usize,
}

fn column_index(letters: &str) -> usize {
    letters.bytes().fold(0, |n, b| {
        n * 26 + usize::from(b.to_ascii_uppercase() - b'A' + 1)
    }) - 1
}

fn column_letters(mut index: usize) -> String {
    let mut out = Vec::new();
    index += 1;
    while index > 0 {
        index -= 1;
        out.push(b'A' + u8::try_from(index % 26).unwrap_or(0));
        index /= 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// Split `text` at the first `:` that is not inside single quotes.
fn split_range(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (at, c) in text.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            ':' if !quoted => {
                parts.push(&text[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// A sheet name as written, `$Name`, `Name`, `$'Na me'` or `'Na me'`, and the
/// name itself.
fn sheet_name(text: &str) -> Option<String> {
    let text = text.strip_prefix('$').unwrap_or(text);
    if let Some(quoted) = text.strip_prefix('\'') {
        let inner = quoted.strip_suffix('\'')?;
        return Some(inner.replace("''", "'"));
    }
    (!text.is_empty() && !text.contains(['\'', '#', '[', ']', ':'])).then(|| text.to_owned())
}

/// Read one end of a reference.
fn read_end(text: &str) -> Option<End> {
    // The dot that ends the sheet part is the last one outside quotes.
    let mut quoted = false;
    let mut dot = None;
    for (at, c) in text.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            '.' if !quoted => dot = Some(at),
            _ => {}
        }
    }
    let dot = dot?;
    let (sheet_part, cell) = (&text[..dot], &text[dot + 1..]);
    let sheet = if sheet_part.is_empty() {
        None
    } else {
        Some(sheet_name(sheet_part)?)
    };
    let mut rest = cell;
    let take = |rest: &mut &str| {
        let absolute = rest.starts_with('$');
        if absolute {
            *rest = &rest[1..];
        }
        absolute
    };
    let column_absolute = take(&mut rest);
    let letters = rest.len()
        - rest
            .trim_start_matches(|c: char| c.is_ascii_alphabetic())
            .len();
    let (letters, after) = rest.split_at(letters);
    rest = after;
    let row_absolute = take(&mut rest);
    let digits_end = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let (digits, after) = rest.split_at(digits_end);
    if !after.is_empty() {
        return None;
    }
    let column = (!letters.is_empty()).then(|| Part {
        absolute: column_absolute,
        index: column_index(letters),
    });
    let row = (!digits.is_empty()).then(|| {
        digits
            .parse::<usize>()
            .ok()
            .filter(|n| *n > 0)
            .map(|n| Part {
                absolute: row_absolute,
                index: n - 1,
            })
    });
    let row = match row {
        Some(Some(part)) => Some(part),
        Some(None) => return None,
        None => None,
    };
    if column.is_none() && row.is_none() {
        return None;
    }
    // A `$` with nothing after it for a part that is absent is not a form.
    if (column.is_none() && column_absolute) || (row.is_none() && row_absolute) {
        return None;
    }
    Some(End {
        sheet_text: text[..=dot].to_owned(),
        sheet,
        column,
        row,
    })
}

fn write_end(end: &End) -> String {
    let mut out = end.sheet_text.clone();
    if let Some(column) = end.column {
        if column.absolute {
            out.push('$');
        }
        out.push_str(&column_letters(column.index));
    }
    if let Some(row) = end.row {
        if row.absolute {
            out.push('$');
        }
        out.push_str(&(row.index + 1).to_string());
    }
    out
}

/// The row or column number of an end, on one axis.
fn index_of(end: &mut End, axis: Axis) -> Option<&mut usize> {
    match axis {
        Axis::Row => end.row.as_mut().map(|part| &mut part.index),
        Axis::Column => end.column.as_mut().map(|part| &mut part.index),
    }
}

/// Where an index goes when a line is inserted before `at`.
fn after_insert(index: usize, at: usize, count: usize) -> usize {
    if index >= at { index + count } else { index }
}

/// Shift the ends of a reference on the changed axis. One end alone is a cell
/// and is refused if it is deleted; two are a range.
fn shift_ends(ends: &mut [End], axis: Axis, at: usize, kind: Kind) -> Result<(), ()> {
    match kind {
        Kind::Insert(count) => {
            for end in ends {
                if let Some(index) = index_of(end, axis) {
                    *index = after_insert(*index, at, count);
                }
            }
            Ok(())
        }
        Kind::Delete(count) => {
            let last = at + count - 1;
            let mut indices: Vec<usize> = Vec::new();
            for end in ends.iter_mut() {
                if let Some(index) = index_of(end, axis) {
                    indices.push(*index);
                }
            }
            // A cell, or a range on a line it has no index on, has one or none.
            if let [only] = indices[..]
                && (at..=last).contains(&only)
                && ends.len() == 1
            {
                return Err(());
            }
            if indices.len() == 2 {
                let (first, end) = (indices[0].min(indices[1]), indices[0].max(indices[1]));
                // Wholly inside what is deleted: nothing is left to point at.
                if first >= at && end <= last {
                    return Err(());
                }
            }
            for end in ends {
                if let Some(index) = index_of(end, axis) {
                    let moved = if *index < at {
                        *index
                    } else if *index > last {
                        *index - count
                    } else if indices.len() == 2
                        && *index == indices.iter().copied().min().unwrap_or(0)
                    {
                        // The start of a range, inside: the first row that is left.
                        at
                    } else {
                        // The end of a range, inside: the row before the gap.
                        at.saturating_sub(1)
                    };
                    *index = moved;
                }
            }
            Ok(())
        }
    }
}

/// Shift the references in one bracketed span, `text` being what is between
/// the brackets. `own_sheet` is the sheet the formula sits on, which an
/// unqualified reference means.
fn shift_reference(
    text: &str,
    own_sheet: Option<&str>,
    change: &Change,
) -> Result<String, Refusal> {
    if text == ".#REF!" || text.starts_with("#REF!") {
        return Ok(text.to_owned());
    }
    let unreadable = || Refusal::Unreadable(text.to_owned());
    let parts = split_range(text);
    if parts.len() > 2 {
        return Err(unreadable());
    }
    let mut ends = parts
        .iter()
        .map(|part| read_end(part))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(unreadable)?;
    // A range names one sheet: the one its first end says, and the second end
    // either says the same or says nothing.
    let sheet = ends[0]
        .sheet
        .clone()
        .or_else(|| own_sheet.map(ToOwned::to_owned));
    let Some(sheet) = sheet else {
        return Err(unreadable());
    };
    if ends.len() == 2 && ends[1].sheet.as_ref().is_some_and(|other| *other != sheet) {
        return Err(unreadable());
    }
    if sheet != change.sheet {
        return Ok(text.to_owned());
    }
    shift_ends(&mut ends, change.axis, change.at, change.kind)
        .map_err(|()| Refusal::Deleted(text.to_owned()))?;
    Ok(ends.iter().map(write_end).collect::<Vec<_>>().join(":"))
}

/// A formula with the references in it moved for a change to a sheet.
///
/// `own_sheet` is the sheet the formula's cell is on. The text outside the
/// brackets, and what is inside a string, is returned untouched.
///
/// # Errors
///
/// A reference this module cannot read, or one to a cell that is deleted.
pub fn shift_formula(
    formula: &str,
    own_sheet: Option<&str>,
    change: &Change,
) -> Result<String, Refusal> {
    let mut out = String::with_capacity(formula.len());
    let mut chars = formula.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        match c {
            '"' => {
                out.push(c);
                // A string runs to the next quote that is not doubled.
                while let Some((_, s)) = chars.next() {
                    out.push(s);
                    if s == '"' {
                        if chars.peek().is_some_and(|(_, next)| *next == '"') {
                            out.extend(chars.next().map(|(_, q)| q));
                        } else {
                            break;
                        }
                    }
                }
            }
            '[' => {
                // To the matching bracket, with single quotes holding any.
                let start = at + 1;
                let mut quoted = false;
                let mut end = None;
                for (i, s) in chars.by_ref() {
                    match s {
                        '\'' => quoted = !quoted,
                        ']' if !quoted => {
                            end = Some(i);
                            break;
                        }
                        _ => {}
                    }
                }
                let end = end.ok_or_else(|| Refusal::Unreadable(formula[start..].to_owned()))?;
                out.push('[');
                out.push_str(&shift_reference(&formula[start..end], own_sheet, change)?);
                out.push(']');
            }
            other => out.push(other),
        }
    }
    Ok(out)
}

/// An address that stands alone, as a named range or a base cell gives one:
/// `$Sheet1.$A$1:.$F$6`, with no brackets around it.
///
/// # Errors
///
/// An address this module cannot read, or one that is deleted.
pub fn shift_address(
    address: &str,
    own_sheet: Option<&str>,
    change: &Change,
) -> Result<String, Refusal> {
    shift_reference(address, own_sheet, change)
}

/// The sheet a standalone address names first, which is the sheet a formula
/// written beside it means by a reference with none.
pub fn address_sheet(address: &str) -> Option<String> {
    read_end(split_range(address).first()?)?.sheet
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(sheet: &str, at: usize, kind: Kind) -> Change {
        Change {
            sheet: sheet.to_owned(),
            axis: Axis::Row,
            at,
            kind,
        }
    }

    fn columns(sheet: &str, at: usize, kind: Kind) -> Change {
        Change {
            axis: Axis::Column,
            ..rows(sheet, at, kind)
        }
    }

    fn shifted(formula: &str, change: &Change) -> Result<String, Refusal> {
        shift_formula(formula, Some("Sheet1"), change)
    }

    #[test]
    fn column_letters_and_numbers_agree() {
        for (letters, index) in [("A", 0), ("Z", 25), ("AA", 26), ("AB", 27), ("XFD", 16383)] {
            assert_eq!(column_index(letters), index);
            assert_eq!(column_letters(index), letters);
        }
    }

    #[test]
    fn an_insert_moves_what_is_at_or_after_it_and_keeps_the_dollars() {
        let insert = rows("Sheet1", 2, Kind::Insert(3));
        assert_eq!(
            shifted("of:=[.B2]*[.C3]/[.$D$4]", &insert).unwrap(),
            "of:=[.B2]*[.C6]/[.$D$7]"
        );
    }

    #[test]
    fn an_insert_inside_a_range_grows_it_and_one_below_does_not() {
        assert_eq!(
            shifted(
                "of:=SUM([.$C$2:.$C$4])",
                &rows("Sheet1", 2, Kind::Insert(1))
            )
            .unwrap(),
            "of:=SUM([.$C$2:.$C$5])"
        );
        assert_eq!(
            shifted("of:=SUM([.C2:.C4])", &rows("Sheet1", 4, Kind::Insert(1))).unwrap(),
            "of:=SUM([.C2:.C4])"
        );
        assert_eq!(
            shifted("of:=SUM([.C2:.C4])", &rows("Sheet1", 1, Kind::Insert(1))).unwrap(),
            "of:=SUM([.C3:.C5])",
            "an insert at the first row moves the whole range"
        );
    }

    #[test]
    fn columns_move_the_letters_and_leave_the_rows() {
        let insert = columns("Sheet1", 1, Kind::Insert(1));
        assert_eq!(
            shifted("of:=[.A1]+[.B1]+[.$Z$9]", &insert).unwrap(),
            "of:=[.A1]+[.C1]+[.$AA$9]"
        );
    }

    #[test]
    fn a_delete_moves_what_follows_up_and_shrinks_a_range_around_it() {
        let delete = rows("Sheet1", 2, Kind::Delete(1));
        assert_eq!(
            shifted("of:=SUM([.C2:.C5])+[.A9]", &delete).unwrap(),
            "of:=SUM([.C2:.C4])+[.A8]"
        );
        let delete_two = rows("Sheet1", 1, Kind::Delete(2));
        assert_eq!(
            shifted("of:=SUM([.C1:.C5])", &delete_two).unwrap(),
            "of:=SUM([.C1:.C3])"
        );
        assert_eq!(
            shifted("of:=SUM([.C2:.C5])", &delete_two).unwrap(),
            "of:=SUM([.C2:.C3])",
            "a start inside the gap becomes the first row left"
        );
    }

    #[test]
    fn a_reference_to_a_deleted_cell_or_a_wholly_deleted_range_is_refused() {
        let delete = rows("Sheet1", 2, Kind::Delete(2));
        assert_eq!(
            shifted("of:=[.B3]", &delete),
            Err(Refusal::Deleted(".B3".to_owned()))
        );
        assert_eq!(
            shifted("of:=SUM([.B3:.B4])", &delete),
            Err(Refusal::Deleted(".B3:.B4".to_owned()))
        );
        assert!(shifted("of:=[.B2]+[.B5]", &delete).is_ok());
    }

    #[test]
    fn only_references_to_the_changed_sheet_move() {
        let insert = rows("Sheet2", 0, Kind::Insert(1));
        assert_eq!(
            shifted("of:=[.A1]+[$Sheet2.A1]+[$'Sheet2'.B2:.C3]", &insert).unwrap(),
            "of:=[.A1]+[$Sheet2.A2]+[$'Sheet2'.B3:.C4]"
        );
        assert_eq!(
            shift_formula("of:=[.A1]", Some("Sheet2"), &insert).unwrap(),
            "of:=[.A2]",
            "a formula on the changed sheet means that sheet by a bare reference"
        );
    }

    #[test]
    fn quoted_sheet_names_with_spaces_and_quotes_are_read() {
        let insert = rows("Bob's sheet", 0, Kind::Insert(1));
        assert_eq!(
            shifted("of:=[$'Bob''s sheet'.A1]", &insert).unwrap(),
            "of:=[$'Bob''s sheet'.A2]"
        );
    }

    #[test]
    fn whole_columns_and_rows_move_on_the_axis_that_has_numbers() {
        let insert = rows("Sheet1", 0, Kind::Insert(1));
        assert_eq!(
            shifted("of:=SUM([.A:.A])", &insert).unwrap(),
            "of:=SUM([.A:.A])"
        );
        assert_eq!(
            shifted("of:=SUM([.2:.3])", &insert).unwrap(),
            "of:=SUM([.3:.4])"
        );
        let insert = columns("Sheet1", 0, Kind::Insert(1));
        assert_eq!(
            shifted("of:=SUM([.A:.B])", &insert).unwrap(),
            "of:=SUM([.B:.C])"
        );
    }

    #[test]
    fn strings_and_text_outside_the_brackets_are_left_alone() {
        let insert = rows("Sheet1", 0, Kind::Insert(1));
        assert_eq!(
            shifted(r#"of:=IF([.A1]="[.A1] and ""[.B2]""";"x";[.A2])"#, &insert).unwrap(),
            r#"of:=IF([.A2]="[.A1] and ""[.B2]""";"x";[.A3])"#
        );
    }

    #[test]
    fn what_cannot_be_read_is_refused() {
        let insert = rows("Sheet1", 0, Kind::Insert(1));
        for formula in [
            "of:=['file:///other.ods'#$Sheet1.A1]",
            "of:=[$Sheet1.A1:$Sheet2.B2]",
            "of:=[.A1:.B2:.C3]",
            "of:=[.1A]",
            "of:=[nonsense]",
            "of:=[.A1",
        ] {
            assert!(
                matches!(shifted(formula, &insert), Err(Refusal::Unreadable(_))),
                "{formula}"
            );
        }
    }

    #[test]
    fn a_reference_that_is_already_an_error_is_left_as_it_is() {
        let insert = rows("Sheet1", 0, Kind::Insert(1));
        assert_eq!(
            shifted("of:=[.#REF!]+[.A1]", &insert).unwrap(),
            "of:=[.#REF!]+[.A2]"
        );
    }

    #[test]
    fn a_standalone_address_shifts_and_names_its_sheet() {
        let insert = rows("Sheet1", 0, Kind::Insert(2));
        assert_eq!(
            shift_address("$Sheet1.$A$1:.$F$6", None, &insert).unwrap(),
            "$Sheet1.$A$3:.$F$8"
        );
        assert_eq!(
            address_sheet("$Sheet1.$A$1:.$F$6").as_deref(),
            Some("Sheet1")
        );
        assert_eq!(address_sheet("garbage"), None);
    }
}
