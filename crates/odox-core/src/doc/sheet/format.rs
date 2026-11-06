//! Formatting a block of cells: its font, colours, alignment, borders and
//! number format. DESIGN.md §11.
//!
//! A cell's formatting is its cell style. A cell given a format takes an
//! automatic style in `content.xml` that says what its style said with the
//! change written over it: a copy of its own automatic style, or one that
//! inherits from the named style it had. One the document already holds is
//! used again where it says the same thing; otherwise one is written, named
//! `ce` and the first number no cell style has, as `LibreOffice` names them.
//! A number format is a data style written the same way, named `N` and a
//! number, in the shape `LibreOffice` writes, in the document's language.
//!
//! A value takes the kind its new format shows: an amount given a percentage
//! format is a percentage, a number given a date format is that day, as a
//! spreadsheet changes it, and its text is written again in the new format.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::HashMap;

use super::fill::Block;
use super::recalc::{date_serial, duration, duration_days, iso_date};
use super::{SheetDocument, Value, Writing};
use crate::Color;
use crate::data_style::{DataStyle, Kind};
use crate::edit::Refused;
use crate::edit::format::{automatic_styles, element, unnamed};
use crate::style::{Family, TextAlign, VerticalAlign};
use crate::xml::{Attribute, Element, Name, Node, Ns};

/// What a format changes; what it leaves as `None` stays as each cell has it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CellFormat {
    /// Bold or not.
    pub bold: Option<bool>,
    /// Italic or not.
    pub italic: Option<bool>,
    /// Underlined or not.
    pub underline: Option<bool>,
    /// The text's colour, or with `Some(None)` the automatic one.
    pub color: Option<Option<Color>>,
    /// The cell's fill, or with `Some(None)` none.
    pub background: Option<Option<Color>>,
    /// How the text sits across the cell, or with `Some(None)` as its value
    /// says: numbers right, text left.
    pub align: Option<Option<TextAlign>>,
    /// How the text sits down the cell.
    pub vertical: Option<VerticalAlign>,
    /// Whether the text wraps inside the cell.
    pub wrap: Option<bool>,
    /// The borders.
    pub borders: Option<Borders>,
    /// The number format.
    pub number: Option<NumberFormat>,
}

/// Borders given to a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Borders {
    /// None on any cell of the block.
    None,
    /// A line around the block, and its inside left as it is.
    Outline,
    /// A line around every cell of the block.
    All,
}

/// A number format a cell is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberFormat {
    /// The format its cell style inherits, which for most cells is none.
    General,
    /// A number with this many decimals, in groups of thousands or not.
    Number {
        /// Decimal places.
        decimals: u8,
        /// Whether thousands are grouped.
        grouping: bool,
    },
    /// A percentage with this many decimals.
    Percent {
        /// Decimal places.
        decimals: u8,
    },
    /// An amount in the currency of the document's country.
    Currency {
        /// Decimal places.
        decimals: u8,
    },
    /// A day, in the order the document's country writes one.
    Date,
    /// A time of day, in hours and minutes.
    Time,
}

/// The line a border is drawn with, `LibreOffice`'s thin one.
const LINE: &str = "0.74pt solid #000000";

