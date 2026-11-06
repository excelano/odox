//! Data styles: how a value is shown, and how what a person types is read back
//! in the same form. ODF 1.3 part 3 §16.29.
//!
//! A cell names a data style through its cell style's `style:data-style-name`.
//! The style is a sequence of parts, a number, a currency symbol, a month,
//! literal text, laid out in order, and may hand a value on to another style
//! through `style:map` conditions, which is how a negative amount is shown red
//! with a minus sign while a positive one is not. A style with conditions shows
//! the magnitude and leaves the sign to its literal text.
//!
//! Separators follow the style's own language rather than the reader's,
//! because the style is what the document says the cell looks like: a German
//! currency style shows `1.234,50 €` on any machine.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::HashMap;
use std::fmt::Write;

use crate::doc::Value;
use crate::xml::{Element, Ns};

/// What kind of value a data style shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `number:number-style`.
    Number,
    /// `number:currency-style`.
    Currency,
    /// `number:percentage-style`.
    Percentage,
    /// `number:date-style`.
    Date,
    /// `number:time-style`.
    Time,
    /// `number:boolean-style`.
    Boolean,
    /// `number:text-style`.
    Text,
}

#[derive(Debug, Clone, PartialEq)]
enum Part {
    Text(String),
    Number(Digits),
    Scientific {
        digits: Digits,
        exponent_digits: usize,
        forced_sign: bool,
    },
    Fraction {
        integer: bool,
        denominator: u32,
    },
    CurrencySymbol(String),
    Year {
        long: bool,
    },
    Month {
        long: bool,
        textual: bool,
    },
    Day {
        long: bool,
    },
    DayOfWeek {
        long: bool,
    },
    Hours {
        long: bool,
    },
    Minutes {
        long: bool,
    },
    Seconds {
        long: bool,
    },
    AmPm,
    Boolean,
    TextContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Digits {
    decimals: Option<usize>,
    min_decimals: usize,
    min_integer: usize,
    grouping: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Equal,
    NotEqual,
}

#[derive(Debug, Clone, PartialEq)]
struct Condition {
    op: Op,
    against: f64,
    style: String,
}

impl Condition {
    fn holds(&self, x: f64) -> bool {
        match self.op {
            Op::Less => x < self.against,
            Op::LessOrEqual => x <= self.against,
            Op::Greater => x > self.against,
            Op::GreaterOrEqual => x >= self.against,
            Op::Equal => (x - self.against).abs() < f64::EPSILON,
            Op::NotEqual => (x - self.against).abs() >= f64::EPSILON,
        }
    }

