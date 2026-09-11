//! The window's one view: a sheet drawn as a grid, and the cell a person picked.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use eframe::egui::{self, Align, FontId, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use odox_core::doc::{Sheet, SheetDocument};
use odox_ui::format::{self, DEFAULT_SIZE};
use odox_ui::i18n::{fill, t};
use odox_ui::{Viewer, fonts};

use crate::grid::{Metrics, address, column_name};

/// A spreadsheet, open or not.
#[derive(Default)]
pub struct SheetView {
    document: Option<SheetDocument>,
    sheet: usize,
    selected: (usize, usize),
    metrics: Option<Metrics>,
    /// What the metrics were measured for, so that they are measured again when
    /// the sheet or the zoom changes and not on every frame.
    measured: (usize, f32),
}

impl Viewer for SheetView {
    fn open(&mut self, ctx: &egui::Context, bytes: &[u8], path: &Path) -> Result<(), String> {
        let document = SheetDocument::read(bytes).map_err(|e| {
            fill(
                t("{file} could not be opened: {reason}"),
                &[
                    (
                        "file",
                        &path.file_name().unwrap_or_default().to_string_lossy(),
                    ),
                    ("reason", &e.to_string()),
                ],
            )
        })?;
        let mut parts = vec![&document.document.content];
        if let Some(styles) = &document.document.styles_part {
            parts.push(styles);
        }
        ctx.set_fonts(fonts::definitions(&fonts::families_used(&parts)));

        // The first sheet a person can see, which is not always the first sheet:
        // a workbook may open on a hidden one.
        self.sheet = document
            .sheets()
            .iter()
            .position(|s| s.visible)
            .unwrap_or(0);
        self.document = Some(document);
        self.selected = (0, 0);
        self.metrics = None;
        Ok(())
    }

    fn close(&mut self) {
        self.document = None;
        self.metrics = None;
    }

    fn is_open(&self) -> bool {
        self.document.is_some()
    }

    fn title(&self) -> Option<String> {
        self.document.as_ref()?.document.meta.title.clone()
    }

    fn central(&mut self, ui: &mut Ui, zoom: f32) {
        if self.document.is_none() {
            return;
        }
        egui::Panel::top("cell").show(ui, |ui| self.cell_bar(ui));
        egui::Panel::bottom("sheets").show(ui, |ui| self.sheet_tabs(ui));
        self.grid(ui, zoom);
    }

    fn side(&mut self, _ui: &mut Ui) -> bool {
        // A workbook's sheets are its tabs along the bottom, where a person
        // reaches for them, so there is nothing to put beside the grid.
        false
    }

    fn view_menu(&mut self, ui: &mut Ui) {
        let Some(document) = &self.document else {
            return;
        };
        let Some(sheet) = document.sheets().get(self.sheet) else {
            return;
        };
        ui.separator();
        if ui
            .button(t("Copy the sheet as tab-separated text"))
            .clicked()
        {
            ui.ctx().copy_text(as_text(document, sheet));
            ui.close();
        }
    }
}

impl SheetView {
    /// The bar above the grid: which cell is picked, and what is in it.
    fn cell_bar(&mut self, ui: &mut Ui) {
        let Some(document) = &self.document else {
            return;
        };
        let Some(sheet) = document.sheets().get(self.sheet) else {
            return;
        };
        let (row, column) = self.selected;
        let cell = document.cell(sheet, row, column);

        ui.horizontal(|ui| {
            ui.monospace(address(row, column));
            ui.separator();
            let text = cell
                .as_ref()
                .map(odox_core::doc::Cell::text)
                .unwrap_or_default();
            // A formula is what the cell says; the text is what it shows, and
            // both are worth seeing at once. A cell with no formula shows only
            // the text, which is all it has.
            if let Some(formula) = cell.as_ref().and_then(odox_core::doc::Cell::formula) {
                ui.monospace(format!("={formula}"));
                ui.weak(text);
            } else {
                ui.label(text);
            }
        });
    }

