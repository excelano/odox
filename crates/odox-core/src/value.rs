//! The value types ODF writes its measurements and colours in.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

/// A length, held in points.
///
/// ODF writes a length as a number and a unit, and the unit varies by writer and
/// by locale: the same document carries centimetres in its page layout and points
/// in its font sizes. Everything is converted on the way in so that nothing
/// downstream has to ask which unit it is looking at. A point is the unit a
/// renderer wants, being what a font size is already in.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Length(pub f32);

impl Length {
    /// Points.
    pub const fn points(self) -> f32 {
        self.0
    }

    /// Parse a length with its unit, as ODF writes one.
    ///
    /// `None` for anything unreadable, including a bare number: ODF requires a
    /// unit on every length, and a number with none is a writer's mistake whose
    /// intended unit cannot be guessed.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let split = text.len().checked_sub(2)?;
        if !text.is_char_boundary(split) {
            return None;
        }
        let (number, unit) = text.split_at(split);
        let value: f32 = number.trim().parse().ok()?;
        let points = match unit {
            "pt" => value,
            "in" => value * 72.0,
            "cm" => value * 72.0 / 2.54,
            "mm" => value * 72.0 / 25.4,
            // A pica is twelve points, and ODF permits it because XSL does.
            "pc" => value * 12.0,
            // Not an ODF unit and written by some producers anyway, read at the
            // CSS reference resolution of 96 pixels to the inch.
            "px" => value * 0.75,
            _ => return None,
        };
        Some(Self(points))
    }
}

/// A percentage, held as the number before the sign.
///
/// A font size, a line height and a column width can each be written as one,
/// relative to something the renderer knows and this crate does not.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Percent(pub f32);

impl Percent {
    /// The fraction this percentage is of its reference: 150% is 1.5.
    pub const fn fraction(self) -> f32 {
        self.0 / 100.0
    }

    /// Parse a percentage.
    pub fn parse(text: &str) -> Option<Self> {
        let value: f32 = text.trim().strip_suffix('%')?.trim().parse().ok()?;
        Some(Self(value))
    }
}

/// A length or a percentage, which is what ODF permits wherever it permits
/// either.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Measure {
    /// An absolute length.
    Absolute(Length),
    /// A proportion of whatever the property is relative to.
    Relative(Percent),
}

impl Measure {
    /// Parse either form.
    pub fn parse(text: &str) -> Option<Self> {
        if let Some(percent) = Percent::parse(text) {
            return Some(Self::Relative(percent));
        }
        Length::parse(text).map(Self::Absolute)
    }

    /// Resolve against the value the property is relative to, in points.
    pub fn resolve(self, reference: f32) -> f32 {
        match self {
            Self::Absolute(length) => length.points(),
            Self::Relative(percent) => reference * percent.fraction(),
        }
    }
}

/// An opaque colour.
///
/// ODF writes a colour as `#rrggbb` and has no notation for an alpha channel;
/// where something is meant to be see-through it says `transparent` in the
/// property instead, which is [`None`] here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Color {
    /// Parse `#rrggbb`.
    ///
    /// `None` for `transparent`, for the three-digit CSS form ODF does not
    /// define, and for anything else unreadable.
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.trim().strip_prefix('#')?;
        if hex.len() != 6 {
            return None;
        }
        Some(Self {
            r: u8::from_str_radix(&hex[0..2], 16).ok()?,
            g: u8::from_str_radix(&hex[2..4], 16).ok()?,
            b: u8::from_str_radix(&hex[4..6], 16).ok()?,
        })
    }
}

/// Read an ODF boolean attribute, which is spelled `true` or `false`.
pub(crate) fn boolean(text: &str) -> Option<bool> {
    match text.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_convert_to_points() {
        assert_eq!(Length::parse("12pt"), Some(Length(12.0)));
        assert_eq!(Length::parse("1in"), Some(Length(72.0)));
        assert_eq!(Length::parse("1pc"), Some(Length(12.0)));
        let cm = Length::parse("2.54cm").unwrap();
        assert!((cm.points() - 72.0).abs() < 0.01);
        let mm = Length::parse("25.4mm").unwrap();
        assert!((mm.points() - 72.0).abs() < 0.01);
    }

    #[test]
    fn a_length_needs_a_unit() {
        assert_eq!(Length::parse("12"), None);
        assert_eq!(Length::parse(""), None);
        assert_eq!(Length::parse("pt"), None);
    }

    #[test]
    fn a_multibyte_tail_is_not_split_through() {
        // Two bytes from the end of "12µm" is inside the µ, and splitting a
        // string there panics rather than returning.
        assert_eq!(Length::parse("12µm"), None);
    }

    #[test]
    fn colours_and_percentages() {
        assert_eq!(
            Color::parse("#ff8000"),
            Some(Color {
                r: 255,
                g: 128,
                b: 0
            })
        );
        assert_eq!(Color::parse("transparent"), None);
        assert_eq!(Color::parse("#abc"), None);
        assert_eq!(Percent::parse("150%").map(Percent::fraction), Some(1.5));
    }
}
