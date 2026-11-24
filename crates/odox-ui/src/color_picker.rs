//! The font colour and highlight buttons: each applies the colour it last
//! applied, and its arrow opens `LibreOffice`'s standard palette with the
//! automatic colour, or no highlight, above it.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{self, Color32, Id, Sense, Stroke, Ui, vec2};

use crate::i18n::t;

/// Which button: the ink, whose `None` is the automatic colour, or the
/// highlight, whose `None` is none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// The font colour.
    Ink,
    /// The highlight.
    Highlight,
}

/// Rows of `LibreOffice`'s standard palette, twelve to a row: the greys, the
/// lightest tints, the light tints, the full colours and the second shades,
/// which hold the default font colour.
const PALETTE: [[u32; 12]; 5] = [
    [
        0x00_0000, 0x11_1111, 0x1c_1c1c, 0x33_3333, 0x66_6666, 0x80_8080, 0x99_9999, 0xb2_b2b2,
        0xcc_cccc, 0xdd_dddd, 0xee_eeee, 0xff_ffff,
    ],
    [
        0xff_ffd7, 0xff_f5ce, 0xff_dbb6, 0xff_d8ce, 0xff_d7d7, 0xf7_d1d5, 0xe0_c2cd, 0xde_dce6,
        0xde_e6ef, 0xde_e7e5, 0xdd_e8cb, 0xf6_f9d4,
    ],
    [
        0xff_ffa6, 0xff_e994, 0xff_b66c, 0xff_aa95, 0xff_a6a6, 0xec_9ba4, 0xbf_819e, 0xb7_b3ca,
        0xb4_c7dc, 0xb3_cac7, 0xaf_d095, 0xe8_f2a1,
    ],
    [
        0xff_ff00, 0xff_bf00, 0xff_8000, 0xff_4000, 0xff_0000, 0xbf_0041, 0x80_0080, 0x55_308d,
        0x2a_6099, 0x15_8466, 0x00_a933, 0x81_d41a,
    ],
    [
        0xac_b20c, 0xb4_7804, 0xb8_5c00, 0xbe_480a, 0xc9_211e, 0x86_1141, 0x65_0953, 0x55_215b,
        0x35_5269, 0x1e_6a39, 0x12_7622, 0x46_8a1a,
    ],
];

fn rgb(packed: u32) -> [u8; 3] {
    let [_, r, g, b] = packed.to_be_bytes();
    [r, g, b]
}

fn color32([r, g, b]: [u8; 3]) -> Color32 {
    Color32::from_rgb(r, g, b)
}

impl Kind {
    fn first(self) -> [u8; 3] {
        match self {
            Self::Ink => rgb(0xc9_211e),
            Self::Highlight => rgb(0xff_ff00),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Ink => "A",
            Self::Highlight => "ab",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Ink => t("Font color"),
            Self::Highlight => t("Highlighting"),
        }
    }

    fn none(self) -> &'static str {
        match self {
            Self::Ink => t("Automatic"),
            Self::Highlight => t("None"),
        }
    }
}

fn last_id(kind: Kind) -> Id {
    Id::unique(("odox-last-color", kind))
}

/// The split button. Answers the colour picked, `Some(None)` for the
/// automatic ink or no highlight.
pub fn button(ui: &mut Ui, kind: Kind) -> Option<Option<[u8; 3]>> {
    let last = ui
        .ctx()
        .data(|data| data.get_temp(last_id(kind)))
        .unwrap_or(Some(kind.first()));
    let mut picked = None;
    let response = ui.button(kind.label()).on_hover_text(kind.name());
    let bar = egui::Rect::from_min_size(
        response.rect.left_bottom() + vec2(4.0, -5.0),
        vec2(response.rect.width() - 8.0, 3.0),
    );
    match last {
        Some(color) => {
            ui.painter().rect_filled(bar, 0.0, color32(color));
        }
        None => {
            ui.painter().rect_stroke(
                bar,
                0.0,
                Stroke::new(1.0, ui.visuals().weak_text_color()),
                egui::StrokeKind::Inside,
            );
        }
    }
    if response.clicked() {
        picked = Some(last);
    }
    ui.menu_button("\u{25be}", |ui| {
        if ui.button(kind.none()).clicked() {
            picked = Some(None);
            ui.close();
        }
        egui::Grid::new(("odox-palette", kind))
            .spacing(vec2(2.0, 2.0))
            .show(ui, |ui| {
                for row in PALETTE {
                    for packed in row {
                        let color = rgb(packed);
                        let (rect, swatch) =
                            ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
                        ui.painter().rect_filled(rect, 2.0, color32(color));
                        if swatch.hovered() {
                            ui.painter().rect_stroke(
                                rect,
                                2.0,
                                Stroke::new(1.5, ui.visuals().strong_text_color()),
                                egui::StrokeKind::Outside,
                            );
                        }
                        if swatch.clicked() {
                            picked = Some(Some(color));
                            ui.close();
                        }
                    }
                    ui.end_row();
                }
            });
    })
    .response
    .on_hover_text(kind.name());
    if let Some(color) = picked {
        ui.ctx()
            .data_mut(|data| data.insert_temp(last_id(kind), color));
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::{Kind, PALETTE, rgb};

    #[test]
    fn each_first_colour_can_be_picked_again() {
        for kind in [Kind::Ink, Kind::Highlight] {
            assert!(
                PALETTE
                    .iter()
                    .flatten()
                    .any(|&packed| rgb(packed) == kind.first()),
                "{kind:?}"
            );
        }
    }
}
