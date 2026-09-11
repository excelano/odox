//! A spreadsheet: `.ods`.
//!
//! The body is indexed rather than flattened. ODF writes a run of identical rows
//! or cells once with a repeat count, and a sheet whose last column says
//! `table:number-columns-repeated="16384"` is ordinary: expanding that into cells
//! would turn a small file into a large allocation, and every office application
//! writes one. So a sheet keeps the rows it was given, each with the range of row
//! numbers it stands for, and a lookup is a search through those ranges.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use super::Document;
use crate::media_type;
use crate::value::Length;
use crate::xml::{Element, Ns};
use crate::{Error, Family, Properties};

/// An `OpenDocument` spreadsheet.
pub struct SheetDocument {
    /// The package and everything shared with the other two formats.
    pub document: Document,
    sheets: Vec<Sheet>,
}

/// One sheet, indexed for lookup by row and column.
pub struct Sheet {
    /// The sheet's name, as its tab shows it.
    pub name: String,
    /// The columns that carry a width or a default cell style, expanded from the
    /// column elements' repeat counts and truncated at the last column any cell
    /// reaches. A column past the end of this list is the default width.
    pub columns: Vec<Column>,
    /// The number of rows that carry anything. The grid may show more than this
    /// and a document ends here.
    pub used_rows: usize,
    /// The number of columns that carry anything.
    pub used_columns: usize,
    /// Whether the sheet is hidden, from `table:display`.
    pub visible: bool,
    rows: Vec<RowRange>,
    /// Where the `table:table` element sits among the body's children, so that
    /// the element can be reached again without holding a reference to it.
    table: usize,
}

/// A column's own properties.
pub struct Column {
    /// Its width, where the column style gives one.
    pub width: Option<Length>,
    /// The cell style every cell in the column takes unless it names its own.
    pub default_cell_style: Option<String>,
    /// Whether the column is shown.
    pub visible: bool,
}

/// A run of rows the document wrote once.
struct RowRange {
    /// The first row number this run covers, counting from zero.
    first: usize,
    /// How many rows it covers.
    count: usize,
    /// Where the `table:table-row` element is, relative to the `table:table`.
    path: RowPath,
}

/// Where a row element sits under its table.
///
/// Rows are usually the table's own children, and a sheet with a frozen header
/// or a collapsible outline nests them one or more levels deeper inside
/// `table:table-header-rows` and `table:table-row-group`. The common case costs
/// no allocation and the nested case is a path of child indices, which is what
/// lets a row be reached again without holding a reference into the tree.
enum RowPath {
    /// A child of the table.
    Direct(usize),
    /// A child of a child of the table, to any depth.
    Nested(Box<[usize]>),
}

/// What a cell holds, as ODF types it.
///
/// The type is the cell's own declaration and is independent of how it is shown:
/// a date is a date whatever format it is displayed in, which is what makes a
/// spreadsheet sortable.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Nothing. A cell that exists to carry a style, or to be spanned over.
    Empty,
    /// A number.
    Number(f64),
    /// A number that means a proportion: `0.15` shown as `15%`.
    Percentage(f64),
    /// An amount of money, with the currency code where the cell names one.
    Currency(f64, Option<String>),
    /// A date, as written: an ISO 8601 date or date and time.
    Date(String),
    /// A duration, as written: an ISO 8601 duration.
    Time(String),
    /// True or false.
    Boolean(bool),
    /// Text.
    Text(String),
}

impl Value {
    /// Whether the value is one a spreadsheet puts against the right edge of its
    /// cell: every type but text, which is ODF's own rule and every
    /// application's default.
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Self::Number(_)
                | Self::Percentage(_)
                | Self::Currency(..)
                | Self::Date(_)
                | Self::Time(_)
        )
    }
}

/// One cell, as found in the tree.
pub struct Cell<'a> {
    /// The `table:table-cell` element, so that anything not modelled here is
    /// still reachable.
    pub element: &'a Element,
    /// True when this is a `table:covered-table-cell`: a cell hidden underneath
    /// a neighbour's span. It is kept rather than skipped because the grid has to
    /// know not to draw a border there.
    pub covered: bool,
}

