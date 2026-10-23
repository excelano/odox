//! The window's one view: a sheet drawn as a grid, and the cell a person picked.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use eframe::egui::{
    self, Align, Event, FontId, Key, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use odox_core::doc::{Blocked, Restructure, Sheet, SheetDocument, Value};
use odox_core::{Document, Refused};
use odox_ui::find::{Match, Replaced, ranges, replace_ranges};
use odox_ui::format::{self, DEFAULT_SIZE};
use odox_ui::i18n::{fill, t};
use odox_ui::{Editing, Export, Found, Notice, View, fonts};

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
    /// Where the grid is to scroll when it is next drawn.
    scroll: Scroll,
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
    /// What the Sheet menu last chose.
    sheet_command: Option<SheetCommand>,
    /// The View menu asked to freeze or unfreeze.
    freeze_asked: bool,
    /// What the Sheet menu or a tab's menu asked of a sheet, and which.
    book_command: Option<(BookCommand, usize)>,
    /// The sheet being renamed, while the question is up.
    renaming: Option<Renaming>,
    /// How many rows the grid last showed, which is how far Page Up and Page
    /// Down go.
    page_rows: usize,
    /// What the selection holds, kept for the sheet and range it was counted
    /// over until the document changes.
    summary: Option<(usize, Range, Summary)>,
    /// A column or row whose edge is being dragged, with the size it has so
    /// far, in points.
    resizing: Option<Resize>,
    /// Whether a value in the document has been changed since it was opened,
    /// which may have left a formula's result out of date. A width or a
    /// height leaves every result as it was.
    values_changed: bool,
    export: Export,
}

/// Where the grid scrolls to.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Scroll {
    /// Where it is.
    #[default]
    Stay,
    /// To the picked cell.
    Picked,
    /// Back to its start, which a frozen cell being picked does not ask for
    /// by itself.
    Start,
}

/// A column's width or a row's height, being changed.
#[derive(Clone, Copy, PartialEq)]
struct Resize {
    column: bool,
    index: usize,
    size: f32,
}

/// Where a cell is drawn: from the grid's origin, which scrolls, or for a
/// frozen row or column from where the frozen ones stay.
#[derive(Clone, Copy)]
struct Place {
    origin: egui::Pos2,
    pinned: egui::Pos2,
    /// The frozen rows and columns.
    frozen: (usize, usize),
}

impl Place {
    fn x(self, metrics: &Metrics, column: usize) -> f32 {
        let from = if column < self.frozen.1 {
            self.pinned.x
        } else {
            self.origin.x
        };
        from + metrics.column(column).0
    }

    fn y(self, metrics: &Metrics, row: usize) -> f32 {
        let from = if row < self.frozen.0 {
            self.pinned.y
        } else {
            self.origin.y
        };
        from + metrics.row(row).0
    }

    /// A cell's top left corner.
    fn at(self, metrics: &Metrics, row: usize, column: usize) -> egui::Pos2 {
        pos2(self.x(metrics, column), self.y(metrics, row))
    }
}

/// What a frame paints cells with.
struct Paint<'a> {
    document: &'a SheetDocument,
    sheet: &'a Sheet,
    metrics: &'a Metrics,
    palette: format::Palette,
    zoom: f32,
    values_changed: bool,
    highlights: Option<odox_ui::Highlights<'a>>,
}

