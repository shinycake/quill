//! Message translation: the language table, the local language guess that
//! drives the translate bar, the user's translation preferences, and the
//! small decisions Telegram Desktop makes around them.
//!
//! TDLib translates (`translateMessageText` / `translateText`) but offers no
//! language recognition: Telegram Desktop recognizes each message's language
//! with the platform's recognizer (`Platform::Language::Recognize`) and offers
//! translation when enough recent messages share a language the user did not
//! mark as "Do Not Translate" (`HistoryView::TranslateTracker::checkRecognized`).
//! Quill has no platform recognizer on every OS, so [`detect_language`] is a
//! small dependency-free guess: Unicode script first, then common-word
//! scoring for the Latin and Cyrillic scripts. It returns `None` rather than
//! guess when the text is short or ambiguous.

use crate::telegram::envelope::MessageContent;
use crate::text::TextEntity;
use serde::{Deserialize, Serialize};

/// Languages `translateText.to_language_code` accepts, as
/// `(code, English name, native name)`. A subset of the schema's list
/// (`schema/td_api.tl`, `translateText`) without the legacy alias codes
/// ("iw", "in", "ji", "zh", "zh-Hans", "zh-Hant") that TDLib also accepts.
pub const LANGUAGES: &[(&str, &str, &str)] = &[
    ("af", "Afrikaans", "Afrikaans"),
    ("sq", "Albanian", "Shqip"),
    ("am", "Amharic", "አማርኛ"),
    ("ar", "Arabic", "العربية"),
    ("hy", "Armenian", "Հայերեն"),
    ("az", "Azerbaijani", "Azərbaycanca"),
    ("eu", "Basque", "Euskara"),
    ("be", "Belarusian", "Беларуская"),
    ("bn", "Bengali", "বাংলা"),
    ("bs", "Bosnian", "Bosanski"),
    ("bg", "Bulgarian", "Български"),
    ("ca", "Catalan", "Català"),
    ("ceb", "Cebuano", "Cebuano"),
    ("zh-CN", "Chinese (Simplified)", "简体中文"),
    ("zh-TW", "Chinese (Traditional)", "繁體中文"),
    ("co", "Corsican", "Corsu"),
    ("hr", "Croatian", "Hrvatski"),
    ("cs", "Czech", "Čeština"),
    ("da", "Danish", "Dansk"),
    ("nl", "Dutch", "Nederlands"),
    ("en", "English", "English"),
    ("eo", "Esperanto", "Esperanto"),
    ("et", "Estonian", "Eesti"),
    ("fi", "Finnish", "Suomi"),
    ("fr", "French", "Français"),
    ("fy", "Frisian", "Frysk"),
    ("gl", "Galician", "Galego"),
    ("ka", "Georgian", "ქართული"),
    ("de", "German", "Deutsch"),
    ("el", "Greek", "Ελληνικά"),
    ("gu", "Gujarati", "ગુજરાતી"),
    ("ht", "Haitian Creole", "Kreyòl ayisyen"),
    ("ha", "Hausa", "Hausa"),
    ("haw", "Hawaiian", "ʻŌlelo Hawaiʻi"),
    ("he", "Hebrew", "עברית"),
    ("hi", "Hindi", "हिन्दी"),
    ("hmn", "Hmong", "Hmoob"),
    ("hu", "Hungarian", "Magyar"),
    ("is", "Icelandic", "Íslenska"),
    ("ig", "Igbo", "Igbo"),
    ("id", "Indonesian", "Bahasa Indonesia"),
    ("ga", "Irish", "Gaeilge"),
    ("it", "Italian", "Italiano"),
    ("ja", "Japanese", "日本語"),
    ("jv", "Javanese", "Basa Jawa"),
    ("kn", "Kannada", "ಕನ್ನಡ"),
    ("kk", "Kazakh", "Қазақша"),
    ("km", "Khmer", "ខ្មែរ"),
    ("rw", "Kinyarwanda", "Kinyarwanda"),
    ("ko", "Korean", "한국어"),
    ("ku", "Kurdish", "Kurdî"),
    ("ky", "Kyrgyz", "Кыргызча"),
    ("lo", "Lao", "ລາວ"),
    ("la", "Latin", "Latina"),
    ("lv", "Latvian", "Latviešu"),
    ("lt", "Lithuanian", "Lietuvių"),
    ("lb", "Luxembourgish", "Lëtzebuergesch"),
    ("mk", "Macedonian", "Македонски"),
    ("mg", "Malagasy", "Malagasy"),
    ("ms", "Malay", "Bahasa Melayu"),
    ("ml", "Malayalam", "മലയാളം"),
    ("mt", "Maltese", "Malti"),
    ("mi", "Maori", "Māori"),
    ("mr", "Marathi", "मराठी"),
    ("mn", "Mongolian", "Монгол"),
    ("my", "Myanmar (Burmese)", "မြန်မာ"),
    ("ne", "Nepali", "नेपाली"),
    ("no", "Norwegian", "Norsk"),
    ("ny", "Nyanja (Chichewa)", "Chichewa"),
    ("or", "Odia (Oriya)", "ଓଡ଼ିଆ"),
    ("ps", "Pashto", "پښتو"),
    ("fa", "Persian", "فارسی"),
    ("pl", "Polish", "Polski"),
    ("pt", "Portuguese", "Português"),
    ("pa", "Punjabi", "ਪੰਜਾਬੀ"),
    ("ro", "Romanian", "Română"),
    ("ru", "Russian", "Русский"),
    ("sm", "Samoan", "Gagana Sāmoa"),
    ("gd", "Scots Gaelic", "Gàidhlig"),
    ("sr", "Serbian", "Српски"),
    ("st", "Sesotho", "Sesotho"),
    ("sn", "Shona", "chiShona"),
    ("sd", "Sindhi", "سنڌي"),
    ("si", "Sinhala", "සිංහල"),
    ("sk", "Slovak", "Slovenčina"),
    ("sl", "Slovenian", "Slovenščina"),
    ("so", "Somali", "Soomaali"),
    ("es", "Spanish", "Español"),
    ("su", "Sundanese", "Basa Sunda"),
    ("sw", "Swahili", "Kiswahili"),
    ("sv", "Swedish", "Svenska"),
    ("tl", "Tagalog (Filipino)", "Tagalog"),
    ("tg", "Tajik", "Тоҷикӣ"),
    ("ta", "Tamil", "தமிழ்"),
    ("tt", "Tatar", "Татарча"),
    ("te", "Telugu", "తెలుగు"),
    ("th", "Thai", "ไทย"),
    ("tr", "Turkish", "Türkçe"),
    ("tk", "Turkmen", "Türkmençe"),
    ("uk", "Ukrainian", "Українська"),
    ("ur", "Urdu", "اردو"),
    ("ug", "Uyghur", "ئۇيغۇرچە"),
    ("uz", "Uzbek", "Oʻzbekcha"),
    ("vi", "Vietnamese", "Tiếng Việt"),
    ("cy", "Welsh", "Cymraeg"),
    ("xh", "Xhosa", "isiXhosa"),
    ("yi", "Yiddish", "ייִדיש"),
    ("yo", "Yoruba", "Yorùbá"),
    ("zu", "Zulu", "isiZulu"),
];

