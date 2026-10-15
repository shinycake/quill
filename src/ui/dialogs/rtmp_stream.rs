//! "Stream with other apps": the box a channel or group manager opens from
//! the chat's call button to start a live stream from OBS or similar
//! (tdesktop `StartRtmpProcess` and `StartWithBox`,
//! calls/group/calls_group_rtmp.cpp).
//!
//! The box shows once the server answered with the Server URL and Stream
//! Key. Both rows are the live stream window's (`rtmp_info_rows`, also used
//! by its Settings box). The top-right "..." offers Revoke Stream Key behind
//! a confirmation, the key stays hidden until a click and a warning, and
//! "Start Streaming" creates the call as an RTMP stream.
//!
//! The call button's menu for a manager of a chat with no video chat
//! (`HasGroupCallMenu`, `FillVideoChatMenu`) lives here too.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::shell::{DialogKind, QuillShell};
use super::group_call_create::server_error_toast;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme, Selectable, Sizable, WindowExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::group_call_create::{CreateKind, rtmp_join_as};
use quill::calls::rtmp_start::{
    RtmpInfo, RtmpStart, StartEntry, StartFacts, call_button_visible, start_entries,
};
use quill::ids::{ChatId, RequestId};
use quill::lang::{keys, tr};
use quill::telegram::envelope::ChatKind;
use std::cell::RefCell;
use std::rc::Rc;

/// What the box is showing besides the rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RtmpConfirm {
    /// `lng_group_call_rtmp_revoke_sure`.
    Revoke,
    /// `lng_group_call_rtmp_key_warning`, before the key is shown.
    KeyWarning,
}

/// The box's own state: the key was revealed, a confirmation is up.
#[derive(Default)]
pub(crate) struct RtmpStartUi {
    pub(crate) key_shown: bool,
    pub(crate) confirm: Option<RtmpConfirm>,
}

/// One pair of Server URL / Stream Key rows (tdesktop `FillRtmpRows`),
/// shared by the stream window's Settings and this box.
pub(crate) struct RtmpRows {
    /// Element ids are `{prefix}-copy-url`, `-copy-key`, `-key`, `-rtmp-menu`.
    pub(crate) prefix: &'static str,
    pub(crate) url: String,
    pub(crate) key: String,
    pub(crate) key_shown: bool,
    /// The "..." menu's item: "Revoke" in the start box
    /// (`lng_group_invite_context_revoke`), "Revoke Stream Key" in the
    /// stream window's Settings (`lng_group_call_rtmp_revoke`).
    pub(crate) revoke_label: &'static str,
}

type RowAction = fn(&mut QuillApp, &mut Context<QuillApp>);

/// The rows, with Copy buttons, the masked key and the "..." Revoke menu.
/// `reveal` runs on a click on the hidden key, `revoke` on the menu's
/// Revoke item.
pub(crate) fn rtmp_info_rows(
    rows: RtmpRows,
    cx: &mut Context<QuillApp>,
    reveal: RowAction,
    revoke: RowAction,
) -> AnyElement {
    let RtmpRows {
        prefix,
        url,
        key,
        key_shown: shown,
        revoke_label,
    } = rows;
    let copy = |id: String, label: &'static str, value: String, toast: &'static str| {
        Button::new(id)
            .label(label)
            .outline()
            .w_full()
            .on_click(cx.listener(move |_, _, window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(value.clone()));
                window.push_notification(Notification::new().message(toast), cx);
            }))
    };
    let subtitle = |text: &'static str| {
        div()
            .pt(px(8.))
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().primary)
            .child(text)
    };
    let masked = "\u{2022}".repeat(key.chars().count().clamp(8, 32));
    let owner = cx.entity().downgrade();
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .border_t_1()
        .border_b_1()
        .border_color(cx.theme().border)
        .py(px(6.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(subtitle(tr(keys::GROUP_CALL_RTMP_URL_SUBTITLE)))
                .child(
                    Button::new(format!("{prefix}-rtmp-menu"))
                        .icon(IconName::EllipsisVertical)
                        .ghost()
                        .small()
                        .accessibility_label(revoke_label)
                        .dropdown_menu(move |menu, _, _| {
                            let owner = owner.clone();
                            menu.item(PopupMenuItem::new(revoke_label).on_click(move |_, _, cx| {
                                let _ = owner.update(cx, revoke);
                            }))
                        }),
                ),
        )
        .child(
            div()
                .id(SharedString::from(format!("{prefix}-url")))
                .debug_selector(move || format!("{prefix}-url"))
                .text_sm()
                .child(url.clone()),
        )
        .child(copy(
            format!("{prefix}-copy-url"),
            tr(keys::GROUP_CALL_RTMP_URL_COPY),
            url,
            tr(keys::GROUP_CALL_RTMP_URL_COPIED),
        ))
        .child(subtitle(tr(keys::GROUP_CALL_RTMP_KEY_SUBTITLE)))
        .child(
            div()
                .id(SharedString::from(format!("{prefix}-key")))
                .debug_selector(move || format!("{prefix}-key"))
                .text_sm()
                .cursor_pointer()
                .child(if shown { key.clone() } else { masked })
                .when(!shown, |this| {
                    this.on_click(cx.listener(move |this, _, _, cx| reveal(this, cx)))
                }),
        )
        .child(copy(
            format!("{prefix}-copy-key"),
            tr(keys::GROUP_CALL_RTMP_KEY_COPY),
            key,
            tr(keys::GROUP_CALL_RTMP_KEY_COPIED),
        ))
        .into_any_element()
}

