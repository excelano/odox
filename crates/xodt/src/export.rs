//! File > Export as PDF/UA…: what the export asks before it writes, and what
//! it says after.
//!
//! A PDF/UA file says what every picture shows, or that it shows nothing
//! worth saying, so a document with a picture that says neither is asked
//! about first: each such picture is shown with a place for its alternative
//! text and a box to mark it decorative. What is answered goes into the
//! document, as one step to undo, so the next export does not ask again and
//! the document says it too. DESIGN.md §12.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::{Path, PathBuf};

use eframe::egui::{self, Key, Ui};
use odox_core::doc::TextDocument;
use odox_core::{Ns, edit};
use odox_ui::i18n::{fill, reading_language, t};
use odox_ui::{Editing, Notice, Pictures, replace_file};

/// The export, from the menu to the notice.
#[derive(Default)]
pub struct Export {
    /// Asked for from the menu.
    requested: Option<Request>,
    /// The pictures being asked about, while the question is up.
    asking: Option<Question>,
    notice: Option<Notice>,
}

/// An export asked for, with the file the document was read from, which is
/// where the PDF is offered to go.
struct Request {
    source: Option<PathBuf>,
}

/// The pictures an export cannot describe by itself.
struct Question {
    source: Option<PathBuf>,
    answers: Vec<Answer>,
}

/// One picture and what the person has said about it so far.
struct Answer {
    /// Where its frame is, from the body, which an edit to the styles does
    /// not move.
    path: Vec<usize>,
    /// The picture, for showing which one is meant.
    href: Option<String>,
    text: String,
    decorative: bool,
}

impl Answer {
    fn answered(&self) -> bool {
        self.decorative || !self.text.trim().is_empty()
    }
}

impl Export {
    /// The File menu's item.
    pub fn menu(&mut self, ui: &mut Ui, source: Option<&Path>) {
        if ui.button(t("Export as PDF/UA…")).clicked() {
            ui.close();
            self.requested = Some(Request {
                source: source.map(Path::to_path_buf),
            });
        }
    }

    /// Whether the question is up, which the page waits on.
    pub fn asking(&self) -> bool {
        self.asking.is_some()
    }

    pub fn take_notice(&mut self) -> Option<Notice> {
        self.notice.take()
    }

    /// Carry the export on, on each frame: start it once it has been asked
    /// for, and draw the question while it is up.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        document: &mut TextDocument,
        pictures: &mut Pictures,
        editing: &mut Editing,
    ) {
        if let Some(Request { source }) = self.requested.take() {
            let body = document.body_path().unwrap_or_default();
            let answers: Vec<Answer> = odox_pdf::undescribed(document)
                .into_iter()
                .map(|path| {
                    let href = document
                        .document
                        .content
                        .at(&path)
                        .and_then(|frame| frame.child(&Ns::Draw, "image"))
                        .and_then(|image| image.attr(&Ns::Xlink, "href"))
                        .map(ToOwned::to_owned);
                    Answer {
                        path: path[body.len().min(path.len())..].to_vec(),
                        href,
                        text: String::new(),
                        decorative: false,
                    }
                })
                .collect();
            if answers.is_empty() {
                self.write(document, source.as_deref());
            } else {
                self.asking = Some(Question { source, answers });
            }
        }
        let Some(question) = &mut self.asking else {
            return;
        };
        match ask(ctx, question, document, pictures) {
            Some(true) => {
                let question = self.asking.take();
                if let Some(question) = question {
                    match describe(document, &question.answers, editing) {
                        Ok(()) => self.write(document, question.source.as_deref()),
                        Err(reason) => {
                            self.notice = Some(Notice::Problem(fill(
                                t("The pictures could not be described: {reason}"),
                                &[("reason", &reason.to_string())],
                            )));
                        }
                    }
                }
            }
            Some(false) => self.asking = None,
            None => {}
        }
    }

    /// Ask where to write the PDF, and write it there.
    fn write(&mut self, document: &TextDocument, source: Option<&Path>) {
        let stem = source.and_then(Path::file_stem).map_or_else(
            || t("Untitled").to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let mut dialog = rfd::FileDialog::new()
            .add_filter(t("PDF/UA document"), &["pdf"])
            .set_file_name(format!("{stem}.pdf"));
        if let Some(directory) = source.and_then(Path::parent) {
            dialog = dialog.set_directory(directory);
        }
        let Some(mut target) = dialog.save_file() else {
            return;
        };
        if target.extension().is_none() {
            target.set_extension("pdf");
        }
        let options = odox_pdf::Options {
            title: stem,
            language: reading_language(),
        };
        let file = target.file_name().map_or_else(
            || target.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        self.notice = Some(match odox_pdf::export(document, &options) {
            Ok(exported) => match replace_file(&target, &exported.pdf) {
                Ok(()) => {
                    let mut said = fill(t("Exported {file} as PDF/UA."), &[("file", &file)]);
                    for substitution in &exported.substituted {
                        said.push(' ');
                        said.push_str(&fill(
                            t("{family} may not be embedded, so {used} was used instead."),
                            &[
                                ("family", &substitution.family),
                                ("used", &substitution.used),
                            ],
                        ));
                    }
                    Notice::Done(said)
                }
                Err(e) => Notice::Problem(fill(
                    t("{file} could not be saved: {reason}"),
                    &[("file", &file), ("reason", &e.to_string())],
                )),
            },
            Err(refusal) => Notice::Problem(refused(&refusal)),
        });
    }
}

/// What the person reads when an export is refused.
fn refused(refusal: &odox_pdf::Refusal) -> String {
    match refusal {
        odox_pdf::Refusal::Undescribed(paths) => fill(
            t("{count} pictures still say nothing in place of what they show."),
            &[("count", &paths.len().to_string())],
        ),
        odox_pdf::Refusal::Font(family) => fill(
            t("No font for {family} on this computer may be embedded in a PDF."),
            &[("family", family)],
        ),
        odox_pdf::Refusal::Glyph(character) => fill(
            t("No font on this computer has the character {character}."),
            &[("character", &character.to_string())],
        ),
        odox_pdf::Refusal::Writer(reason) => fill(
            t("The PDF could not be written: {reason}"),
            &[("reason", reason)],
        ),
    }
}

/// Draw the question. Answers whether to go on, once the person has said.
fn ask(
    ctx: &egui::Context,
    question: &mut Question,
    document: &TextDocument,
    pictures: &mut Pictures,
) -> Option<bool> {
    let mut answer = None;
    egui::Modal::new(egui::Id::new("export-pictures")).show(ctx, |ui| {
        ui.set_max_width(520.0);
        ui.heading(t("Describe the pictures"));
        ui.label(t(
            "A PDF/UA file says what each picture shows to someone who cannot see it, or that it is decoration and shows nothing they need. These pictures say neither yet.",
        ));
        ui.add_space(8.0);
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for (index, picture) in question.answers.iter_mut().enumerate() {
                ui.push_id(index, |ui| {
                    ui.horizontal(|ui| {
                        let texture = picture
                            .href
                            .as_deref()
                            .and_then(|href| pictures.get(ctx, &document.document, href));
                        if let Some(texture) = texture {
                            ui.add(
                                egui::Image::new(texture)
                                    .max_size(egui::vec2(96.0, 72.0))
                                    .maintain_aspect_ratio(true),
                            );
                        }
                        ui.vertical(|ui| {
                            ui.add_enabled(
                                !picture.decorative,
                                egui::TextEdit::singleline(&mut picture.text)
                                    .hint_text(t("What the picture shows"))
                                    .desired_width(340.0),
                            );
                            ui.checkbox(&mut picture.decorative, t("Decoration only"));
                        });
                    });
                });
                ui.separator();
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let ready = question.answers.iter().all(Answer::answered);
            if ui
                .add_enabled(ready, egui::Button::new(t("Export")))
                .clicked()
            {
                answer = Some(true);
            }
            if ui.button(t("Mark the rest as decoration")).clicked() {
                for picture in &mut question.answers {
                    if picture.text.trim().is_empty() {
                        picture.decorative = true;
                    }
                }
            }
            if ui.button(t("Cancel")).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                answer = Some(false);
            }
        });
    });
    answer
}