    /// `value()>=0`, the form every producer writes.
    fn parse(condition: &str, style: &str) -> Option<Self> {
        let rest = condition.trim().strip_prefix("value()")?.trim_start();
        let (op, number) = [
            ("<=", Op::LessOrEqual),
            (">=", Op::GreaterOrEqual),
            ("!=", Op::NotEqual),
            ("<>", Op::NotEqual),
            ("<", Op::Less),
            (">", Op::Greater),
            ("=", Op::Equal),
        ]
        .into_iter()
        .find_map(|(spelled, op)| rest.strip_prefix(spelled).map(|number| (op, number)))?;
        Some(Self {
            op,
            against: number.trim().parse().ok()?,
            style: style.to_owned(),
        })
    }
}

/// One data style, as read from its element.
#[derive(Debug, Clone, PartialEq)]
pub struct DataStyle {
    /// What kind of value it shows.
    pub kind: Kind,
    parts: Vec<Part>,
    conditions: Vec<Condition>,
    language: Option<String>,
    country: Option<String>,
}

impl DataStyle {
    /// Read a `number:*-style` element, or `None` for any other element.
    pub fn read(element: &Element) -> Option<Self> {
        if element.name.ns != Ns::Number {
            return None;
        }
        let kind = match &*element.name.local {
            "number-style" => Kind::Number,
            "currency-style" => Kind::Currency,
            "percentage-style" => Kind::Percentage,
            "date-style" => Kind::Date,
            "time-style" => Kind::Time,
            "boolean-style" => Kind::Boolean,
            "text-style" => Kind::Text,
            _ => return None,
        };
        let mut language = element.attr(&Ns::Number, "language").map(ToOwned::to_owned);
        let mut country = element.attr(&Ns::Number, "country").map(ToOwned::to_owned);
        let mut parts = Vec::new();
        let mut conditions = Vec::new();
        for child in element.elements() {
            if child.is(&Ns::Style, "map") {
                if let (Some(condition), Some(style)) = (
                    child.attr(&Ns::Style, "condition"),
                    child.attr(&Ns::Style, "apply-style-name"),
                ) && let Some(condition) = Condition::parse(condition, style)
                {
                    conditions.push(condition);
                }
                continue;
            }
            if child.name.ns != Ns::Number {
                continue;
            }
            let long = child.attr(&Ns::Number, "style") == Some("long");
            let part = match &*child.name.local {
                "text" => Part::Text(child.plain_text()),
                "number" => Part::Number(digits(child)),
                "scientific-number" => Part::Scientific {
                    digits: digits(child),
                    exponent_digits: count(child, "min-exponent-digits").unwrap_or(2),
                    forced_sign: child.attr(&Ns::Number, "forced-exponent-sign") != Some("false"),
                },
                "fraction" => Part::Fraction {
                    integer: child.attr(&Ns::Number, "min-integer-digits").is_some(),
                    denominator: denominator(child),
                },
                "currency-symbol" => {
                    if language.is_none() {
                        language = child.attr(&Ns::Number, "language").map(ToOwned::to_owned);
                        country = child.attr(&Ns::Number, "country").map(ToOwned::to_owned);
                    }
                    Part::CurrencySymbol(child.plain_text())
                }
                "year" => Part::Year { long },
                "month" => Part::Month {
                    long,
                    textual: child.attr(&Ns::Number, "textual") == Some("true"),
                },
                "day" => Part::Day { long },
                "day-of-week" => Part::DayOfWeek { long },
                "hours" => Part::Hours { long },
                "minutes" => Part::Minutes { long },
                "seconds" => Part::Seconds { long },
                "am-pm" => Part::AmPm,
                "boolean" => Part::Boolean,
                "text-content" => Part::TextContent,
                _ => continue,
            };
            parts.push(part);
        }
        Some(Self {
            kind,
            parts,
            conditions,
            language,
            country,
        })
    }

    /// The text this style shows for a value, following its conditions to the
    /// style they name through `find`. `None` where the value is not of a kind
    /// the style shows, such as text in a currency cell.
    pub fn format<'a>(
        &'a self,
        value: &Value,
        find: impl Fn(&str) -> Option<&'a DataStyle>,
    ) -> Option<String> {
        if let Some(x) = amount(value) {
            if matches!(value, Value::Percentage(_)) && self.kind != Kind::Percentage {
                return None;
            }
            if !matches!(self.kind, Kind::Number | Kind::Currency | Kind::Percentage) {
                return None;
            }
            let chosen = self
                .conditions
                .iter()
                .find(|condition| condition.holds(x))
                .and_then(|condition| find(&condition.style))
                .unwrap_or(self);
            let x = if self.conditions.is_empty() {
                x
            } else {
                x.abs()
            };
            return Some(chosen.format_amount(x));
        }
        match (self.kind, value) {
            (Kind::Date | Kind::Time, Value::Date(text)) => {
                Some(self.format_moment(&Moment::of_date(text)?))
            }
            (Kind::Time, Value::Time(text)) => {
                Some(self.format_moment(&Moment::of_duration(text)?))
            }
            (Kind::Boolean, Value::Boolean(truth)) => Some(self.substitute(&self.truth(*truth))),
            (Kind::Text, Value::Text(text)) => Some(self.substitute(text)),
            _ => None,
        }
    }

