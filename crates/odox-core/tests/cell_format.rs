//! Formatting cells, on the blank spreadsheet File > New makes.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use odox_core::doc::{Block, Borders, CellFormat, NumberFormat, SheetDocument, Value};
use odox_core::{Color, TextAlign};

fn blank(language: &str) -> SheetDocument {
    let mut document =
        SheetDocument::read(&odox_core::blank::spreadsheet(language, "Sheet1")).expect("blank");
    for row in 0..3 {
        for column in 0..3 {
            let value = Value::Number(f64::from(u8::try_from(row * 3 + column).unwrap()) + 0.25);
            document.set_cell(0, row, column, &value).expect("written");
        }
    }
    document
}

fn all() -> Block {
    Block {
        top: 0,
        left: 0,
        bottom: 2,
        right: 2,
    }
}

fn saved(document: &mut SheetDocument) -> SheetDocument {
    SheetDocument::read(&document.document.write().expect("written")).expect("read back")
}

#[test]
fn a_block_takes_bold_a_fill_and_alignment_and_keeps_them_saved() {
    let mut document = blank("en-US");
    let yellow = Color {
        r: 0xff,
        g: 0xff,
        b: 0x00,
    };
    let format = CellFormat {
        bold: Some(true),
        background: Some(Some(yellow)),
        align: Some(Some(TextAlign::Center)),
        ..CellFormat::default()
    };
    document.format_cells(0, all(), &format).expect("formatted");
    let document = saved(&mut document);
    let sheet = &document.sheets()[0];
    let mut names = Vec::new();
    for row in 0..3 {
        for column in 0..3 {
            let cell = document.cell(sheet, row, column).expect("a cell");
            let style = document.cell_style(sheet, Some(&cell), column);
            assert_eq!(style.text.bold, Some(true));
            assert_eq!(style.cell.background, Some(yellow));
            assert_eq!(style.paragraph.align, Some(TextAlign::Center));
            names.push(cell.style_name().expect("a style").to_owned());
        }
    }
    names.dedup();
    assert_eq!(names.len(), 1, "one style serves every cell");
    let outside = document.cell(sheet, 3, 0);
    assert!(outside.is_none_or(|c| c.style_name().is_none()));
}

#[test]
fn a_change_keeps_what_the_cell_had() {
    let mut document = blank("en-US");
    let one = Block {
        top: 1,
        left: 1,
        bottom: 1,
        right: 1,
    };
    let sheet_style = |document: &SheetDocument| {
        let sheet = &document.sheets()[0];
        let cell = document.cell(sheet, 1, 1).expect("a cell");
        (
            cell.style_name().map(ToOwned::to_owned),
            document.cell_style(sheet, Some(&cell), 1),
        )
    };
    let bold = CellFormat {
        bold: Some(true),
        ..CellFormat::default()
    };
    let italic = CellFormat {
        italic: Some(true),
        ..CellFormat::default()
    };
    document.format_cells(0, one, &bold).expect("bold");
    let (bold_name, _) = sheet_style(&document);
    document.format_cells(0, one, &italic).expect("italic");
    let (_, style) = sheet_style(&document);
    assert_eq!(
        (style.text.bold, style.text.italic),
        (Some(true), Some(true))
    );
    let roman = CellFormat {
        italic: Some(false),
        ..CellFormat::default()
    };
    document.format_cells(0, one, &roman).expect("roman");
    let (name, _) = sheet_style(&document);
    assert_ne!(name, bold_name, "italic written off is not italic absent");
    let (_, style) = sheet_style(&document);
    assert_eq!(
        (style.text.bold, style.text.italic),
        (Some(true), Some(false))
    );
}

#[test]
fn an_outline_borders_the_block_and_not_its_inside() {
    let mut document = blank("en-US");
    let format = CellFormat {
        borders: Some(Borders::Outline),
        ..CellFormat::default()
    };
    document.format_cells(0, all(), &format).expect("formatted");
    let sheet = &document.sheets()[0];
    let border = |row, column| {
        let cell = document.cell(sheet, row, column);
        let b = document
            .cell_style(sheet, cell.as_ref(), column)
            .cell
            .border;
        [b.left, b.right, b.top, b.bottom].map(|edge| edge.is_some())
    };
    assert_eq!(border(0, 0), [true, false, true, false]);
    assert_eq!(border(1, 1), [false, false, false, false]);
    assert_eq!(border(2, 2), [false, true, false, true]);
    let format = CellFormat {
        borders: Some(Borders::All),
        ..CellFormat::default()
    };
    document.format_cells(0, all(), &format).expect("formatted");
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 1, 1);
    let b = document.cell_style(sheet, cell.as_ref(), 1).cell.border;
    assert!(b.left.is_some() && b.right.is_some() && b.top.is_some() && b.bottom.is_some());
}

