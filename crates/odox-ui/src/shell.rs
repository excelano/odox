//! The window around a document: opening one, saying what went wrong, and the
//! menu and keys that are the same in all three applications.
//
// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::{Path, PathBuf};

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, Ui};

use crate::i18n::{fill, t};

/// What an application tells the shell about itself.
///
/// The identifiers are not translated: a desktop entry, a window class and a file
/// extension are the same in every language.
pub struct Product {
    /// The binary's name, which is also the `.desktop` entry's basename and the
    /// Wayland application id. The compositor matches the three to find the
    /// window's icon, so they have to agree.
    pub id: &'static str,
    /// The file extension the application opens.
    pub extension: &'static str,
    /// What the file dialog and the empty window call that kind of file.
    ///
    /// The English, which is the message id: it is looked up through [`t`] where
    /// it is shown, because a `Product` is built before `run` puts a catalogue in
    /// force and a translation looked up here would be the English every time.
    pub format: &'static str,
    /// The application's icon directory, for the one platform with no other way
    /// to give a window its icon.
    ///
    /// Windows takes a window's icon from a resource compiled into the
    /// executable, and there is no resource compiler in this build, so the file
    /// travels in the binary instead. Empty everywhere else, and empty in a
    /// build made from a published crate, where the file is not there to stage:
    /// see each application's `build.rs`.
    pub icon: &'static [u8],
}

/// What the shell needs from the view that draws a particular format.
pub trait Viewer {
    /// Take a document's bytes. The path is for the window's title and for
    /// reloading, and is never read from here: this is given the bytes.
    ///
    /// The context is for the work a new document makes necessary before it can
    /// be drawn — loading the font faces its styles name, which rebuilds egui's
    /// glyph atlas and so belongs to opening rather than to drawing.
    ///
    /// # Errors
    ///
    /// A message to show the person, already translated.
    fn open(&mut self, ctx: &egui::Context, bytes: &[u8], path: &Path) -> Result<(), String>;

    /// Forget the document.
    fn close(&mut self);

    /// Whether a document is open.
    fn is_open(&self) -> bool;

    /// What the document calls itself, for the window's title.
    fn title(&self) -> Option<String>;

    /// Draw the document. The shell has already put a scroll area or a panel
    /// around whatever this needs.
    fn central(&mut self, ui: &mut Ui, zoom: f32);

    /// Draw the panel beside the document — an outline, a sheet list, a slide
    /// list — and answer whether there is one to draw.
    fn side(&mut self, _ui: &mut Ui) -> bool {
        false
    }

    /// Add the application's own items to the View menu.
    fn view_menu(&mut self, _ui: &mut Ui) {}
}

/// The window: chrome, keys, errors, and one view inside it.
pub struct Shell<V: Viewer> {
    view: V,
    product: Product,
    path: Option<PathBuf>,
    error: Option<String>,
    /// Set when a document was opened during a frame, cleared at the end of it.
    ///
    /// See the comment where it is read in [`eframe::App::ui`].
    settling: bool,
    zoom: f32,
    show_side: bool,
}

/// How far a zoom step moves, and the limits.
const ZOOM_STEP: f32 = 1.1;
const ZOOM_MIN: f32 = 0.4;
const ZOOM_MAX: f32 = 4.0;

impl<V: Viewer> Shell<V> {
    /// A window with nothing open.
    pub fn new(product: Product, view: V) -> Self {
        Self {
            view,
            product,
            path: None,
            error: None,
            settling: false,
            zoom: 1.0,
            show_side: true,
        }
    }

    /// Open a path, reading it here so that the view never touches the file
    /// system.
    pub fn open(&mut self, ctx: &egui::Context, path: &Path) {
        match std::fs::read(path) {
            Ok(bytes) => match self.view.open(ctx, &bytes, path) {
                Ok(()) => {
                    self.path = Some(path.to_path_buf());
                    self.error = None;
                    self.settling = true;
                }
                Err(message) => {
                    self.view.close();
                    self.path = None;
                    self.error = Some(message);
                }
            },
            Err(e) => {
                self.error = Some(fill(
                    t("{file} could not be read: {reason}"),
                    &[("file", &name_of(path)), ("reason", &e.to_string())],
                ));
            }
        }
    }

    fn ask_for_a_file(&mut self, ctx: &egui::Context) {
        let file = rfd::FileDialog::new()
            .add_filter(t(self.product.format), &[self.product.extension])
            .pick_file();
        if let Some(path) = file {
            self.open(ctx, &path);
        }
    }

    fn reload(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.path.clone() {
            self.open(ctx, &path);
        }
    }

    fn close(&mut self) {
        self.view.close();
        self.path = None;
        self.error = None;
    }

