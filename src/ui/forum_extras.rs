//! Forum topic menu extras (tdesktop `Filler::addTopicLink`,
//! `addManageTopic`, the pinned-topics order, the mention / reaction
//! markers and "Unpin all messages"). The actions are shared by the topic
//! list rows and the subsection tabs.

use super::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::telegram::envelope::ForumTopic;

/// One extra topic action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum TopicExtra {
    CopyLink,
    MovePinnedUp,
    MovePinnedDown,
    ReadMentions,
    ReadReactions,
    UnpinMessages,
    Edit,
}

/// Which extras apply to a topic right now.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) struct TopicExtras {
    pub link: bool,
    pub up: bool,
    pub down: bool,
    pub mentions: bool,
    pub reactions: bool,
    pub unpin_messages: bool,
    pub edit: bool,
}

impl TopicExtras {
    pub(super) fn any(self) -> bool {
        self.link
            || self.up
            || self.down
            || self.mentions
            || self.reactions
            || self.unpin_messages
            || self.edit
    }
}

impl QuillApp {
    /// The extras that apply to `topic` of `chat_id`.
    pub(super) fn topic_extras(&self, chat_id: ChatId, topic: &ForumTopic) -> TopicExtras {
        let Some(session) = self.session() else {
            return TopicExtras::default();
        };
        let bot = session.bot_topics(chat_id).is_some();
        let manage = session.chat_can_manage_topics(chat_id);
        let can_pin = session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_pin_messages());
        TopicExtras {
            link: !bot && self.live.is_some(),
            up: manage
                && topic.is_pinned
                && session
                    .moved_pinned_order(chat_id, topic.forum_topic_id, true)
                    .is_some(),
            down: manage
                && topic.is_pinned
                && session
                    .moved_pinned_order(chat_id, topic.forum_topic_id, false)
                    .is_some(),
            mentions: topic.unread_mention_count > 0,
            reactions: topic.unread_reaction_count > 0,
            unpin_messages: can_pin && self.live.is_some(),
            edit: manage || bot,
        }
    }

    pub(super) fn topic_extra_action(
        &mut self,
        chat_id: ChatId,
        topic_id: i32,
        extra: TopicExtra,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if extra == TopicExtra::Edit {
            self.open_forum_topic_editor(chat_id, Some(topic_id), window, cx);
            return;
        }
        let Some(live) = self.live.as_mut() else {
            self.status_note = "topics need a live connection (demo)".into();
            cx.notify();
            return;
        };
        let (result, done, failed) = match extra {
            TopicExtra::CopyLink => (
                live.driver.copy_forum_topic_link(chat_id, topic_id),
                "fetching topic link…",
                "could not get the topic link",
            ),
            TopicExtra::MovePinnedUp => (
                live.driver.move_pinned_forum_topic(chat_id, topic_id, true),
                "pinned topics reordered",
                "could not reorder pinned topics",
            ),
            TopicExtra::MovePinnedDown => (
                live.driver
                    .move_pinned_forum_topic(chat_id, topic_id, false),
                "pinned topics reordered",
                "could not reorder pinned topics",
            ),
            TopicExtra::ReadMentions => (
                live.driver.read_all_forum_topic_mentions(chat_id, topic_id),
                "mentions marked as read",
                "could not mark mentions as read",
            ),
            TopicExtra::ReadReactions => (
                live.driver
                    .read_all_forum_topic_reactions(chat_id, topic_id),
                "reactions marked as read",
                "could not mark reactions as read",
            ),
            TopicExtra::UnpinMessages => (
                live.driver
                    .unpin_all_forum_topic_messages(chat_id, topic_id),
                "unpinning messages…",
                "could not unpin messages",
            ),
            TopicExtra::Edit => return,
        };
        self.status_note = if result.is_ok() { done } else { failed }.into();
        cx.notify();
    }
}

/// Append the applicable extras to a topic menu.
pub(super) fn add_topic_extras(
    mut menu: PopupMenu,
    extras: TopicExtras,
    owner: WeakEntity<QuillApp>,
    chat_id: ChatId,
    topic_id: i32,
) -> PopupMenu {
    let item = |label: &'static str, icon: IconName, extra: TopicExtra| {
        let owner = owner.clone();
        PopupMenuItem::new(label)
            .icon(icon)
            .on_click(move |_, window, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.topic_extra_action(chat_id, topic_id, extra, window, cx);
                });
            })
    };
    if extras.edit {
        menu = menu.item(item("Edit Topic", IconName::Pencil, TopicExtra::Edit));
    }
    if extras.link {
        menu = menu.item(item(
            "Copy Topic Link",
            IconName::Link,
            TopicExtra::CopyLink,
        ));
    }
    if extras.up {
        menu = menu.item(item("Move Up", IconName::ArrowUp, TopicExtra::MovePinnedUp));
    }
    if extras.down {
        menu = menu.item(item(
            "Move Down",
            IconName::ArrowDown,
            TopicExtra::MovePinnedDown,
        ));
    }
    if extras.mentions {
        menu = menu.item(item(
            "Mark all mentions as read",
            IconName::AtSign,
            TopicExtra::ReadMentions,
        ));
    }
    if extras.reactions {
        menu = menu.item(item(
            "Read all reactions",
            IconName::Heart,
            TopicExtra::ReadReactions,
        ));
    }
    if extras.unpin_messages {
        menu = menu.item(item(
            "Unpin all messages",
            IconName::PinOff,
            TopicExtra::UnpinMessages,
        ));
    }
    menu
}

#[cfg(test)]
mod tests {
    use super::TopicExtras;

    #[test]
    fn extras_report_when_any_applies() {
        assert!(!TopicExtras::default().any());
        assert!(
            TopicExtras {
                mentions: true,
                ..Default::default()
            }
            .any()
        );
    }
}
