//! A window of a few paragraphs, the first drawn as a heading, edited with
//! a caret: `cargo run -p egui_richedit --example notes`.
//!
//! The model is a list of strings. Everything a real application would keep
//! richer — runs, styles, lists — lives on its side of [`Model`]; the editor
//! needs only what is here.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{self, FontId, Id, Sense, TextFormat, text::LayoutJob, vec2};
use egui_richedit::{Edit, Laid, Model, ParagraphJob, Position, RichEdit, Selection};

struct Notes {
    paragraphs: Vec<String>,
}

fn byte(text: &str, offset: usize) -> usize {
    text.char_indices()
        .nth(offset)
        .map_or(text.len(), |(i, _)| i)
}

impl Model for Notes {
    type Paragraph = usize;

    fn text(&self, paragraph: &usize) -> Option<String> {
        self.paragraphs.get(*paragraph).cloned()
    }

    fn next(&self, paragraph: &usize) -> Option<usize> {
        (paragraph + 1 < self.paragraphs.len()).then_some(paragraph + 1)
    }

    fn previous(&self, paragraph: &usize) -> Option<usize> {
        paragraph.checked_sub(1)
    }

    fn first(&self) -> Option<usize> {
        (!self.paragraphs.is_empty()).then_some(0)
    }

    fn last(&self) -> Option<usize> {
        self.paragraphs.len().checked_sub(1)
    }

    fn apply(&mut self, edit: Edit<'_, usize>, _new_step: bool) -> Option<Position<usize>> {
        match edit {
            Edit::Replace { from, to, text } => {
                let last = self.paragraphs.get(to.paragraph)?;
                let tail = last[byte(last, to.offset)..].to_owned();
                let first = self.paragraphs.get_mut(from.paragraph)?;
                first.truncate(byte(first, from.offset));
                first.push_str(text);
                first.push_str(&tail);
                self.paragraphs.drain(from.paragraph + 1..=to.paragraph);
                Some(Position::new(
                    from.paragraph,
                    from.offset + text.chars().count(),
                ))
            }
            Edit::Split { at } => {
                let paragraph = self.paragraphs.get_mut(at.paragraph)?;
                let second = paragraph.split_off(byte(paragraph, at.offset));
                self.paragraphs.insert(at.paragraph + 1, second);
                Some(Position::new(at.paragraph + 1, 0))
            }
        }
    }
}

struct App {
    notes: Notes,
    editor: RichEdit<usize>,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                // Input first, so this frame draws what was typed.
                self.editor.input(ui, &mut self.notes);

                let width = ui.available_width().min(520.0);
                let ink = ui.visuals().text_color();
                for (index, text) in self.notes.paragraphs.iter().enumerate() {
                    let format = if index == 0 {
                        TextFormat::simple(FontId::proportional(24.0), ink)
                    } else {
                        TextFormat::simple(FontId::proportional(15.0), ink)
                    };
                    let mut job = ParagraphJob::new(LayoutJob {
                        wrap: egui::text::TextWrapping {
                            max_width: width,
                            ..Default::default()
                        },
                        ..LayoutJob::default()
                    });
                    job.text(text, format.clone());
                    // An empty paragraph still takes a line's height, and the
                    // model holds nothing for the space that gives it one.
                    if job.is_empty() {
                        job.atom(" ", 0, format);
                    }
                    let (job, map) = job.into_parts();
                    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                    let (rect, response) = ui
                        .allocate_exact_size(vec2(width, galley.size().y), Sense::click_and_drag());
                    let laid = Laid {
                        galley,
                        map,
                        origin: rect.min,
                    };
                    self.editor.paragraph(ui, &response, &index, laid);
                    ui.add_space(6.0);
                }
            });
        });
    }
}

fn main() -> eframe::Result {
    let notes = Notes {
        paragraphs: vec![
            "Notes".to_owned(),
            "Click anywhere in this text and type. Enter splits a paragraph and \
             Backspace at the start of one joins it to the one before."
                .to_owned(),
            "Shift with the arrows selects, across paragraphs too, and Ctrl+C, \
             Ctrl+X and Ctrl+V go through the clipboard."
                .to_owned(),
        ],
    };
    eframe::run_native(
        "Notes",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::light());
            let mut editor = RichEdit::new(Id::new("notes"));
            editor.select(Selection::caret(Position::new(1, 0)));
            Ok(Box::new(App { notes, editor }))
        }),
    )
}