    /// What a person typed, read the way this style shows a value: `$1,234.50`
    /// or `1234.5` in a currency cell, `15%` or `15` in a percentage cell,
    /// `11/5/2026` in a date cell written month first. `None` where the input
    /// is not in the style's form, for the caller to read as it would in a cell
    /// with no style.
    pub fn read_input(&self, input: &str) -> Option<Value> {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }
        match self.kind {
            Kind::Number => self.parse_number(input).map(Value::Number),
            Kind::Currency => {
                let mut bare = input.to_owned();
                for part in &self.parts {
                    if let Part::CurrencySymbol(symbol) = part
                        && !symbol.is_empty()
                    {
                        bare = bare.replace(symbol.as_str(), "");
                    }
                }
                let bare = bare.trim();
                let (negative, bare) =
                    match bare.strip_prefix('(').and_then(|b| b.strip_suffix(')')) {
                        Some(inner) => (true, inner.trim()),
                        None => (false, bare),
                    };
                let n = self.parse_number(bare.trim_start_matches('-').trim())?;
                let negative = negative || bare.starts_with('-');
                Some(Value::Currency(if negative { -n } else { n }, None))
            }
            Kind::Percentage => {
                let bare = input.strip_suffix('%').unwrap_or(input).trim();
                self.parse_number(bare)
                    .map(|n| Value::Percentage(n / 100.0))
            }
            Kind::Date => self.parse_date(input),
            Kind::Time => parse_time(input),
            Kind::Boolean => {
                let lower = input.to_lowercase();
                [true, false]
                    .into_iter()
                    .find(|truth| {
                        lower == self.truth(*truth).to_lowercase() || lower == truth.to_string()
                    })
                    .map(Value::Boolean)
            }
            Kind::Text => Some(Value::Text(input.to_owned())),
        }
    }

    /// The ISO 4217 code of the currency this style shows, where its symbol
    /// names one.
    pub fn currency_code(&self) -> Option<String> {
        let symbol = self.parts.iter().find_map(|part| match part {
            Part::CurrencySymbol(symbol) => Some(symbol.trim()),
            _ => None,
        })?;
        if symbol.len() == 3 && symbol.chars().all(|c| c.is_ascii_uppercase()) {
            return Some(symbol.to_owned());
        }
        let country = self.country.as_deref().unwrap_or_default();
        let code = match symbol {
            "$" => match country {
                "CA" => "CAD",
                "AU" => "AUD",
                "NZ" => "NZD",
                "MX" => "MXN",
                _ => "USD",
            },
            "€" => "EUR",
            "£" => "GBP",
            "¥" => {
                if country == "CN" {
                    "CNY"
                } else {
                    "JPY"
                }
            }
            "₹" => "INR",
            "CHF" | "Fr." => "CHF",
            _ => return None,
        };
        Some(code.to_owned())
    }

    fn format_amount(&self, x: f64) -> String {
        let (decimal, group) = separators(self.language.as_deref());
        let x = if self.kind == Kind::Percentage {
            x * 100.0
        } else {
            x
        };
        let signed = self.conditions.is_empty() && x < 0.0;
        let magnitude = x.abs();
        let mut out = String::new();
        let mut shown_number = false;
        for part in &self.parts {
            match part {
                Part::Text(text) | Part::CurrencySymbol(text) => out.push_str(text),
                Part::Number(digits) => {
                    out.push_str(&number(magnitude, digits, decimal, group));
                    shown_number = true;
                }
                Part::Scientific {
                    digits,
                    exponent_digits,
                    forced_sign,
                } => {
                    out.push_str(&scientific(
                        magnitude,
                        digits,
                        *exponent_digits,
                        *forced_sign,
                        decimal,
                    ));
                    shown_number = true;
                }
                Part::Fraction {
                    integer,
                    denominator,
                } => {
                    out.push_str(&fraction(magnitude, *integer, *denominator));
                    shown_number = true;
                }
                _ => {}
            }
        }
        if !shown_number {
            out.push_str(&number(
                magnitude,
                &Digits {
                    decimals: None,
                    min_decimals: 0,
                    min_integer: 1,
                    grouping: false,
                },
                decimal,
                group,
            ));
        }
        if signed && magnitude != 0.0 {
            out.insert(0, '-');
        }
        out
    }

    fn format_moment(&self, moment: &Moment) -> String {
        let twelve = self.parts.contains(&Part::AmPm);
        let names = Names::of(self.language.as_deref());
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Text(text) => out.push_str(text),
                Part::Year { long } => {
                    if *long {
                        let _ = write!(out, "{:04}", moment.year);
                    } else {
                        let _ = write!(out, "{:02}", moment.year.rem_euclid(100));
                    }
                }
                Part::Month { long, textual } => {
                    let index = (moment.month as usize).saturating_sub(1).min(11);
                    match (textual, long) {
                        (true, true) => out.push_str(names.months[index]),
                        (true, false) => out.push_str(&names.months[index][..names.short(index)]),
                        (false, true) => {
                            let _ = write!(out, "{:02}", moment.month);
                        }
                        (false, false) => out.push_str(&moment.month.to_string()),
                    }
                }
                Part::Day { long } => out.push_str(&pad(moment.day, *long)),
                Part::DayOfWeek { long } => {
                    let day = names.days[weekday(moment.year, moment.month, moment.day)];
                    if *long {
                        out.push_str(day);
                    } else {
                        out.push_str(&day[..3.min(day.len())]);
                    }
                }
                Part::Hours { long } => {
                    let hours = if twelve {
                        match moment.hours % 12 {
                            0 => 12,
                            h => h,
                        }
                    } else {
                        moment.hours
                    };
                    out.push_str(&pad(hours, *long));
                }
                Part::Minutes { long } => out.push_str(&pad(moment.minutes, *long)),
                Part::Seconds { long } => out.push_str(&pad(moment.seconds, *long)),
                Part::AmPm => out.push_str(if moment.hours < 12 { "AM" } else { "PM" }),
                _ => {}
            }
        }
        out
    }

    fn substitute(&self, text: &str) -> String {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Text(literal) => out.push_str(literal),
                Part::Boolean | Part::TextContent => out.push_str(text),
                _ => {}
            }
        }
        if out.is_empty() { text.to_owned() } else { out }
    }

    fn truth(&self, truth: bool) -> String {
        let german = self.language.as_deref() == Some("de");
        match (truth, german) {
            (true, false) => "TRUE",
            (false, false) => "FALSE",
            (true, true) => "WAHR",
            (false, true) => "FALSCH",
        }
        .to_owned()
    }

    fn parse_number(&self, input: &str) -> Option<f64> {
        let (decimal, group) = separators(self.language.as_deref());
        let mut text: String = input.chars().filter(|c| !c.is_whitespace()).collect();
        let other = if decimal == ',' { '.' } else { ',' };
        let mistaken = text.contains(other)
            && !text.contains(decimal)
            && text
                .rsplit(other)
                .next()
                .is_some_and(|tail| tail.len() != 3);
        if mistaken {
            text = text.replace(other, ".");
        } else {
            if let Some(group) = group {
                for c in group.chars() {
                    text = text.replace(c, "");
                }
            }
            text = text.replace(decimal, ".");
        }
        let n: f64 = text.parse().ok()?;
        n.is_finite().then_some(n)
    }

    fn parse_date(&self, input: &str) -> Option<Value> {
        if let Some(moment) = Moment::of_date(input)
            && input.len() >= 10
        {
            return Some(Value::Date(moment.iso_date()));
        }
        let order: Vec<char> = self
            .parts
            .iter()
            .filter_map(|part| match part {
                Part::Year { .. } => Some('y'),
                Part::Month { .. } => Some('m'),
                Part::Day { .. } => Some('d'),
                _ => None,
            })
            .collect();
        let names = Names::of(self.language.as_deref());
        let mut fields: Vec<(char, i64, usize)> = Vec::new();
        let taken = |fields: &[(char, i64, usize)], slot: char| fields.iter().any(|f| f.0 == slot);
        for token in input
            .split(|c: char| !c.is_alphanumeric())
            .filter(|token| !token.is_empty())
        {
            if let Ok(n) = token.parse::<i64>() {
                let slot = order.iter().copied().find(|slot| !taken(&fields, *slot))?;
                fields.push((slot, n, token.len()));
            } else if let Some(month) = names.month_named(token) {
                if taken(&fields, 'm') {
                    return None;
                }
                fields.push(('m', i64::from(month), token.len()));
            }
        }
        let get = |slot| fields.iter().find(|f| f.0 == slot);
        let (year, month, day) = (get('y')?, get('m')?.1, get('d')?.1);
        let year = match year {
            (_, n, width) if *width <= 2 && *n < 30 => 2000 + n,
            (_, n, width) if *width <= 2 => 1900 + n,
            (_, n, _) => *n,
        };
        let month = u32::try_from(month).ok()?;
        let day = u32::try_from(day).ok()?;
        let year = i32::try_from(year).ok()?;
        if !(1..=12).contains(&month) || day == 0 || day > days_in(year, month) {
            return None;
        }
        Some(Value::Date(format!("{year:04}-{month:02}-{day:02}")))
    }
}

