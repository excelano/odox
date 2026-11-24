//! Validity rules on entry: `table:content-validation`, which a cell names,
//! saying what may be typed into it and what to say when something else is.
//! DESIGN.md §11.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::cmp::Ordering;

use super::recalc::{date_serial, duration_days};
use super::{SheetDocument, Value};
use crate::formula::text_number;
use crate::xml::{Element, Ns};

/// What a rule says to a person about a value it does not take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// The message's title.
    pub title: String,
    /// What it says.
    pub text: String,
}

/// What a rule makes of a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The value is taken.
    Allowed,
    /// The value is not taken.
    Refused(Message),
    /// The value is taken if the person says so.
    Warned(Message),
    /// The value is taken, and the person told.
    Informed(Message),
}

/// What a value has to be, before any comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Any,
    Whole,
    Decimal,
    Date,
    Time,
}

/// What a value is compared by: its value, or its text's length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Measure {
    Value,
    Length,
}

/// The comparison a rule makes.
#[derive(Debug, Clone, PartialEq)]
enum Test {
    None,
    Compare(Measure, &'static str, Bound),
    Between(Measure, Bound, Bound, bool),
    List(Vec<Bound>),
    Unread,
}

/// A bound or a list item: a number or text written into the rule.
#[derive(Debug, Clone, PartialEq)]
enum Bound {
    Number(f64),
    Text(String),
}

/// How a refused value is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answer {
    Stop,
    Warning,
    Information,
}

/// A cell's validity rule.
#[derive(Debug, Clone)]
pub struct Validation {
    kind: Kind,
    test: Test,
    empty: bool,
    /// Whether a list item and text compare with their case.
    case: bool,
    answer: Option<(Answer, Message)>,
}

impl Validation {
    /// Read a `table:content-validation`.
    fn read(element: &Element) -> Self {
        let condition = element.attr(&Ns::Table, "condition").unwrap_or("");
        let (kind, test) = condition_of(condition.strip_prefix("of:").unwrap_or(condition));
        let answer = element
            .child(&Ns::Table, "error-message")
            .filter(|message| message.attr(&Ns::Table, "display") != Some("false"))
            .map(|message| {
                let answer = match message.attr(&Ns::Table, "message-type") {
                    Some("warning") => Answer::Warning,
                    Some("information") => Answer::Information,
                    _ => Answer::Stop,
                };
                let said = message
                    .elements()
                    .filter(|p| p.is(&Ns::Text, "p"))
                    .map(Element::plain_text)
                    .collect::<Vec<_>>()
                    .join("\n");
                let title = message.attr(&Ns::Table, "title").unwrap_or("").to_owned();
                (answer, Message { title, text: said })
            });
        Self {
            kind,
            test,
            empty: element.attr(&Ns::Table, "allow-empty-cell") != Some("false"),
            case: element.attr(&Ns::Table, "case-sensitive") == Some("true"),
            answer,
        }
    }

    /// What the rule makes of a value.
    pub fn check(&self, value: &Value) -> Verdict {
        if self.holds(value) {
            return Verdict::Allowed;
        }
        match &self.answer {
            None => Verdict::Allowed,
            Some((Answer::Stop, message)) => Verdict::Refused(message.clone()),
            Some((Answer::Warning, message)) => Verdict::Warned(message.clone()),
            Some((Answer::Information, message)) => Verdict::Informed(message.clone()),
        }
    }

    /// Whether a value meets the rule.
    fn holds(&self, value: &Value) -> bool {
        if matches!(value, Value::Empty) || matches!(value, Value::Text(text) if text.is_empty()) {
            return self.empty;
        }
        let number = match (self.kind, value) {
            (Kind::Date | Kind::Time, Value::Text(text)) => match text_number(text, None) {
                Some(n) => Some(n),
                None => return true,
            },
            _ => number_of(value),
        };
        let fits = match self.kind {
            Kind::Any => true,
            Kind::Whole => number.is_some_and(|n| (n - n.round()).abs() <= 1e-9 * n.abs().max(1.0)),
            Kind::Decimal | Kind::Date | Kind::Time => number.is_some(),
        };
        if !fits {
            return false;
        }
        let measured = |measure: Measure| match measure {
            Measure::Value => number.map_or_else(|| Bound::Text(value.input_text()), Bound::Number),
            #[allow(clippy::cast_precision_loss)]
            Measure::Length => Bound::Number(value.input_text().chars().count() as f64),
        };
        match &self.test {
            Test::None | Test::Unread => true,
            Test::Compare(measure, op, bound) => order(&measured(*measure), bound, self.case)
                .is_some_and(|order| match *op {
                    "=" => order.is_eq(),
                    "!=" => order.is_ne(),
                    "<" => order.is_lt(),
                    ">" => order.is_gt(),
                    "<=" => order.is_le(),
                    _ => order.is_ge(),
                }),
            Test::Between(measure, low, high, outside) => {
                let value = measured(*measure);
                let within = order(&value, low, self.case).is_some_and(Ordering::is_ge)
                    && order(&value, high, self.case).is_some_and(Ordering::is_le);
                within != *outside
            }
            Test::List(items) => {
                let value = measured(Measure::Value);
                items.iter().any(|item| {
                    order(&value, item, self.case) == Some(Ordering::Equal)
                        || matches!((&value, item), (Bound::Number(n), Bound::Text(t))
                            if t.trim().parse::<f64>().is_ok_and(|item| (item - *n).abs() < f64::EPSILON))
                })
            }
        }
    }
}