/// The code the rest of the module uses for a language tag: a known code
/// stays as is; the regional and legacy tags TDLib, the OS or Telegram's
/// own language packs report ("zh", "pt-BR", "iw", "en-US") map onto the
/// table. `None` for a tag with no translation target.
pub fn normalize_code(tag: &str) -> Option<&'static str> {
    let tag = tag.trim().replace('_', "-");
    let lower = tag.to_ascii_lowercase();
    let mapped = match lower.as_str() {
        "zh" | "zh-cn" | "zh-hans" | "zh-sg" => "zh-CN",
        "zh-tw" | "zh-hant" | "zh-hk" => "zh-TW",
        "iw" => "he",
        "in" => "id",
        "ji" => "yi",
        "nb" | "nn" => "no",
        "fil" => "tl",
        other => other.split('-').next().unwrap_or(other),
    };
    LANGUAGES
        .iter()
        .find(|(code, _, _)| code.eq_ignore_ascii_case(mapped))
        .map(|(code, _, _)| *code)
}

/// English name of a language code ("Russian"); the code itself when it is
/// not in the table.
pub fn language_name(code: &str) -> String {
    normalize_code(code)
        .and_then(|code| LANGUAGES.iter().find(|(c, _, _)| *c == code))
        .map(|(_, english, _)| (*english).to_string())
        .unwrap_or_else(|| code.to_string())
}

