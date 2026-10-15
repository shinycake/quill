//! Join requests: the bar above the chat and the requests dialog.
use super::*;

impl QuillApp {
    pub(super) fn join_requests_bar(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let count = session
            .groups
            .pending_join_request_counts
            .get(&chat_id.0)
            .copied()
            .unwrap_or(0);
        if count <= 0 || !session.chat_can_invite_users(chat_id) {
            return None;
        }
        let users = session
            .groups
            .pending_join_request_users
            .get(&chat_id.0)
            .cloned()
            .unwrap_or_default();
        let text = trn(keys::GROUP_REQUESTS_PENDING, count);
        let avatars = users
            .iter()
            .take(REQUEST_AVATARS)
            .enumerate()
            .map(|(i, uid)| {
                let name = self.contact_display_name(*uid);
                let photo = session.user_photo_path(*uid).map(std::path::PathBuf::from);
                div()
                    .when(i > 0, |d| d.ml(px(-8.)))
                    .rounded_full()
                    .border_2()
                    .border_color(bg_canvas())
                    .child(chat_avatar(&name, photo.as_deref(), 24.))
            });
        Some(
            self.bar_shell("join-requests-bar", cx)
                .cursor_pointer()
                .role(gpui_kit::Role::Button)
                .aria_label(text.clone())
                .tab_index(0)
                .hover(|s| s.bg(cx.theme().accent.opacity(0.08)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_join_requests_dialog(chat_id, window, cx);
                }))
                .child(div().flex().items_center().children(avatars))
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .font_medium()
                        .text_color(accent())
                        .child(text),
                )
                .child(
                    Icon::new(gpui_kit::assets::IconName::ChevronRight)
                        .with_size(px(16.))
                        .text_color(text_muted()),
                )
                .into_any_element(),
        )
    }

    pub(in crate::ui) fn open_join_requests_dialog(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_CHAT_SEARCH))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        // tdesktop's requests box searches server-side as you type.
        let subscription = cx.subscribe_in(
            &search,
            window,
            move |this, state, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = state.read(cx).value().trim().to_string();
                    this.search_join_requests(chat_id, &query, cx);
                }
            },
        );
        self.admin.join_requests_dialog = Some(JoinRequestsDialog {
            chat_id,
            search,
            _search_subscription: subscription,
        });
        if let Some(live) = self.live.as_mut() {
            live.driver
                .session
                .groups
                .join_request_queries
                .remove(&chat_id.0);
            let _ = live.driver.refresh_chat_join_requests(chat_id);
        }
        cx.notify();
    }

    /// B8: re-run `getChatJoinRequests` with a search query (skipped when
    /// the query is unchanged).
    pub(super) fn search_join_requests(
        &mut self,
        chat_id: ChatId,
        query: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let current = live
            .driver
            .session
            .groups
            .join_request_queries
            .get(&chat_id.0)
            .map(String::as_str)
            .unwrap_or("");
        if current == query {
            return;
        }
        let _ = live.driver.search_chat_join_requests(chat_id, query);
        cx.notify();
    }

    /// B8: confirm, then approve (`true`) or dismiss (`false`) every
    /// pending request via `processChatJoinRequests`.
    pub(super) fn confirm_process_all_join_requests(
        &mut self,
        chat_id: ChatId,
        approve: bool,
        count: i32,
        is_channel: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let place = if is_channel { "channel" } else { "group" };
        let (title, text, ok) = if approve {
            (
                tr(keys::Q_CHAT_ADD_ALL),
                tr_with(
                    keys::Q_CHAT_DO_YOU_WANT_TO_ADD,
                    &[("count", &count.to_string()), ("place", place)],
                ),
                tr(keys::Q_CHAT_ADD_ALL),
            )
        } else {
            (
                tr(keys::Q_CHAT_DISMISS_ALL),
                tr_with(
                    keys::Q_CHAT_DO_YOU_WANT_TO_DISMISS,
                    &[("count", &count.to_string())],
                ),
                tr(keys::Q_CHAT_DISMISS_ALL),
            )
        };
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let app = app.clone();
            alert
                .title(title)
                .description(text.clone())
                .ok_text(ok)
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let _ = app.update(cx, |this, cx| {
                        this.process_all_join_requests(chat_id, approve, cx);
                    });
                    true
                })
        });
    }

    /// B8: `processChatJoinRequests` for all pending requests.
    pub(in crate::ui) fn process_all_join_requests(
        &mut self,
        chat_id: ChatId,
        approve: bool,
        cx: &mut Context<Self>,
    ) {
        self.connection.status_note = match self.live.as_mut() {
            Some(live) => match live.driver.process_all_chat_join_requests(chat_id, approve) {
                Ok(_) => tr(keys::Q_CHAT_PROCESSING_JOIN_REQUESTS).into(),
                Err(_) => tr(keys::Q_CHAT_COULD_NOT_PROCESS_JOIN_REQUESTS).into(),
            },
            None => tr(keys::Q_CHAT_JOIN_REQUESTS_NEED_A_LIVE).into(),
        };
        cx.notify();
    }

    /// B8: next page of the join-request list.
    pub(super) fn load_more_join_requests(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.load_more_chat_join_requests(chat_id);
        }
        cx.notify();
    }

    pub(in crate::ui) fn close_join_requests_dialog(&mut self, cx: &mut Context<Self>) {
        self.admin.join_requests_dialog = None;
        cx.notify();
    }

    /// kit dialog: pending requests with "Add to Group/Channel" / "Dismiss".
    pub(in crate::ui) fn build_join_requests_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::JoinRequests, |this, _, cx| {
                this.close_join_requests_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true);
            let Some((chat_id, search)) = this
                .admin
                .join_requests_dialog
                .as_ref()
                .map(|d| (d.chat_id, d.search.clone()))
            else {
                return dialog
                    .title(crate::ui::shell::dialog_title(tr(
                        keys::Q_CHAT_JOIN_REQUESTS_2,
                    )))
                    .on_close(on_close);
            };
            let is_channel = this
                .session()
                .and_then(|s| s.chats.get(&chat_id.0))
                .is_some_and(|c| c.kind.is_channel());
            let add_label = if is_channel {
                tr(keys::Q_CHAT_ADD_TO_CHANNEL)
            } else {
                tr(keys::Q_CHAT_ADD_TO_GROUP)
            };
            let fetch = this
                .session()
                .and_then(|s| s.groups.join_requests.get(&chat_id.0))
                .cloned();
            let mut title = tr(keys::Q_CHAT_JOIN_REQUESTS_2).to_string();
            let mut body = div().flex().flex_col().gap_2();
            let searching = this
                .session()
                .and_then(|s| s.groups.join_request_queries.get(&chat_id.0))
                .is_some_and(|q| !q.is_empty());
            body = body.child(
                Textarea::new(&search)
                    .aria_label(tr(keys::Q_CHAT_SEARCH_JOIN_REQUESTS))
                    .h(px(40.)),
            );
            if let Some(JoinRequestFetch::Loaded(list)) = &fetch
                && !searching
                && list.total_count > 1
            {
                let count = list.total_count;
                body = body.child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("join-requests-add-all")
                                .label(tr(keys::Q_CHAT_ADD_ALL))
                                .small()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.confirm_process_all_join_requests(
                                        chat_id, true, count, is_channel, window, cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new("join-requests-dismiss-all")
                                .label(tr(keys::Q_CHAT_DISMISS_ALL))
                                .ghost()
                                .small()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.confirm_process_all_join_requests(
                                        chat_id, false, count, is_channel, window, cx,
                                    );
                                })),
                        ),
                );
            }
            match fetch {
                None | Some(JoinRequestFetch::Loading) => {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_CHAT_LOADING_JOIN_REQUESTS)),
                    );
                }
                Some(JoinRequestFetch::Failed(message)) => {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(message),
                    );
                }
                Some(JoinRequestFetch::Loaded(list)) => {
                    if list.requests.is_empty() {
                        body = body.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(if searching {
                                    tr(keys::Q_CHAT_NO_MATCHING_JOIN_REQUESTS)
                                } else {
                                    tr(keys::GROUP_REQUESTS_NONE)
                                }),
                        );
                    } else {
                        title = trn(keys::GROUP_REQUESTS_PENDING, list.total_count.max(1));
                    }
                    let has_more = (list.requests.len() as i32) < list.total_count;
                    for request in list.requests {
                        let uid = request.user_id;
                        let name = this.contact_display_name(uid);
                        let photo = this
                            .session()
                            .and_then(|s| s.user_photo_path(uid))
                            .map(std::path::PathBuf::from);
                        body = body.child(
                            div()
                                .id(("join-request-row", uid as u64))
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(chat_avatar(&name, photo.as_deref(), 40.))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .child(div().text_sm().font_medium().truncate().child(name))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .truncate()
                                                .child(requested_status(request.date)),
                                        ),
                                )
                                .child(
                                    Button::new(format!("join-request-add-{uid}"))
                                        .label(add_label)
                                        .small()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.process_join_request(chat_id, uid, true, cx);
                                        })),
                                )
                                .child(
                                    Button::new(format!("join-request-dismiss-{uid}"))
                                        .label(tr(keys::Q_CHAT_DISMISS))
                                        .ghost()
                                        .small()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.process_join_request(chat_id, uid, false, cx);
                                        })),
                                ),
                        );
                    }
                    if has_more {
                        body = body.child(
                            Button::new("join-requests-more")
                                .label(tr(keys::Q_CHAT_SHOW_MORE))
                                .ghost()
                                .small()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.load_more_join_requests(chat_id, cx);
                                })),
                        );
                    }
                }
            }
            let body = body.into_any_element();
            dialog
                .width(px(540.))
                .title(crate::ui::shell::dialog_title(title))
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
                .on_close(on_close)
        })
    }
}