/// A value as a number where it is one: a date as its serial day, a time as
/// the fraction of a day.
fn number_of(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) | Value::Percentage(n) | Value::Currency(n, _) => Some(*n),
        Value::Date(text) => date_serial(text),
        Value::Time(text) => duration_days(text),
        Value::Boolean(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    }
}

/// Two bounds in order: numbers by number, text with or without regard to
/// case; a number and text have none.
fn order(a: &Bound, b: &Bound, case: bool) -> Option<Ordering> {
    match (a, b) {
        (Bound::Number(a), Bound::Number(b)) => a.partial_cmp(b),
        (Bound::Text(a), Bound::Text(b)) if case => Some(a.cmp(b)),
        (Bound::Text(a), Bound::Text(b)) => Some(a.to_lowercase().cmp(&b.to_lowercase())),
        _ => None,
    }
}

/// A rule's condition: `cell-content-is-whole-number() and
/// cell-content-is-between(1,10)`, `cell-content-text-length()<=5`,
/// `cell-content-is-in-list("a";"b")`.
fn condition_of(condition: &str) -> (Kind, Test) {
    let condition = condition.trim();
    if condition.is_empty() {
        return (Kind::Any, Test::None);
    }
    let kinds = [
        ("cell-content-is-whole-number()", Kind::Whole),
        ("cell-content-is-decimal-number()", Kind::Decimal),
        ("cell-content-is-date()", Kind::Date),
        ("cell-content-is-time()", Kind::Time),
    ];
    for (written, kind) in kinds {
        if let Some(rest) = condition.strip_prefix(written) {
            let rest = rest.trim();
            if rest.is_empty() {
                return (kind, Test::None);
            }
            return match rest.strip_prefix("and") {
                Some(test) => (kind, test_of(test.trim())),
                None => (kind, Test::Unread),
            };
        }
    }
    (Kind::Any, test_of(condition))
}

fn test_of(test: &str) -> Test {
    let call = |name: &str| {
        test.strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('('))
            .and_then(|rest| rest.strip_suffix(')'))
    };
    if let Some(inside) = call("cell-content-is-in-list") {
        return items(inside, ';').map_or(Test::Unread, Test::List);
    }
    for (name, measure, outside) in [
        ("cell-content-is-between", Measure::Value, false),
        ("cell-content-is-not-between", Measure::Value, true),
        (
            "cell-content-text-length-is-between",
            Measure::Length,
            false,
        ),
        (
            "cell-content-text-length-is-not-between",
            Measure::Length,
            true,
        ),
    ] {
        if let Some(inside) = call(name) {
            return match items(inside, ',').as_deref() {
                Some([low, high]) => Test::Between(measure, low.clone(), high.clone(), outside),
                _ => Test::Unread,
            };
        }
    }
    for (prefix, measure) in [
        ("cell-content-text-length()", Measure::Length),
        ("cell-content()", Measure::Value),
    ] {
        if let Some(rest) = test.strip_prefix(prefix) {
            for op in ["<=", ">=", "!=", "=", "<", ">"] {
                if let Some(bound) = rest.trim().strip_prefix(op) {
                    return bound_of(bound.trim())
                        .map_or(Test::Unread, |bound| Test::Compare(measure, op, bound));
                }
            }
        }
    }
    Test::Unread
}

/// Bounds or list items at a separator outside quotes, each a literal.
fn items(inside: &str, separator: char) -> Option<Vec<Bound>> {
    let mut parts = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (at, c) in inside.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if c == separator && !quoted {
            parts.push(&inside[start..at]);
            start = at + c.len_utf8();
        }
    }
    parts.push(&inside[start..]);
    parts
        .into_iter()
        .map(|part| bound_of(part.trim()))
        .collect()
}

/// A literal number or quoted text.
fn bound_of(written: &str) -> Option<Bound> {
    if let Some(text) = written.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        return Some(Bound::Text(text.replace("\"\"", "\"")));
    }
    written.parse().ok().map(Bound::Number)
}

impl SheetDocument {
    /// The validity rule a cell names, if any.
    pub fn validation(&self, sheet: usize, row: usize, column: usize) -> Option<Validation> {
        let index = self.sheets.get(sheet)?;
        let cell = self.cell(index, row, column)?;
        let name = cell.element.attr(&Ns::Table, "content-validation-name")?;
        self.document
            .body_of("spreadsheet")?
            .child(&Ns::Table, "content-validations")?
            .elements()
            .find(|rule| {
                rule.is(&Ns::Table, "content-validation")
                    && rule.attr(&Ns::Table, "name") == Some(name)
            })
            .map(Validation::read)
    }
}
