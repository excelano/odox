//! Spell checking: which words of a text a Hunspell dictionary for its
//! language does not know, and what it would put instead. Dictionaries are the
//! system's on Linux and travel with the application on Windows and macOS;
//! each is read on a thread of its own the first time its language is asked
//! for. A person's own words are kept in `words.txt` beside the settings.
//! DESIGN.md §11.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, RwLock};

use std::io::Write as _;

use eframe::egui;
pub use spellbook::Dictionary;

/// The most suggestions offered for a word.
const SUGGESTIONS: usize = 5;

/// How many paragraphs' answers are kept before they are all let go.
const REMEMBERED: usize = 4096;

/// A dictionary for one language: being read, read, or not there.
#[derive(Clone)]
enum Loaded {
    Reading,
    Ready(Arc<Dictionary>),
    Missing,
}

/// A text checked, by its language and the text.
type Checked = (String, String);

/// The dictionaries, the person's own words, and what has been checked.
pub struct Speller {
    folders: Vec<PathBuf>,
    words_file: Option<PathBuf>,
    loaded: Mutex<HashMap<String, Loaded>>,
    own: RwLock<HashSet<String>>,
    ignored: RwLock<HashSet<String>>,
    checked: Mutex<HashMap<Checked, Vec<Range<usize>>>>,
    waker: OnceLock<egui::Context>,
}

impl Speller {
    /// A speller over dictionaries in these folders, with the person's words
    /// kept in a file.
    pub fn new(folders: Vec<PathBuf>, words_file: Option<PathBuf>) -> Self {
        let own = words_file
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| {
                text.lines()
                    .map(str::trim)
                    .filter(|word| !word.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            folders,
            words_file,
            loaded: Mutex::new(HashMap::new()),
            own: RwLock::new(own),
            ignored: RwLock::new(HashSet::new()),
            checked: Mutex::new(HashMap::new()),
            waker: OnceLock::new(),
        }
    }

    /// The application's speller, over the platform's dictionaries.
    pub fn shared() -> &'static Self {
        static SHARED: OnceLock<Speller> = OnceLock::new();
        SHARED.get_or_init(|| {
            let words =
                crate::settings::path().map(|settings| settings.with_file_name("words.txt"));
            Self::new(folders(), words)
        })
    }

    /// Wake this context when a dictionary has been read.
    pub fn wake(&self, ctx: &egui::Context) {
        let _ = self.waker.set(ctx.clone());
    }

    /// The dictionary for a language, if it has been read; the first time a
    /// language is asked for, it is read on a thread of its own.
    pub fn for_language(&'static self, tag: &str) -> Option<Arc<Dictionary>> {
        let mut loaded = self.loaded.lock().unwrap_or_else(PoisonError::into_inner);
        match loaded.get(tag) {
            Some(Loaded::Ready(dictionary)) => return Some(Arc::clone(dictionary)),
            Some(_) => return None,
            None => {}
        }
        loaded.insert(tag.to_owned(), Loaded::Reading);
        let tag = tag.to_owned();
        std::thread::spawn(move || {
            let read = self.read(&tag);
            self.loaded
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(tag, read);
            if let Some(ctx) = self.waker.get() {
                ctx.request_repaint();
            }
        });
        None
    }

    /// The dictionary for a language, read now where it has not been.
    pub fn dictionary(&self, tag: &str) -> Option<Arc<Dictionary>> {
        let mut loaded = self.loaded.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(Loaded::Ready(dictionary)) = loaded.get(tag) {
            return Some(Arc::clone(dictionary));
        }
        let read = self.read(tag);
        loaded.insert(tag.to_owned(), read.clone());
        match read {
            Loaded::Ready(dictionary) => Some(dictionary),
            _ => None,
        }
    }

    fn read(&self, tag: &str) -> Loaded {
        find(&self.folders, tag)
            .and_then(|(aff, dic)| {
                let aff = decoded(&std::fs::read(aff).ok()?, None)?;
                let set = aff
                    .lines()
                    .find_map(|line| line.strip_prefix("SET "))
                    .map(str::trim);
                let dic = decoded(&std::fs::read(dic).ok()?, set)?;
                Dictionary::new(&aff, &dic).ok()
            })
            .map_or(Loaded::Missing, |dictionary| {
                Loaded::Ready(Arc::new(dictionary))
            })
    }