    /// The window's title: the document's own title, or its file name.
    fn window_title(&self) -> String {
        let document = self
            .view
            .title()
            .filter(|title| !title.trim().is_empty())
            .or_else(|| self.path.as_deref().map(name_of));
        match document {
            Some(name) => format!("{name} — {}", self.product.id),
            None => self.product.id.to_owned(),
        }
    }
}

impl<V: Viewer> eframe::App for Shell<V> {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.keys(&ctx);
        self.dropped_files(&ctx);

        // A document macOS asked for, which reaches here rather than through
        // the command line. Taken rather than read, so one event opens one
        // document instead of reopening it on every frame after.
        #[cfg(target_os = "macos")]
        if let Some(path) = crate::opened_document::taken() {
            self.open(&ctx, &path);
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(t("File"), |ui| {
                    if ui.button(t("Open…")).clicked() {
                        ui.close();
                        self.ask_for_a_file(&ctx);
                    }
                    let open = self.path.is_some();
                    if ui
                        .add_enabled(open, egui::Button::new(t("Reload")))
                        .clicked()
                    {
                        ui.close();
                        self.reload(&ctx);
                    }
                    if ui
                        .add_enabled(open, egui::Button::new(t("Close")))
                        .clicked()
                    {
                        ui.close();
                        self.close();
                    }
                    ui.separator();
                    if ui.button(t("Quit")).clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button(t("View"), |ui| {
                    if ui.button(t("Zoom in")).clicked() {
                        self.zoom = (self.zoom * ZOOM_STEP).min(ZOOM_MAX);
                    }
                    if ui.button(t("Zoom out")).clicked() {
                        self.zoom = (self.zoom / ZOOM_STEP).max(ZOOM_MIN);
                    }
                    if ui.button(t("Actual size")).clicked() {
                        self.zoom = 1.0;
                    }
                    ui.separator();
                    ui.checkbox(&mut self.show_side, t("Show the side panel"));
                    self.view.view_menu(ui);
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let percent = (self.zoom * 100.0).round() as u32;
                    ui.label(fill(t("{percent}%"), &[("percent", &percent.to_string())]));
                });
            });
        });

        if let Some(message) = self.error.clone() {
            egui::Panel::bottom("error").show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, "\u{26a0}");
                    ui.label(message);
                    if ui.button(t("Dismiss")).clicked() {
                        self.error = None;
                    }
                });
            });
        }

        if self.show_side && self.view.is_open() {
            // The view answers whether it has anything to put beside the
            // document, so that a format with nothing there gets no empty panel
            // rather than a blank one a person has to close.
            let mut drew = false;
            egui::Panel::left("side")
                .resizable(true)
                .default_size(220.0)
                .min_size(120.0)
                .show(ui, |ui| {
                    egui::ScrollArea::both().show(ui, |ui| {
                        drew = self.view.side(ui);
                    });
                });
            if !drew {
                self.show_side = false;
            }
        }

        // **A document opened during this frame is not drawn until the next
        // one.** Opening one registers the font families it names, and
        // `Context::set_fonts` takes effect at the start of the following pass,
        // so laying the document out now would ask for a family the definitions
        // still in force do not carry. egui does not fall back for that: it
        // panics, and a panic inside the macOS event callback cannot unwind, so
        // the process aborts.
        //
        // Every way of opening a document but one goes through a frame: a drop,
        // Ctrl+O, Reload, and the Apple Event. The exception is the path on the
        // command line, which is opened in eframe's creation closure before any
        // pass has begun, and is why this went unnoticed until a runner opened a
        // document through Launch Services. `tests/fonts_midframe.rs` pins the
        // hazard.
        //
        // One frame, and the repaint is asked for rather than waited for, so
        // the document appears immediately rather than when the pointer next
        // moves.
        let settling = self.settling;
        if settling {
            self.settling = false;
            ctx.request_repaint();
        }

        egui::CentralPanel::default_margins().show(ui, |ui| {
            if settling {
                // Deliberately blank, and for one frame. Drawing the
                // nothing-open message here instead would flash it between a
                // double-click and the document.
            } else if self.view.is_open() {
                self.view.central(ui, self.zoom);
            } else {
                self.nothing_open(ui);
            }
        });
    }
}

