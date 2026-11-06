// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{self, Key};
use odox_core::data_style::Kind;
use odox_core::doc::{Block, Borders, CellFormat, NumberFormat, SheetDocument};
use odox_core::{Color, TextAlign, VerticalAlign};
use odox_ui::i18n::t;

pub struct Formatting {
    pub block: Block,
    was: Choices,
    now: Choices,
}

#[derive(Clone, Copy, PartialEq)]
struct Choices {
    number: Figure,
    decimals: u8,
    grouping: bool,
    font: Font,
    color: Option<[u8; 3]>,
    fill: Option<[u8; 3]>,
    across: Option<TextAlign>,
    down: VerticalAlign,
    wrap: bool,
    borders: Option<Borders>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Font {
    bold: bool,
    italic: bool,
    underline: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Figure {
    General,
    Number,
    Percent,
    Currency,
    Date,
    Time,
}

impl Formatting {
    pub fn of(
        document: &SheetDocument,
        sheet: usize,
        block: Block,
        picked: (usize, usize),
    ) -> Option<Self> {
        let index = document.sheets().get(sheet)?;
        let (row, column) = picked;
        let cell = document.cell(index, row, column);
        let style = document.cell_style(index, cell.as_ref(), column);
        let shown = document.data_style(sheet, row, column);
        let (decimals, grouping) = shown
            .and_then(odox_core::data_style::DataStyle::digits)
            .map_or((2, true), |(decimals, grouping)| {
                (u8::try_from(decimals.min(10)).unwrap_or(2), grouping)
            });
        let number = match shown.map(|d| d.kind) {
            Some(Kind::Number) => Figure::Number,
            Some(Kind::Percentage) => Figure::Percent,
            Some(Kind::Currency) => Figure::Currency,
            Some(Kind::Date) => Figure::Date,
            Some(Kind::Time) => Figure::Time,
            _ => Figure::General,
        };
        let rgb = |c: Color| [c.r, c.g, c.b];
        let was = Choices {
            number,
            decimals,
            grouping,
            font: Font {
                bold: style.text.bold == Some(true),
                italic: style.text.italic == Some(true),
                underline: style.text.underline == Some(true),
            },
            color: style.text.color.map(rgb),
            fill: style.cell.background.map(rgb),
            across: style.paragraph.align,
            down: style.cell.vertical_align.unwrap_or(VerticalAlign::Bottom),
            wrap: style.cell.wrap == Some(true),
            borders: None,
        };
        Some(Self {
            block,
            was,
            now: was,
        })
    }

    pub fn changes(&self) -> CellFormat {
        let (was, now) = (&self.was, &self.now);
        let color = |c: Option<[u8; 3]>| c.map(|[r, g, b]| Color { r, g, b });
        let numbers =
            (was.number, was.decimals, was.grouping) != (now.number, now.decimals, now.grouping);
        CellFormat {
            bold: (was.font.bold != now.font.bold).then_some(now.font.bold),
            italic: (was.font.italic != now.font.italic).then_some(now.font.italic),
            underline: (was.font.underline != now.font.underline).then_some(now.font.underline),
            color: (was.color != now.color).then_some(color(now.color)),
            background: (was.fill != now.fill).then_some(color(now.fill)),
            align: (was.across != now.across).then_some(now.across),
            vertical: (was.down != now.down).then_some(now.down),
            wrap: (was.wrap != now.wrap).then_some(now.wrap),
            borders: now.borders,
            number: numbers.then_some(match now.number {
                Figure::General => NumberFormat::General,
                Figure::Number => NumberFormat::Number {
                    decimals: now.decimals,
                    grouping: now.grouping,
                },
                Figure::Percent => NumberFormat::Percent {
                    decimals: now.decimals,
                },
                Figure::Currency => NumberFormat::Currency {
                    decimals: now.decimals,
                },
                Figure::Date => NumberFormat::Date,
                Figure::Time => NumberFormat::Time,
            }),
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<bool> {
        let mut answer = None;
        let now = &mut self.now;
        egui::Modal::new(egui::Id::unique("format-cells")).show(ctx, |ui| {
            ui.heading(t("Format cells"));
            ui.add_space(6.0);
            egui::Grid::new(egui::Id::unique("format-grid"))
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    number_rows(ui, now);
                    font_rows(ui, now);
                    alignment_rows(ui, now);
                    border_row(ui, now);
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(t("Apply")).clicked()
                    || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter))
                {
                    answer = Some(true);
                }
                if ui.button(t("Cancel")).clicked()
                    || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape))
                {
                    answer = Some(false);
                }
            });
        });
        answer
    }
}

