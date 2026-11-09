//! Where each custom emoji id's `getCustomEmojiStickers` lookup stands, so
//! an id is asked for again when its lookup fails instead of staying blank
//! for the session.
//!
//! Each id is in one [`IdLookup`] stage. An id is asked for (`Asking`) when
//! a lookup goes out, so a repaint never asks twice. A lookup that was
//! answered moves its ids to `Answered`: the loaded ones sit in the cache,
//! and TDLib knows no sticker for the rest. A lookup that was never sent,
//! was rate limited, failed or got no answer frees its ids, so the next
//! resolve pass asks again; an id whose lookups failed [`MAX_FAILURES`]
//! times is `GaveUp`, so a lasting error can't turn every pass into a
//! request. A rate limit is not a failure: the connect layer holds every
//! lookup until the wait is over (`ConnectDriver::custom_emoji_hold`), then
//! the freed ids are asked for again.
use super::EmojiPanel;
use crate::ids::RequestId;
use std::collections::HashMap;

/// Failed lookups after which an id is no longer asked for this session.
pub const MAX_FAILURES: u8 = 3;
/// Most ids kept only for their failure count; past it the counts start over.
const MAX_COUNTED: usize = 4096;

/// One custom emoji id's lookup stage. An id with no entry was never asked
/// for (or may be asked for again with no failures counted).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdLookup {
    /// A lookup for it is out; `failures` earlier ones failed.
    Asking { failures: u8 },
    /// Free to ask again; `failures` lookups failed so far.
    Failed { failures: u8 },
    /// Its lookups failed [`MAX_FAILURES`] times: not asked for again until
    /// the status picker opens.
    GaveUp,
    /// TDLib answered (the sticker loaded, or there is none).
    Answered,
}

#[derive(Debug, Clone, Default)]
pub struct CustomEmojiLookups {
    by_request: HashMap<RequestId, Vec<i64>>,
    ids: HashMap<i64, IdLookup>,
}

impl CustomEmojiLookups {
    /// Asked for already: a lookup is out, it was answered, or it kept
    /// failing. Such an id is not asked for again.
    pub fn attempted(&self, id: i64) -> bool {
        matches!(
            self.ids.get(&id),
            Some(IdLookup::Asking { .. } | IdLookup::GaveUp | IdLookup::Answered)
        )
    }

    /// The cache dropped `id`'s sticker: ask for it again when it shows.
    /// (A drop during the answer that brought it lands before the answer
    /// is marked, so the answer leaves the id free.)
    pub fn forget(&mut self, id: i64) {
        self.ids.remove(&id);
    }

    /// `request`'s ids are free to ask again; `failed` counts one failure.
    fn release(&mut self, request: RequestId, failed: bool) {
        let Some(ids) = self.by_request.remove(&request) else {
            return;
        };
        if failed && self.ids.len() > MAX_COUNTED {
            self.ids
                .retain(|_, stage| !matches!(stage, IdLookup::Failed { .. }));
        }
        for id in ids {
            let Some(IdLookup::Asking { failures }) = self.ids.get(&id).copied() else {
                continue;
            };
            let failures = if failed {
                failures.saturating_add(1)
            } else {
                failures
            };
            let next = if failures >= MAX_FAILURES {
                Some(IdLookup::GaveUp)
            } else if failures > 0 {
                Some(IdLookup::Failed { failures })
            } else {
                None
            };
            match next {
                Some(stage) => {
                    self.ids.insert(id, stage);
                }
                None => {
                    self.ids.remove(&id);
                }
            }
        }
    }
}

impl EmojiPanel {
    /// A lookup for `ids` is going out as `request`.
    pub fn begin_custom_emoji_lookup(&mut self, request: RequestId, ids: &[i64]) {
        let lookups = &mut self.custom_lookups;
        for id in ids {
            let failures = match lookups.ids.get(id) {
                Some(IdLookup::Failed { failures } | IdLookup::Asking { failures }) => *failures,
                _ => 0,
            };
            lookups.ids.insert(*id, IdLookup::Asking { failures });
        }
        lookups.by_request.insert(request, ids.to_vec());
    }

    /// `request` was answered: its ids that loaded landed in the cache, and
    /// the rest (TDLib knows no sticker for them) are not asked for again.
    pub fn custom_emoji_lookup_answered(&mut self, request: RequestId) {
        let lookups = &mut self.custom_lookups;
        for id in lookups.by_request.remove(&request).unwrap_or_default() {
            if matches!(lookups.ids.get(&id), Some(IdLookup::Asking { .. })) {
                lookups.ids.insert(id, IdLookup::Answered);
            }
        }
    }

