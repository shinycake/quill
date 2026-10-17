//! Batch 8: the bars above a chat's history that tdesktop stacks under the
//! header — the contact-status action bar (`ChatActionBar`), the
//! "N join requests" bar with its requests box, and the voice-chat join bar.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn};
use quill::local_time::{civil_local, day_label, hhmm, now_unix};
use quill::state::JoinRequestFetch;
use quill::telegram::envelope::{ChatActionBar, ChatKind};
use std::cell::RefCell;
use std::rc::Rc;

mod join_requests;

/// "Block {name}" box state (tdesktop `PeerMenuBlockUserBox`): optional
/// report and chat deletion alongside `setMessageSenderBlockList`.
pub struct BlockBarDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) user_id: i64,
    pub(crate) report: bool,
    pub(crate) delete_chat: bool,
}

/// The join-requests box: the chat plus its request-search input (B8).
pub struct JoinRequestsDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) search: Entity<TextareaState>,
    pub(crate) _search_subscription: Subscription,
}

/// Cap of requester avatars on the requests bar (tdesktop shows three).
const REQUEST_AVATARS: usize = 3;

fn private_user_id(kind: &ChatKind) -> Option<i64> {
    match kind {
        ChatKind::Private { user_id } | ChatKind::Secret { user_id, .. } => Some(user_id.0),
        _ => None,
    }
}

/// "requested to join today at 14:05" (tdesktop `lng_group_requests_status_*`).
pub(crate) fn requested_status(date: i32) -> String {
    let at = civil_local(i64::from(date));
    let now = civil_local(now_unix());
    let time = hhmm(&at);
    match now.day_number() - at.day_number() {
        0 => tr_with(
            keys::GROUP_REQUESTS_STATUS_TODAY,
            &[("time", &time.to_string())],
        ),
        1 => tr_with(
            keys::GROUP_REQUESTS_STATUS_YESTERDAY,
            &[("time", &time.to_string())],
        ),
        _ => tr_with(
            keys::GROUP_REQUESTS_STATUS_DATE_TIME,
            &[
                ("date", &(day_label(&at, &now)).to_string()),
                ("time", &time.to_string()),
            ],
        ),
    }
}

impl QuillApp {
    /// All bars for the open chat, in tdesktop's order: contact status,
    /// join requests, voice chat.
    pub(super) fn chat_top_bars(&self, chat_id: ChatId, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut bars: Vec<AnyElement> = [
            self.chat_action_bar_view(chat_id, cx),
            self.business_bot_bar_view(chat_id, cx),
            self.join_requests_bar(chat_id, cx),
            self.voice_chat_bar(chat_id, cx),
        ]
        .into_iter()
        .flatten()
        .collect();
        // Batch 7: the translate bar (and its toast) sits below the others.
        bars.extend(self.translate_bar_views(chat_id, cx));
        // The Star reaction's "sent" toast with Undo.
        bars.extend(self.paid_toast_view(chat_id, cx));
        bars
    }

    pub(super) fn bar_shell(&self, id: &'static str, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .min_h(px(40.))
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(bg_canvas())
    }

    pub(super) fn bar_button(id: String, label: impl Into<SharedString>, danger: bool) -> Button {
        let label: SharedString = label.into();
        let button = Button::new(SharedString::from(id))
            .label(label.clone())
            .ghost()
            .small()
            .accessibility_label(label);
        if danger {
            button.danger()
        } else {
            button.text_color(accent())
        }
    }

