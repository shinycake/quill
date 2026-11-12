//! Forum topic menu extras (tdesktop `Filler::addTopicLink`,
//! `addManageTopic`, the pinned-topics order, the mention / reaction
//! markers and "Unpin all messages"). The actions are shared by the topic
//! list rows and the subsection tabs.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
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

/// A topic action sent from the menu, until its answer settles the note.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum TopicAction {
    #[default]
    Idle,
    /// Its answer shows `done`, or the error note when it failed.
    Waiting {
        request: quill::ids::RequestId,
        done: String,
    },
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
            self.set_note(StatusNote::info(tr(
                keys::Q_MISC_TOPICS_NEED_A_LIVE_CONNECTION,
            )));
            cx.notify();
            return;
        };
        let (result, done, failed) = match extra {
            TopicExtra::CopyLink => (
                live.driver.copy_forum_topic_link(chat_id, topic_id),
                tr(keys::Q_MISC_FETCHING_TOPIC_LINK),
                tr(keys::Q_MISC_COULD_NOT_GET_THE_TOPIC),
            ),
            TopicExtra::MovePinnedUp => (
                live.driver.move_pinned_forum_topic(chat_id, topic_id, true),
                tr(keys::Q_MISC_PINNED_TOPICS_REORDERED),
                tr(keys::Q_MISC_COULD_NOT_REORDER_PINNED_TOPICS),
            ),
            TopicExtra::MovePinnedDown => (
                live.driver
                    .move_pinned_forum_topic(chat_id, topic_id, false),
                tr(keys::Q_MISC_PINNED_TOPICS_REORDERED),
                tr(keys::Q_MISC_COULD_NOT_REORDER_PINNED_TOPICS),
            ),
            TopicExtra::ReadMentions => (
                live.driver.read_all_forum_topic_mentions(chat_id, topic_id),
                tr(keys::Q_MISC_MENTIONS_MARKED_AS_READ),
                tr(keys::Q_MISC_COULD_NOT_MARK_MENTIONS_AS),
            ),
            TopicExtra::ReadReactions => (
                live.driver
                    .read_all_forum_topic_reactions(chat_id, topic_id),
                tr(keys::Q_MISC_REACTIONS_MARKED_AS_READ),
                tr(keys::Q_MISC_COULD_NOT_MARK_REACTIONS_AS),
            ),
            TopicExtra::UnpinMessages => (
                live.driver
                    .unpin_all_forum_topic_messages(chat_id, topic_id),
                tr(keys::Q_MISC_UNPINNING_MESSAGES),
                tr(keys::Q_MISC_COULD_NOT_UNPIN_MESSAGES),
            ),
            TopicExtra::Edit => return,
        };
        match result {
            // The link is copied when its answer lands: this says it is on
            // its way.
            Ok(Some(_)) if extra == TopicExtra::CopyLink => {
                self.set_note(StatusNote::info(done));
            }
            // The note says it worked when the answer does.
            Ok(Some(request)) => {
                self.chat_ui.topic_action = TopicAction::Waiting {
                    request,
                    done: done.to_string(),
                };
            }
            // Nothing needed sending (a pinned topic already at the end).
            Ok(None) => {}
            Err(_) => self.set_note(StatusNote::error(failed)),
        }
        cx.notify();
    }

    /// Each window's frame: the topic action it sent was answered. A
    /// failure already has its own error note (the session's action
    /// errors), so only a success is shown here.
    pub(in crate::ui) fn topic_action_upkeep(&mut self, cx: &mut Context<Self>) {
        let TopicAction::Waiting { request, .. } = &self.chat_ui.topic_action else {
            return;
        };
        let request = *request;
        let answer = match self.live.as_mut() {
            Some(live) => live.driver.session.take_topic_action_answer(request),
            None => Some(false),
        };
        let Some(ok) = answer else {
            return;
        };
        if let TopicAction::Waiting { done, .. } = std::mem::take(&mut self.chat_ui.topic_action)
            && ok
        {
            self.set_note(StatusNote::info(done));
        }
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
        menu = menu.item(item(
            tr(keys::FORUM_TOPIC_EDIT),
            IconName::Pencil,
            TopicExtra::Edit,
        ));
    }
    if extras.link {
        menu = menu.item(item(
            tr(keys::Q_MISC_COPY_TOPIC_LINK),
            IconName::Link,
            TopicExtra::CopyLink,
        ));
    }
    if extras.up {
        menu = menu.item(item(
            tr(keys::Q_MISC_MOVE_UP),
            IconName::ArrowUp,
            TopicExtra::MovePinnedUp,
        ));
    }
    if extras.down {
        menu = menu.item(item(
            tr(keys::Q_MISC_MOVE_DOWN),
            IconName::ArrowDown,
            TopicExtra::MovePinnedDown,
        ));
    }
    if extras.mentions {
        menu = menu.item(item(
            tr(keys::CONTEXT_MARK_READ_MENTIONS_ALL),
            IconName::AtSign,
            TopicExtra::ReadMentions,
        ));
    }
    if extras.reactions {
        menu = menu.item(item(
            tr(keys::Q_MISC_READ_ALL_REACTIONS),
            IconName::Heart,
            TopicExtra::ReadReactions,
        ));
    }
    if extras.unpin_messages {
        menu = menu.item(item(
            tr(keys::Q_MISC_UNPIN_ALL_MESSAGES),
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
