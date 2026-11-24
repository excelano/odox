//! Menu accelerators: a letter of a menu entry's label, marked in its message
//! with `&` ("Save &As…"), drawn underlined. Alt and the letter opens a menu
//! on the menu bar, and the letter alone chooses an entry of the open menu.
//! `&&` is an ampersand. macOS has no such letters, and Option with a letter
//! types a character there, so on macOS the mark is only dropped.
//!
//! A letter is matched once the menus are drawn, and the entry it names is
//! clicked at the start of the next frame, as a screen reader's click is.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{
    self, Context, Event, Id, Key, LayerId, Response, Stroke, TextFormat, TextStyle, Ui, Widget,
    WidgetText, accesskit, containers::menu::MenuState, text::LayoutJob,
};

/// Whether this platform has menu accelerators.
const ON: bool = !cfg!(target_os = "macos");

/// An entry drawn this frame with a letter.
#[derive(Clone, Debug)]
struct Entry {
    layer: LayerId,
    letter: char,
    id: Id,
    enabled: bool,
    /// On the menu bar, which Alt reaches, rather than in an open menu.
    bar: bool,
}

/// The entries drawn in one pass.
#[derive(Clone, Debug, Default)]
struct Drawn {
    pass: u64,
    entries: Vec<Entry>,
}

fn drawn_id() -> Id {
    Id::unique("odox-mnemonics")
}

fn pending_id() -> Id {
    Id::unique("odox-mnemonic-pending")
}

/// A label without its mark, and the byte its letter starts at in that
/// label with the letter, lower case.
pub fn parse(text: &str) -> (String, Option<(usize, char)>) {
    let mut shown = String::with_capacity(text.len());
    let mut mark = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '&' {
            shown.push(c);
            continue;
        }
        match chars.next() {
            Some('&') => shown.push('&'),
            Some(next) => {
                if mark.is_none() {
                    mark = Some((shown.len(), next.to_ascii_lowercase()));
                }
                shown.push(next);
            }
            None => {}
        }
    }
    (shown, mark)
}

/// A label without its mark, for where it is not a menu entry: a tooltip,
/// a dialog's title.
pub fn plain(text: &str) -> String {
    parse(text).0
}

/// A label drawn with its letter underlined where `underlined`, in the
/// widget's own colours, and the letter.
fn label(ui: &Ui, text: &str, underlined: bool) -> (WidgetText, Option<char>) {
    let (shown, mark) = parse(text);
    let Some((at, letter)) = mark.filter(|_| ON) else {
        return (shown.into(), None);
    };
    let font_id = TextStyle::Button.resolve(ui.style());
    let plain = TextFormat {
        font_id,
        color: egui::Color32::PLACEHOLDER,
        ..TextFormat::default()
    };
    let marked = TextFormat {
        underline: if underlined {
            Stroke::new(1.0, egui::Color32::PLACEHOLDER)
        } else {
            Stroke::NONE
        },
        ..plain.clone()
    };
    let end = at + shown[at..].chars().next().map_or(0, char::len_utf8);
    let mut job = LayoutJob::default();
    job.append(&shown[..at], 0.0, plain.clone());
    job.append(&shown[at..end], 0.0, marked);
    job.append(&shown[end..], 0.0, plain);
    (job.into(), Some(letter))
}

fn register(ui: &Ui, response: &Response, letter: Option<char>, bar: bool) {
    let Some(letter) = letter else {
        return;
    };
    let pass = ui.ctx().cumulative_pass_nr();
    let entry = Entry {
        layer: response.layer_id,
        letter,
        id: response.id,
        enabled: response.enabled(),
        bar,
    };
    ui.ctx().data_mut(|data| {
        let drawn = data.get_temp_mut_or_default::<Drawn>(drawn_id());
        if drawn.pass != pass {
            *drawn = Drawn {
                pass,
                entries: Vec::new(),
            };
        }
        debug_assert!(
            !drawn
                .entries
                .iter()
                .any(|e| e.layer == entry.layer && e.letter == entry.letter && e.bar == bar),
            "two entries of one menu share the letter {letter:?}"
        );
        drawn.entries.push(entry);
    });
}

/// An entry of a menu, made by `widget` from its label, enabled or not.
pub fn add<W: Widget>(
    ui: &mut Ui,
    enabled: bool,
    text: &str,
    widget: impl FnOnce(WidgetText) -> W,
) -> Response {
    let (label, letter) = label(ui, text, true);
    let response = ui.add_enabled(enabled, widget(label));
    register(ui, &response, letter, false);
    response
}