/// A date or time as its parts.
struct Moment {
    year: i32,
    month: u32,
    day: u32,
    hours: u32,
    minutes: u32,
    seconds: u32,
}

impl Moment {
    /// `2026-11-05` or `2026-11-05T14:05:09`.
    fn of_date(text: &str) -> Option<Self> {
        let (date, time) = text.trim().split_once('T').unwrap_or((text.trim(), ""));
        let mut fields = date.splitn(3, '-');
        let year = fields.next()?.parse().ok()?;
        let month = fields.next()?.parse().ok()?;
        let day = fields.next()?.get(..2)?.parse().ok()?;
        if !(1..=12).contains(&month) || day == 0 || day > days_in(year, month) {
            return None;
        }
        let mut clock = time.split(':');
        let mut next = || -> u32 {
            clock
                .next()
                .and_then(|field| field.get(..2.min(field.len())))
                .and_then(|field| field.parse().ok())
                .unwrap_or(0)
        };
        Some(Self {
            year,
            month,
            day,
            hours: next(),
            minutes: next(),
            seconds: next(),
        })
    }

    /// `PT14H05M09S`, the duration a time cell holds.
    fn of_duration(text: &str) -> Option<Self> {
        let rest = text.trim().strip_prefix("PT").or_else(|| {
            text.trim()
                .strip_prefix('P')
                .and_then(|rest| rest.split_once('T').map(|(_, time)| time))
        })?;
        let mut moment = Self {
            year: 1899,
            month: 12,
            day: 30,
            hours: 0,
            minutes: 0,
            seconds: 0,
        };
        let mut digits = String::new();
        for c in rest.chars() {
            if c.is_ascii_digit() || c == '.' {
                digits.push(c);
                continue;
            }
            let whole: f64 = digits.parse().ok()?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = whole.trunc() as u32;
            match c {
                'H' => moment.hours = whole,
                'M' => moment.minutes = whole,
                'S' => moment.seconds = whole,
                _ => return None,
            }
            digits.clear();
        }
        Some(moment)
    }

