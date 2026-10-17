//! Which messages `pump_translation` has to test on a pass.
//!
//! A pass used to walk the whole loaded history, newest first. Whether a
//! message wants a translation depends on the message itself, the chat's
//! translation settings and the translations stored so far, and stored
//! translations only ever turn "wanted" into "done". So after a full pass
//! the next one tests the messages that wanted a translation last time plus
//! the ones stored or changed since (`HistoryMessages::changed_since`),
//! and walks everything again only when the settings behind the answer
//! moved.
#![deny(clippy::indexing_slicing)]

/// The settings a pass's answer depends on.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) struct PumpKey {
    pub(super) chat: i64,
    pub(super) to: &'static str,
    pub(super) automatic: bool,
    pub(super) skip: Vec<&'static str>,
}

#[derive(Default)]
pub(in crate::ui) struct PumpScan {
    key: Option<PumpKey>,
    /// The messages' `revision` at the last pass.
    revision: u64,
    /// The messages that wanted a translation at the last pass.
    wanted: Vec<i64>,
}

impl PumpScan {
    /// The ids to test now, newest first: `all` (every loaded id, newest
    /// first) after a change of `key`, else the ones that wanted a
    /// translation and the ones `changed_since` the last pass names.
    pub(super) fn candidates(
        &self,
        key: &PumpKey,
        all: impl FnOnce() -> Vec<i64>,
        changed_since: impl FnOnce(u64) -> Vec<i64>,
    ) -> Vec<i64> {
        if self.key.as_ref() != Some(key) {
            return all();
        }
        let mut ids = changed_since(self.revision);
        ids.extend_from_slice(&self.wanted);
        ids.sort_unstable_by(|a, b| b.cmp(a));
        ids.dedup();
        ids
    }

    /// Remember a pass over `key`'s messages at `revision`, with the ids
    /// that still want a translation (newest first).
    pub(super) fn record(&mut self, key: PumpKey, revision: u64, wanted: Vec<i64>) {
        self.key = Some(key);
        self.revision = revision;
        self.wanted = wanted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use std::collections::BTreeMap;

    fn key(to: &'static str) -> PumpKey {
        PumpKey {
            chat: 7,
            to,
            automatic: false,
            skip: vec!["en"],
        }
    }

    /// A stand-in for `HistoryMessages`: ids with the revision they were
    /// last stored at.
    #[derive(Default)]
    struct Window {
        at: BTreeMap<i64, u64>,
        revision: u64,
    }

    impl Window {
        fn of(ids: &[i64]) -> Self {
            let mut window = Self::default();
            for id in ids {
                window.store(*id);
            }
            window
        }

        fn store(&mut self, id: i64) {
            self.revision += 1;
            self.at.insert(id, self.revision);
        }

        fn drop_id(&mut self, id: i64) {
            self.at.remove(&id);
            self.revision += 1;
        }

        fn all(&self) -> Vec<i64> {
            self.at.keys().rev().copied().collect()
        }

        fn changed_since(&self, revision: u64) -> Vec<i64> {
            self.at
                .iter()
                .filter(|(_, at)| **at > revision)
                .map(|(id, _)| *id)
                .collect()
        }

        fn candidates(&self, scan: &PumpScan, key: &PumpKey) -> Vec<i64> {
            scan.candidates(key, || self.all(), |rev| self.changed_since(rev))
        }
    }

    #[test]
    fn the_first_pass_and_a_new_key_walk_the_whole_window() {
        let window = Window::of(&[1, 2, 3]);
        let mut scan = PumpScan::default();
        assert_eq!(window.candidates(&scan, &key("fr")), vec![3, 2, 1]);
        scan.record(key("fr"), window.revision, vec![]);
        assert!(window.candidates(&scan, &key("fr")).is_empty());
        assert_eq!(window.candidates(&scan, &key("de")), vec![3, 2, 1]);
        let mut other_chat = key("fr");
        other_chat.chat = 8;
        assert_eq!(window.candidates(&scan, &other_chat), vec![3, 2, 1]);
    }

    #[test]
    fn later_passes_test_new_and_changed_messages_and_the_backlog() {
        let mut window = Window::of(&[1, 2, 3, 4]);
        let mut scan = PumpScan::default();
        scan.record(key("fr"), window.revision, vec![4, 2]);
        // A new message, and an old one edited in place.
        window.store(5);
        window.store(1);
        assert_eq!(window.candidates(&scan, &key("fr")), vec![5, 4, 2, 1]);
        scan.record(key("fr"), window.revision, vec![5]);
        assert_eq!(window.candidates(&scan, &key("fr")), vec![5]);
    }

    #[test]
    fn the_candidates_give_what_a_full_walk_would() {
        // A message wants a translation unless it is a multiple of 20 or
        // already translated (the translated set only grows).
        let wants = |translated: &[i64], id: i64| id % 20 != 0 && !translated.contains(&id);
        let full = |window: &Window, translated: &[i64]| -> Vec<i64> {
            window
                .all()
                .into_iter()
                .filter(|id| wants(translated, *id))
                .collect()
        };
        let mut scan = PumpScan::default();
        let mut cached = |window: &Window, translated: &[i64]| -> Vec<i64> {
            let found: Vec<i64> = window
                .candidates(&scan, &key("fr"))
                .into_iter()
                .filter(|id| window.at.contains_key(id) && wants(translated, *id))
                .collect();
            scan.record(key("fr"), window.revision, found.clone());
            found
        };
        let mut window = Window::of(&[10, 20, 30, 40, 50]);
        let mut translated: Vec<i64> = vec![];
        assert_eq!(cached(&window, &translated), full(&window, &translated));
        translated.push(50);
        assert_eq!(cached(&window, &translated), full(&window, &translated));
        window.store(60);
        window.store(25);
        window.drop_id(30);
        assert_eq!(cached(&window, &translated), full(&window, &translated));
        translated.extend([60, 25]);
        assert_eq!(cached(&window, &translated), full(&window, &translated));
        window.store(10);
        assert_eq!(cached(&window, &translated), full(&window, &translated));
    }
}
