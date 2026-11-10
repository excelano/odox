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
use crate::value::Color;
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
    /// A number, and the literal text `number:embedded-text` puts among its
    /// integer digits, each with how many digits stand to its right.
    Number(Digits, Vec<(usize, String)>),
    Scientific {
        digits: Digits,
        exponent_digits: usize,
        forced_sign: bool,
        /// What the exponent is a multiple of: three for engineering
        /// notation.
        interval: i32,
    },
    Fraction(Fraction),
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
    Quarter {
        long: bool,
    },
    WeekOfYear,
    Hours {
        long: bool,
    },
    Minutes {
        long: bool,
    },
    Seconds {
        long: bool,
        decimals: u32,
    },
    AmPm,
    Boolean,
    TextContent,
}

/// A fraction: whether it has a whole part, its denominator, which is the
/// largest it may be unless it is `fixed`, and the widths its numerator and
/// denominator are padded to with figure spaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fraction {
    integer: bool,
    denominator: u32,
    fixed: bool,
    numerator_digits: usize,
    denominator_digits: usize,
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
    colour: Option<Color>,
    /// Whether a time is shown as the whole duration in its largest unit,
    /// `[HH]`, `[MM]` or `[SS]`, rather than as the time of day it comes to:
    /// `[MM]:SS` shows 144:00 for two hours and twenty-four minutes.
    elapsed: bool,
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
        let mut colour = None;
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
            if child.is(&Ns::Style, "text-properties") {
                colour = child.attr(&Ns::Fo, "color").and_then(Color::parse);
                continue;
            }
            if child.name.ns != Ns::Number {
                continue;
            }
            let long = child.attr(&Ns::Number, "style") == Some("long");
            let part = match &*child.name.local {
                "text" => Part::Text(child.plain_text()),
                "number" => Part::Number(digits(child), embedded(child)),
                "scientific-number" => Part::Scientific {
                    digits: digits(child),
                    exponent_digits: count(child, "min-exponent-digits").unwrap_or(2),
                    forced_sign: child.attr(&Ns::Number, "forced-exponent-sign") != Some("false"),
                    interval: count(child, "exponent-interval")
                        .and_then(|n| i32::try_from(n).ok())
                        .unwrap_or(1)
                        .max(1),
                },
                "fraction" => Part::Fraction(Fraction {
                    integer: child.attr(&Ns::Number, "min-integer-digits").is_some(),
                    denominator: denominator(child),
                    fixed: child.attr(&Ns::Number, "denominator-value").is_some(),
                    numerator_digits: count(child, "min-numerator-digits").unwrap_or(1),
                    denominator_digits: count(child, "min-denominator-digits").unwrap_or(1),
                }),
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
                "quarter" => Part::Quarter { long },
                "week-of-year" => Part::WeekOfYear,
                "hours" => Part::Hours { long },
                "minutes" => Part::Minutes { long },
                "seconds" => Part::Seconds {
                    long,
                    decimals: count(child, "decimal-places")
                        .and_then(|n| u32::try_from(n).ok())
                        .unwrap_or(0)
                        .min(9),
                },
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
            colour,
            elapsed: element.attr(&Ns::Number, "truncate-on-overflow") == Some("false"),
        })
    }

    /// The colour this style gives a value's text, from the section its
    /// conditions choose; `None` where the style gives none, or does not show
    /// the value.
    pub fn colour<'a>(
        &'a self,
        value: &Value,
        find: impl Fn(&str) -> Option<&'a DataStyle> + Copy,
    ) -> Option<Color> {
        self.format(value, find)?;
        match amount(value) {
            Some(x) => self.section(x, find).colour,
            None => self.colour,
        }
    }

    /// The style a number is shown in: the one the first condition it meets
    /// names, or this one.
    fn section<'a>(&'a self, x: f64, find: impl Fn(&str) -> Option<&'a DataStyle>) -> &'a Self {
        self.conditions
            .iter()
            .find(|condition| condition.holds(x))
            .and_then(|condition| find(&condition.style))
            .unwrap_or(self)
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
            if matches!(value, Value::Number(_)) && matches!(self.kind, Kind::Date | Kind::Time) {
                return Moment::of_serial(
                    x,
                    self.kind == Kind::Time && self.elapsed,
                    self.second_places(),
                )
                .map(|moment| self.format_moment(&moment));
            }
            if matches!(value, Value::Percentage(_)) && self.kind != Kind::Percentage {
                return None;
            }
            let chosen = self.section(x, find);
            if !matches!(
                chosen.kind,
                Kind::Number | Kind::Currency | Kind::Percentage
            ) {
                return None;
            }
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
                let days = duration_days(text)?;
                Some(self.format_moment(&Moment::of_serial(
                    days,
                    self.elapsed,
                    self.second_places(),
                )?))
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

    /// How many decimals a number is shown with, and whether its thousands
    /// are grouped, where the style shows a number.
    pub fn digits(&self) -> Option<(usize, bool)> {
        self.parts.iter().find_map(|part| match part {
            Part::Number(digits, _) => Some((digits.decimals.unwrap_or(0), digits.grouping)),
            _ => None,
        })
    }

    /// How many decimals the seconds are shown with.
    fn second_places(&self) -> u32 {
        self.parts
            .iter()
            .find_map(|part| match part {
                Part::Seconds { decimals, .. } => Some(*decimals),
                _ => None,
            })
            .unwrap_or(0)
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
                Part::Number(digits, embedded) => {
                    out.push_str(&number(magnitude, digits, embedded, decimal, group));
                    shown_number = true;
                }
                Part::Scientific {
                    digits,
                    exponent_digits,
                    forced_sign,
                    interval,
                } => {
                    out.push_str(&scientific(
                        magnitude,
                        digits,
                        (*exponent_digits, *forced_sign, *interval),
                        decimal,
                    ));
                    shown_number = true;
                }
                Part::Fraction(spec) => {
                    out.push_str(&fraction(magnitude, spec));
                    shown_number = true;
                }
                _ => {}
            }
        }
        if !shown_number && !self.parts.iter().any(|part| matches!(part, Part::Text(_))) {
            out.push_str(&number(
                magnitude,
                &Digits {
                    decimals: None,
                    min_decimals: 0,
                    min_integer: 1,
                    grouping: false,
                },
                &[],
                decimal,
                group,
            ));
        }
        if signed && out.chars().any(|c| c.is_ascii_digit() && c != '0') {
            out.insert(0, '-');
        }
        out
    }

    fn format_moment(&self, moment: &Moment) -> String {
        let twelve = self.parts.contains(&Part::AmPm);
        let names = Names::of(self.language.as_deref());
        let lead = self.parts.iter().find(|part| {
            matches!(
                part,
                Part::Hours { .. } | Part::Minutes { .. } | Part::Seconds { .. }
            )
        });
        let leads = |unit: fn(&Part) -> bool| self.elapsed && lead.is_some_and(unit);
        let minutes = if leads(|p| matches!(p, Part::Minutes { .. })) {
            moment.hours * 60 + moment.minutes
        } else {
            moment.minutes
        };
        let seconds = if leads(|p| matches!(p, Part::Seconds { .. })) {
            (moment.hours * 60 + moment.minutes) * 60 + moment.seconds
        } else {
            moment.seconds
        };
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
                    } else if self.elapsed {
                        moment.hours
                    } else {
                        moment.hours % 24
                    };
                    out.push_str(&pad(hours, *long));
                }
                Part::Quarter { long } => out.push_str(&names.quarter(moment.month, *long)),
                Part::WeekOfYear => {
                    out.push_str(&iso_week(moment.year, moment.month, moment.day).to_string());
                }
                Part::Minutes { long } => out.push_str(&pad(minutes, *long)),
                Part::Seconds { long, decimals } => {
                    out.push_str(&pad(seconds, *long));
                    if *decimals > 0 {
                        let places = *decimals as usize;
                        let (decimal, _) = separators(self.language.as_deref());
                        let _ = write!(out, "{decimal}{:0>places$}", moment.subsecond);
                    }
                }
                Part::AmPm => out.push_str(if moment.hours % 24 < 12 { "AM" } else { "PM" }),
                _ => {}
            }
        }
        if moment.negative {
            out.insert(0, '-');
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
            let after_decimal = text.split_once(decimal).map_or("", |(_, rest)| rest);
            if group.is_some_and(|group| group.chars().any(|c| after_decimal.contains(c))) {
                return None;
            }
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
            let date = moment.iso_date();
            return Some(Value::Date(if input.contains('T') {
                format!(
                    "{date}T{:02}:{:02}:{:02}",
                    moment.hours, moment.minutes, moment.seconds
                )
            } else {
                date
            }));
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
    /// The part of a second shown, in as many decimal places as the style
    /// shows.
    subsecond: u64,
    /// A duration below zero.
    negative: bool,
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
            subsecond: 0,
            negative: false,
        })
    }

    /// A count of days from 30 December 1899 as a date and time of day, or,
    /// as a `duration`, as the hours, minutes and seconds it comes to, rounded
    /// to `places` decimals of a second; `None` for a date no calendar here
    /// reaches.
    fn of_serial(serial: f64, duration: bool, places: u32) -> Option<Self> {
        if !serial.is_finite() || !(-693_594.0..=2_958_466.0).contains(&serial) {
            return None;
        }
        let scale = 10_i64.pow(places);
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let units = (serial * 86_400.0 * scale as f64).round() as i64;
        let subsecond = units.unsigned_abs() % scale.unsigned_abs();
        let total = units.signum() * (units.abs() / scale);
        let (days, of_day) = (total.div_euclid(86_400), total.rem_euclid(86_400));
        #[allow(clippy::cast_precision_loss)]
        let (year, month, day) = crate::formula::eval_civil(days as f64);
        let clock = if duration {
            total.unsigned_abs()
        } else {
            of_day.unsigned_abs()
        };
        Some(Self {
            year: i32::try_from(year).ok()?,
            month: u32::try_from(month).ok()?,
            day: u32::try_from(day).ok()?,
            hours: u32::try_from(clock / 3600).ok()?,
            minutes: u32::try_from(clock / 60 % 60).ok()?,
            seconds: u32::try_from(clock % 60).ok()?,
            subsecond,
            negative: duration && units < 0,
        })
    }

    fn iso_date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// `PT14H05M09S`, `P1DT02H` or `-PT01H`, the duration a time cell holds, in