impl Cell<'_> {
    /// The cell's typed value.
    pub fn value(&self) -> Value {
        let e = self.element;
        match e.attr(&Ns::Office, "value-type") {
            Some("float") => e
                .attr(&Ns::Office, "value")
                .and_then(|v| v.parse().ok())
                .map_or(Value::Empty, Value::Number),
            Some("percentage") => e
                .attr(&Ns::Office, "value")
                .and_then(|v| v.parse().ok())
                .map_or(Value::Empty, Value::Percentage),
            Some("currency") => e
                .attr(&Ns::Office, "value")
                .and_then(|v| v.parse().ok())
                .map_or(Value::Empty, |amount| {
                    Value::Currency(
                        amount,
                        e.attr(&Ns::Office, "currency").map(ToOwned::to_owned),
                    )
                }),
            Some("date") => e
                .attr(&Ns::Office, "date-value")
                .map_or(Value::Empty, |v| Value::Date(v.to_owned())),
            Some("time") => e
                .attr(&Ns::Office, "time-value")
                .map_or(Value::Empty, |v| Value::Time(v.to_owned())),
            Some("boolean") => e
                .attr(&Ns::Office, "boolean-value")
                .and_then(crate::value::boolean)
                .map_or(Value::Empty, Value::Boolean),
            // A string cell carries its text in its paragraphs, and
            // `office:string-value` only where the producer chose to write it
            // there as well.
            Some("string") => match e.attr(&Ns::Office, "string-value") {
                Some(text) => Value::Text(text.to_owned()),
                None => Value::Text(self.text()),
            },
            _ => {
                let text = self.text();
                if text.is_empty() {
                    Value::Empty
                } else {
                    Value::Text(text)
                }
            }
        }
    }

    /// What the cell shows: the text the producing application formatted and
    /// stored in the cell's paragraphs.
    ///
    /// This is why a viewer needs neither a number-format engine nor a formula
    /// evaluator. ODF requires a cell to carry both its typed value and the text
    /// of that value as the document was last displayed, so the formatted string
    /// — thousands separators, currency symbol, date order, decimal places — is
    /// already in the file. A cell edited here would have to be re-formatted
    /// from its data style, which is the work an editor adds and a viewer does
    /// not.
    ///
    /// A cell with more than one paragraph gives them separated by newlines.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for paragraph in self.element.elements() {
            if paragraph.is(&Ns::Text, "p") {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&paragraph.plain_text());
            }
        }
        out
    }

    /// The formula, without the namespace prefix ODF writes in front of it.
    ///
    /// A formula is written `of:=SUM([.A1:.A9])`, where the part before the
    /// colon says which formula language it is in. The prefix is dropped here
    /// and the expression given as it stands: nothing in this release evaluates
    /// one, and a formula bar shows what the document says.
    pub fn formula(&self) -> Option<&str> {
        let formula = self.element.attr(&Ns::Table, "formula")?;
        Some(match formula.split_once(":=") {
            Some((_, expression)) => expression,
            None => formula,
        })
    }

    /// The cell style the cell names, if it names one.
    pub fn style_name(&self) -> Option<&str> {
        self.element.attr(&Ns::Table, "style-name")
    }

    /// How many columns the cell spans, which is one unless it says otherwise.
    pub fn columns_spanned(&self) -> usize {
        self.element
            .attr_usize(&Ns::Table, "number-columns-spanned")
            .unwrap_or(1)
            .max(1)
    }

    /// How many rows the cell spans.
    pub fn rows_spanned(&self) -> usize {
        self.element
            .attr_usize(&Ns::Table, "number-rows-spanned")
            .unwrap_or(1)
            .max(1)
    }

    /// Whether the cell has anything in it: a value, text, or a formula. A cell
    /// that carries only a style is empty.
    fn occupied(&self) -> bool {
        self.element.attr(&Ns::Office, "value-type").is_some()
            || self.element.attr(&Ns::Table, "formula").is_some()
            || self.element.elements().any(|e| e.is(&Ns::Text, "p"))
    }
}

impl SheetDocument {
    /// Read a `.ods` package.
    ///
    /// # Errors
    ///
    /// The bytes are not a spreadsheet, or its `content.xml` cannot be read.
    pub fn read(bytes: &[u8]) -> Result<Self, Error> {
        let document = Document::read(bytes, media_type::SPREADSHEET_ANY)?;
        let sheets = index_sheets(&document);
        Ok(Self { document, sheets })
    }

    /// The sheets, in the order the document holds them.
    pub fn sheets(&self) -> &[Sheet] {
        &self.sheets
    }

    /// The `table:table` element of a sheet.
    fn table(&self, sheet: &Sheet) -> Option<&Element> {
        let body = self.document.body_of("spreadsheet")?;
        body.children.get(sheet.table).and_then(|node| match node {
            crate::xml::Node::Element(e) => Some(e),
            _ => None,
        })
    }

