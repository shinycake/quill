//! Rollback values for optimistic mutations the server may refuse.
use super::*;

/// Slice G1: the pre-request value an optimistic mutation restores when
/// TDLib rejects it. Stored on `PendingRequest::rollback` at send time;
/// the error arm below restores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestRollback {
    /// `setChatPermissions`: the permission block and
    /// `can_send_basic_messages` a refusal puts back (what the server has
    /// once the saves ahead of this one are settled), and the block this
    /// save sent, which a later save's rollback takes over when this one
    /// succeeds.
    ChatPermissions {
        previous: Option<ChatPermissions>,
        previous_can_send: bool,
        sent: ChatPermissions,
    },
    /// `toggleSupergroupJoinByRequest`: the previous join-by-request flag
    /// (`None` = unknown, treated as disabled).
    JoinByRequest {
        supergroup_id: i64,
        previous: Option<bool>,
        shown: bool,
    },
    /// `setSupergroupUsername`: the previous username (`None` = none set).
    SupergroupUsername {
        supergroup_id: i64,
        previous: Option<String>,
        shown: String,
    },
    /// Slice G2: `toggleSupergroupSignMessages`: the previous
    /// `sign_messages` / `show_message_sender` flags (`None` = unknown).
    SignMessages {
        supergroup_id: i64,
        previous_sign: Option<bool>,
        previous_show: Option<bool>,
        shown_sign: bool,
        shown_show: bool,
    },
    /// `toggleChatIsTranslatable`: the previous `chat.is_translatable`.
    ChatIsTranslatable {
        chat_id: i64,
        previous: bool,
        shown: bool,
    },
    /// `toggleSupergroupHasAutomaticTranslation`: the previous flag.
    AutoTranslate {
        supergroup_id: i64,
        previous: bool,
        shown: bool,
    },
    /// Slice G2: `toggleSupergroupHasAggressiveAntiSpamEnabled`: the
    /// previous `has_aggressive_anti_spam_enabled` flag (`None` =
    /// unknown).
    AntiSpam {
        supergroup_id: i64,
        previous: Option<bool>,
        shown: bool,
    },
    /// B7: a supergroup toggle (join-to-send, history for new members,
    /// hidden members): the previous value (`None` = unknown).
    GroupToggle {
        supergroup_id: i64,
        toggle: GroupToggle,
        previous: Option<bool>,
        shown: bool,
    },
    /// B7: `toggleChatHasProtectedContent`: the previous flag.
    /// `previous_disabled_at` is the "you disabled sharing" record the
    /// request replaced (private chats only).
    ProtectedContent {
        chat_id: i64,
        previous: bool,
        shown: bool,
        previous_disabled_at: Option<i64>,
    },
    /// `setDefaultReactionType`: the quick reaction shown before the choice.
    DefaultReaction {
        previous: Option<crate::state::ReactionChoice>,
        shown: crate::state::ReactionChoice,
    },
    /// `deleteBusinessChatLink`: the row dropped at once and where it was.
    ChatLinkDelete {
        index: usize,
        row: crate::business_chat_links::ChatLink,
    },
    /// Pause, resume or remove of the business bot (`previous` is the bar
    /// before, `shown` the one drawn instead; `None` = no bar). Restored
    /// only while the bar still shows `shown`.
    BusinessBotBar {
        chat_id: i64,
        previous: Option<crate::business_bot_bar::BusinessBotManageBar>,
        shown: Option<crate::business_bot_bar::BusinessBotManageBar>,
    },
    /// B7: `setChatAvailableReactions`: the previous setting.
    AvailableReactions {
        chat_id: i64,
        previous: Option<crate::telegram::envelope::ChatAvailableReactions>,
        shown: crate::telegram::envelope::ChatAvailableReactions,
    },
    /// Slice CL1: `toggleChatIsPinned` — the previous pinned flag and
    /// which list it belonged to (`archived` = archive list). The
    /// authoritative state arrives via `updateChatPosition`.
    ChatPin {
        chat_id: i64,
        previous: bool,
        shown: bool,
        archived: bool,
    },
    /// Slice CL1: `toggleChatIsMarkedAsUnread` — the previous
    /// marked-as-unread flag.
    ChatMarkedAsUnread {
        chat_id: i64,
        previous: bool,
        shown: bool,
    },
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
    /// `reorderChatFolders`: the tab order before the drag and the one it
    /// showed instead.
    FolderOrder {
        previous: crate::folder_order::FolderOrder,
        shown: crate::folder_order::FolderOrder,
    },
    /// `removeRecentlyFoundChat`: the entry and where it stood.
    RecentSearchRemoved { chat_id: i64, index: usize },
    /// `removeTopChat`: the frequent contact and where it stood.
    TopChatRemoved { chat_id: i64, index: usize },
    /// `setOption(disable_top_chats)`: the flag before, the one it showed,
    /// and the strip it cleared.
    TopChatsDisabled {
        previous: bool,
        shown: bool,
        top_chats: Vec<ChatId>,
    },
    /// `toggleChatFolderTags`: the flag before the flip.
    FolderTags { previous: bool, shown: bool },
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
            RequestRollback::FolderOrder { previous, shown } => {
                if self.chat_list.folder_order() == shown {
                    self.chat_list.apply_folder_order(&previous);
                }
            }
            RequestRollback::RecentSearchRemoved { chat_id, index } => {
                self.search.search.restore_recent(ChatId(chat_id), index);
            }
            RequestRollback::TopChatRemoved { chat_id, index } => {
                self.search.search.restore_top_chat(ChatId(chat_id), index);
            }
            RequestRollback::TopChatsDisabled {
                previous,
                shown,
                top_chats,
            } => {
                if self.search.search.top_chats_disabled == shown {
                    self.search.search.top_chats_disabled = previous;
                    if self.search.search.top_chats.is_empty() {
                        self.search.search.top_chats = top_chats;
                    }
                }
            }
            RequestRollback::FolderTags { previous, shown } => {
                if self.chat_list.are_folder_tags_enabled == shown {
                    self.chat_list.are_folder_tags_enabled = previous;
                }
            }
            RequestRollback::PollVote {
                chat_id,
                message_id,
                previous,
                shown,
            } => {
                self.set_poll_marks(
                    ChatId(chat_id),
                    MessageId(message_id),
                    &previous,
                    Some(&shown),
                );
            }
            RequestRollback::BusinessBotBar {
                chat_id,
                previous,
                shown,
            } => {
                if self.chats_state.business_bot_bars.get(&chat_id) == shown.as_ref() {
                    self.set_business_bot_bar(chat_id, previous);
                }
            }
            RequestRollback::ChatLinkDelete { index, row } => {
                let links = &mut self.settings.business_extra.chat_links;
                if let Some(list) = links.data_mut()
                    && !list.iter().any(|l| l.link == row.link)
                {
                    list.insert(index.min(list.len()), row);
                }
            }
            RequestRollback::JoinByRequest {
                supergroup_id,
                previous,
                shown,
            } => {
                let flags = &mut self.groups.supergroup_join_by_request;
                if flags.get(&supergroup_id) == Some(&shown) {
                    restore_flag(flags, supergroup_id, previous);
                }
            }
            RequestRollback::SignMessages {
                supergroup_id,
                previous_sign,
                previous_show,
                shown_sign,
                shown_show,
            } => {
                if self.groups.supergroup_sign_messages.get(&supergroup_id) == Some(&shown_sign)
                    && self.groups.supergroup_show_message_sender.get(&supergroup_id)
                        == Some(&shown_show)
                {
                    restore_flag(
                        &mut self.groups.supergroup_sign_messages,
                        supergroup_id,
                        previous_sign,
                    );
                    restore_flag(
                        &mut self.groups.supergroup_show_message_sender,
                        supergroup_id,
                        previous_show,
                    );
                }
            }
            RequestRollback::AntiSpam {
                supergroup_id,
                previous,
                shown,
            } => {
                let flags = &mut self.groups.supergroup_anti_spam_enabled;
                if flags.get(&supergroup_id) == Some(&shown) {
                    restore_flag(flags, supergroup_id, previous);
                }
            }
            RequestRollback::ChatIsTranslatable {
                chat_id,
                previous,
                shown,
            } => {
                if self.chat_is_translatable(ChatId(chat_id)) == shown {
                    self.set_chat_translatable(chat_id, previous);
                }
            }
            RequestRollback::AutoTranslate {
                supergroup_id,
                previous,
                shown,
            } => {
                let current = self
                    .messages
                    .translate
                    .auto_translate_supergroups
                    .contains(&supergroup_id);
                if current == shown {
                    self.set_supergroup_auto_translate(supergroup_id, previous);
                }
            }
            RequestRollback::GroupToggle {
                supergroup_id,
                toggle,
                previous,
                shown,
            } => {
                if self.group_toggle_value(supergroup_id, toggle) == Some(shown) {
                    self.restore_group_toggle(supergroup_id, toggle, previous);
                }
            }
            RequestRollback::ProtectedContent {
                chat_id,
                previous,
                shown,
                previous_disabled_at,
            } => {
                if self.chat_has_protected_content(ChatId(chat_id)) == shown {
                    self.set_chat_protected(chat_id, previous);
                    self.groups
                        .private_sharing
                        .restore_disabled(ChatId(chat_id), previous_disabled_at);
                }
            }
            RequestRollback::ChatPin {
                chat_id,
                previous,
                shown,
                archived,
            } => {
                if let Some(chat) = self.chats.get_mut(&chat_id) {
                    let flag = if archived {
                        &mut chat.archive_is_pinned
                    } else {
                        &mut chat.is_pinned
                    };
                    if *flag == shown {
                        *flag = previous;
                    }
                }
            }
            RequestRollback::ChatMarkedAsUnread {
                chat_id,
                previous,
                shown,
            } => {
                if let Some(chat) = self.chats.get_mut(&chat_id)
                    && chat.is_marked_as_unread == shown
                {
                    chat.is_marked_as_unread = previous;
                }
            }
            RequestRollback::SupergroupUsername {
                supergroup_id,
                previous,
                shown,
            } => {
                if self.groups.supergroup_usernames.get(&supergroup_id) == Some(&shown) {
                    match previous {
                        Some(username) => {
                            self.groups
                                .supergroup_usernames
                                .insert(supergroup_id, username);
                        }
                        None => {
                            self.groups.supergroup_usernames.remove(&supergroup_id);
                        }
                    }
                }
            }
            RequestRollback::DefaultReaction { previous, shown } => {
                if self.stickers.default_reaction.as_ref() == Some(&shown) {
                    self.stickers.default_reaction = previous;
                }
            }
            RequestRollback::AvailableReactions {
                chat_id,
                previous,
                shown,
            } => {
                let held = &mut self.chats_state.chat_available_reactions;
                if held.get(&chat_id) == Some(&shown) {
                    match previous {
                        Some(setting) => {
                            held.insert(chat_id, setting);
                        }
                        None => {
                            held.remove(&chat_id);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Put a remembered flag back: its old value, or no value when it was
/// unknown.
fn restore_flag(
    flags: &mut std::collections::HashMap<i64, bool>,
    key: i64,
    previous: Option<bool>,
) {
    match previous {
        Some(flag) => {
            flags.insert(key, flag);
        }
        None => {
            flags.remove(&key);
        }
    }
}