/// A menu entry that does something when chosen.
pub fn button(ui: &mut Ui, enabled: bool, text: &str) -> Response {
    add(ui, enabled, text, egui::Button::new)
}

/// A menu entry that turns a setting on or off.
pub fn checkbox(ui: &mut Ui, value: &mut bool, text: &str) -> Response {
    add(ui, true, text, |label| egui::Checkbox::new(value, label))
}

/// A menu: on the menu bar, where Alt underlines its letter, or inside
/// another menu. Answers what its contents answered, where it was open.
pub fn menu<R>(
    ui: &mut Ui,
    enabled: bool,
    text: &str,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let bar = !egui::containers::menu::is_in_menu(ui);
    let underlined = !bar || ui.input(|input| input.modifiers.alt && !input.modifiers.ctrl);
    let (label, letter) = label(ui, text, underlined);
    let inner = ui.add_enabled_ui(enabled, |ui| ui.menu_button(label, content));
    register(ui, &inner.inner.response, letter, bar);
    inner.inner.inner
}

/// The letter a key stands for, where it stands for one.
fn letter_of(key: Key) -> Option<char> {
    let mut name = key.name().chars();
    match (name.next(), name.next()) {
        (Some(c), None) if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase()),
        _ => None,
    }
}

/// Match a letter pressed this frame against the menus drawn: Alt with it
/// against the menu bar, and alone against the open menu, which takes
/// every letter typed while it is open. Called once the menus are drawn.
pub fn keys(ctx: &Context) {
    if !ON {
        return;
    }
    let pass = ctx.cumulative_pass_nr();
    let entries = ctx
        .data(|data| data.get_temp::<Drawn>(drawn_id()))
        .filter(|drawn| drawn.pass == pass)
        .map(|drawn| drawn.entries)
        .unwrap_or_default();
    let pressed = ctx.input(|input| {
        input.events.iter().find_map(|event| match event {
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => letter_of(*key).map(|letter| (*key, letter, *modifiers)),
            _ => None,
        })
    });
    let Some((key, letter, modifiers)) = pressed else {
        return;
    };
    let open: Vec<LayerId> = entries
        .iter()
        .filter(|entry| !entry.bar)
        .map(|entry| entry.layer)
        .collect();
    let chosen = |layer: Option<LayerId>, bar: bool| {
        entries
            .iter()
            .find(|entry| {
                entry.bar == bar
                    && entry.enabled
                    && entry.letter == letter
                    && layer.is_none_or(|layer| entry.layer == layer)
            })
            .map(|entry| entry.id)
    };
    let target = if modifiers.alt && !modifiers.ctrl && !modifiers.command {
        let target = chosen(None, true);
        if target.is_some() {
            egui::Popup::close_all(ctx);
        }
        target
    } else if modifiers.is_none() && !open.is_empty() {
        let deepest = open
            .iter()
            .rev()
            .find(|layer| MenuState::is_deepest_open_sub_menu(ctx, layer.id))
            .copied();
        chosen(deepest, false)
    } else {
        return;
    };
    ctx.input_mut(|input| {
        input.events.retain(|event| match event {
            Event::Key { key: pressed, .. } => *pressed != key,
            Event::Text(text) => !text.eq_ignore_ascii_case(&letter.to_string()),
            _ => true,
        });
    });
    if let Some(target) = target {
        ctx.data_mut(|data| data.insert_temp(pending_id(), target));
        ctx.request_repaint();
    }
}

