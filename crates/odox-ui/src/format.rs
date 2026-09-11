//! Turning resolved ODF properties into what egui draws text with.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{Align, Color32, FontFamily, FontId, Stroke, TextFormat};
use odox_core::{Color, Measure, Position, TextProperties};

use crate::fonts::{Variant, family_of};

/// The font size a document that names none is drawn at.
///
/// ODF has no default, and every producer writes one into its default paragraph
/// style, so this is reached only by a document with no styles at all.
pub const DEFAULT_SIZE: f32 = 12.0;

/// How a superscript or subscript is drawn, as a proportion of the run's size.
///
/// ODF writes the offset and the scale per run, and honouring them exactly would
/// mean placing a glyph at a baseline offset egui's text layout does not offer.
/// This is the proportion every office application uses by default.
const SCRIPT_SCALE: f32 = 0.58;

/// The colours a document is drawn in where the document itself does not say.
///
/// **A page is paper, in a dark window as much as a light one.** The colours in
/// a document are the document's: a heading the author made near-black is drawn
/// near-black, and drawing it on a dark ground because the desktop asked for a
/// dark desktop makes it invisible. Found by opening a deck Impress wrote, whose
/// title is dark by its master page's style and disappeared. So the page keeps
/// its own ground and the window's chrome — menus, panels, the grid's headers —
/// follows the desktop. That is what every office application and every PDF
/// viewer does with a page, and what the theme is for everywhere else.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// What the page itself is.
    pub paper: Color32,
    /// The colour of text the document does not colour.
    pub ink: Color32,
    /// The colour of a link the document does not colour.
    pub link: Color32,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            paper: Color32::from_rgb(0xff, 0xff, 0xff),
            // ODF's own default, and what a producer means by writing no colour.
            ink: Color32::from_rgb(0x00, 0x00, 0x00),
            // Legible on paper and recognizable as a link, which the window's own
            // hyperlink colour is not: that one is chosen against the window's
            // background and can be a pale blue meant for a dark panel.
            link: Color32::from_rgb(0x1a, 0x5f, 0xb4),
        }
    }
}

/// What a resolved run of text is drawn with.
///
/// `inherited` is the size in points of the text this run sits inside, which is
/// what a relative font size is relative to; `zoom` scales points to the screen.
pub fn text_format(
    properties: &TextProperties,
    inherited: f32,
    zoom: f32,
    palette: Palette,
) -> TextFormat {
    let size = match properties.size {
        Some(measure) => measure.resolve(inherited),
        None => inherited,
    };
    let variant = Variant {
        bold: properties.bold.unwrap_or(false),
        italic: properties.italic.unwrap_or(false),
    };
    let family = match &properties.font_family {
        Some(name) if !name.is_empty() => family_of(name, variant),
        // A document that names no family is drawn in egui's own, which is the
        // one face that is certainly present.
        _ => FontFamily::Proportional,
    };

    let (scale, valign) = match properties.position {
        Some(Position::Super) => (SCRIPT_SCALE, Align::TOP),
        Some(Position::Sub) => (SCRIPT_SCALE, Align::BOTTOM),
        _ => (1.0, Align::BOTTOM),
    };

    let color = properties.color.map_or(palette.ink, color32);
    let line = Stroke::new((size * zoom * 0.06).max(1.0), color);

    TextFormat {
        font_id: FontId::new(size * zoom * scale, family),
        color,
        background: properties.background.map_or(Color32::TRANSPARENT, color32),
        underline: if properties.underline.unwrap_or(false) {
            line
        } else {
            Stroke::NONE
        },
        strikethrough: if properties.strike.unwrap_or(false) {
            line
        } else {
            Stroke::NONE
        },
        valign,
        // The italic face was asked for by name above. egui's own `italics` skews
        // the regular face, which is a different drawing from the one the font
        // designer made, and setting both would skew an italic face further.
        italics: false,
        ..TextFormat::default()
    }
}

/// The same, for a run the document marks as a hyperlink.
pub fn link_format(
    properties: &TextProperties,
    inherited: f32,
    zoom: f32,
    palette: Palette,
) -> TextFormat {
    let mut format = text_format(properties, inherited, zoom, palette);
    if properties.color.is_none() {
        format.color = palette.link;
        if format.underline == Stroke::NONE {
            format.underline = Stroke::new(1.0, format.color);
        }
    }
    format
}

/// The size in points a run is drawn at, for handing to whatever it contains.
pub fn size_of(properties: &TextProperties, inherited: f32) -> f32 {
    properties
        .size
        .map_or(inherited, |measure| measure.resolve(inherited))
}

/// An ODF colour as egui's.
pub fn color32(color: Color) -> Color32 {
    Color32::from_rgb(color.r, color.g, color.b)
}

/// A length or proportion resolved to points, for a line height.
pub fn line_height(measure: Option<Measure>, size: f32) -> Option<f32> {
    measure.map(|m| m.resolve(size))
}
