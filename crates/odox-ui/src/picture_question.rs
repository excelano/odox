//! Insert a picture…: the file is chosen, and then the person is asked what
//! the picture shows, or whether it is decoration only, before it goes in.
//! DESIGN.md §11.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{self, TextureHandle, TextureOptions};
use odox_core::picture;

use crate::Picture;
use crate::flow::decode;
use crate::i18n::t;

/// The question asked once a picture has been chosen.
pub struct PictureQuestion {
    picture: Picture,
    texture: Option<TextureHandle>,
    text: String,
    decorative: bool,
}

impl PictureQuestion {
    /// Ask for a PNG or a JPEG file, and read it into a question, drawn no
    /// wider than `width` points. `Ok(None)` where nothing was chosen, and the
    /// sentence to show where what was chosen cannot go in.
    ///
    /// # Errors
    ///
    /// The file could not be read, or is neither a PNG nor a JPEG.
    pub fn choose(ctx: &egui::Context, width: f32) -> Result<Option<Self>, String> {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(t("Pictures"), &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return Ok(None);
        };
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let not_a_picture = || t("That file is not a PNG or a JPEG picture.").to_owned();
        let (media_type, extension) = picture::kind(&bytes).ok_or_else(not_a_picture)?;
        let (across, down) = picture::pixel_size(&bytes)
            .filter(|&(across, down)| across > 0 && down > 0)
            .ok_or_else(not_a_picture)?;
        let texture = decode(&bytes)
            .map(|image| ctx.load_texture("picture to insert", image, TextureOptions::LINEAR));
        Ok(Some(Self {
            picture: Picture {
                size: drawn_size(&bytes, (across, down), width),
                bytes,
                media_type,
                extension,
                description: None,
            },
            texture,
            text: String::new(),
            decorative: false,
        }))
    }

    /// Draw the question while it is up. Answers once the person has:
    /// whether to put the picture in.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<bool> {
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("insert-picture")).show(ctx, |ui| {
            ui.heading(t("Insert a picture"));
            ui.add_space(6.0);
            if let Some(texture) = &self.texture {
                ui.add(egui::Image::new(texture).max_size(egui::vec2(320.0, 200.0)));
                ui.add_space(6.0);
            }
            ui.label(t("What the picture shows"));
            ui.add_enabled(
                !self.decorative,
                egui::TextEdit::multiline(&mut self.text)
                    .desired_rows(2)
                    .desired_width(320.0),
            );
            ui.checkbox(&mut self.decorative, t("Decoration only"));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(t("Insert")).clicked() {
                    answer = Some(true);
                }
                if ui.button(t("Cancel")).clicked()
                    || ui.input(|input| input.key_pressed(egui::Key::Escape))
                {
                    answer = Some(false);
                }
            });
        });
        answer
    }

    /// The picture, with what the person said it shows.
    pub fn picture(self) -> Picture {
        Picture {
            description: (!self.decorative).then(|| self.text.trim().to_owned()),
            ..self.picture
        }
    }
}

/// The size a picture is drawn at, in points: its pixels at the resolution
/// its header gives, or 96 to the inch, made no wider than `width`.
fn drawn_size(bytes: &[u8], (across, down): (u32, u32), width: f32) -> (f32, f32) {
    let (per_inch_across, per_inch_down) = picture::dots_per_inch(bytes).unwrap_or((96.0, 96.0));
    #[allow(clippy::cast_precision_loss)]
    let (wide, high) = (
        across as f32 / per_inch_across * 72.0,
        down as f32 / per_inch_down * 72.0,
    );
    let shrink = (width / wide).min(1.0);
    (wide * shrink, high * shrink)
}