    /// The words of a text in a language that its dictionary does not know,
    /// as character ranges; nothing while the dictionary is being read, or
    /// where there is none.
    pub fn misspelled(&'static self, text: &str, tag: &str) -> Vec<Range<usize>> {
        let key = (tag.to_owned(), text.to_owned());
        if let Some(found) = self
            .checked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            return found.clone();
        }
        let Some(dictionary) = self.for_language(tag) else {
            return Vec::new();
        };
        let found = self.check(&dictionary, text);
        let mut checked = self.checked.lock().unwrap_or_else(PoisonError::into_inner);
        if checked.len() >= REMEMBERED {
            checked.clear();
        }
        checked.insert(key, found.clone());
        found
    }

    /// The words of a text a dictionary does not know, leaving out numbers,
    /// addresses and the person's own words.
    pub fn check(&self, dictionary: &Dictionary, text: &str) -> Vec<Range<usize>> {
        let skipped = addresses(text);
        let chars: Vec<char> = text.chars().collect();
        let own = self.own.read().unwrap_or_else(PoisonError::into_inner);
        let ignored = self.ignored.read().unwrap_or_else(PoisonError::into_inner);
        odox_core::words::word_ranges(text)
            .into_iter()
            .filter(|range| {
                !skipped
                    .iter()
                    .any(|skip| skip.start <= range.start && range.end <= skip.end)
            })
            .filter(|range| {
                let word: String = chars[range.clone()].iter().collect();
                !word.chars().any(char::is_numeric)
                    && !own.contains(&word)
                    && !ignored.contains(&word)
                    && !dictionary.check(&word)
            })
            .collect()
    }

    /// What a dictionary would put in place of a word, the likeliest first.
    pub fn suggest(&self, dictionary: &Dictionary, word: &str) -> Vec<String> {
        let mut found = Vec::new();
        dictionary.suggest(word, &mut found);
        found.truncate(SUGGESTIONS);
        found
    }

    /// Take a word as spelled from now on, and keep it in the person's words.
    ///
    /// # Errors
    ///
    /// The words file could not be written.
    pub fn add(&self, word: &str) -> std::io::Result<()> {
        self.own
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(word.to_owned());
        self.forget();
        let Some(path) = &self.words_file else {
            return Ok(());
        };
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(file, "{word}")
    }

    /// Take a word as spelled until the application closes.
    pub fn ignore(&self, word: &str) {
        self.ignored
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(word.to_owned());
        self.forget();
    }

    fn forget(&self) {
        self.checked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}

/// The words of a drawn text its dictionaries do not know, each with its
/// language, where each language begins at a drawn character.
pub fn wrong_in(drawn: &str, spoken: &[(usize, Option<String>)]) -> Vec<(Range<usize>, String)> {
    let speller = Speller::shared();
    let chars: Vec<char> = drawn.chars().collect();
    let mut wrong = Vec::new();
    for (index, (start, language)) in spoken.iter().enumerate() {
        let end = spoken.get(index + 1).map_or(chars.len(), |next| next.0);
        let Some(language) = language.as_deref().filter(|_| *start < end) else {
            continue;
        };
        let part: String = chars[*start..end].iter().collect();
        for range in speller.misspelled(&part, language) {
            wrong.push((start + range.start..start + range.end, language.to_owned()));
        }
    }
    wrong
}

/// A word a right click asked about: where it is drawn, what it is, and what
/// its dictionary would put instead.
struct Asked {
    range: Range<usize>,
    word: String,
    offered: Vec<String>,
}