    fn chat_action_bar_view(&self, chat_id: ChatId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let bar = session.chat_action_bar(chat_id)?.clone();
        let chat = session.chats.get(&chat_id.0)?;
        let user_id = private_user_id(&chat.kind);
        let name = user_id
            .map(|id| self.contact_display_name(id))
            .unwrap_or_default();
        let can_add_members = session.chat_can_add_members(chat_id);
        let mut buttons: Vec<Button> = Vec::new();
        let mut handlers: Vec<Box<dyn Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>)>> =
            Vec::new();
        let mut push =
            |button: Button,
             handler: Box<dyn Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>)>| {
                buttons.push(button);
                handlers.push(handler);
            };
        let unarchive = bar.can_unarchive();
        if unarchive {
            push(
                Self::bar_button(
                    "action-bar-unarchive".into(),
                    tr(keys::Q_CHAT_UNARCHIVE),
                    false,
                ),
                Box::new(move |this, _, cx| this.bar_unarchive(chat_id, cx)),
            );
        }
        match (&bar, user_id) {
            (ChatActionBar::ReportAddBlock { .. }, Some(uid)) => {
                if !unarchive {
                    push(
                        Self::bar_button(
                            "action-bar-add".into(),
                            tr(keys::Q_CHAT_ADD_CONTACT),
                            false,
                        ),
                        Box::new(move |this, window, cx| {
                            this.open_add_contact_dialog(uid, window, cx)
                        }),
                    );
                }
                push(
                    Self::bar_button("action-bar-block".into(), tr(keys::Q_CHAT_BLOCK_USER), true),
                    Box::new(move |this, _, cx| this.open_block_bar_dialog(chat_id, uid, cx)),
                );
            }
            (ChatActionBar::AddContact, Some(uid)) => {
                push(
                    Self::bar_button(
                        "action-bar-add".into(),
                        tr_with(keys::NEW_CONTACT_ADD_NAME, &[("user", &name.to_string())]),
                        false,
                    ),
                    Box::new(move |this, window, cx| this.open_add_contact_dialog(uid, window, cx)),
                );
            }
            (ChatActionBar::SharePhoneNumber, Some(uid)) => {
                push(
                    Self::bar_button(
                        "action-bar-share".into(),
                        tr(keys::NEW_CONTACT_SHARE),
                        false,
                    ),
                    Box::new(move |this, window, cx| {
                        this.bar_share_phone(chat_id, uid, window, cx)
                    }),
                );
            }
            (ChatActionBar::ReportSpam { .. }, _) => {
                let label = if unarchive {
                    tr(keys::Q_CHAT_REPORT_SPAM)
                } else {
                    tr(keys::REPORT_SPAM_AND_LEAVE)
                };
                push(
                    Self::bar_button("action-bar-report".into(), label, true),
                    Box::new(move |this, window, cx| this.bar_report_spam(chat_id, window, cx)),
                );
            }
            (ChatActionBar::InviteMembers, _) if can_add_members => {
                push(
                    Self::bar_button(
                        "action-bar-invite".into(),
                        tr(keys::RIGHTS_CHAT_ADD_MEMBERS),
                        false,
                    ),
                    Box::new(move |this, window, cx| this.open_member_dialog(chat_id, window, cx)),
                );
            }
            _ => {}
        }
        if let ChatActionBar::JoinRequest {
            title, is_channel, ..
        } = &bar
        {
            let what = if *is_channel { "channel" } else { "group" };
            let text = tr_with(
                keys::Q_CHAT_IS_AN_ADMIN_OF_A,
                &[
                    ("name", &name.to_string()),
                    ("title", &title.to_string()),
                    ("what", what),
                ],
            );
            let (title, date) = (title.clone(), bar_request_date(&bar));
            return Some(
                self.bar_shell("chat-action-bar", cx)
                    .cursor_pointer()
                    .role(gpui_kit::Role::Button)
                    .aria_label(text.clone())
                    .tab_index(0)
                    .on_click(cx.listener(move |_, _, window, cx| {
                        open_join_request_notice(window, cx, &title, date);
                    }))
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(text_primary())
                            .child(text),
                    )
                    .into_any_element(),
            );
        }
        if buttons.is_empty() {
            return None;
        }
        let dismissible = bar.is_dismissible();
        let handlers = std::rc::Rc::new(handlers);
        let row = buttons
            .into_iter()
            .enumerate()
            .map(|(index, button)| {
                let handlers = handlers.clone();
                button.on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(handler) = handlers.get(index) {
                        handler(this, window, cx);
                    }
                }))
            })
            .collect::<Vec<_>>();
        Some(
            self.bar_shell("chat-action-bar", cx)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .children(row),
                )
                .when(dismissible, |this| {
                    this.child(
                        Button::new("action-bar-close")
                            .icon(gpui_kit::assets::IconName::X)
                            .ghost()
                            .small()
                            .tooltip(tr(keys::Q_CHAT_HIDE))
                            .accessibility_label(tr(keys::Q_CHAT_HIDE))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.bar_dismiss(chat_id, cx);
                            })),
                    )
                })
                .into_any_element(),
        )
    }

    fn bar_dismiss(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.dismiss_chat_action_bar(chat_id).is_err() {
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_HIDE_THE_BAR)));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.set_chat_action_bar(chat_id.0, None);
        }
        cx.notify();
    }

    /// "Unarchive": back to the main list with default notifications
    /// (`addChatToList` + `setChatNotificationSettings`, schema line 3668).
    fn bar_unarchive(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let moved = live.driver.unarchive_chat(chat_id).is_ok();
            let reset = live
                .driver
                .reset_chat_notification_settings(chat_id)
                .is_ok();
            let _ = live.driver.dismiss_chat_action_bar(chat_id);
            if !(moved && reset) {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_CHAT_COULD_NOT_UNARCHIVE_THE_CHAT,
                )));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.set_chat_action_bar(chat_id.0, None);
        }
        cx.notify();
    }

    fn bar_share_phone(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = self.contact_display_name(user_id);
        let phone = self
            .session()
            .and_then(|s| s.my_user_id.and_then(|me| s.user(me)))
            .map(|u| u.phone_number.clone())
            .filter(|p| !p.is_empty())
            .map(|p| format!("+{}", p.trim_start_matches('+')))
            .unwrap_or_else(|| tr(keys::Q_CHAT_YOUR_NUMBER).to_string());
        let app = cx.entity().downgrade();
        let text = tr_with(
            keys::NEW_CONTACT_SHARE_SURE,
            &[("phone", &phone.to_string()), ("user", &name.to_string())],
        );
        window.open_alert_dialog(cx, move |alert, _, _| {
            let app = app.clone();
            alert
                .title(tr(keys::NEW_CONTACT_SHARE))
                .description(text.clone())
                .ok_text("OK")
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let _ = app.update(cx, |this, cx| {
                        if let Some(live) = this.live.as_mut() {
                            if live.driver.share_phone_number(chat_id, user_id).is_err() {
                                this.connection.set_note(StatusNote::error(tr(
                                    keys::Q_CHAT_COULD_NOT_SHARE_YOUR_PHONE,
                                )));
                            }
                        } else if let Some(session) = this.demo_session.as_mut() {
                            session.set_chat_action_bar(chat_id.0, None);
                        }
                        cx.notify();
                    });
                    true
                })
        });
    }

    /// "Report spam (and leave)": confirm, `reportChat`, then `leaveChat`.
    fn bar_report_spam(&mut self, chat_id: ChatId, window: &mut Window, cx: &mut Context<Self>) {
        let channel = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .is_some_and(|c| c.kind.is_channel());
        let text = if channel {
            tr(keys::REPORT_SPAM_SURE_CHANNEL)
        } else {
            tr(keys::REPORT_SPAM_SURE_GROUP)
        };
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let app = app.clone();
            alert
                .title(tr(keys::Q_CHAT_REPORT_SPAM))
                .description(text)
                .ok_text(tr(keys::Q_CHAT_REPORT))
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let _ = app.update(cx, |this, cx| this.submit_bar_report(chat_id, cx));
                    true
                })
        });
    }

    fn submit_bar_report(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let reported = live.driver.report_chat(chat_id);
            let left = live.driver.leave_channel(chat_id);
            self.connection
                .set_note(if reported.is_ok() && left.is_ok() {
                    StatusNote::silent(tr(keys::REPORT_SPAM_DONE))
                } else {
                    StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_REPORT_THE_CHAT))
                });
        } else if let Some(session) = self.demo_session.as_mut() {
            session.set_chat_action_bar(chat_id.0, None);
        }
        cx.notify();
    }

    fn open_block_bar_dialog(&mut self, chat_id: ChatId, user_id: i64, cx: &mut Context<Self>) {
        // tdesktop pre-checks both boxes for a stranger's bar.
        self.dialogs.block_bar_dialog = Some(BlockBarDialog {
            chat_id,
            user_id,
            report: true,
            delete_chat: true,
        });
        cx.notify();
    }

    pub(super) fn close_block_bar_dialog(&mut self, cx: &mut Context<Self>) {
        self.dialogs.block_bar_dialog = None;
        cx.notify();
    }

    fn submit_block_bar_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialogs.block_bar_dialog.take() else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            let mut ok = live.driver.block_sender(dialog.user_id).is_ok();
            if dialog.report {
                ok &= live.driver.report_chat(dialog.chat_id).is_ok();
            }
            if dialog.delete_chat {
                ok &= live.driver.remove_chat_from_list(dialog.chat_id).is_ok();
            }
            self.connection.set_note(if ok {
                StatusNote::silent(tr_with(
                    keys::Q_CHAT_IS_NOW_BLOCKED,
                    &[(
                        "user_id",
                        &(self.contact_display_name(dialog.user_id)).to_string(),
                    )],
                ))
            } else {
                StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_BLOCK_THE_USER))
            });
        } else if let Some(session) = self.demo_session.as_mut() {
            session.set_chat_action_bar(dialog.chat_id.0, None);
        }
        cx.notify();
    }

    /// kit dialog: "Block {name}" with Report spam / Delete this chat.
    pub(super) fn build_block_bar_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::BlockBar, |this, _, cx| {
                this.close_block_bar_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true);
            let Some(state) = this.dialogs.block_bar_dialog.as_ref() else {
                return dialog
                    .title(crate::ui::shell::dialog_title(tr(keys::Q_CHAT_BLOCK_USER)))
                    .on_close(on_close);
            };
            let name = this.contact_display_name(state.user_id);
            let (report, delete_chat) = (state.report, state.delete_chat);
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().child(tr_with(
                    keys::BLOCKED_LIST_CONFIRM_TEXT,
                    &[("name", &name.to_string())],
                )))
                .child(
                    Checkbox::new("block-bar-report")
                        .label(tr(keys::Q_CHAT_REPORT_SPAM))
                        .checked(report)
                        .on_click(cx.listener(|this, &on, _, cx| {
                            if let Some(d) = this.dialogs.block_bar_dialog.as_mut() {
                                d.report = on;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Checkbox::new("block-bar-delete")
                        .label(tr(keys::Q_CHAT_DELETE_THIS_CHAT))
                        .checked(delete_chat)
                        .on_click(cx.listener(|this, &on, _, cx| {
                            if let Some(d) = this.dialogs.block_bar_dialog.as_mut() {
                                d.delete_chat = on;
                            }
                            cx.notify();
                        })),
                )
                .into_any_element();
            let footer = div()
                .flex()
                .gap_2()
                .child(
                    Button::new("block-bar-submit")
                        .label(tr(keys::Q_CHAT_BLOCK))
                        .danger()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_block_bar_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::BlockBar, window, cx);
                        })),
                )
                .child(
                    Button::new("block-bar-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_block_bar_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::BlockBar, window, cx);
                        })),
                );
            dialog
                .title(crate::ui::shell::dialog_title(tr_with(
                    keys::Q_CHAT_BLOCK_2,
                    &[("name", &name.to_string())],
                )))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    // ----- voice chat ----------------------------------------------------

    /// tdesktop's group-call bar for a live voice chat the viewer has not
    /// joined: title, who is in it, and Join.
    fn voice_chat_bar(&self, chat_id: ChatId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let chat = session.chats.get(&chat_id.0)?;
        let video_chat = chat.video_chat.as_ref()?;
        if session
            .calls
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.is_joined)
        {
            return None;
        }
        let title = if chat.kind.is_channel() {
            tr(keys::VIEW_BUTTON_VOICE_CHAT_CHANNEL)
        } else {
            tr(keys::Q_CHAT_VOICE_CHAT)
        };
        let sub = if video_chat.has_participants {
            tr(keys::Q_CHAT_ACTIVE_NOW)
        } else {
            tr(keys::Q_CHAT_CLICK_TO_JOIN)
        };
        Some(
            self.bar_shell("voice-chat-bar", cx)
                .child(
                    Icon::new(gpui_kit::assets::IconName::Mic)
                        .with_size(px(18.))
                        .text_color(accent()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_sm()
                                .font_medium()
                                .text_color(text_primary())
                                .child(title),
                        )
                        .child(div().text_xs().text_color(text_muted()).child(sub)),
                )
                .child(
                    Button::new("voice-chat-bar-join")
                        .label(tr(keys::Q_CHAT_JOIN))
                        .small()
                        .accessibility_label(tr(keys::Q_CHAT_JOIN_VOICE_CHAT))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.start_or_join_video_chat(chat_id, window, cx);
                        })),
                )
                .into_any_element(),
        )
    }
}

