//! xodt: an `OpenDocument` text document, read.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

// `deny` rather than `forbid`, and the difference is the exception this leaves
// room for: receiving a document from macOS needs one Objective-C method that
// cannot be written without `unsafe`, and `forbid` cannot be lifted beneath it.
// `odox-core` and `odox-ui` both keep `forbid`. The same arrangement
// slipcase-desktop's manifest records.
#![deny(unsafe_code)]
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
        },
        |_ctx| view::TextView::default(),
    )
}
