//! Methods moved out of `groups_forum.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// Phase 5.1: the topic list for a forum supergroup — one row per
    /// topic with name, unread badge, and a cheap last-message preview.
    /// Selecting a row opens the per-topic history.
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
        let selected_topic = session.and_then(|s| s.open_topic);
        let mut list = div()
            .id("forum-topics")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px_3()
            .pt_2()
            .gap_1();
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
                .child(format!("{} topics", topics.len())),
        );
        for topic in topics {
            let topic_id = topic.forum_topic_id;
            // kit Phase 4: the unread count badges the topic name via the
            // kit `Badge`.
            let unread = topic.unread_count;
            let name = if topic.name.is_empty() {
                tr_with(
                    keys::Q_PEOPLE_TOPIC,
                    &[("forum_topic_id", &topic.forum_topic_id.to_string())],
                )
            } else {
                topic.name.clone()
            };
            // A topic draft replaces the last-message preview (red
            // "Draft:" prefix), as a chat row's does.
            let draft = topic.draft_preview();
            let preview = match &draft {
                Some(draft) => draft.text.clone(),
                None => topic.last_message_preview.clone(),
            };
            let draft_reply = draft.map(|draft| draft.reply);
            let extras = open
                .map(|id| self.topic_extras(id, &topic))
                .unwrap_or_default();
            let can_pin = open.is_some_and(|id| {
                self.session().is_some_and(|s| s.chat_can_manage_topics(id)) && !topic.is_general
            });
            let pinned = topic.is_pinned;
            let owner = cx.entity().downgrade();
            let chat_for_menu = open.unwrap_or(ChatId(0));
            list = list.child(
                div()
                    .id(("forum-topic-row", topic_id as u64))
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .role(gpui_kit::Role::Button)
                    .aria_label(name.clone())
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .bg(if selected_topic == Some(topic_id) {
                        cx.theme().accent
                    } else {
                        cx.theme().sidebar
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_topic_ui(topic_id, cx);
                    }))
                    .context_menu(move |menu, _, _| {
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
                                        this.forum_topic_action(
                                            chat_for_menu,
                                            topic_id,
                                            action,
                                            cx,
                                        );
                                    });
                                }),
                            );
                        }
                        super::forum_extras::add_topic_extras(
                            menu,
                            extras,
                            owner.clone(),
                            chat_for_menu,
                            topic_id,
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child({
                                let name_el = div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .min_w_0()
                                    .child(div().font_medium().min_w_0().child(name))
                                    .when(
                                        topic.is_general
                                            && !topic.name.eq_ignore_ascii_case("general"),
                                        |this| {
                                            this.child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(tr(keys::Q_PEOPLE_GENERAL)),
                                            )
                                        },
                                    )
                                    .when(topic.is_pinned, |this| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(tr(keys::Q_PEOPLE_PINNED)),
                                        )
                                    })
                                    .when(topic.is_closed, |this| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(tr(keys::Q_PEOPLE_CLOSED)),
                                        )
                                    });
                                if unread > 0 {
                                    unread_badge(name_el.into_any_element(), unread, false)
                                } else {
                                    name_el.into_any_element()
                                }
                            }),
                    )
                    .when(!preview.is_empty() || draft_reply.is_some(), |this| {
                        this.child(
                            div()
                                .flex()
                                .items_center()
                                .min_w_0()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .when_some(draft_reply, |this, reply| {
                                    this.child(super::super::chat_row::draft_prefix(
                                        reply,
                                        preview.is_empty(),
                                    ))
                                })
                                .child(preview),
                        )
                    }),
            );
        }
        list.into_any_element()
    }

    /// Where the forum topic column sits for this window; also records
    /// whether it is on screen for the conversation pane. A peek at the chat
    /// list ends once the open chat is no longer a forum.
    pub(in crate::ui) fn forum_column_layout(
        &mut self,
        window: &Window,
    ) -> quill::state::ForumColumn {
        let width = f32::from(window.viewport_size().width);
        let layout = self
            .session()
            .map_or(quill::state::ForumColumn::Hidden, |s| {
                s.forum_column(width, self.chat_list.forum_chats_peek)
            });
        if self.chat_list.forum_chats_peek
            && self
                .session()
                .is_some_and(|s| s.forum_column(width, false) == quill::state::ForumColumn::Hidden)
        {
            self.chat_list.forum_chats_peek = false;
        }
        self.chat_list.forum_column_shown = layout != quill::state::ForumColumn::Hidden;
        layout
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
        div()
            .id("forum-column")
            .role(gpui_kit::Role::Group)
            .a11y_tag(quill::agent::ui_driver::redact::REGION_FORUM)
            .flex()
            .flex_col()
            .map(|this| {
                if pane == ForumPane::Fill {
                    this.flex_1().min_w_0()
                } else {
                    this.flex_none().w(px(quill::state::FORUM_COLUMN_WIDTH))
                }
            })
            .h_full()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
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
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.chat_list.forum_chats_peek = true;
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
                                    .child(match count {
                                        1 => tr(keys::Q_PEOPLE_1_TOPIC).to_owned(),
                                        n => format!("{n} topics"),
                                    }),
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
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.select_topic(forum_topic_id);
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_PEOPLE_TOPIC_SELECTED)),
                Err(_) => StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_OPEN_TOPIC)),
            });
        } else if let Some(session) = self.demo_session.as_mut()
            && let Some(chat_id) = session.open_chat
        {
            session.select_topic(chat_id, forum_topic_id);
        }
        self.request_draft_restore();
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
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_PEOPLE_BACK_TO_TOPICS)));
        cx.notify();
    }
}
