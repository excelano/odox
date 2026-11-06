//! Filling cells from the ones beside them: down or across, as copies, or
//! continuing a series. DESIGN.md §11.
//!
//! Each column of a fill down, or row of a fill across, is filled from its own
//! source cells, over and over: a value is copied as it is, a formula with its
//! relative references moved, and the cell style with either. A series goes
//! on where the source cells start one: numbers or dates a constant step apart,
//! text ending in a number, and the names of days and months. One source cell
//! starts a series of names and of numbered text, as it does in every
//! spreadsheet, and a lone number or date is copied.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::{Place, SheetDocument, Value};
use crate::edit::Refused;
use crate::formula;
use crate::xml::{Element, Node, Ns};

/// A rectangle of cells, by its top left and bottom right corners, counting
/// rows and columns from zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    /// The first row.
    pub top: usize,
    /// The first column.
    pub left: usize,
    /// The last row.
    pub bottom: usize,
    /// The last column.
    pub right: usize,
}

/// One source cell, as a fill or a sort copies it.
pub(super) struct Source {
    pub(super) row: usize,
    pub(super) column: usize,
    pub(super) value: Value,
    pub(super) formula: Option<String>,
    pub(super) style: Option<String>,
    /// Its paragraphs as the document has them, with their spans and links.
    pub(super) paragraphs: Vec<Element>,
}

impl SheetDocument {
    /// Fill the `count` rows below a block, or the `count` columns to its
    /// right, from it: with copies, or continuing a series where `series` asks
    /// and the block starts one. Answers the cells written, for
    /// [`Self::recalculate`].
    ///
    /// # Errors
    ///
    /// The sheet does not exist, or a cell the fill reads or writes is merged
    /// or under a merge. Nothing is changed in either case.
    pub fn fill(
        &mut self,
        sheet: usize,
        source: Block,
        down: bool,
        count: usize,
        series: bool,
    ) -> Result<Vec<Place>, Refused> {
        let index = self.sheets.get(sheet).ok_or(Refused::NotFound)?;
        let (rows, columns) = if down {
            (
                source.top..=source.bottom + count,
                source.left..=source.right,
            )
        } else {
            (
                source.top..=source.bottom,
                source.left..=source.right + count,
            )
        };
        for row in rows {
            for column in columns.clone() {
                let merged = self.cell(index, row, column).is_some_and(|cell| {
                    cell.covered || cell.columns_spanned() > 1 || cell.rows_spanned() > 1
                });
                if merged {
                    return Err(Refused::Structure);
                }
            }
        }
        let lines = if down {
            source.left..=source.right
        } else {
            source.top..=source.bottom
        };
        let whole = if down {
            Block {
                bottom: source.bottom + count,
                ..source
            }
        } else {
            Block {
                right: source.right + count,
                ..source
            }
        };
        self.in_batch(sheet, whole, |this| {
            let mut written = Vec::new();
            for line in lines {
                let sources: Vec<Source> = if down {
                    (source.top..=source.bottom)
                        .map(|row| this.source(sheet, row, line))
                        .collect()
                } else {
                    (source.left..=source.right)
                        .map(|column| this.source(sheet, line, column))
                        .collect()
                };
                let continued = if series { series_of(&sources) } else { None };
                for step in 0..count {
                    let (row, column) = if down {
                        (source.bottom + 1 + step, line)
                    } else {
                        (line, source.right + 1 + step)
                    };
                    let from = &sources[step % sources.len()];
                    this.set_style_name(sheet, row, column, from.style.as_deref())?;
                    match (&continued, &from.formula) {
                        (Some(next), _) => {
                            this.set_cell(sheet, row, column, &next(sources.len() + step))?;
                        }
                        (None, Some(stored)) => {
                            let moved = formula::shifted(
                                stored,
                                offset(row, from.row),
                                offset(column, from.column),
                            );
                            this.set_formula(sheet, row, column, &moved)?;
                        }
                        (None, None) => this.copy_value(sheet, row, column, from)?,
                    }
                    written.push((sheet, row, column));
                }
            }
            Ok(written)
        })
    }