/// Click the entry a letter chose last frame. Called before the menus are
/// drawn.
pub fn deliver(ctx: &Context) {
    let Some(target) = ctx.data_mut(|data| {
        let target = data.get_temp::<Id>(pending_id());
        data.remove::<Id>(pending_id());
        target
    }) else {
        return;
    };
    ctx.input_mut(|input| {
        input
            .events
            .push(Event::AccessKitActionRequest(accesskit::ActionRequest {
                action: accesskit::Action::Click,
                target_tree: accesskit::TreeId::ROOT,
                target_node: target.accesskit_id(),
                data: None,
            }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Modifiers, RawInput};

    #[test]
    fn a_label_loses_its_mark_and_names_its_letter() {
        assert_eq!(parse("Save &As…"), ("Save As…".to_owned(), Some((5, 'a'))));
        assert_eq!(
            parse("Fish && &Chips"),
            ("Fish & Chips".to_owned(), Some((7, 'c')))
        );
        assert_eq!(parse("Plain"), ("Plain".to_owned(), None));
        assert_eq!(plain("Ö&ffnen"), "Öffnen");
        assert_eq!(parse("Ö&ffnen").1, Some((2, 'f')));
    }

    #[test]
    fn every_mark_in_the_catalogues_is_on_a_letter_a_key_types() {
        for (language, catalogue) in crate::i18n::LANGUAGES {
            let entries = catalogue.split("\n\n").filter_map(|entry| {
                let joined = |key: &str| -> Option<String> {
                    let start = entry.find(&format!("\n{key} \""))? + 1;
                    let lines = entry[start + key.len() + 1..].lines();
                    let mut text = String::new();
                    for line in lines {
                        let Some(quoted) = line.strip_prefix('"').and_then(|l| l.strip_suffix('"'))
                        else {
                            break;
                        };
                        text.push_str(quoted);
                    }
                    Some(text)
                };
                Some((joined("msgid")?, joined("msgstr")?))
            });
            for (english, translated) in entries {
                for text in [&english, &translated] {
                    if let Some((at, letter)) = parse(text).1 {
                        assert!(
                            letter.is_ascii_alphanumeric(),
                            "{language}: {text:?} marks {letter:?} at {at}, which no key types"
                        );
                    }
                }
            }
        }
    }

    /// What the frames' menus did: an entry chosen, or File shown open.
    type Chosen = Vec<&'static str>;

    fn key(key: Key, modifiers: Modifiers) -> Vec<Event> {
        vec![
            Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
            Event::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers,
            },
        ]
    }

    fn frame(ctx: &Context, events: Vec<Event>, chosen: &mut Chosen) {
        let input = RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..RawInput::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            deliver(ui.ctx());
            egui::MenuBar::new().ui(ui, |ui| {
                let shown = menu(ui, true, "&File", |ui| {
                    if button(ui, true, "&Save").clicked() {
                        chosen.push("save");
                        ui.close();
                    }
                    if button(ui, false, "&Close").clicked() {
                        chosen.push("close");
                    }
                    menu(ui, true, "&More", |ui| {
                        if button(ui, true, "&Nested").clicked() {
                            chosen.push("nested");
                            ui.close();
                        }
                    });
                });
                if shown.is_some() {
                    chosen.push("file open");
                }
                menu(ui, true, "&Edit", |ui| {
                    button(ui, true, "&Undo");
                });
            });
            keys(ui.ctx());
        });
        output.textures_delta.clear();
    }

    /// A few frames with nothing new in them, for what was asked to happen.
    fn settle(ctx: &Context, chosen: &mut Chosen) {
        for _ in 0..3 {
            frame(ctx, Vec::new(), chosen);
        }
    }

    #[test]
    fn alt_and_a_letter_open_a_menu_and_a_letter_chooses_in_it() {
        let ctx = Context::default();
        let mut chosen = Chosen::new();
        settle(&ctx, &mut chosen);
        assert!(!chosen.contains(&"file open"));
        frame(&ctx, key(Key::F, Modifiers::ALT), &mut chosen);
        settle(&ctx, &mut chosen);
        assert!(chosen.contains(&"file open"), "Alt+F opens File");
        frame(&ctx, key(Key::S, Modifiers::NONE), &mut chosen);
        settle(&ctx, &mut chosen);
        assert!(chosen.contains(&"save"), "S chooses Save");
    }

    #[test]
    fn a_disabled_entry_is_not_chosen() {
        let ctx = Context::default();
        let mut chosen = Chosen::new();
        settle(&ctx, &mut chosen);
        frame(&ctx, key(Key::F, Modifiers::ALT), &mut chosen);
        settle(&ctx, &mut chosen);
        frame(&ctx, key(Key::C, Modifiers::NONE), &mut chosen);
        settle(&ctx, &mut chosen);
        assert!(!chosen.contains(&"close"));
    }

    #[test]
    fn a_letter_opens_a_submenu_and_another_chooses_in_it() {
        let ctx = Context::default();
        let mut chosen = Chosen::new();
        settle(&ctx, &mut chosen);
        frame(&ctx, key(Key::F, Modifiers::ALT), &mut chosen);
        settle(&ctx, &mut chosen);
        frame(&ctx, key(Key::M, Modifiers::NONE), &mut chosen);
        settle(&ctx, &mut chosen);
        frame(&ctx, key(Key::N, Modifiers::NONE), &mut chosen);
        settle(&ctx, &mut chosen);
        assert!(chosen.contains(&"nested"));
    }

    #[test]
    fn a_letter_without_an_open_menu_is_left_for_the_document() {
        let ctx = Context::default();
        let mut chosen = Chosen::new();
        settle(&ctx, &mut chosen);
        frame(&ctx, key(Key::S, Modifiers::NONE), &mut chosen);
        settle(&ctx, &mut chosen);
        assert_eq!(chosen, Chosen::new());
    }
}