/// Write the answers into the document, as one step to undo.
fn describe(
    document: &mut TextDocument,
    answers: &[Answer],
    editing: &mut Editing,
) -> Result<(), odox_core::Refused> {
    let before = document.document.content.clone();
    let described = answers.iter().try_for_each(|answer| {
        let body = document.body_path().ok_or(odox_core::Refused::NotFound)?;
        let path = [body, answer.path.clone()].concat();
        let odox_core::Document {
            content, styles, ..
        } = &mut document.document;
        if answer.decorative {
            edit::set_decorative(content, &path, styles)
        } else {
            edit::set_alternative_text(content, &path, answer.text.trim())
        }
    });
    match described {
        Ok(()) => {
            editing.record_snapshot(before, None);
            Ok(())
        }
        Err(refused) => {
            // Nothing is left half described.
            document.document.content = before;
            Err(refused)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> TextDocument {
        let bytes = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libreoffice/export.odt"),
        )
        .expect("the export fixture");
        TextDocument::read(&bytes).expect("it reads")
    }

    fn answer(document: &TextDocument, index: usize, text: &str, decorative: bool) -> Answer {
        Answer {
            path: edit::figures(document.body().expect("a body"))[index].clone(),
            href: None,
            text: text.to_owned(),
            decorative,
        }
    }

    #[test]
    fn the_answers_go_into_the_document_as_one_step_to_undo() {
        let mut document = fixture();
        let mut editing = Editing::default();
        let answers = [answer(&document, 0, "  Sales by quarter ", false)];
        describe(&mut document, &answers, &mut editing).expect("described");
        assert!(editing.can_undo());
        assert_eq!(odox_pdf::undescribed(&document), Vec::<Vec<usize>>::new());
        let body = document.body_path().expect("a body");
        let frame = document
            .document
            .content
            .at(&[body, answers[0].path.clone()].concat())
            .expect("the frame");
        assert_eq!(
            edit::description(frame, &document.document.styles),
            edit::Description::Text("Sales by quarter".to_owned())
        );
    }

    #[test]
    fn an_answer_that_cannot_be_written_leaves_the_document_as_it_was() {
        let mut document = fixture();
        let before = document.document.content.clone();
        let mut editing = Editing::default();
        let mut answers = vec![answer(&document, 0, "", true)];
        answers.push(Answer {
            path: vec![0],
            href: None,
            text: "a heading is not a picture".to_owned(),
            decorative: false,
        });
        assert!(describe(&mut document, &answers, &mut editing).is_err());
        assert_eq!(document.document.content, before);
        assert!(!editing.can_undo());
    }
}
