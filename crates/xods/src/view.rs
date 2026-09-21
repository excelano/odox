//! The window's one view: a sheet drawn as a grid, and the cell a person picked.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use eframe::egui::{
    self, Align, Event, FontId, Key, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use odox_core::doc::{Sheet, SheetDocument, Value};
use odox_core::{Document, Refused};
use odox_ui::format::{self, DEFAULT_SIZE};
use odox_ui::i18n::{fill, t};
use odox_ui::{Editing, View, fonts};

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
    /// The cell being typed into, while one is.
    editor: Option<CellEditor>,
    /// Why the picked cell could not be edited, shown until the pick moves.
    notice: Option<String>,
}

/// A cell being typed into.
///
/// A spreadsheet edits without a mode, by every spreadsheet's convention:
/// typing on the picked cell replaces it, Enter or F2 opens it with what it
/// holds, Enter commits and moves down, Tab commits and moves right, Escape
/// puts it back. The text box sits in the cell, in the cell's own font.
struct CellEditor {
    row: usize,
    column: usize,
    text: String,
    /// What the cell held when the editor opened, so that leaving it as it was
    /// is not an edit and does not retype a currency as a number.
    original: String,
    /// The editor was opened this frame and has yet to take the focus.
    opened: bool,
}

/// What the editor asked for when it closed.
enum Outcome {
    Commit(Step),
    Cancel,
}

/// Where the pick goes after a commit.
enum Step {
    Stay,
    Down,
    Right,
}