    fn iso_date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// `14:05`, `14:05:09`, `2:05 PM` or `2:05pm`, as the duration a time cell
/// holds.
fn parse_time(input: &str) -> Option<Value> {
    let lower = input.trim().to_lowercase();
    let (clock, meridiem) = match lower
        .strip_suffix("am")
        .or_else(|| lower.strip_suffix('a'))
        .map(|clock| (clock, Some(false)))
        .or_else(|| {
            lower
                .strip_suffix("pm")
                .or_else(|| lower.strip_suffix('p'))
                .map(|clock| (clock, Some(true)))
        }) {
        Some(found) => found,
        None => (lower.as_str(), None),
    };
    let fields: Vec<u32> = clock
        .trim()
        .split(':')
        .map(|field| field.trim().parse().ok())
        .collect::<Option<_>>()?;
    let (&hours, rest) = fields.split_first()?;
    if (rest.is_empty() && meridiem.is_none()) || rest.len() > 2 {
        return None;
    }
    let minutes = rest.first().copied().unwrap_or(0);
    let seconds = rest.get(1).copied().unwrap_or(0);
    if minutes > 59 || seconds > 59 {
        return None;
    }
    let hours = match meridiem {
        Some(pm) if (1..=12).contains(&hours) => hours % 12 + if pm { 12 } else { 0 },
        Some(_) => return None,
        None => hours,
    };
    Some(Value::Time(format!(
        "PT{hours:02}H{minutes:02}M{seconds:02}S"
    )))
}

/// What a value amounts to as a number, where it is one.
fn amount(value: &Value) -> Option<f64> {
    match value {
        Value::Number(x) | Value::Percentage(x) | Value::Currency(x, _) => Some(*x),
        _ => None,
    }
}

fn count(element: &Element, attribute: &str) -> Option<usize> {
    element.attr(&Ns::Number, attribute)?.parse().ok()
}

fn digits(element: &Element) -> Digits {
    let decimals = count(element, "decimal-places");
    Digits {
        decimals,
        min_decimals: count(element, "min-decimal-places")
            .or(decimals)
            .unwrap_or(0),
        min_integer: count(element, "min-integer-digits").unwrap_or(1),
        grouping: element.attr(&Ns::Number, "grouping") == Some("true"),
    }
}

fn denominator(element: &Element) -> u32 {
    let attr = |name| -> Option<u32> { element.attr(&Ns::Number, name)?.parse().ok() };
    attr("denominator-value")
        .or_else(|| attr("max-denominator-value"))
        .or_else(|| {
            let digits = attr("min-denominator-digits")?.min(6);
            Some(10_u32.pow(digits) - 1)
        })
        .unwrap_or(9)
        .max(1)
}

/// The decimal separator and the thousands separator a language writes.
fn separators(language: Option<&str>) -> (char, Option<&'static str>) {
    match language.unwrap_or("en") {
        "de" | "es" | "it" | "nl" | "pt" | "da" | "tr" | "id" | "el" | "ro" | "hr" | "sl" => {
            (',', Some("."))
        }
        "fr" | "ru" | "pl" | "cs" | "sk" | "sv" | "nb" | "nn" | "no" | "fi" | "uk" | "hu"
        | "bg" | "et" | "lv" | "lt" => (',', Some("\u{a0}")),
        _ => ('.', Some(",")),
    }
}

fn number(x: f64, digits: &Digits, decimal: char, group: Option<&str>) -> String {
    let written = if let Some(places) = digits.decimals {
        let fixed = format!("{x:.places$}");
        match fixed.split_once('.') {
            Some((whole, fraction)) => {
                let keep = fraction
                    .trim_end_matches('0')
                    .len()
                    .max(digits.min_decimals);
                if keep == 0 {
                    whole.to_owned()
                } else {
                    format!("{whole}.{}", &fraction[..keep.min(fraction.len())])
                }
            }
            None => fixed,
        }
    } else {
        let general = format!("{x}");
        if general.contains('e') {
            format!("{x:.10}")
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_owned()
        } else {
            general
        }
    };
    let (whole, fraction) = written.split_once('.').unwrap_or((&written, ""));
    let whole = if whole == "0" && digits.min_integer == 0 {
        String::new()
    } else {
        format!("{whole:0>width$}", width = digits.min_integer)
    };
    let whole = match group.filter(|_| digits.grouping) {
        Some(separator) => grouped(&whole, separator),
        None => whole,
    };
    if fraction.is_empty() {
        whole
    } else {
        format!("{whole}{decimal}{fraction}")
    }
}

fn grouped(whole: &str, separator: &str) -> String {
    let mut out = String::new();
    for (index, c) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index).is_multiple_of(3) {
            out.push_str(separator);
        }
        out.push(c);
    }
    out
}