/// `**text**` from a tdesktop string, drawn bold.
fn plain(text: &str) -> SharedString {
    text.trim_start_matches("**")
        .trim_end_matches("**")
        .to_string()
        .into()
}

impl QuillApp {
    /// The chat's call button for a chat that may have no video chat yet:
    /// a menu for a manager (`TopBarWidget::groupCall`), otherwise the
    /// plain start-or-join button.
    pub(in crate::ui) fn chat_voice_chat_button(
        &self,
        chat_id: ChatId,
        voice_live: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let facts = self.rtmp_start_facts(chat_id);
        let entries = start_entries(facts);
        let label = if voice_live {
            tr(keys::Q_CHAT_JOIN_VOICE_CHAT)
        } else {
            tr(keys::Q_CHAT_START_VOICE_CHAT)
        };
        let button = Button::new("chat-voice-chat")
            .icon(IconName::AudioLines)
            .ghost()
            .when(voice_live, |button| button.selected(true))
            .tooltip(label)
            .accessibility_label(label);
        if entries.is_empty() {
            return button
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.start_or_join_video_chat(chat_id, window, cx);
                }))
                .into_any_element();
        }
        let livestream = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .is_some_and(|chat| chat.kind.is_channel());
        let owner = cx.entity().downgrade();
        button
            .on_click(|event, window, cx| {
                if matches!(event, ClickEvent::Keyboard(_)) {
                    window.dispatch_action(
                        Box::new(gpui_kit::base::actions::Confirm { secondary: false }),
                        cx,
                    );
                }
            })
            .dropdown_menu(move |mut menu, _, _| {
                for entry in &entries {
                    let entry = *entry;
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(entry_label(entry, livestream)).on_click(
                        move |_, window, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                this.run_start_entry(entry, chat_id, window, cx)
                            });
                        },
                    ));
                }
                menu
            })
            .into_any_element()
    }

    /// What `FillVideoChatMenu` reads for a chat.
    pub(in crate::ui) fn rtmp_start_facts(&self, chat_id: ChatId) -> StartFacts {
        let Some(session) = self.session() else {
            return StartFacts {
                is_group_chat: false,
                manager: false,
                has_call: false,
            };
        };
        let chat = session.chats.get(&chat_id.0);
        StartFacts {
            is_group_chat: chat.is_some_and(|chat| {
                matches!(
                    chat.kind,
                    ChatKind::BasicGroup { .. } | ChatKind::Supergroup { .. }
                )
            }),
            manager: session.chat_can_manage_video_chats(chat_id),
            has_call: chat.is_some_and(|chat| chat.video_chat.is_some()),
        }
    }

    /// Whether the chat header shows the call button
    /// (`TopBarWidget::updateControlsVisibility`).
    pub(in crate::ui) fn chat_call_button_visible(&self, chat_id: ChatId) -> bool {
        let has_participants = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .and_then(|chat| chat.video_chat.as_ref())
            .is_some_and(|video_chat| video_chat.has_participants);
        call_button_visible(self.rtmp_start_facts(chat_id), has_participants)
    }

    fn run_start_entry(
        &mut self,
        entry: StartEntry,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match entry {
            StartEntry::Start => self.begin_group_call_create(chat_id, CreateKind::Now, window, cx),
            StartEntry::Schedule => {
                self.begin_group_call_create(chat_id, CreateKind::Schedule, window, cx)
            }
            StartEntry::StreamWith => self.open_rtmp_start(chat_id, window, cx),
        }
    }

    /// "Stream with...": ask for the RTMP URL; the box opens with the
    /// answer.
    pub(in crate::ui) fn open_rtmp_start(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.group_call.rtmp_start = RtmpStartUi::default();
        match self.live.as_mut() {
            Some(live) => {
                if live.driver.begin_rtmp_start(chat_id).is_err() {
                    server_error_toast(window, cx);
                }
            }
            // Screenshot demos and tests have no TDLib: the process opens
            // and the fixture supplies the answer.
            None => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.calls.rtmp_start = Some(RtmpStart::new(chat_id));
                }
            }
        }
        cx.notify();
    }

    /// The box closed (Cancel, Esc, backdrop): end the process.
    pub(in crate::ui) fn close_rtmp_start(&mut self, cx: &mut Context<Self>) {
        self.group_call.rtmp_start = RtmpStartUi::default();
        match self.live.as_mut() {
            Some(live) => live.driver.close_rtmp_start(),
            None => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.calls.rtmp_start = None;
                }
            }
        }
        cx.notify();
    }

    /// "Start Streaming": confirm leaving the call you are in first
    /// (`showStartWithRtmp` → `confirmLeaveCurrent`), then create the
    /// stream.
    pub(in crate::ui) fn confirm_rtmp_start(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chat_id = self
            .session()
            .and_then(|s| s.calls.rtmp_start.as_ref())
            .map(|start| start.chat_id);
        if let Some(chat_id) = chat_id {
            self.begin_group_call_create(chat_id, CreateKind::Rtmp, window, cx);
        }
    }

    /// Create the call as an RTMP stream and end the process; the window
    /// joins it once it is tracked, and only when the request went out,
    /// as the chat itself when you created it (`StartRtmpProcess::
    /// createBox`: `amCreator() ? peer : self`).
    pub(in crate::ui) fn start_rtmp_stream_now(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chat_id = self
            .session()
            .and_then(|s| s.calls.rtmp_start.as_ref())
            .map(|start| start.chat_id);
        let Some(chat_id) = chat_id else {
            return;
        };
        let join_as = rtmp_join_as(
            chat_id.0,
            self.session().is_some_and(|s| s.chat_am_creator(chat_id)),
        );
        self.end_current_call_for_create(cx);
        self.group_call.rtmp_start = RtmpStartUi::default();
        let sent = match self.live.as_mut() {
            Some(live) => live.driver.start_rtmp_stream().ok(),
            // Screenshot demos and tests have no TDLib: the process ends.
            None => self.demo_session.as_mut().map(|session| {
                session.calls.rtmp_start = None;
                RequestId(0)
            }),
        };
        self.created_video_chat(chat_id, sent, true, join_as, window, cx);
    }

    /// The revoke confirmation was accepted: ask for a new URL and key.
    fn revoke_rtmp_start_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.group_call.rtmp_start.confirm = None;
        self.group_call.rtmp_start.key_shown = false;
        if let Some(live) = self.live.as_mut()
            && live.driver.revoke_rtmp_start_key().is_err()
        {
            server_error_toast(window, cx);
        }
        cx.notify();
    }

    /// Per frame: the server error of a failed request is a toast
    /// (`Lang::Hard::ServerError`); a failed first request opens no box.
    pub(in crate::ui) fn rtmp_start_frame_upkeep(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session = match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_mut(),
        };
        let Some(session) = session else {
            return;
        };
        let Some(start) = session.calls.rtmp_start.as_mut() else {
            return;
        };
        if !start.failed {
            return;
        }
        start.failed = false;
        if start.info.is_none() {
            session.calls.rtmp_start = None;
        }
        window.push_notification(
            Notification::new().message(tr(keys::Q_OPS_SERVER_ERROR)),
            cx,
        );
    }

    /// The registered dialog: rows, info text, Start Streaming and Cancel,
    /// or a confirmation in their place.
    pub(crate) fn build_rtmp_start_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::RtmpStart, |this, _, cx| {
                this.close_rtmp_start(cx);
            });
        app.update(cx, |this, cx| {
            let info = this
                .session()
                .and_then(|s| s.calls.rtmp_start.as_ref())
                .and_then(|start| start.info.clone());
            let Some(RtmpInfo { url, key }) = info else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title(tr(
                        keys::GROUP_CALL_RTMP_TITLE,
                    )))
                    .on_close(on_close.clone());
            };
            let confirm = this.group_call.rtmp_start.confirm;
            let body = match confirm {
                Some(which) => confirm_view(which, cx),
                None => div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(rtmp_info_rows(
                        RtmpRows {
                            prefix: "rtmp-start",
                            url,
                            key,
                            key_shown: this.group_call.rtmp_start.key_shown,
                            revoke_label: tr(keys::GROUP_INVITE_CONTEXT_REVOKE),
                        },
                        cx,
                        |this, cx| {
                            this.group_call.rtmp_start.confirm = Some(RtmpConfirm::KeyWarning);
                            cx.notify();
                        },
                        |this, cx| {
                            this.group_call.rtmp_start.confirm = Some(RtmpConfirm::Revoke);
                            cx.notify();
                        },
                    ))
                    .child(
                        div()
                            .id("rtmp-start-info")
                            .text_sm()
                            .whitespace_normal()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::GROUP_CALL_RTMP_INFO)),
                    )
                    .into_any_element(),
            };
            let footer = (confirm.is_none()).then(|| {
                div()
                    .flex()
                    .w_full()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("rtmp-start-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_rtmp_start(cx);
                                this.close_kit_dialog_if_done(DialogKind::RtmpStart, window, cx);
                            })),
                    )
                    .child(
                        div().debug_selector(|| "rtmp-start-go".into()).child(
                            Button::new("rtmp-start-go")
                                .label(tr(keys::GROUP_CALL_RTMP_START))
                                .primary()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.confirm_rtmp_start(window, cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::RtmpStart,
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                    )
            });
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::GROUP_CALL_RTMP_TITLE,
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
                .when_some(footer, |dialog, footer| dialog.footer(footer))
                .on_close(on_close)
        })
    }
}

