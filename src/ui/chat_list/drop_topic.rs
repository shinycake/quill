//! The topic chooser a drop on a forum row opens (tdesktop `ShowDropMediaBox`
//! with `ChooseTopicBoxController`): a "Forward to…" box with a search field
//! and the forum's topics. Choosing a topic opens the chat in that topic and
//! sends what was dropped there (files attach to the composer, messages wait
//! in the forward bar); Cancel, Escape or closing the box does nothing. A
//! bot with topics adds an "All Messages" row for the chat itself.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use quill::composer::{SendKeyMode, should_send_on_enter};
use quill::drop_modes::{DragState, DropAction};
use quill::drop_topic::topics_for;
use quill::forward_drag::ForwardDrag;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// What the drop brought: files from the desktop or messages from a history.
#[derive(Clone)]
pub(in crate::ui) enum DropPayload {
    Files {
        paths: Vec<PathBuf>,
        kind: DragState,
    },
    Forward(ForwardDrag),
}

/// A drop that waits for its topic.
pub(in crate::ui) struct DropTopic {
    chat_id: ChatId,
    payload: DropPayload,
    search: Entity<TextareaState>,
    /// The request for the forum's topics could not be sent.
    load_failed: bool,
}

/// One row of the chooser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct TopicChoice {
    /// `None` is the chat itself ("All Messages").
    pub topic: Option<i32>,
    pub name: String,
    pub closed: bool,
}