fn scientific(
    x: f64,
    digits: &Digits,
    exponent_digits: usize,
    forced_sign: bool,
    decimal: char,
) -> String {
    let places = digits.decimals.unwrap_or(2);
    let written = format!("{x:.places$e}");
    let (mantissa, exponent) = written.split_once('e').unwrap_or((&written, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let sign = if exponent < 0 {
        "-"
    } else if forced_sign {
        "+"
    } else {
        ""
    };
    format!(
        "{}E{sign}{:0>exponent_digits$}",
        mantissa.replace('.', &decimal.to_string()),
        exponent.unsigned_abs()
    )
}

fn fraction(x: f64, integer: bool, most: u32) -> String {
    let whole = if integer { x.trunc() } else { 0.0 };
    let rest = x - whole;
    let (mut numerator, mut denominator) = (0_u32, 1_u32);
    let mut best = rest.abs();
    for candidate in 1..=most {
        let scaled = rest * f64::from(candidate);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let top = scaled.round() as u32;
        let error = (rest - f64::from(top) / f64::from(candidate)).abs();
        if error + f64::EPSILON < best {
            best = error;
            numerator = top;
            denominator = candidate;
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    let mut whole = whole as i64;
    if integer && numerator == denominator {
        whole += 1;
        numerator = 0;
    }
    match (integer && whole != 0, numerator) {
        (true, 0) => whole.to_string(),
        (true, _) => format!("{whole} {numerator}/{denominator}"),
        (false, 0) => "0".to_owned(),
        (false, _) => format!("{numerator}/{denominator}"),
    }
}

fn pad(n: u32, long: bool) -> String {
    if long {
        format!("{n:02}")
    } else {
        n.to_string()
    }
}

fn leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in(year: i32, month: u32) -> u32 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The day of the week, Sunday first, by Sakamoto's method.
fn weekday(year: i32, month: u32, day: u32) -> usize {
    const OFFSETS: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let year = if month < 3 { year - 1 } else { year };
    let month_offset = OFFSETS[(month as usize).saturating_sub(1).min(11)];
    let day = i32::try_from(day).unwrap_or(1);
    let index = (year + year / 4 - year / 100 + year / 400 + month_offset + day).rem_euclid(7);
    usize::try_from(index).unwrap_or(0)
}

/// Month and day names in a style's language: English, or German where the
/// style says so.
struct Names {
    months: [&'static str; 12],
    days: [&'static str; 7],
}

impl Names {
    fn of(language: Option<&str>) -> Self {
        if language == Some("de") {
            Self {
                months: [
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
                days: [
                    "Sonntag",
                    "Montag",
                    "Dienstag",
                    "Mittwoch",
                    "Donnerstag",
                    "Freitag",
                    "Samstag",
                ],
            }
        } else {
            Self {
                months: [
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
                days: [
                    "Sunday",
                    "Monday",
                    "Tuesday",
                    "Wednesday",
                    "Thursday",
                    "Friday",
                    "Saturday",
                ],
            }
        }
    }

    /// The byte length of a month's short name: its first three letters.
    fn short(&self, index: usize) -> usize {
        self.months[index]
            .char_indices()
            .nth(3)
            .map_or(self.months[index].len(), |(at, _)| at)
    }

    /// The month a word names, in full or by its first three letters.
    fn month_named(&self, word: &str) -> Option<u32> {
        let word = word.to_lowercase();
        let index = self.months.iter().position(|month| {
            let month = month.to_lowercase();
            month == word || (word.chars().count() >= 3 && month.starts_with(&word))
        })?;
        u32::try_from(index + 1).ok()
    }
}

/// Every data style in a document, by name.
#[derive(Debug, Default)]
pub struct DataStyles(HashMap<String, DataStyle>);

impl DataStyles {
    /// Take in a data style element, if it is one.
    pub fn collect(&mut self, element: &Element) {
        if let (Some(name), Some(style)) =
            (element.attr(&Ns::Style, "name"), DataStyle::read(element))
        {
            self.0.insert(name.to_owned(), style);
        }
    }

    /// A data style by name.
    pub fn get(&self, name: &str) -> Option<&DataStyle> {
        self.0.get(name)
    }

    /// The text the named style shows for a value.
    pub fn format(&self, name: &str, value: &Value) -> Option<String> {
        self.get(name)?.format(value, |other| self.get(other))
    }
}