impl SheetDocument {
    /// Give a block of cells a format. A block larger than a hundred thousand
    /// cells is cut to the sheet's used cells, so that a whole column or row
    /// picked does not write a million cells.
    ///
    /// # Errors
    ///
    /// The sheet does not exist, or the document cannot declare the namespace
    /// a number format is written in. Nothing is changed in either case.
    pub fn format_cells(
        &mut self,
        sheet: usize,
        block: Block,
        format: &CellFormat,
    ) -> Result<(), Refused> {
        let index = self.sheets.get(sheet).ok_or(Refused::NotFound)?;
        let mut block = block;
        if (block.bottom - block.top + 1) * (block.right - block.left + 1) > 100_000 {
            block.bottom = block.bottom.min(index.used_rows.max(block.top + 1) - 1);
            block.right = block.right.min(index.used_columns.max(block.left + 1) - 1);
        }
        if format.number.is_some_and(|n| n != NumberFormat::General)
            && !self.document.content.declare(&Ns::Number)
        {
            return Err(Refused::Namespace);
        }
        let data_style = match format.number {
            Some(NumberFormat::General) | None => None,
            Some(number) => Some(self.data_style_named(number)),
        };
        let mut made: HashMap<(Option<String>, [bool; 4]), String> = HashMap::new();
        for row in block.top..=block.bottom {
            for column in block.left..=block.right {
                let index = &self.sheets[sheet];
                let cell = self.cell(index, row, column);
                if cell.as_ref().is_some_and(|c| c.covered) {
                    continue;
                }
                let had = cell
                    .as_ref()
                    .and_then(|c| c.style_name())
                    .or_else(|| {
                        index
                            .columns
                            .get(column)
                            .and_then(|c| c.default_cell_style.as_deref())
                    })
                    .map(ToOwned::to_owned);
                let edges = [
                    column == block.left,
                    column == block.right,
                    row == block.top,
                    row == block.bottom,
                ];
                let name = if let Some(name) = made.get(&(had.clone(), edges)) {
                    name.clone()
                } else {
                    let style =
                        self.formatted(had.as_deref(), format, edges, data_style.as_deref());
                    let name = self.cell_style_named(style);
                    made.insert((had, edges), name.clone());
                    name
                };
                let style_name = self.document.name(&Ns::Table, "style-name");
                self.cell_element(sheet, row, column)?
                    .set_attr(style_name, name);
                self.reindex();
                if format.number.is_some() {
                    self.retype(sheet, row, column)?;
                }
            }
        }
        Ok(())
    }

    /// A cell style that says what `had` says, with the format written over
    /// it; `edges` are which of the block's left, right, top and bottom edges
    /// the cell lies on.
    fn formatted(
        &self,
        had: Option<&str>,
        format: &CellFormat,
        edges: [bool; 4],
        data_style: Option<&str>,
    ) -> Element {
        let root = &self.document.content;
        let own = had.and_then(|name| {
            root.child(&Ns::Office, "automatic-styles")?
                .elements()
                .find(|e| {
                    e.is(&Ns::Style, "style")
                        && e.attr(&Ns::Style, "family") == Some("table-cell")
                        && e.attr(&Ns::Style, "name") == Some(name)
                })
                .cloned()
        });
        let mut style = own.unwrap_or_else(|| {
            let mut style = element(root.name_for(&Ns::Style, "style"));
            style.set_attr(root.name_for(&Ns::Style, "family"), "table-cell");
            let parent = had.or_else(|| {
                self.document
                    .styles
                    .style(&Family::TableCell, "Default")
                    .map(|_| "Default")
            });
            if let Some(parent) = parent {
                style.set_attr(root.name_for(&Ns::Style, "parent-style-name"), parent);
            }
            style
        });
        let text = properties(&mut style, root, "text-properties");
        write_text(root, text, format);
        let paragraph = properties(&mut style, root, "paragraph-properties");
        if let Some(align) = format.align {
            let written = align.map(|align| match align {
                TextAlign::Start => "start",
                TextAlign::End => "end",
                TextAlign::Center => "center",
                TextAlign::Justify => "justify",
            });
            set(root, paragraph, &Ns::Fo, "text-align", written);
            set(
                root,
                paragraph,
                &Ns::Fo,
                "margin-left",
                written.map(|_| "0cm"),
            );
        }
        let cell = properties(&mut style, root, "table-cell-properties");
        write_cell(root, cell, format, edges);
        if let Some(number) = format.number {
            let written = (number != NumberFormat::General)
                .then_some(data_style)
                .flatten();
            set(root, &mut style, &Ns::Style, "data-style-name", written);
        }
        for local in [
            "table-cell-properties",
            "paragraph-properties",
            "text-properties",
        ] {
            style.children.retain(|n| {
                !matches!(n, Node::Element(e) if e.is(&Ns::Style, local) && e.attrs.is_empty() && e.children.is_empty())
            });
        }
        style.self_closing = style.children.is_empty();
        style
    }

    /// The name of an automatic cell style that says what this one does,
    /// written into the document where none is there yet.
    fn cell_style_named(&mut self, style: Element) -> String {
        let taken = |document: &Self, n: &str| {
            document
                .document
                .styles
                .style(&Family::TableCell, n)
                .is_some()
        };
        self.minted(
            style,
            "ce",
            &|e: &Element| {
                e.is(&Ns::Style, "style") && e.attr(&Ns::Style, "family") == Some("table-cell")
            },
            &taken,
        )
    }