#[test]
fn a_number_format_retypes_and_rewrites_the_value() {
    let mut document = blank("en-US");
    let first = Block {
        top: 0,
        left: 0,
        bottom: 0,
        right: 2,
    };
    let percent = CellFormat {
        number: Some(NumberFormat::Percent { decimals: 0 }),
        ..CellFormat::default()
    };
    document.format_cells(0, first, &percent).expect("percent");
    let second = Block {
        top: 1,
        left: 0,
        bottom: 1,
        right: 0,
    };
    document
        .set_cell(0, 1, 0, &Value::Number(1234.5))
        .expect("amount");
    let currency = CellFormat {
        number: Some(NumberFormat::Currency { decimals: 2 }),
        ..CellFormat::default()
    };
    document
        .format_cells(0, second, &currency)
        .expect("currency");
    document
        .set_cell(0, 2, 0, &Value::Number(46_000.0))
        .expect("serial");
    let third = Block {
        top: 2,
        left: 0,
        bottom: 2,
        right: 0,
    };
    let date = CellFormat {
        number: Some(NumberFormat::Date),
        ..CellFormat::default()
    };
    document.format_cells(0, third, &date).expect("date");

    let document = saved(&mut document);
    let sheet = &document.sheets()[0];
    let cell = |row, column| document.cell(sheet, row, column).expect("a cell");
    assert_eq!(cell(0, 0).value(), Value::Percentage(0.25));
    assert_eq!(cell(0, 0).text(), "25%");
    assert_eq!(cell(0, 2).text(), "225%");
    assert_eq!(
        cell(1, 0).value(),
        Value::Currency(1234.5, Some("USD".to_owned()))
    );
    assert_eq!(cell(1, 0).text(), "$1,234.50");
    assert_eq!(cell(2, 0).value(), Value::Date("2025-12-09".to_owned()));
    assert_eq!(cell(2, 0).text(), "12/09/2025");

    let mut document = document;
    let general = CellFormat {
        number: Some(NumberFormat::General),
        ..CellFormat::default()
    };
    document.format_cells(0, first, &general).expect("general");
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 0, 0).expect("a cell");
    assert_eq!(cell.value(), Value::Number(0.25));
    assert_eq!(cell.text(), "0.25");
}

#[test]
fn a_german_document_writes_its_own_currency_and_date() {
    let mut document = blank("de-DE");
    document
        .set_cell(0, 0, 0, &Value::Number(1234.5))
        .expect("amount");
    let one = Block {
        top: 0,
        left: 0,
        bottom: 0,
        right: 0,
    };
    let currency = CellFormat {
        number: Some(NumberFormat::Currency { decimals: 2 }),
        ..CellFormat::default()
    };
    document.format_cells(0, one, &currency).expect("currency");
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 0, 0).expect("a cell");
    assert_eq!(
        cell.value(),
        Value::Currency(1234.5, Some("EUR".to_owned()))
    );
    assert_eq!(cell.text(), "1.234,50 €");
}

#[test]
fn a_formula_keeps_its_formula_when_its_format_changes() {
    let mut document = blank("en-US");
    document
        .set_formula(0, 3, 0, "of:=[.A1]+[.B1]")
        .expect("formula");
    document.recalculate(&[(0, 3, 0)]);
    let one = Block {
        top: 3,
        left: 0,
        bottom: 3,
        right: 0,
    };
    let percent = CellFormat {
        number: Some(NumberFormat::Percent { decimals: 1 }),
        ..CellFormat::default()
    };
    document.format_cells(0, one, &percent).expect("percent");
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 3, 0).expect("a cell");
    assert!(cell.formula().is_some());
    assert!(
        document.document.declares(&odox_core::Ns::Of),
        "a new sheet declares what its formulas are written in"
    );
    assert_eq!(cell.value(), Value::Percentage(1.5));
    assert_eq!(cell.text(), "150.0%");
}

fn one(row: usize, column: usize) -> Block {
    Block {
        top: row,
        left: column,
        bottom: row,
        right: column,
    }
}

fn shown_as(number: NumberFormat) -> CellFormat {
    CellFormat {
        number: Some(number),
        ..CellFormat::default()
    }
}

#[test]
fn a_negative_number_keeps_its_sign_as_a_time() {
    let mut document = blank("en-US");
    document
        .set_cell(0, 5, 0, &Value::Number(-2.5))
        .expect("set");
    document
        .format_cells(0, one(5, 0), &shown_as(NumberFormat::Time))
        .expect("time");
    let mut document = saved(&mut document);
    let cell = |document: &SheetDocument| {
        let sheet = &document.sheets()[0];
        let cell = document.cell(sheet, 5, 0).expect("a cell");
        (cell.value(), cell.text())
    };
    assert_eq!(
        cell(&document),
        (Value::Time("-PT60H00M00S".to_owned()), "12:00".to_owned())
    );
    document
        .format_cells(0, one(5, 0), &shown_as(NumberFormat::General))
        .expect("general");
    assert_eq!(cell(&document).0, Value::Number(-2.5));
}

