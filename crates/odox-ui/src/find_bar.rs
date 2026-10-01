//! The bar a search is typed into, under the menu: the query, how many matches
//! there are, and the way from one to the next. It holds what a person typed
//! and nothing of the document; the shell asks the view what was found.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{self, Key, Ui, text::CCursor, text::CCursorRange};

use crate::i18n::{fill, t};

/// Which way to move among the matches.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The match after this one, from the last round to the first.
    Next,
    /// The match before this one, from the first round to the last.
    Previous,
}

/// The state of the search bar.
#[derive(Default)]
pub struct FindBar {
    open: bool,
    query: String,
    /// The field is to take the keyboard, and what it holds be selected, on
    /// the next frame it is drawn.
    focus: bool,
    /// What the view was last asked, so that it is asked again only when the
    /// query or the document is not what it was.
    asked: Option<(String, u64)>,
    /// How many matches the view reported for what it was last asked.
    pub(crate) count: usize,
    /// Which of them is current, counted from zero.
    pub(crate) current: usize,
    /// The view holds a search it has not been told to forget.
    pub(crate) searching: bool,
}

impl FindBar {
    /// Show the bar and put the keyboard in it.
    pub fn open(&mut self) {
        self.open = true;
        self.focus = true;
    }

    /// Hide the bar. The query stays for the next time.
    pub fn close(&mut self) {
        self.open = false;
        self.asked = None;
    }

    /// Whether the bar is showing.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// What was typed.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Whether the view has to be asked again, given the document's revision;
    /// answers `Some(query_changed)` if so, and remembers it was asked.
    pub fn due(&mut self, revision: u64) -> Option<bool> {
        let same_query = self.asked.as_ref().is_some_and(|(q, _)| *q == self.query);
        let same_revision = self.asked.as_ref().is_some_and(|(_, r)| *r == revision);
        if same_query && same_revision {
            return None;
        }
        self.asked = Some((self.query.clone(), revision));
        Some(!same_query)
    }

    /// Draw the bar.
    pub fn ui(&mut self, ui: &mut Ui) -> Option<Step> {
        let (count, current) = (self.count, self.current);
        let mut step = None;
        ui.horizontal(|ui| {
            let id = egui::Id::new("odox-find");
            let mut output = egui::TextEdit::singleline(&mut self.query)
                .id(id)
                .hint_text(t("Find"))
                .desired_width(240.0)
                .show(ui);
            if self.focus {
                self.focus = false;
                output.response.request_focus();
                output.state.cursor.set_char_range(Some(CCursorRange::two(
                    CCursor::new(0),
                    CCursor::new(self.query.chars().count()),
                )));
                output.state.store(ui.ctx(), id);
            }
            if output.response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                step = Some(if ui.input(|input| input.modifiers.shift) {
                    Step::Previous
                } else {
                    Step::Next
                });
                output.response.request_focus();
            }
            if output.response.lost_focus() && ui.input(|input| input.key_pressed(Key::Escape)) {
                self.close();
                return;
            }
            if ui.button(t("Previous")).clicked() {
                step = Some(Step::Previous);
            }
            if ui.button(t("Next")).clicked() {
                step = Some(Step::Next);
            }
            if self.query.is_empty() {
            } else if count == 0 {
                ui.colored_label(ui.visuals().warn_fg_color, t("No matches"));
            } else {
                ui.label(fill(
                    t("{current} of {count}"),
                    &[
                        ("current", &(current + 1).to_string()),
                        ("count", &count.to_string()),
                    ],
                ));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("×").on_hover_text(t("Close")).clicked() {
                    self.close();
                }
            });
        });
        step
    }
}