fn bar_request_date(bar: &ChatActionBar) -> i32 {
    match bar {
        ChatActionBar::JoinRequest { request_date, .. } => *request_date,
        _ => 0,
    }
}

/// tdesktop's "Response to your join request" explanation box.
fn open_join_request_notice(window: &mut Window, cx: &mut App, title: &str, date: i32) {
    let at = civil_local(i64::from(date));
    let now = civil_local(now_unix());
    let text = tr_with(
        keys::FROM_REQUEST_BODY,
        &[
            ("name", title),
            ("date", &(day_label(&at, &now)).to_string()),
        ],
    );
    window.open_alert_dialog(cx, move |alert, _, _| {
        alert
            .title(tr(keys::FROM_REQUEST_TITLE_GROUP))
            .description(text.clone())
            .ok_text(tr(keys::Q_CHAT_I_UNDERSTAND))
            .show_cancel(false)
    });
}

// ----- screenshot fixtures ---------------------------------------------

crate::ui::shell::register_dialogs! {
    /// Batch 8: chat action bar's "Block {name}" box.
    BlockBar => DialogSpec::new(
        6300,
        |app| app.dialogs.block_bar_dialog.is_some(),
        QuillApp::build_block_bar_dialog,
    ),

    /// Batch 8: the chat's pending join requests.
    JoinRequests => DialogSpec::new(
        6400,
        |app| app.admin.join_requests_dialog.is_some(),
        QuillApp::build_join_requests_dialog,
    ),
}