#[test]
fn a_moment_just_short_of_midnight_is_the_next_day() {
    let mut document = blank("en-US");
    document
        .set_cell(0, 5, 0, &Value::Number(45_000.999_999_9))
        .expect("set");
    document
        .format_cells(0, one(5, 0), &shown_as(NumberFormat::Date))
        .expect("date");
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 5, 0).expect("a cell");
    assert_eq!(cell.value(), Value::Date("2023-03-16".to_owned()));
}

#[test]
fn a_number_no_calendar_reaches_stays_a_number() {
    let mut document = blank("en-US");
    document
        .set_cell(0, 5, 0, &Value::Number(1e20))
        .expect("set");
    document
        .format_cells(0, one(5, 0), &shown_as(NumberFormat::Date))
        .expect("date");
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 5, 0).expect("a cell");
    assert_eq!(cell.value(), Value::Number(1e20));
}

#[test]
fn a_number_in_a_date_cell_is_shown_as_its_day() {
    let mut document = blank("en-US");
    document
        .format_cells(0, one(5, 0), &shown_as(NumberFormat::Date))
        .expect("date");
    document
        .set_formula(0, 5, 0, "of:=45000+1")
        .expect("formula");
    document.recalculate(&[(0, 5, 0)]);
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 5, 0).expect("a cell");
    assert_eq!(cell.text(), "03/16/2023");
}

#[test]
fn an_outline_reaches_a_merged_cell_on_the_blocks_edge() {
    let mut document = blank("en-US");
    document.merge(0, 1..=1, 1..=2).expect("merged");
    let format = CellFormat {
        borders: Some(Borders::Outline),
        ..CellFormat::default()
    };
    let block = Block {
        top: 1,
        left: 1,
        bottom: 2,
        right: 2,
    };
    document.format_cells(0, block, &format).expect("formatted");
    let sheet = &document.sheets()[0];
    let cell = document.cell(sheet, 1, 1);
    let b = document.cell_style(sheet, cell.as_ref(), 1).cell.border;
    assert!(b.left.is_some() && b.right.is_some() && b.top.is_some());
}

#[test]
fn a_block_inside_repeated_rows_is_split_and_the_rest_reads_the_same() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/gaps.ods"),
    )
    .expect("the gaps fixture");
    let mut document = SheetDocument::read(&bytes).expect("it reads");
    let read = |document: &SheetDocument| {
        let sheet = &document.sheets()[0];
        (0..sheet.used_rows + 3)
            .flat_map(|row| (0..4).map(move |column| (row, column)))
            .map(|(row, column)| {
                document
                    .cell(sheet, row, column)
                    .map_or(Value::Empty, |cell| cell.value())
            })
            .collect::<Vec<_>>()
    };
    let before = read(&document);
    let used = document.sheets()[0].used_rows;
    let block = Block {
        top: 2,
        left: 1,
        bottom: used + 1,
        right: 2,
    };
    let bold = CellFormat {
        bold: Some(true),
        ..CellFormat::default()
    };
    document.format_cells(0, block, &bold).expect("formatted");
    let document = saved(&mut document);
    assert_eq!(read(&document), before);
    let sheet = &document.sheets()[0];
    for row in 2..=used + 1 {
        let cell = document.cell(sheet, row, 1);
        let style = document.cell_style(sheet, cell.as_ref(), 1);
        assert_eq!(style.text.bold, Some(true), "row {row}");
    }
    let cell = document.cell(sheet, 1, 1);
    assert_ne!(
        document.cell_style(sheet, cell.as_ref(), 1).text.bold,
        Some(true)
    );
}

#[test]
fn a_new_amount_takes_its_countrys_currency_where_libreoffice_puts_it() {
    let cases = [
        ("sv-SE", "1\u{a0}234,50 kr", "SEK"),
        ("pl-PL", "1\u{a0}234,50 zł", "PLN"),
        ("pt-BR", "R$ 1.234,50", "BRL"),
        ("da-DK", "kr. 1.234,50", "DKK"),
        ("nb-NO", "kr 1\u{a0}234,50", "NOK"),
        ("ko-KR", "\u{ffe6}1,234.50", "KRW"),
    ];
    let one = Block {
        top: 0,
        left: 0,
        bottom: 0,
        right: 0,
    };
    let currency = CellFormat {
        number: Some(NumberFormat::Currency { decimals: 2 }),
        ..CellFormat::default()
    };
    for (tag, shown, code) in cases {
        let mut document = blank(tag);
        document
            .set_cell(0, 0, 0, &Value::Number(1234.5))
            .expect("amount");
        document.format_cells(0, one, &currency).expect("currency");
        let sheet = &document.sheets()[0];
        let cell = document.cell(sheet, 0, 0).expect("a cell");
        assert_eq!(cell.text(), shown, "{tag}");
        assert_eq!(
            cell.value(),
            Value::Currency(1234.5, Some(code.to_owned())),
            "{tag}"
        );
    }
}
