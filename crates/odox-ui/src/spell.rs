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

mod codepages;

use eframe::egui;
/// A dictionary, with the scripts its letters are written in.
pub struct Lexicon {
    dictionary: spellbook::Dictionary,
    /// The scripts of the letters its affix file tries in a suggestion, by
    /// [`script`]; none where it names none, and every word is then checked.
    scripts: Vec<u32>,
}

impl Lexicon {
    /// Whether a word is in letters this dictionary is written in.
    fn writes(&self, word: &str) -> bool {
        self.scripts.is_empty()
            || word
                .chars()
                .filter(|c| c.is_alphabetic())
                .all(|c| self.scripts.contains(&script(c)))
    }
}

/// Which script a letter is in, as the start of its Unicode block, Latin's
/// several blocks taken as one.
fn script(c: char) -> u32 {
    let point = u32::from(c);
    match point {
        0..=0x024F | 0x1E00..=0x1EFF => 0,
        0x0370..=0x03FF | 0x1F00..=0x1FFF => 0x0370,
        0x0400..=0x052F => 0x0400,
        _ => point & !0x7F,
    }
}

/// The most suggestions offered for a word.
const SUGGESTIONS: usize = 5;

/// How many paragraphs' answers are kept before they are all let go.
const REMEMBERED: usize = 4096;

/// A dictionary for one language: being read, read, or not there.
#[derive(Clone)]
enum Loaded {
    Reading,
    Ready(Arc<Lexicon>),
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
    pub fn for_language(&'static self, tag: &str) -> Option<Arc<Lexicon>> {
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
    pub fn dictionary(&self, tag: &str) -> Option<Arc<Lexicon>> {
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
                let aff = std::fs::read(aff).ok()?;
                let set = encoding_of(&aff);
                let aff = decoded(&aff, set.as_deref())?;
                let dic = decoded(&std::fs::read(dic).ok()?, set.as_deref())?;
                let mut scripts: Vec<u32> = aff
                    .lines()
                    .find_map(|line| line.strip_prefix("TRY "))
                    .map(|letters| {
                        letters
                            .chars()
                            .filter(|c| c.is_alphabetic())
                            .map(script)
                            .collect()
                    })
                    .unwrap_or_default();
                scripts.sort_unstable();
                scripts.dedup();
                let dictionary = spellbook::Dictionary::new(&aff, &dic).ok()?;
                Some(Lexicon {
                    dictionary,
                    scripts,
                })
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
    pub fn check(&self, lexicon: &Lexicon, text: &str) -> Vec<Range<usize>> {
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
                    && lexicon.writes(&word)
                    && !lexicon.dictionary.check(&word)
            })
            .collect()
    }