    /// One cell, by sheet, row and column, all counting from zero.
    ///
    /// `None` for a cell the document never wrote, which is the usual answer
    /// past the edge of the used range and means an empty cell rather than an
    /// error.
    pub fn cell(&self, sheet: &Sheet, row: usize, column: usize) -> Option<Cell<'_>> {
        cell_in_row(self.row_element(sheet, row)?, column)
    }

    /// The `table:table-row` element a row number falls in.
    pub fn row_element(&self, sheet: &Sheet, row: usize) -> Option<&Element> {
        let table = self.table(sheet)?;
        let range = sheet.row_range(row)?;
        let mut element = table;
        for step in range.path.steps() {
            let crate::xml::Node::Element(child) = element.children.get(*step)? else {
                return None;
            };
            element = child;
        }
        Some(element)
    }

    /// A row's height, where its style gives one.
    pub fn row_height(&self, sheet: &Sheet, row: usize) -> Option<Length> {
        let name = self
            .row_element(sheet, row)?
            .attr(&Ns::Table, "style-name")?;
        self.document
            .styles
            .resolve(&Family::TableRow, name)
            .row_height
    }

    /// The resolved style of a cell: the style it names, or the one its column
    /// gives every cell that names none.
    pub fn cell_style(
        &self,
        sheet: &Sheet,
        cell: Option<&Cell<'_>>,
        column: usize,
    ) -> std::rc::Rc<Properties> {
        let named = cell.and_then(Cell::style_name);
        let from_column = sheet
            .columns
            .get(column)
            .and_then(|c| c.default_cell_style.as_deref());
        let name = named.or(from_column).unwrap_or("Default");
        self.document.styles.resolve(&Family::TableCell, name)
    }
}

impl Sheet {
    /// The run of rows a row number falls in.
    fn row_range(&self, row: usize) -> Option<&RowRange> {
        let found = self
            .rows
            .binary_search_by(|range| {
                if row < range.first {
                    std::cmp::Ordering::Greater
                } else if row >= range.first + range.count {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()?;
        self.rows.get(found)
    }

    /// The width of a column, where its column style gives one.
    pub fn column_width(&self, column: usize) -> Option<Length> {
        self.columns.get(column).and_then(|c| c.width)
    }
}

/// Find a cell by column number inside a row, stepping over repeat counts.
fn cell_in_row(row: &Element, column: usize) -> Option<Cell<'_>> {
    let mut at = 0usize;
    for child in row.elements() {
        let covered = child.is(&Ns::Table, "covered-table-cell");
        if !covered && !child.is(&Ns::Table, "table-cell") {
            continue;
        }
        let repeat = child
            .attr_usize(&Ns::Table, "number-columns-repeated")
            .unwrap_or(1)
            .max(1);
        if column < at + repeat {
            return Some(Cell {
                element: child,
                covered,
            });
        }
        at += repeat;
    }
    None
}

/// Build the row and column index of every sheet in the document.
fn index_sheets(document: &Document) -> Vec<Sheet> {
    let Some(body) = document.body_of("spreadsheet") else {
        return Vec::new();
    };
    let mut sheets = Vec::new();
    for (position, node) in body.children.iter().enumerate() {
        let crate::xml::Node::Element(table) = node else {
            continue;
        };
        if !table.is(&Ns::Table, "table") {
            continue;
        }
        sheets.push(index_sheet(document, table, position));
    }
    sheets
}

fn index_sheet(document: &Document, table: &Element, position: usize) -> Sheet {
    let mut index = Index {
        document,
        rows: Vec::new(),
        columns: Vec::new(),
        at_row: 0,
        used_rows: 0,
        used_columns: 0,
    };
    index.walk(table, &mut Vec::new());

    Sheet {
        name: table
            .attr(&Ns::Table, "name")
            .unwrap_or_default()
            .to_owned(),
        columns: index.columns,
        used_rows: index.used_rows,
        used_columns: index.used_columns,
        visible: table.attr(&Ns::Table, "display").unwrap_or("true") != "false",
        rows: index.rows,
        table: position,
    }
}

/// The state of one sheet's indexing pass.
struct Index<'a> {
    document: &'a Document,
    rows: Vec<RowRange>,
    columns: Vec<Column>,
    at_row: usize,
    used_rows: usize,
    used_columns: usize,
}

impl Index<'_> {
    /// Collect the rows and columns under an element, descending through the
    /// containers that hold them.
    ///
    /// `path` is the route from the table to whatever is being walked, and is the
    /// route a lookup will take back.
    fn walk(&mut self, parent: &Element, path: &mut Vec<usize>) {
        for (child_index, child) in parent.children.iter().enumerate() {
            let crate::xml::Node::Element(element) = child else {
                continue;
            };

            if element.is(&Ns::Table, "table-column") {
                self.column(element);
            } else if element.is(&Ns::Table, "table-row") {
                path.push(child_index);
                self.row(element, path);
                path.pop();
            } else if is_row_container(element) || is_column_container(element) {
                // A header band or an outline group holds rows and columns that
                // belong to the sheet as if they were the table's own. The
                // grouping itself is what a view would draw a collapse handle
                // for, and the tree still carries it.
                path.push(child_index);
                self.walk(element, path);
                path.pop();
            }
        }
    }