/// Draw a red wave under each wrong word of a galley drawn at `origin`, and on
/// a right click over one, offer what to put instead, or to take it as
/// spelled. Answers the drawn characters to replace and what with, once
/// chosen.
pub fn mark(
    ui: &mut egui::Ui,
    response: &egui::Response,
    (galley, origin): (&egui::Galley, egui::Pos2),
    drawn: &str,
    wrong: &[(Range<usize>, String)],
    zoom: f32,
) -> Option<(Range<usize>, String)> {
    if wrong.is_empty() {
        return None;
    }
    let speller = Speller::shared();
    speller.wake(ui.ctx());
    if ui.is_rect_visible(response.rect) {
        for (range, _) in wrong {
            wave(ui, galley, origin, range.clone(), zoom);
        }
    }
    let asked = response.id.with("spelling");
    if response.secondary_clicked()
        && let Some(at) = response.interact_pointer_pos()
    {
        let index = galley.cursor_from_pos(at - origin).index.0;
        let chars: Vec<char> = drawn.chars().collect();
        let hit = wrong
            .iter()
            .find(|(range, _)| range.start <= index && index <= range.end)
            .and_then(|(range, language)| {
                let word: String = chars.get(range.clone())?.iter().collect();
                let dictionary = speller.for_language(language)?;
                let offered = speller.suggest(&dictionary, &word);
                Some(Arc::new(Asked {
                    range: range.clone(),
                    word,
                    offered,
                }))
            });
        ui.data_mut(|data| match hit {
            Some(hit) => {
                data.insert_temp(asked, hit);
            }
            None => data.remove::<Arc<Asked>>(asked),
        });
    }
    let hit = ui.data(|data| data.get_temp::<Arc<Asked>>(asked))?;
    let mut chosen = None;
    response.context_menu(|ui| {
        for offered in &hit.offered {
            if ui.button(offered).clicked() {
                chosen = Some((hit.range.clone(), offered.clone()));
                ui.close();
            }
        }
        if !hit.offered.is_empty() {
            ui.separator();
        }
        if ui.button(crate::i18n::t("Add to dictionary")).clicked() {
            let _ = speller.add(&hit.word);
            ui.close();
        }
        if ui.button(crate::i18n::t("Ignore")).clicked() {
            speller.ignore(&hit.word);
            ui.close();
        }
    });
    chosen
}

/// A red wave under the drawn characters of a range, row by row.
fn wave(ui: &egui::Ui, galley: &egui::Galley, origin: egui::Pos2, range: Range<usize>, zoom: f32) {
    use egui::text::CCursor;
    let mut rows: Vec<(f32, f32, f32)> = Vec::new();
    for index in range {
        let from = galley.pos_from_cursor(CCursor::new(index));
        let to = galley.pos_from_cursor(CCursor::new(index + 1));
        let bottom = from.max.y;
        match rows.last_mut() {
            Some((y, _, right)) if (*y - bottom).abs() < 0.5 => *right = to.min.x.max(*right),
            _ => rows.push((bottom, from.min.x, to.min.x.max(from.min.x))),
        }
    }
    let height = (1.5 * zoom).clamp(1.0, 3.0);
    let stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(0xe0, 0x1b, 0x24));
    for (y, left, right) in rows {
        let mut points = Vec::new();
        let mut x = left;
        let mut up = false;
        while x <= right {
            let dy = if up { -height } else { 0.0 };
            points.push(origin + egui::vec2(x, y + dy));
            x += height * 1.5;
            up = !up;
        }
        if points.len() > 1 {
            ui.painter().add(egui::Shape::line(points, stroke));
        }
    }
}

/// Where dictionaries are looked for: the system's on Linux, the user's own
/// beside them, and the ones that travel with the application elsewhere.
fn folders() -> Vec<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|program| program.parent().map(Path::to_path_buf));
    if cfg!(target_os = "windows") {
        beside
            .map(|folder| vec![folder.join("dictionaries")])
            .unwrap_or_default()
    } else if cfg!(target_os = "macos") {
        beside
            .map(|folder| vec![folder.join("../Resources/dictionaries")])
            .unwrap_or_default()
    } else {
        let data = match std::env::var_os("XDG_DATA_HOME") {
            Some(data) if !data.is_empty() => Some(PathBuf::from(data)),
            _ => std::env::home_dir().map(|home| home.join(".local/share")),
        };
        let mut folders: Vec<PathBuf> =
            data.map(|data| data.join("hunspell")).into_iter().collect();
        folders.extend(
            [
                "/usr/share/hunspell",
                "/usr/share/myspell/dicts",
                "/usr/share/myspell",
            ]
            .map(PathBuf::from),
        );
        folders
    }
}

/// The affix and word files for a language tag: `de_AT` for `de-AT`, then
/// `de`, then the first `de_` of any country.
fn find(folders: &[PathBuf], tag: &str) -> Option<(PathBuf, PathBuf)> {
    let mut parts = tag.split(['-', '_']);
    let language = parts
        .next()
        .filter(|language| !language.is_empty() && *language != "zxx")?;
    let country = parts.next();
    let pair = |folder: &Path, stem: &str| {
        let (aff, dic) = (
            folder.join(format!("{stem}.aff")),
            folder.join(format!("{stem}.dic")),
        );
        (aff.is_file() && dic.is_file()).then_some((aff, dic))
    };
    let mut stems: Vec<String> = country
        .map(|country| format!("{language}_{country}"))
        .into_iter()
        .chain(std::iter::once(language.to_owned()))
        .collect();
    for folder in folders {
        for stem in &stems {
            if let Some(found) = pair(folder, stem) {
                return Some(found);
            }
        }
    }
    stems.clear();
    for folder in folders {
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        let mut others: Vec<String> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let stem = name.strip_suffix(".aff")?;
                stem.starts_with(&format!("{language}_"))
                    .then(|| stem.to_owned())
            })
            .collect();
        others.sort();
        if let Some(found) = others.iter().find_map(|stem| pair(folder, stem)) {
            return Some(found);
        }
    }
    None
}

