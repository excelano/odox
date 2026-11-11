//! Opening a document during a frame, which is every way but one.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::BTreeSet;

use eframe::egui;

/// `Context::set_fonts` takes effect at the start of the next pass, so a
/// document opened during a frame and drawn in that same frame is laid out
/// against the definitions the *previous* document left behind. Asking for a
/// family they do not carry is not a fallback: egui panics.
#[test]
fn a_family_registered_this_frame_is_not_available_until_the_next() {
    let mut families = BTreeSet::new();
    families.insert("Liberation Serif".to_owned());
    let definitions = odox_ui::fonts::definitions(&families);

    let ctx = egui::Context::default();
    let wanted = odox_ui::fonts::family_of(
        "Liberation Serif",
        odox_ui::fonts::Variant {
            bold: true,
            italic: false,
        },
    );

    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = ctx.run_ui(egui::RawInput::default(), |ctx| {
            // What `Shell::ui` does when a document arrives from a drop, from
            // Ctrl+O, or from an Apple Event.
            ctx.set_fonts(definitions.clone());
            let mut job = egui::text::LayoutJob::default();
            job.append(
                "text in a family this frame has never heard of",
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::new(12.0, wanted.clone()),
                    ..Default::default()
                },
            );
            ctx.fonts_mut(|fonts| fonts.layout_job(job));
        });
    }))
    .is_err();

    assert!(
        panicked,
        "this test exists to pin the hazard; if it stopped panicking, egui \
         changed and `Shell::ui`'s guard can be reconsidered"
    );
}
