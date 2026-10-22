//! The topic chooser a drop on a forum row opens (tdesktop
//! `ChooseTopicBoxController` shown by `ShowDropMediaBox`): which topics it
//! lists for what was typed in its search field.
#![deny(clippy::indexing_slicing)]

use crate::telegram::envelope::ForumTopic;

/// The words of `text`, lower-cased (`TextUtilities::PrepareSearchWords`).
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Whether `name` answers the search `query`: every word typed starts some
/// word of the name (`PeerListController` search by name words). An empty
/// query matches everything.
pub fn name_matches(query: &str, name: &str) -> bool {
    let wanted = words(query);
    if wanted.is_empty() {
        return true;
    }
    let have = words(name);
    wanted
        .iter()
        .all(|want| have.iter().any(|word| word.starts_with(want.as_str())))
}

/// The topics the chooser lists for `query`, in the order given. The
/// General topic a forum hides above its list is not one of them.
pub fn topics_for<'a>(topics: &'a [ForumTopic], query: &str) -> Vec<&'a ForumTopic> {
    topics
        .iter()
        .filter(|topic| !topic.is_hidden && name_matches(query, &topic.name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_query_matches_every_name() {
        assert!(name_matches("", "General"));
        assert!(name_matches("  ", ""));
    }

    #[test]
    fn every_typed_word_must_start_a_word_of_the_name() {
        assert!(name_matches("rel", "Release planning"));
        assert!(name_matches("plan rel", "Release planning"));
        assert!(!name_matches("lease", "Release planning"));
        assert!(!name_matches("rel zzz", "Release planning"));
    }

    #[test]
    fn case_and_punctuation_do_not_matter() {
        assert!(name_matches("BUG", "bugs & fixes"));
        assert!(name_matches("fix", "Bugs/Fixes"));
    }

    fn topic(id: i32, name: &str) -> ForumTopic {
        ForumTopic {
            forum_topic_id: id,
            name: name.to_string(),
            is_general: false,
            is_closed: false,
            is_pinned: false,
            is_hidden: false,
            unread_count: 0,
            order: 0,
            last_message_preview: String::new(),
            icon_color: 0,
            icon_custom_emoji_id: 0,
            last_message_id: 0,
            last_read_inbox_message_id: 0,
            notification_settings: Default::default(),
            unread_mention_count: 0,
            unread_reaction_count: 0,
            creation_date: 0,
            creator: None,
            is_outgoing: false,
            draft: None,
        }
    }

    #[test]
    fn the_list_keeps_its_order_and_drops_misses() {
        let all = vec![
            topic(1, "General"),
            topic(2, "Releases"),
            topic(3, "Random"),
        ];
        let ids: Vec<i32> = topics_for(&all, "r")
            .iter()
            .map(|t| t.forum_topic_id)
            .collect();
        assert_eq!(ids, vec![2, 3]);
        assert_eq!(topics_for(&all, "").len(), 3);
        assert!(topics_for(&all, "zzz").is_empty());
    }

    #[test]
    fn the_hidden_general_topic_is_not_listed() {
        let mut general = topic(1, "General");
        general.is_hidden = true;
        let all = vec![general, topic(2, "Releases")];
        let ids: Vec<i32> = topics_for(&all, "")
            .iter()
            .map(|t| t.forum_topic_id)
            .collect();
        assert_eq!(ids, vec![2]);
    }
}
