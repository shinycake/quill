//! Saved Messages sublists and tags (tdesktop `Data::SavedMessages`,
//! `Data::SavedSublist`, the Saved tags bar).
use super::*;
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
    /// The first page was answered (an empty result then says "no messages").
    pub loaded: bool,
}

/// Everything the Saved Messages sublist / tag UI needs.
#[derive(Debug, Default)]
pub struct SavedMessagesState {
    /// Sublists by id (`savedMessagesTopic.id`).
    pub topics: BTreeMap<i64, SavedMessagesTopic>,
    /// `updateSavedMessagesTopicCount`: approximate total.
    pub topic_count: i32,
    /// `loadSavedMessagesTopics` answered 404: every sublist is loaded.
    pub topics_exhausted: bool,
    /// The first `loadSavedMessagesTopics` was sent.
    pub topics_requested: bool,
    /// Tags of all Saved Messages (`saved_messages_topic_id` 0).
    pub tags: Vec<SavedMessagesTag>,
    pub tags_loaded: bool,
    /// Tags of one sublist, by sublist id.
    pub topic_tags: HashMap<i64, Vec<SavedMessagesTag>>,
    pub sublist: Option<SavedSublistView>,
    pub tag_search: Option<SavedTagSearch>,
    /// A one-shot line for the status bar (a refused action).
    pub note: Option<String>,
}

impl SavedMessagesState {
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
