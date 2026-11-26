//! Methods moved out of `groups_forum.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::history_state::ForumPeek;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::menu::PopupMenu;
use quill::lang::{keys, tr, trn};

impl QuillApp {
    /// Phase 5.1: the topic list for a forum supergroup, one chat-list row
    /// per topic (`forum_topic_row`). Selecting a row opens the per-topic
    /// history.
    pub(in crate::ui) fn forum_topics_pane(
        &self,
        open: Option<ChatId>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let session = self.session();
        let topics: Vec<ForumTopic> = open
            .map(|id| {
                session
                    .map(|s| s.ordered_forum_topics(id))
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let selected_topic = session.and_then(|s| s.open_forum_topic());
        let mut list = div()
            .id("forum-topics")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px_3()
            .pt_2();
        if topics.is_empty() {
            return pane_placeholder(
                tr(keys::Q_PEOPLE_NO_TOPICS_YET),
                tr(keys::Q_PEOPLE_THE_TOPIC_LIST_ARRIVES_VIA),
                cx,
            )
            .into_any_element();
        }
        list = list.child(
            div()
                .id("forum-topics-count")
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(trn(keys::Q_PEOPLE_TOPICS_COUNT, topics.len() as i64)),
        );
        let chat_id = open.unwrap_or(ChatId(0));
        for topic in &topics {
            let selected = selected_topic == Some(topic.forum_topic_id);
            list = list.child(self.forum_topic_row(chat_id, topic, selected, cx));
        }
        list.into_any_element()
    }

    /// The topic row's context menu: Pin / Unpin for those who manage
    /// topics, then the shared topic actions.
    pub(super) fn forum_topic_menu(
        &self,
        chat_id: ChatId,
        topic: &ForumTopic,
        cx: &Context<Self>,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
        let topic_id = topic.forum_topic_id;
        let pinned = topic.is_pinned;
        let can_pin = !topic.is_general
            && self
                .session()
                .is_some_and(|s| s.chat_can_manage_topics(chat_id));
        let extras = self.topic_extras(chat_id, topic);
        let owner = cx.entity().downgrade();
        move |menu, _, _| {
            let mut menu = menu;
            if can_pin {
                let pin_owner = owner.clone();
                menu = menu.item(
                    PopupMenuItem::new(if pinned {
                        tr(keys::Q_PEOPLE_UNPIN)
                    } else {
                        tr(keys::CONTEXT_PIN_MSG)
                    })
                    .icon(if pinned {
                        IconName::PinOff
                    } else {
                        IconName::Pin
                    })
                    .on_click(move |_, _, cx| {
                        let _ = pin_owner.update(cx, |this, cx| {
                            let action = if pinned {
                                ForumTopicAction::Unpin
                            } else {
                                ForumTopicAction::Pin
                            };
                            this.forum_topic_action(chat_id, topic_id, action, cx);
                        });
                    }),
                );
            }
            super::forum_extras::add_topic_extras(menu, extras, owner.clone(), chat_id, topic_id)
        }
    }

    /// Whether the user is peeking at the chat list over the open forum's
    /// topics.
    pub(in crate::ui) fn forum_peeking(&self) -> bool {
        let open = self.session().and_then(|s| s.open_chat_id());
        self.history.forum_peek.is_for(open)
    }

    /// Where the forum topic column sits in a window of this width: a pure
    /// function of the session, the width and the user's peek.
    pub(in crate::ui) fn forum_column_at(&self, width: f32) -> quill::state::ForumColumn {
        self.session()
            .map_or(quill::state::ForumColumn::Hidden, |s| {
                s.forum_column(width, self.forum_peeking())
            })
    }

    /// `forum_column_at` for the width of `window`.
    pub(in crate::ui) fn forum_column_layout(&self, window: &Window) -> quill::state::ForumColumn {
        self.forum_column_at(f32::from(window.viewport_size().width))
    }

    /// The forum's topic list as its own column (tdesktop shows the topics
    /// where the chat list was): the forum's name and topic count on top,
    /// then the topics. Where the topics replace the chat list a "Chats"
    /// button brings it back; filling a one-column window, a back button
    /// leaves the forum.
    pub(in crate::ui) fn forum_column_view(
        &self,
        open: Option<ChatId>,
        pane: crate::ui::shell_layout::ForumPane,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::ui::shell_layout::ForumPane;
        let title = open
            .and_then(|id| self.session().and_then(|s| s.chats.get(&id.0)))
            .map(|chat| chat.title.clone())
            .unwrap_or_default();
        let count = open
            .and_then(|id| self.session().map(|s| s.ordered_forum_topics(id).len()))
            .unwrap_or(0);
        let chrome = self.chrome();
        let gutter = chrome.metrics.gutter;
        let slot = div()
            .id("forum-column")
            .role(gpui_kit::Role::Group)
            .a11y_tag(quill::agent::ui_driver::redact::REGION_FORUM)
            .flex()
            .map(|this| {
                if pane == ForumPane::Fill {
                    this.flex_1().min_w_0()
                } else {
                    this.flex_none()
                        .w(px(gutter + quill::state::FORUM_COLUMN_WIDTH))
                }
            })
            .h_full();
        let column = div()
            .flex()
            .flex_col()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .when(pane == ForumPane::Fill, |this| {
                        this.child(
                            Button::new("forum-column-back")
                                .icon(gpui_kit::assets::IconName::ChevronLeft)
                                .ghost()
                                .tooltip(tr(keys::Q_CHAT_BACK))
                                .accessibility_label(tr(keys::Q_CHAT_BACK))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.back_to_chat_list(window, cx);
                                })),
                        )
                    })
                    .when(pane == ForumPane::Column { chats_button: true }, |this| {
                        this.child(
                            Button::new("forum-column-chats")
                                .icon(gpui_kit::assets::IconName::ChevronLeft)
                                .ghost()
                                .tooltip(tr(keys::Q_PEOPLE_SHOW_CHATS))
                                .accessibility_label(tr(keys::Q_PEOPLE_SHOW_CHATS))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(forum) = open {
                                        this.history.forum_peek = ForumPeek::Chats { forum };
                                    }
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(div().font_semibold().truncate().child(title))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(trn(keys::Q_PEOPLE_TOPICS_COUNT, count as i64)),
                            ),
                    ),
            )
            .child(
                div()
                    .id("forum-column-scroll")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(self.forum_topics_pane(open, cx)),
            );
        crate::ui::chrome::frames::side_column(
            slot,
            column,
            &chrome,
            cx.theme(),
            self.card_glass(cx).as_ref(),
        )
        .into_any_element()
    }

    /// Phase 5.1: select a forum topic (live: `searchChatMessages` with
    /// `topic_id`; demo: the seeded session takes it directly).
    pub(in crate::ui) fn select_topic_ui(&mut self, forum_topic_id: i32, cx: &mut Context<Self>) {
        // The composer belongs to the slot being left (the chat's "All"
        // view or the previous topic): save it there, then restore the new
        // topic's draft once the switch is done.
        self.flush_leaving_draft(cx);
        let open_chat = self.session().and_then(|s| s.open_chat_id());
        let mut opened = true;
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.select_topic(forum_topic_id);
            opened = result.is_ok();
            self.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_PEOPLE_TOPIC_SELECTED)),
                Err(_) => StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_OPEN_TOPIC)),
            });
        } else if let Some(session) = self.demo_session.as_mut()
            && let Some(chat_id) = session.open_chat_id()
        {
            session.select_topic(chat_id, forum_topic_id);
        }
        // An opened topic lands at its bottom, never where it was left; a
        // refused open changes nothing, so it forgets nothing either.
        if opened {
            if let Some(chat_id) = open_chat {
                self.history
                    .scroll_memory
                    .forget((chat_id.0, Some(forum_topic_id), 0));
            }
            self.request_draft_restore();
        }
        cx.notify();
    }

    /// Phase 5.1: leave the topic view, back to the forum's topic list.
    pub(in crate::ui) fn deselect_topic_ui(&mut self, cx: &mut Context<Self>) {
        self.flush_leaving_draft(cx);
        if let Some(live) = self.live.as_mut() {
            live.driver.deselect_topic();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.deselect_topic();
        }
        self.request_draft_restore();
        self.set_note(StatusNote::silent(tr(keys::Q_PEOPLE_BACK_TO_TOPICS)));
        cx.notify();
    }
}
