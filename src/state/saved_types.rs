//! Saved Messages sublists and tags (tdesktop `Data::SavedMessages`,
//! `Data::SavedSublist`, the Saved tags bar).
use super::*;
use crate::state::fetch::Fetch;
use crate::telegram::envelope::{ReactionType, SavedMessagesTag, SavedMessagesTopic};

/// The open sublist: the messages saved from one original chat
/// (`getSavedMessagesTopicHistory`).
#[derive(Debug)]
pub struct SavedSublistView {
    pub topic_id: i64,
    pub history: TopicHistory,
}

/// A tag filter over Saved Messages or one sublist (`searchSavedMessages`
/// with a tag, tdesktop's "Filter by Tag").
#[derive(Debug)]
pub struct SavedTagSearch {
    /// 0 = all of Saved Messages.
    pub topic_id: i64,
    pub tag: ReactionType,
    pub history: TopicHistory,
}

/// Everything the Saved Messages sublist / tag UI needs.
#[derive(Debug, Default)]
pub struct SavedMessagesState {
    /// Sublists by id (`savedMessagesTopic.id`).
    pub topics: BTreeMap<i64, SavedMessagesTopic>,
    /// `updateSavedMessagesTopicCount`: approximate total.
    pub topic_count: i32,
    /// How far the sublist list is loaded.
    pub topics_load: SavedTopicsLoad,
    /// Tags of all Saved Messages (`saved_messages_topic_id` 0). A failed
    /// fetch waits for the connection like a failed first sublist page.
    pub tags: Fetch<Vec<SavedMessagesTag>>,
    /// Tags of one sublist, by sublist id.
    pub topic_tags: HashMap<i64, Vec<SavedMessagesTag>>,
    pub sublist: Option<SavedSublistView>,
    pub tag_search: Option<SavedTagSearch>,
    /// A one-shot line for the status bar (a refused action).
    pub note: Option<String>,
}

/// The sublist list of Saved Messages (`loadSavedMessagesTopics`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SavedTopicsLoad {
    /// The first page was not asked for yet.
    #[default]
    Idle,
    /// A page went out as `request`; only its answer or failure settles it.
    Loading { request: RequestId },
    /// The last page was answered; later pages follow the scroll.
    Loaded,
    /// A page failed: the pump asks again once the connection is back
    /// (`updateConnectionState` ready) or Saved Messages is opened again
    /// (`Session::open_chat`), not on every poll pass.
    Retry,
    /// `loadSavedMessagesTopics` answered 404: every sublist is loaded.
    Exhausted,
}

impl SavedTopicsLoad {
    /// A page is in flight.
    pub fn is_loading(self) -> bool {
        matches!(self, Self::Loading { .. })
    }
}

impl SavedMessagesState {
    /// A page of sublists was asked for as `request` (the first page, or the
    /// next one when the list is scrolled). Exhausted stays exhausted.
    pub fn page_sent(&mut self, request: RequestId) {
        if self.topics_load != SavedTopicsLoad::Exhausted {
            self.topics_load = SavedTopicsLoad::Loading { request };
        }
    }

    /// `request`, a page of sublists, was answered: the next follows the
    /// scroll. Another request's answer settles nothing.
    pub fn page_answered(&mut self, request: RequestId) {
        if self.topics_load == (SavedTopicsLoad::Loading { request }) {
            self.topics_load = SavedTopicsLoad::Loaded;
        }
    }

    /// `request` failed: a 404 says every sublist is loaded; any other
    /// failure waits for the connection. Another request's failure settles
    /// nothing. Returns whether it was the page this list waits for.
    pub fn page_failed(&mut self, request: RequestId, all_loaded: bool) -> bool {
        if self.topics_load != (SavedTopicsLoad::Loading { request }) {
            return false;
        }
        self.topics_load = if all_loaded {
            SavedTopicsLoad::Exhausted
        } else {
            SavedTopicsLoad::Retry
        };
        true
    }

    /// The tags of all Saved Messages held.
    pub fn tag_list(&self) -> &[SavedMessagesTag] {
        self.tags.data().map_or(&[], Vec::as_slice)
    }

    /// Every sublist is loaded.
    pub fn topics_exhausted(&self) -> bool {
        self.topics_load == SavedTopicsLoad::Exhausted
    }

    /// A failed first page or tags request waits for the connection (or for
    /// Saved Messages to open again).
    pub fn waiting_to_retry(&self) -> bool {
        self.topics_load == SavedTopicsLoad::Retry || self.tags.error().is_some()
    }

    /// The connection is back or Saved Messages opened again: what failed is
    /// asked again by the pump.
    pub fn retry_after_reconnect(&mut self) {
        if self.topics_load == SavedTopicsLoad::Retry {
            self.topics_load = SavedTopicsLoad::Idle;
        }
        self.tags.clear_error();
    }

    /// Sublists in list order: `order` descending (TDLib already puts the
    /// pinned ones first through `order`).
    pub fn ordered_topics(&self) -> Vec<&SavedMessagesTopic> {
        let mut topics: Vec<&SavedMessagesTopic> = self.topics.values().collect();
        topics.sort_by(|a, b| b.order.cmp(&a.order).then(a.id.cmp(&b.id)));
        topics
    }

    /// Drop the open sublist and tag filter (leaving the Saved chat).
    pub fn close_views(&mut self) {
        self.sublist = None;
        self.tag_search = None;
    }
}

/// A tag label is at most 12 characters (`setSavedMessagesTagLabel`,
/// tdesktop `kTagNameLimit`).
pub const SAVED_TAG_LABEL_MAX: usize = 12;

/// Normalise a typed tag label: trimmed, whitespace collapsed to one line,
/// cut to 12 characters.
pub fn clean_tag_label(raw: &str) -> String {
    let single_line = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = single_line.chars().take(SAVED_TAG_LABEL_MAX).collect();
    cut.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_labels_are_trimmed_and_limited() {
        assert_eq!(clean_tag_label("  Work\n stuff  "), "Work stuff");
        assert_eq!(clean_tag_label("abcdefghijklmnop"), "abcdefghijkl");
        assert_eq!(clean_tag_label("A very long tag"), "A very long");
        assert_eq!(clean_tag_label("   "), "");
    }
}
