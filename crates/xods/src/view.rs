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
use odox_ui::find::{Match, Replaced, ranges, replace_ranges};
use odox_ui::format::{self, DEFAULT_SIZE};
use odox_ui::i18n::{fill, t};
use odox_ui::{Editing, Found, View, fonts};

use crate::grid::{Metrics, address, column_name};

/// A spreadsheet, open or not.
#[derive(Default)]
pub struct SheetView {
    document: Option<SheetDocument>,
    sheet: usize,
    selected: (usize, usize),
    /// The corner of the selection opposite `selected`, which is where it was
    /// begun: the two are the corners of the range a copy takes.
    anchor: (usize, usize),
    /// The grid is to scroll to `selected` when it is next drawn.
    follow: bool,
    metrics: Option<Metrics>,
    /// What the metrics were measured for, so that they are measured again when
    /// the sheet or the zoom changes and not on every frame.
    measured: (usize, f32),
    /// The cell being typed into, while one is.
    editor: Option<CellEditor>,
    /// Why the picked cell could not be edited, shown until the pick moves.
    notice: Option<String>,
    /// What a search found, by sheet and by the cell's row and column.
    found: Found,
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
        self.pick((0, 0));
        self.metrics = None;
        self.editor = None;
        self.notice = None;
        self.found.clear();
        Ok(())
    }

    fn close(&mut self) {
        self.document = None;
        self.metrics = None;
        self.editor = None;
        self.notice = None;
        self.found.clear();
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

    fn find(&mut self, query: &str) -> usize {
        let matches = self
            .document
            .as_ref()
            .map(|document| matches_in(document, query))
            .unwrap_or_default();
        self.found.set(matches);
        self.found.len()
    }

    fn show_match(&mut self, index: usize) {
        let Some(found) = self.found.show(index).cloned() else {
            return;
        };
        if found.scope != self.sheet {
            self.sheet = found.scope;
            self.metrics = None;
            self.editor = None;
        }
        if let [row, column] = found.paragraph[..] {
            self.pick((row, column));
        }
        self.notice = None;
    }

    fn can_replace(&self, _editing: &Editing) -> bool {
        self.document.is_some()
    }

    /// Replace in the cells that hold text. A formula, and a cell holding a
    /// number, a date or a boolean, would be read again as something else or
    /// lose its format, so those are left and counted.
    fn replace(&mut self, with: &str, all: bool, editing: &mut Editing) -> Replaced {
        let matches: Vec<Match> = if all {
            self.found.all().to_vec()
        } else {
            self.found.current_match().cloned().into_iter().collect()
        };
        let Some(document) = &mut self.document else {
            return Replaced::default();
        };
        let mut done = Replaced::default();
        let mut writes = Vec::new();
        let mut at = 0;
        while at < matches.len() {
            let [row, column] = matches[at].paragraph[..] else {
                at += 1;
                continue;
            };
            let scope = matches[at].scope;
            let mut cell_ranges = Vec::new();
            while at < matches.len()
                && matches[at].scope == scope
                && matches[at].paragraph == [row, column]
            {
                cell_ranges.push(matches[at].range.clone());
                at += 1;
            }
            let Some(sheet) = document.sheets().get(scope) else {
                continue;
            };
            let Some(cell) = document.cell(sheet, row, column) else {
                continue;
            };
            if cell.formula().is_some() || !matches!(cell.value(), Value::Text(_)) {
                done.skipped += cell_ranges.len();
                continue;
            }
            let text = replace_ranges(&cell.text(), &cell_ranges, with);
            done.replaced += cell_ranges.len();
            writes.push((scope, row, column, Value::Text(text)));
        }
        if !writes.is_empty() {
            editing.record(&document.document.content);
            for (sheet, row, column, value) in &writes {
                let _ = document.set_cell(*sheet, *row, *column, value);
            }
            self.metrics = None;
        }
        done
    }

    fn central(&mut self, ui: &mut Ui, zoom: f32, editing: &mut Editing) {
        if self.document.is_none() {
            return;
        }
        egui::Panel::top("cell").show(ui, |ui| self.cell_bar(ui));
        egui::Panel::bottom("sheets").show(ui, |ui| self.sheet_tabs(ui));
        self.grid(ui, zoom, editing);
        self.found.drawn();
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
    /// Select one cell, which is then the whole selection.
    fn pick(&mut self, cell: (usize, usize)) {
        self.selected = cell;
        self.anchor = cell;
        self.follow = true;
        self.notice = None;
    }

    /// Stretch the selection from where it began to a cell.
    fn extend(&mut self, cell: (usize, usize)) {
        self.selected = cell;
        self.follow = true;
        self.notice = None;
    }

    /// The selected cells as the first and last row and the first and last
    /// column, each counted in.
    fn range(&self) -> ((usize, usize), (usize, usize)) {
        let ((a_row, a_column), (row, column)) = (self.anchor, self.selected);
        (
            (a_row.min(row), a_column.min(column)),
            (a_row.max(row), a_column.max(column)),
        )
    }

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

        let (first, last) = self.range();
        let label = if first == last {
            address(row, column)
        } else {
            format!("{}:{}", address(first.0, first.1), address(last.0, last.1))
        };
        ui.horizontal(|ui| {
            ui.monospace(label);
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
                            self.pick((0, 0));
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
        let palette = format::Palette::for_theme(ui.visuals().dark_mode);
        let faint = palette.ink.gamma_multiply(0.18);
        let header_fill = ui.visuals().faint_bg_color;
        let header_text = ui.visuals().text_color();
        let mut pointed = None;
        let follow = std::mem::take(&mut self.follow);
        let (range_start, range_end) = self.range();
        let highlights = self.found.highlights(self.sheet);
        let editor = &mut self.editor;
        let mut outcome = None;

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let (width, height) = metrics.size();
                let (area, response) = ui.allocate_exact_size(
                    vec2(width + header_width, height + header_height),
                    Sense::click_and_drag(),
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
                        if let Some(highlights) = &highlights {
                            let hits = highlights.within(&[row, column]);
                            if !hits.is_empty() {
                                let current = hits.iter().any(|&(_, current)| current);
                                painter.rect_filled(drawn, 0.0, format::match_fill(current));
                                format.color = egui::Color32::BLACK;
                                if current && highlights.reveal() {
                                    ui.scroll_to_rect(drawn, Some(Align::Center));
                                }
                            }
                        }
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

                // The selected range, tinted over the cells in it.
                if range_start != range_end {
                    let (top, _) = metrics.row(range_start.0);
                    let (left, _) = metrics.column(range_start.1);
                    let (bottom, bottom_height) = metrics.row(range_end.0);
                    let (right, right_width) = metrics.column(range_end.1);
                    painter.rect_filled(
                        Rect::from_min_max(
                            pos2(origin.x + left, origin.y + top),
                            pos2(
                                origin.x + right + right_width,
                                origin.y + bottom + bottom_height,
                            ),
                        ),
                        0.0,
                        ui.visuals().selection.bg_fill.gamma_multiply(0.35),
                    );
                }

                // The picked cell, over everything in it.
                let (row, column) = (self.selected.0, self.selected.1);
                if follow {
                    let (top, height) = metrics.row(row);
                    let (left, width) = metrics.column(column);
                    // Out from under the headers, which cover the grid's edge.
                    ui.scroll_to_rect(
                        Rect::from_min_size(
                            pos2(origin.x + left, origin.y + top)
                                - vec2(header_width, header_height),
                            vec2(width + header_width, height + header_height),
                        ),
                        None,
                    );
                }
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

                // A press picks the cell under it, or with Shift stretches the
                // selection to it, and a drag goes on stretching it. The cell
                // is the one under where the button went down: a frame can
                // see the press and the pointer's travel together.
                let cell_at = |at: egui::Pos2| {
                    (at.x > origin.x && at.y > origin.y).then(|| {
                        (
                            metrics.row_at(at.y - origin.y),
                            metrics.column_at(at.x - origin.x),
                        )
                    })
                };
                let (pressed, press_origin, shift) = ui.input(|input| {
                    (
                        input.pointer.primary_pressed(),
                        input.pointer.press_origin(),
                        input.modifiers.shift,
                    )
                });
                let at = response.interact_pointer_pos();
                if response.is_pointer_button_down_on()
                    && pressed
                    && let Some(cell) = press_origin.and_then(cell_at)
                {
                    pointed = Some((cell, shift));
                } else if response.clicked()
                    && let Some(cell) = at.and_then(cell_at)
                {
                    pointed = Some((cell, shift));
                } else if response.dragged()
                    && let Some(cell) = at.and_then(cell_at)
                {
                    pointed = Some((cell, true));
                }
            });

        let used = (sheet.used_rows, sheet.used_columns);
        let grid = self
            .metrics
            .as_ref()
            .map_or(used, |m| (m.rows.len() - 1, m.columns.len() - 1));
        if let Some(outcome) = outcome {
            self.finish_editing(outcome, editing);
        }
        match pointed {
            Some((cell, true)) if cell != self.selected => self.extend(cell),
            Some((cell, false)) if cell != self.selected || cell != self.anchor => self.pick(cell),
            _ => {}
        }
        if self.editor.is_some() || editing.asking {
            return;
        }
        if !was_editing {
            self.clipboard(ui, editing);
            self.begin_editing(ui, editing);
        }
        if self.editor.is_none() {
            self.arrow_keys(ui, grid, used);
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
        if delete {
            self.clear_range(editing);
            return;
        }
        if let Err(refused) = document.can_edit(self.sheet, row, column) {
            self.notice = Some(notice(&refused));
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
        self.pick(match step {
            Step::Stay => (editor.row, editor.column),
            Step::Down => (editor.row + 1, editor.column),
            Step::Right => (editor.row, editor.column + 1),
        });
    }

    /// Copy, cut and paste, which go through the selected cells as tab
    /// separated text, the way every spreadsheet's clipboard does.
    fn clipboard(&mut self, ui: &Ui, editing: &mut Editing) {
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        let events: Vec<Event> = ui.input(|input| {
            input
                .events
                .iter()
                .filter(|event| matches!(event, Event::Copy | Event::Cut | Event::Paste(_)))
                .cloned()
                .collect()
        });
        for event in events {
            match event {
                Event::Copy => self.copy(ui),
                Event::Cut => {
                    self.copy(ui);
                    self.clear_range(editing);
                }
                Event::Paste(text) => self.paste(&text, editing),
                _ => {}
            }
        }
    }

    /// Put what the selected cells show on the clipboard.
    fn copy(&self, ui: &Ui) {
        let Some(document) = &self.document else {
            return;
        };
        let Some(sheet) = document.sheets().get(self.sheet) else {
            return;
        };
        let ((top, left), (bottom, right)) = self.range();
        ui.ctx()
            .copy_text(text_of(document, sheet, top..bottom + 1, left..right + 1));
    }

    /// Empty the selected cells. A cell under a merged neighbour holds nothing
    /// to empty and is left; a formula refuses the whole of it.
    fn clear_range(&mut self, editing: &mut Editing) {
        let ((top, left), (bottom, right)) = self.range();
        let block: Vec<Vec<Value>> = (top..=bottom)
            .map(|_| (left..=right).map(|_| Value::Empty).collect())
            .collect();
        self.write_block((top, left), &block, true, editing);
    }

    /// Put copied cells down with their top left corner at the selection's,
    /// and select them.
    fn paste(&mut self, text: &str, editing: &mut Editing) {
        let block = cells_of(text);
        let ((top, left), _) = self.range();
        if self.write_block((top, left), &block, false, editing) {
            let width = block.iter().map(Vec::len).max().unwrap_or(1);
            self.pick((top, left));
            self.extend((top + block.len() - 1, left + width - 1));
        }
    }

    /// Write a block of values with its top left corner at a cell, as one
    /// step to undo. Nothing is written unless every cell can be: the first
    /// one that cannot is what the notice says. A block of empty values leaves
    /// alone a cell that is not there, and with `leave_covered` one that is
    /// under a merge. Answers whether anything was written.
    fn write_block(
        &mut self,
        (top, left): (usize, usize),
        block: &[Vec<Value>],
        leave_covered: bool,
        editing: &mut Editing,
    ) -> bool {
        let Some(document) = &mut self.document else {
            return false;
        };
        let sheet = self.sheet;
        let mut writes = Vec::new();
        for (down, row) in block.iter().enumerate() {
            for (across, value) in row.iter().enumerate() {
                let (r, c) = (top + down, left + across);
                let existing = document
                    .sheets()
                    .get(sheet)
                    .and_then(|sheet| document.cell(sheet, r, c));
                if matches!(value, Value::Empty)
                    && existing
                        .as_ref()
                        .is_none_or(|cell| leave_covered && cell.covered)
                {
                    continue;
                }
                if let Err(refused) = document.can_edit(sheet, r, c) {
                    self.notice = Some(notice(&refused));
                    return false;
                }
                writes.push((r, c, value));
            }
        }
        if writes.is_empty() {
            return false;
        }
        editing.record(&document.document.content);
        for (r, c, value) in writes {
            if let Err(refused) = document.set_cell(sheet, r, c, value) {
                self.notice = Some(notice(&refused));
                break;
            }
        }
        self.metrics = None;
        self.notice = None;
        true
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
    fn arrow_keys(&mut self, ui: &Ui, grid: (usize, usize), used: (usize, usize)) {
        let (mut row, mut column) = self.selected;
        // The arrows go as far as the grid is drawn, empty cells included, so
        // that a person can walk to one to type in it. End is the last column
        // the document wrote.
        let last_row = grid.0.saturating_sub(1);
        let last_column = grid.1.saturating_sub(1);
        let last_written = used.1.saturating_sub(1);
        let mut extend = false;
        ui.input(|input| {
            extend = input.modifiers.shift;
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
                column = last_written;
            }
        });
        let cell = (row.min(last_row), column.min(last_column));
        if cell != self.selected {
            if extend {
                self.extend(cell);
            } else {
                self.pick(cell);
            }
        }
    }
}

/// What the cell bar says about a cell that cannot be edited.
fn notice(refused: &Refused) -> String {
    match refused {
        Refused::Formula => t("This cell holds a formula, which this version does not edit."),
        Refused::Covered => t("This cell is covered by the one that spans it."),
        // A cell is never a range and is not formatted here, so a refusal
        // over structure or a namespace is not one a cell hears.
        Refused::NotFound | Refused::Structure | Refused::Namespace | Refused::LastOne => {
            t("There is no cell there.")
        }
    }
    .to_owned()
}

/// A sheet as text, for handing to something else.
fn as_text(document: &SheetDocument, sheet: &Sheet) -> String {
    text_of(document, sheet, 0..sheet.used_rows, 0..sheet.used_columns)
}

/// Some rows and columns of a sheet as text.
///
/// Tab separated and taking each cell as the document displays it, because that
/// is the string the producing application formatted and the one a person sees.
fn text_of(
    document: &SheetDocument,
    sheet: &Sheet,
    rows: std::ops::Range<usize>,
    columns: std::ops::Range<usize>,
) -> String {
    let mut out = String::new();
    for row in rows {
        for (at, column) in columns.clone().enumerate() {
            if at > 0 {
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

/// What the clipboard held, as rows of cells: a line is a row and a tab ends a
/// cell. A final newline ends the last row and does not begin another.
fn cells_of(text: &str) -> Vec<Vec<Value>> {
    let text = text.replace("\r\n", "\n");
    let text = text.strip_suffix('\n').unwrap_or(&text);
    text.split('\n')
        .map(|line| line.split('\t').map(Value::from_input).collect())
        .collect()
}

/// Every place a query occurs in the cells of the sheets a person can see.
fn matches_in(document: &SheetDocument, query: &str) -> Vec<Match> {
    let mut found = Vec::new();
    if query.is_empty() {
        return found;
    }
    for (scope, sheet) in document.sheets().iter().enumerate() {
        if !sheet.visible {
            continue;
        }
        for row in 0..sheet.used_rows {
            for column in 0..sheet.used_columns {
                let Some(cell) = document.cell(sheet, row, column).filter(|c| !c.covered) else {
                    continue;
                };
                for range in ranges(&cell.text(), query) {
                    found.push(Match {
                        scope,
                        paragraph: vec![row, column],
                        range,
                    });
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opened() -> (SheetView, Editing) {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/libreoffice/calc.ods"),
        )
        .expect("the corpus sheet");
        let view = SheetView {
            document: Some(SheetDocument::read(&bytes).expect("it reads")),
            ..SheetView::default()
        };
        let mut editing = Editing::default();
        editing.reset();
        (view, editing)
    }

    fn text_at(view: &SheetView, row: usize, column: usize) -> String {
        let document = view.document.as_ref().expect("open");
        let sheet = &document.sheets()[0];
        document
            .cell(sheet, row, column)
            .map(|cell| cell.text())
            .unwrap_or_default()
    }

    #[test]
    fn clipboard_text_is_rows_of_cells() {
        let block = cells_of("a\t1\r\nTRUE\t\n");
        assert_eq!(block.len(), 2);
        assert!(matches!(&block[0][0], Value::Text(t) if t == "a"));
        assert!(matches!(block[0][1], Value::Number(n) if (n - 1.0).abs() < f64::EPSILON));
        assert!(matches!(block[1][0], Value::Boolean(true)));
        assert!(matches!(block[1][1], Value::Empty));
    }

    #[test]
    fn a_paste_lands_at_the_selection_as_one_undo_step() {
        let (mut view, mut editing) = opened();
        view.pick((6, 1));
        view.paste("x\t7\ny\t8\n", &mut editing);
        assert_eq!(text_at(&view, 6, 1), "x");
        assert_eq!(text_at(&view, 7, 2), "8");
        assert_eq!(view.range(), ((6, 1), (7, 2)));
        assert!(editing.modified());
        let document = view.document.as_mut().expect("open");
        let (previous, _) = editing
            .undo(&document.document.content, None)
            .expect("one step");
        document.document.content = previous;
        document.reindex();
        assert!(!editing.can_undo(), "the paste was a single step");
        assert_eq!(text_at(&view, 6, 1), "");
    }

    #[test]
    fn a_paste_over_a_formula_writes_nothing() {
        let (mut view, mut editing) = opened();
        let (row, column) = (1..5)
            .flat_map(|r| (0..6).map(move |c| (r, c)))
            .find(|&(r, c)| {
                let document = view.document.as_ref().expect("open");
                document
                    .cell(&document.sheets()[0], r, c)
                    .is_some_and(|cell| cell.formula().is_some())
            })
            .expect("the sheet has a formula");
        view.pick((row, column - 1));
        let before = text_at(&view, row, column - 1);
        view.paste("changed\tchanged\n", &mut editing);
        assert_eq!(text_at(&view, row, column - 1), before);
        assert!(!editing.modified());
        assert!(view.notice.is_some());
    }

    #[test]
    fn deleting_a_range_empties_each_cell_in_it() {
        let (mut view, mut editing) = opened();
        view.pick((1, 0));
        view.extend((2, 1));
        view.clear_range(&mut editing);
        for (row, column) in [(1, 0), (1, 1), (2, 0), (2, 1)] {
            assert_eq!(text_at(&view, row, column), "", "{row},{column}");
        }
        assert!(editing.can_undo());
    }

    fn pressed(view: &mut SheetView, key: egui::Key, modifiers: egui::Modifiers) {
        let ctx = egui::Context::default();
        let events = vec![
            Event::ModifiersChanged(modifiers),
            Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
        ];
        let input = egui::RawInput {
            events,
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 400.0))),
            ..egui::RawInput::default()
        };
        let mut output = ctx.run_ui(input, |ui| view.arrow_keys(ui, (45, 44), (5, 7)));
        output.textures_delta.clear();
    }

    #[test]
    fn the_arrows_walk_past_the_data_into_the_empty_grid() {
        let (mut view, _) = opened();
        view.pick((4, 6));
        pressed(&mut view, Key::ArrowDown, egui::Modifiers::NONE);
        pressed(&mut view, Key::ArrowRight, egui::Modifiers::NONE);
        assert_eq!(view.selected, (5, 7));
        pressed(&mut view, Key::ArrowDown, egui::Modifiers::SHIFT);
        assert_eq!(
            view.range(),
            ((5, 7), (6, 7)),
            "Shift stretches past the data too"
        );
    }

    #[test]
    fn end_goes_to_the_last_column_the_document_wrote() {
        let (mut view, _) = opened();
        view.pick((1, 0));
        pressed(&mut view, Key::End, egui::Modifiers::NONE);
        assert_eq!(view.selected, (1, 6));
    }

    #[test]
    fn replace_all_changes_text_cells_in_one_step_and_leaves_the_rest() {
        let (mut view, mut editing) = opened();
        assert_eq!(view.find("m6"), 2, "Bolt M6 and Nut M6");
        let done = view.replace("M8", true, &mut editing);
        assert_eq!(
            done,
            Replaced {
                replaced: 2,
                skipped: 0
            }
        );
        assert_eq!(text_at(&view, 1, 0), "Bolt M8");
        assert_eq!(text_at(&view, 2, 0), "Nut M8");
        assert_eq!(text_at(&view, 3, 0), "Washer");
        let document = view.document.as_mut().expect("open");
        editing
            .undo(&document.document.content, None)
            .expect("one step");
        assert!(!editing.can_undo(), "both cells were one step");
    }

    #[test]
    fn replace_leaves_numbers_and_formulas_alone_and_says_so() {
        let (mut view, mut editing) = opened();
        let found = view.find("1");
        assert!(found > 0);
        let done = view.replace("9", true, &mut editing);
        assert_eq!(done.replaced, 0);
        assert_eq!(done.skipped, found);
        assert!(!editing.modified(), "nothing was written");
        assert_eq!(text_at(&view, 4, 3), "136.4");
    }

    #[test]
    fn replace_one_takes_the_current_match_only() {
        let (mut view, mut editing) = opened();
        view.find("m6");
        view.show_match(1);
        view.replace("X", false, &mut editing);
        assert_eq!(text_at(&view, 1, 0), "Bolt M6");
        assert_eq!(text_at(&view, 2, 0), "Nut X");
    }

    #[test]
    fn the_selection_is_the_rectangle_between_its_corners() {
        let (mut view, _) = opened();
        view.pick((4, 3));
        view.extend((1, 5));
        assert_eq!(view.range(), ((1, 3), (4, 5)));
    }
}
