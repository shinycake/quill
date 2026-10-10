//! forum topics.

use super::app::QuillApp;
use super::app::pane_placeholder;
use super::chat_row::unread_badge;
use super::dialogs::TopicEditor;
use super::pressable::PressableDiv;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::local_path::sandboxed_display_path;
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::ForumTopic;
use quill::telegram::requests::TOPIC_ICON_COLORS;
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
            r#"{{"info":{{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":{id},"name":"{name}","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"creation_date":1760000000,"creator_id":{{"@type":"messageSenderUser","user_id":6}},"is_general":{general},"is_outgoing":false,"is_closed":{closed},"is_hidden":false,"is_name_implicit":false}},{last_message},"order":"{order}","is_pinned":{pinned},"unread_count":{unread},"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"unread_poll_vote_count":0,"notification_settings":{{"@type":"chatNotificationSettings"}},"draft_message":null}}"#,
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

    /// Open the manage dialog straight on the topic editor (the topic
    /// menu's "Edit Topic", the header's "New Topic").
    pub(super) fn open_forum_topic_editor(
        &mut self,
        chat_id: ChatId,
        topic_id: Option<i32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.forum_manage_dialog.is_none() {
            self.open_forum_manage_dialog(chat_id, window, cx);
        }
        self.begin_topic_editor(topic_id, window, cx);
    }

    /// Start the editor for a new topic (`None`) or an existing one: the
    /// name and icon of the topic fill the form; a new topic gets the next
    /// of tdesktop's six colors.
    pub(super) fn begin_topic_editor(
        &mut self,
        topic_id: Option<i32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(chat_id) = self.forum_manage_dialog.as_ref().map(|d| d.chat_id) else {
            return;
        };
        let topics = self
            .session()
            .map(|s| s.ordered_forum_topics(chat_id))
            .unwrap_or_default();
        let existing = topic_id.and_then(|id| topics.iter().find(|t| t.forum_topic_id == id));
        let (name, color, emoji) = match existing {
            Some(topic) => (
                topic.name.clone(),
                topic.icon_color,
                topic.icon_custom_emoji_id,
            ),
            None => (
                String::new(),
                TOPIC_ICON_COLORS[topics.len() % TOPIC_ICON_COLORS.len()],
                0,
            ),
        };
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.load_forum_topic_icons();
        }
        if let Some(dialog) = self.forum_manage_dialog.as_mut() {
            dialog.name_input.update(cx, |input, cx| {
                input.set_value(name, window, cx);
            });
            dialog.editor = Some(TopicEditor {
                target: topic_id,
                color,
                icon_emoji: emoji,
                original_emoji: emoji,
            });
        }
        cx.notify();
    }

    /// Clicking the icon preview picks the next color, while the topic is
    /// new and has no custom emoji (tdesktop `ChooseNextColorId`).
    fn cycle_topic_editor_color(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self
            .forum_manage_dialog
            .as_mut()
            .and_then(|d| d.editor.as_mut())
            .filter(|e| e.target.is_none() && e.icon_emoji == 0)
        {
            editor.color = next_topic_color(editor.color);
        }
        cx.notify();
    }

    fn set_topic_editor_emoji(&mut self, custom_emoji_id: i64, cx: &mut Context<Self>) {
        if let Some(editor) = self
            .forum_manage_dialog
            .as_mut()
            .and_then(|d| d.editor.as_mut())
        {
            editor.icon_emoji = custom_emoji_id;
        }
        cx.notify();
    }

    /// Leave the editor without sending anything.
    fn cancel_topic_editor(&mut self, cx: &mut Context<Self>) {
        if let Some(dialog) = self.forum_manage_dialog.as_mut() {
            dialog.editor = None;
        }
        cx.notify();
    }

    /// Create or save the topic (`createForumTopic` / `editForumTopic`
    /// with the chosen icon); an empty name is refused up front.
    pub(super) fn submit_topic_editor(&mut self, cx: &mut Context<Self>) {
        let Some((chat_id, editor, name)) = self.forum_manage_dialog.as_ref().and_then(|d| {
            Some((
                d.chat_id,
                d.editor.clone()?,
                d.name_input.read(cx).value().trim().to_string(),
            ))
        }) else {
            return;
        };
        if name.is_empty() {
            self.status_note = "topic name cannot be empty".into();
            cx.notify();
            return;
        }
        let general = editor.target.is_some_and(|id| {
            self.session().is_some_and(|s| {
                s.forum_topics.get(&chat_id.0).is_some_and(|topics| {
                    topics
                        .iter()
                        .any(|t| t.forum_topic_id == id && t.is_general)
                })
            })
        });
        if let Some(live) = self.live.as_mut() {
            let result = match editor.target {
                None => live.driver.create_forum_topic_with_icon(
                    chat_id,
                    &name,
                    editor.color,
                    editor.icon_emoji,
                ),
                // The General topic only has a name.
                Some(id) if general => live.driver.edit_forum_topic(chat_id, id, &name),
                Some(id) => {
                    live.driver
                        .edit_forum_topic_with_icon(chat_id, id, &name, editor.icon_emoji)
                }
            };
            self.status_note = match (result, editor.target) {
                (Ok(_), None) => "topic created".into(),
                (Ok(_), Some(_)) => "topic saved".into(),
                (Err(_), None) => "could not create topic".into(),
                (Err(_), Some(_)) => "could not save topic".into(),
            };
        } else {
            self.status_note = "topics need a live connection (demo)".into();
        }
        if let Some(dialog) = self.forum_manage_dialog.as_mut() {
            dialog.editor = None;
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
            let Some(dialog_state) = this.forum_manage_dialog.as_ref() else {
                return dialog.overlay(true).on_close(on_close);
            };
            let chat_id = dialog_state.chat_id;
            let editor = dialog_state.editor.clone();
            let title = match &editor {
                None => "Manage topics",
                Some(e) if e.target.is_none() => "New Topic",
                Some(_) => "Edit Topic",
            };
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title));
            if editor.is_some()
                && let Some(live) = this.live.as_mut()
            {
                live.driver.download_forum_topic_icons();
            }
            let body =
                match editor {
                    Some(editor) => this.topic_editor_body(&editor, cx),
                    None => {
                        let topics: Vec<ForumTopic> = this
                            .session()
                            .map(|session| session.ordered_forum_topics(chat_id))
                            .unwrap_or_default();
                        let mut list = div()
                            .id("g2-topic-list")
                            .flex()
                            .flex_col()
                            .gap_1()
                            .max_h(px(320.))
                            .overflow_y_scroll();
                        for topic in &topics {
                            list = list.child(this.forum_topic_manage_row(chat_id, topic, cx));
                        }
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().flex().justify_end().child(
                                Button::new("g2-topic-new").label("New Topic").on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.begin_topic_editor(None, window, cx);
                                    }),
                                ),
                            ))
                            .child(list)
                            .into_any_element()
                    }
                };
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body)));
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

    /// The create / edit form (tdesktop `EditForumTopicBox`): the icon
    /// preview next to the name, then the icon choices, then the buttons.
    fn topic_editor_body(&self, editor: &TopicEditor, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.forum_manage_dialog.as_ref() else {
            return div().into_any_element();
        };
        let session = self.session();
        let name = dialog.name_input.read(cx).value().to_string();
        let roots = self.media_display_roots();
        let sticker_for = |id: i64| {
            session.and_then(|s| {
                s.forum_topic_icons
                    .iter()
                    .find(|sticker| sticker.custom_emoji_id == Some(id))
            })
        };
        let path_for = |sticker: &quill::telegram::envelope::StickerItem| {
            sticker
                .display_file_id()
                .and_then(|id| session.and_then(|s| s.files.get(&id.0)))
                .and_then(|file| file.usable_path())
                .and_then(|path| sandboxed_display_path(path, &roots))
        };
        let tile = |selected: bool| {
            let base = div()
                .size(px(40.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_md();
            if selected {
                base.bg(cx.theme().primary.opacity(0.25))
            } else {
                base
            }
        };
        let visual = |emoji: i64, size: f32| -> AnyElement {
            match sticker_for(emoji) {
                Some(sticker) if emoji != 0 => match path_for(sticker) {
                    Some(path) => img(path)
                        .w(px(size))
                        .h(px(size))
                        .object_fit(ObjectFit::Contain)
                        .into_any_element(),
                    None => div()
                        .text_size(px(size * 0.7))
                        .child(sticker.emoji.clone())
                        .into_any_element(),
                },
                _ => letter_icon(&name, editor.color, size),
            }
        };
        let changeable = editor.target.is_none() && editor.icon_emoji == 0;
        let mut grid = div()
            .id("topic-icon-grid")
            .flex()
            .flex_wrap()
            .gap_1()
            .max_h(px(200.))
            .overflow_y_scroll()
            .role(Role::List)
            .aria_label("Topic icons")
            .child(
                tile(editor.icon_emoji == 0)
                    .id("topic-icon-default")
                    .pressable(cx.theme())
                    .role(Role::Button)
                    .aria_label("Default icon")
                    .tab_index(0)
                    .on_click(cx.listener(|this, _, _, cx| this.set_topic_editor_emoji(0, cx)))
                    .child(letter_icon(&name, editor.color, 28.)),
            );
        for (ix, sticker) in session
            .map(|s| s.forum_topic_icons.as_slice())
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let Some(id) = sticker.custom_emoji_id else {
                continue;
            };
            grid = grid.child(
                tile(editor.icon_emoji == id)
                    .id(("topic-icon", ix))
                    .pressable(cx.theme())
                    .role(Role::Button)
                    .aria_label(if sticker.emoji.is_empty() {
                        "Topic icon".to_string()
                    } else {
                        format!("Topic icon {}", sticker.emoji)
                    })
                    .tab_index(0)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_topic_editor_emoji(id, cx);
                    }))
                    .child(visual(id, 28.)),
            );
        }
        let loading = session.is_some_and(|s| {
            s.forum_topic_icons.is_empty()
                && s.requests
                    .has_purpose(RequestPurpose::GetForumTopicDefaultIcons)
        });
        let creating = editor.target.is_none();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("topic-icon-preview")
                            .size(px(44.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .role(Role::Button)
                            .aria_label(if changeable {
                                "Change icon color"
                            } else {
                                "Topic icon"
                            })
                            .when(changeable, |this| {
                                this.pressable(cx.theme()).tab_index(0).on_click(
                                    cx.listener(|this, _, _, cx| this.cycle_topic_editor_color(cx)),
                                )
                            })
                            .child(visual(editor.icon_emoji, 36.)),
                    )
                    .child(
                        div().flex_1().child(
                            Textarea::new(&dialog.name_input)
                                .aria_label("Topic name")
                                .h(px(36.)),
                        ),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Choose a topic name and icon"),
            )
            .child(grid)
            .when(loading, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading icons"),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("topic-editor-cancel")
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_topic_editor(cx))),
                    )
                    .child(
                        Button::new("topic-editor-save")
                            .label(if creating { "Create" } else { "Save" })
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| this.submit_topic_editor(cx))),
                    ),
            )
            .into_any_element()
    }

    /// Slice G2: one topic row in the management dialog — name +
    /// state badges and the applicable actions (General only gets
    /// Hide/Show).
    pub(super) fn forum_topic_manage_row(
        &self,
        chat_id: ChatId,
        topic: &ForumTopic,
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
        let row = div().flex().flex_col().w_full().gap_1().child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(super::subsection_tabs::topic_icon(topic, 22., cx))
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
        let mut actions = div().flex().flex_wrap().gap_1();
        actions = actions.child(
            Button::new(format!("g2-topic-rename-{topic_id}"))
                .label("Edit")
                .ghost()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.begin_topic_editor(Some(topic_id), window, cx);
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
        row.child(actions).into_any_element()
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
            .child(
                Button::new("forum-topic-info-toggle")
                    .icon(gpui_kit::assets::IconName::Info)
                    .ghost()
                    .tooltip("Topic info")
                    .accessibility_label("Topic info")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.topic_info_open = !this.topic_info_open;
                        cx.notify();
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
                                PopupMenuItem::new(if pinned { "Unpin" } else { "Pin" })
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

    /// Where the forum topic column sits for this window; also records
    /// whether it is on screen for the conversation pane. A peek at the chat
    /// list ends once the open chat is no longer a forum.
    pub(super) fn forum_column_layout(&mut self, window: &Window) -> quill::state::ForumColumn {
        let width = f32::from(window.viewport_size().width);
        let layout = self
            .session()
            .map_or(quill::state::ForumColumn::Hidden, |s| {
                s.forum_column(width, self.forum_chats_peek)
            });
        if self.forum_chats_peek
            && self
                .session()
                .is_some_and(|s| s.forum_column(width, false) == quill::state::ForumColumn::Hidden)
        {
            self.forum_chats_peek = false;
        }
        self.forum_column_shown = layout != quill::state::ForumColumn::Hidden;
        layout
    }

    /// The forum's topic list as its own column (tdesktop shows the topics
    /// where the chat list was): the forum's name and topic count on top,
    /// then the topics. On a narrow window a "Chats" button brings the
    /// chat list back.
    pub(super) fn forum_column_view(
        &self,
        open: Option<ChatId>,
        replacing: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = open
            .and_then(|id| self.session().and_then(|s| s.chats.get(&id.0)))
            .map(|chat| chat.title.clone())
            .unwrap_or_default();
        let count = open
            .and_then(|id| self.session().map(|s| s.ordered_forum_topics(id).len()))
            .unwrap_or(0);
        div()
            .id("forum-column")
            .flex()
            .flex_col()
            .flex_none()
            .w(px(quill::state::FORUM_COLUMN_WIDTH))
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
                    .when(replacing, |this| {
                        this.child(
                            Button::new("forum-column-chats")
                                .icon(gpui_kit::assets::IconName::ChevronLeft)
                                .ghost()
                                .tooltip("Show chats")
                                .accessibility_label("Show chats")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.forum_chats_peek = true;
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
                                        1 => "1 topic".to_owned(),
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
        } else if let Some(session) = self.demo_session.as_mut()
            && let Some(chat_id) = session.open_chat
        {
            session.select_topic(chat_id, forum_topic_id);
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

/// The color after `current` in tdesktop's list (`ChooseNextColorId`
/// without the randomness, so a click always changes the color).
fn next_topic_color(current: i32) -> i32 {
    let at = TOPIC_ICON_COLORS.iter().position(|c| *c == current);
    TOPIC_ICON_COLORS[at.map_or(0, |i| (i + 1) % TOPIC_ICON_COLORS.len())]
}

/// The first letter of a topic name for its round icon; "#" when the
/// name has none yet.
pub(super) fn topic_letter(name: &str) -> String {
    name.chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "#".to_string())
}

/// A colored round icon with the topic name's first letter.
fn letter_icon(name: &str, color: i32, size: f32) -> AnyElement {
    let fill: Hsla = if color > 0 {
        rgb(color as u32).into()
    } else {
        accent().into()
    };
    div()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(fill)
        .text_color(gpui_kit::white())
        .text_size(px(size * 0.5))
        .font_semibold()
        .child(topic_letter(name))
        .into_any_element()
}

crate::ui::shell::register_dialogs! {
    ForumManage => DialogSpec::new(
        5200,
        |app| app.forum_manage_dialog.is_some(),
        QuillApp::build_forum_manage_dialog,
    ),
}

#[cfg(test)]
mod forum_editor_tests {
    use super::{next_topic_color, topic_letter};
    use quill::telegram::requests::TOPIC_ICON_COLORS;

    #[test]
    fn colors_cycle_through_the_six_defaults() {
        let mut color = TOPIC_ICON_COLORS[0];
        let mut seen = Vec::new();
        for _ in 0..6 {
            seen.push(color);
            color = next_topic_color(color);
        }
        assert_eq!(color, TOPIC_ICON_COLORS[0]);
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 6);
        // An unknown color (an older topic) starts the cycle over.
        assert_eq!(next_topic_color(1), TOPIC_ICON_COLORS[0]);
    }

    #[test]
    fn the_icon_letter_skips_symbols() {
        assert_eq!(topic_letter("  ideas"), "I");
        assert_eq!(topic_letter("\u{1f525} news"), "N");
        assert_eq!(topic_letter(""), "#");
    }
}