impl QuillApp {
    /// A drop on the forum row of `chat_id` asks for a topic first.
    pub(in crate::ui) fn open_drop_topic(
        &mut self,
        chat_id: ChatId,
        payload: DropPayload,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::DLG_FILTER))
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        cx.subscribe_in(
            &search,
            window,
            |this, state, event: &InputEvent, window, cx| {
                this.in_window_of(window, cx, |this, window, cx| {
                    this.on_drop_topic_search(state, event, window, cx)
                });
            },
        )
        .detach();
        // The topics of a forum that is not open are not loaded yet.
        let load_failed = self
            .live
            .as_mut()
            .is_some_and(|live| live.driver.load_forum_topics(chat_id).is_err());
        self.chat_list.drop_topic = Some(DropTopic {
            chat_id,
            payload,
            search,
            load_failed,
        });
        cx.notify();
    }

    fn on_drop_topic_search(
        &mut self,
        state: &Entity<TextareaState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            cx.notify();
        }
        if let InputEvent::PressEnter { secondary, shift } = event {
            let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
            if should_send_on_enter(
                quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                SendKeyMode::Enter,
            ) {
                let first = self.drop_topic_choices(cx).into_iter().next();
                if let Some(choice) = first {
                    self.choose_drop_topic(choice.topic, window, cx);
                    self.close_kit_dialog_if_done(DialogKind::DropTopic, window, cx);
                }
            }
        }
    }

    /// The rows the chooser lists for what is typed, in the forum's order.
    pub(in crate::ui) fn drop_topic_choices(&self, cx: &App) -> Vec<TopicChoice> {
        let (Some(waiting), Some(session)) = (self.chat_list.drop_topic.as_ref(), self.session())
        else {
            return Vec::new();
        };
        let query = waiting.search.read(cx).value().to_string();
        let mut choices = Vec::new();
        if let Some(bot) = session.bot_topics(waiting.chat_id) {
            let name = if bot.allows_users_to_create_topics {
                tr(keys::FORUM_CREATE_NEW_TOPIC)
            } else {
                tr(keys::FORUM_ALL_MESSAGES)
            };
            if quill::drop_topic::name_matches(&query, name) {
                choices.push(TopicChoice {
                    topic: None,
                    name: name.to_string(),
                    closed: false,
                });
            }
        }
        let topics = session.ordered_forum_topics(waiting.chat_id);
        choices.extend(
            topics_for(&topics, &query)
                .into_iter()
                .map(|topic| TopicChoice {
                    topic: Some(topic.forum_topic_id),
                    name: topic.name.clone(),
                    closed: topic.is_closed,
                }),
        );
        choices
    }

    /// Whether the forum's topic list has arrived (an empty list before it
    /// shows as loading, not as "No topics found").
    fn drop_topic_loaded(&self) -> bool {
        let (Some(waiting), Some(session)) = (self.chat_list.drop_topic.as_ref(), self.session())
        else {
            return true;
        };
        session
            .threads
            .forum_topics
            .contains_key(&waiting.chat_id.0)
            || session.bot_topics(waiting.chat_id).is_some()
    }

    /// Whether the chat the chooser waits on is still a forum of this
    /// account (tdesktop closes the box when the forum is destroyed).
    pub(in crate::ui) fn drop_topic_alive(&self) -> bool {
        match (self.chat_list.drop_topic.as_ref(), self.session()) {
            (Some(waiting), Some(session)) => session.chat_shown_as_forum(waiting.chat_id),
            _ => false,
        }
    }

    /// Drops a chooser whose forum is gone, left or of another account.
    pub(in crate::ui) fn prune_drop_topic(&mut self) {
        if self.chat_list.drop_topic.is_some() && !self.drop_topic_alive() {
            self.chat_list.drop_topic = None;
        }
    }

    fn drop_topic_failed(&self) -> bool {
        self.chat_list
            .drop_topic
            .as_ref()
            .is_some_and(|waiting| waiting.load_failed)
    }

    fn close_drop_topic(&mut self, cx: &mut Context<Self>) {
        if self.chat_list.drop_topic.take().is_some() {
            cx.notify();
        }
    }

    /// A topic was picked: the chat opens in it and the drop is completed
    /// there (`filesOrForwardDrop(thread, data, forumResolved = true)`). A
    /// refusal or a vanished topic leaves the chooser open with a note.
    pub(in crate::ui) fn choose_drop_topic(
        &mut self,
        topic: Option<i32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((chat_id, payload)) = self
            .chat_list
            .drop_topic
            .as_ref()
            .map(|waiting| (waiting.chat_id, waiting.payload.clone()))
        else {
            return;
        };
        let refusal = match &payload {
            DropPayload::Files { .. } => self.start_files_refusal(chat_id).map(str::to_string),
            DropPayload::Forward(_) => self.share_dest_refusal(chat_id, cx).map(|n| n.to_string()),
        };
        if let Some(note) = refusal {
            self.connection.set_note(StatusNote::error(note));
            cx.notify();
            return;
        }
        let gone = topic.is_some_and(|id| {
            self.session().is_none_or(|s| {
                !s.ordered_forum_topic_refs(chat_id)
                    .iter()
                    .any(|t| t.forum_topic_id == id)
            })
        });
        if gone {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_OPEN_TOPIC)));
            cx.notify();
            return;
        }
        self.chat_list.drop_topic = None;
        cx.notify();
        if !self.enter_drop_topic(chat_id, topic, window, cx) {
            return;
        }
        match &payload {
            DropPayload::Files { paths, kind } => match kind {
                DragState::Folder => self.drop_on_zone(DropAction::Archive, paths, cx),
                _ => self.drop_files_plain(paths, cx),
            },
            DropPayload::Forward(drag) => {
                self.set_forward_bar(chat_id, topic, drag, cx);
            }
        }
    }

    /// Opens `chat_id` and, for a topic, selects it; "All Messages" leaves
    /// the topic the chat was open in. False when the chat did not become
    /// the open one (it shows in a window of its own) or the topic is gone.
    fn enter_drop_topic(
        &mut self,
        chat_id: ChatId,
        topic: Option<i32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.select_listed_chat(chat_id, window, cx);
        if self.session().and_then(|s| s.open_chat) != Some(chat_id) {
            return false;
        }
        match topic {
            Some(topic) => {
                self.select_topic_ui(topic, cx);
                if self.session().and_then(|s| s.open_topic) != Some(topic) {
                    self.connection
                        .set_note(StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_OPEN_TOPIC)));
                    return false;
                }
            }
            None => {
                if self.session().and_then(|s| s.open_topic).is_some() {
                    self.deselect_topic_ui(cx);
                }
            }
        }
        true
    }

    pub(in crate::ui) fn build_drop_topic_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::DropTopic, |this, _, cx| {
                this.close_drop_topic(cx);
            });
        app.update(cx, |this, cx| {
            let Some(search) = this
                .chat_list
                .drop_topic
                .as_ref()
                .map(|waiting| waiting.search.clone())
            else {
                return dialog.on_close(on_close);
            };
            let choices = this.drop_topic_choices(cx);
            let mut list = div().id("drop-topic-list").flex().flex_col().gap_1();
            if choices.is_empty() {
                let text = if this.drop_topic_failed() {
                    tr(keys::Q_CHATLIST_TOPICS_COULD_NOT_LOAD)
                } else if this.drop_topic_loaded() {
                    tr(keys::TOPICS_NOT_FOUND)
                } else {
                    tr(keys::CONTACTS_LOADING)
                };
                list = list.child(div().text_sm().text_color(text_muted()).child(text));
            }
            for choice in choices {
                let topic = choice.topic;
                let id = SharedString::from(format!("drop-topic-{}", topic.unwrap_or(0)));
                let mut row = Button::new(id)
                    .label(choice.name)
                    .ghost()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_drop_topic(topic, window, cx);
                        this.close_kit_dialog_if_done(DialogKind::DropTopic, window, cx);
                    }));
                if choice.closed {
                    row = row.icon(gpui_kit::assets::IconName::Lock);
                }
                list = list.child(row);
            }
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    Textarea::new(&search)
                        .aria_label(tr(keys::DLG_FILTER))
                        .h(px(36.)),
                )
                .child(list)
                .into_any_element();
            let footer = div().flex().justify_end().gap_2().child(
                Button::new("drop-topic-cancel")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_drop_topic(cx);
                        this.close_kit_dialog_if_done(DialogKind::DropTopic, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::FORWARD_CHOOSE)))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// A drop on a forum row: which topic gets the files or messages.
    DropTopic => DialogSpec::new(
        2907,
        |app| app.drop_topic_alive(),
        QuillApp::build_drop_topic_dialog,
    ),
}

#[cfg(feature = "demo-capture")]
crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chats(
        "ready-drop-topic",
        "screenshot demo — chat list: the topic chooser a file drop on a forum opens"
    )
    .setup(QuillApp::demo_drop_topic),
];

#[cfg(feature = "demo-capture")]
impl QuillApp {
    /// A file dropped on the "Dev Forum" row of the forum-rows demo.
    fn demo_drop_topic(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_forum_row_topics(window, cx);
        let payload = DropPayload::Files {
            paths: vec![PathBuf::from("quarterly-report.pdf")],
            kind: DragState::Files,
        };
        self.open_drop_topic(ChatId(71), payload, window, cx);
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "drop_topic_tests.rs"]
mod tests;
