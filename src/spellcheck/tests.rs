use super::*;

fn checker() -> SpellChecker {
    SpellChecker::wordlist()
}

fn words(text: &str) -> Vec<&str> {
    checkable_words(text)
        .into_iter()
        .map(|r| &text[r])
        .collect()
}

fn flagged(text: &str) -> Vec<String> {
    checker()
        .check_text(text)
        .into_iter()
        .map(|m| m.word)
        .collect()
}

#[test]
fn dictionary_loads() {
    let backend = WordlistBackend::new();
    assert!(backend.dictionary_size() > 40_000);
    let sc = checker();
    assert!(sc.is_correct("hello"));
    assert!(sc.is_correct("Hello"));
    assert!(sc.is_correct("WORLD"));
}

#[test]
fn flags_typos_not_valid_words() {
    let sc = checker();
    assert!(!sc.is_correct("teh"));
    assert!(!sc.is_correct("speling"));
    assert!(!sc.is_correct("recieve"));
    assert!(sc.is_correct("quill"));
    assert!(sc.is_correct("telegram"));
}

#[test]
fn segmentation_keeps_apostrophes_and_splits_hyphens() {
    assert_eq!(words("don't stop"), vec!["don't", "stop"]);
    assert_eq!(words("don\u{2019}t"), vec!["don\u{2019}t"]);
    assert_eq!(words("'quoted' word"), vec!["quoted", "word"]);
    assert_eq!(words("well-known"), vec!["well", "known"]);
    assert_eq!(words("Hello, world!"), vec!["Hello", "world"]);
    // Typographic apostrophes are checked as plain ones.
    assert!(checker().is_correct("don\u{2019}t"));
}

#[test]
fn skips_links_and_entities() {
    let text = "teh https://teh.example/teh?x=teh www.tehh.org/a tehh.com \
                mail tehh@tehh.com @tehuser #tehtag $TEHH /tehcmd and/or";
    assert_eq!(words(text), vec!["teh", "mail", "and", "or"]);
    // Markdown link: the label is text, the target is a link.
    assert_eq!(words("[teh](https://x.y/teh)"), vec!["teh"]);
    // Custom-emoji markup is a tg:// link.
    assert_eq!(words("![🔠](tg://emoji?id=1) ok"), vec!["ok"]);
    // A dotted typo is not a domain ("Start" isn't a TLD).
    assert_eq!(words("end.Start"), vec!["end", "Start"]);
}

#[test]
fn skips_code_spans() {
    assert_eq!(words("a `tehh code` b"), vec!["a", "b"]);
    assert_eq!(words("x ```\nfn tehh()\n``` y"), vec!["x", "y"]);
    // Unclosed fence: everything after it is code.
    assert_eq!(words("ok ```tehh"), vec!["ok"]);
    // A lone backtick is just text.
    assert_eq!(words("it`s"), vec!["it", "s"]);
}

#[test]
fn skips_noise_words() {
    assert_eq!(words("teh123 teh_user NASA FYI 😀 x2"), Vec::<&str>::new());
    // Mixed script words are skipped; pure other-script words segment.
    assert_eq!(word_script("caféteh"), Some(Script::Latin));
    assert_eq!(word_script("teшh"), None);
    assert!(checker().is_correct("teшh"));
    let long = "a".repeat(MAX_WORD_CHARS + 1);
    assert!(words(&long).is_empty());
}

#[test]
fn other_scripts_are_never_flagged_by_the_wordlist() {
    assert!(flagged("שלום привет 日本語 café naïve").is_empty());
    assert_eq!(char_script('日'), Some(Script::Han));
    assert_eq!(char_script('ש'), Some(Script::Hebrew));
    assert_eq!(char_script('1'), None);
    assert_eq!(locale_script("en_US"), Some(Script::Latin));
    assert_eq!(locale_script("pt-BR"), Some(Script::Latin));
    assert_eq!(locale_script("ru"), Some(Script::Cyrillic));
    assert_eq!(locale_script("he"), Some(Script::Hebrew));
    assert_eq!(locale_script("xx"), None);
}

#[test]
fn check_text_finds_byte_ranges() {
    let text = "Hello teh world, this is a speling test";
    let miss = checker().check_text(text);
    let found: Vec<&str> = miss.iter().map(|m| &text[m.range()]).collect();
    assert_eq!(found, vec!["teh", "speling"]);
    // Byte ranges stay valid after multi-byte characters.
    let text = "😀 日本 teh";
    let miss = checker().check_text(text);
    assert_eq!(&text[miss[0].range()], "teh");
}

#[test]
fn suggestions_rank_the_obvious_first() {
    let sc = checker();
    assert_eq!(
        sc.suggestions("teh", 5).first().map(String::as_str),
        Some("the")
    );
    assert_eq!(
        sc.suggestions("Teh", 5).first().map(String::as_str),
        Some("The")
    );
    assert!(
        sc.suggestions("speling", 5)
            .contains(&"spelling".to_string())
    );
    assert!(sc.suggestions("hello", 5).is_empty());
    assert!(sc.suggestions(&"z".repeat(4096), 5).is_empty());
    assert!(
        sc.suggestions("wellknown", 5)
            .contains(&"well-known".to_string())
    );
}