/// Native name of a language code ("Русский"); the English name when the
/// two are the same, and the code when the language is unknown.
pub fn language_native_name(code: &str) -> String {
    normalize_code(code)
        .and_then(|code| LANGUAGES.iter().find(|(c, _, _)| *c == code))
        .map(|(_, _, native)| (*native).to_string())
        .unwrap_or_else(|| code.to_string())
}

/// The languages whose English or native name (or code) contains `query`,
/// case-insensitively; every language for an empty query.
pub fn search_languages(query: &str) -> Vec<(&'static str, &'static str, &'static str)> {
    let needle = query.trim().to_lowercase();
    LANGUAGES
        .iter()
        .filter(|(code, english, native)| {
            needle.is_empty()
                || english.to_lowercase().contains(&needle)
                || native.to_lowercase().contains(&needle)
                || code.to_lowercase().contains(&needle)
        })
        .copied()
        .collect()
}

// ---------------------------------------------------------------------------
// Language guess
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct ScriptCounts {
    latin: u32,
    cyrillic: u32,
    hebrew: u32,
    arabic: u32,
    greek: u32,
    hangul: u32,
    kana: u32,
    han: u32,
    thai: u32,
    devanagari: u32,
    bengali: u32,
    tamil: u32,
    telugu: u32,
    kannada: u32,
    malayalam: u32,
    gujarati: u32,
    gurmukhi: u32,
    georgian: u32,
    armenian: u32,
    khmer: u32,
    lao: u32,
    myanmar: u32,
    sinhala: u32,
    ethiopic: u32,
}

impl ScriptCounts {
    fn add(&mut self, c: char) {
        let slot = match c as u32 {
            0x41..=0x5A | 0x61..=0x7A | 0xC0..=0x24F | 0x1E00..=0x1EFF => &mut self.latin,
            0x370..=0x3FF | 0x1F00..=0x1FFF => &mut self.greek,
            0x400..=0x52F => &mut self.cyrillic,
            0x531..=0x58F => &mut self.armenian,
            0x590..=0x5FF => &mut self.hebrew,
            0x600..=0x6FF | 0x750..=0x77F | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => &mut self.arabic,
            0x900..=0x97F => &mut self.devanagari,
            0x980..=0x9FF => &mut self.bengali,
            0xA00..=0xA7F => &mut self.gurmukhi,
            0xA80..=0xAFF => &mut self.gujarati,
            0xB80..=0xBFF => &mut self.tamil,
            0xC00..=0xC7F => &mut self.telugu,
            0xC80..=0xCFF => &mut self.kannada,
            0xD00..=0xD7F => &mut self.malayalam,
            0xD80..=0xDFF => &mut self.sinhala,
            0xE00..=0xE7F => &mut self.thai,
            0xE80..=0xEFF => &mut self.lao,
            0x1000..=0x109F => &mut self.myanmar,
            0x10A0..=0x10FF => &mut self.georgian,
            0x1200..=0x137F => &mut self.ethiopic,
            0x1780..=0x17FF => &mut self.khmer,
            0x3040..=0x30FF => &mut self.kana,
            0x4E00..=0x9FFF | 0x3400..=0x4DBF => &mut self.han,
            0xAC00..=0xD7AF | 0x1100..=0x11FF | 0x3130..=0x318F => &mut self.hangul,
            _ => return,
        };
        *slot += 1;
    }
}

