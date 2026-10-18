//! Where the pull leads: the next unread channel or topic, and when the
//! bottom is not the end yet.

use super::Mode;
use crate::ids::ChatId;
use crate::state::{ChatSummary, Session};

/// `PeerData::computeUnavailableReason` is not empty: TDLib reports a
/// restriction reason for the channel.
pub fn chat_unavailable(session: &Session, chat: &ChatSummary) -> bool {
    match chat.kind {
        crate::telegram::envelope::ChatKind::Supergroup { supergroup_id, .. } => session
            .groups
            .supergroup_restricted
            .contains(&supergroup_id),
        _ => false,
    }
}

/// `SponsoredMessages::hasUnshownFor`: the open channel has an ad that will
/// be shown after its last message but has not been shown yet, so the
/// bottom is not the end yet. Lists fetched more than five minutes ago and
/// ads placed between posts (`messages_between`) never hold the pull.
pub fn sponsored_unshown(session: &Session, chat: ChatId, now: std::time::Instant) -> bool {
    if session.open_chat != Some(chat) {
        return false;
    }
    let Some(entry) = session.messages.sponsored.get(&chat.0) else {
        return false;
    };
    if entry.messages_between != 0
        || entry.fetched_at.is_none_or(|at| {
            now.saturating_duration_since(at) >= crate::state::SPONSORED_REFETCH_AFTER
        })
    {
        return false;
    }
    session
        .open_sponsored_tail()
        .is_some_and(|ad| !entry.viewed.contains(&ad.message_id))
}

/// `FindInList`: the first other channel with unread messages that is not
/// the promoted (proxy / announcement) chat and is available.
fn find_in_list<'a>(
    session: &Session,
    list: impl IntoIterator<Item = &'a ChatSummary>,
    current: ChatId,
) -> Option<ChatId> {
    list.into_iter()
        .find(|chat| {
            chat.id != current
                && chat.is_channel()
                && chat.unread_count > 0
                && !session.chat_list.promoted_chats.contains_key(&chat.id.0)
                && !chat_unavailable(session, chat)
        })
        .map(|chat| chat.id)
}

/// `FindNextUnreadChannel`: the open folder's list first, then every other
/// folder in order, then all chats (when a folder was open), then the
/// archive.
pub fn find_next_unread_channel(
    session: &Session,
    current: ChatId,
    folder: Option<i32>,
) -> Option<ChatId> {
    let current_list = match folder {
        Some(id) => session.ordered_folder_chats(id),
        None => session.ordered_chats(),
    };
    if let Some(found) = find_in_list(session, current_list, current) {
        return Some(found);
    }
    for other in &session.chat_list.chat_folders {
        if Some(other.id) == folder {
            continue;
        }
        if let Some(found) = find_in_list(session, session.ordered_folder_chats(other.id), current)
        {
            return Some(found);
        }
    }
    if folder.is_some()
        && let Some(found) = find_in_list(session, session.ordered_chats(), current)
    {
        return Some(found);
    }
    find_in_list(session, session.ordered_archived_chats(), current)
}

/// `FindNextUnreadTopic`: the first other topic of the forum with unread
/// messages, in the topic list's order.
pub fn find_next_unread_topic(session: &Session, chat: ChatId, current: i32) -> Option<i32> {
    session
        .ordered_forum_topics(chat)
        .into_iter()
        .find(|topic| topic.forum_topic_id != current && topic.unread_count > 0)
        .map(|topic| topic.forum_topic_id)
}

/// The hint's text (`lng_pull_next_*` / `lng_release_next_*`).
pub fn hint_text(mode: Mode, release: bool) -> &'static str {
    match (mode, release) {
        (Mode::History, false) => "Pull up to go to the next unread channel",
        (Mode::History, true) => "Release to go to the next unread channel",
        (Mode::Topic, false) => "Pull up to go to the next unread topic",
        (Mode::Topic, true) => "Release to go to the next unread topic",
    }
}

/// The indicator's label when nothing is left (`lng_pull_no_unread_*`).
pub fn nothing_left_text(mode: Mode, group: &str) -> String {
    match mode {
        Mode::History => "You have no unread channels".into(),
        Mode::Topic => format!("No more unread topics in {group}"),
    }
}