impl<V: Viewer> Shell<V> {
    /// What the window says before a document is opened.
    fn nothing_open(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            // The application's own name, which is not translated, and the name
            // of the format, which is: Comma's German has said
            // `OpenDocument-Tabellendokument` since it shipped.
            ui.heading(self.product.id);
            ui.label(t(self.product.format));
            ui.add_space(12.0);
            if ui.button(t("Open a document…")).clicked() {
                let ctx = ui.ctx().clone();
                self.ask_for_a_file(&ctx);
            }
            ui.add_space(8.0);
            ui.weak(t("or drop one on this window"));
        });
    }

    fn keys(&mut self, ctx: &egui::Context) {
        let pressed = |key| {
            ctx.input_mut(|input| {
                input.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, key))
            })
        };
        if pressed(Key::O) {
            self.ask_for_a_file(ctx);
        }
        if pressed(Key::R) {
            self.reload(ctx);
        }
        if pressed(Key::W) {
            self.close();
        }
        // `Plus` is what the key sends with shift held and `Equals` without, and
        // a person pressing the same physical key means the same thing either way.
        if pressed(Key::Plus) || pressed(Key::Equals) {
            self.zoom = (self.zoom * ZOOM_STEP).min(ZOOM_MAX);
        }
        if pressed(Key::Minus) {
            self.zoom = (self.zoom / ZOOM_STEP).max(ZOOM_MIN);
        }
        if pressed(Key::Num0) {
            self.zoom = 1.0;
        }
    }

    fn dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .first()
                .map(|file| file.path().to_path_buf())
        });
        if let Some(path) = dropped {
            self.open(ctx, &path);
        }
    }
}

/// A file's name without its directory, for a title or a message.
fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Open a window for a product, with the paths given on the command line.
///
/// # Errors
///
/// The window could not be created, which is eframe's answer and not this
/// application's.
pub fn run<V: Viewer + 'static>(
    product: Product,
    build: impl FnOnce(&egui::Context) -> V + 'static,
) -> eframe::Result {
    crate::i18n::start();

    let id = product.id;
    let viewport = egui::ViewportBuilder::default()
        // How a Wayland compositor finds the window's icon and its name: it
        // matches this against the basename of the `.desktop` entry.
        .with_app_id(id)
        .with_title(id)
        .with_inner_size([1000.0, 760.0])
        .with_min_inner_size([420.0, 320.0]);

    // **Naming no icon is not neutral on macOS, and it costs the Dock.** The
    // bundle carries the `.icns` and `CFBundleIconFile` points at it, which is
    // where a macOS application's icon comes from, so this looks like an arm
    // with nothing to do. It has something to do: eframe substitutes its own
    // logo for a viewport that names no icon and hands that to
    // `setApplicationIconImage:`, which outranks the bundle. Finder, Launch
    // Services and every API still resolve the right drawing, so nothing short
    // of a person looking at the Dock finds it, and it caught two of the
    // sibling applications before it was written down.
    //
    // An empty `IconData` declines the icon rather than replacing it:
    // eframe turns one into `None` and the macOS arm only calls the selector
    // where there is an image. Handing the drawing over again would also work
    // and would carry a second copy of it in the binary to overwrite the
    // bundle's with a worse-scaled equal.
    //
    // One line for three windows, because all three come through here.
    // **Unverified from Linux**: it compiles for the target and nothing else
    // about it can be checked without looking at a Dock. `CHECKLIST.md` asks.
    #[cfg(target_os = "macos")]
    let viewport = viewport.with_icon(egui::IconData::default());

    // Windows, which is the other half of the same question. Here an icon has
    // to be given, because the shell reads one out of a resource and this build
    // compiles no resource.
    #[cfg(target_os = "windows")]
    let viewport = match window_icon(product.icon) {
        Some(icon) => viewport.with_icon(icon),
        None => viewport,
    };

    let options = eframe::NativeOptions {
        viewport,
        ..eframe::NativeOptions::default()
    };

    // Before `eframe`, because macOS dispatches the document that launched this
    // process before the creation closure is reached and AppKit's own handler
    // refuses it there. `opened_document` has the measurement.
    #[cfg(target_os = "macos")]
    crate::opened_document::watch();

    let first = std::env::args_os().nth(1).map(PathBuf::from);
    eframe::run_native(
        id,
        options,
        Box::new(move |cc| {
            crate::system_theme::follow(&cc.egui_ctx);
            // Now that there is a context, a document that arrives has
            // somewhere to wake.
            #[cfg(target_os = "macos")]
            crate::opened_document::wake_with(&cc.egui_ctx);
            let mut shell = Shell::new(product, build(&cc.egui_ctx));
            if let Some(path) = first {
                shell.open(&cc.egui_ctx, &path);
            }
            Ok(Box::new(shell))
        }),
    )
}

/// The 64-pixel entry of an icon directory, as a window icon.
///
/// 64 because it is the largest size no scaling has to enlarge, and every
/// smaller one the shell wants is a reduction of it. `None` where the directory
/// is empty, which is what a build from a published crate has, or where it does
/// not decode, which is a broken artefact rather than a reason to refuse to
/// open a window.
#[cfg(target_os = "windows")]
fn window_icon(bytes: &'static [u8]) -> Option<egui::IconData> {
    if bytes.is_empty() {
        return None;
    }
    let directory = ico::IconDir::read(std::io::Cursor::new(bytes)).ok()?;
    let entry = directory
        .entries()
        .iter()
        .find(|entry| entry.width() == 64)
        .or_else(|| directory.entries().last())?;
    let image = entry.decode().ok()?;
    Some(egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.rgba_data().to_vec(),
    })
}