    pub(super) fn source(&self, sheet: usize, row: usize, column: usize) -> Source {
        let cell = self
            .sheets
            .get(sheet)
            .and_then(|index| self.cell(index, row, column));
        Source {
            row,
            column,
            value: cell.as_ref().map_or(Value::Empty, super::Cell::value),
            formula: cell
                .as_ref()
                .and_then(|cell| cell.element.attr(&Ns::Table, "formula"))
                .map(ToOwned::to_owned),
            style: cell
                .as_ref()
                .and_then(|cell| cell.style_name())
                .map(ToOwned::to_owned),
            paragraphs: cell
                .as_ref()
                .map(|cell| {
                    cell.element
                        .elements()
                        .filter(|e| e.is(&Ns::Text, "p"))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// Write a cell's value as a copy of another's, its paragraphs taken as
    /// they stand so that a link or a bold word goes with it.
    pub(super) fn copy_value(
        &mut self,
        sheet: usize,
        row: usize,
        column: usize,
        from: &Source,
    ) -> Result<(), Refused> {
        self.set_cell(sheet, row, column, &from.value)?;
        if from.paragraphs.is_empty() {
            return Ok(());
        }
        let cell = self.cell_element(sheet, row, column)?;
        let at = cell
            .children
            .iter()
            .position(|n| matches!(n, Node::Element(e) if e.is(&Ns::Text, "p")))
            .unwrap_or(cell.children.len());
        cell.children
            .retain(|n| !matches!(n, Node::Element(e) if e.is(&Ns::Text, "p")));
        let at = at.min(cell.children.len());
        cell.children
            .splice(at..at, from.paragraphs.iter().cloned().map(Node::Element));
        cell.self_closing = false;
        self.touched();
        Ok(())
    }

    /// Give a cell a cell style, or take its own away.
    pub(super) fn set_style_name(
        &mut self,
        sheet: usize,
        row: usize,
        column: usize,
        style: Option<&str>,
    ) -> Result<(), Refused> {
        let name = self.document.name(&Ns::Table, "style-name");
        let cell = self.cell_element(sheet, row, column)?;
        match style {
            Some(style) => cell.set_attr(name, style),
            None => {
                cell.remove_attr(&Ns::Table, "style-name");
            }
        }
        self.touched();
        Ok(())
    }
}

pub(super) fn offset(to: usize, from: usize) -> isize {
    isize::try_from(to).unwrap_or(isize::MAX) - isize::try_from(from).unwrap_or(isize::MAX)
}

/// What the cell `n` places along from the first source cell holds, where the
/// source cells start a series.
type Series = Box<dyn Fn(usize) -> Value>;

fn series_of(sources: &[Source]) -> Option<Series> {
    if sources.iter().any(|s| s.formula.is_some()) {
        return None;
    }
    let values: Vec<&Value> = sources.iter().map(|s| &s.value).collect();
    numbers(&values)
        .or_else(|| dates(&values))
        .or_else(|| named(&values))
        .or_else(|| numbered_text(&values))
}

/// The step between each value and the next, where it is the same throughout.
fn steady(points: &[f64]) -> Option<f64> {
    let step = points.get(1)? - points.first()?;
    let scale = points.iter().fold(1.0_f64, |m, p| m.max(p.abs()));
    points
        .windows(2)
        .all(|pair| ((pair[1] - pair[0]) - step).abs() <= 1e-9 * scale)
        .then_some(step)
}

#[allow(clippy::cast_precision_loss)]
fn numbers(values: &[&Value]) -> Option<Series> {
    let points: Vec<f64> = values
        .iter()
        .map(|value| match value {
            Value::Number(n) | Value::Percentage(n) | Value::Currency(n, _) => Some(*n),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let same_kind = values
        .iter()
        .all(|v| std::mem::discriminant(*v) == std::mem::discriminant(values[0]));
    let step = steady(&points).filter(|_| same_kind)?;
    let (first, kind) = (points[0], values[0].clone());
    Some(Box::new(move |n| {
        let x = first + step * n as f64;
        match &kind {
            Value::Percentage(_) => Value::Percentage(x),
            Value::Currency(_, code) => Value::Currency(x, code.clone()),
            _ => Value::Number(x),
        }
    }))
}

#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn dates(values: &[&Value]) -> Option<Series> {
    let days: Vec<f64> = values
        .iter()
        .map(|value| match value {
            Value::Date(text) if text.len() == 10 => {
                let mut fields = text.splitn(3, '-').map(|f| f.parse::<i64>().ok());
                Some(formula::eval_serial(
                    fields.next()??,
                    fields.next()??,
                    fields.next()??,
                ))
            }
            _ => None,
        })
        .collect::<Option<_>>()?;
    let step = steady(&days)?;
    let first = days[0];
    Some(Box::new(move |n| {
        let (year, month, day) = formula::eval_civil(first + (step * n as f64).round());
        Value::Date(format!("{year:04}-{month:02}-{day:02}"))
    }))
}

/// Text ending in a number, `Item 1`, `Q03`, counting on by a steady step
/// and keeping the width of a number written with leading zeros; one such
/// cell counts on by one.
#[allow(clippy::cast_possible_truncation)]
fn numbered_text(values: &[&Value]) -> Option<Series> {
    let parts: Vec<(String, i64, Option<usize>)> = values
        .iter()
        .map(|value| {
            let Value::Text(text) = value else {
                return None;
            };
            let digits = text.chars().rev().take_while(char::is_ascii_digit).count();
            if digits == 0 || digits == text.len() {
                return None;
            }
            let (prefix, number) = text.split_at(text.len() - digits);
            let padded = (digits > 1 && number.starts_with('0')).then_some(digits);
            Some((prefix.to_owned(), number.parse().ok()?, padded))
        })
        .collect::<Option<_>>()?;
    if parts.iter().any(|(prefix, ..)| *prefix != parts[0].0) {
        return None;
    }
    let step = if parts.len() == 1 {
        1
    } else {
        #[allow(clippy::cast_precision_loss)]
        let numbers: Vec<f64> = parts.iter().map(|p| p.1 as f64).collect();
        steady(&numbers)?.round() as i64
    };
    let (prefix, first, _) = parts[0].clone();
    let width = parts.iter().find_map(|p| p.2);
    Some(Box::new(move |n| {
        let number = first + step * i64::try_from(n).unwrap_or(i64::MAX);
        Value::Text(match width {
            Some(width) => format!("{prefix}{number:0width$}"),
            None => format!("{prefix}{number}"),
        })
    }))
}

const NAMES: [&[&str]; 8] = [
    &[
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ],
    &["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
    &[
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ],
    &[
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    &[
        "Montag",
        "Dienstag",
        "Mittwoch",
        "Donnerstag",
        "Freitag",
        "Samstag",
        "Sonntag",
    ],
    &["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"],
    &[
        "Januar",
        "Februar",
        "März",
        "April",
        "Mai",
        "Juni",
        "Juli",
        "August",
        "September",
        "Oktober",
        "November",
        "Dezember",
    ],
    &[
        "Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
    ],
];

/// Names of days or months, in English or German, going round by a steady
/// step and written in the case the first is; one name goes on to the next.
#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
fn named(values: &[&Value]) -> Option<Series> {
    let texts: Vec<&str> = values
        .iter()
        .map(|value| match value {
            Value::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let (list, places) = NAMES.iter().find_map(|list| {
        let places: Vec<usize> = texts
            .iter()
            .map(|text| list.iter().position(|name| name.eq_ignore_ascii_case(text)))
            .collect::<Option<_>>()?;
        Some((*list, places))
    })?;
    let length = list.len() as i64;
    let step = if places.len() == 1 {
        1
    } else {
        let step = (places[1] as i64 - places[0] as i64).rem_euclid(length);
        let steady = places
            .windows(2)
            .all(|pair| (pair[1] as i64 - pair[0] as i64).rem_euclid(length) == step);
        if !steady {
            return None;
        }
        step
    };
    let first = places[0] as i64;
    let case = texts[0].to_owned();
    Some(Box::new(move |n| {
        let name = list[(first + step * n as i64).rem_euclid(length) as usize];
        Value::Text(if case.chars().all(|c| !c.is_lowercase()) {
            name.to_uppercase()
        } else if case.chars().all(|c| !c.is_uppercase()) {
            name.to_lowercase()
        } else {
            name.to_owned()
        })
    }))
}
