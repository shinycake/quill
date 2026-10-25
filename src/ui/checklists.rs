//! Checklist composer, task mutations and the poll "Add an Option" panel
//! (B15). tdesktop references: `edit_todo_list_box.cpp`,
//! `history_view_todo_list.cpp`, `history_view_poll.cpp`
//! (`lng_polls_add_option*`).
#![deny(clippy::indexing_slicing)]

use super::ai_compose::marked_link;
use super::app::QuillApp;
use super::dialogs::OptionMediaSlot;
use super::polls::AttachTarget;
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::checklist::{
    CHECKLIST_TASKS_MAX, can_create_checklist, tasks_left, toggle_task_request,
    validate_added_tasks,
};
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::poll::validate_new_option;
use quill::telegram::envelope::{MessageContent, MessageSender};

impl QuillApp {
    /// tdesktop `PeerMenuTodoWantsPremium`: the toast whose "Telegram
    /// Premium" opens the checklists preview.
    fn checklist_premium_toast(
        &mut self,
        key: quill::lang::Key,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let link = format!("**{}**", tr(keys::TODO_PREMIUM_LINK));
        let marked = marked_link(tr(key), "link", &link);
        let toast = Self::marked_toast("checklist-premium-toast", marked, cx, |this, _, cx| {
            this.open_premium_at(quill::premium_hub::PremiumFeature::Checklists, cx);
        });
        cx.defer_in(window, move |_, window, cx| {
            window.push_notification(toast, cx);
        });
    }

