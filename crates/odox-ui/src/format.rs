//! Turning resolved ODF properties into what egui draws text with.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{Align, Color32, FontFamily, FontId, Stroke, TextFormat};
use odox_core::{Color, Position, TextProperties};

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
/// The document's own colours are always the document's: a colour the author
/// set is drawn as set, whatever the window's theme. Where a document sets
/// none, [`Palette::for_theme`] follows the window instead of staying fixed —
/// paper and ink move together, the way a page and its own unset text would
/// if it were reprinted for the window it is read in, so a document that
/// never colours itself is legible in both. The chrome around the page —
/// menus, panels, the grid's headers — already follows the desktop by the
/// same principle, one level up.
///
/// This does not reach a document that colours its own text but leaves the
/// page unset: that colour is drawn as set, on paper that now follows the
/// window, and an author's own dark heading can still land on dark paper.
/// That is the narrower case the old, page-only version of this rule was
/// written against; recolouring an author's own choice for contrast is a
/// larger step than this file takes.
///
/// `link` is the one colour that does not follow this rule: [`link_format`]
/// draws every hyperlink in it regardless of what the document says, because
/// almost every ODF producer writes a resolved colour into a hyperlink's
/// character style whether an author touched it or not — "the document set
/// a colour" is barely ever true of a link's blue in the way it is of a
/// heading's, so treating it as an author's choice would mean this file's
/// dark mode never reaching almost any real hyperlink.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// What the page itself is.
    pub paper: Color32,
    /// The colour of text the document does not colour.
    pub ink: Color32,
    /// The colour every hyperlink is drawn in, whatever the document says.
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

impl Palette {
    /// The palette for a window in, or not in, dark mode.
    ///
    /// All three move together, because they are the unset document's own
    /// page and the unset document's own text, and a page redrawn dark with
    /// text left black would be unreadable rather than themed. The dark ink
    /// is an off-white rather than pure white, and the dark link a lighter,
    /// more saturated blue than the light palette's — both chosen to meet
    /// WCAG AA contrast against the dark paper (measured: ink 11.2:1, link
    /// 5.4:1; AA needs 4.5:1) rather than merely looking legible in one shot.
    pub fn for_theme(dark_mode: bool) -> Self {
        if dark_mode {
            Self {
                paper: Color32::from_rgb(0x1e, 0x1e, 0x1e),
                ink: Color32::from_rgb(0xd4, 0xd4, 0xd4),
                link: Color32::from_rgb(0x37, 0x94, 0xff),
            }
        } else {
            Self::default()
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
///
/// Always `palette.link`, even where `properties` carries its own colour:
/// [`Palette`]'s own doc says why a hyperlink's colour does not get the
/// respect this file gives every other one.
pub fn link_format(
    properties: &TextProperties,
    inherited: f32,
    zoom: f32,
    palette: Palette,
) -> TextFormat {
    let mut format = text_format(properties, inherited, zoom, palette);
    format.color = palette.link;
    // The underline keeps the width the document asked for, if any, but its
    // colour follows the text it underlines rather than whatever colour that
    // text used to be.
    format.underline = Stroke::new(format.underline.width.max(1.0), format.color);
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