/// Words that mark a Latin-script language, by language. Chosen to be
/// frequent and as unambiguous as possible between the listed languages.
const LATIN_WORDS: &[(&str, &[&str])] = &[
    (
        "en",
        &[
            "the", "and", "is", "are", "you", "that", "with", "for", "have", "this", "not", "but",
            "what", "was", "they", "from", "will", "would", "there", "been",
        ],
    ),
    (
        "es",
        &[
            "el", "los", "las", "que", "una", "por", "con", "para", "como", "pero", "muy", "está",
            "esto", "son", "del", "más", "también", "hola", "gracias", "yo",
        ],
    ),
    (
        "fr",
        &[
            "le", "les", "des", "est", "une", "que", "pour", "pas", "dans", "avec", "vous", "nous",
            "sont", "mais", "très", "bonjour", "merci", "je", "ce", "qui",
        ],
    ),
    (
        "de",
        &[
            "der", "die", "das", "und", "ist", "nicht", "ich", "mit", "ein", "eine", "für", "auf",
            "sie", "wir", "auch", "aber", "wie", "dass", "hallo", "danke",
        ],
    ),
    (
        "it",
        &[
            "il", "che", "non", "per", "una", "sono", "con", "della", "questo", "anche", "come",
            "ma", "più", "ciao", "grazie", "gli", "nel", "alla", "molto", "perché",
        ],
    ),
    (
        "pt",
        &[
            "não", "uma", "que", "para", "com", "são", "você", "mais", "muito", "isso", "como",
            "mas", "obrigado", "olá", "está", "então", "também", "dos", "das", "pelo",
        ],
    ),
    (
        "nl",
        &[
            "het", "een", "van", "dat", "niet", "met", "zijn", "voor", "maar", "ook", "dit", "wat",
            "hallo", "dank", "ik", "je", "hebben", "naar", "heel", "gaat",
        ],
    ),
    (
        "pl",
        &[
            "nie",
            "się",
            "jest",
            "to",
            "na",
            "że",
            "jak",
            "ale",
            "dla",
            "tak",
            "czy",
            "co",
            "bardzo",
            "cześć",
            "dziękuję",
            "już",
            "tylko",
            "może",
            "który",
            "przez",
        ],
    ),
    (
        "tr",
        &[
            "bir",
            "ve",
            "bu",
            "için",
            "ile",
            "değil",
            "çok",
            "ama",
            "ben",
            "sen",
            "var",
            "daha",
            "merhaba",
            "teşekkür",
            "evet",
            "hayır",
            "gibi",
            "kadar",
            "olarak",
            "ne",
        ],
    ),
    (
        "sv",
        &[
            "och", "att", "det", "som", "är", "inte", "jag", "med", "för", "på", "men", "har",
            "hej", "tack", "ett", "den", "vi", "kan", "från", "också",
        ],
    ),
    (
        "da",
        &[
            "og", "at", "det", "som", "er", "ikke", "jeg", "med", "for", "på", "men", "har", "tak",
            "hej", "af", "til", "vi", "kan", "fra", "også",
        ],
    ),
    (
        "ro",
        &[
            "și",
            "este",
            "nu",
            "de",
            "să",
            "un",
            "pentru",
            "cu",
            "mai",
            "dar",
            "foarte",
            "mulțumesc",
            "bună",
            "sunt",
            "care",
            "în",
            "pe",
            "ce",
            "am",
            "acest",
        ],
    ),
    (
        "cs",
        &[
            "je", "se", "na", "to", "že", "ale", "jak", "pro", "ano", "ne", "co", "byl", "jsem",
            "děkuji", "ahoj", "velmi", "také", "nebo", "tak", "už",
        ],
    ),
    (
        "id",
        &[
            "yang", "dan", "di", "itu", "ini", "dengan", "untuk", "tidak", "ada", "saya", "anda",
            "dari", "akan", "juga", "terima", "kasih", "halo", "apa", "bisa", "sudah",
        ],
    ),
    (
        "vi",
        &[
            "và", "của", "là", "không", "có", "những", "một", "được", "cho", "tôi", "bạn", "này",
            "xin", "chào", "cảm", "ơn", "rất", "đã", "với", "người",
        ],
    ),
];

