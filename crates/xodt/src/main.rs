//! xodt: an `OpenDocument` text document, read.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

// `forbid`, which is stronger than the `deny` the fleet's single-application
// repositories carry. The exception they leave room for is receiving a document
// from macOS, and here that lives in `odox_ui::opened_document` rather than in
// each application, because all three windows come through one `run`.
#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
// Windows creates a console for a console-subsystem process, and a file manager
// launching this one is not attached to a terminal, so double-clicking a document
// would open a black console window behind it. Ignored everywhere else, and off
// in a debug build, which is where a panic message still has somewhere to go.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod view;

use odox_ui::{Product, mark, run};

fn main() -> eframe::Result {
    run(
        Product {
            id: "xodt",
            extension: "odt",
            format: mark("OpenDocument Text"),
            icon: include_bytes!(concat!(env!("OUT_DIR"), "/window.ico")),
        },
        |_ctx| view::TextView::default(),
    )
}