    /// The tabs along the bottom, one per sheet a person can see.
    fn sheet_tabs(&mut self, ui: &mut Ui) {
        let Some(document) = &self.document else {
            return;
        };
        let names: Vec<(usize, String)> = document
            .sheets()
            .iter()
            .enumerate()
            .filter(|(_, sheet)| sheet.visible)
            .map(|(index, sheet)| (index, sheet.name.clone()))
            .collect();
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                for (index, name) in names {
                    let picked = index == self.sheet;
                    if ui.selectable_label(picked, name).clicked() {
                        self.sheet = index;
                        self.selected = (0, 0);
                        self.metrics = None;
                    }
                }
            });
        });
    }

    /// The grid, painting the cells the window covers and no others.
    #[allow(clippy::too_many_lines)]
    fn grid(&mut self, ui: &mut Ui, zoom: f32) {
        let Some(document) = &self.document else {
            return;
        };
        let Some(sheet) = document.sheets().get(self.sheet) else {
            return;
        };

        if self.metrics.is_none() || self.measured != (self.sheet, zoom) {
            self.metrics = Some(Metrics::of(document, sheet, zoom));
            self.measured = (self.sheet, zoom);
        }
        let Some(metrics) = &self.metrics else { return };

        let header_height = 18.0 * zoom;
        let header_width = 46.0 * zoom;
        // The grid is a page and carries the document's own colours; the headers
        // around it are chrome and follow the desktop. `format::Palette` says why.
        let palette = format::Palette::default();
        let faint = palette.ink.gamma_multiply(0.18);
        let header_fill = ui.visuals().faint_bg_color;
        let header_text = ui.visuals().text_color();
        let mut clicked = None;

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let (width, height) = metrics.size();
                let (area, response) = ui.allocate_exact_size(
                    vec2(width + header_width, height + header_height),
                    Sense::click(),
                );
                let origin = area.min + vec2(header_width, header_height);
                let painter = ui.painter();
                painter.rect_filled(Rect::from_min_max(origin, area.max), 0.0, palette.paper);

                let rows = metrics.rows_between(
                    viewport.min.y - header_height,
                    viewport.max.y - header_height,
                );
                let columns = metrics
                    .columns_between(viewport.min.x - header_width, viewport.max.x - header_width);

                // The lines first, so that a cell with a fill covers them the way a
                // spreadsheet's do.
                for row in rows.clone() {
                    let (top, height) = metrics.row(row);
                    painter.hline(
                        (origin.x + metrics.column(columns.start).0)
                            ..=(origin.x
                                + metrics.column(columns.end.saturating_sub(1)).0
                                + metrics.column(columns.end.saturating_sub(1)).1),
                        origin.y + top + height,
                        Stroke::new(1.0, faint),
                    );
                }
                for column in columns.clone() {
                    let (left, width) = metrics.column(column);
                    painter.vline(
                        origin.x + left + width,
                        (origin.y + metrics.row(rows.start).0)
                            ..=(origin.y
                                + metrics.row(rows.end.saturating_sub(1)).0
                                + metrics.row(rows.end.saturating_sub(1)).1),
                        Stroke::new(1.0, faint),
                    );
                }

                for row in rows.clone() {
                    let (top, row_height) = metrics.row(row);
                    for column in columns.clone() {
                        let (left, column_width) = metrics.column(column);
                        let cell = document.cell(sheet, row, column);
                        let style = document.cell_style(sheet, cell.as_ref(), column);

                        // A merged cell is one cell over several columns and rows.
                        // Its fill, its borders and its text all belong to the whole
                        // rectangle rather than to the first column of it, so the
                        // extent is worked out before anything is painted.
                        let across = cell
                            .as_ref()
                            .map_or(1, odox_core::doc::Cell::columns_spanned);
                        let down = cell.as_ref().map_or(1, odox_core::doc::Cell::rows_spanned);
                        let width: f32 = (0..across).map(|n| metrics.column(column + n).1).sum();
                        let height: f32 = (0..down).map(|n| metrics.row(row + n).1).sum();
                        let rect = Rect::from_min_size(
                            pos2(origin.x + left, origin.y + top),
                            vec2(width.max(column_width), height.max(row_height)),
                        );

                        if let Some(fill) = style.cell.background {
                            painter.rect_filled(rect, 0.0, format::color32(fill));
                        }
                        for (from, to, border) in [
                            (rect.left_top(), rect.left_bottom(), style.cell.border.left),
                            (
                                rect.right_top(),
                                rect.right_bottom(),
                                style.cell.border.right,
                            ),
                            (rect.left_top(), rect.right_top(), style.cell.border.top),
                            (
                                rect.left_bottom(),
                                rect.right_bottom(),
                                style.cell.border.bottom,
                            ),
                        ] {
                            if let Some(border) = border {
                                painter.line_segment(
                                    [from, to],
                                    Stroke::new(
                                        (border.width.points() * zoom).max(1.0),
                                        format::color32(border.color),
                                    ),
                                );
                            }
                        }

                        let Some(cell) = cell else { continue };
                        if cell.covered {
                            continue;
                        }
                        let text = cell.text();
                        if text.is_empty() {
                            continue;
                        }
                        let drawn = rect;

                        let format = format::text_format(&style.text, DEFAULT_SIZE, zoom, palette);
                        // Text against the left edge and numbers against the right,
                        // which is ODF's own default and every spreadsheet's, unless
                        // the cell's paragraph style says otherwise.
                        let align = match style.paragraph.align {
                            Some(odox_core::TextAlign::Center) => Align::Center,
                            Some(odox_core::TextAlign::End) => Align::Max,
                            // Text against the left edge and every other type
                            // against the right, which is ODF's own rule.
                            None if cell.value().is_numeric() => Align::Max,
                            _ => Align::Min,
                        };
                        let padding = 3.0 * zoom;
                        let anchor = match align {
                            Align::Min => pos2(drawn.left() + padding, drawn.top()),
                            Align::Center => pos2(drawn.center().x, drawn.top()),
                            Align::Max => pos2(drawn.right() - padding, drawn.top()),
                        };
                        painter
                            .with_clip_rect(drawn.intersect(ui.clip_rect()))
                            .text(
                                anchor,
                                match align {
                                    Align::Min => egui::Align2::LEFT_TOP,
                                    Align::Center => egui::Align2::CENTER_TOP,
                                    Align::Max => egui::Align2::RIGHT_TOP,
                                },
                                text,
                                format.font_id.clone(),
                                format.color,
                            );
                    }
                }

                // The picked cell, over everything in it.
                let (row, column) = (self.selected.0, self.selected.1);
                if rows.contains(&row) && columns.contains(&column) {
                    let (top, row_height) = metrics.row(row);
                    let (left, column_width) = metrics.column(column);
                    painter.rect_stroke(
                        Rect::from_min_size(
                            pos2(origin.x + left, origin.y + top),
                            vec2(column_width, row_height),
                        ),
                        0.0,
                        Stroke::new(2.0, ui.visuals().selection.stroke.color),
                        StrokeKind::Inside,
                    );
                }

                // The headers last and at the edge of what is visible, so that they
                // stay where a person is looking while the grid scrolls under them.
                let header_font = FontId::proportional(11.0 * zoom);
                let top = area.min.y + viewport.min.y;
                let left = area.min.x + viewport.min.x;
                for column in columns.clone() {
                    let (x, width) = metrics.column(column);
                    let rect =
                        Rect::from_min_size(pos2(origin.x + x, top), vec2(width, header_height));
                    painter.rect_filled(rect, 0.0, header_fill);
                    painter.rect_stroke(rect, 0.0, Stroke::new(1.0, faint), StrokeKind::Inside);
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        column_name(column),
                        header_font.clone(),
                        header_text,
                    );
                }
                for row in rows.clone() {
                    let (y, height) = metrics.row(row);
                    let rect =
                        Rect::from_min_size(pos2(left, origin.y + y), vec2(header_width, height));
                    painter.rect_filled(rect, 0.0, header_fill);
                    painter.rect_stroke(rect, 0.0, Stroke::new(1.0, faint), StrokeKind::Inside);
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        (row + 1).to_string(),
                        header_font.clone(),
                        header_text,
                    );
                }
                // The square where the two headers meet, which would otherwise show
                // the grid scrolling behind them.
                painter.rect_filled(
                    Rect::from_min_size(pos2(left, top), vec2(header_width, header_height)),
                    0.0,
                    header_fill,
                );

                if response.clicked()
                    && let Some(at) = response.interact_pointer_pos()
                    && at.x > origin.x
                    && at.y > origin.y
                {
                    clicked = Some((
                        metrics.row_at(at.y - origin.y),
                        metrics.column_at(at.x - origin.x),
                    ));
                }
            });

        let extent = (sheet.used_rows, sheet.used_columns);
        if let Some(cell) = clicked {
            self.selected = cell;
        }
        self.arrow_keys(ui, extent);
    }

    /// Move the picked cell with the arrow keys, which is how a person walks a
    /// sheet without reaching for the pointer.
    fn arrow_keys(&mut self, ui: &Ui, (rows, columns): (usize, usize)) {
        let (mut row, mut column) = self.selected;
        let last_row = rows.saturating_sub(1);
        let last_column = columns.saturating_sub(1);
        ui.input(|input| {
            if input.key_pressed(egui::Key::ArrowDown) {
                row = row.saturating_add(1);
            }
            if input.key_pressed(egui::Key::ArrowUp) {
                row = row.saturating_sub(1);
            }
            if input.key_pressed(egui::Key::ArrowRight) {
                column = column.saturating_add(1);
            }
            if input.key_pressed(egui::Key::ArrowLeft) {
                column = column.saturating_sub(1);
            }
            if input.key_pressed(egui::Key::Home) {
                column = 0;
            }
            if input.key_pressed(egui::Key::End) {
                column = last_column;
            }
        });
        self.selected = (row.min(last_row), column.min(last_column));
    }
}

/// A sheet as text, for handing to something else.
///
/// Tab separated and taking each cell as the document displays it, because that
/// is the string the producing application formatted and the one a person sees.
fn as_text(document: &SheetDocument, sheet: &Sheet) -> String {
    let mut out = String::new();
    for row in 0..sheet.used_rows {
        for column in 0..sheet.used_columns {
            if column > 0 {
                out.push('\t');
            }
            if let Some(cell) = document.cell(sheet, row, column) {
                // A tab or a newline inside a cell would make the row unreadable
                // as a row, so each becomes a space.
                out.push_str(&cell.text().replace(['\t', '\n'], " "));
            }
        }
        out.push('\n');
    }
    out
}