    fn column(&mut self, element: &Element) {
        let repeat = element
            .attr_usize(&Ns::Table, "number-columns-repeated")
            .unwrap_or(1)
            .max(1);
        let width = element
            .attr(&Ns::Table, "style-name")
            .map(|name| self.document.styles.resolve(&Family::TableColumn, name))
            .and_then(|p| p.column_width);
        let default_cell_style = element
            .attr(&Ns::Table, "default-cell-style-name")
            .map(ToOwned::to_owned);
        let visible = element.attr(&Ns::Table, "visibility").unwrap_or("visible") == "visible";
        // A trailing column run covering the whole sheet is ordinary, and
        // expanding it is what this index exists to avoid. The run is kept only
        // as far as ODF permits a column to exist; past that the document is
        // saying *the rest of the sheet* and the last entry answers for all of it.
        let keep = repeat.min(MAX_COLUMNS.saturating_sub(self.columns.len()));
        for _ in 0..keep {
            self.columns.push(Column {
                width,
                default_cell_style: default_cell_style.clone(),
                visible,
            });
        }
    }

    fn row(&mut self, element: &Element, path: &[usize]) {
        let repeat = element
            .attr_usize(&Ns::Table, "number-rows-repeated")
            .unwrap_or(1)
            .max(1);
        if let Some(last) = last_occupied_column(element) {
            self.used_rows = self.at_row + repeat;
            self.used_columns = self.used_columns.max(last + 1);
        }
        self.rows.push(RowRange {
            first: self.at_row,
            count: repeat,
            path: RowPath::of(path),
        });
        self.at_row += repeat;
    }
}

impl RowPath {
    /// The route to a row, taking the cheap form where it is a child of the
    /// table, which is what a sheet with no grouping gives for every row.
    fn of(path: &[usize]) -> Self {
        match path {
            [only] => Self::Direct(*only),
            nested => Self::Nested(nested.into()),
        }
    }

    /// The child indices to follow, from the table down to the row.
    fn steps(&self) -> &[usize] {
        match self {
            Self::Direct(only) => std::slice::from_ref(only),
            Self::Nested(path) => path,
        }
    }
}

/// Whether an element holds rows on the sheet's behalf.
fn is_row_container(element: &Element) -> bool {
    element.is(&Ns::Table, "table-rows")
        || element.is(&Ns::Table, "table-header-rows")
        || element.is(&Ns::Table, "table-row-group")
}

/// Whether an element holds columns on the sheet's behalf.
fn is_column_container(element: &Element) -> bool {
    element.is(&Ns::Table, "table-columns")
        || element.is(&Ns::Table, "table-header-columns")
        || element.is(&Ns::Table, "table-column-group")
}

/// ODF's own column limit, and the point past which a repeat count is a way of
/// saying *the rest of the sheet*.
const MAX_COLUMNS: usize = 16_384;

/// The last column in a row that carries anything, or `None` for an empty row.
fn last_occupied_column(row: &Element) -> Option<usize> {
    let mut at = 0usize;
    let mut last = None;
    for child in row.elements() {
        let covered = child.is(&Ns::Table, "covered-table-cell");
        if !covered && !child.is(&Ns::Table, "table-cell") {
            continue;
        }
        let repeat = child
            .attr_usize(&Ns::Table, "number-columns-repeated")
            .unwrap_or(1)
            .max(1);
        let cell = Cell {
            element: child,
            covered,
        };
        if cell.occupied() {
            last = Some(at + repeat - 1);
        }
        at += repeat;
    }
    last
}