impl View for SheetView {
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
        self.editor = None;
        self.notice = None;
        Ok(())
    }

    fn close(&mut self) {
        self.document = None;
        self.metrics = None;
        self.editor = None;
        self.notice = None;
    }

    fn is_open(&self) -> bool {
        self.document.is_some()
    }

    fn title(&self) -> Option<String> {
        self.document.as_ref()?.document.meta.title.clone()
    }

    fn document_mut(&mut self) -> Option<&mut Document> {
        Some(&mut self.document.as_mut()?.document)
    }

    fn reindex(&mut self) {
        if let Some(document) = &mut self.document {
            document.reindex();
        }
        self.metrics = None;
        self.editor = None;
    }

    fn central(&mut self, ui: &mut Ui, zoom: f32, editing: &mut Editing) {
        if self.document.is_none() {
            return;
        }
        egui::Panel::top("cell").show(ui, |ui| self.cell_bar(ui));
        egui::Panel::bottom("sheets").show(ui, |ui| self.sheet_tabs(ui));
        self.grid(ui, zoom, editing);
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
            if let Some(notice) = &self.notice {
                ui.separator();
                ui.colored_label(ui.visuals().warn_fg_color, notice);
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
        // `auto_shrink` vertically, or the scroll area claims every point the
        // panel could give it and leaves a dead band between the grid and the
        // tabs. It scrolls sideways because a workbook may have more sheets than
        // fit; it never scrolls down.
        egui::ScrollArea::horizontal()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, name) in names {
                        let picked = index == self.sheet;
                        if ui.selectable_label(picked, name).clicked() {
                            self.sheet = index;
                            self.selected = (0, 0);
                            self.metrics = None;
                            self.editor = None;
                            self.notice = None;
                        }
                    }
                });
            });
    }

    /// The grid, painting the cells the window covers and no others.
    #[allow(clippy::too_many_lines)]
    fn grid(&mut self, ui: &mut Ui, zoom: f32, editing: &mut Editing) {
        let was_editing = self.editor.is_some();
        let anything_changed = editing.modified();
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
        let editor = &mut self.editor;
        let mut outcome = None;

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let (width, height) = metrics.size();
                let (area, response) = ui.allocate_exact_size(
                    vec2(width + header_width, height + header_height),
                    Sense::click(),
                );
                let origin = area.min + vec2(header_width, header_height);
                let painter = ui.painter().clone();
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

                        let mut format =
                            format::text_format(&style.text, DEFAULT_SIZE, zoom, palette);
                        // A formula's cached result may be out of date once
                        // anything has changed, and which ones are cannot be
                        // told without evaluating them; every one is drawn
                        // faint until the file is opened by something that
                        // recalculates. DESIGN.md §11.
                        if anything_changed && cell.formula().is_some() {
                            format.color = format.color.gamma_multiply(0.45);
                        }
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

                // The cell being typed into, as a text box where the cell is.
                if let Some(editor) = editor.as_mut() {
                    if rows.contains(&editor.row) && columns.contains(&editor.column) {
                        let (top, row_height) = metrics.row(editor.row);
                        let (left, column_width) = metrics.column(editor.column);
                        // Wide enough to type in, whatever the column is.
                        let rect = Rect::from_min_size(
                            pos2(origin.x + left, origin.y + top),
                            vec2(column_width.max(120.0 * zoom), row_height),
                        );
                        let cell = document.cell(sheet, editor.row, editor.column);
                        let style = document.cell_style(sheet, cell.as_ref(), editor.column);
                        let format = format::text_format(&style.text, DEFAULT_SIZE, zoom, palette);
                        let id = egui::Id::new("cell-editor");
                        let response = ui.put(
                            rect,
                            egui::TextEdit::singleline(&mut editor.text)
                                .id(id)
                                .font(format.font_id.clone())
                                .text_color(format.color)
                                .background_color(palette.paper)
                                .margin(vec2(3.0 * zoom, 0.0))
                                .vertical_align(Align::Center),
                        );
                        if editor.opened {
                            editor.opened = false;
                            response.request_focus();
                            // The caret at the end of what is there, rather
                            // than egui's choice of the start.
                            let mut state =
                                egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
                            let end = egui::text::CCursor::new(editor.text.chars().count());
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(end)));
                            egui::TextEdit::store_state(ui.ctx(), id, state);
                        } else if response.lost_focus() {
                            outcome = Some(ui.input(|input| {
                                if input.key_pressed(Key::Escape) {
                                    Outcome::Cancel
                                } else if input.key_pressed(Key::Enter) {
                                    Outcome::Commit(Step::Down)
                                } else if input.key_pressed(Key::Tab) {
                                    Outcome::Commit(Step::Right)
                                } else {
                                    Outcome::Commit(Step::Stay)
                                }
                            }));
                        }
                    } else {
                        // Scrolled out of sight, which takes the focus with
                        // it: what was typed is kept rather than lost.
                        outcome = Some(Outcome::Commit(Step::Stay));
                    }
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
        if let Some(outcome) = outcome {
            self.finish_editing(outcome, editing);
        }
        if let Some(cell) = clicked
            && cell != self.selected
        {
            self.selected = cell;
            self.notice = None;
        }
        if self.editor.is_some() || editing.asking {
            return;
        }
        if !was_editing {
            self.begin_editing(ui, editing);
        }
        if self.editor.is_none() {
            self.arrow_keys(ui, extent);
        }
    }

    /// Open the editor on the picked cell when the keys ask for it: Enter or
    /// F2 with what the cell holds, a typed character in its place. Delete
    /// clears the cell without opening anything.
    fn begin_editing(&mut self, ui: &Ui, editing: &mut Editing) {
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        let (typed, open, delete) = ui.input(|input| {
            let typed = input.events.iter().find_map(|event| match event {
                Event::Text(text) => Some(text.clone()),
                _ => None,
            });
            (
                typed,
                input.key_pressed(Key::Enter) || input.key_pressed(Key::F2),
                input.key_pressed(Key::Delete),
            )
        });
        if typed.is_none() && !open && !delete {
            return;
        }
        let (row, column) = self.selected;
        let Some(document) = &self.document else {
            return;
        };
        if let Err(refused) = document.can_edit(self.sheet, row, column) {
            self.notice = Some(notice(&refused));
            return;
        }
        if delete {
            self.write(row, column, &Value::Empty, editing);
            return;
        }
        let original = document
            .sheets()
            .get(self.sheet)
            .and_then(|sheet| document.cell(sheet, row, column))
            .map(|cell| cell.value().input_text())
            .unwrap_or_default();
        let text = typed.unwrap_or_else(|| original.clone());
        self.notice = None;
        self.editor = Some(CellEditor {
            row,
            column,
            text,
            original,
            opened: true,
        });
    }

    /// Close the editor, writing what was typed if it was committed and is
    /// not what the cell already held.
    fn finish_editing(&mut self, outcome: Outcome, editing: &mut Editing) {
        let Some(editor) = self.editor.take() else {
            return;
        };
        let Outcome::Commit(step) = outcome else {
            return;
        };
        if editor.text != editor.original {
            self.write(
                editor.row,
                editor.column,
                &Value::from_input(&editor.text),
                editing,
            );
        }
        self.selected = match step {
            Step::Stay => (editor.row, editor.column),
            Step::Down => (editor.row + 1, editor.column),
            Step::Right => (editor.row, editor.column + 1),
        };
    }

    /// Put a value in a cell, recording the tree first so that it can be
    /// undone.
    fn write(&mut self, row: usize, column: usize, value: &Value, editing: &mut Editing) {
        let Some(document) = &mut self.document else {
            return;
        };
        if let Err(refused) = document.can_edit(self.sheet, row, column) {
            self.notice = Some(notice(&refused));
            return;
        }
        editing.record(&document.document.content);
        match document.set_cell(self.sheet, row, column, value) {
            Ok(()) => self.metrics = None,
            Err(refused) => self.notice = Some(notice(&refused)),
        }
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

/// What the cell bar says about a cell that cannot be edited.
fn notice(refused: &Refused) -> String {
    match refused {
        Refused::Formula => t("This cell holds a formula, which this version does not edit."),
        Refused::Covered => t("This cell is covered by the one that spans it."),
        Refused::NotFound => t("There is no cell there."),
    }
    .to_owned()
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