impl Paint<'_> {
    /// The grid's lines between some rows and columns, first, so that a cell
    /// with a fill covers them the way a spreadsheet's do.
    fn lines(
        &self,
        painter: &egui::Painter,
        place: Place,
        rows: &std::ops::Range<usize>,
        columns: &std::ops::Range<usize>,
        colour: egui::Color32,
    ) {
        let metrics = self.metrics;
        let (last_row, last_column) = (rows.end - 1, columns.end - 1);
        let left = place.x(metrics, columns.start);
        let right = place.x(metrics, last_column) + metrics.column(last_column).1;
        let top = place.y(metrics, rows.start);
        let bottom = place.y(metrics, last_row) + metrics.row(last_row).1;
        for row in rows.clone() {
            let y = place.y(metrics, row) + metrics.row(row).1;
            painter.hline(left..=right, y, Stroke::new(1.0, colour));
        }
        for column in columns.clone() {
            let x = place.x(metrics, column) + metrics.column(column).1;
            painter.vline(x, top..=bottom, Stroke::new(1.0, colour));
        }
    }

    /// One cell: its fill, its borders and its text.
    fn cell(&self, ui: &Ui, painter: &egui::Painter, place: Place, row: usize, column: usize) {
        let (document, sheet, metrics, palette, zoom) = (
            self.document,
            self.sheet,
            self.metrics,
            self.palette,
            self.zoom,
        );
        let (_, row_height) = metrics.row(row);
        let (_, column_width) = metrics.column(column);
        let cell = document.cell(sheet, row, column);
        let style = document.cell_style(sheet, cell.as_ref(), column);

        // A merged cell is one cell over several columns and rows. Its fill,
        // its borders and its text all belong to the whole rectangle rather
        // than to the first column of it, so the extent is worked out before
        // anything is painted.
        let across = cell
            .as_ref()
            .map_or(1, odox_core::doc::Cell::columns_spanned);
        let down = cell.as_ref().map_or(1, odox_core::doc::Cell::rows_spanned);
        let width: f32 = (0..across).map(|n| metrics.column(column + n).1).sum();
        let height: f32 = (0..down).map(|n| metrics.row(row + n).1).sum();
        let rect = Rect::from_min_size(
            place.at(metrics, row, column),
            vec2(width.max(column_width), height.max(row_height)),
        );

        if let Some(fill) = style.cell.background {
            painter.rect_filled(rect, 0.0, format::color32(fill));
        } else if across > 1 || down > 1 {
            // One cell, so the grid's lines stop at its edge.
            painter.rect_filled(rect.shrink(0.5), 0.0, palette.paper);
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

        if let Some(cell) = cell.filter(|cell| !cell.covered) {
            self.text(ui, painter, rect, &cell, &style, (row, column));
        }
    }

    /// A cell's text, inside the rectangle it is drawn in.
    fn text(
        &self,
        ui: &Ui,
        painter: &egui::Painter,
        rect: Rect,
        cell: &odox_core::doc::Cell<'_>,
        style: &odox_core::Properties,
        (row, column): (usize, usize),
    ) {
        let (palette, zoom) = (self.palette, self.zoom);
        let text = cell.text();
        if text.is_empty() {
            return;
        }
        let ground = style
            .cell
            .background
            .map_or(palette, |fill| palette.on(format::color32(fill)));
        let mut format = format::text_format(&style.text, DEFAULT_SIZE, zoom, ground);
        if let Some(highlights) = &self.highlights {
            let hits = highlights.within(&[row, column]);
            if !hits.is_empty() {
                let current = hits.iter().any(|&(_, current)| current);
                painter.rect_filled(rect, 0.0, format::match_fill(current));
                format.color = egui::Color32::BLACK;
                if current && highlights.reveal() {
                    ui.scroll_to_rect(rect, Some(Align::Center));
                }
            }
        }
        // A formula's cached result may be out of date once a value has
        // changed, and which ones are cannot be told without evaluating them;
        // every one is drawn faint until the file is opened by something that
        // recalculates. DESIGN.md §11.
        if self.values_changed && cell.formula().is_some() {
            format.color = format.color.gamma_multiply(0.45);
        }
        let align = match style.paragraph.align {
            Some(odox_core::TextAlign::Center) => Align::Center,
            Some(odox_core::TextAlign::End) => Align::Max,
            // Text against the left edge and every other type against the
            // right, which is ODF's own rule.
            None if cell.value().is_numeric() => Align::Max,
            _ => Align::Min,
        };
        let padding = 3.0 * zoom;
        let anchor = match align {
            Align::Min => pos2(rect.left() + padding, rect.top()),
            Align::Center => pos2(rect.center().x, rect.top()),
            Align::Max => pos2(rect.right() - padding, rect.top()),
        };
        let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
        if style.cell.wrap == Some(true) {
            // Wrapped inside the column, each line aligned the way the cell
            // is, and cut off at the row's height.
            let mut job = egui::text::LayoutJob::simple(
                text,
                format.font_id.clone(),
                format.color,
                (rect.width() - padding * 2.0).max(1.0),
            );
            job.halign = align;
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
            painter.galley(anchor, galley, format.color);
        } else {
            painter.text(
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
}

/// What a header's edge was asked for in a frame.
enum Edge {
    /// Dragged to this size, or still being dragged.
    Dragging(Resize),
    /// Let go at this size.
    Released(Resize),
}

/// The first and last row and the first and last column, each counted in.
type Range = ((usize, usize), (usize, usize));

/// What a selection holds: the numbers in it and every cell that says
/// anything.
#[derive(Clone, Copy, Default)]
struct Summary {
    sum: f64,
    numbers: usize,
    filled: usize,
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

/// What the Sheet menu asked for, applied when the grid is next drawn.
#[derive(Clone, Copy)]
enum SheetCommand {
    RowAbove,
    RowBelow,
    ColumnLeft,
    ColumnRight,
    DeleteRows,
    DeleteColumns,
}

/// What is asked of one of the workbook's sheets.
#[derive(Clone, Copy)]
enum BookCommand {
    Add,
    Rename,
    Delete,
    Left,
    Right,
}

/// A sheet's new name, as it is typed.
struct Renaming {
    sheet: usize,
    name: String,
    /// Why the name typed so far was not taken.
    problem: Option<String>,
    /// The question was put up this frame and has yet to take the focus.
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
        fonts::install(ctx, &parts);

        // The first sheet a person can see, which is not always the first sheet:
        // a workbook may open on a hidden one.
        self.sheet = document
            .sheets()
            .iter()
            .position(|s| s.visible)
            .unwrap_or(0);
        self.document = Some(document);
        self.values_changed = false;
        self.pick((0, 0));
        self.metrics = None;
        self.editor = None;
        self.notice = None;
        self.found.clear();
        self.export = Export::default();
        Ok(())
    }

    fn blank(&self, language: &str) -> Vec<u8> {
        odox_core::blank::spreadsheet(language, &fill(t("Sheet{number}"), &[("number", "1")]))
    }

    fn close(&mut self) {
        self.document = None;
        self.metrics = None;
        self.editor = None;
        self.notice = None;
        self.found.clear();
        self.export = Export::default();
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
            // An undo can take away the sheet being looked at.
            if document
                .sheets()
                .get(self.sheet)
                .is_none_or(|sheet| !sheet.visible)
            {
                self.sheet = document
                    .sheets()
                    .iter()
                    .position(|s| s.visible)
                    .unwrap_or(0);
                self.pick((0, 0));
            }
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

    fn menus(&mut self, ui: &mut Ui, editing: &Editing) {
        let can = !editing.asking && self.editor.is_none();
        ui.menu_button(t("Sheet"), |ui| {
            let entries = [
                (t("Insert row above"), SheetCommand::RowAbove),
                (t("Insert row below"), SheetCommand::RowBelow),
                (t("Insert column left"), SheetCommand::ColumnLeft),
                (t("Insert column right"), SheetCommand::ColumnRight),
                (t("Delete row"), SheetCommand::DeleteRows),
                (t("Delete column"), SheetCommand::DeleteColumns),
            ];
            for (name, command) in entries {
                if ui.add_enabled(can, egui::Button::new(name)).clicked() {
                    self.sheet_command = Some(command);
                    ui.close();
                }
            }
            ui.separator();
            self.book_menu(ui, self.sheet, can);
        });
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
            self.values_changed = true;
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
        if let Some(command) = self.sheet_command.take() {
            self.change_sheet(command, editing);
        }
        if std::mem::take(&mut self.freeze_asked) {
            self.freeze(editing);
        }
        if let Some((command, sheet)) = self.book_command.take() {
            self.change_book(command, sheet, editing);
        }
        self.rename_question(ui.ctx(), editing);
        if let Some(document) = &mut self.document {
            self.export.show(ui.ctx(), document, editing);
        }
        let can = !editing.asking && self.editor.is_none();
        egui::Panel::top("cell").show(ui, |ui| self.cell_bar(ui));
        egui::Panel::bottom("sheets").show(ui, |ui| self.sheet_tabs(ui, can));
        self.grid(ui, zoom, editing);
        self.found.drawn();
    }

    fn side(&mut self, _ui: &mut Ui) -> bool {
        // A workbook's sheets are its tabs along the bottom, where a person
        // reaches for them, so there is nothing to put beside the grid.
        false
    }

    fn file_menu(&mut self, ui: &mut Ui, path: Option<&Path>) {
        ui.separator();
        self.export.menu(ui, path);
    }

    fn take_notice(&mut self) -> Option<Notice> {
        self.export.take_notice()
    }

    fn view_menu(&mut self, ui: &mut Ui) {
        let Some(document) = &self.document else {
            return;
        };
        let Some(sheet) = document.sheets().get(self.sheet) else {
            return;
        };
        ui.separator();
        let label = if sheet.frozen == odox_core::doc::Frozen::default() {
            t("Freeze rows and columns")
        } else {
            t("Unfreeze rows and columns")
        };
        if ui.button(label).clicked() {
            self.freeze_asked = true;
            ui.close();
        }
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
    /// Insert or delete the rows or columns the selection covers, as one step
    /// to undo, and select what was put in; or say what is in the way.
    fn change_sheet(&mut self, command: SheetCommand, editing: &mut Editing) {
        let ((top, left), (bottom, right)) = self.range();
        let (rows, columns) = (bottom - top + 1, right - left + 1);
        let change = match command {
            SheetCommand::RowAbove => Restructure::InsertRows {
                at: top,
                count: rows,
            },
            SheetCommand::RowBelow => Restructure::InsertRows {
                at: bottom + 1,
                count: rows,
            },
            SheetCommand::ColumnLeft => Restructure::InsertColumns {
                at: left,
                count: columns,
            },
            SheetCommand::ColumnRight => Restructure::InsertColumns {
                at: right + 1,
                count: columns,
            },
            SheetCommand::DeleteRows => Restructure::DeleteRows {
                at: top,
                count: rows,
            },
            SheetCommand::DeleteColumns => Restructure::DeleteColumns {
                at: left,
                count: columns,
            },
        };
        let Some(document) = &mut self.document else {
            return;
        };
        let before = document.document.content.clone();
        if let Err(blocked) = document.restructure(self.sheet, change) {
            self.notice = Some(blocked_notice(&blocked));
            return;
        }
        editing.record_snapshot(before, None);
        self.values_changed = true;
        self.metrics = None;
        self.editor = None;
        let new = match command {
            SheetCommand::RowAbove | SheetCommand::ColumnLeft => {
                Some(((top, left), (bottom, right)))
            }
            SheetCommand::RowBelow => Some(((bottom + 1, left), (bottom + rows, right))),
            SheetCommand::ColumnRight => Some(((top, right + 1), (bottom, right + columns))),
            SheetCommand::DeleteRows | SheetCommand::DeleteColumns => None,
        };
        match new {
            Some((from, to)) => {
                self.pick(from);
                self.extend(to);
            }
            None => self.pick((top, left)),
        }
    }

    /// Look at another sheet, from its first cell.
    fn show_sheet(&mut self, index: usize) {
        self.sheet = index;
        self.pick((0, 0));
        self.metrics = None;
        self.editor = None;
        self.notice = None;
    }

    /// The items that add, rename, delete and move a sheet, for the Sheet
    /// menu and for a tab's own.
    fn book_menu(&mut self, ui: &mut Ui, sheet: usize, can: bool) {
        let Some(document) = &self.document else {
            return;
        };
        let shown: Vec<usize> = document
            .sheets()
            .iter()
            .enumerate()
            .filter(|(_, s)| s.visible)
            .map(|(index, _)| index)
            .collect();
        let entries = [
            (t("Add a sheet"), BookCommand::Add, true),
            (t("Rename the sheet…"), BookCommand::Rename, true),
            (t("Delete the sheet"), BookCommand::Delete, shown.len() > 1),
            (
                t("Move the sheet left"),
                BookCommand::Left,
                shown.first().is_some_and(|&first| first < sheet),
            ),
            (
                t("Move the sheet right"),
                BookCommand::Right,
                shown.last().is_some_and(|&last| last > sheet),
            ),
        ];
        for (name, command, possible) in entries {
            if ui
                .add_enabled(can && possible, egui::Button::new(name))
                .clicked()
            {
                self.book_command = Some((command, sheet));
                ui.close();
            }
        }
    }

    /// Add, delete or move a sheet as one step to undo, or put up the
    /// question that renames one.
    fn change_book(&mut self, command: BookCommand, sheet: usize, editing: &mut Editing) {
        let Some(document) = &mut self.document else {
            return;
        };
        if let BookCommand::Rename = command {
            let name = document
                .sheets()
                .get(sheet)
                .map(|s| s.name.clone())
                .unwrap_or_default();
            self.renaming = Some(Renaming {
                sheet,
                name,
                problem: None,
                opened: true,
            });
            return;
        }
        let before = document.document.content.clone();
        let shown = |document: &SheetDocument, forward: bool| {
            let visible = |&index: &usize| document.sheets()[index].visible;
            if forward {
                (sheet + 1..document.sheets().len()).find(visible)
            } else {
                (0..sheet).rev().find(visible)
            }
        };
        let done = match command {
            BookCommand::Add => {
                let name = fresh_name(document);
                document.add_sheet(sheet, &name)
            }
            BookCommand::Delete => document.delete_sheet(sheet).map(|()| {
                let after = document.sheets().len().saturating_sub(1).min(sheet);
                (0..=after)
                    .rev()
                    .chain(after..document.sheets().len())
                    .find(|&index| document.sheets()[index].visible)
                    .unwrap_or(0)
            }),
            BookCommand::Left | BookCommand::Right => {
                match shown(document, matches!(command, BookCommand::Right)) {
                    Some(to) => document.move_sheet(sheet, to).map(|()| to),
                    None => return,
                }
            }
            BookCommand::Rename => return,
        };
        match done {
            Ok(index) => {
                editing.record_snapshot(before, None);
                self.show_sheet(index);
            }
            Err(blocked) => self.notice = Some(blocked_notice(&blocked)),
        }
    }

    /// The question that renames a sheet, while it is up.
    fn rename_question(&mut self, ctx: &egui::Context, editing: &mut Editing) {
        let Some(renaming) = &mut self.renaming else {
            return;
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::new("rename-sheet")).show(ctx, |ui| {
            ui.heading(t("Rename the sheet"));
            let response = ui.add(
                egui::TextEdit::singleline(&mut renaming.name)
                    .id(egui::Id::new("sheet-name"))
                    .desired_width(260.0),
            );
            if renaming.opened {
                renaming.opened = false;
                response.request_focus();
            }
            if let Some(problem) = &renaming.problem {
                ui.colored_label(ui.visuals().warn_fg_color, problem);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let entered = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                if ui.button(t("Rename")).clicked() || entered {
                    answer = Some(true);
                }
                if ui.button(t("Cancel")).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                let Some(document) = &mut self.document else {
                    return;
                };
                let before = document.document.content.clone();
                match document.rename_sheet(renaming.sheet, renaming.name.trim()) {
                    Ok(()) => {
                        editing.record_snapshot(before, None);
                        self.renaming = None;
                    }
                    Err(blocked) => renaming.problem = Some(blocked_notice(&blocked)),
                }
            }
            Some(false) => self.renaming = None,
            None => {}
        }
    }

    /// Freeze the rows above and the columns left of the picked cell, or,
    /// with any frozen, unfreeze them. A setting rather than a step to undo,
    /// and one the next save writes.
    fn freeze(&mut self, editing: &mut Editing) {
        let Some(document) = &mut self.document else {
            return;
        };
        let Some(sheet) = document.sheets().get(self.sheet) else {
            return;
        };
        let frozen = if sheet.frozen == odox_core::doc::Frozen::default() {
            odox_core::doc::Frozen {
                rows: self.selected.0,
                columns: self.selected.1,
            }
        } else {
            odox_core::doc::Frozen::default()
        };
        match document.set_frozen(self.sheet, frozen) {
            Ok(()) => editing.change_settings(),
            Err(refused) => self.notice = Some(notice(&refused)),
        }
        self.scroll = Scroll::Picked;
    }

    /// Select one cell, which is then the whole selection.
    fn pick(&mut self, cell: (usize, usize)) {
        self.selected = cell;
        self.anchor = cell;
        self.scroll = Scroll::Picked;
        self.notice = None;
    }

    /// Stretch the selection from where it began to a cell.
    fn extend(&mut self, cell: (usize, usize)) {
        self.selected = cell;
        self.scroll = Scroll::Picked;
        self.notice = None;
    }

    /// The selected cells as the first and last row and the first and last
    /// column, each counted in.
    fn range(&self) -> Range {
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
            if first != last {
                let range = (first, last);
                let summary = match self.summary {
                    Some((sheet, counted, summary))
                        if sheet == self.sheet && counted == range && self.metrics.is_some() =>
                    {
                        summary
                    }
                    _ => {
                        let summary = summarize(document, sheet, range);
                        self.summary = Some((self.sheet, range, summary));
                        summary
                    }
                };
                ui.separator();
                if summary.numbers > 0 {
                    #[allow(clippy::cast_precision_loss)]
                    let average = summary.sum / summary.numbers as f64;
                    ui.label(fill(t("Sum: {sum}"), &[("sum", &shown(summary.sum))]));
                    ui.label(fill(
                        t("Average: {average}"),
                        &[("average", &shown(average))],
                    ));
                }
                ui.label(fill(
                    t("Count: {count}"),
                    &[("count", &summary.filled.to_string())],
                ));
            }
            if let Some(notice) = &self.notice {
                ui.separator();
                ui.colored_label(ui.visuals().warn_fg_color, notice);
            }
        });
    }

    /// The tabs along the bottom, one per sheet a person can see.
    fn sheet_tabs(&mut self, ui: &mut Ui, can: bool) {
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
                        let response = ui.selectable_label(picked, name);
                        if response.clicked() {
                            self.show_sheet(index);
                        }
                        if response.double_clicked() && can {
                            self.book_command = Some((BookCommand::Rename, index));
                        }
                        response.context_menu(|ui| self.book_menu(ui, index, can));
                    }
                });
            });
    }

    /// The grid, painting the cells the window covers and no others.
    #[allow(clippy::too_many_lines)]
    fn grid(&mut self, ui: &mut Ui, zoom: f32, editing: &mut Editing) {
        let was_editing = self.editor.is_some();
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
        let paint = Paint {
            document,
            sheet,
            metrics,
            palette,
            zoom,
            values_changed: self.values_changed,
            highlights: self.found.highlights(self.sheet),
        };
        // No more frozen than the grid has.
        let frozen = (
            sheet.frozen.rows.min(metrics.rows.len() - 1),
            sheet.frozen.columns.min(metrics.columns.len() - 1),
        );
        // How much of the grid the frozen rows and columns take.
        let band = vec2(metrics.column(frozen.1).0, metrics.row(frozen.0).0);
        let mut pointed = None;
        let scroll = std::mem::take(&mut self.scroll);
        let (range_start, range_end) = self.range();
        let selected = self.selected;
        let editor = &mut self.editor;
        let mut outcome = None;
        let mut page_rows = self.page_rows;
        let resizing = self.resizing;
        let mut edge = None;

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let (width, height) = metrics.size();
                let (area, response) = ui.allocate_exact_size(
                    vec2(width + header_width, height + header_height),
                    Sense::click_and_drag(),
                );
                let origin = area.min + vec2(header_width, header_height);
                // Where the frozen rows and columns stay: at the edge of what
                // is in view, under the headers.
                let pinned = area.min + viewport.min.to_vec2() + vec2(header_width, header_height);
                let place = Place {
                    origin,
                    pinned,
                    frozen,
                };
                let painter = ui.painter().clone();
                painter.rect_filled(Rect::from_min_max(origin, area.max), 0.0, palette.paper);

                // The rows and columns that scroll, from the first one the
                // frozen ones leave in view.
                let rows =
                    metrics.rows_between(viewport.min.y + band.y, viewport.max.y - header_height);
                let rows = rows.start.max(frozen.0)..rows.end.max(frozen.0);
                let columns =
                    metrics.columns_between(viewport.min.x + band.x, viewport.max.x - header_width);
                let columns = columns.start.max(frozen.1)..columns.end.max(frozen.1);
                // The last row is seldom all in view, and a page keeps one row of
                // the one before it in sight.
                page_rows = rows.len().saturating_sub(2);

                let visible = ui.clip_rect();
                let body = Rect::from_min_max(pinned + band, visible.max);
                // Four panes, each drawn inside its own edges so that what
                // scrolls under the frozen rows and columns is not seen: the
                // part that scrolls both ways, the frozen rows above it, the
                // frozen columns beside it, and the corner where they meet.
                let panes = [
                    (rows.clone(), columns.clone(), body),
                    (
                        0..frozen.0,
                        columns.clone(),
                        Rect::from_min_max(
                            pos2(body.min.x, pinned.y),
                            pos2(body.max.x, body.min.y),
                        ),
                    ),
                    (
                        rows.clone(),
                        0..frozen.1,
                        Rect::from_min_max(
                            pos2(pinned.x, body.min.y),
                            pos2(body.min.x, body.max.y),
                        ),
                    ),
                    (
                        0..frozen.0,
                        0..frozen.1,
                        Rect::from_min_max(pinned, body.min),
                    ),
                ];
                let pane_of = |row: usize, column: usize| {
                    panes[usize::from(row < frozen.0) + 2 * usize::from(column < frozen.1)].2
                };
                for (rows, columns, clip) in &panes {
                    if rows.is_empty() || columns.is_empty() {
                        continue;
                    }
                    let painter = painter.with_clip_rect(clip.intersect(visible));
                    painter.rect_filled(*clip, 0.0, palette.paper);
                    paint.lines(&painter, place, rows, columns, faint);
                    for row in rows.clone() {
                        for column in columns.clone() {
                            paint.cell(ui, &painter, place, row, column);
                        }
                    }
                    // The selected range, tinted over the cells of it in this
                    // pane.
                    let top = range_start.0.max(rows.start);
                    let bottom = range_end.0.min(rows.end - 1);
                    let left = range_start.1.max(columns.start);
                    let right = range_end.1.min(columns.end - 1);
                    if range_start != range_end && top <= bottom && left <= right {
                        painter.rect_filled(
                            Rect::from_min_max(
                                place.at(metrics, top, left),
                                place.at(metrics, bottom, right)
                                    + vec2(metrics.column(right).1, metrics.row(bottom).1),
                            ),
                            0.0,
                            ui.visuals().selection.bg_fill.gamma_multiply(0.35),
                        );
                    }
                }

                // The picked cell, over everything in it.
                let (row, column) = selected;
                let at = place.at(metrics, row, column);
                let size = vec2(metrics.column(column).1, metrics.row(row).1);
                if scroll == Scroll::Start {
                    ui.scroll_to_rect(Rect::from_min_size(area.min, vec2(1.0, 1.0)), None);
                } else if scroll == Scroll::Picked {
                    // Out from under the headers and the frozen rows and
                    // columns, which cover the grid's edge; a frozen row or
                    // column is in view already and is not scrolled to.
                    let min = pos2(
                        if column < frozen.1 {
                            at.x
                        } else {
                            at.x - header_width - band.x
                        },
                        if row < frozen.0 {
                            at.y
                        } else {
                            at.y - header_height - band.y
                        },
                    );
                    ui.scroll_to_rect(Rect::from_min_max(min, at + size), None);
                }
                painter
                    .with_clip_rect(pane_of(row, column).intersect(visible))
                    .rect_stroke(
                        Rect::from_min_size(at, size),
                        0.0,
                        Stroke::new(2.0, ui.visuals().selection.stroke.color),
                        StrokeKind::Inside,
                    );

                // The cell being typed into, as a text box where the cell is.
                if let Some(editor) = editor.as_mut() {
                    let in_view = (editor.row < frozen.0 || rows.contains(&editor.row))
                        && (editor.column < frozen.1 || columns.contains(&editor.column));
                    if in_view {
                        let at = place.at(metrics, editor.row, editor.column);
                        let (_, row_height) = metrics.row(editor.row);
                        let (_, column_width) = metrics.column(editor.column);
                        // Wide enough to type in, whatever the column is.
                        let rect = Rect::from_min_size(
                            at,
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
                // stay where a person is looking while the grid scrolls under them;
                // a frozen row's or column's stays with it.
                let header_font = FontId::proportional(11.0 * zoom);
                let top = pinned.y - header_height;
                let left = pinned.x - header_width;
                let header = |rect: Rect, clip: Rect, label: String| {
                    let painter = painter.with_clip_rect(clip.intersect(visible));
                    painter.rect_filled(rect, 0.0, header_fill);
                    painter.rect_stroke(rect, 0.0, Stroke::new(1.0, faint), StrokeKind::Inside);
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        header_font.clone(),
                        header_text,
                    );
                };
                for column in (0..frozen.1).chain(columns.clone()) {
                    let x = place.x(metrics, column);
                    let rect = Rect::from_min_size(
                        pos2(x, top),
                        vec2(metrics.column(column).1, header_height),
                    );
                    let clip = if column < frozen.1 {
                        Rect::from_min_max(pos2(pinned.x, top), pos2(body.min.x, pinned.y))
                    } else {
                        Rect::from_min_max(pos2(body.min.x, top), pos2(body.max.x, pinned.y))
                    };
                    header(rect, clip, column_name(column));
                }
                for row in (0..frozen.0).chain(rows.clone()) {
                    let y = place.y(metrics, row);
                    let rect =
                        Rect::from_min_size(pos2(left, y), vec2(header_width, metrics.row(row).1));
                    let clip = if row < frozen.0 {
                        Rect::from_min_max(pos2(left, pinned.y), pos2(pinned.x, body.min.y))
                    } else {
                        Rect::from_min_max(pos2(left, body.min.y), pos2(pinned.x, body.max.y))
                    };
                    header(rect, clip, (row + 1).to_string());
                }
                // The square where the two headers meet, which would otherwise show
                // the grid scrolling behind them.
                painter.rect_filled(
                    Rect::from_min_size(pos2(left, top), vec2(header_width, header_height)),
                    0.0,
                    header_fill,
                );
                // Where the frozen rows and columns end.
                let line = Stroke::new(1.5, palette.ink.gamma_multiply(0.5));
                if frozen.0 > 0 {
                    painter.hline(pinned.x..=body.max.x, body.min.y, line);
                }
                if frozen.1 > 0 {
                    painter.vline(body.min.x, pinned.y..=body.max.y, line);
                }

                // The edge of a column's or a row's header is dragged to
                // resize it, and a column's is double-clicked to fit its text.
                // One that has scrolled under the frozen ones is not there to
                // take hold of.
                for column in (0..frozen.1).chain(columns.clone()) {
                    let x = place.x(metrics, column) + metrics.column(column).1;
                    if column >= frozen.1 && x < body.min.x {
                        continue;
                    }
                    let width = metrics.column(column).1;
                    let rect = Rect::from_min_size(pos2(x - 3.0, top), vec2(6.0, header_height));
                    let response = ui
                        .interact(
                            rect,
                            ui.id().with(("column-edge", column)),
                            Sense::click_and_drag(),
                        )
                        .on_hover_cursor(egui::CursorIcon::ResizeColumn);
                    let fit = response
                        .double_clicked()
                        .then(|| widest(ui, document, sheet, column, zoom, palette));
                    if let Some(Some(size)) = fit {
                        edge = Some(Edge::Released(Resize {
                            column: true,
                            index: column,
                            size,
                        }));
                    } else if let Some(change) =
                        dragged(&response, resizing, true, column, width / zoom, zoom)
                    {
                        edge = Some(change);
                    }
                }
                for row in (0..frozen.0).chain(rows.clone()) {
                    let y = place.y(metrics, row) + metrics.row(row).1;
                    if row >= frozen.0 && y < body.min.y {
                        continue;
                    }
                    let height = metrics.row(row).1;
                    let rect = Rect::from_min_size(pos2(left, y - 3.0), vec2(header_width, 6.0));
                    let response = ui
                        .interact(rect, ui.id().with(("row-edge", row)), Sense::drag())
                        .on_hover_cursor(egui::CursorIcon::ResizeRow);
                    if let Some(change) =
                        dragged(&response, resizing, false, row, height / zoom, zoom)
                    {
                        edge = Some(change);
                    }
                }

                // A press picks the cell under it, or with Shift stretches the
                // selection to it, and a drag goes on stretching it. The cell
                // is the one under where the button went down: a frame can
                // see the press and the pointer's travel together.
                let cell_at = |at: egui::Pos2| {
                    (at.x > pinned.x && at.y > pinned.y).then(|| {
                        (
                            if at.y < body.min.y {
                                metrics.row_at(at.y - pinned.y)
                            } else {
                                metrics.row_at(at.y - origin.y)
                            },
                            if at.x < body.min.x {
                                metrics.column_at(at.x - pinned.x)
                            } else {
                                metrics.column_at(at.x - origin.x)
                            },
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

        self.page_rows = page_rows;
        match edge {
            Some(Edge::Dragging(resize)) => {
                if let Some(metrics) = &mut self.metrics {
                    if resize.column {
                        metrics.set_column(resize.index, resize.size * zoom);
                    } else {
                        metrics.set_row(resize.index, resize.size * zoom);
                    }
                }
                self.resizing = Some(resize);
            }
            Some(Edge::Released(resize)) => {
                self.resizing = None;
                self.resize(resize, editing);
            }
            None => {}
        }
        let Some(document) = &self.document else {
            return;
        };
        let Some(sheet) = document.sheets().get(self.sheet) else {
            return;
        };
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
        if self.editor.is_some() || editing.asking || self.renaming.is_some() {
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

    /// Write a column's width or a row's height into the document, as one
    /// step to undo.
    fn resize(&mut self, resize: Resize, editing: &mut Editing) {
        let Some(document) = &mut self.document else {
            return;
        };
        let before = document.document.content.clone();
        let size = odox_core::Length(resize.size.max(1.0));
        let done = if resize.column {
            document.set_column_width(self.sheet, resize.index, size)
        } else {
            document.set_row_height(self.sheet, resize.index, size)
        };
        match done {
            Ok(()) => editing.record_snapshot(before, None),
            Err(refused) => self.notice = Some(notice(&refused)),
        }
        self.metrics = None;
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
        self.values_changed = true;
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
        self.values_changed = true;
        match document.set_cell(self.sheet, row, column, value) {
            Ok(()) => self.metrics = None,
            Err(refused) => self.notice = Some(notice(&refused)),
        }
    }

    /// Move the picked cell from the keyboard, which is how a person walks a
    /// sheet without reaching for the pointer. Shift with any move stretches
    /// the selection instead.
    fn arrow_keys(&mut self, ui: &Ui, grid: (usize, usize), used: (usize, usize)) {
        // The arrows go as far as the grid is drawn, empty cells included, so
        // that a person can walk to one to type in it. End is the last column
        // the document wrote.
        let last_row = grid.0.saturating_sub(1);
        let last_column = grid.1.saturating_sub(1);
        let last_used = (used.0.saturating_sub(1), used.1.saturating_sub(1));
        let page = self.page_rows.max(1);
        // Each key's own modifiers rather than the frame's: a press and its
        // release can arrive in one frame, which leaves the frame's modifiers
        // saying nothing was held.
        let pressed = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if MOVES.contains(key) => Some((*key, modifiers.command, modifiers.shift)),
                _ => None,
            })
        });
        let Some((key, jump, extend)) = pressed else {
            return;
        };
        if key == Key::A {
            if jump {
                self.pick((0, 0));
                self.extend(last_used);
                // All of it is selected, and the grid stays where the person is looking.
                self.scroll = Scroll::Stay;
            }
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        let Some(sheet) = document.sheets().get(self.sheet) else {
            return;
        };
        let filled = |row: usize, column: usize| {
            document.cell(sheet, row, column).is_some_and(|cell| {
                !cell.covered && (cell.formula().is_some() || !cell.text().is_empty())
            })
        };
        let (row, column) = self.selected;
        let cell = match (key, jump) {
            (Key::ArrowDown, false) => (row + 1, column),
            (Key::ArrowUp, false) => (row.saturating_sub(1), column),
            (Key::ArrowRight, false) => (row, column + 1),
            (Key::ArrowLeft, false) => (row, column.saturating_sub(1)),
            (Key::ArrowDown, true) => (edge(row, last_row, true, |r| filled(r, column)), column),
            (Key::ArrowUp, true) => (edge(row, last_row, false, |r| filled(r, column)), column),
            (Key::ArrowRight, true) => (row, edge(column, last_column, true, |c| filled(row, c))),
            (Key::ArrowLeft, true) => (row, edge(column, last_column, false, |c| filled(row, c))),
            (Key::PageDown, _) => (row + page, column),
            (Key::PageUp, _) => (row.saturating_sub(page), column),
            (Key::Home, false) => (row, 0),
            (Key::Home, true) => (0, 0),
            (Key::End, false) => (row, last_used.1),
            _ => last_used,
        };
        let cell = (cell.0.min(last_row), cell.1.min(last_column));
        if cell != self.selected {
            if extend {
                self.extend(cell);
            } else {
                self.pick(cell);
            }
        }
        if (key, jump) == (Key::Home, true) {
            self.scroll = Scroll::Start;
        }
    }
}

/// The keys that move the picked cell, with Ctrl and Shift or without.
const MOVES: [Key; 9] = [
    Key::ArrowDown,
    Key::ArrowUp,
    Key::ArrowRight,
    Key::ArrowLeft,
    Key::PageDown,
    Key::PageUp,
    Key::Home,
    Key::End,
    Key::A,
];

/// What a drag on a header's edge did this frame: a size, from the one the
/// column or row had when the drag began and the pointer's travel since.
fn dragged(
    response: &egui::Response,
    resizing: Option<Resize>,
    column: bool,
    index: usize,
    size: f32,
    zoom: f32,
) -> Option<Edge> {
    let ours = resizing.filter(|r| r.column == column && r.index == index);
    let travel = if column {
        response.drag_delta().x
    } else {
        response.drag_delta().y
    };
    let resize = Resize {
        column,
        index,
        size: (ours.map_or(size, |r| r.size) + travel / zoom).max(2.0),
    };
    if response.drag_stopped() {
        Some(Edge::Released(resize))
    } else if response.dragged() {
        Some(Edge::Dragging(resize))
    } else {
        None
    }
}

/// The width a column needs for the widest text in it, in points, padding
/// included: `None` for a column with no text.
fn widest(
    ui: &Ui,
    document: &SheetDocument,
    sheet: &Sheet,
    column: usize,
    zoom: f32,
    palette: format::Palette,
) -> Option<f32> {
    (0..sheet.used_rows)
        .filter_map(|row| {
            let cell = document.cell(sheet, row, column)?;
            if cell.covered || cell.columns_spanned() > 1 {
                return None;
            }
            let text = cell.text();
            if text.is_empty() {
                return None;
            }
            let style = document.cell_style(sheet, Some(&cell), column);
            let format = format::text_format(&style.text, DEFAULT_SIZE, zoom, palette);
            let width = ui.fonts_mut(|fonts| {
                text.lines()
                    .map(|line| {
                        fonts
                            .layout_no_wrap(line.to_owned(), format.font_id.clone(), format.color)
                            .size()
                            .x
                    })
                    .fold(0.0_f32, f32::max)
            });
            Some(width / zoom + 6.0 + 2.0)
        })
        .reduce(f32::max)
}

/// Where Ctrl and an arrow go along one row or column, from `from` towards 0
/// or towards `last`: to the end of the block of filled cells it is in, or,
/// starting at that end or in an empty cell, to the next filled cell, and to
/// the edge when there is none.
fn edge(from: usize, last: usize, forward: bool, filled: impl Fn(usize) -> bool) -> usize {
    let step = |at: usize| {
        if forward {
            (at < last).then_some(at + 1)
        } else {
            at.checked_sub(1)
        }
    };
    let Some(mut at) = step(from) else {
        return from;
    };
    if filled(from) && filled(at) {
        while let Some(next) = step(at).filter(|&next| filled(next)) {
            at = next;
        }
    } else {
        while !filled(at) {
            match step(at) {
                Some(next) => at = next,
                None => break,
            }
        }
    }
    at
}

/// A name for a new sheet that no sheet has: "Sheet" and the first number
/// from the count of sheets on that is free.
fn fresh_name(document: &SheetDocument) -> String {
    // As many numbers as there are sheets, and one more, hold one that is
    // free.
    let count = document.sheets().len();
    (count + 1..=count * 2 + 1)
        .map(|number| fill(t("Sheet{number}"), &[("number", &number.to_string())]))
        .find(|name| document.check_sheet_name(name, None).is_ok())
        .unwrap_or_default()
}

/// What the cell bar says when rows or columns could not be inserted or
/// deleted.
fn blocked_notice(blocked: &Blocked) -> String {
    match blocked {
        Blocked::NotFound => t("There is nothing there to change.").to_owned(),
        Blocked::Merged => t("A merged cell is in the way: this would cut through it.").to_owned(),
        Blocked::Unreadable(reference) => fill(
            t("A formula holds [{reference}], which this version cannot read."),
            &[("reference", reference)],
        ),
        Blocked::Deleted(reference) => fill(
            t("A formula points at [{reference}], which would be deleted."),
            &[("reference", reference)],
        ),
        Blocked::Taken => t("Another sheet already has that name.").to_owned(),
        Blocked::Unnamed => {
            t("A sheet's name cannot be empty or hold [ ] * ? : / or \\.").to_owned()
        }
        Blocked::Last => t("A workbook keeps at least one sheet to show.").to_owned(),
        Blocked::Names(what) => {
            let what = match *what {
                "conditional formats" => t("conditional formats"),
                "validations" => t("validations"),
                "database ranges" => t("database ranges"),
                "pivot tables" => t("pivot tables"),
                "consolidations" => t("consolidations"),
                "charts and objects" => t("charts and objects"),
                "print ranges" => t("print ranges"),
                _ => t("shapes anchored to cells"),
            };
            fill(
                t("This document has {what}, which this version cannot update to match."),
                &[("what", what)],
            )
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

/// Count what a range holds, as far as the sheet has cells: a selection
/// stretched into the empty grid past them holds nothing more.
fn summarize(
    document: &SheetDocument,
    sheet: &Sheet,
    ((top, left), (bottom, right)): Range,
) -> Summary {
    let mut summary = Summary::default();
    for row in top..=bottom.min(sheet.used_rows.saturating_sub(1)) {
        for column in left..=right.min(sheet.used_columns.saturating_sub(1)) {
            let Some(cell) = document.cell(sheet, row, column).filter(|c| !c.covered) else {
                continue;
            };
            match cell.value() {
                Value::Number(n) | Value::Percentage(n) | Value::Currency(n, _) => {
                    summary.sum += n;
                    summary.numbers += 1;
                    summary.filled += 1;
                }
                Value::Empty => {}
                _ => summary.filled += 1,
            }
        }
    }
    summary
}

/// A number as the summary shows it: to six decimal places at most, so that
/// the sum of 0.1 and 0.2 is 0.3, with the reader's decimal separator.
fn shown(number: f64) -> String {
    let text = format!("{number:.6}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    let text = if text == "-0" { "0" } else { text };
    match text.split_once('.') {
        Some((whole, fraction)) => fill(
            t("{whole}.{fraction}"),
            &[("whole", whole), ("fraction", fraction)],
        ),
        None => text.to_owned(),
    }
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
    fn ctrl_and_an_arrow_go_to_the_edge_of_the_data() {
        let (mut view, _) = opened();
        view.pick((0, 1));
        pressed(&mut view, Key::ArrowDown, egui::Modifiers::COMMAND);
        assert_eq!(view.selected, (4, 1), "the end of the filled column");
        pressed(&mut view, Key::ArrowRight, egui::Modifiers::COMMAND);
        assert_eq!(
            view.selected,
            (4, 3),
            "over the empty cell to the next filled one"
        );
        pressed(&mut view, Key::ArrowDown, egui::Modifiers::COMMAND);
        assert_eq!(
            view.selected,
            (44, 3),
            "nothing below: the edge of the grid"
        );
        pressed(&mut view, Key::ArrowUp, egui::Modifiers::COMMAND);
        assert_eq!(view.selected, (4, 3), "back to the last filled cell");
    }

    #[test]
    fn ctrl_home_and_ctrl_end_go_to_the_corners_of_the_data() {
        let (mut view, _) = opened();
        view.pick((2, 2));
        pressed(&mut view, Key::End, egui::Modifiers::COMMAND);
        assert_eq!(view.selected, (4, 6));
        pressed(
            &mut view,
            Key::Home,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        );
        assert_eq!(view.range(), ((0, 0), (4, 6)));
    }

    #[test]
    fn ctrl_a_selects_what_the_sheet_holds() {
        let (mut view, _) = opened();
        view.pick((2, 2));
        pressed(&mut view, Key::A, egui::Modifiers::COMMAND);
        assert_eq!(view.range(), ((0, 0), (4, 6)));
        pressed(&mut view, Key::A, egui::Modifiers::NONE);
        assert_eq!(
            view.range(),
            ((0, 0), (4, 6)),
            "a plain A is typing, not a move"
        );
    }

    #[test]
    fn page_down_and_up_move_by_what_the_grid_showed() {
        let (mut view, _) = opened();
        view.page_rows = 20;
        view.pick((3, 1));
        pressed(&mut view, Key::PageDown, egui::Modifiers::NONE);
        assert_eq!(view.selected, (23, 1));
        pressed(&mut view, Key::PageDown, egui::Modifiers::NONE);
        assert_eq!(view.selected, (43, 1));
        pressed(&mut view, Key::PageDown, egui::Modifiers::NONE);
        assert_eq!(view.selected, (44, 1), "no further than the grid");
        pressed(&mut view, Key::PageUp, egui::Modifiers::SHIFT);
        assert_eq!(view.range(), ((24, 1), (44, 1)));
    }

    #[test]
    fn the_summary_counts_numbers_and_every_filled_cell() {
        let (view, _) = opened();
        let document = view.document.as_ref().expect("open");
        let sheet = &document.sheets()[0];
        let summary = summarize(document, sheet, ((0, 1), (40, 1)));
        assert_eq!(summary.numbers, 4, "12, 300, 88 and 400");
        assert_eq!(summary.filled, 5, "and the heading");
        assert!((summary.sum - 800.0).abs() < 1e-9);
    }

    #[test]
    fn a_shown_number_drops_the_noise_of_binary_fractions() {
        assert_eq!(shown(0.1 + 0.2), "0.3");
        assert_eq!(shown(136.4), "136.4");
        assert_eq!(shown(800.0), "800");
        assert_eq!(shown(-0.000_000_1), "0");
        assert_eq!(shown(76.826_428_571_4), "76.826429");
        assert_eq!(shown(-2.5), "-2.5");
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
    fn rows_inserted_above_the_selection_are_selected_and_undo_takes_them_back() {
        let (mut view, mut editing) = opened();
        view.pick((1, 0));
        view.extend((2, 1));
        view.change_sheet(SheetCommand::RowAbove, &mut editing);
        assert_eq!(text_at(&view, 1, 0), "");
        assert_eq!(
            text_at(&view, 3, 0),
            "Bolt M6",
            "the rows moved down by the two inserted"
        );
        assert_eq!(
            view.range(),
            ((1, 0), (2, 1)),
            "the new rows are what is selected"
        );
        let document = view.document.as_mut().expect("open");
        let (previous, _) = editing
            .undo(&document.document.content, None)
            .expect("one step");
        document.document.content = previous;
        document.reindex();
        assert_eq!(text_at(&view, 1, 0), "Bolt M6");
        assert!(!editing.can_undo());
    }

    #[test]
    fn what_is_in_the_way_is_said_and_nothing_changes() {
        let (mut view, mut editing) = opened();
        view.pick((0, 1));
        view.change_sheet(SheetCommand::DeleteColumns, &mut editing);
        let notice = view.notice.clone().expect("a notice");
        assert!(
            notice.contains("[.B2]") || notice.contains("[."),
            "{notice}"
        );
        assert!(!editing.modified());
        assert_eq!(text_at(&view, 1, 1), "12");
    }

    #[test]
    fn the_selection_is_the_rectangle_between_its_corners() {
        let (mut view, _) = opened();
        view.pick((4, 3));
        view.extend((1, 5));
        assert_eq!(view.range(), ((1, 3), (4, 5)));
    }
}
