//! macOS spellcheck backend: the system `NSSpellChecker`, as Telegram
//! Desktop uses it (`lib_spellcheck/spellcheck/platform/mac/spellcheck_mac.mm`).
//!
//! Dictionaries, languages and learned words are the user's system ones:
//! "Add to Dictionary" here is "Learn Spelling" in TextEdit and vice versa.
//! Languages follow tdesktop: with System Settings → Keyboard → Spelling
//! on "Automatic by Language", the macOS preferred languages (General →
//! Language & Region) that have a dictionary; a word is checked against
//! those of its script and is correct when any accepts it (tdesktop
//! recognizes the language per word with CLD3 and checks that one). With
//! a single spelling language picked there, only that one.
//!
//! Called from background threads (tdesktop checks through `crl::async`
//! too); a process-wide mutex serializes every call and each one runs in
//! its own autorelease pool.

use std::sync::Mutex;

use objc2::rc::autoreleasepool;
use objc2_app_kit::NSSpellChecker;
use objc2_foundation::{NSLocale, NSRange, NSString};
use quill::spellcheck::{Script, SpellBackend, locale_script, spelling_languages};

/// `NSNotFound` (`NSIntegerMax`) as an `NSRange.location`.
const NOT_FOUND: usize = isize::MAX as usize;

/// Serializes all `NSSpellChecker` traffic.
static SPELL_LOCK: Mutex<()> = Mutex::new(());

pub(super) struct SystemSpellBackend {
    /// Spelling languages with their script, in preference order.
    languages: Vec<(String, Script)>,
}

impl SystemSpellBackend {
    /// Reads the user's spelling languages. Call on the main thread (the
    /// shared checker is created here). `None` when no language with a
    /// checkable script is configured.
    pub(super) fn new() -> Option<Self> {
        let _guard = SPELL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let languages: Vec<(String, Script)> = autoreleasepool(|_| {
            let checker = NSSpellChecker::sharedSpellChecker();
            let codes: Vec<String> = if checker.automaticallyIdentifiesLanguages() {
                // tdesktop's `SystemLanguages()`: the macOS preferred
                // languages (not every installed spelling dictionary —
                // "Teh" is a fine Czech word), mapped onto dictionaries.
                let system: Vec<String> = NSLocale::preferredLanguages()
                    .iter()
                    .map(|l| l.to_string())
                    .collect();
                let available: Vec<String> = checker
                    .availableLanguages()
                    .iter()
                    .map(|l| l.to_string())
                    .collect();
                let picked = spelling_languages(&system, &available);
                if picked.is_empty() {
                    vec![checker.language().to_string()]
                } else {
                    picked
                }
            } else {
                vec![checker.language().to_string()]
            };
            let mut out: Vec<(String, Script)> = Vec::new();
            for code in codes {
                if let Some(script) = locale_script(&code).filter(|s| s.is_checkable())
                    && !out.iter().any(|(c, _)| *c == code)
                {
                    out.push((code, script));
                }
            }
            out
        });
        (!languages.is_empty()).then_some(Self { languages })
    }

    fn languages_for(&self, word: &str) -> impl Iterator<Item = &str> {
        let script = quill::spellcheck::word_script(word);
        self.languages
            .iter()
            .filter(move |(_, s)| Some(*s) == script)
            .map(|(code, _)| code.as_str())
    }
}

fn with_checker<T>(f: impl FnOnce(&NSSpellChecker) -> T) -> T {
    let _guard = SPELL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    autoreleasepool(|_| f(&NSSpellChecker::sharedSpellChecker()))
}

/// UTF-16 length, for `NSRange`s over an `NSString`.
fn utf16_len(word: &str) -> usize {
    word.encode_utf16().count()
}

impl SpellBackend for SystemSpellBackend {
    fn handles(&self, script: Script, _word: &str) -> bool {
        self.languages.iter().any(|(_, s)| *s == script)
    }

    fn is_correct(&self, word: &str) -> bool {
        let ns_word = NSString::from_str(word);
        with_checker(|checker| {
            self.languages_for(word).any(|lang| {
                let lang = NSString::from_str(lang);
                // SAFETY: `word_count` may be null per the API contract.
                let range = unsafe {
                    checker.checkSpellingOfString_startingAt_language_wrap_inSpellDocumentWithTag_wordCount(
                        &ns_word,
                        0,
                        Some(&lang),
                        false,
                        0,
                        std::ptr::null_mut(),
                    )
                };
                range.location == NOT_FOUND || range.length == 0
            })
        })
    }

    fn suggestions(&self, word: &str, limit: usize) -> Vec<String> {
        let ns_word = NSString::from_str(word);
        let range = NSRange::new(0, utf16_len(word));
        with_checker(|checker| {
            let mut out: Vec<String> = Vec::new();
            for lang in self.languages_for(word) {
                let lang = NSString::from_str(lang);
                let Some(guesses) = checker
                    .guessesForWordRange_inString_language_inSpellDocumentWithTag(
                        range,
                        &ns_word,
                        Some(&lang),
                        0,
                    )
                else {
                    continue;
                };
                for guess in guesses.iter() {
                    let guess = guess.to_string();
                    if !out.contains(&guess) {
                        out.push(guess);
                    }
                    if out.len() >= limit {
                        return out;
                    }
                }
            }
            out
        })
    }

    fn learn(&self, word: &str) -> bool {
        let ns_word = NSString::from_str(word);
        with_checker(|checker| checker.learnWord(&ns_word));
        true
    }

    fn unlearn(&self, word: &str) -> bool {
        let ns_word = NSString::from_str(word);
        with_checker(|checker| {
            let learned = checker.hasLearnedWord(&ns_word);
            if learned {
                checker.unlearnWord(&ns_word);
            }
            learned
        })
    }

    fn has_learned(&self, word: &str) -> bool {
        let ns_word = NSString::from_str(word);
        with_checker(|checker| checker.hasLearnedWord(&ns_word))
    }

    fn ignore(&self, word: &str) {
        let ns_word = NSString::from_str(word);
        with_checker(|checker| checker.ignoreWord_inSpellDocumentWithTag(&ns_word, 0));
    }
}
