//! Format > Page setup…: the paper a text document is printed on, which way
//! round, and its margins. The window has no pages, so this is the width the
//! page is drawn at and the pages an export makes. DESIGN.md §11.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui;
use odox_core::Length;
use odox_core::doc::PageSetup;

use crate::i18n::t;

/// A paper offered by name, upright, in points.
struct Paper {
    name: &'static str,
    width: f32,
    height: f32,
    /// Whether the countries that use it measure in inches.
    inches: bool,
}

const PAPERS: [Paper; 4] = [
    Paper {
        name: "A4",
        width: 595.28,
        height: 841.89,
        inches: false,
    },
    Paper {
        name: "A5",
        width: 419.53,
        height: 595.28,
        inches: false,
    },
    Paper {
        name: "Letter",
        width: 612.0,
        height: 792.0,
        inches: true,
    },
    Paper {
        name: "Legal",
        width: 612.0,
        height: 1008.0,
        inches: true,
    },
];

/// The paper a page is, either way round.
fn paper_of(setup: &PageSetup) -> Option<&'static Paper> {
    let (short, long) = {
        let (a, b) = (setup.width.points(), setup.height.points());
        (a.min(b), a.max(b))
    };
    PAPERS
        .iter()
        .find(|paper| (paper.width - short).abs() < 1.0 && (paper.height - long).abs() < 1.0)
}

/// The question, while it is up.
pub struct PageSetupQuestion {
    setup: PageSetup,
    /// Whether lengths are shown in inches rather than centimetres, which
    /// follows the paper the page was on when the question was asked.
    inches: bool,
}

impl PageSetupQuestion {
    /// Ask about a page as it is now.
    pub fn new(setup: PageSetup) -> Self {
        let inches = paper_of(&setup).is_some_and(|paper| paper.inches);
        Self { setup, inches }
    }

    /// The page as the question now has it.
    pub fn setup(&self) -> PageSetup {
        self.setup
    }

    /// Draw the question. Answers once the person has: whether to change the
    /// page.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<bool> {
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("page-setup")).show(ctx, |ui| {
            ui.heading(t("Page setup"));
            ui.add_space(6.0);
            egui::Grid::new(egui::Id::unique("page-setup-grid"))
                .num_columns(2)
                .show(ui, |ui| {
                    self.paper(ui);
                    self.orientation(ui);
                    let unit = if self.inches { "in" } else { "cm" };
                    let [top, right, bottom, left] = &mut self.setup.margins;
                    for (name, length) in [
                        (t("Width"), &mut self.setup.width),
                        (t("Height"), &mut self.setup.height),
                        (t("Top margin"), top),
                        (t("Bottom margin"), bottom),
                        (t("Left margin"), left),
                        (t("Right margin"), right),
                    ] {
                        ui.label(name);
                        length_field(ui, length, self.inches, unit);
                        ui.end_row();
                    }
                });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(t("OK")).clicked() {
                    answer = Some(true);
                }
                if ui.button(t("Cancel")).clicked()
                    || ui.input(|input| input.key_pressed(egui::Key::Escape))
                {
                    answer = Some(false);
                }
            });
        });
        answer
    }

    /// The papers offered, the page's own lit, or Custom where it is none of
    /// them. A paper picked keeps the page's orientation.
    fn paper(&mut self, ui: &mut egui::Ui) {
        ui.label(t("Paper"));
        let current = paper_of(&self.setup);
        let landscape = self.setup.landscape();
        egui::ComboBox::from_id_salt("paper")
            .selected_text(current.map_or_else(|| t("Custom"), |paper| paper.name))
            .show_ui(ui, |ui| {
                for paper in &PAPERS {
                    let lit = current.is_some_and(|c| c.name == paper.name);
                    if ui.selectable_label(lit, paper.name).clicked() {
                        let (width, height) = if landscape {
                            (paper.height, paper.width)
                        } else {
                            (paper.width, paper.height)
                        };
                        self.setup.width = Length(width);
                        self.setup.height = Length(height);
                    }
                }
            });
        ui.end_row();
    }

    /// Portrait or landscape, which swaps the page's width and height.
    fn orientation(&mut self, ui: &mut egui::Ui) {
        ui.label(t("Orientation"));
        ui.horizontal(|ui| {
            let landscape = self.setup.landscape();
            let portrait = ui.radio(!landscape, t("Portrait")).clicked();
            let turned = ui.radio(landscape, t("Landscape")).clicked();
            if (portrait && landscape) || (turned && !landscape) {
                std::mem::swap(&mut self.setup.width, &mut self.setup.height);
            }
        });
        ui.end_row();
    }
}

/// A length shown and changed in inches or centimetres.
fn length_field(ui: &mut egui::Ui, length: &mut Length, inches: bool, unit: &str) {
    let per_unit = if inches { 72.0 } else { 72.0 / 2.54 };
    let mut shown = length.points() / per_unit;
    let changed = ui
        .add(
            egui::DragValue::new(&mut shown)
                .speed(0.01)
                .range(0.0..=200.0)
                .max_decimals(2)
                .suffix(format!(" {unit}")),
        )
        .changed();
    if changed {
        *length = Length(shown * per_unit);
    }
}