/// Cyrillic languages told apart by letters that exist in one of them.
fn cyrillic_language(text: &str) -> &'static str {
    let has = |letters: &str| text.chars().any(|c| letters.contains(c));
    if has("іїєґІЇЄҐ") && !has("ыэЫЭъЪ") {
        "uk"
    } else if has("ўЎ") {
        "be"
    } else if has("ђћљњџЂЋЉЊЏ") {
        "sr"
    } else if has("ѓќѕЃЌЅ") {
        "mk"
    } else if has("әөүұқңһӘӨҮҰҚҢҺ") {
        "kk"
    } else {
        "ru"
    }
}

fn arabic_language(text: &str) -> &'static str {
    let has = |letters: &str| text.chars().any(|c| letters.contains(c));
    if has("ټډړږښګڼ") {
        "ps"
    } else if has("ٹڈڑںےھ") {
        "ur"
    } else if has("پچژگکی") {
        "fa"
    } else {
        "ar"
    }
}

fn latin_language(text: &str) -> Option<&'static str> {
    let mut scores = [0u32; LATIN_WORDS.len()];
    let mut tokens = 0u32;
    for token in text
        .split(|c: char| !c.is_alphabetic())
        .filter(|token| !token.is_empty())
    {
        tokens += 1;
        let lower = token.to_lowercase();
        for (index, (_, words)) in LATIN_WORDS.iter().enumerate() {
            if words.contains(&lower.as_str()) {
                scores[index] += 1;
            }
        }
    }
    // Diacritics only a few Latin languages use.
    for c in text.chars() {
        match c {
            'ñ' | '¿' | '¡' => bump(&mut scores, "es", 2),
            'ß' => bump(&mut scores, "de", 2),
            'ä' | 'ö' | 'ü' => bump(&mut scores, "de", 1),
            'ç' | 'œ' | 'ê' | 'è' | 'ù' => bump(&mut scores, "fr", 1),
            'ã' | 'õ' => bump(&mut scores, "pt", 2),
            'ł' | 'ą' | 'ę' | 'ż' | 'ź' | 'ś' | 'ć' | 'ń' => bump(&mut scores, "pl", 2),
            'ğ' | 'ı' | 'ş' => bump(&mut scores, "tr", 2),
            'ă' | 'ț' => bump(&mut scores, "ro", 2),
            'ř' | 'ě' | 'ů' => bump(&mut scores, "cs", 2),
            'ệ' | 'ố' | 'ư' | 'ơ' | 'đ' | 'ạ' | 'ả' | 'ế' | 'ề' | 'ể' => {
                bump(&mut scores, "vi", 2)
            }
            'å' => {
                bump(&mut scores, "sv", 1);
                bump(&mut scores, "da", 1);
            }
            'ø' | 'æ' => bump(&mut scores, "da", 2),
            _ => {}
        }
    }
    let (best_index, best) = scores
        .iter()
        .copied()
        .enumerate()
        .max_by_key(|(_, score)| *score)?;
    let runner_up = scores
        .iter()
        .copied()
        .enumerate()
        .filter(|(index, _)| *index != best_index)
        .map(|(_, score)| score)
        .max()
        .unwrap_or(0);
    // Two clear hits at least, and clearly ahead of the next language.
    let enough = best >= 2 && (best * 2 > tokens.min(40) / 2 || tokens <= 6);
    (enough && best > runner_up).then(|| LATIN_WORDS[best_index].0)
}

