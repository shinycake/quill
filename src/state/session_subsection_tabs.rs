//! Subsection tabs: bots with topics, forums with tabs, live topic updates.
//!
//! Telegram Desktop shows topic tabs (`HistoryView::SubsectionTabs`) for a
//! bot whose `userTypeBot.has_topics` is set and for a forum supergroup with
//! `supergroup.has_forum_tabs` (`PeerData::displaySubsectionTabs`,
//! `data_peer.cpp`).
use super::*;
use crate::subsection_tabs::SubsectionTabsMode;
use crate::telegram::envelope::{ForumTopicInfoUpdate, ForumTopicUpdate, ParsedMessage};

/// `QUILL_TRACE_TOPICS=1`: print every write of a topic's unread count with
/// its source, to compare Quill's badges with TDLib's answers. Temporary.
pub(crate) fn trace_topic_unread(source: &str, chat_id: i64, topic: &ForumTopic) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("QUILL_TRACE_TOPICS").is_some()) {
        eprintln!(
            "quill topics: {source} chat={chat_id} topic={} unread={} read_inbox={} last={}",
            topic.forum_topic_id,
            topic.unread_count,
            topic.last_read_inbox_message_id,
            topic.last_message_id
        );
    }
}

/// A topic read up to (or past) its last message has nothing unread,
/// whatever count came with it.
pub(crate) fn settle_topic_unread(topic: &mut ForumTopic) {
    if topic.last_message_id != 0 && topic.last_read_inbox_message_id >= topic.last_message_id {
        topic.unread_count = 0;
    }
}

/// What a topic tab shows for unread messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicBadge {
    None,
    /// The server's unread count (the topic has a known read position).
    Count(i32),
    /// Unread without a trustworthy count: a dot.
    Dot,
}

/// The bot-side topic flags of a private chat (`userTypeBot`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BotTopics {
    /// `userTypeBot.allows_users_to_create_topics`.
    pub allows_users_to_create_topics: bool,
}

impl Session {
    /// Record `supergroup.has_forum_tabs` (`updateSupergroup` / `getSupergroup`).
    pub fn set_supergroup_forum_tabs(&mut self, supergroup_id: i64, has_forum_tabs: bool) {
        if has_forum_tabs {
            self.forum_tabs_supergroups.insert(supergroup_id);
        } else {
            self.forum_tabs_supergroups.remove(&supergroup_id);
        }
    }

    /// `Some` for a private chat with a bot that has topics
    /// (`userTypeBot.has_topics`; Telegram Desktop `UserData::isForum`).
    pub fn bot_topics(&self, chat_id: ChatId) -> Option<BotTopics> {
        let chat = self.chats.get(&chat_id.0)?;
        let ChatKind::Private { user_id } = chat.kind else {
            return None;
        };
        let user = self.users.get(&user_id.0)?;
        (user.is_bot && user.has_topics).then_some(BotTopics {
            allows_users_to_create_topics: user.allows_users_to_create_topics,
        })
    }

