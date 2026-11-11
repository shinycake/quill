//! The words the user told the spell checker, and the verdicts they decide.

use std::collections::{HashMap, HashSet};

/// What the user told the checker, and the backend answers that depend on
/// it.
#[derive(Default)]
pub(super) struct UserWords {
    /// Words the user added and the backend can't store (lowercased).
    pub(super) app_words: HashSet<String>,
    /// Session ignores ("Ignore"), lowercased. Like tdesktop's
    /// `ignoreWord:inSpellDocumentWithTag:0` they last until quit.
    pub(super) ignored: HashSet<String>,
    /// Backend answers per exact word (case matters: "Paris" ≠ "paris").
    pub(super) cache: VerdictCache,
}

impl UserWords {
    /// Drop every verdict; one asked for before this is not stored after it.
    pub(super) fn clear_cache(&mut self) {
        self.cache.generation = self.cache.generation.wrapping_add(1);
        self.cache.words.clear();
    }

    pub(super) fn replace_app_words(&mut self, words: HashSet<String>) {
        self.app_words = words;
        self.clear_cache();
    }

    pub(super) fn add_app_word(&mut self, word: String) {
        self.app_words.insert(word);
        self.clear_cache();
    }

    pub(super) fn remove_app_word(&mut self, word: &str) -> bool {
        let removed = self.app_words.remove(word);
        self.clear_cache();
        removed
    }
}

/// The verdicts, stamped with how many times they were cleared: a verdict
/// asked for before a clear (an Add to Dictionary, a language change) is
/// not stored after it.
#[derive(Default)]
pub(super) struct VerdictCache {
    pub(super) generation: u64,
    pub(super) words: HashMap<String, bool>,
}