    /// Whether the attach menu offers "Checklist" in the open chat
    /// (`PeerData::canCreateTodoLists`).
    pub(super) fn checklist_creation_allowed(&self) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        session
            .open_chat
            .and_then(|id| session.chats.get(&id.0))
            .is_some_and(|chat| {
                chat.can_post() && can_create_checklist(chat, session.my_is_premium())
            })
    }

    pub(super) fn open_checklist_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.checklist_creation_allowed() {
            self.checklist_premium_toast(keys::TODO_CREATE_PREMIUM, window, cx);
            return;
        }
        self.composer_ui.checklist_dialog = Some(ChecklistDialog::new(None, window, cx));
        if let Some(dialog) = &self.composer_ui.checklist_dialog {
            dialog
                .title_input
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    /// "Add a task" under a checklist (`addChecklistTasks`).
    pub(super) fn open_checklist_add_tasks(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.session().is_some_and(|s| s.my_is_premium()) {
            self.checklist_premium_toast(keys::TODO_ADD_PREMIUM, window, cx);
            return;
        }
        self.composer_ui.checklist_dialog = Some(ChecklistDialog::new(
            Some((chat_id, message_id)),
            window,
            cx,
        ));
        if let Some(dialog) = self.composer_ui.checklist_dialog.as_ref()
            && let Some(first) = dialog.task_inputs.first()
        {
            first.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    pub(super) fn close_checklist_dialog(&mut self, cx: &mut Context<Self>) {
        self.composer_ui.checklist_dialog = None;
        cx.notify();
    }

    fn checklist_of(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Option<quill::telegram::envelope::Checklist> {
        self.session()
            .and_then(|session| session.histories.get(&chat_id.0))
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| match &message.content {
                MessageContent::Checklist(content) => Some(content.list.clone()),
                _ => None,
            })
    }

    /// Create the checklist (or add the typed tasks) through the driver.
    pub(super) fn submit_checklist_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.composer_ui.checklist_dialog.as_ref() else {
            return;
        };
        let add_to = dialog.add_to;
        let draft = dialog.draft(cx);
        // Surface validation inline, like tdesktop's field shake + toast.
        let error = match add_to {
            None => draft.validate(),
            Some((chat_id, message_id)) => self
                .checklist_of(chat_id, message_id)
                .and_then(|list| validate_added_tasks(&list, &draft.tasks).err()),
        };
        if let Some(reason) = error {
            if let Some(dialog) = self.composer_ui.checklist_dialog.as_mut() {
                dialog.error = Some(reason);
            }
            cx.notify();
            return;
        }
        if let Some((chat_id, message_id)) = add_to {
            if let Some(live) = self.live.as_mut() {
                let result = live
                    .driver
                    .add_checklist_tasks(chat_id, message_id, &draft.tasks);
                match result {
                    Ok(_) => {
                        self.composer_ui.checklist_dialog = None;
                        self.connection
                            .set_note(StatusNote::silent(tr(keys::Q_MEDIA_ADDING_TASKS)));
                    }
                    Err(_) => self
                        .connection
                        .set_note(StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_ADD_THE_TASKS))),
                }
            } else {
                self.apply_demo_checklist_tasks(chat_id, message_id, &draft.tasks);
                self.composer_ui.checklist_dialog = None;
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_MEDIA_TASKS_ADDED_DEMO)));
            }
            cx.notify();
            return;
        }
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MEDIA_SELECT_A_CHAT_TO_SEND)));
            cx.notify();
            return;
        };
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        let reply_to = self.send_reply_for(chat_id, cx);
        if let Some(live) = self.live.as_mut() {
            match live.driver.send_checklist_draft(chat_id, &draft, reply_to) {
                Ok(_) => {
                    self.composer_ui.checklist_dialog = None;
                    self.composer_ui.set_reply(None);
                    self.connection
                        .set_note(StatusNote::silent(tr(keys::Q_MEDIA_SENDING_CHECKLIST)));
                }
                Err(_) => self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MEDIA_COULD_NOT_SEND_THE_CHECKLIST,
                ))),
            }
        } else {
            self.composer_ui.checklist_dialog = None;
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_MEDIA_CHECKLISTS_NEED_A_LIVE_CONNECTION,
            )));
        }
        cx.notify();
    }

    /// Tap on a task: flip it (`markChecklistTasksAsDone`).
    pub(super) fn toggle_checklist_task(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        task_id: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            if !live.driver.session.my_is_premium() {
                self.checklist_premium_toast(keys::TODO_MARK_PREMIUM, window, cx);
                return;
            }
            self.connection.set_note(
                match live
                    .driver
                    .toggle_checklist_task(chat_id, message_id, task_id)
                {
                    Ok(_) => StatusNote::silent(tr(keys::Q_MEDIA_UPDATING_CHECKLIST)),
                    Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_UPDATE_THE_CHECKLIST)),
                },
            );
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_checklist_toggle(chat_id, message_id, task_id);
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MEDIA_CHECKLIST_UPDATED_DEMO)));
            cx.notify();
        }
    }

    /// Demo sessions have no TDLib: flip the mark in place, crediting the
    /// demo account like the server would.
    fn apply_demo_checklist_toggle(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        task_id: i32,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let me = session.my_user_id;
        let Some(message) = session
            .histories
            .get_mut(&chat_id.0)
            .and_then(|history| history.messages.get_mut(&message_id.0))
        else {
            return;
        };
        let MessageContent::Checklist(content) = &mut message.content else {
            return;
        };
        if toggle_task_request(&content.list, task_id).is_none() {
            return;
        }
        if let Some(task) = content.list.tasks.iter_mut().find(|t| t.id == task_id) {
            if task.is_done() {
                task.completion_date = 0;
                task.completed_by = None;
            } else {
                task.completion_date = quill::local_time::now_unix() as i32;
                task.completed_by = me.map(|user_id| MessageSender::User { user_id });
            }
        }
    }

    fn apply_demo_checklist_tasks(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        texts: &[String],
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let Some(message) = session
            .histories
            .get_mut(&chat_id.0)
            .and_then(|history| history.messages.get_mut(&message_id.0))
        else {
            return;
        };
        let MessageContent::Checklist(content) = &mut message.content else {
            return;
        };
        let Ok(tasks) = validate_added_tasks(&content.list, texts) else {
            return;
        };
        let ids = quill::checklist::next_task_ids(&content.list, tasks.len());
        for (id, text) in ids.into_iter().zip(tasks) {
            content
                .list
                .tasks
                .push(quill::telegram::envelope::ChecklistTask {
                    id,
                    text,
                    completed_by: None,
                    completion_date: 0,
                });
        }
    }

    /// The checklist composer / "Add Tasks" panel above the composer
    /// (tdesktop `EditTodoListBox`).
    pub(super) fn checklist_dialog_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.composer_ui.checklist_dialog.as_ref()?;
        self.checklist_dialog_panel_body(cx)
    }

    /// `checklist_dialog_panel` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn checklist_dialog_panel_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.composer_ui.checklist_dialog.as_ref()?;
        let adding = dialog.add_to.is_some();
        let remaining = match dialog.add_to {
            Some((chat_id, message_id)) => self
                .checklist_of(chat_id, message_id)
                .map(|list| tasks_left(&list))
                .unwrap_or(CHECKLIST_TASKS_MAX),
            None => CHECKLIST_TASKS_MAX,
        };
        let mut panel = div()
            .id("checklist-dialog")
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .child(div().text_sm().font_semibold().child(if adding {
                tr(keys::Q_MEDIA_ADD_TASKS)
            } else {
                tr(keys::Q_MEDIA_NEW_CHECKLIST)
            }));
        if !adding {
            panel = panel.child(
                Textarea::new(&dialog.title_input)
                    .aria_label(tr(keys::Q_MEDIA_CHECKLIST_TITLE))
                    .h(px(40.)),
            );
        }
        panel = panel.child(div().text_xs().text_color(text_muted()).child(if adding {
            tr(keys::Q_MEDIA_TASKS)
        } else {
            tr(keys::Q_MEDIA_TASKS_LIST)
        }));
        for (index, input) in dialog.task_inputs.iter().enumerate() {
            let mut row = div()
                .id(("checklist-task-input", index as u64))
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div().flex_1().child(
                        Textarea::new(input)
                            .aria_label(tr(keys::Q_MEDIA_CHECKLIST_TASK))
                            .h(px(40.)),
                    ),
                );
            if dialog.task_inputs.len() > 1 {
                row = row.child(
                    Button::new(format!("checklist-remove-task-{index}"))
                        .label("\u{2715}")
                        .ghost()
                        .tooltip(tr(keys::Q_MEDIA_REMOVE_TASK))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(dialog) = this.composer_ui.checklist_dialog.as_mut()
                                && index < dialog.task_inputs.len()
                            {
                                dialog.task_inputs.remove(index);
                            }
                            cx.notify();
                        })),
                );
            }
            panel = panel.child(row);
        }
        let left = remaining.saturating_sub(dialog.task_inputs.len());
        if left > 0 {
            panel = panel
                .child(
                    Button::new("checklist-add-task-row")
                        .label(tr(keys::Q_MEDIA_ADD_A_TASK))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(dialog) = this.composer_ui.checklist_dialog.as_mut() {
                                dialog.push_task_row(window, cx);
                            }
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(text_muted())
                        .child(if left == 1 {
                            tr(keys::Q_MEDIA_YOU_CAN_ADD_1_MORE).to_string()
                        } else {
                            tr_with(
                                keys::Q_MEDIA_YOU_CAN_ADD_MORE_TASKS,
                                &[("left", &left.to_string())],
                            )
                        }),
                );
        } else {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(text_muted())
                    .child(tr(keys::TODO_CREATE_MAXIMUM)),
            );
        }
        if !adding {
            panel = panel
                .child(
                    Checkbox::new("checklist-others-add")
                        .checked(dialog.others_can_add_tasks)
                        .label(tr(keys::TODO_CREATE_ALLOW_ADD))
                        .on_click(cx.listener(|this, &on, _, cx| {
                            if let Some(dialog) = this.composer_ui.checklist_dialog.as_mut() {
                                dialog.others_can_add_tasks = on;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Checkbox::new("checklist-others-mark")
                        .checked(dialog.others_can_mark_tasks_as_done)
                        .label(tr(keys::TODO_CREATE_ALLOW_MARK))
                        .on_click(cx.listener(|this, &on, _, cx| {
                            if let Some(dialog) = this.composer_ui.checklist_dialog.as_mut() {
                                dialog.others_can_mark_tasks_as_done = on;
                            }
                            cx.notify();
                        })),
                );
        }
        if let Some(error) = dialog.error {
            panel = panel.child(
                div()
                    .id("checklist-error")
                    .text_xs()
                    .text_color(danger())
                    .child(error),
            );
        }
        Some(
            panel
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("checklist-create")
                                .label(if adding {
                                    tr(keys::Q_MEDIA_ADD)
                                } else {
                                    tr(keys::Q_MEDIA_CREATE)
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.submit_checklist_dialog(cx);
                                })),
                        )
                        .child(
                            Button::new("checklist-cancel")
                                .label(tr(keys::CANCEL))
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.close_checklist_dialog(cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------------
    // Poll: "Add an Option"
    // ------------------------------------------------------------------

    /// Open the inline "Add an Option" panel (`lng_polls_add_option`).
    pub(super) fn open_poll_add_option(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input: Entity<TextareaState> = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MEDIA_OPTION_TEXT))
                .auto_grow(1, 2)
                .submit_on_enter(false)
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        self.message_ui.poll_add_option = Some(PollAddOption {
            chat_id,
            message_id,
            input,
            attach: Default::default(),
            error: None,
        });
        cx.notify();
    }

    pub(super) fn close_poll_add_option(&mut self, cx: &mut Context<Self>) {
        self.message_ui.poll_add_option = None;
        cx.notify();
    }

    pub(super) fn submit_poll_add_option(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.message_ui.poll_add_option.as_ref() else {
            return;
        };
        let (chat_id, message_id) = (panel.chat_id, panel.message_id);
        let text = super::composer_field::field_doc(panel.input.read(cx)).to_markup();
        let poll = self
            .session()
            .and_then(|session| session.histories.get(&chat_id.0))
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| match &message.content {
                MessageContent::Poll(content) => Some(content.poll.clone()),
                _ => None,
            });
        let Some(poll) = poll else {
            self.message_ui.poll_add_option = None;
            cx.notify();
            return;
        };
        let plain = quill::composer::parse_format_markup(&text).0;
        let text = match validate_new_option(&poll, &plain) {
            Ok(_) => text.trim().to_string(),
            Err(reason) => {
                if let Some(panel) = self.message_ui.poll_add_option.as_mut() {
                    panel.error = Some(reason);
                }
                cx.notify();
                return;
            }
        };
        if self
            .message_ui
            .poll_add_option
            .as_ref()
            .is_some_and(|panel| matches!(panel.attach.media, OptionMediaSlot::Preparing(_)))
        {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_MEDIA_WAIT_FOR_THE_VIDEO_TO)));
            cx.notify();
            return;
        }
        let media = self
            .message_ui
            .poll_add_option
            .as_ref()
            .and_then(|panel| panel.attach.media.ready().cloned());
        if let Some(live) = self.live.as_mut() {
            match live
                .driver
                .add_poll_option(chat_id, message_id, &text, media.as_ref())
            {
                Ok(_) => {
                    self.message_ui.poll_add_option = None;
                    self.connection
                        .set_note(StatusNote::silent(tr(keys::Q_MEDIA_ADDING_OPTION)));
                }
                Err(_) => {
                    self.connection
                        .set_note(StatusNote::error(tr(keys::POLLS_ADD_OPTION_ERROR)));
                }
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(message) = session
                .histories
                .get_mut(&chat_id.0)
                .and_then(|history| history.messages.get_mut(&message_id.0))
                && let MessageContent::Poll(content) = &mut message.content
            {
                content
                    .poll
                    .options
                    .push(quill::telegram::PollOption::plain(
                        format!("added-{}", content.poll.options.len()),
                        text,
                    ));
            }
            self.message_ui.poll_add_option = None;
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MEDIA_OPTION_ADDED_DEMO)));
        }
        cx.notify();
    }

    pub(super) fn poll_add_option_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.message_ui.poll_add_option.as_ref()?;
        self.poll_add_option_panel_body(cx)
    }

    /// `poll_add_option_panel` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn poll_add_option_panel_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let panel = self.message_ui.poll_add_option.as_ref()?;
        let mut body = div()
            .id("poll-add-option-panel")
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child(tr(keys::Q_MEDIA_ADD_AN_OPTION)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div().flex_1().child(
                            Textarea::new(&panel.input)
                                .aria_label(tr(keys::Q_MEDIA_NEW_POLL_OPTION))
                                .token(self.field_token_renderer(&panel.input, cx))
                                .h(px(40.)),
                        ),
                    )
                    .child(self.poll_option_emoji_button(AttachTarget::Added, cx))
                    .child(self.poll_option_attach_button(AttachTarget::Added, cx)),
            )
            .children(self.poll_option_attach_area(AttachTarget::Added, cx));
        if let Some(error) = panel.error {
            body = body.child(
                div()
                    .id("poll-add-option-error")
                    .text_xs()
                    .text_color(danger())
                    .child(error),
            );
        }
        Some(
            body.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("poll-add-option-save")
                            .label(tr(keys::SETTINGS_SAVE))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.submit_poll_add_option(cx);
                            })),
                    )
                    .child(
                        Button::new("poll-add-option-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_poll_add_option(cx);
                            })),
                    ),
            )
            .into_any_element(),
        )
    }
}
