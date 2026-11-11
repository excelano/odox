//! Spreadsheet formulas: read from the `OpenFormula` a cell stores, and
//! worked out from the cells they refer to. DESIGN.md §11.
//!
//! The core is deliberately small: arithmetic, comparison and joining text
//! over numbers, text, booleans and references to cells and ranges on any
//! sheet, and about forty common functions. A formula that uses anything
//! else is [`Unsupported`] and keeps the result its producer wrote. What a
//! formula answers follows `LibreOffice`, measured on
//! `corpus/libreoffice/formulas.ods`: a boolean is the number one or zero, `=`
//! compares text with its case and a criterion matches text without it, and a
//! date is a count of days from 30 December 1899.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

mod convert;
mod eval;
mod parse;
mod shift;
mod typed;

pub use eval::{Cells, evaluate};
pub(crate) use eval::{
    GREGORIAN, approx, civil as eval_civil, iso_day, julian, read_iso_day, serial as eval_serial,
};
pub use parse::parse;
pub use shift::shifted;
pub use typed::{to_stored, to_typed};

/// A formula, or part of one.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A number written in the formula.
    Number(f64),
    /// Text written in the formula.
    Text(String),
    /// An error written in the formula.
    Error(Error),
    /// A cell or a range.
    Reference(Reference),
    /// A function's argument left out, as in `IF(a;;b)`.
    Missing,
    /// `-x`.
    Negate(Box<Expr>),
    /// `x%`.
    Percent(Box<Expr>),
    /// Two operands and what is done with them.
    Binary(Op, Box<Expr>, Box<Expr>),
    /// A function, by its upper-case name, and its arguments.
    Call(String, Vec<Expr>),
}

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `+`
    Add,
    /// `-`
    Subtract,
    /// `*`
    Multiply,
    /// `/`
    Divide,
    /// `^`
    Power,
    /// `&`
    Join,
    /// `=`
    Equal,
    /// `<>`
    NotEqual,
    /// `<`
    Less,
    /// `<=`
    LessOrEqual,
    /// `>`
    Greater,
    /// `>=`
    GreaterOrEqual,
}

impl Op {
    fn of(spelled: &str) -> Self {
        match spelled {
            "+" => Self::Add,
            "-" => Self::Subtract,
            "*" => Self::Multiply,
            "/" => Self::Divide,
            "^" => Self::Power,
            "&" => Self::Join,
            "=" => Self::Equal,
            "<>" => Self::NotEqual,
            "<" => Self::Less,
            "<=" => Self::LessOrEqual,
            ">" => Self::Greater,
            _ => Self::GreaterOrEqual,
        }
    }
}

/// A cell, counting rows and columns from zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Address {
    /// The row.
    pub row: usize,
    /// The column.
    pub column: usize,
}

/// A cell or a rectangle of cells, on the sheet it names or the formula's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// The sheet by name, or `None` for the sheet the formula is on.
    pub sheet: Option<String>,
    /// The top left cell.
    pub from: Address,
    /// The bottom right cell, the same as `from` for one cell.
    pub to: Address,
}

impl Reference {
    /// Whether a cell on the reference's sheet is inside it.
    pub fn contains(&self, row: usize, column: usize) -> bool {
        (self.from.row..=self.to.row).contains(&row)
            && (self.from.column..=self.to.column).contains(&column)
    }
}

/// A spreadsheet's error values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// `#DIV/0!`
    DivisionByZero,
    /// `#VALUE!`
    Value,
    /// `#N/A`
    NotAvailable,
    /// `#REF!`
    Reference,
    /// `#NAME?`
    Name,
    /// `#NUM!`
    Number,
    /// `Err:502`, an argument outside what the function takes.
    Argument,
    /// `Err:504`, an argument of a kind the function does not take.
    ArgumentKind,
    /// `Err:511`, an argument the function needs left out.
    ArgumentMissing,
    /// `Err:522`, a formula that comes back round to itself.
    Circular,
}

impl Error {
    /// Every error, longest code first where one begins another.
    pub const ALL: [Self; 10] = [
        Self::DivisionByZero,
        Self::Value,
        Self::NotAvailable,
        Self::Reference,
        Self::Name,
        Self::Number,
        Self::Argument,
        Self::ArgumentKind,
        Self::ArgumentMissing,
        Self::Circular,
    ];

    /// How a cell shows the error.
    pub fn code(self) -> &'static str {
        match self {
            Self::DivisionByZero => "#DIV/0!",
            Self::Value => "#VALUE!",
            Self::NotAvailable => "#N/A",
            Self::Reference => "#REF!",
            Self::Name => "#NAME?",
            Self::Number => "#NUM!",
            Self::Argument => "Err:502",
            Self::ArgumentKind => "Err:504",
            Self::ArgumentMissing => "Err:511",
            Self::Circular => "Err:522",
        }
    }
}

/// What a formula, or a cell it reads, comes to.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// An empty cell.
    Empty,
    /// A number, which a date, a time, a percentage and an amount also are.
    Number(f64),
    /// A number that is a date, as `DATE` answers it.
    Date(f64),
    /// Text.
    Text(String),
    /// True or false.
    Boolean(bool),
    /// An error.
    Error(Error),
}

/// A formula that uses something outside the core this crate works out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unsupported;
