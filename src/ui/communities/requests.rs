//! The pending requests list (tdesktop
//! `info/community_requests/info_community_requests_widget.cpp`): chats
//! community members suggested, each with Add and Decline, and Add All /
//! Decline All at the bottom.
//!
//! Add and Decline are undoable for three seconds (`kUndoToastDuration`):
//! the row waits, a bar names what was done with an Undo button, and the
//! request goes when the time is up, when another row is answered, or
//! when the list closes.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use quill::lang::trn;
use std::time::Duration;

/// `kUndoToastDuration`.
pub(crate) const UNDO_DURATION: Duration = Duration::from_millis(3000);
const UNDO_COUNTDOWN_FPS: u32 = 4;

/// An answered row waiting out its undo time.
pub(crate) struct UndoAction {
    pub(crate) chat_id: i64,
    pub(crate) approve: bool,
    pub(crate) started: Instant,
    /// Wakes the app when the time is up; dropping it cancels the wake.
    pub(crate) _timer: Option<Task<()>>,
}

impl UndoAction {
    /// Whole seconds left, as the countdown shows them (rounded up).
    pub(crate) fn seconds_left(&self, now: Instant) -> u64 {
        let left = UNDO_DURATION.saturating_sub(now.saturating_duration_since(self.started));
        left.as_millis().div_ceil(1000) as u64
    }
}

impl QuillApp {
    /// Open the pending requests list and ask for its first page.
    pub(super) fn open_community_requests(&mut self, community_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live
                .driver
                .fetch_community_add_chat_requests(community_id, true)
        {
            self.connection
                .set_note(StatusNote::error(user_error(&err)));
        }
        self.show_community_box(
            CommunityBox::Requests {
                community_id,
                undo: None,
            },
            cx,
        );
    }

    /// Add or Decline on a row: the previous undoable action goes now,
    /// this one waits.
    pub(super) fn answer_community_request(
        &mut self,
        chat_id: i64,
        approve: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(CommunityBox::Requests { community_id, undo }) =
            self.admin.community.flow.open.as_mut()
        else {
            return;
        };
        let community_id = *community_id;
        let previous = undo.take();
        let started = Instant::now();
        let timer = self.spawn_screen(cx, async move |this, cx| {
            cx.background_executor().timer(UNDO_DURATION).await;
            let _ = this.update(cx, |app, cx| app.finish_community_undo(started, cx));
        });
        if let Some(CommunityBox::Requests { undo, .. }) = self.admin.community.flow.open.as_mut() {
            *undo = Some(UndoAction {
                chat_id,
                approve,
                started,
                _timer: Some(timer),
            });
        }
        if let Some(previous) = previous {
            self.perform_request_action(community_id, previous, cx);
        }
        cx.notify();
    }

    /// The undo time of the answer given at `started` is up: send it,
    /// unless it was undone or replaced since.
    fn finish_community_undo(&mut self, started: Instant, cx: &mut Context<Self>) {
        let Some(CommunityBox::Requests { community_id, undo }) =
            self.admin.community.flow.open.as_mut()
        else {
            return;
        };
        if !undo
            .as_ref()
            .is_some_and(|action| action.started == started)
        {
            return;
        }
        let community_id = *community_id;
        if let Some(action) = undo.take() {
            self.perform_request_action(community_id, action, cx);
        }
        cx.notify();
    }

    pub(super) fn undo_community_request(&mut self, cx: &mut Context<Self>) {
        if let Some(CommunityBox::Requests { undo, .. }) = self.admin.community.flow.open.as_mut() {
            *undo = None;
        }
        cx.notify();
    }

    pub(super) fn perform_request_action(
        &mut self,
        community_id: i64,
        action: UndoAction,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.process_community_add_chat_request(
                community_id,
                action.chat_id,
                action.approve,
            )
        {
            self.connection
                .set_note(StatusNote::error(user_error(&err)));
        }
        cx.notify();
    }

    /// Add All / Decline All: confirm with the count first.
    pub(super) fn ask_process_all_requests(&mut self, approve: bool, cx: &mut Context<Self>) {
        let Some(CommunityBox::Requests { community_id, .. }) = self.admin.community.flow.open
        else {
            return;
        };
        let count = self
            .session()
            .and_then(|s| s.groups.community_full_infos.get(&community_id))
            .map_or(0, |info| info.add_chat_request_count)
            .max(1);
        self.flush_and_close_community_box(cx);
        self.show_community_box(
            CommunityBox::ConfirmProcessAll {
                community_id,
                approve,
                count,
            },
            cx,
        );
    }