    /// What a dictionary would put in place of a word, the likeliest first.
    pub fn suggest(&self, lexicon: &Lexicon, word: &str) -> Vec<String> {
        let mut found = Vec::new();
        lexicon.dictionary.suggest(word, &mut found);
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

/// A drawn text with the characters in `gaps` made spaces, which are
/// drawn for something that is not the text's own words.
fn blanked(drawn: &str, gaps: &[Range<usize>]) -> Vec<char> {
    drawn
        .chars()
        .enumerate()
        .map(|(index, c)| {
            if gaps.iter().any(|gap| gap.contains(&index)) {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// The words of a drawn text its dictionaries do not know, each with its
/// language, where each language begins at a drawn character; what `gaps`
/// covers is not words.
pub fn wrong_in(
    drawn: &str,
    spoken: &[(usize, Option<String>)],
    gaps: &[Range<usize>],
) -> Vec<(Range<usize>, String)> {
    let speller = Speller::shared();
    let chars = blanked(drawn, gaps);
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
            .map(|folder| bundled(&folder.join("dictionaries")))
            .unwrap_or_default()
    } else if cfg!(target_os = "macos") {
        beside
            .map(|folder| bundled(&folder.join("../Resources/dictionaries")))
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

/// The dictionaries that travel with the application: a folder of them, one
/// folder for each language with its license files beside it.
fn bundled(root: &Path) -> Vec<PathBuf> {
    let mut languages: Vec<PathBuf> = std::fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default();
    languages.sort();
    std::iter::once(root.to_path_buf())
        .chain(languages)
        .collect()
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
        let own_country = |stem: &String| {
            country.is_some_and(|country| stem.starts_with(&format!("{language}_{country}")))
        };
        others.sort_by_key(|stem| (!own_country(stem), stem.clone()));
        if let Some(found) = others.iter().find_map(|stem| pair(folder, stem)) {
            return Some(found);
        }
    }
    None
}

/// The encoding an affix file's `SET` line names, read from its bytes, as
/// the line itself is ASCII whatever the encoding.
fn encoding_of(aff: &[u8]) -> Option<String> {
    aff.split(|&b| b == b'\n').find_map(|line| {
        let line = String::from_utf8_lossy(line);
        line.trim().strip_prefix("SET ").map(|set| {
            set.trim()
                .to_ascii_uppercase()
                .replace("ISO-8859", "ISO8859")
        })
    })
}

/// A dictionary file as text, in the encoding its affix file names: UTF-8,
/// or one of the single-byte code pages Hunspell reads. Another is not read,
/// and the dictionary is taken as missing.
fn decoded(bytes: &[u8], set: Option<&str>) -> Option<String> {
    match set {
        None | Some("UTF-8") => Some(String::from_utf8_lossy(bytes).into_owned()),
        Some(set) => {
            let (_, upper) = codepages::PAGES.iter().find(|(name, _)| *name == set)?;
            let upper: Vec<char> = upper.chars().collect();
            Some(
                bytes
                    .iter()
                    .map(|&b| {
                        if b < 0x80 {
                            char::from(b)
                        } else {
                            upper[usize::from(b - 0x80)]
                        }
                    })
                    .collect(),
            )
        }
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
        let text = "Die Straße ist naß und die Strase lang, die Häuser und Bäume alt.";
        assert_eq!(
            words(text, &speller.check(&dictionary, text)),
            ["naß", "Strase"]
        );
        assert!(
            speller
                .suggest(&dictionary, "Strase")
                .contains(&"Straße".to_owned())
        );
    }

    #[test]
    fn code_pages_decode_their_own_characters() {
        for (name, upper) in super::codepages::PAGES {
            assert_eq!(upper.chars().count(), 128, "{name}");
        }
        assert_eq!(
            super::decoded(&[0xA4], Some("ISO8859-15")).as_deref(),
            Some("€")
        );
        assert_eq!(
            super::decoded(&[0xC4], Some("ISO8859-1")).as_deref(),
            Some("Ä")
        );
        assert_eq!(
            super::decoded(&[0xC1], Some("KOI8-R")).as_deref(),
            Some("а")
        );
        assert_eq!(super::decoded(&[0x80], Some("ISCII-DEVANAGARI")), None);
    }

    #[test]
    fn words_in_another_script_are_not_checked() {
        let speller = Speller::new(vec![dictionaries()], None);
        let dictionary = speller.dictionary("en-US").expect("en_US");
        let text = "Hello 你好世界 and 日本語のテキスト and مرحبا wrold.";
        assert_eq!(words(text, &speller.check(&dictionary, text)), ["wrold"]);
    }

    #[test]
    fn what_stands_for_no_text_is_not_a_word() {
        let chars = super::blanked("Smtih note1tset", &[0..5, 10..11]);
        let text: String = chars.iter().collect();
        assert_eq!(text, "      note tset");
    }

    #[test]
    fn a_country_of_its_own_comes_first() {
        let folder = std::env::temp_dir().join(format!("odox-dicts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("made");
        for stem in ["de_AT_frami", "de_DE_frami"] {
            std::fs::write(folder.join(format!("{stem}.aff")), "SET UTF-8\n").expect("written");
            std::fs::write(folder.join(format!("{stem}.dic")), "1\nHaus\n").expect("written");
        }
        let (aff, _) = super::find(std::slice::from_ref(&folder), "de-DE").expect("found");
        assert!(aff.ends_with("de_DE_frami.aff"));
        std::fs::remove_dir_all(&folder).expect("removed");
    }

    #[test]
    fn bundled_dictionaries_are_found_in_their_language_folders() {
        let bundled =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packaging/dictionaries");
        let speller = Speller::new(super::bundled(&bundled), None);
        assert!(speller.dictionary("en-US").is_some());
        assert!(speller.dictionary("de-DE").is_some());
        assert!(
            speller.dictionary("de-AT").is_some(),
            "the language's other country"
        );
    }
}