    /// `request` never went out: its ids are asked for again.
    pub fn custom_emoji_lookup_not_sent(&mut self, request: RequestId) {
        self.custom_lookups.release(request, false);
    }

    /// `request` was rate limited: not a failure. Its ids are asked for
    /// again once the connect layer's hold is over.
    pub fn custom_emoji_lookup_rate_limited(&mut self, request: RequestId) {
        self.custom_lookups.release(request, false);
    }

    /// `request` failed (or will never be answered): its ids are asked for
    /// again, up to [`MAX_FAILURES`] times each.
    pub fn custom_emoji_lookup_failed(&mut self, request: RequestId) {
        self.custom_lookups.release(request, true);
    }

    /// Lookups no longer pending that were never answered (the stale
    /// request sweep dropped them) count as failed.
    pub fn sweep_custom_emoji_lookups(&mut self, pending: impl Fn(RequestId) -> bool) {
        let lost: Vec<RequestId> = self
            .custom_lookups
            .by_request
            .keys()
            .copied()
            .filter(|request| !pending(*request))
            .collect();
        for request in lost {
            self.custom_emoji_lookup_failed(request);
        }
    }

    /// The status picker opened: every id not being asked for now may be
    /// asked for again, with no failures counted.
    pub fn reset_custom_emoji_lookups(&mut self) {
        self.custom_lookups
            .ids
            .retain(|_, stage| matches!(stage, IdLookup::Asking { .. }));
        for stage in self.custom_lookups.ids.values_mut() {
            *stage = IdLookup::Asking { failures: 0 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EmojiPanel, MAX_FAILURES};
    use crate::ids::RequestId;

    #[test]
    fn a_failed_lookup_asks_again_until_it_keeps_failing() {
        let mut panel = EmojiPanel::default();
        for round in 1..=u64::from(MAX_FAILURES) {
            assert!(!panel.custom_lookups.attempted(7));
            panel.begin_custom_emoji_lookup(RequestId(round), &[7]);
            assert!(panel.custom_lookups.attempted(7));
            panel.custom_emoji_lookup_failed(RequestId(round));
        }
        assert!(
            panel.custom_lookups.attempted(7),
            "a lasting error stops asking"
        );
        panel.reset_custom_emoji_lookups();
        assert!(!panel.custom_lookups.attempted(7));
    }

    /// Every lookup goes out through `ConnectDriver::lookup_custom_emoji`,
    /// so none marks its ids attempted without a way to ask again.
    #[test]
    fn custom_emoji_lookups_go_out_through_one_seam() {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    out.push(path);
                }
            }
        }
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&src, &mut files);
        let mut senders = Vec::new();
        for file in files {
            let at = file
                .strip_prefix(&src)
                .unwrap_or(&file)
                .display()
                .to_string()
                .replace('\\', "/");
            if at.starts_with("telegram") || at.contains("tests") {
                continue;
            }
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            let production = text.split("#[cfg(test)]").next().unwrap_or_default();
            if production.contains("get_custom_emoji_stickers(") {
                senders.push(at);
            }
        }
        assert_eq!(senders, vec!["connect/emoji_sets.rs".to_string()]);
    }

    #[test]
    fn an_unsent_or_lost_lookup_asks_again() {
        let mut panel = EmojiPanel::default();
        panel.begin_custom_emoji_lookup(RequestId(1), &[7, 8]);
        panel.custom_emoji_lookup_not_sent(RequestId(1));
        assert!(
            !panel.custom_lookups.attempted(7) && !panel.custom_lookups.attempted(8),
            "not sent"
        );
        panel.begin_custom_emoji_lookup(RequestId(2), &[7]);
        panel.begin_custom_emoji_lookup(RequestId(3), &[8]);
        panel.sweep_custom_emoji_lookups(|request| request == RequestId(3));
        assert!(!panel.custom_lookups.attempted(7), "swept");
        assert!(panel.custom_lookups.attempted(8), "pending");
        panel.custom_emoji_lookup_answered(RequestId(3));
        panel.custom_emoji_lookup_failed(RequestId(3));
        assert!(
            panel.custom_lookups.attempted(8),
            "an answered lookup is done"
        );
    }
}
