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
//! Every `NSSpellChecker` call runs on one dedicated serial worker thread
//! (callers block on a reply channel; they are already background tasks),
//! inside an autorelease pool, with one app-wide spell-document tag.
//! Calling the checker from several pool threads at once, or while the
//! spelling server was still loading a dictionary, made AppKit log
//! "NSSpellServer findMisspelledWordInString timed out" and drop the
//! answer. The worker is warmed up at startup so the first real check
//! doesn't pay the dictionary load.

use std::sync::OnceLock;
use std::sync::mpsc::{self, Sender};

use objc2::rc::autoreleasepool;
use objc2_app_kit::NSSpellChecker;
use objc2_foundation::{NSLocale, NSRange, NSString};
use quill::spellcheck::{Script, SpellBackend, locale_script, spelling_languages};

/// `NSNotFound` (`NSIntegerMax`) as an `NSRange.location`.
const NOT_FOUND: usize = isize::MAX as usize;

type Job = Box<dyn FnOnce(&NSSpellChecker, isize) + Send>;

/// The serial worker; started on first use.
fn worker() -> &'static Sender<Job> {
    static WORKER: OnceLock<Sender<Job>> = OnceLock::new();
    WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        let spawned = std::thread::Builder::new()
            .name("quill-spell".into())
            .spawn(move || {
                // The tag is created on this thread, like every other call.
                let tag = autoreleasepool(|_| NSSpellChecker::uniqueSpellDocumentTag());
                while let Ok(job) = rx.recv() {
                    autoreleasepool(|_| job(&NSSpellChecker::sharedSpellChecker(), tag));
                }
            });
        if let Err(err) = spawned {
            eprintln!("quill: couldn't start the spellcheck thread: {err}");
        }
        tx
    })
}

/// Runs `f` on the worker and waits for its result; `None` when the
/// worker is gone.
fn with_checker<T: Send + 'static>(
    f: impl FnOnce(&NSSpellChecker, isize) -> T + Send + 'static,
) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    let job: Job = Box::new(move |checker, tag| {
        let _ = tx.send(f(checker, tag));
    });
    worker().send(job).ok()?;
    rx.recv().ok()
}

/// Queues `f` without waiting.
fn post_to_checker(f: impl FnOnce(&NSSpellChecker, isize) + Send + 'static) {
    let _ = worker().send(Box::new(f));
}

pub(super) struct SystemSpellBackend {
    /// Spelling languages with their script, in preference order.
    languages: Vec<(String, Script)>,
}

impl SystemSpellBackend {
    /// Reads the user's spelling languages. Call on the main thread (the
    /// shared checker is created here). `None` when no language with a
    /// checkable script is configured.
    pub(super) fn new() -> Option<Self> {
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
        if languages.is_empty() {
            return None;
        }
        // Load each dictionary now, off the UI thread, so the first
        // keystroke-driven check doesn't time out waiting for the server.
        for (code, _) in &languages {
            let code = code.clone();
            post_to_checker(move |checker, tag| {
                let ns_word = NSString::from_str("a");
                let lang = NSString::from_str(&code);
                // SAFETY: `word_count` may be null per the API contract.
                let _ = unsafe {
                    checker.checkSpellingOfString_startingAt_language_wrap_inSpellDocumentWithTag_wordCount(
                        &ns_word,
                        0,
                        Some(&lang),
                        false,
                        tag,
                        std::ptr::null_mut(),
                    )
                };
            });
        }
        Some(Self { languages })
    }

    fn languages_for(&self, word: &str) -> Vec<String> {
        let script = quill::spellcheck::word_script(word);
        self.languages
            .iter()
            .filter(|(_, s)| Some(*s) == script)
            .map(|(code, _)| code.clone())
            .collect()
    }
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
        let langs = self.languages_for(word);
        let word = word.to_string();
        with_checker(move |checker, tag| {
            let ns_word = NSString::from_str(&word);
            langs.iter().any(|lang| {
                let lang = NSString::from_str(lang);
                // SAFETY: `word_count` may be null per the API contract.
                let range = unsafe {
                    checker.checkSpellingOfString_startingAt_language_wrap_inSpellDocumentWithTag_wordCount(
                        &ns_word,
                        0,
                        Some(&lang),
                        false,
                        tag,
                        std::ptr::null_mut(),
                    )
                };
                range.location == NOT_FOUND || range.length == 0
            })
        })
        // No answer is no verdict: don't underline.
        .unwrap_or(true)
    }

    fn suggestions(&self, word: &str, limit: usize) -> Vec<String> {
        let langs = self.languages_for(word);
        let word = word.to_string();
        with_checker(move |checker, tag| {
            let ns_word = NSString::from_str(&word);
            let range = NSRange::new(0, utf16_len(&word));
            let mut out: Vec<String> = Vec::new();
            for lang in &langs {
                let lang = NSString::from_str(lang);
                let Some(guesses) = checker
                    .guessesForWordRange_inString_language_inSpellDocumentWithTag(
                        range,
                        &ns_word,
                        Some(&lang),
                        tag,
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
        .unwrap_or_default()
    }

    fn learn(&self, word: &str) -> bool {
        let word = word.to_string();
        with_checker(move |checker, _| checker.learnWord(&NSString::from_str(&word))).is_some()
    }

    fn unlearn(&self, word: &str) -> bool {
        let word = word.to_string();
        with_checker(move |checker, _| {
            let ns_word = NSString::from_str(&word);
            let learned = checker.hasLearnedWord(&ns_word);
            if learned {
                checker.unlearnWord(&ns_word);
            }
            learned
        })
        .unwrap_or(false)
    }

    fn has_learned(&self, word: &str) -> bool {
        let word = word.to_string();
        with_checker(move |checker, _| checker.hasLearnedWord(&NSString::from_str(&word)))
            .unwrap_or(false)
    }

    fn ignore(&self, word: &str) {
        let word = word.to_string();
        post_to_checker(move |checker, tag| {
            checker.ignoreWord_inSpellDocumentWithTag(&NSString::from_str(&word), tag);
        });
    }
}