/// days.
pub(crate) fn duration_days(text: &str) -> Option<f64> {
    let text = text.trim();
    if let Some(positive) = text.strip_prefix('-') {
        return duration_days(positive).map(|days| -days);
    }
    let rest = text.strip_prefix('P')?;
    let (days, time) = rest.split_once('T').unwrap_or((rest, ""));
    let days: f64 = if days.is_empty() {
        0.0
    } else {
        days.strip_suffix('D')?.parse().ok()?
    };
    let mut seconds = 0.0;
    let mut digits = String::new();
    for c in time.chars() {
        if c.is_ascii_digit() || c == '.' {
            digits.push(c);
            continue;
        }
        let n: f64 = digits.parse().ok()?;
        seconds += n * match c {
            'H' => 3600.0,
            'M' => 60.0,
            'S' => 1.0,
            _ => return None,
        };
        digits.clear();
    }
    Some(days + seconds / 86_400.0)
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

/// The literal text a number element puts among its integer digits, each
/// with how many digits stand to its right.
fn embedded(number: &Element) -> Vec<(usize, String)> {
    number
        .elements()
        .filter(|child| child.is(&Ns::Number, "embedded-text"))
        .filter_map(|child| Some((count(child, "position")?, child.plain_text())))
        .collect()
}

/// A number with no decimals stated, as a spreadsheet's General format
/// writes it, measured against `LibreOffice` in `corpus/libreoffice/codes.ods`:
/// up to fifteen significant digits, and an integer below 2^53 in full. From
/// 2^53 up it is written with an exponent of at least three digits,
/// `1E+016`. Below a ten-thousandth it keeps to decimals while that takes no
/// more than sixteen places and the number is at least a ten-billionth, and is
/// otherwise written with an exponent of at least two digits, `1E-10`.
fn general(x: f64) -> String {
    const EXACT: f64 = 9_007_199_254_740_992.0;
    if x == 0.0 || !x.is_finite() {
        return "0".to_owned();
    }
    if x < EXACT && x.fract() == 0.0 {
        return format!("{x:.0}");
    }
    let written = format!("{x:.14e}");
    let (mantissa, exponent) = written.split_once('e').unwrap_or((&written, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let significant = mantissa.replace('.', "");
    let significant = significant.trim_end_matches('0');
    let count = i32::try_from(significant.len()).unwrap_or(15);
    let decimal = x < EXACT && (exponent >= -4 || (exponent >= -9 && count - 1 - exponent <= 16));
    if !decimal {
        let (first, rest) = significant.split_at(1);
        let mantissa = if rest.is_empty() {
            first.to_owned()
        } else {
            format!("{first}.{rest}")
        };
        let (sign, width) = if exponent < 0 { ('-', 2) } else { ('+', 3) };
        return format!("{mantissa}E{sign}{:0>width$}", exponent.unsigned_abs());
    }
    if exponent < 0 {
        let zeros = "0".repeat(exponent.unsigned_abs() as usize - 1);
        return format!("0.{zeros}{significant}");
    }
    let point = exponent.unsigned_abs() as usize + 1;
    if significant.len() <= point {
        format!("{significant:0<point$}")
    } else {
        format!("{}.{}", &significant[..point], &significant[point..])
    }
}

fn number(
    x: f64,
    digits: &Digits,
    embedded: &[(usize, String)],
    decimal: char,
    group: Option<&str>,
) -> String {
    let written = if let Some(places) = digits.decimals {
        let x = half_away(x, places);
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
        let general = general(x);
        if general.contains('E') {
            return general.replace('.', &decimal.to_string());
        }
        general
    };
    let (whole, fraction) = written.split_once('.').unwrap_or((&written, ""));
    let mut whole = if whole == "0" && digits.min_integer == 0 {
        String::new()
    } else {
        format!("{whole:0>width$}", width = digits.min_integer)
    };
    let mut embedded = embedded.to_vec();
    embedded.sort_by_key(|(position, _)| std::cmp::Reverse(*position));
    for (position, text) in embedded {
        if let Some(at) = whole.len().checked_sub(position) {
            whole.insert_str(at, &text);
        }
    }
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

/// A number rounded to `places` decimals with a half going away from zero,
/// as a spreadsheet shows it, after it is taken to the fifteen significant
/// digits it is written with: `1.005` to two places is `1.01`.
fn half_away(x: f64, places: usize) -> f64 {
    let factor = 10f64.powi(i32::try_from(places).unwrap_or(i32::MAX));
    let scaled = x * factor;
    if !scaled.is_finite() {
        return x;
    }
    crate::formula::approx(scaled).round() / factor
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

/// A number with an exponent that is a multiple of `interval`, its mantissa
/// to the style's decimals: `1.23E+04`, or `12.35E+03` in engineering
/// notation.
fn scientific(
    x: f64,
    digits: &Digits,
    (exponent_digits, forced_sign, interval): (usize, bool, i32),
    decimal: char,
) -> String {
    let places = digits.decimals.unwrap_or(2);
    #[allow(clippy::cast_possible_truncation)]
    let mut exponent = if x > 0.0 && x.is_finite() {
        let magnitude = x.log10().floor() as i32;
        magnitude - magnitude.rem_euclid(interval)
    } else {
        0
    };
    let mut mantissa = half_away(x / 10f64.powi(exponent), places);
    if mantissa >= 10f64.powi(interval) {
        exponent += interval;
        mantissa = half_away(x / 10f64.powi(exponent), places);
    }
    let mantissa = format!("{mantissa:.places$}");
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

/// A fraction as `LibreOffice` writes one, measured in
/// `corpus/libreoffice/codes.ods`: the whole part, blank where it is nought
/// and there is a fraction, then a space and the fraction, its numerator
/// padded on the left and a denominator that is not fixed padded on the
/// right with figure spaces. A whole number is the number, then blanks where
/// the fraction would be.
fn fraction(x: f64, spec: &Fraction) -> String {
    let whole = if spec.integer { x.trunc() } else { 0.0 };
    let rest = x - whole;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (mut numerator, denominator) = if spec.fixed {
        (
            (rest * f64::from(spec.denominator)).round() as u32,
            spec.denominator,
        )
    } else {
        let (mut numerator, mut denominator) = (0_u32, 1_u32);
        let mut best = rest.abs();
        for candidate in 1..=spec.denominator {
            let top = (rest * f64::from(candidate)).round() as u32;
            let error = (rest - f64::from(top) / f64::from(candidate)).abs();
            if error + f64::EPSILON < best {
                best = error;
                numerator = top;
                denominator = candidate;
            }
        }
        (numerator, denominator)
    };
    #[allow(clippy::cast_possible_truncation)]
    let mut whole = whole as i64;
    if spec.integer && numerator == denominator {
        whole += 1;
        numerator = 0;
    }
    let blank = |width: usize| "\u{2007}".repeat(width);
    let below = if spec.fixed {
        String::new()
    } else {
        blank(spec.denominator_digits)
    };
    match (spec.integer, numerator) {
        (true, 0) => format!("{whole} {} {below}", blank(spec.numerator_digits)),
        (false, 0) => "0".to_owned(),
        (integer, _) => {
            let top = numerator.to_string();
            let bottom = denominator.to_string();
            let mut written = format!(
                "{}{top}/{bottom}",
                blank(spec.numerator_digits.saturating_sub(top.len()))
            );
            if !spec.fixed {
                written.push_str(&blank(spec.denominator_digits.saturating_sub(bottom.len())));
            }
            if integer {
                let shown = if whole == 0 {
                    String::new()
                } else {
                    whole.to_string()
                };
                format!("{shown} {written}")
            } else {
                written
            }
        }
    }
}

/// The ISO 8601 week a date falls in, Monday first.
fn iso_week(year: i32, month: u32, day: u32) -> u32 {
    let day_of_year = (1..month).map(|m| days_in(year, m)).sum::<u32>() + day;
    let monday_first = u32::try_from((weekday(year, month, day) + 6) % 7).unwrap_or(0);
    let week = (day_of_year + 9 - (monday_first + 1)) / 7;
    if week == 0 {
        iso_week(year - 1, 12, 31)
    } else if week == 53 {
        let december_31 = u32::try_from((weekday(year, 12, 31) + 6) % 7).unwrap_or(0);
        if december_31 < 3 { 1 } else { 53 }
    } else {
        week
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

    /// The quarter a month falls in, `Q2` or, long, `2nd quarter`.
    #[allow(clippy::unused_self)]
    fn quarter(&self, month: u32, long: bool) -> String {
        let quarter = (month.clamp(1, 12) - 1) / 3 + 1;
        if long {
            let ordinal = ["1st", "2nd", "3rd", "4th"][quarter as usize - 1];
            format!("{ordinal} quarter")
        } else {
            format!("Q{quarter}")
        }
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

    /// The colour the named style gives a value's text, where it gives one.
    pub fn colour(&self, name: &str, value: &Value) -> Option<Color> {
        self.get(name)?.colour(value, |other| self.get(other))
    }
}
