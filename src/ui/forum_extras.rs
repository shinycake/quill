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

/// One topic action sent from the menu: its answer shows `done`, or the
/// error note when it failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TopicWait {
    pub(super) request: quill::ids::RequestId,
    pub(super) done: String,
}

/// The topic actions sent from the menu, until their answers settle the
/// notes. Each action keeps its own request, so a second one never
/// replaces the first. `Waiting` is never empty: the last answer leaves it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum TopicAction {
    #[default]
    Idle,
    Waiting(Vec<TopicWait>),
}

impl TopicAction {
    /// An action went out as `wait.request`.
    fn add(&mut self, wait: TopicWait) {
        match self {
            Self::Waiting(waits) => waits.push(wait),
            Self::Idle => *self = Self::Waiting(vec![wait]),
        }
    }

    /// Settle every wait `answer` has a word for; each comes back as its
    /// `done` text and whether it succeeded. Idle once none is left.
    fn settle(
        &mut self,
        mut answer: impl FnMut(quill::ids::RequestId) -> Option<bool>,
    ) -> Vec<(String, bool)> {
        let Self::Waiting(waits) = self else {
            return Vec::new();
        };
        let mut settled = Vec::new();
        waits.retain(|wait| match answer(wait.request) {
            Some(ok) => {
                settled.push((wait.done.clone(), ok));
                false
            }
            None => true,
        });
        if waits.is_empty() {
            *self = Self::Idle;
        }
        settled
    }
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
                self.chat_ui.topic_action.add(TopicWait {
                    request,
                    done: done.to_string(),
                });
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
        if self.chat_ui.topic_action == TopicAction::Idle {
            return;
        }
        let mut action = std::mem::take(&mut self.chat_ui.topic_action);
        let settled = match self.live.as_mut() {
            Some(live) => {
                action.settle(|request| live.driver.session.take_topic_action_answer(request))
            }
            None => action.settle(|_| Some(false)),
        };
        self.chat_ui.topic_action = action;
        if settled.is_empty() {
            return;
        }
        for (done, ok) in settled {
            if ok {
                self.set_note(StatusNote::info(done));
            }
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
    use super::{TopicAction, TopicExtras, TopicWait};
    use quill::ids::RequestId;

    fn wait(request: u64, done: &str) -> TopicWait {
        TopicWait {
            request: RequestId(request),
            done: done.into(),
        }
    }

    #[test]
    fn a_second_action_keeps_the_first_ones_wait() {
        let mut action = TopicAction::Idle;
        action.add(wait(1, "mentions read"));
        action.add(wait(2, "unpinned"));
        // The second answers first; the first is still waiting.
        let settled = action.settle(|request| (request == RequestId(2)).then_some(true));
        assert_eq!(settled, vec![("unpinned".to_string(), true)]);
        assert_ne!(action, TopicAction::Idle);
        // The first fails: no toast text of its own, and nothing is left.
        let settled = action.settle(|request| (request == RequestId(1)).then_some(false));
        assert_eq!(settled, vec![("mentions read".to_string(), false)]);
        assert_eq!(action, TopicAction::Idle);
    }

    #[test]
    fn an_answer_for_another_request_settles_nothing() {
        let mut action = TopicAction::Idle;
        action.add(wait(1, "done"));
        assert!(action.settle(|_| None).is_empty());
        assert_eq!(action, TopicAction::Waiting(vec![wait(1, "done")]));
    }

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
