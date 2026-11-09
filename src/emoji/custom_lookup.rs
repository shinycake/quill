//! Which custom emoji ids each `getCustomEmojiStickers` asked for, so an
//! id is asked for again when its lookup fails instead of staying blank
//! for the session.
//!
//! An id is marked attempted (`EmojiPanel::status_resolution_attempted`)
//! when a lookup goes out, so a repaint never asks twice. A loaded sticker
//! unmarks it as before. A lookup that was never sent, failed or got no
//! answer unmarks its ids too, so the next resolve pass asks again; an id
//! whose lookups failed [`MAX_FAILURES`] times stays marked, so a lasting
//! error can't turn every pass into a request. A rate limit never gets
//! here: the read is re-sent after the wait (`RETRYABLE_TYPES`).
use super::EmojiPanel;
use crate::ids::RequestId;
use std::collections::HashMap;

/// Failed lookups after which an id is no longer asked for this session.
pub const MAX_FAILURES: u8 = 3;
/// Most ids whose failures are counted; past it the counts start over.
const MAX_COUNTED: usize = 4096;

#[derive(Debug, Clone, Default)]
pub struct CustomEmojiLookups {
    by_request: HashMap<RequestId, Vec<i64>>,
    failures: HashMap<i64, u8>,
}

impl EmojiPanel {
    /// A lookup for `ids` is going out as `request`.
    pub fn begin_custom_emoji_lookup(&mut self, request: RequestId, ids: &[i64]) {
        self.status_resolution_attempted.extend(ids.iter().copied());
        self.custom_lookups.by_request.insert(request, ids.to_vec());
    }

    /// `request` was answered: its ids that loaded were unmarked when they
    /// landed, and the rest (TDLib knows no sticker for them) stay marked.
    pub fn custom_emoji_lookup_answered(&mut self, request: RequestId) {
        self.custom_lookups.by_request.remove(&request);
    }

    /// `request` never went out: its ids are asked for again.
    pub fn custom_emoji_lookup_not_sent(&mut self, request: RequestId) {
        for id in self
            .custom_lookups
            .by_request
            .remove(&request)
            .unwrap_or_default()
        {
            self.status_resolution_attempted.remove(&id);
        }
    }

    /// `request` failed (or will never be answered): its ids are asked for
    /// again, up to [`MAX_FAILURES`] times each.
    pub fn custom_emoji_lookup_failed(&mut self, request: RequestId) {
        let Some(ids) = self.custom_lookups.by_request.remove(&request) else {
            return;
        };
        if self.custom_lookups.failures.len() > MAX_COUNTED {
            self.custom_lookups.failures.clear();
        }
        for id in ids {
            let failures = self.custom_lookups.failures.entry(id).or_insert(0);
            *failures = failures.saturating_add(1);
            if *failures < MAX_FAILURES {
                self.status_resolution_attempted.remove(&id);
            }
        }
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

    /// The status picker opened: every id may be asked for again.
    pub fn reset_custom_emoji_lookups(&mut self) {
        self.status_resolution_attempted.clear();
        self.custom_lookups.failures.clear();
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
            assert!(!panel.status_resolution_attempted.contains(&7));
            panel.begin_custom_emoji_lookup(RequestId(round), &[7]);
            assert!(panel.status_resolution_attempted.contains(&7));
            panel.custom_emoji_lookup_failed(RequestId(round));
        }
        assert!(
            panel.status_resolution_attempted.contains(&7),
            "a lasting error stops asking"
        );
        panel.reset_custom_emoji_lookups();
        assert!(!panel.status_resolution_attempted.contains(&7));
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
        assert!(panel.status_resolution_attempted.is_empty(), "not sent");
        panel.begin_custom_emoji_lookup(RequestId(2), &[7]);
        panel.begin_custom_emoji_lookup(RequestId(3), &[8]);
        panel.sweep_custom_emoji_lookups(|request| request == RequestId(3));
        assert!(!panel.status_resolution_attempted.contains(&7), "swept");
        assert!(panel.status_resolution_attempted.contains(&8), "pending");
        panel.custom_emoji_lookup_answered(RequestId(3));
        panel.custom_emoji_lookup_failed(RequestId(3));
        assert!(
            panel.status_resolution_attempted.contains(&8),
            "an answered lookup is done"
        );
    }
}