    /// The name of a data style for a number format, written into the
    /// document where none like it is there yet.
    fn data_style_named(&mut self, number: NumberFormat) -> String {
        let (language, country) = self.language();
        let root = &self.document.content;
        let style = data_style(root, number, &language, &country);
        let taken =
            |document: &Self, n: &str| document.document.styles.data_styles().get(n).is_some();
        self.minted(style, "N", &|e: &Element| e.name.ns == Ns::Number, &taken)
    }

    /// The name of an automatic style that says what `style` does, among the
    /// automatic styles `kind` picks out, or the name it is written under,
    /// `prefix` and the first number `taken` does not answer for.
    fn minted(
        &mut self,
        mut style: Element,
        prefix: &str,
        kind: &dyn Fn(&Element) -> bool,
        taken: &dyn Fn(&Self, &str) -> bool,
    ) -> String {
        let wanted = unnamed(&style);
        let container = automatic_styles(&mut self.document.content);
        let found = container
            .elements()
            .filter(|e| kind(e))
            .find(|e| unnamed(e) == wanted)
            .and_then(|e| e.attr(&Ns::Style, "name"))
            .map(ToOwned::to_owned);
        if let Some(name) = found {
            return name;
        }
        let in_tree = |document: &Self, n: &str| {
            document
                .document
                .content
                .child(&Ns::Office, "automatic-styles")
                .is_some_and(|c| c.elements().any(|e| e.attr(&Ns::Style, "name") == Some(n)))
        };
        let mut number = 1;
        while taken(self, &format!("{prefix}{number}"))
            || in_tree(self, &format!("{prefix}{number}"))
        {
            number += 1;
        }
        let name = format!("{prefix}{number}");
        style.remove_attr(&Ns::Style, "name");
        style.attrs.insert(
            0,
            Attribute {
                name: self.document.content.name_for(&Ns::Style, "name"),
                value: name.clone(),
            },
        );
        let container = automatic_styles(&mut self.document.content);
        container.children.push(Node::Element(style.clone()));
        container.self_closing = false;
        self.document.styles.add(&style);
        name
    }

    /// The language and country the document's cells are written in, from
    /// the default cell style, or the United States' English.
    fn language(&self) -> (String, String) {
        let said = |e: &Element| {
            let text = e.child(&Ns::Style, "text-properties")?;
            Some((
                text.attr(&Ns::Fo, "language")?.to_owned(),
                text.attr(&Ns::Fo, "country").unwrap_or_default().to_owned(),
            ))
        };
        let defaults = [
            self.document.styles_part.as_ref(),
            Some(&self.document.content),
        ]
        .into_iter()
        .flatten()
        .filter_map(|root| root.child(&Ns::Office, "styles"))
        .flat_map(Element::elements)
        .filter(|e| {
            e.is(&Ns::Style, "default-style") && e.attr(&Ns::Style, "family") == Some("table-cell")
        })
        .find_map(said);
        defaults
            .or_else(|| {
                let named = self.document.styles.style(&Family::TableCell, "Default")?;
                said(&named.element)
            })
            .unwrap_or_else(|| ("en".to_owned(), "US".to_owned()))
    }

    /// Write a cell's value again as the kind its number format shows, in
    /// that format.
    fn retype(&mut self, sheet: usize, row: usize, column: usize) -> Result<(), Refused> {
        let index = &self.sheets[sheet];
        let Some(cell) = self.cell(index, row, column) else {
            return Ok(());
        };
        let (value, formula) = (cell.value(), cell.formula().is_some());
        let kind = self.data_style(sheet, row, column).map(|d| d.kind);
        let Some(number) = figure(&value) else {
            return Ok(());
        };
        let retyped = match kind {
            Some(Kind::Percentage) => Value::Percentage(number),
            Some(Kind::Currency) => Value::Currency(number, None),
            Some(Kind::Date) => Value::Date(iso_date(number)),
            Some(Kind::Time) => Value::Time(duration(number)),
            _ => match value {
                Value::Date(_) | Value::Time(_) | Value::Percentage(_) | Value::Currency(..) => {
                    Value::Number(number)
                }
                other => other,
            },
        };
        let retyped = match retyped {
            Value::Currency(n, None) => Value::Currency(
                n,
                self.data_style(sheet, row, column)
                    .and_then(DataStyle::currency_code),
            ),
            other => other,
        };
        let index = &self.sheets[sheet];
        let text = self
            .data_style_name(index, row, column)
            .and_then(|name| self.document.styles.data_styles().format(name, &retyped))
            .unwrap_or_else(|| retyped.cached_text());
        let writing = if formula {
            Writing::Result
        } else {
            Writing::Value
        };
        self.write_cell(sheet, row, column, &retyped, &text, writing)?;
        self.reindex();
        Ok(())
    }
}

