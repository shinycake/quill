//! welcome messages.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::state::WelcomeMessagesFetch;
use quill::telegram::envelope::ParsedWelcomeMessage;
use std::cell::RefCell;
use std::rc::Rc;
impl QuillApp {
    /// Slice G2: open the welcome-message editor and load the pack.
    pub(super) fn open_welcome_dialog(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.admin.welcome_dialog = Some(WelcomeDialog::new(window, cx, chat_id));
        if let Some(live) = self.live.as_mut()
            && live.driver.load_chat_welcome_messages(chat_id).is_err()
        {
            self.connection.status_note = tr(keys::Q_PEOPLE_COULD_NOT_LOAD_WELCOME_MESSAGES).into();
        }
        cx.notify();
    }

    pub(super) fn close_welcome_dialog(&mut self, cx: &mut Context<Self>) {
        self.admin.welcome_dialog = None;
        cx.notify();
    }

    /// Slice G2: add the welcome message typed in the dialog
    /// (`addChatWelcomeMessage`); empty text is refused up front.
    pub(super) fn submit_welcome_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (chat_id, text) = match self.admin.welcome_dialog.as_ref() {
            Some(dialog) => (
                dialog.chat_id,
                dialog.new_input.read(cx).value().trim().to_string(),
            ),
            None => return,
        };
        if text.is_empty() {
            self.connection.status_note = tr(keys::Q_PEOPLE_WELCOME_MESSAGE_CANNOT_BE_EMPTY).into();
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.add_chat_welcome_message(chat_id, &text) {
                Ok(_) => {
                    self.connection.status_note = tr(keys::Q_PEOPLE_WELCOME_MESSAGE_ADDED).into()
                }
                Err(_) => {
                    self.connection.status_note =
                        tr(keys::Q_PEOPLE_COULD_NOT_ADD_WELCOME_MESSAGE).into()
                }
            }
        } else {
            self.connection.status_note = tr(keys::Q_PEOPLE_WELCOME_MESSAGES_NEED_A_LIVE).into();
        }
        if let Some(dialog) = self.admin.welcome_dialog.as_mut() {
            dialog.new_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
        }
        cx.notify();
    }

    /// Slice G2: submit the inline welcome-message edit.
    pub(super) fn submit_welcome_edit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let (chat_id, welcome_id, text) = match self.admin.welcome_dialog.as_ref() {
            Some(dialog) => match dialog.editing {
                Some(welcome_id) => (
                    dialog.chat_id,
                    welcome_id,
                    dialog.edit_input.read(cx).value().trim().to_string(),
                ),
                None => return,
            },
            None => return,
        };
        if text.is_empty() {
            self.connection.status_note = tr(keys::Q_PEOPLE_WELCOME_MESSAGE_CANNOT_BE_EMPTY).into();
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live
                .driver
                .edit_chat_welcome_message(chat_id, welcome_id, &text)
            {
                Ok(_) => {
                    self.connection.status_note = tr(keys::Q_PEOPLE_WELCOME_MESSAGE_UPDATED).into()
                }
                Err(_) => {
                    self.connection.status_note =
                        tr(keys::Q_PEOPLE_COULD_NOT_EDIT_WELCOME_MESSAGE).into()
                }
            }
        } else {
            self.connection.status_note = tr(keys::Q_PEOPLE_WELCOME_MESSAGES_NEED_A_LIVE).into();
        }
        if let Some(dialog) = self.admin.welcome_dialog.as_mut() {
            dialog.editing = None;
        }
        cx.notify();
    }

    /// Slice G2: delete one welcome message (`deleteChatWelcomeMessage`).
    pub(super) fn delete_welcome_message(
        &mut self,
        chat_id: ChatId,
        welcome_id: i32,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.delete_chat_welcome_message(chat_id, welcome_id) {
                Ok(_) => {
                    self.connection.status_note = tr(keys::Q_PEOPLE_WELCOME_MESSAGE_DELETED).into()
                }
                Err(_) => {
                    self.connection.status_note =
                        tr(keys::Q_PEOPLE_COULD_NOT_DELETE_WELCOME_MESSAGE).into()
                }
            }
        } else {
            self.connection.status_note = tr(keys::Q_PEOPLE_WELCOME_MESSAGES_NEED_A_LIVE).into();
        }
        cx.notify();
    }

    /// kit Phase 2 (redo): welcome message editor hosted in a kit
    /// `Dialog` via `window.open_dialog`. Esc / backdrop / ✕ clear state
    /// via `on_close`.
    pub(super) fn build_welcome_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(app, shell, DialogKind::Welcome, |this, _, cx| {
            this.close_welcome_dialog(cx);
        });
        app.update(cx, |this, cx| {
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::MANAGE_PEER_WELCOME_MESSAGES,
                )));
            let Some(dialog_state) = this.admin.welcome_dialog.as_ref() else {
                return dialog.on_close(on_close);
            };
            let chat_id = dialog_state.chat_id;
            let editing = dialog_state.editing;
            let fetch = this.session().and_then(|session| {
                session
                    .groups
                    .welcome_message_fetches
                    .get(&chat_id.0)
                    .cloned()
            });
            let messages: Vec<ParsedWelcomeMessage> = this
                .session()
                .and_then(|session| session.groups.welcome_messages.get(&chat_id.0).cloned())
                .unwrap_or_default();
            let mut body = div().flex().flex_col().gap_2().child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div().flex_1().child(
                            Textarea::new(&dialog_state.new_input)
                                .aria_label(tr(keys::Q_PEOPLE_NEW_WELCOME_MESSAGE))
                                .h(px(64.)),
                        ),
                    )
                    .child(
                        Button::new("g2-welcome-add")
                            .label(tr(keys::SETTINGS_CHANNEL_ADD))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_welcome_add(window, cx);
                                this.close_kit_dialog_if_done(DialogKind::Welcome, window, cx);
                            })),
                    ),
            );
            match fetch {
                Some(WelcomeMessagesFetch::Failed(error)) => {
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
                                Button::new("g2-welcome-retry")
                                    .label(tr(keys::Q_PEOPLE_RETRY))
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_welcome_dialog(chat_id, window, cx);
                                        this.close_kit_dialog_if_done(
                                            DialogKind::Welcome,
                                            window,
                                            cx,
                                        );
                                    })),
                            ),
                    );
                }
                Some(WelcomeMessagesFetch::Loaded) | None => {
                    if messages.is_empty() {
                        body = body.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(tr(keys::Q_PEOPLE_NO_WELCOME_MESSAGES_YET)),
                        );
                    } else {
                        let mut list = div()
                            .id("g2-welcome-list")
                            .flex()
                            .flex_col()
                            .gap_2()
                            .max_h(px(300.))
                            .overflow_y_scroll();
                        for message in &messages {
                            list =
                                list.child(this.welcome_message_row(chat_id, message, editing, cx));
                        }
                        body = body.child(list);
                    }
                }
                Some(WelcomeMessagesFetch::Loading) => {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PEOPLE_LOADING_WELCOME_MESSAGES)),
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

    /// Slice G2: one welcome-message row — text preview, inline edit,
    /// and delete.
    pub(super) fn welcome_message_row(
        &self,
        chat_id: ChatId,
        message: &ParsedWelcomeMessage,
        editing: Option<i32>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let welcome_id = message.id;
        let text = Self::message_copyable_text(&message.content)
            .unwrap_or_else(|| tr(keys::Q_PEOPLE_NO_TEXT).to_string());
        let mut row = div()
            .flex()
            .flex_col()
            .w_full()
            .gap_1()
            .child(div().text_sm().child(text));
        if editing == Some(welcome_id) {
            let edit_input = self
                .admin
                .welcome_dialog
                .as_ref()
                .map(|dialog| dialog.edit_input.clone());
            let mut edit_row = div().flex().items_center().gap_1();
            if let Some(input) = edit_input {
                edit_row = edit_row.child(
                    div().flex_1().child(
                        Textarea::new(&input)
                            .aria_label(tr(keys::MANAGE_PEER_WELCOME_MESSAGES))
                            .h(px(56.)),
                    ),
                );
            }
            edit_row = edit_row
                .child(
                    Button::new(format!("g2-welcome-save-{welcome_id}"))
                        .label(tr(keys::SETTINGS_SAVE))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_welcome_edit(window, cx);
                        })),
                )
                .child(
                    Button::new(format!("g2-welcome-cancel-{welcome_id}"))
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(dialog) = this.admin.welcome_dialog.as_mut() {
                                dialog.editing = None;
                            }
                            cx.notify();
                        })),
                );
            row = row.child(edit_row);
        } else {
            row = row.child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        Button::new(format!("g2-welcome-edit-{welcome_id}"))
                            .label(tr(keys::GROUP_INVITE_CONTEXT_EDIT))
                            .ghost()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let text = this
                                    .session()
                                    .and_then(|session| {
                                        session.groups.welcome_messages.get(&chat_id.0).and_then(
                                            |messages| messages.iter().find(|m| m.id == welcome_id),
                                        )
                                    })
                                    .and_then(|message| {
                                        Self::message_copyable_text(&message.content)
                                    })
                                    .unwrap_or_default();
                                if let Some(dialog) = this.admin.welcome_dialog.as_mut() {
                                    dialog.editing = Some(welcome_id);
                                    dialog.edit_input.update(cx, |input, cx| {
                                        input.set_value(&text, window, cx);
                                    });
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(format!("g2-welcome-delete-{welcome_id}"))
                            .label(tr(keys::GROUP_INVITE_CONTEXT_DELETE))
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.delete_welcome_message(chat_id, welcome_id, cx);
                            })),
                    ),
            );
        }
        row.into_any_element()
    }
}

crate::ui::shell::register_dialogs! {
    Welcome => DialogSpec::new(
        5800,
        |app| app.admin.welcome_dialog.is_some(),
        QuillApp::build_welcome_dialog,
    ),
}