fn bump(scores: &mut [u32], code: &str, by: u32) {
    if let Some(index) = LATIN_WORDS.iter().position(|(c, _)| *c == code) {
        scores[index] += by;
    }
}

/// Guess the language of `text` (a code from [`LANGUAGES`]), or `None` when
/// the text has too few letters or the guess would be a coin flip. Only the
/// first 400 characters are read.
pub fn detect_language(text: &str) -> Option<&'static str> {
    let sample: String = text.chars().take(400).collect();
    let mut counts = ScriptCounts::default();
    for c in sample.chars().filter(|c| c.is_alphabetic()) {
        counts.add(c);
    }
    let total = counts.latin
        + counts.cyrillic
        + counts.hebrew
        + counts.arabic
        + counts.greek
        + counts.hangul
        + counts.kana
        + counts.han
        + counts.thai
        + counts.devanagari
        + counts.bengali
        + counts.tamil
        + counts.telugu
        + counts.kannada
        + counts.malayalam
        + counts.gujarati
        + counts.gurmukhi
        + counts.georgian
        + counts.armenian
        + counts.khmer
        + counts.lao
        + counts.myanmar
        + counts.sinhala
        + counts.ethiopic;
    if total < 3 {
        return None;
    }
    // Japanese mixes kana with Han: any real kana makes it Japanese.
    if counts.kana * 10 >= total {
        return Some("ja");
    }
    let scripts: [(u32, &'static str); 22] = [
        (counts.hebrew, "he"),
        (counts.greek, "el"),
        (counts.hangul, "ko"),
        (counts.thai, "th"),
        (counts.devanagari, "hi"),
        (counts.bengali, "bn"),
        (counts.tamil, "ta"),
        (counts.telugu, "te"),
        (counts.kannada, "kn"),
        (counts.malayalam, "ml"),
        (counts.gujarati, "gu"),
        (counts.gurmukhi, "pa"),
        (counts.georgian, "ka"),
        (counts.armenian, "hy"),
        (counts.khmer, "km"),
        (counts.lao, "lo"),
        (counts.myanmar, "my"),
        (counts.sinhala, "si"),
        (counts.ethiopic, "am"),
        (counts.han, "zh-CN"),
        (counts.cyrillic, "ru"),
        (counts.arabic, "ar"),
    ];
    let (top, code) = scripts
        .iter()
        .copied()
        .max_by_key(|(count, _)| *count)
        .unwrap_or((0, "en"));
    if top > counts.latin {
        return Some(match code {
            "ru" => cyrillic_language(&sample),
            "ar" => arabic_language(&sample),
            other => other,
        });
    }
    if counts.latin == 0 {
        return None;
    }
    latin_language(&sample)
}

// ---------------------------------------------------------------------------
// Preferences
// ---------------------------------------------------------------------------

/// The user's translation preferences, persisted as `translate_prefs.json`.
/// Tdesktop keeps these in its local settings (`translateButtonEnabled`,
/// `translateChatEnabled`, `translateTo`, `skipTranslationLanguages`) and the
/// per-chat "hide the bar" flag on the server; TDLib has no way to set the
/// server flag, so the hidden chats are local here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslatePrefs {
    /// "Show Translate Button": the Translate entries in the message menu.
    #[serde(default = "default_true")]
    pub show_button: bool,
    /// "Translate Entire Chats" (Telegram Premium): the chat bar. On by
    /// default like tdesktop (`_translateChatEnabled = true`); the switch is
    /// locked for accounts without Premium, but channels with automatic
    /// translation still use it.
    #[serde(default = "default_true")]
    pub translate_chats: bool,
    /// The "Translate to" language; empty means the app language.
    #[serde(default)]
    pub translate_to: String,
    /// The "Do Not Translate" languages; empty means just the app language.
    #[serde(default)]
    pub skip_languages: Vec<String>,
    /// Chats whose translate bar the user hid.
    #[serde(default)]
    pub hidden_chats: Vec<i64>,
}

fn default_true() -> bool {
    true
}

