//! xods: an `OpenDocument` spreadsheet, read.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

// The reasoning for each of these three is in `xodt/src/main.rs`, which is where
// the suite's first application wrote them down.
#![deny(unsafe_code)]
#![warn(clippy::pedantic)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod grid;
mod view;

use odox_ui::{Product, mark, run};

fn main() -> eframe::Result {
    run(
        Product {
            id: "xods",
            extension: "ods",
            format: mark("OpenDocument Spreadsheet"),
            icon: include_bytes!(concat!(env!("OUT_DIR"), "/window.ico")),
        },
        |_ctx| view::SheetView::default(),
    )
}
