//! Text read as a number where a formula wants one, as `LibreOffice` reads it:
//! in the conventions of United States English whatever the document's
//! language, measured on `corpus/libreoffice/formulas.ods`.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::eval::serial;

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// The number text stands for: a number, with its thousands grouped in
/// threes by commas, a sign before or after it or brackets round it, a `$`
/// before it and a `%` after it; `TRUE` or `FALSE`; a whole number and a
/// fraction, `1 1/2`; a date, `5/1/2024`, `2024-05-01`, `May 1, 2024`, a time,
/// `12:30`, or both. A date written without its year falls in `year`, and is
/// not a number where that is `None`.
pub(super) fn text_number(text: &str, year: Option<i64>) -> Option<f64> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("true") {
        return Some(1.0);
    }
    if text.eq_ignore_ascii_case("false") {
        return Some(0.0);
    }
    plain(text)
        .or_else(|| moment(text, year))
        .or_else(|| fraction(text))
}

fn plain(text: &str) -> Option<f64> {
    let (bracketed, text) = match text.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
        Some(inner) => (true, inner.trim()),
        None => (false, text),
    };
    let (percent, text) = match text.strip_suffix('%') {
        Some(rest) => (true, rest.trim_end()),
        None => (false, text),
    };
    let (sign, text) = match text.strip_prefix(['-', '+']) {
        Some(rest) => (text.chars().next(), rest.trim_start()),
        None => (None, text),
    };
    let text = text.strip_prefix('$').unwrap_or(text);
    let (trailing, text) = match text.strip_suffix('-') {
        Some(rest) if sign.is_none() && !bracketed => (true, rest),
        _ => (false, text),
    };
    let magnitude = digits(text)?;
    let n = if bracketed || trailing || sign == Some('-') {
        -magnitude
    } else {
        magnitude
    };
    Some(if percent { n / 100.0 } else { n })
}

fn all_digits(text: &str) -> bool {
    text.bytes().all(|b| b.is_ascii_digit())
}

