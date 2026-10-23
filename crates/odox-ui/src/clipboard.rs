//! The system clipboard, with HTML beside the text.
//!
//! egui's clipboard carries text. A copy that is to keep its formatting in
//! another program puts HTML there too, which this does through one clipboard
//! kept for as long as the application runs: on X11 the clipboard is a
//! conversation with whoever pastes, and the one who copied has to stay to
//! answer. Where no clipboard can be had, a copy is egui's text alone and a
//! paste finds no HTML.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::cell::RefCell;

use eframe::egui::Context;

/// The one clipboard the application keeps.
pub struct System;

/// What every editor and grid copies through.
pub static SYSTEM: System = System;

thread_local! {
    /// Made on the window's thread, the only one that copies or pastes, and
    /// kept until it ends.
    static CLIPBOARD: RefCell<Option<arboard::Clipboard>> =
        RefCell::new(arboard::Clipboard::new().ok());
}

fn with<T>(f: impl FnOnce(&mut arboard::Clipboard) -> Option<T>) -> Option<T> {
    CLIPBOARD.with(|held| f(held.try_borrow_mut().ok()?.as_mut()?))
}

/// Put text on the clipboard, with HTML beside it where there is some.
pub fn put(ctx: &Context, text: &str, html: Option<&str>) {
    let put = html.and_then(|html| with(|clipboard| clipboard.set_html(html, Some(text)).ok()));
    if put.is_none() {
        ctx.copy_text(text.to_owned());
    }
}

impl egui_richedit::Clipboard for System {
    fn put(&self, ctx: &Context, text: &str, html: Option<&str>) {
        put(ctx, text, html);
    }

    fn html(&self) -> Option<String> {
        with(|clipboard| clipboard.get().html().ok())
    }
}