    /// The chat is split into forum topics: a forum supergroup or a bot with
    /// topics (`PeerData::isForum`). Gates `getForumTopics`, topic selection
    /// and topic history.
    pub fn chat_has_topics(&self, chat_id: ChatId) -> bool {
        self.chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.is_forum_chat())
            || self.bot_topics(chat_id).is_some()
    }

    /// `SubsectionTabs::UsedFor` / `PeerData::displaySubsectionTabs`:
    /// - a bot with topics — but when the bot creates the topics itself
    ///   (`!allows_users_to_create_topics`, `Data::IsBotCreatesTopics`), only
    ///   once at least one topic exists (`displayAsForum`);
    /// - a forum supergroup with `has_forum_tabs` (`useSubsectionTabs`).
    pub fn subsection_tabs_used_for(&self, chat_id: ChatId) -> bool {
        if let Some(bot) = self.bot_topics(chat_id) {
            return bot.allows_users_to_create_topics
                || self
                    .forum_topics
                    .get(&chat_id.0)
                    .is_some_and(|topics| !topics.is_empty());
        }
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        match chat.kind {
            ChatKind::Supergroup {
                supergroup_id,
                is_channel: false,
            } => chat.is_forum_chat() && self.forum_tabs_supergroups.contains(&supergroup_id),
            _ => false,
        }
    }

    /// Telegram Desktop's topic badge rule (`Data::ForumTopic::
    /// chatListBadgesState` + `RepliesList::displayedUnreadCount`): the
    /// server's `unread_count` is shown only when the topic has a read
    /// position (`read_inbox_max_id > 1`; `0` means the topic was never
    /// read, and the server's count is not meaningful then — TDLib passes
    /// both through unchanged, `ForumTopic.cpp`). Without one, a bot chat
    /// or a joined group shows a count-less unread mark when the topic's
    /// last message is newer than the whole chat's read position, else
    /// nothing.
    pub fn topic_badge(&self, chat_id: ChatId, topic: &ForumTopic) -> TopicBadge {
        if topic.last_read_inbox_message_id > 1 {
            return if topic.unread_count > 0 {
                TopicBadge::Count(topic.unread_count)
            } else {
                TopicBadge::None
            };
        }
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return TopicBadge::None;
        };
        let member = self.bot_topics(chat_id).is_some()
            || matches!(
                chat.my_member_status,
                Some(
                    crate::telegram::envelope::ChannelMemberStatus::Creator
                        | crate::telegram::envelope::ChannelMemberStatus::Administrator
                        | crate::telegram::envelope::ChannelMemberStatus::Member
                )
            );
        if member && topic.last_message_id > chat.last_read_inbox_message_id.0 {
            TopicBadge::Dot
        } else {
            TopicBadge::None
        }
    }

    /// Unread messages across the chat's topics as the badge counts them
    /// (tdesktop `History::chatListUnreadState` for a forum sums the topics'
    /// `displayedUnreadCount`): only topics with a trustworthy count, see
    /// [`Session::topic_badge`]. Zero for a chat without loaded topics.
    pub fn forum_topics_unread(&self, chat_id: ChatId) -> i32 {
        if !self.chat_has_topics(chat_id) {
            return 0;
        }
        self.forum_topics.get(&chat_id.0).map_or(0, |topics| {
            topics
                .iter()
                .map(|topic| match self.topic_badge(chat_id, topic) {
                    TopicBadge::Count(n) => n.max(0),
                    _ => 0,
                })
                .fold(0, i32::saturating_add)
        })
    }

    /// The chat-list row's topic line for a chat with topics: topic names in
    /// topic order (Telegram Desktop's forum rows list them on the second
    /// line). `None` until topics are loaded or when there are none.
    pub fn chat_row_topic_names(&self, chat_id: ChatId) -> Option<String> {
        if !self.chat_has_topics(chat_id) {
            return None;
        }
        let names: Vec<String> = self
            .ordered_forum_topics(chat_id)
            .into_iter()
            .filter(|t| !t.is_hidden)
            .take(8)
            .map(|t| t.name)
            .filter(|name| !name.is_empty())
            .collect();
        (!names.is_empty()).then(|| names.join("  "))
    }

    /// Telegram Desktop's topic header while a tab topic is open: the
    /// topic name and "N messages". `None` outside tab chats or topics.
    pub fn subsection_topic_header(&self, chat_id: ChatId) -> Option<(String, String)> {
        if !self.subsection_tabs_used_for(chat_id) {
            return None;
        }
        let topic = self.open_topic_info(chat_id)?;
        let count = self
            .topic_histories
            .get(&(chat_id.0, topic.forum_topic_id))
            .map_or(0, |h| h.total_count);
        let line = match count {
            0 => String::new(),
            1 => "1 message".to_string(),
            n => format!("{n} messages"),
        };
        Some((topic.name, line))
    }

    /// The saved tab layout for a chat (Telegram Desktop default: Top).
    pub fn subsection_tabs_mode(&self, chat_id: ChatId) -> SubsectionTabsMode {
        self.media_prefs
            .subsection_tabs_modes
            .get(&chat_id.0)
            .copied()
            .unwrap_or_default()
    }

    /// The toggle button: advance Top → Bottom → Left → Top and remember it
    /// for this chat. The caller persists `media_prefs`.
    pub fn cycle_subsection_tabs_mode(&mut self, chat_id: ChatId) -> SubsectionTabsMode {
        let next = self.subsection_tabs_mode(chat_id).next();
        if next == SubsectionTabsMode::default() {
            self.media_prefs.subsection_tabs_modes.remove(&chat_id.0);
        } else {
            self.media_prefs
                .subsection_tabs_modes
                .insert(chat_id.0, next);
        }
        next
    }

    /// `updateForumTopicInfo`: rename / icon / closed / hidden, or a new
    /// topic (a bot creating one). New topics go first, as the newest
    /// activity does in Telegram's topic order.
    pub(crate) fn apply_update_forum_topic_info(&mut self, info: ForumTopicInfoUpdate) {
        let Some(topics) = self.forum_topics.get_mut(&info.chat_id) else {
            // Not loaded yet: the first `getForumTopics` brings it.
            return;
        };
        if let Some(topic) = topics
            .iter_mut()
            .find(|t| t.forum_topic_id == info.forum_topic_id)
        {
            topic.name = info.name;
            topic.icon_color = info.icon_color;
            topic.icon_custom_emoji_id = info.icon_custom_emoji_id;
            topic.is_general = info.is_general;
            topic.is_closed = info.is_closed;
            topic.is_hidden = info.is_hidden;
            return;
        }
        let order = topics.iter().map(|t| t.order).max().unwrap_or(0) + 1;
        topics.push(ForumTopic {
            forum_topic_id: info.forum_topic_id,
            name: info.name,
            is_general: info.is_general,
            is_closed: info.is_closed,
            is_pinned: false,
            is_hidden: info.is_hidden,
            unread_count: 0,
            order,
            last_message_preview: String::new(),
            icon_color: info.icon_color,
            icon_custom_emoji_id: info.icon_custom_emoji_id,
            last_message_id: 0,
            last_read_inbox_message_id: 0,
            notification_settings: Default::default(),
        });
    }

    /// `updateForumTopic`: pin state, notification settings and the read
    /// position. TDLib sends no per-topic unread count here; the driver
    /// refetches the topic (`getForumTopic`) for TDLib's own count. Reading
    /// up to TDLib's last message clears the badge right away.
    pub(crate) fn apply_update_forum_topic(&mut self, update: ForumTopicUpdate) {
        let Some(topic) = self
            .forum_topics
            .get_mut(&update.chat_id)
            .and_then(|topics| {
                topics
                    .iter_mut()
                    .find(|t| t.forum_topic_id == update.forum_topic_id)
            })
        else {
            return;
        };
        topic.is_pinned = update.is_pinned;
        topic.notification_settings = update.notification_settings;
        if update.last_read_inbox_message_id > topic.last_read_inbox_message_id {
            topic.last_read_inbox_message_id = update.last_read_inbox_message_id;
            settle_topic_unread(topic);
            trace_topic_unread("updateForumTopic", update.chat_id, topic);
        }
    }

    /// A new message in a loaded topic list moves the topic up and refreshes
    /// its preview. Unread counts are not guessed locally (own messages,
    /// pending sends and messages read elsewhere would skew them): the
    /// driver refetches the topic from TDLib.
    pub(crate) fn note_forum_topic_message(&mut self, message: &ParsedMessage) {
        let Some(topic_id) = message.topic_id else {
            return;
        };
        let Some(topics) = self.forum_topics.get_mut(&message.chat_id.0) else {
            return;
        };
        let top = topics.iter().map(|t| t.order).max().unwrap_or(0);
        let Some(topic) = topics.iter_mut().find(|t| t.forum_topic_id == topic_id) else {
            return;
        };
        topic.last_message_preview =
            effective_content(&message.content, message.ephemeral.as_ref()).preview();
        if !topic.is_pinned && topic.order != top {
            topic.order = top + 1;
        }
    }

    /// A `getForumTopic` answer: TDLib's state for one topic replaces the
    /// cached entry (or adds it).
    pub(crate) fn replace_forum_topic(&mut self, chat_id: ChatId, mut topic: ForumTopic) {
        let Some(topics) = self.forum_topics.get_mut(&chat_id.0) else {
            return;
        };
        settle_topic_unread(&mut topic);
        trace_topic_unread("getForumTopic", chat_id.0, &topic);
        match topics
            .iter_mut()
            .find(|t| t.forum_topic_id == topic.forum_topic_id)
        {
            Some(slot) => *slot = topic,
            None => topics.push(topic),
        }
    }

    /// Optimistic "Mark as read" for one topic (the server's
    /// `updateForumTopic` confirms it).
    pub fn mark_forum_topic_read_locally(&mut self, chat_id: ChatId, forum_topic_id: i32) {
        if let Some(topic) = self.forum_topics.get_mut(&chat_id.0).and_then(|topics| {
            topics
                .iter_mut()
                .find(|t| t.forum_topic_id == forum_topic_id)
        }) {
            topic.unread_count = 0;
            topic.last_read_inbox_message_id =
                topic.last_read_inbox_message_id.max(topic.last_message_id);
            trace_topic_unread("markRead(local)", chat_id.0, topic);
        }
    }
}
