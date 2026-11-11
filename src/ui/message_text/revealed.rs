//! What the reader has opened in message text: spoilers shown and
//! collapsed quotes expanded. Two sets with their own keys, so a spoiler
//! and a quote that start at the same run never share an entry.
#![deny(clippy::indexing_slicing)]

use std::collections::HashSet;

/// `(chat id, message id, run index, is-caption block)`. Message ids are
/// only unique within a chat, so the chat id is part of the key.
pub(in crate::ui) type RunKey = (i64, u64, u64, bool);

#[derive(Default)]
pub(in crate::ui) struct Revealed {
    spoilers: HashSet<RunKey>,
    quotes: HashSet<RunKey>,
}

impl Revealed {
    pub(in crate::ui) fn spoiler_shown(&self, key: &RunKey) -> bool {
        self.spoilers.contains(key)
    }

    pub(in crate::ui) fn reveal_spoiler(&mut self, key: RunKey) {
        self.spoilers.insert(key);
    }

    /// The quote starting at run `key.2` is expanded.
    pub(in crate::ui) fn quote_expanded(&self, key: &RunKey) -> bool {
        self.quotes.contains(key)
    }

    pub(in crate::ui) fn toggle_quote(&mut self, key: RunKey) {
        if !self.quotes.remove(&key) {
            self.quotes.insert(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spoiler_and_a_quote_at_the_same_run_are_separate() {
        let mut revealed = Revealed::default();
        let key = (1, 2, 0, false);
        revealed.reveal_spoiler(key);
        assert!(revealed.spoiler_shown(&key) && !revealed.quote_expanded(&key));
        revealed.toggle_quote(key);
        assert!(revealed.quote_expanded(&key));
        revealed.toggle_quote(key);
        assert!(!revealed.quote_expanded(&key) && revealed.spoiler_shown(&key));
    }
}