/// A dictionary file as text, in the encoding its affix file names: UTF-8,
/// or one byte to a character for the Latin ones.
fn decoded(bytes: &[u8], set: Option<&str>) -> Option<String> {
    match set.map(str::to_ascii_uppercase).as_deref() {
        None | Some("UTF-8") => Some(String::from_utf8_lossy(bytes).into_owned()),
        Some("ISO8859-1" | "ISO-8859-1" | "ISO8859-15" | "ISO-8859-15") => {
            Some(bytes.iter().map(|&b| char::from(b)).collect())
        }
        Some(_) => None,
    }
}

/// The character ranges of the addresses in a text, web and mail, which are
/// not words.
fn addresses(text: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut at = 0;
    for token in text.split(char::is_whitespace) {
        let length = token.chars().count();
        if token.contains("://") || token.starts_with("www.") || token.contains('@') {
            found.push(at..at + length);
        }
        at += length + 1;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::Speller;
    use std::path::PathBuf;

    fn dictionaries() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packaging/dictionaries/en_US")
    }

    fn words(text: &str, ranges: &[std::ops::Range<usize>]) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        ranges
            .iter()
            .map(|range| chars[range.clone()].iter().collect())
            .collect()
    }

    #[test]
    fn finds_misspelled_word() {
        let speller = Speller::new(vec![dictionaries()], None);
        let dictionary = speller.dictionary("en-US").expect("en_US");
        let text = "This sentense is spelled well enough.";
        assert_eq!(words(text, &speller.check(&dictionary, text)), ["sentense"]);
        assert!(
            speller
                .suggest(&dictionary, "sentense")
                .contains(&"sentence".to_owned())
        );
    }

    #[test]
    fn language_falls_back_to_language_only() {
        let speller = Speller::new(vec![dictionaries()], None);
        assert!(speller.dictionary("en-GB").is_some());
        assert!(speller.dictionary("en").is_some());
    }

    #[test]
    fn unknown_language_checks_nothing() {
        let speller = Speller::new(vec![dictionaries()], None);
        assert!(speller.dictionary("xx-YY").is_none());
        assert!(speller.dictionary("zxx").is_none());
    }

    #[test]
    fn added_word_not_flagged() {
        let file = std::env::temp_dir().join(format!("odox-words-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let speller = Speller::new(vec![dictionaries()], Some(file.clone()));
        let dictionary = speller.dictionary("en-US").expect("en_US");
        let text = "Odox reads it.";
        assert_eq!(words(text, &speller.check(&dictionary, text)), ["Odox"]);
        speller.add("Odox").expect("added");
        assert_eq!(
            speller.check(&dictionary, text),
            Vec::<std::ops::Range<usize>>::new()
        );
        let again = Speller::new(vec![dictionaries()], Some(file.clone()));
        assert!(
            again.check(&dictionary, text).is_empty(),
            "the word is kept"
        );
        std::fs::remove_file(&file).expect("removed");
    }

    #[test]
    fn numbers_and_urls_skipped() {
        let speller = Speller::new(vec![dictionaries()], None);
        let dictionary = speller.dictionary("en-US").expect("en_US");
        let text = "See https://exmple.com/pagez or mail sombody@exmple.org about 3rd and 42x.";
        assert_eq!(
            speller.check(&dictionary, text),
            Vec::<std::ops::Range<usize>>::new()
        );
    }

    #[test]
    fn a_latin_dictionary_is_read() {
        let german =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packaging/dictionaries/de_DE");
        let speller = Speller::new(vec![german], None);
        let dictionary = speller.dictionary("de-DE").expect("de_DE");
        let text = "Die Straße ist naß und die Strase lang.";
        assert_eq!(
            words(text, &speller.check(&dictionary, text)),
            ["naß", "Strase"]
        );
    }
}