/// tdesktop's `MakeConfirmBox` in the box's place: the text, Cancel and the
/// confirm button.
fn confirm_view(which: RtmpConfirm, cx: &mut Context<QuillApp>) -> AnyElement {
    let (text, confirm_label) = match which {
        RtmpConfirm::Revoke => (
            SharedString::from(tr(keys::GROUP_CALL_RTMP_REVOKE_SURE)),
            tr(keys::GROUP_INVITE_CONTEXT_REVOKE),
        ),
        RtmpConfirm::KeyWarning => (
            plain(tr(keys::GROUP_CALL_RTMP_KEY_WARNING)),
            tr(keys::FROM_REQUEST_UNDERSTAND),
        ),
    };
    div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(
            div()
                .id("rtmp-start-confirm-text")
                .debug_selector(|| "rtmp-start-confirm-text".into())
                .text_sm()
                // Only the key warning is bold (its `**` markup);
                // `lng_group_call_rtmp_revoke_sure` is plain.
                .when(which == RtmpConfirm::KeyWarning, |this| {
                    this.font_weight(FontWeight::SEMIBOLD)
                })
                .child(text),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.))
                .child(
                    Button::new("rtmp-start-confirm-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.group_call.rtmp_start.confirm = None;
                            cx.notify();
                        })),
                )
                .child(
                    div().debug_selector(|| "rtmp-start-confirm".into()).child(
                        Button::new("rtmp-start-confirm")
                            .label(confirm_label)
                            // tdesktop `attentionBoxButton` for the key
                            // warning, the default style for Revoke.
                            .when(which == RtmpConfirm::KeyWarning, |button| button.danger())
                            .when(which == RtmpConfirm::Revoke, |button| button.primary())
                            .on_click(cx.listener(move |this, _, window, cx| match which {
                                RtmpConfirm::Revoke => this.revoke_rtmp_start_key(window, cx),
                                RtmpConfirm::KeyWarning => {
                                    this.group_call.rtmp_start.confirm = None;
                                    this.group_call.rtmp_start.key_shown = true;
                                    cx.notify();
                                }
                            })),
                    ),
                ),
        )
        .into_any_element()
}