fn number_rows(ui: &mut egui::Ui, now: &mut Choices) {
    let name = |number: Figure| match number {
        Figure::General => t("General"),
        Figure::Number => t("Number"),
        Figure::Percent => t("Percent"),
        Figure::Currency => t("Currency"),
        Figure::Date => t("Date"),
        Figure::Time => t("Time"),
    };
    ui.label(t("Number format"));
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("format-number")
            .selected_text(name(now.number))
            .show_ui(ui, |ui| {
                for number in [
                    Figure::General,
                    Figure::Number,
                    Figure::Percent,
                    Figure::Currency,
                    Figure::Date,
                    Figure::Time,
                ] {
                    ui.selectable_value(&mut now.number, number, name(number));
                }
            });
        if matches!(
            now.number,
            Figure::Number | Figure::Percent | Figure::Currency
        ) {
            ui.label(t("Decimal places"));
            ui.add(egui::DragValue::new(&mut now.decimals).range(0..=10));
        }
        if now.number == Figure::Number {
            ui.checkbox(&mut now.grouping, t("Thousands separator"));
        }
    });
    ui.end_row();
}

fn font_rows(ui: &mut egui::Ui, now: &mut Choices) {
    ui.label(t("Font"));
    ui.horizontal(|ui| {
        ui.checkbox(&mut now.font.bold, t("Bold"));
        ui.checkbox(&mut now.font.italic, t("Italic"));
        ui.checkbox(&mut now.font.underline, t("Underline"));
    });
    ui.end_row();
    ui.label(t("Font color"));
    colour(ui, &mut now.color, t("Automatic"), [0xc9, 0x21, 0x1e]);
    ui.end_row();
    ui.label(t("Background"));
    colour(ui, &mut now.fill, t("None"), [0xff, 0xff, 0x99]);
    ui.end_row();
}

fn colour(ui: &mut egui::Ui, chosen: &mut Option<[u8; 3]>, without: &str, first: [u8; 3]) {
    ui.horizontal(|ui| {
        let mut none = chosen.is_none();
        if ui.checkbox(&mut none, without).changed() {
            *chosen = if none { None } else { Some(first) };
        }
        if let Some(rgb) = chosen {
            ui.color_edit_button_srgb(rgb);
        }
    });
}

fn alignment_rows(ui: &mut egui::Ui, now: &mut Choices) {
    let across = |align: Option<TextAlign>| match align {
        None => t("Automatic"),
        Some(TextAlign::Start) => t("Left"),
        Some(TextAlign::Center) => t("Center"),
        Some(TextAlign::End) => t("Right"),
        Some(TextAlign::Justify) => t("Justified"),
    };
    let down = |align: VerticalAlign| match align {
        VerticalAlign::Top => t("Top"),
        VerticalAlign::Middle => t("Middle"),
        VerticalAlign::Bottom => t("Bottom"),
    };
    ui.label(t("Alignment"));
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("format-across")
            .selected_text(across(now.across))
            .show_ui(ui, |ui| {
                for align in [
                    None,
                    Some(TextAlign::Start),
                    Some(TextAlign::Center),
                    Some(TextAlign::End),
                    Some(TextAlign::Justify),
                ] {
                    ui.selectable_value(&mut now.across, align, across(align));
                }
            });
        egui::ComboBox::from_id_salt("format-down")
            .selected_text(down(now.down))
            .show_ui(ui, |ui| {
                for align in [
                    VerticalAlign::Top,
                    VerticalAlign::Middle,
                    VerticalAlign::Bottom,
                ] {
                    ui.selectable_value(&mut now.down, align, down(align));
                }
            });
        ui.checkbox(&mut now.wrap, t("Wrap text"));
    });
    ui.end_row();
}

fn border_row(ui: &mut egui::Ui, now: &mut Choices) {
    let name = |borders: Option<Borders>| match borders {
        None => t("As they are"),
        Some(Borders::None) => t("None"),
        Some(Borders::Outline) => t("Around the selection"),
        Some(Borders::All) => t("Around every cell"),
    };
    ui.label(t("Borders"));
    egui::ComboBox::from_id_salt("format-borders")
        .selected_text(name(now.borders))
        .show_ui(ui, |ui| {
            for borders in [
                None,
                Some(Borders::None),
                Some(Borders::Outline),
                Some(Borders::All),
            ] {
                ui.selectable_value(&mut now.borders, borders, name(borders));
            }
        });
    ui.end_row();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_what_is_changed_is_written() {
        let document =
            SheetDocument::read(&odox_core::blank::spreadsheet("en-US", "Sheet1")).expect("blank");
        let block = Block {
            top: 0,
            left: 0,
            bottom: 1,
            right: 1,
        };
        let mut formatting = Formatting::of(&document, 0, block, (0, 0)).expect("a sheet");
        assert_eq!(formatting.changes(), CellFormat::default());
        formatting.now.font.bold = true;
        formatting.now.number = Figure::Percent;
        formatting.now.decimals = 1;
        assert_eq!(
            formatting.changes(),
            CellFormat {
                bold: Some(true),
                number: Some(NumberFormat::Percent { decimals: 1 }),
                ..CellFormat::default()
            }
        );
    }
}
