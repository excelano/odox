//! How a locale writes numbers, dates and amounts, from what `LibreOffice`
//! shows in it. The table is `locales.rs`, which `locale.py` writes by asking
//! `LibreOffice`.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use crate::locales::LOCALES;

/// The currency a new amount is shown in, and where its symbol goes.
pub(crate) struct Currency {
    pub symbol: &'static str,
    /// The ISO 4217 code.
    pub code: &'static str,
    /// Whether the symbol goes before the amount.
    pub before: bool,
    /// What stands between the symbol and the amount.
    pub between: &'static str,
}

/// One locale.
pub(crate) struct Locale {
    pub language: &'static str,
    pub country: &'static str,
    pub decimal: &'static str,
    pub group: &'static str,
    /// Whether digits are grouped in threes and then in twos, as in India.
    pub indian: bool,
    pub months: [&'static str; 12],
    pub months_short: [&'static str; 12],
    /// Sunday first.
    pub days: [&'static str; 7],
    pub days_short: [&'static str; 7],
    pub quarters: [&'static str; 4],
    pub quarters_long: [&'static str; 4],
    /// True, then false.
    pub truth: [&'static str; 2],
    pub currency: Currency,
}

impl Locale {
    /// The decimal separator.
    pub fn decimal(&self) -> char {
        self.decimal.chars().next().unwrap_or('.')
    }
}

/// The locale of a language and country: that one where it is in the table,
/// the first of the language where only the language is, and United States
/// English otherwise.
pub(crate) fn find(language: Option<&str>, country: Option<&str>) -> &'static Locale {
    let language = language.unwrap_or("en");
    LOCALES
        .iter()
        .find(|locale| locale.language == language && Some(locale.country) == country)
        .or_else(|| LOCALES.iter().find(|locale| locale.language == language))
        .unwrap_or(&LOCALES[0])
}

/// The ISO 4217 code of the currency a symbol names: the one of the country
/// where it uses that symbol, and otherwise the first locale's that does.
/// Half-width `¥` and `₩` are the full-width ones `LibreOffice` writes.
pub(crate) fn currency_code(symbol: &str, country: Option<&str>) -> Option<&'static str> {
    if symbol == "Fr." {
        return Some("CHF");
    }
    let symbol: String = symbol
        .chars()
        .map(|c| match c {
            '¥' => '\u{ffe5}',
            '₩' => '\u{ffe6}',
            other => other,
        })
        .collect();
    let using = |locale: &&Locale| locale.currency.symbol == symbol;
    LOCALES
        .iter()
        .filter(using)
        .find(|locale| Some(locale.country) == country)
        .or_else(|| LOCALES.iter().find(using))
        .map(|locale| locale.currency.code)
}
