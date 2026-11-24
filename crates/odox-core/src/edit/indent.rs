//! A paragraph's left indent, stepped a tab's width at a time, and the space
//! between its lines. DESIGN.md §11.
//!
//! Both are paragraph properties written into an automatic style, as an
//! alignment is (`edit::set_alignment`). A paragraph in a list item indents
//! by its level in the list instead.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::align::restyle_paragraphs;
use super::block::declares;
use super::list::list_level;
use super::{Refused, is_paragraph};
use crate::style::{Family, ParagraphProperties, Styles};
use crate::value::{Length, Measure};
use crate::xml::{Element, Ns};

/// The line spacings offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineSpacing {
    /// One line's height.
    Single,
    /// 115%.
    OnePointOneFive,
    /// 150%.
    OnePointFive,
    /// Two lines' height.
    Double,
}

/// Move the left indent of paragraphs a tab's width in (`step` above zero) or
/// out, never past the left margin; a paragraph in a list item moves a level
/// in its list instead.
///
/// # Errors
///
/// As `edit::set_alignment`, and as `edit::list_level` for a paragraph in a
/// list.
pub fn set_indent(
    content: &mut Element,
    paths: &[Vec<usize>],
    step: i8,
    styles: &mut Styles,
) -> Result<(), Refused> {
    for path in paths {
        content
            .at(path)
            .filter(|e| is_paragraph(e))
            .ok_or(Refused::NotFound)?;
    }
    let in_item = |path: &&Vec<usize>| {
        path.split_last().is_some_and(|(_, item)| {
            content
                .at(item)
                .is_some_and(|e| e.is(&Ns::Text, "list-item") || e.is(&Ns::Text, "list-header"))
        })
    };
    let (listed, plain): (Vec<&Vec<usize>>, Vec<&Vec<usize>>) = paths.iter().partition(in_item);
    let listed: Vec<Vec<usize>> = listed.into_iter().cloned().collect();
    let plain: Vec<Vec<usize>> = plain.into_iter().cloned().collect();
    if !plain.is_empty() {
        declares(content, &[Ns::Text, Ns::Style, Ns::Fo])?;
    }
    if !listed.is_empty() {
        list_level(content, &listed, step)?;
    }
    restyle_paragraphs(content, &plain, styles, |content, path, styles| {
        let properties = resolved(content, path, styles);
        let left = properties.margin.left.map_or(0.0, Length::points);
        let usual = 1.25 / 2.54 * 72.0;
        let distance = properties.tab_distance.map_or(usual, Length::points);
        let hanging = properties
            .text_indent
            .map_or(0.0, |indent| -indent.points());
        let wanted = f32::from(step)
            .mul_add(distance, left)
            .max(hanging.max(0.0));
        ((wanted - left).abs() > 0.001)
            .then(|| vec![(Ns::Fo, "margin-left", Some(centimetres(wanted)))])
    })
}

/// A length in points, written in centimetres to the thousandth.
fn centimetres(points: f32) -> String {
    let written = format!("{:.3}", points / 72.0 * 2.54);
    format!("{}cm", written.trim_end_matches('0').trim_end_matches('.'))
}

/// The paragraph properties a paragraph's style gives it.
fn resolved(content: &Element, path: &[usize], styles: &Styles) -> ParagraphProperties {
    let name = content
        .at(path)
        .filter(|e| is_paragraph(e))
        .and_then(|paragraph| paragraph.attr(&Ns::Text, "style-name"))
        .unwrap_or("Standard");
    styles.resolve(&Family::Paragraph, name).paragraph.clone()
}

impl LineSpacing {
    fn percent(self) -> u16 {
        match self {
            Self::Single => 100,
            Self::OnePointOneFive => 115,
            Self::OnePointFive => 150,
            Self::Double => 200,
        }
    }
}

/// Space the lines of paragraphs.
///
/// # Errors
///
/// As `edit::set_alignment`.
pub fn set_line_spacing(
    content: &mut Element,
    paths: &[Vec<usize>],
    spacing: LineSpacing,
    styles: &mut Styles,
) -> Result<(), Refused> {
    restyle_paragraphs(content, paths, styles, |content, path, styles| {
        (line_spacing(content, path, styles) != Some(spacing)).then(|| {
            vec![
                (
                    Ns::Fo,
                    "line-height",
                    Some(format!("{}%", spacing.percent())),
                ),
                (Ns::Style, "line-height-at-least", None),
                (Ns::Style, "line-spacing", None),
            ]
        })
    })
}

/// The line spacing a paragraph has, where it is one of those offered;
/// a paragraph whose style says nothing is single spaced.
pub fn line_spacing(content: &Element, path: &[usize], styles: &Styles) -> Option<LineSpacing> {
    let properties = resolved(content, path, styles);
    if properties.other_line_height == Some(true) {
        return None;
    }
    match properties.line_height {
        None => Some(LineSpacing::Single),
        Some(Measure::Relative(percent)) => [
            LineSpacing::Single,
            LineSpacing::OnePointOneFive,
            LineSpacing::OnePointFive,
            LineSpacing::Double,
        ]
        .into_iter()
        .find(|spacing| (percent.fraction() * 100.0 - f32::from(spacing.percent())).abs() < 0.5),
        Some(Measure::Absolute(_)) => None,
    }
}
