//! forum topics + comment threads.

use super::app::QuillApp;
use super::app::pane_placeholder;
use super::chat_row::unread_badge;
use super::pressable::PressableDiv;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{ForumTopic, effective_content};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// Shared seed for the forum-topics screenshot fixtures: forum supergroup
/// (id 16) via `updateNewChat` + `updateChatPosition`, marked a forum via
/// `updateSupergroup`, with a three-topic `getForumTopics` response
/// (General pinned + unread, Announcements with a preview, Random closed)
/// injected through the same reducer the live path uses.
pub(super) fn seed_forum_chat_16(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let topic_json = |id: i32,
                      name: &str,
                      general: bool,
                      closed: bool,
                      pinned: bool,
                      unread: i32,
                      preview: Option<&str>| {
        let last_message = match preview {
            Some(text) => format!(
                r#""last_message":{{"id":{}, "chat_id":16, "is_outgoing":false, "content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{}","entities":[]}}}}}}"#,
                9000 + id,
                text
            ),
            None => r#""last_message":null"#.to_string(),
        };
        format!(
            r#"{{"info":{{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":{id},"name":"{name}","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"creation_date":1,"creator_id":{{"@type":"messageSenderUser","user_id":6}},"is_general":{general},"is_outgoing":false,"is_closed":{closed},"is_hidden":false,"is_name_implicit":false}},{last_message},"order":"{order}","is_pinned":{pinned},"unread_count":{unread},"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"unread_poll_vote_count":0,"notification_settings":{{"@type":"chatNotificationSettings"}},"draft_message":null}}"#,
            id = id,
            name = name,
            general = general,
            closed = closed,
            pinned = pinned,
            unread = unread,
            order = 900 - id,
        )
    };
    let jsons = [
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Demo forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":3}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":16,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"25","is_pinned":false}}"#.to_string(),
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true}}"#.to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(16)));
    let topics = [
        topic_json(
            1,
            "General",
            true,
            false,
            true,
            3,
            Some("Pinned: please read the rules before posting."),
        ),
        topic_json(
            2,
            "Announcements",
            false,
            false,
            false,
            0,
            Some("v2.1 is rolling out this week."),
        ),
        topic_json(3, "Random", false, true, false, 0, None),
    ]
    .join(",");
    let json = format!(
        r#"{{"@type":"forumTopics","@extra":"{}","total_count":3,"topics":[{topics}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
        extra.0,
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

/// `ReadyForumTopics` fixture: the demo opens the forum with no topic
/// selected, so the topic list shows.
pub(super) fn apply_ready_forum_topics(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    seed_forum_chat_16(session, sink, seq);
    session.open_chat(ChatId(16));
}

/// `ReadyTopicPost` fixture (parity slice 4): the forum's General topic is
/// open with a two-message injected history and the composer enabled — the
/// composer now posts into the topic via `sendMessage` with
/// `topic_id = messageTopicForum`.
pub(super) fn apply_ready_topic_post(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    seed_forum_chat_16(session, sink, seq);
    session.open_chat(ChatId(16));
    session.select_topic(ChatId(16), 1);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let extra = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 1);
    let json = format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":0,"messages":[{{"id":101,"chat_id":16,"is_outgoing":false,"topic_id":{{"@type":"messageTopicForum","forum_topic_id":1}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Welcome to General — say hello!","entities":[]}}}}}},{{"id":102,"chat_id":16,"is_outgoing":true,"topic_id":{{"@type":"messageTopicForum","forum_topic_id":1}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Hello from the new topic composer.","entities":[]}}}}}}]}}"#,
        extra.0,
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

impl QuillApp {
    /// Slice G2: open the forum-topic management dialog.
    pub(super) fn open_forum_manage_dialog(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.forum_manage_dialog = Some(ForumManageDialog::new(window, cx, chat_id));
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.refresh_forum_topics(chat_id);
        }
        cx.notify();
    }

    pub(super) fn close_forum_manage_dialog(&mut self, cx: &mut Context<Self>) {
        self.forum_manage_dialog = None;
        cx.notify();
    }

    /// Slice G2: create the topic named in the dialog's input
    /// (`createForumTopic`); empty names are refused up front.
    pub(super) fn submit_forum_topic_create(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (chat_id, name) = match self.forum_manage_dialog.as_ref() {
            Some(dialog) => (
                dialog.chat_id,
                dialog.new_topic_input.read(cx).value().trim().to_string(),
            ),
            None => return,
        };
        if name.is_empty() {
            self.status_note = "topic name cannot be empty".into();
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.create_forum_topic(chat_id, &name) {
                // The topic list refetches after the server confirms
                // (the render path reloads when the cache is dropped).
                Ok(_) => {
                    self.status_note = "topic created".into();
                }
                Err(_) => self.status_note = "could not create topic".into(),
            }
        } else {
            self.status_note = "topics need a live connection (demo)".into();
        }
        if let Some(dialog) = self.forum_manage_dialog.as_mut() {
            dialog.new_topic_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
        }
        cx.notify();
    }

    /// Slice G2: one-shot forum-topic action (close/reopen,
    /// pin/unpin, delete, hide/show General).
    pub(super) fn forum_topic_action(
        &mut self,
        chat_id: ChatId,
        topic_id: i32,
        action: ForumTopicAction,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let result = match action {
                ForumTopicAction::Close => live
                    .driver
                    .toggle_forum_topic_closed(chat_id, topic_id, true),
                ForumTopicAction::Reopen => live
                    .driver
                    .toggle_forum_topic_closed(chat_id, topic_id, false),
                ForumTopicAction::Pin => live
                    .driver
                    .toggle_forum_topic_pinned(chat_id, topic_id, true),
                ForumTopicAction::Unpin => live
                    .driver
                    .toggle_forum_topic_pinned(chat_id, topic_id, false),
                ForumTopicAction::Delete => live.driver.delete_forum_topic(chat_id, topic_id),
                ForumTopicAction::HideGeneral => {
                    live.driver.toggle_general_forum_topic_hidden(chat_id, true)
                }
                ForumTopicAction::ShowGeneral => live
                    .driver
                    .toggle_general_forum_topic_hidden(chat_id, false),
            };
            match result {
                // The topic list refetches after the server confirms
                // (the render path reloads when the cache is dropped).
                Ok(_) => {
                    self.status_note = "topic updated".into();
                }
                Err(_) => self.status_note = "could not update topic".into(),
            }
        } else {
            self.status_note = "topics need a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Slice G2: begin an inline rename for one topic row.
    pub(super) fn begin_forum_topic_rename(
        &mut self,
        topic_id: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(dialog) = self.forum_manage_dialog.as_mut() {
            dialog.editing_topic = Some(topic_id);
            dialog.edit_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
        }
        cx.notify();
    }

    /// Slice G2: submit the inline rename (`editForumTopic`).
    pub(super) fn submit_forum_topic_rename(&mut self, cx: &mut Context<Self>) {
        let (chat_id, topic_id, name) = match self.forum_manage_dialog.as_ref() {
            Some(dialog) => match dialog.editing_topic {
                Some(topic_id) => (
                    dialog.chat_id,
                    topic_id,
                    dialog.edit_input.read(cx).value().trim().to_string(),
                ),
                None => return,
            },
            None => return,
        };
        if name.is_empty() {
            self.status_note = "topic name cannot be empty".into();
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.edit_forum_topic(chat_id, topic_id, &name) {
                // The topic list refetches after the server confirms
                // (the render path reloads when the cache is dropped).
                Ok(_) => {
                    self.status_note = "topic renamed".into();
                }
                Err(_) => self.status_note = "could not rename topic".into(),
            }
        } else {
            self.status_note = "topics need a live connection (demo)".into();
        }
        if let Some(dialog) = self.forum_manage_dialog.as_mut() {
            dialog.editing_topic = None;
        }
        cx.notify();
    }

    /// Slice G2: open the channel-post comment-thread viewer and
    /// fetch the thread history.
    pub(super) fn open_comment_thread_dialog(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.comment_thread_dialog = Some(CommentThreadDialog {
            chat_id,
            message_id,
        });
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .fetch_message_thread_history(chat_id, message_id)
                .is_err()
            {
                self.status_note = "could not load comments".into();
            }
        }
        cx.notify();
    }

    pub(super) fn close_comment_thread_dialog(&mut self, cx: &mut Context<Self>) {
        self.comment_thread_dialog = None;
        cx.notify();
    }

    /// kit Phase 2 (redo): forum manage hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_forum_manage_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ForumManage, |this, _, cx| {
                this.close_forum_manage_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Manage topics"));
            let Some(dialog_state) = this.forum_manage_dialog.as_ref() else {
                return dialog.on_close(on_close);
            };
            let chat_id = dialog_state.chat_id;
            let topics: Vec<ForumTopic> = this
                .session()
                .map(|session| session.ordered_forum_topics(chat_id))
                .unwrap_or_default();
            let editing = dialog_state.editing_topic;
            let mut body =
                div().flex().flex_col().gap_2().child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div().flex_1().child(
                                Textarea::new(&dialog_state.new_topic_input)
                                    .aria_label("New topic name")
                                    .h(px(36.)),
                            ),
                        )
                        .child(Button::new("g2-topic-create").label("Create").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.submit_forum_topic_create(window, cx);
                                this.close_kit_dialog_if_done(DialogKind::ForumManage, window, cx);
                            }),
                        )),
                );
            let mut list = div()
                .id("g2-topic-list")
                .flex()
                .flex_col()
                .gap_1()
                .max_h(px(320.))
                .overflow_y_scroll();
            for topic in &topics {
                list = list.child(this.forum_topic_manage_row(chat_id, topic, editing, cx));
            }
            body = body.child(list);
            let body = body.into_any_element();
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .on_close(on_close)
        })
    }

    /// kit Phase 2 (redo): comment thread hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_comment_thread_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CommentThread, |this, _, cx| {
                this.close_comment_thread_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Comments"));
            let Some((chat_id, message_id)) = this
                .comment_thread_dialog
                .as_ref()
                .map(|dialog| (dialog.chat_id, dialog.message_id))
            else {
                return dialog.on_close(on_close);
            };
            let fetch = this
                .session()
                .and_then(|session| session.comment_thread.clone());
            let mut body = div().flex().flex_col().gap_2();
            match fetch {
                Some(thread) if thread.chat_id == chat_id && thread.message_id == message_id => {
                    if let Some(error) = thread.failed {
                        body = body.child(
                            div()
                                .flex()
                                .items_center()
                                .w_full()
                                .gap_1()
                                .child(
                                    div()
                                        .flex_1()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(error),
                                )
                                .child(
                                    Button::new("g2-comments-retry")
                                        .label("Retry")
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_comment_thread_dialog(
                                                chat_id, message_id, window, cx,
                                            );
                                            this.close_kit_dialog_if_done(
                                                DialogKind::CommentThread,
                                                window,
                                                cx,
                                            );
                                        })),
                                ),
                        );
                    } else if thread.messages.is_empty() {
                        body = body.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("No comments yet."),
                        );
                    } else {
                        let mut list = div()
                            .id("g2-comment-list")
                            .flex()
                            .flex_col()
                            .gap_2()
                            .max_h(px(360.))
                            .overflow_y_scroll();
                        for message in &thread.messages {
                            let name = if message.is_outgoing {
                                "You".to_string()
                            } else {
                                message
                                    .author_signature
                                    .clone()
                                    .unwrap_or_else(|| "Comment".to_string())
                            };
                            let text = Self::message_copyable_text(effective_content(
                                &message.content,
                                message.ephemeral.as_ref(),
                            ))
                            .unwrap_or_else(|| "(no text)".to_string());
                            list = list.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_semibold()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(name),
                                    )
                                    .child(div().text_sm().child(text)),
                            );
                        }
                        body = body.child(list);
                    }
                }
                _ => {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Loading comments…"),
                    );
                }
            }
            let body = body.into_any_element();
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .on_close(on_close)
        })
    }

    /// Slice G2: one topic row in the management dialog — name +
    /// state badges and the applicable actions (General only gets
    /// Hide/Show).
    pub(super) fn forum_topic_manage_row(
        &self,
        chat_id: ChatId,
        topic: &ForumTopic,
        editing: Option<i32>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let topic_id = topic.forum_topic_id;
        let mut badges = String::new();
        if topic.is_general {
            badges.push_str(" · General");
        }
        if topic.is_closed {
            badges.push_str(" · closed");
        }
        if topic.is_pinned {
            badges.push_str(" · pinned");
        }
        let mut row = div().flex().flex_col().w_full().gap_1().child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .font_medium()
                        .child(topic.name.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(badges.trim_start_matches(" · ").to_string()),
                ),
        );
        if editing == Some(topic_id) {
            let edit_input = self
                .forum_manage_dialog
                .as_ref()
                .map(|dialog| dialog.edit_input.clone());
            let mut edit_row = div().flex().items_center().gap_1();
            if let Some(input) = edit_input {
                edit_row = edit_row.child(
                    div()
                        .flex_1()
                        .child(Textarea::new(&input).aria_label("Topic name").h(px(32.))),
                );
            }
            edit_row = edit_row
                .child(
                    Button::new(format!("g2-topic-save-{topic_id}"))
                        .label("Save")
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.submit_forum_topic_rename(cx);
                        })),
                )
                .child(
                    Button::new(format!("g2-topic-cancel-{topic_id}"))
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(dialog) = this.forum_manage_dialog.as_mut() {
                                dialog.editing_topic = None;
                            }
                            cx.notify();
                        })),
                );
            row = row.child(edit_row);
        } else {
            let mut actions = div().flex().flex_wrap().gap_1();
            // Rename opens the inline editor rather than sending.
            actions = actions.child(
                Button::new(format!("g2-topic-rename-{topic_id}"))
                    .label("Rename")
                    .ghost()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.begin_forum_topic_rename(topic_id, window, cx);
                    })),
            );
            if topic.is_general {
                let hide_label = if topic.is_hidden { "Show" } else { "Hide" };
                let hide_action = if topic.is_hidden {
                    ForumTopicAction::ShowGeneral
                } else {
                    ForumTopicAction::HideGeneral
                };
                actions = actions.child(
                    Button::new(format!("g2-topic-hide-{topic_id}"))
                        .label(hide_label)
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.forum_topic_action(chat_id, topic_id, hide_action, cx);
                        })),
                );
            } else {
                let close_label = if topic.is_closed { "Reopen" } else { "Close" };
                let close_action = if topic.is_closed {
                    ForumTopicAction::Reopen
                } else {
                    ForumTopicAction::Close
                };
                let pin_label = if topic.is_pinned { "Unpin" } else { "Pin" };
                let pin_action = if topic.is_pinned {
                    ForumTopicAction::Unpin
                } else {
                    ForumTopicAction::Pin
                };
                for (id, label, action_kind) in [
                    ("close", close_label, close_action),
                    ("pin", pin_label, pin_action),
                    ("delete", "Delete", ForumTopicAction::Delete),
                ] {
                    actions = actions.child(
                        Button::new(format!("g2-topic-{id}-{topic_id}"))
                            .label(label)
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.forum_topic_action(chat_id, topic_id, action_kind, cx);
                            })),
                    );
                }
            }
            row = row.child(actions);
        }
        row.into_any_element()
    }

    /// Phase 5.1: strip shown above a topic's history — back to the topic
    /// list plus the topic name (and its unread count).
    pub(super) fn forum_topic_strip(
        &self,
        info: &ForumTopic,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // kit Phase 4: the unread count badges the topic name via the
        // kit `Badge`.
        let unread = info.unread_count;
        let topic_name = info.name.clone();
        div()
            .id("forum-topic-strip")
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                Button::new("forum-topic-back")
                    .label("\u{2039} Topics")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.deselect_topic_ui(cx);
                    })),
            )
            .child({
                let name = div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(div().font_semibold().child(topic_name))
                    .when(
                        info.is_general && !info.name.eq_ignore_ascii_case("general"),
                        |this| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("General"),
                            )
                        },
                    )
                    .when(info.is_pinned, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Pinned"),
                        )
                    })
                    .when(info.is_closed, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Closed"),
                        )
                    });
                if unread > 0 {
                    unread_badge(name.into_any_element(), unread, false)
                } else {
                    name.into_any_element()
                }
            })
    }

    /// Phase 5.1: the topic list for a forum supergroup — one row per
    /// topic with name, unread badge, and a cheap last-message preview.
    /// Selecting a row opens the per-topic history.
    pub(super) fn forum_topics_pane(
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
                "No topics yet",
                "The topic list arrives via getForumTopics.",
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
                format!("Topic {}", topic.forum_topic_id)
            } else {
                topic.name.clone()
            };
            let preview = topic.last_message_preview.clone();
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
                    .bg(cx.theme().sidebar)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_topic_ui(topic_id, cx);
                    }))
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
                                                    .child("General"),
                                            )
                                        },
                                    )
                                    .when(topic.is_pinned, |this| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child("Pinned"),
                                        )
                                    })
                                    .when(topic.is_closed, |this| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child("Closed"),
                                        )
                                    });
                                if unread > 0 {
                                    unread_badge(name_el.into_any_element(), unread, false)
                                } else {
                                    name_el.into_any_element()
                                }
                            }),
                    )
                    .when(!preview.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(preview),
                        )
                    }),
            );
        }
        list.into_any_element()
    }

    /// Phase 5.1: select a forum topic (live: `searchChatMessages` with
    /// `topic_id`; demo: the seeded session takes it directly).
    pub(super) fn select_topic_ui(&mut self, forum_topic_id: i32, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .select_topic(forum_topic_id);
            self.status_note = match result {
                Ok(_) => "topic selected".into(),
                Err(_) => "could not open topic".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat_id) = session.open_chat {
                session.select_topic(chat_id, forum_topic_id);
            }
        }
        cx.notify();
    }

    /// Phase 5.1: leave the topic view, back to the forum's topic list.
    pub(super) fn deselect_topic_ui(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            self.live.as_mut().expect("live").driver.deselect_topic();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.deselect_topic();
        }
        self.status_note = "back to topics".into();
        cx.notify();
    }
}