impl Default for TranslatePrefs {
    fn default() -> Self {
        Self {
            show_button: true,
            translate_chats: true,
            translate_to: String::new(),
            skip_languages: Vec::new(),
            hidden_chats: Vec::new(),
        }
    }
}

impl TranslatePrefs {
    /// The "Translate to" language given the app language tag.
    pub fn to_language(&self, ui_language: &str) -> &'static str {
        normalize_code(&self.translate_to)
            .or_else(|| normalize_code(ui_language))
            .unwrap_or("en")
    }

    /// The "Do Not Translate" list (never empty: at least the app language).
    pub fn skip(&self, ui_language: &str) -> Vec<&'static str> {
        let mut list: Vec<&'static str> = self
            .skip_languages
            .iter()
            .filter_map(|code| normalize_code(code))
            .collect();
        list.dedup();
        if list.is_empty() {
            list.push(normalize_code(ui_language).unwrap_or("en"));
        }
        list
    }

    /// Toggle a language in the skip list. Removing the last one is refused
    /// (tdesktop: "choose at least one language"): returns `false` then.
    pub fn toggle_skip(&mut self, code: &str, ui_language: &str) -> bool {
        let Some(code) = normalize_code(code) else {
            return false;
        };
        let mut list = self.skip(ui_language);
        if let Some(index) = list.iter().position(|c| *c == code) {
            if list.len() == 1 {
                return false;
            }
            list.remove(index);
        } else {
            list.push(code);
        }
        self.skip_languages = list.into_iter().map(str::to_string).collect();
        true
    }

    /// Add a language to the skip list ("Don't translate X" in the bar menu).
    pub fn add_skip(&mut self, code: &str, ui_language: &str) {
        let Some(code) = normalize_code(code) else {
            return;
        };
        let mut list = self.skip(ui_language);
        if !list.contains(&code) {
            list.push(code);
        }
        self.skip_languages = list.into_iter().map(str::to_string).collect();
    }

    pub fn bar_hidden(&self, chat_id: i64) -> bool {
        self.hidden_chats.contains(&chat_id)
    }

    pub fn set_bar_hidden(&mut self, chat_id: i64, hidden: bool) {
        self.hidden_chats.retain(|id| *id != chat_id);
        if hidden {
            self.hidden_chats.push(chat_id);
        }
    }
}

/// `ChooseTranslateTo`: the saved language, unless that is the language the
/// chat is in, then the first "Do Not Translate" language.
pub fn choose_translate_to(
    offered_from: Option<&str>,
    saved_to: &'static str,
    skip: &[&'static str],
) -> &'static str {
    match offered_from {
        Some(from) if from == saved_to => skip.first().copied().unwrap_or(saved_to),
        _ => saved_to,
    }
}

/// `SkipTranslate`: whether the message menu leaves Translate out for
/// `text`. Empty or letterless text, a disabled button and text already in
/// a "Do Not Translate" language are skipped.
pub fn skip_translate(text: &str, prefs: &TranslatePrefs, ui_language: &str) -> bool {
    if text.is_empty() || !prefs.show_button {
        return true;
    }
    if !text.chars().take(100).any(char::is_alphabetic) {
        return true;
    }
    detect_language(text).is_some_and(|lang| prefs.skip(ui_language).contains(&lang))
}

/// Messages tdesktop reads before offering to translate the chat, and the
/// share of them that must be in a translatable language.
const ENOUGH_FOR_RECOGNITION: usize = 10;
const ENOUGH_FOR_TRANSLATION: usize = 6;
/// Messages the guess looks at (newest first).
pub const MAX_RECOGNIZED: usize = 100;