/// Set or take off one property of a style.
fn set(root: &Element, properties: &mut Element, ns: &Ns, local: &str, value: Option<&str>) {
    properties.remove_attr(ns, local);
    if let Some(value) = value {
        properties.set_attr(root.name_for(ns, local), value);
    }
}

/// Write a format's font into a style's text properties.
fn write_text(root: &Element, text: &mut Element, format: &CellFormat) {
    if let Some(bold) = format.bold {
        let weight = if bold { "bold" } else { "normal" };
        for (ns, local) in [
            (Ns::Fo, "font-weight"),
            (Ns::Style, "font-weight-asian"),
            (Ns::Style, "font-weight-complex"),
        ] {
            set(root, text, &ns, local, Some(weight));
        }
    }
    if let Some(italic) = format.italic {
        let slant = if italic { "italic" } else { "normal" };
        for (ns, local) in [
            (Ns::Fo, "font-style"),
            (Ns::Style, "font-style-asian"),
            (Ns::Style, "font-style-complex"),
        ] {
            set(root, text, &ns, local, Some(slant));
        }
    }
    if let Some(underline) = format.underline {
        let (kind, width, colour) = if underline {
            (Some("solid"), Some("auto"), Some("font-color"))
        } else {
            (Some("none"), None, None)
        };
        set(root, text, &Ns::Style, "text-underline-style", kind);
        set(root, text, &Ns::Style, "text-underline-width", width);
        set(root, text, &Ns::Style, "text-underline-color", colour);
    }
    if let Some(color) = format.color {
        set(root, text, &Ns::Fo, "color", color.map(hex).as_deref());
    }
}

/// Write a format's fill, alignment and borders into a style's cell
/// properties; `edges` are which of the block's edges the cell lies on.
fn write_cell(root: &Element, cell: &mut Element, format: &CellFormat, edges: [bool; 4]) {
    if let Some(align) = format.align {
        let source = if align.is_some() { "fix" } else { "value-type" };
        set(root, cell, &Ns::Style, "text-align-source", Some(source));
    }
    if let Some(fill) = format.background {
        let written = fill.map_or_else(|| "transparent".to_owned(), hex);
        set(root, cell, &Ns::Fo, "background-color", Some(&written));
    }
    if let Some(vertical) = format.vertical {
        let written = match vertical {
            VerticalAlign::Top => "top",
            VerticalAlign::Middle => "middle",
            VerticalAlign::Bottom => "bottom",
        };
        set(root, cell, &Ns::Style, "vertical-align", Some(written));
    }
    if let Some(wrap) = format.wrap {
        let written = if wrap { "wrap" } else { "no-wrap" };
        set(root, cell, &Ns::Fo, "wrap-option", Some(written));
    }
    let sides = ["border-left", "border-right", "border-top", "border-bottom"];
    match format.borders {
        Some(Borders::None | Borders::All) => {
            for side in sides {
                set(root, cell, &Ns::Fo, side, None);
            }
            let line = if format.borders == Some(Borders::All) {
                LINE
            } else {
                "none"
            };
            set(root, cell, &Ns::Fo, "border", Some(line));
        }
        Some(Borders::Outline) => {
            for (side, on_edge) in sides.into_iter().zip(edges) {
                if on_edge {
                    set(root, cell, &Ns::Fo, side, Some(LINE));
                }
            }
        }
        None => {}
    }
}

/// A value as the number a spreadsheet takes it for, where it is one.
fn figure(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) | Value::Percentage(n) | Value::Currency(n, _) => Some(*n),
        Value::Date(text) => date_serial(text),
        Value::Time(text) => duration_days(text),
        Value::Boolean(_) | Value::Text(_) | Value::Empty => None,
    }
}