/// `lng_menu_start_group_call*`: channels say "live stream".
fn entry_label(entry: StartEntry, livestream: bool) -> &'static str {
    match (entry, livestream) {
        (StartEntry::Start, false) => tr(keys::MENU_START_GROUP_CALL),
        (StartEntry::Start, true) => tr(keys::MENU_START_GROUP_CALL_CHANNEL),
        (StartEntry::Schedule, false) => tr(keys::MENU_START_GROUP_CALL_SCHEDULED),
        (StartEntry::Schedule, true) => tr(keys::MENU_START_GROUP_CALL_SCHEDULED_CHANNEL),
        (StartEntry::StreamWith, false) => tr(keys::MENU_START_GROUP_CALL_WITH),
        (StartEntry::StreamWith, true) => tr(keys::MENU_START_GROUP_CALL_WITH_CHANNEL),
    }
}

crate::ui::shell::register_dialogs! {
    RtmpStart => DialogSpec::new(
        410,
        |app| {
            app.session()
                .and_then(|s| s.calls.rtmp_start.as_ref())
                .is_some_and(|start| start.box_open())
        },
        QuillApp::build_rtmp_start_dialog,
    ),
}

#[cfg(feature = "demo-capture")]
#[path = "rtmp_stream_demo.rs"]
mod demo;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "rtmp_stream_tests.rs"]
mod ui_tests;