/// `TranslateTracker::checkRecognized`: the language to offer translating
/// from, given the texts of the chat's recent messages. At least six
/// messages are needed; the most common language that is not in `skip` wins
/// when enough of them (six of ten, scaled) are in non-skipped languages.
pub fn offer_language<'a>(
    texts: impl IntoIterator<Item = &'a str>,
    skip: &[&'static str],
) -> Option<&'static str> {
    let mut count = 0usize;
    let mut tally: Vec<(&'static str, usize)> = Vec::new();
    for text in texts.into_iter().take(MAX_RECOGNIZED) {
        count += 1;
        if let Some(lang) = detect_language(text).filter(|lang| !skip.contains(lang)) {
            match tally.iter_mut().find(|(code, _)| *code == lang) {
                Some((_, n)) => *n += 1,
                None => tally.push((lang, 1)),
            }
        }
    }
    if count < ENOUGH_FOR_TRANSLATION {
        return None;
    }
    let threshold = if count > ENOUGH_FOR_RECOGNITION {
        count * ENOUGH_FOR_TRANSLATION / ENOUGH_FOR_RECOGNITION
    } else {
        ENOUGH_FOR_TRANSLATION
    };
    let translatable: usize = tally.iter().map(|(_, n)| n).sum();
    if translatable < threshold {
        return None;
    }
    tally.into_iter().max_by_key(|(_, n)| *n).map(|(c, _)| c)
}

/// The text (or caption) a translation of `content` applies to, with its
/// entities; `None` for content with no text.
pub fn translatable_content(content: &MessageContent) -> Option<(&str, &[TextEntity])> {
    let (text, entities) = match content {
        MessageContent::Text(text) => (text.text.as_str(), text.entities.as_slice()),
        MessageContent::Photo(c) => (c.caption.as_str(), c.caption_entities.as_slice()),
        MessageContent::Document(c) => (c.caption.as_str(), c.caption_entities.as_slice()),
        MessageContent::Animation(c) => (c.caption.as_str(), c.caption_entities.as_slice()),
        MessageContent::Video(c) => (c.caption.as_str(), c.caption_entities.as_slice()),
        MessageContent::VoiceNote(c) => (c.caption.as_str(), c.caption_entities.as_slice()),
        MessageContent::Audio(c) => (c.caption.as_str(), c.caption_entities.as_slice()),
        _ => return None,
    };
    (!text.is_empty()).then_some((text, entities))
}

/// Replace the text (or caption) of `content` with a translation. Returns
/// `false` and leaves `content` alone when it has no text to replace.
pub fn replace_content_text(
    content: &mut MessageContent,
    text: &str,
    entities: &[TextEntity],
) -> bool {
    macro_rules! caption {
        ($c:expr) => {{
            $c.caption = text.to_string();
            $c.caption_entities = entities.to_vec();
        }};
    }
    match content {
        MessageContent::Text(c) => {
            c.text = text.to_string();
            c.entities = entities.to_vec();
        }
        MessageContent::Photo(c) => caption!(c),
        MessageContent::Document(c) => caption!(c),
        MessageContent::Animation(c) => caption!(c),
        MessageContent::Video(c) => caption!(c),
        MessageContent::VoiceNote(c) => caption!(c),
        MessageContent::Audio(c) => caption!(c),
        _ => return false,
    }
    true
}

/// `TranslateTracker::setup`: the bar and the translated chat are tracked
/// when "Translate Entire Chats" is on and the account has Premium or the
/// channel translates automatically.
pub fn tracking_enabled(translate_chats: bool, premium: bool, automatic: bool) -> bool {
    translate_chats && (premium || automatic)
}

/// The bar's label with the channel auto-translate wording
/// (`lng_translate_return_original`: "View Original (Spanish)").
pub fn bar_label_for(translated: bool, to: &str, automatic: bool, from: Option<&str>) -> String {
    match (translated, automatic, from) {
        (true, true, Some(from)) => format!("View Original ({})", language_name(from)),
        _ => bar_label(translated, to),
    }
}

/// The translate bar's label.
pub fn bar_label(translated: bool, to: &str) -> String {
    if translated {
        "Show Original".to_string()
    } else {
        format!("Translate to {}", language_name(to))
    }
}

#[cfg(test)]
mod tests;
