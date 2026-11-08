//! Rollback values for optimistic mutations the server may refuse.
use super::*;

/// Slice G1: the pre-request value an optimistic mutation restores when
/// TDLib rejects it. Stored on `PendingRequest::rollback` at send time;
/// the error arm below restores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestRollback {
    /// `setChatPermissions`: the chat's previous permission block and
    /// `can_send_basic_messages`.
    ChatPermissions {
        previous: Option<ChatPermissions>,
        previous_can_send: bool,
    },
    /// `toggleSupergroupJoinByRequest`: the previous join-by-request flag
    /// (`None` = unknown, treated as disabled).
    JoinByRequest {
        supergroup_id: i64,
        previous: Option<bool>,
    },
    /// `setSupergroupUsername`: the previous username (`None` = none set).
    SupergroupUsername {
        supergroup_id: i64,
        previous: Option<String>,
    },
    /// Slice G2: `toggleSupergroupSignMessages`: the previous
    /// `sign_messages` / `show_message_sender` flags (`None` = unknown).
    SignMessages {
        supergroup_id: i64,
        previous_sign: Option<bool>,
        previous_show: Option<bool>,
    },
    /// `toggleChatIsTranslatable`: the previous `chat.is_translatable`.
    ChatIsTranslatable { chat_id: i64, previous: bool },
    /// `toggleSupergroupHasAutomaticTranslation`: the previous flag.
    AutoTranslate { supergroup_id: i64, previous: bool },
    /// Slice G2: `toggleSupergroupHasAggressiveAntiSpamEnabled`: the
    /// previous `has_aggressive_anti_spam_enabled` flag (`None` =
    /// unknown).
    AntiSpam {
        supergroup_id: i64,
        previous: Option<bool>,
    },
    /// B7: a supergroup toggle (join-to-send, history for new members,
    /// hidden members): the previous value (`None` = unknown).
    GroupToggle {
        supergroup_id: i64,
        toggle: GroupToggle,
        previous: Option<bool>,
    },
    /// B7: `toggleChatHasProtectedContent`: the previous flag.
    ProtectedContent { chat_id: i64, previous: bool },
    /// B7: `setChatAvailableReactions`: the previous setting.
    AvailableReactions {
        chat_id: i64,
        previous: Option<crate::telegram::envelope::ChatAvailableReactions>,
    },
    /// Slice CL1: `toggleChatIsPinned` — the previous pinned flag and
    /// which list it belonged to (`archived` = archive list). The
    /// authoritative state arrives via `updateChatPosition`.
    ChatPin { previous: bool, archived: bool },
    /// Slice CL1: `toggleChatIsMarkedAsUnread` — the previous
    /// marked-as-unread flag.
    ChatMarkedAsUnread { previous: bool },
    /// Slice CL2: `setPinnedChats` — the previous `(chat_id, order)`
    /// pairs of the pinned chats in the list (`archived` = archive
    /// list). The reorder swaps `order` values among the pinned chats
    /// so `rebuild_main_order` keeps the new arrangement until the
    /// authoritative `updateChatPosition` orders arrive.
    ChatPinOrder {
        previous: Vec<(i64, i64)>,
        archived: bool,
    },
    /// Slice CL2: `setArchiveChatListSettings` — the previous settings
    /// (`None` = never fetched; the optimistic value is dropped and
    /// the panel re-fetches).
    ArchiveChatListSettings {
        previous: Option<ArchiveChatListSettings>,
    },
    /// The tab menu's Mute / Unmute (`setForumTopicNotificationSettings`):
    /// the topic's settings before it and the ones it showed instead.
    ForumTopicMute {
        chat_id: i64,
        forum_topic_id: i32,
        previous: crate::telegram::envelope::ChatNotificationSettings,
        shown: crate::telegram::envelope::ChatNotificationSettings,
    },
    /// A poll vote or retraction (`setPollAnswer`): each option's chosen
    /// mark before it and the marks it showed instead.
    PollVote {
        chat_id: i64,
        message_id: i64,
        previous: Vec<bool>,
        shown: Vec<bool>,
    },
}

impl Session {
    /// Undo an optimistic topic mute or poll vote the server refused,
    /// unless something has changed it since (an update then tells the
    /// truth, and restoring would overwrite it).
    pub(crate) fn restore_optimistic(&mut self, rollback: RequestRollback) {
        match rollback {
            RequestRollback::ForumTopicMute {
                chat_id,
                forum_topic_id,
                previous,
                shown,
            } => {
                self.threads
                    .forum_topics
                    .update_topic(chat_id, forum_topic_id, |topic| {
                        if topic.notification_settings == shown {
                            topic.notification_settings = previous;
                        }
                    });
            }
            RequestRollback::PollVote {
                chat_id,
                message_id,
                previous,
                shown,
            } => {
                if let Some(history) = self.histories.get_mut(&chat_id)
                    && let Some(message) = history.messages.get_mut(&message_id)
                    && let MessageContent::Poll(content) = &mut message.content
                {
                    let options = &mut content.poll.options;
                    let marks: Vec<bool> = options.iter().map(|o| o.is_chosen).collect();
                    if marks == shown {
                        for (option, was) in options.iter_mut().zip(previous) {
                            option.is_chosen = was;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