/// A style's properties element of one kind, written in where it has none,
/// in the order ODF lists them.
fn properties<'a>(style: &'a mut Element, root: &Element, local: &str) -> &'a mut Element {
    const ORDER: [&str; 3] = [
        "table-cell-properties",
        "paragraph-properties",
        "text-properties",
    ];
    let rank = |e: &Element| ORDER.iter().position(|o| e.is(&Ns::Style, o));
    let at = style
        .children
        .iter()
        .position(|n| matches!(n, Node::Element(e) if e.is(&Ns::Style, local)));
    let at = at.unwrap_or_else(|| {
        let mine = ORDER.iter().position(|o| *o == local);
        let before = style
            .children
            .iter()
            .position(|n| match n {
                Node::Element(e) if e.is(&Ns::Style, "map") => true,
                Node::Element(e) => rank(e).is_some_and(|r| Some(r) > mine),
                _ => false,
            })
            .unwrap_or(style.children.len());
        style.children.insert(
            before,
            Node::Element(element(root.name_for(&Ns::Style, local))),
        );
        style.self_closing = false;
        before
    });
    let Node::Element(found) = &mut style.children[at] else {
        unreachable!("found or written as an element");
    };
    found
}

fn hex(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

/// A data style for a number format, as `LibreOffice` writes one, unnamed.
fn data_style(root: &Element, number: NumberFormat, language: &str, country: &str) -> Element {
    let named = |local: &str| -> Name { root.name_for(&Ns::Number, local) };
    let leaf = |local: &str, attrs: &[(&str, String)]| {
        let mut e = element(named(local));
        for (attr, value) in attrs {
            e.set_attr(named(attr), value.clone());
        }
        e
    };
    let text = |words: &str| {
        let mut e = element(named("text"));
        e.children.push(Node::Text(words.to_owned()));
        e.self_closing = false;
        e
    };
    let digits = |decimals: u8, grouping: bool| {
        let mut attrs = vec![
            ("decimal-places", decimals.to_string()),
            ("min-decimal-places", decimals.to_string()),
            ("min-integer-digits", "1".to_owned()),
        ];
        if grouping {
            attrs.push(("grouping", "true".to_owned()));
        }
        leaf("number", &attrs)
    };
    let (local, parts): (&str, Vec<Element>) = match number {
        NumberFormat::General | NumberFormat::Number { .. } => {
            let (decimals, grouping) = match number {
                NumberFormat::Number { decimals, grouping } => (decimals, grouping),
                _ => (0, false),
            };
            ("number-style", vec![digits(decimals, grouping)])
        }
        NumberFormat::Percent { decimals } => {
            ("percentage-style", vec![digits(decimals, false), text("%")])
        }
        NumberFormat::Currency { decimals } => {
            let symbol = currency_symbol(country);
            let mut sign = leaf(
                "currency-symbol",
                &[
                    ("language", language.to_owned()),
                    ("country", country.to_owned()),
                ],
            );
            sign.children.push(Node::Text(symbol.to_owned()));
            sign.self_closing = false;
            let amount = digits(decimals, true);
            let parts = if language == "en" {
                vec![sign, amount]
            } else {
                vec![amount, text(" "), sign]
            };
            ("currency-style", parts)
        }
        NumberFormat::Date => {
            let year = leaf("year", &[("style", "long".to_owned())]);
            let month = leaf("month", &[("style", "long".to_owned())]);
            let day = leaf("day", &[("style", "long".to_owned())]);
            let parts = match country {
                "US" => vec![month, text("/"), day, text("/"), year],
                "DE" | "AT" | "CH" | "LI" | "LU" => vec![day, text("."), month, text("."), year],
                _ if language == "en"
                    || language == "fr"
                    || language == "es"
                    || language == "it" =>
                {
                    vec![day, text("/"), month, text("/"), year]
                }
                _ => vec![year, text("-"), month, text("-"), day],
            };
            ("date-style", parts)
        }
        NumberFormat::Time => {
            let hours = leaf("hours", &[("style", "long".to_owned())]);
            let minutes = leaf("minutes", &[("style", "long".to_owned())]);
            ("time-style", vec![hours, text(":"), minutes])
        }
    };
    let mut style = element(named(local));
    if !matches!(number, NumberFormat::Currency { .. }) {
        style.set_attr(named("language"), language);
        if !country.is_empty() {
            style.set_attr(named("country"), country);
        }
    }
    style.children = parts.into_iter().map(Node::Element).collect();
    style.self_closing = false;
    style
}

/// The symbol of a country's currency.
fn currency_symbol(country: &str) -> &'static str {
    match country {
        "US" | "CA" | "AU" | "NZ" | "MX" => "$",
        "GB" => "£",
        "JP" | "CN" => "¥",
        "IN" => "₹",
        "CH" | "LI" => "CHF",
        _ => "€",
    }
}
