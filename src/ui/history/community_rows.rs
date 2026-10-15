//! Community service rows (`messageChatAddedToCommunity` and kin):
//! centered notices in TGX's wording. The name comes from the
//! `updateCommunity` cache, or TGX's nameless forms.

use super::*;
use quill::lang::{keys, tr, tr_with};

/// The notice text of a community service message, `None` for other
/// content.
pub(super) fn community_service_text(
    message: &HistoryMessage,
    session: Option<&Session>,
    sender: Option<&SenderLabel>,
) -> Option<String> {
    let community_name = |community_id: i64| {
        session.and_then(|s| {
            s.groups
                .communities
                .get(&community_id)
                .map(|c| c.name.clone())
        })
    };
    let someone = || sender.map_or(tr(keys::Q_CHAT_SOMEONE), |label| label.name.as_str());
    let text = match &message.content {
        MessageContent::ChatAddedToCommunity { community_id } => {
            match community_name(*community_id) {
                Some(name) => tr_with(
                    keys::Q_CHAT_THIS_CHAT_WAS_ADDED_TO,
                    &[("name", &name.to_string())],
                ),
                None => tr(keys::Q_CHAT_THIS_CHAT_WAS_ADDED_TO_2).to_string(),
            }
        }
        MessageContent::ChatRemovedFromCommunity => {
            tr(keys::Q_CHAT_THIS_CHAT_WAS_REMOVED_FROM).to_string()
        }
        MessageContent::ChatJoinFromCommunity { community_id } => {
            match (message.is_outgoing, community_name(*community_id)) {
                (true, Some(name)) => tr_with(
                    keys::Q_CHAT_YOU_JOINED_THE_GROUP_FROM,
                    &[("name", &name.to_string())],
                ),
                (true, None) => tr(keys::Q_CHAT_YOU_JOINED_THE_GROUP_FROM_2).to_string(),
                (false, Some(name)) => tr_with(
                    keys::Q_CHAT_JOINED_THE_GROUP_FROM_THE,
                    &[("someone", someone()), ("name", &name.to_string())],
                ),
                (false, None) => tr_with(
                    keys::Q_CHAT_JOINED_THE_GROUP_FROM_THE_2,
                    &[("someone", someone())],
                ),
            }
        }
        _ => return None,
    };
    Some(text)
}