#[test]
fn corrections_match_capitalization() {
    assert_eq!(match_capitalization("Teh", "the"), "The");
    assert_eq!(match_capitalization("THE", "the"), "THE");
    assert_eq!(match_capitalization("teh", "the"), "the");
    assert_eq!(match_capitalization("'Teh", "'the"), "'The");
    assert_eq!(match_capitalization("---", "the"), "the");
}

#[test]
fn learn_unlearn_and_ignore() {
    let sc = checker();
    assert!(!sc.is_correct("blorpt"));
    // The wordlist has no dictionary of its own: the app keeps it.
    assert_eq!(sc.learn("blorpt"), LearnedIn::App);
    assert!(sc.is_correct("blorpt"));
    assert!(sc.is_correct("Blorpt"));
    assert!(sc.is_learned("blorpt"));
    assert_eq!(sc.app_words(), vec!["blorpt".to_string()]);
    assert_eq!(sc.unlearn("blorpt"), Some(LearnedIn::App));
    assert!(!sc.is_correct("blorpt"));
    assert_eq!(sc.unlearn("blorpt"), None);
    sc.ignore("Zorbl");
    assert!(sc.is_correct("zorbl"));
    assert!(!sc.is_learned("zorbl"));
    sc.set_app_words(["Frobz".to_string()]);
    assert!(sc.is_correct("frobz"));
}

struct CountingBackend(std::sync::atomic::AtomicUsize);
impl SpellBackend for CountingBackend {
    fn handles(&self, _: Script, _: &str) -> bool {
        true
    }
    fn is_correct(&self, word: &str) -> bool {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        word != "bad"
    }
    fn suggestions(&self, _: &str, _: usize) -> Vec<String> {
        vec!["bad".into(), "good".into(), "good".into()]
    }
}

#[test]
fn backend_answers_are_cached_per_word() {
    let backend = Arc::new(CountingBackend(Default::default()));
    let sc = SpellChecker::new(backend.clone());
    let text = "bad good bad good bad";
    assert_eq!(sc.check_text(text).len(), 3);
    assert_eq!(sc.check_text(text).len(), 3);
    // Two distinct words, two backend calls, across both checks.
    assert_eq!(backend.0.load(std::sync::atomic::Ordering::SeqCst), 2);
    // The word itself and duplicates are dropped from suggestions.
    assert_eq!(sc.suggestions("bad", 5), vec!["good".to_string()]);
}

fn miss(text: &str, word: &str) -> Misspelling {
    let start = text.find(word).unwrap();
    Misspelling {
        word: word.into(),
        start,
        end: start + word.len(),
    }
}

#[test]
fn shift_misspellings_follows_edits() {
    let old = "teh cat speling";
    let list = vec![miss(old, "teh"), miss(old, "speling")];
    // Typing in front shifts both.
    let new = "Oh teh cat speling";
    assert_eq!(
        shift_misspellings(old, new, &list),
        vec![miss(new, "teh"), miss(new, "speling")]
    );
    // Editing inside a word drops just that word.
    let new = "tehx cat speling";
    assert_eq!(
        shift_misspellings(old, new, &list),
        vec![miss(new, "speling")]
    );
    // Punctuation right after a flagged word keeps it.
    let new = "teh cat speling,";
    assert_eq!(
        shift_misspellings(old, new, &list),
        vec![miss(new, "teh"), miss(new, "speling")]
    );
    // Letters right after it drop it (the word changed).
    let new = "teh cat spelingx";
    assert_eq!(shift_misspellings(old, new, &list), vec![miss(new, "teh")]);
    // Multi-byte edits stay on char boundaries.
    let new = "😀teh cat speling";
    assert_eq!(
        shift_misspellings(old, new, &list),
        vec![miss(new, "teh"), miss(new, "speling")]
    );
    assert_eq!(edit_window("aé", "aè"), (1, 3, 3));
}

#[test]
fn spelling_languages_follow_system_languages() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let available = s(&["en", "en_GB", "en_AU", "he", "cs", "pt_BR", "pt_PT", "ru"]);
    assert_eq!(
        spelling_languages(&s(&["en-US", "he-IL"]), &available),
        s(&["en", "he"])
    );
    assert_eq!(
        spelling_languages(&s(&["en-GB"]), &available),
        s(&["en_GB"])
    );
    assert_eq!(
        spelling_languages(&s(&["pt-BR", "pt"]), &available),
        s(&["pt_BR"])
    );
    assert_eq!(spelling_languages(&s(&["pt"]), &available), s(&["pt_BR"]));
    assert_eq!(
        spelling_languages(&s(&["ja-JP"]), &available),
        Vec::<String>::new()
    );
    assert_eq!(
        spelling_languages(&s(&["en", "en-US"]), &available),
        s(&["en"])
    );
}

#[test]
fn typing_word_detection() {
    assert!(is_typing_word("hel", "hell"));
    assert!(is_typing_word("", "a"));
    assert!(!is_typing_word("hell", "hell "));
    assert!(!is_typing_word("hell", "hell,"));
    assert!(!is_typing_word("hello", "hell"));
    assert!(!is_typing_word("same", "same"));
}

#[test]
fn word_at_offset() {
    let text = "hello teh world";
    assert_eq!(word_at(text, 7), Some(6..9));
    assert_eq!(word_at(text, 9), Some(6..9));
    assert_eq!(word_at(text, 5), Some(0..5));
    assert_eq!(word_at(text, 6), Some(6..9));
    assert_eq!(word_at("a @mention", 5), None);
}