/// A number with no sign: digits, grouped in threes by commas or not, a
/// decimal point and an exponent. One too large for a number is the largest.
fn digits(text: &str) -> Option<f64> {
    let (mantissa, exponent) = match text.find(['e', 'E']) {
        Some(at) => (&text[..at], Some(&text[at + 1..])),
        None => (text, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if (whole.is_empty() && fraction.is_empty()) || !all_digits(fraction) {
        return None;
    }
    let mut groups = whole.split(',');
    let first = groups.next()?;
    let mut written = first.to_owned();
    let mut grouped = false;
    for group in groups {
        if group.len() != 3 || !all_digits(group) {
            return None;
        }
        written.push_str(group);
        grouped = true;
    }
    if !all_digits(first) || (grouped && !(1..=3).contains(&first.len())) {
        return None;
    }
    written.push('.');
    written.push_str(fraction);
    if let Some(exponent) = exponent {
        let bare = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if bare.is_empty() || !all_digits(bare) {
            return None;
        }
        written.push('e');
        written.push_str(exponent);
    }
    let n: f64 = written.parse().ok()?;
    Some(if n.is_finite() { n } else { f64::MAX })
}

/// A date, a time of day, or a date and then a time after a space or a `T`.
fn moment(text: &str, year: Option<i64>) -> Option<f64> {
    if let Some(time) = clock(text) {
        return Some(time);
    }
    if let Some(day) = date(text, year) {
        return Some(day);
    }
    text.char_indices()
        .filter(|(_, c)| *c == ' ' || *c == 'T')
        .find_map(|(at, _)| Some(date(&text[..at], year)? + clock(text[at + 1..].trim())?))
}

/// `12:30`, `12:30:15.5`, `3:`, `9:30 pm`, as a fraction of a day; the hours
/// may run past a day.
fn clock(text: &str) -> Option<f64> {
    let lower = text.to_ascii_lowercase();
    let (text, afternoon) = match lower.strip_suffix("am") {
        Some(rest) => (rest.trim_end(), Some(false)),
        None => match lower.strip_suffix("pm") {
            Some(rest) => (rest.trim_end(), Some(true)),
            None => (lower.as_str(), None),
        },
    };
    let fields: Vec<&str> = text.split(':').collect();
    if !(2..=3).contains(&fields.len()) || fields[0].is_empty() || !all_digits(fields[0]) {
        return None;
    }
    let minutes = fields[1];
    if !all_digits(minutes) || (minutes.is_empty() && fields.len() == 3) {
        return None;
    }
    let seconds = fields.get(2).copied().unwrap_or("");
    let (whole, part) = seconds.split_once('.').unwrap_or((seconds, ""));
    if !all_digits(whole) || !all_digits(part) {
        return None;
    }
    let field = |f: &str| f.parse::<f64>().unwrap_or(0.0);
    let mut hours = field(fields[0]);
    let (minutes, seconds) = (field(minutes), field(&format!("0{seconds}")));
    if minutes >= 60.0 || seconds >= 60.0 {
        return None;
    }
    if let Some(afternoon) = afternoon {
        if !(1.0..=12.0).contains(&hours) {
            return None;
        }
        hours = hours % 12.0 + if afternoon { 12.0 } else { 0.0 };
    }
    Some((hours * 3600.0 + minutes * 60.0 + seconds) / 86_400.0)
}

/// `5/1/2024`, `5/1/24`, `5/1`, `2024-05-01`, `24-5-1`, `May 1, 2024`,
/// `May 2024`, `May 1`, as days from 30 December 1899.
fn date(text: &str, year: Option<i64>) -> Option<f64> {
    let number = |field: &str| {
        (!field.is_empty() && field.len() <= 4 && all_digits(field))
            .then(|| field.parse::<i64>().ok())
            .flatten()
    };
    let fields: Vec<&str> = text.split(['/', '-']).collect();
    let (y, m, d, width) = if text.contains('/') {
        match fields.as_slice() {
            [m, d] => (year?, number(m)?, number(d)?, 4),
            [m, d, y] => (number(y)?, number(m)?, number(d)?, y.len()),
            _ => return None,
        }
    } else if let [y, m, d] = fields.as_slice() {
        (number(y)?, number(m)?, number(d)?, y.len())
    } else {
        named(text, year)?
    };
    let y = match (width, y) {
        (..=2, ..30) => y + 2000,
        (..=2, _) => y + 1900,
        _ => y,
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let first = serial(y, m, 1);
    let next = if m == 12 {
        serial(y + 1, 1, 1)
    } else {
        serial(y, m + 1, 1)
    };
    #[allow(clippy::cast_precision_loss)]
    let day = first + (d - 1) as f64;
    (day < next).then_some(day)
}

/// A date that names its month: `May 1, 2024`, `May 2024`, `May 1`, with the
/// width the year was written in.
fn named(text: &str, year: Option<i64>) -> Option<(i64, i64, i64, usize)> {
    let mut words = text.split([' ', ',']).filter(|w| !w.is_empty());
    let name = words.next()?.to_ascii_lowercase();
    let month = MONTHS
        .iter()
        .position(|m| *m == name || (name.len() == 3 && m.starts_with(&name)))?;
    let month = i64::try_from(month).ok()? + 1;
    let rest: Vec<&str> = words.collect();
    let number = |w: &str| all_digits(w).then(|| w.parse::<i64>().ok()).flatten();
    match rest.as_slice() {
        [day, y] => Some((number(y)?, month, number(day)?, y.len())),
        [n] if n.len() > 2 => Some((number(n)?, month, 1, n.len())),
        [day] => Some((year?, month, number(day)?, 4)),
        _ => None,
    }
}

/// `1 1/2`, a whole number and a fraction.
fn fraction(text: &str) -> Option<f64> {
    let (whole, part) = text.split_once(' ')?;
    let (numerator, denominator) = part.trim().split_once('/')?;
    let (negative, whole) = match whole.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, whole),
    };
    if [whole, numerator, denominator]
        .iter()
        .any(|f| f.is_empty() || !all_digits(f))
    {
        return None;
    }
    let field = |f: &str| f.parse::<f64>().ok();
    let denominator = field(denominator)?;
    if denominator == 0.0 {
        return None;
    }
    let n = field(whole)? + field(numerator)? / denominator;
    Some(if negative { -n } else { n })
}
