//! The Quick Replies dialog: the list, the name field and one shortcut's
//! messages.

use super::{QuickRepliesDialog, View};
use crate::ui::app::QuillApp;
use crate::ui::pressable::action_row;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, Textarea};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::quick_replies::{self as qr, NAME_MAX};
use std::cell::RefCell;
use std::rc::Rc;

const ABOUT: &str = "Set up shortcuts with rich text and media to respond to messages faster. \
You can access Quick Replies in any chat with a person by typing /.";

impl QuillApp {
    fn quick_replies_list_body(&mut self, premium: bool, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let rows: Vec<(String, String, i32)> = self
            .session()
            .map(|s| {
                s.settings
                    .quick_replies
                    .list()
                    .into_iter()
                    .filter(|sc| sc.message_count > 0)
                    .map(|sc| (sc.name.clone(), qr::count_label(sc.message_count), sc.id))
                    .collect()
            })
            .unwrap_or_default();
        let limit = self.session().map_or(qr::SHORTCUTS_MAX, |s| {
            s.settings.quick_replies.shortcut_limit()
        });
        let full = rows.len() >= limit;
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().text_color(muted).child(ABOUT));
        if !premium {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_OPS_QUICK_REPLIES_ARE_A_TELEGRAM)),
            );
        }
        body = body.child(
            action_row(
                "quick-replies-add",
                Some(IconName::Plus),
                tr(keys::REPLIES_ADD),
                false,
                cx,
            )
            .when(!premium || full, |row| row.opacity(0.5))
            .on_click(cx.listener(move |this, _, window, cx| {
                if !premium {
                    this.open_premium_to_buy(quill::premium_hub::BusinessFeature::QuickReplies, cx);
                } else if full {
                    this.quick_replies_error_line(tr(keys::Q_OPS_YOU_HAVE_REACHED_THE_LIMIT), cx);
                } else {
                    this.quick_replies_go(View::Name { rename_of: None }, window, cx);
                }
            })),
        );
        if rows.is_empty() {
            return body;
        }
        let mut list = div().flex().flex_col();
        for (index, (name, count, id)) in rows.into_iter().enumerate() {
            let open_name = name.clone();
            list = list.child(
                div()
                    .id(("quick-reply-shortcut", index as u64))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .w_full()
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .text_sm()
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!("/{name}, {count}"))
                    .tab_index(0)
                    .hover(|s| s.bg(cx.theme().secondary))
                    .active(|s| s.bg(cx.theme().secondary_active))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.quick_replies_go(
                            View::Shortcut {
                                name: open_name.clone(),
                                id: Some(id),
                            },
                            window,
                            cx,
                        );
                    }))
                    .child(div().truncate().child(format!("/{name}")))
                    .child(div().flex_none().text_color(muted).child(count)),
            );
        }
        body.child(list)
    }

    fn quick_replies_error_line(&mut self, line: &str, cx: &mut Context<Self>) {
        if let Some(dialog) = self.dialogs.quick_replies_dialog.as_mut() {
            dialog.local_error = Some(line.to_string());
        }
        cx.notify();
    }

    fn quick_replies_name_body(
        &mut self,
        rename_of: Option<i32>,
        state: &QuickRepliesDialog,
        cx: &mut Context<Self>,
    ) -> Div {
        let muted = cx.theme().muted_foreground;
        let len = state.name_input.read(cx).value().trim().chars().count();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().child(if rename_of.is_some() {
                tr(keys::REPLIES_EDIT_ABOUT)
            } else {
                tr(keys::REPLIES_ADD_SHORTCUT)
            }))
            .child(Input::new(&state.name_input).aria_label(tr(keys::REPLIES_ADD_PLACEHOLDER)))
            .child(
                div()
                    .text_xs()
                    .text_color(if len > NAME_MAX {
                        cx.theme().danger
                    } else {
                        muted
                    })
                    .child(format!("{len}/{NAME_MAX}")),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("quick-replies-name-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let back = match rename_of {
                                    Some(_) => this.quick_replies_shortcut_view(),
                                    None => View::List,
                                };
                                this.quick_replies_go(back, window, cx);
                            })),
                    )
                    .child(
                        Button::new("quick-replies-name-save")
                            .label(tr(keys::SETTINGS_SAVE))
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_quick_reply_name(window, cx);
                            })),
                    ),
            )
    }

    /// The shortcut screen a rename started from (the list when unknown).
    fn quick_replies_shortcut_view(&self) -> View {
        let Some(View::Name {
            rename_of: Some(id),
        }) = self
            .dialogs
            .quick_replies_dialog
            .as_ref()
            .map(|d| d.view.clone())
        else {
            return View::List;
        };
        match self
            .session()
            .and_then(|s| s.settings.quick_replies.shortcuts.get(&id))
        {
            Some(shortcut) => View::Shortcut {
                name: shortcut.name.clone(),
                id: Some(id),
            },
            None => View::List,
        }
    }

    fn quick_replies_shortcut_body(
        &mut self,
        name: String,
        id: Option<i32>,
        premium: bool,
        state: &QuickRepliesDialog,
        cx: &mut Context<Self>,
    ) -> Div {
        let muted = cx.theme().muted_foreground;
        let messages = id
            .and_then(|id| {
                self.session()
                    .map(|s| s.settings.quick_replies.messages_of(id).to_vec())
            })
            .unwrap_or_default();
        let editing = state.editing;
        let message_limit = self.session().map_or(qr::MESSAGES_MAX, |s| {
            s.settings.quick_replies.message_limit()
        });
        let full = messages.len() >= message_limit && editing.is_none();
        let mut body = div().flex().flex_col().gap_2();
        if messages.is_empty() {
            body = body.child(div().text_sm().text_color(muted).child(tr_with(
                keys::Q_OPS_ENTER_A_MESSAGE_BELOW_THAT,
                &[("name", &name.to_string())],
            )));
        }
        for (index, message) in messages.iter().enumerate() {
            let message_id = message.id;
            let text = message.text.clone();
            let can_edit = premium && message.is_editable_text();
            let mut row = div()
                .id(("quick-reply-message", index as u64))
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1p5()
                .rounded_md()
                .bg(if editing == Some(message_id) {
                    cx.theme().selection
                } else {
                    cx.theme().secondary
                })
                .child(div().flex_1().min_w_0().text_sm().child(message.summary()));
            if can_edit {
                row = row.child(
                    Button::new(("quick-reply-edit", index as u64))
                        .icon(IconName::Pencil)
                        .ghost()
                        .small()
                        .tooltip(tr(keys::Q_OPS_EDIT_MESSAGE))
                        .accessibility_label(tr(keys::Q_OPS_EDIT_MESSAGE))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.begin_quick_reply_edit(message_id, text.clone(), window, cx);
                        })),
                );
            }
            if let (true, Some(shortcut_id)) = (premium, id) {
                row = row.child(
                    Button::new(("quick-reply-delete", index as u64))
                        .icon(IconName::Trash)
                        .ghost()
                        .small()
                        .tooltip(tr(keys::Q_OPS_DELETE_MESSAGE))
                        .accessibility_label(tr(keys::Q_OPS_DELETE_MESSAGE))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.delete_quick_reply_message(shortcut_id, message_id, cx);
                        })),
                );
            }
            body = body.child(row);
        }
        let field_note = if full {
            tr_with(
                keys::Q_OPS_A_SHORTCUT_CAN_HAVE_MESSAGES,
                &[("message_limit", &message_limit.to_string())],
            )
        } else if editing.is_some() {
            tr(keys::Q_OPS_EDITING_A_MESSAGE).to_string()
        } else {
            String::new()
        };
        body = body
            .child(Textarea::new(&state.text_input).aria_label(tr(keys::Q_OPS_QUICK_REPLY_MESSAGE)))
            .when(!field_note.is_empty(), |this| {
                this.child(div().text_xs().text_color(muted).child(field_note))
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .when(editing.is_some(), |this| {
                        this.child(
                            Button::new("quick-replies-edit-cancel")
                                .label(tr(keys::Q_OPS_CANCEL_EDIT))
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.cancel_quick_reply_edit(window, cx);
                                })),
                        )
                    })
                    .child(
                        Button::new("quick-replies-message-save")
                            .label(if editing.is_some() {
                                tr(keys::SETTINGS_SAVE)
                            } else {
                                tr(keys::Q_OPS_ADD)
                            })
                            .primary()
                            .disabled(!premium || full)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_quick_reply_text(window, cx);
                            })),
                    ),
            );
        let confirm = state.confirm_delete;
        body.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("quick-replies-rename")
                        .icon(IconName::Pencil)
                        .label(tr(keys::REPLIES_EDIT_TITLE))
                        .ghost()
                        .small()
                        .disabled(!premium || id.is_none())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(id) = id {
                                this.quick_replies_go(
                                    View::Name {
                                        rename_of: Some(id),
                                    },
                                    window,
                                    cx,
                                );
                            }
                        })),
                )
                .child(
                    Button::new("quick-replies-delete-shortcut")
                        .icon(IconName::Trash)
                        .label(if confirm {
                            tr(keys::Q_OPS_DELETE_WITH_ALL_MESSAGES)
                        } else {
                            tr(keys::Q_OPS_DELETE_SHORTCUT)
                        })
                        .ghost()
                        .small()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.delete_quick_reply_shortcut(window, cx);
                        })),
                ),
        )
    }

    pub(in crate::ui) fn build_quick_replies_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::QuickReplies, |this, _, cx| {
                this.close_quick_replies(cx);
            });
        app.update(cx, |this, cx| {
            let Some(state) = this.dialogs.quick_replies_dialog.clone() else {
                return dialog.on_close(on_close);
            };
            let premium = this.session().is_some_and(|s| s.my_is_premium());
            let (title, mut body) = match state.view.clone() {
                View::List => (
                    tr(keys::BUSINESS_SUBTITLE_QUICK_REPLIES).to_string(),
                    this.quick_replies_list_body(premium, cx),
                ),
                View::Name { rename_of } => (
                    if rename_of.is_some() {
                        tr(keys::REPLIES_EDIT_TITLE).to_string()
                    } else {
                        tr(keys::REPLIES_ADD_TITLE).to_string()
                    },
                    this.quick_replies_name_body(rename_of, &state, cx),
                ),
                View::Shortcut { name, id } => {
                    // The server's name wins once it is known.
                    let shown = id
                        .and_then(|id| {
                            this.session()
                                .and_then(|s| s.settings.quick_replies.shortcuts.get(&id))
                                .map(|s| s.name.clone())
                        })
                        .unwrap_or_else(|| name.clone());
                    (
                        format!("/{shown}"),
                        this.quick_replies_shortcut_body(shown, id, premium, &state, cx),
                    )
                }
            };
            let server_error = this
                .session()
                .and_then(|s| s.settings.quick_replies.error.clone());
            let danger = cx.theme().danger;
            let error = state.local_error.clone().or(server_error);
            if let Some(line) = error {
                body = body.child(div().text_xs().text_color(danger).child(line));
            }
            let in_list = state.view == View::List;
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .when(!in_list, |this| {
                    this.child(
                        Button::new("quick-replies-back")
                            .label(tr(keys::Q_OPS_BACK))
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.quick_replies_go(View::List, window, cx);
                            })),
                    )
                })
                .child(
                    Button::new("quick-replies-close")
                        .label(tr(keys::CLOSE))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_quick_replies(cx);
                            this.close_kit_dialog_if_done(DialogKind::QuickReplies, window, cx);
                        })),
                );
            let content = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(body)
                .child(footer)
                .into_any_element();
            let content = Rc::new(RefCell::new(Some(content)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |slot, _, _| {
                        let body = content
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        slot.child(body)
                    },
                ))
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    QuickReplies => DialogSpec::new(
        5437,
        |app| app.dialogs.quick_replies_dialog.is_some(),
        QuillApp::build_quick_replies_dialog,
    ),
}