    pub(super) fn confirm_process_all_requests(&mut self, cx: &mut Context<Self>) {
        let Some(CommunityBox::ConfirmProcessAll {
            community_id,
            approve,
            count,
        }) = self.admin.community.flow.open
        else {
            return;
        };
        if let Some(live) = self.live.as_mut()
            && let Err(err) =
                live.driver
                    .process_community_add_chat_requests(community_id, approve, count)
        {
            self.connection
                .set_note(StatusNote::error(user_error(&err)));
        }
        cx.notify();
    }

    /// Cancel on the Add All / Decline All confirmation: back to the list.
    pub(super) fn cancel_process_all_requests(&mut self, cx: &mut Context<Self>) {
        if let Some(CommunityBox::ConfirmProcessAll { community_id, .. }) =
            self.admin.community.flow.open
        {
            self.open_community_requests(community_id, cx);
        }
    }

    fn community_request_row(
        &self,
        community_id: i64,
        request: &quill::telegram::envelope::CommunityAddChatRequest,
        waiting: bool,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chat_id = request.chat_id;
        let session = self.session();
        let title = self.community_chat_title(chat_id);
        let kind = self.community_chat_kind(chat_id);
        let suggester = session
            .and_then(|s| s.user(request.sender_user_id))
            .map(|user| user.first_name.clone())
            .unwrap_or_default();
        let suggested = tr_with(
            match kind {
                CommunityChatKind::Bot => keys::COMMUNITY_REQUEST_SUGGESTED_BOT,
                CommunityChatKind::Channel => keys::COMMUNITY_REQUEST_SUGGESTED_CHANNEL,
                CommunityChatKind::Group => keys::COMMUNITY_REQUEST_SUGGESTED_GROUP,
            },
            &[("user", &suggester)],
        );
        let members = session
            .and_then(|s| s.chats.get(&chat_id).and_then(|c| s.group_member_counts(c)))
            .map(|(members, _)| members)
            .filter(|count| *count > 0)
            .map(|count| member_status(kind, count));
        let muted = cx.theme().muted_foreground;
        let hidden = request.is_hidden;
        let sender = request.sender_user_id;
        div()
            .id(("community-request", chat_id as u64))
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .py_1p5()
            .rounded_md()
            .when(waiting, |row| row.opacity(0.5))
            .when(!waiting, |row| {
                row.cursor_pointer()
                    .hover(|row| row.bg(cx.theme().list_hover))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if hidden {
                            this.show_community_box(
                                CommunityBox::HiddenChat {
                                    community_id,
                                    chat_id,
                                    owner_user_id: sender,
                                },
                                cx,
                            );
                        } else {
                            this.flush_and_close_community_box(cx);
                            this.select_listed_chat(quill::ids::ChatId(chat_id), window, cx);
                        }
                    }))
            })
            .child(self.community_chat_avatar(chat_id, &title, 42.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .min_w_0()
                            .child(div().font_semibold().truncate().child(title))
                            .when(hidden, |line| {
                                line.child(
                                    div()
                                        .id(("community-request-hidden", chat_id as u64))
                                        .flex_none()
                                        .px_1p5()
                                        .rounded_full()
                                        .bg(cx.theme().muted)
                                        .text_xs()
                                        .text_color(muted)
                                        .child(tr(keys::COMMUNITY_HIDDEN_STATUS))
                                        .tooltip(|window, cx| {
                                            Tooltip::new(tr(keys::COMMUNITY_REQUEST_ONLY_MEMBERS))
                                                .build(window, cx)
                                        }),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .truncate()
                            .child(suggested),
                    )
                    .when_some(members, |col, members| {
                        col.child(div().text_xs().text_color(muted).child(members))
                    }),
            )
            .when(!waiting, |row| {
                row.child(
                    div()
                        .flex()
                        .flex_none()
                        .gap_1()
                        .child(
                            Button::new(("community-request-decline", chat_id as u64))
                                .label(tr(keys::COMMUNITY_REQUEST_DECLINE))
                                .ghost()
                                .small()
                                .disabled(busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.answer_community_request(chat_id, false, cx);
                                })),
                        )
                        .child(
                            Button::new(("community-request-add", chat_id as u64))
                                .label(tr(keys::COMMUNITY_REQUEST_ADD))
                                .primary()
                                .small()
                                .disabled(busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.answer_community_request(chat_id, true, cx);
                                })),
                        ),
                )
            })
            .into_any_element()
    }

    fn community_undo_bar(&self, undo: &UndoAction, cx: &mut Context<Self>) -> AnyElement {
        let chat = self.community_chat_title(undo.chat_id);
        let text = tr_with(
            if undo.approve {
                keys::COMMUNITY_REQUEST_ADDED_TOAST_UNDO
            } else {
                keys::COMMUNITY_REQUEST_DECLINED_TOAST_UNDO
            },
            &[("chat", &chat)],
        );
        let left = undo.seconds_left(Instant::now());
        self.request_animation_tick(UNDO_COUNTDOWN_FPS, cx);
        div()
            .id("community-request-undo")
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(cx.theme().secondary)
            .role(Role::Status)
            .child(div().flex_1().min_w_0().text_sm().child(text))
            .child(
                Button::new("community-request-undo-button")
                    .label(format!("{} {left}", tr(keys::PAID_REACT_UNDO)))
                    .ghost()
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.undo_community_request(cx))),
            )
            .into_any_element()
    }

    fn build_community_requests_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CommunityRequests, |this, _, cx| {
                this.flush_and_close_community_box(cx)
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true).w(px(420.));
            let Some(CommunityBox::Requests { community_id, undo }) =
                this.admin.community.flow.open.as_ref()
            else {
                return dialog.on_close(on_close);
            };
            let community_id = *community_id;
            let waiting = undo.as_ref().map(|u| u.chat_id);
            let undo_bar = undo.as_ref().map(|u| this.community_undo_bar(u, cx));
            let session = this.session();
            let fetch = session.map(|s| s.communities.add_chat_requests(community_id));
            let busy = session.is_some_and(|s| s.community_requests_processing(community_id));
            let requests: Vec<_> = fetch
                .and_then(|f| f.data())
                .map(|list| list.requests.clone())
                .unwrap_or_default();
            let loading = fetch.is_some_and(|f| f.is_loading());
            let more = fetch
                .and_then(|f| f.data())
                .is_some_and(|list| list.next_offset.is_some());
            let error = fetch.and_then(|f| f.error()).map(str::to_owned);
            let count = session
                .and_then(|s| s.groups.community_full_infos.get(&community_id))
                .map_or(requests.len() as i32, |info| info.add_chat_request_count)
                .max(1);
            let muted = cx.theme().muted_foreground;
            let about_text = tr_with(keys::COMMUNITY_REQUESTS_ABOUT, &[("link", "")]);
            let mut body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_start()
                        .text_xs()
                        .text_color(muted)
                        .child(div().w_full().child(about_text.trim_end().to_string()))
                        .child(
                            Button::new("community-requests-settings")
                                .label(tr(keys::COMMUNITY_REQUESTS_ABOUT_LINK))
                                .link()
                                .xsmall()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.flush_and_close_community_box(cx);
                                    this.open_info_panel_target(
                                        quill::state::InfoPanelTarget::Community(community_id),
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(cx.theme().accent_foreground)
                        .child(trn(keys::COMMUNITY_REQUESTS_COUNT, count)),
                );
            let mut list = div()
                .id("community-requests-list")
                .flex()
                .flex_col()
                .max_h(px(360.))
                .overflow_y_scroll();
            for request in &requests {
                list = list.child(this.community_request_row(
                    community_id,
                    request,
                    waiting == Some(request.chat_id),
                    busy,
                    cx,
                ));
            }
            if loading && requests.is_empty() {
                list = list.child(
                    div()
                        .py_4()
                        .flex()
                        .justify_center()
                        .child(gpui_kit::component::spinner::Spinner::new()),
                );
            }
            if more && !loading {
                list = list.child(
                    Button::new("community-requests-more")
                        .label(tr(keys::Q_OPS_LOAD_MORE))
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(live) = this.live.as_mut()
                                && let Err(err) = live
                                    .driver
                                    .fetch_community_add_chat_requests(community_id, false)
                            {
                                this.connection
                                    .set_note(StatusNote::error(user_error(&err)));
                            }
                            cx.notify();
                        })),
                );
            }
            if let Some(error) = error {
                list = list.child(div().text_sm().text_color(cx.theme().danger).child(error));
            }
            body = body.child(list).children(undo_bar);
            let has_requests = !requests.is_empty();
            let footer = div()
                .flex()
                .justify_between()
                .gap_2()
                .w_full()
                .child(
                    Button::new("community-requests-decline-all")
                        .label(tr(keys::COMMUNITY_REQUESTS_DECLINE_ALL))
                        .outline()
                        .disabled(busy || !has_requests)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.ask_process_all_requests(false, cx);
                        })),
                )
                .child(
                    Button::new("community-requests-add-all")
                        .label(tr(keys::COMMUNITY_REQUESTS_ADD_ALL))
                        .primary()
                        .disabled(busy || !has_requests)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.ask_process_all_requests(true, cx);
                        })),
                );
            let body = body.into_any_element();
            dialog
                .title(crate::ui::shell::dialog_title(tr(
                    keys::COMMUNITY_REQUESTS_TITLE,
                )))
                .content(crate::ui::shell::scrollable_dialog_content(
                    super::boxes::once(body),
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    CommunityRequests => DialogSpec::new(
        5413,
        |app| matches!(app.admin.community.flow.open, Some(CommunityBox::Requests { .. })),
        QuillApp::build_community_requests_dialog,
    )
    .close(|app, _, cx| app.flush_and_close_community_box(cx)),
}
