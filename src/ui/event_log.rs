//! admin log (recent actions) panel.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::group_members::scrolled_near_end;
use crate::ui::dialogs::group_admin_settings::GroupSettingsView;
use crate::ui::lean_buttons::ghost_text_button;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::state::{AdminListFetch, ChatEventLogFetch, event_log_relative_time};
use quill::telegram::envelope::{
    ChannelMemberStatus, ChatEventAction, MessageSender, ParsedChatEvent,
};
use quill::telegram::requests::ChatEventLogFilterSet;
use std::time::Duration;

/// How often an open recent-actions page asks for new events (tdesktop's
/// `kNewEventsCheckInterval`).
const EVENT_LOG_POLL_INTERVAL: Duration = Duration::from_secs(5);
/// The height of the scrolling list of events.
const EVENT_LOG_LIST_HEIGHT: f32 = 420.;

impl QuillApp {
    /// Slice G2: apply the event-log search box value (the driver
    /// stores it per chat; an empty box clears it).
    pub(super) fn apply_event_log_search(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let query = self
            .admin
            .event_log_search
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_string())
            .unwrap_or_default();
        if let Some(live) = self.live.as_mut() {
            live.driver.set_chat_event_log_query(chat_id, &query);
            if live.driver.refresh_chat_event_log(chat_id).is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_OPS_COULD_NOT_SEARCH_RECENT_ACTIONS,
                )));
            }
        }
        cx.notify();
    }

    /// Slice G2: flip one event-log filter category and refetch.
    pub(super) fn toggle_event_log_filter(
        &mut self,
        chat_id: ChatId,
        toggle: impl FnOnce(&mut ChatEventLogFilterSet) + 'static,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            live.driver.toggle_chat_event_log_filter(chat_id, toggle);
            if live.driver.refresh_chat_event_log(chat_id).is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_OPS_COULD_NOT_FILTER_RECENT_ACTIONS,
                )));
            }
        }
        cx.notify();
    }

    pub(super) fn refresh_event_log(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.refresh_chat_event_log(chat_id) {
                Ok(_) => {}
                Err(_) => {
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_OPS_COULD_NOT_REFRESH_RECENT_ACTIONS,
                    )));
                }
            }
        } else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_OPS_RECENT_ACTIONS_NEED_A_LIVE)));
        }
        cx.notify();
    }

    /// The list scrolled: close to its end, the next older page is asked
    /// for (the driver does nothing while one is already on its way or the
    /// log is exhausted).
    fn event_log_scrolled(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if scrolled_near_end(&self.admin.event_log_scroll) {
            self.load_more_event_log(chat_id, cx);
        }
    }

    /// Keep an open recent-actions page live: every few seconds the newest
    /// events are fetched and join the top of the list. A reader who has
    /// scrolled down is left alone (the rows would jump under them); the
    /// poll catches up once the list is back at the top.
    pub(super) fn start_event_log_poll(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if self.live.is_none() || self.admin.event_log_poll == Some(chat_id) {
            return;
        }
        self.admin.event_log_poll = Some(chat_id);
        self.spawn_scoped(cx, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(EVENT_LOG_POLL_INTERVAL)
                    .await;
                let keep = this
                    .update(cx, |this, cx| this.event_log_poll_tick(chat_id, cx))
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                if this.admin.event_log_poll == Some(chat_id) {
                    this.admin.event_log_poll = None;
                }
            });
        })
        .detach();
    }

    /// One poll step; `false` once the page is closed.
    fn event_log_poll_tick(&mut self, chat_id: ChatId, cx: &mut Context<Self>) -> bool {
        let open = self
            .admin
            .group_settings_dialog
            .as_ref()
            .is_some_and(|dialog| {
                dialog.chat_id == chat_id && dialog.view == GroupSettingsView::RecentActions
            });
        if !open {
            return false;
        }
        let at_top = self.admin.event_log_scroll.offset().y > px(-4.);
        if at_top
            && self.frame.window_active.get()
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.poll_chat_event_log(chat_id);
            cx.notify();
        }
        true
    }

    /// Phase D3c: fetch the next older page of the event log.
    pub(super) fn load_more_event_log(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.fetch_chat_event_log_more(chat_id) {
                Ok(_) => {}
                Err(_) => {
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_OPS_COULD_NOT_LOAD_MORE_ACTIONS,
                    )));
                }
            }
        } else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_OPS_RECENT_ACTIONS_NEED_A_LIVE)));
        }
        cx.notify();
    }

    /// Phase D3c: "Recent actions" admin-log section for the channel/group
    /// info panel. Shown only to administrators and the creator
    /// (`Session::chat_can_view_event_log` — `getChatEventLog` "requires
    /// administrator rights", schema 1.8.67 line 15252). Honest states:
    /// loading / failed-with-retry / loaded rows with actor name, action
    /// description, and relative timestamp, plus "Load more" while older
    /// pages exist. Unhandled event types render as generic rows — never
    /// faked details.
    pub(super) fn event_log_section(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self
            .session()
            .is_some_and(|session| session.chat_can_view_event_log(chat_id))
        {
            return None;
        }
        let fetch = self
            .session()
            .and_then(|session| session.groups.event_logs.get(&chat_id.0))
            .cloned();
        let mut section = div()
            .id("event-log-section")
            .flex()
            .flex_col()
            .w_full()
            .gap_1()
            .child(
                div().flex().items_center().w_full().gap_1().child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_OPS_RECENT_ACTIONS)),
                ),
            );
        section = section.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(quill::admin_extras::event_log_about(
                    self.session()
                        .and_then(|session| session.chats.get(&chat_id.0))
                        .is_some_and(|chat| chat.is_channel()),
                )),
        );
        // Slice G2: compact search + filter controls. The search box
        // feeds `getChatEventLog.query` (schema 1.8.67, line 15252);
        // the chips flip `chatEventLogFilters` categories (line 7956).
        // An empty selection means "all types" (the driver passes
        // `null`), so "Clear" just removes every active chip.
        let mut controls = div().flex().items_center().w_full().gap_1();
        if let Some(input) = self.admin.event_log_search.clone() {
            controls = controls.child(
                div().flex_1().child(
                    Textarea::new(&input)
                        .aria_label(tr(keys::Q_OPS_FILTER_EVENT_LOG))
                        .h(px(32.)),
                ),
            );
        }
        controls = controls.child(ghost_text_button(
            "event-log-search",
            tr(keys::Q_OPS_SEARCH),
            cx.listener(move |this, _, _, cx| {
                this.apply_event_log_search(chat_id, cx);
            }),
        ));
        section = section.child(controls);
        let active_filters = self
            .session()
            .and_then(|session| session.groups.event_log_filters.get(&chat_id.0).copied())
            .unwrap_or_default();
        section = section.child(self.event_log_filter_chips(chat_id, active_filters, cx));
        // The admin filter is server side (`getChatEventLog.user_ids`), so
        // the chips come from the administrator list, not the loaded page.
        let selected_admins = self
            .session()
            .and_then(|session| session.groups.event_log_users.get(&chat_id.0).cloned())
            .unwrap_or_default();
        let filtered_by_admin = !selected_admins.is_empty();
        let admins = self.event_log_admin_ids(chat_id, fetch.as_ref());
        if admins.len() > 1 {
            section =
                section.child(self.event_log_admin_chips(chat_id, &admins, &selected_admins, cx));
        }
        match fetch {
            None | Some(ChatEventLogFetch::Loading) => {
                section = section.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_OPS_LOADING_RECENT_ACTIONS)),
                );
            }
            Some(ChatEventLogFetch::Failed(message)) => {
                section = section.child(
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
                                .child(message),
                        )
                        .child(ghost_text_button(
                            "event-log-retry",
                            tr(keys::Q_OPS_RETRY),
                            cx.listener(move |this, _, _, cx| {
                                this.refresh_event_log(chat_id, cx);
                            }),
                        )),
                );
            }
            Some(ChatEventLogFetch::Loaded(page)) => {
                if page.events.is_empty() {
                    section = section.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(if filtered_by_admin {
                                tr(keys::Q_OPS_NO_RECENT_ACTIONS_BY_THE)
                            } else {
                                tr(keys::Q_OPS_NO_RECENT_ACTIONS)
                            }),
                    );
                } else {
                    let mut list = div()
                        .id("event-log-list")
                        .flex()
                        .flex_col()
                        .w_full()
                        .gap_1()
                        .max_h(px(EVENT_LOG_LIST_HEIGHT))
                        .overflow_y_scroll()
                        .track_scroll(&self.admin.event_log_scroll)
                        .on_scroll_wheel(
                            cx.listener(move |this, _, _, cx| this.event_log_scrolled(chat_id, cx)),
                        );
                    for event in page.events.iter() {
                        list = list.child(self.event_log_row(event, cx));
                    }
                    if page.has_more {
                        list = list.child(
                            div()
                                .id("event-log-loading-older")
                                .flex()
                                .items_center()
                                .justify_center()
                                .w_full()
                                .gap_2()
                                .py_2()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(Spinner::new().small())
                                .child(tr(keys::Q_OPS_LOADING)),
                        );
                    }
                    section = section.child(list);
                }
            }
        }
        Some(section.into_any_element())
    }

    /// Administrators to offer as filter chips: the loaded administrator
    /// list, or the actors seen in the loaded page while that is missing.
    pub(super) fn event_log_admin_ids(
        &self,
        chat_id: ChatId,
        fetch: Option<&ChatEventLogFetch>,
    ) -> Vec<i64> {
        let from_list =
            self.session()
                .and_then(|session| match session.groups.admin_lists.get(&chat_id.0) {
                    Some(AdminListFetch::Loaded(entries)) => Some(
                        entries
                            .iter()
                            .map(|entry| entry.user_id)
                            .collect::<Vec<_>>(),
                    ),
                    _ => None,
                });
        match (from_list, fetch) {
            (Some(list), _) => list,
            (None, Some(ChatEventLogFetch::Loaded(page))) => page.admin_user_ids(),
            _ => Vec::new(),
        }
    }

    /// Per-admin filter chips. The selection goes to the server as
    /// `getChatEventLog.user_ids` (tdesktop's admin filter); with nothing
    /// selected every admin shows.
    pub(super) fn event_log_admin_chips(
        &self,
        chat_id: ChatId,
        admins: &[i64],
        selected: &[i64],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut chips = div()
            .id("event-log-admin-filters")
            .flex()
            .flex_wrap()
            .w_full()
            .gap_1();
        let all_label = if selected.is_empty() {
            tr(keys::Q_OPS_ALL_ADMINS).to_string()
        } else {
            tr(keys::Q_OPS_ALL_ADMINS_2).to_string()
        };
        chips = chips.child(ghost_text_button(
            "event-log-admin-all",
            all_label,
            cx.listener(move |this, _, _, cx| {
                if let Some(live) = this.live.as_mut() {
                    live.driver.clear_chat_event_log_users(chat_id);
                }
                this.refresh_event_log(chat_id, cx);
            }),
        ));
        for user_id in admins.iter().copied() {
            let name = self.group_call_participant_name(&MessageSender::User { user_id });
            let label = if selected.contains(&user_id) {
                format!("✓ {name}")
            } else {
                name
            };
            chips = chips.child(ghost_text_button(
                format!("event-log-admin-{user_id}"),
                label,
                cx.listener(move |this, _, _, cx| {
                    if let Some(live) = this.live.as_mut() {
                        live.driver.toggle_chat_event_log_user(chat_id, user_id);
                    }
                    this.refresh_event_log(chat_id, cx);
                }),
            ));
        }
        chips.into_any_element()
    }

    /// Slice G2: one toggle chip per `chatEventLogFilters` category
    /// (schema 1.8.67, line 7956). Active chips show a check mark; the
    /// "Clear" chip appears when any category is active (an empty
    /// selection means "all types" — the driver passes `null`).
    pub(super) fn event_log_filter_chips(
        &self,
        chat_id: ChatId,
        active: ChatEventLogFilterSet,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let categories: Vec<(&str, &str, bool, fn(&mut ChatEventLogFilterSet))> = vec![
            (
                "message-edits",
                tr(keys::Q_OPS_EDITS),
                active.message_edits,
                |f| f.message_edits = !f.message_edits,
            ),
            (
                "message-deletions",
                tr(keys::Q_OPS_DELETIONS),
                active.message_deletions,
                |f| f.message_deletions = !f.message_deletions,
            ),
            (
                "message-pins",
                tr(keys::Q_OPS_PINS),
                active.message_pins,
                |f| f.message_pins = !f.message_pins,
            ),
            (
                "member-joins",
                tr(keys::Q_OPS_JOINS),
                active.member_joins,
                |f| f.member_joins = !f.member_joins,
            ),
            (
                "member-leaves",
                tr(keys::Q_OPS_LEAVES),
                active.member_leaves,
                |f| f.member_leaves = !f.member_leaves,
            ),
            (
                "member-invites",
                tr(keys::Q_OPS_INVITES),
                active.member_invites,
                |f| f.member_invites = !f.member_invites,
            ),
            (
                "member-promotions",
                tr(keys::Q_OPS_PROMOTIONS),
                active.member_promotions,
                |f| f.member_promotions = !f.member_promotions,
            ),
            (
                "member-restrictions",
                tr(keys::Q_OPS_RESTRICTIONS),
                active.member_restrictions,
                |f| f.member_restrictions = !f.member_restrictions,
            ),
            (
                "member-tag-changes",
                tr(keys::Q_OPS_TAGS),
                active.member_tag_changes,
                |f| f.member_tag_changes = !f.member_tag_changes,
            ),
            (
                "info-changes",
                tr(keys::Q_OPS_INFO),
                active.info_changes,
                |f| f.info_changes = !f.info_changes,
            ),
            (
                "setting-changes",
                tr(keys::MENU_SETTINGS),
                active.setting_changes,
                |f| f.setting_changes = !f.setting_changes,
            ),
            (
                "invite-link-changes",
                tr(keys::ADMIN_LOG_FILTER_INVITE_LINKS),
                active.invite_link_changes,
                |f| f.invite_link_changes = !f.invite_link_changes,
            ),
            (
                "video-chat-changes",
                tr(keys::ADMIN_LOG_FILTER_VOICE_CHATS),
                active.video_chat_changes,
                |f| f.video_chat_changes = !f.video_chat_changes,
            ),
            (
                "forum-changes",
                tr(keys::Q_OPS_FORUM),
                active.forum_changes,
                |f| f.forum_changes = !f.forum_changes,
            ),
            (
                "subscription-extensions",
                tr(keys::Q_OPS_STARS),
                active.subscription_extensions,
                |f| f.subscription_extensions = !f.subscription_extensions,
            ),
        ];
        let mut chips = div()
            .id("event-log-filters")
            .flex()
            .flex_wrap()
            .w_full()
            .gap_1();
        for (id, label, enabled, toggle) in categories {
            let label = if enabled {
                format!("✓ {label}")
            } else {
                label.to_string()
            };
            chips = chips.child(ghost_text_button(
                format!("event-log-filter-{id}"),
                label,
                cx.listener(move |this, _, _, cx| {
                    this.toggle_event_log_filter(chat_id, toggle, cx);
                }),
            ));
        }
        if active.any_enabled() {
            chips = chips.child(ghost_text_button(
                "event-log-filter-clear",
                tr(keys::Q_OPS_CLEAR),
                cx.listener(move |this, _, _, cx| {
                    this.clear_event_log_filters(chat_id, cx);
                }),
            ));
        }
        chips.into_any_element()
    }

    /// Slice G2: clear every event-log filter category for the chat and
    /// refetch (empty selection = all types).
    pub(super) fn clear_event_log_filters(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver
                .session
                .groups
                .event_log_filters
                .remove(&chat_id.0);
            if live.driver.refresh_chat_event_log(chat_id).is_err() {
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_OPS_COULD_NOT_CLEAR_FILTERS)));
            }
        }
        cx.notify();
    }

    /// Phase D3c: one admin-log row — actor name, action description, and
    /// relative timestamp.
    pub(super) fn event_log_row(
        &self,
        event: &ParsedChatEvent,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let actor = self.group_call_participant_name(&event.member_id);
        let description = self.event_log_action_description(event);
        div()
            .id(("event-log-row", event.id as u64))
            .flex()
            .w_full()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .truncate()
                            .child(super::bidi_line::one_line_plain(actor)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(description),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(event_log_relative_time(event.date)),
            )
            .into_any_element()
    }

    /// Phase D3c: human description of an admin-log action. Every string
    /// is built only from parsed fields; unhandled constructors render
    /// the honest generic "performed an action".
    pub(super) fn event_log_action_description(&self, event: &ParsedChatEvent) -> String {
        let user_name = |user_id: i64| {
            self.session()
                .and_then(|session| session.user(user_id))
                .map(|user| user.display_name())
                .unwrap_or_else(|| tr_with(keys::Q_OPS_USER, &[("user_id", &user_id.to_string())]))
        };
        let sender_name = |sender: &MessageSender| match sender {
            MessageSender::User { user_id } => user_name(*user_id),
            MessageSender::Chat { chat_id } => self
                .session()
                .and_then(|session| session.chats.get(chat_id))
                .map(|chat| chat.title.clone())
                .unwrap_or_else(|| tr_with(keys::Q_OPS_CHAT, &[("chat_id", &chat_id.to_string())])),
        };
        // Short quoted excerpt; long titles/descriptions are cut rather
        // than wrapping the row.
        let quote = |text: &str| {
            let mut short: String = text.chars().take(60).collect();
            if text.chars().count() > 60 {
                short.push('…');
            }
            format!("\"{short}\"")
        };
        let with_text = |verb: &str, text: &str| {
            if text.is_empty() {
                verb.to_owned()
            } else {
                format!("{verb} {}", quote(text))
            }
        };
        match &event.action {
            ChatEventAction::MessageEdited { text, .. } => {
                with_text(tr(keys::Q_OPS_EDITED_A_MESSAGE), text)
            }
            ChatEventAction::MessageDeleted { text, .. } => {
                with_text(tr(keys::Q_OPS_DELETED_A_MESSAGE), text)
            }
            ChatEventAction::MessagePinned { text, .. } => {
                with_text(tr(keys::Q_OPS_PINNED_A_MESSAGE), text)
            }
            ChatEventAction::MessageUnpinned { text, .. } => {
                with_text(tr(keys::Q_OPS_UNPINNED_A_MESSAGE), text)
            }
            ChatEventAction::MemberJoined => tr(keys::Q_OPS_JOINED_THE_CHAT).to_owned(),
            // B4: `chatEventPollStopped` (schema 1.8.67 line 7776) — TGX
            // `EventLogPollStopped` / `EventLogQuizStopped` copy.
            ChatEventAction::PollStopped { is_quiz } => {
                if *is_quiz {
                    tr(keys::Q_OPS_STOPPED_THE_QUIZ).to_owned()
                } else {
                    tr(keys::Q_OPS_STOPPED_THE_POLL).to_owned()
                }
            }
            ChatEventAction::MemberJoinedByInviteLink {
                invite_link_name, ..
            } => {
                if invite_link_name.is_empty() {
                    tr(keys::Q_OPS_JOINED_VIA_AN_INVITE_LINK).to_owned()
                } else {
                    tr_with(
                        keys::Q_OPS_JOINED_VIA_INVITE_LINK,
                        &[("quote", &(quote(invite_link_name)).to_string())],
                    )
                }
            }
            ChatEventAction::MemberJoinedByRequest {
                approver_user_id, ..
            } => {
                if *approver_user_id == 0 {
                    tr(keys::Q_OPS_JOINED_VIA_AN_APPROVED_JOIN).to_owned()
                } else {
                    tr_with(
                        keys::Q_OPS_JOIN_REQUEST_APPROVED_BY,
                        &[("user_name", &(user_name(*approver_user_id)).to_string())],
                    )
                }
            }
            ChatEventAction::MemberInvited { user_id, .. } => {
                format!("invited {}", user_name(*user_id))
            }
            ChatEventAction::MemberPromoted {
                user_id,
                old_status,
                new_status,
            } => {
                let name = user_name(*user_id);
                match (old_status, new_status) {
                    (_, ChannelMemberStatus::Creator) => {
                        tr_with(keys::ADMIN_LOG_TRANSFERRED, &[("user", &name.to_string())])
                    }
                    (_, ChannelMemberStatus::Administrator) => tr_with(
                        keys::Q_OPS_PROMOTED_TO_ADMINISTRATOR,
                        &[("name", &name.to_string())],
                    ),
                    (ChannelMemberStatus::Administrator, ChannelMemberStatus::Member) => tr_with(
                        keys::Q_OPS_REMOVED_AS_ADMINISTRATOR,
                        &[("name", &name.to_string())],
                    ),
                    _ => tr_with(keys::Q_OPS_CHANGED_S_ROLE, &[("name", &name.to_string())]),
                }
            }
            ChatEventAction::MemberRestricted {
                member_id,
                old_status,
                new_status,
            } => {
                let name = sender_name(member_id);
                match (old_status, new_status) {
                    (_, ChannelMemberStatus::Banned) => format!("banned {name}"),
                    (_, ChannelMemberStatus::Restricted) => format!("restricted {name}"),
                    (ChannelMemberStatus::Banned, _) => format!("unbanned {name}"),
                    (ChannelMemberStatus::Restricted, _) => tr_with(
                        keys::Q_OPS_LIFTED_RESTRICTIONS_ON,
                        &[("name", &name.to_string())],
                    ),
                    _ => tr_with(
                        keys::Q_OPS_CHANGED_S_RESTRICTIONS,
                        &[("name", &name.to_string())],
                    ),
                }
            }
            ChatEventAction::DescriptionChanged {
                new_description, ..
            } => {
                if new_description.is_empty() {
                    tr(keys::Q_OPS_CLEARED_THE_DESCRIPTION).to_owned()
                } else {
                    tr_with(
                        keys::Q_OPS_CHANGED_THE_DESCRIPTION_TO,
                        &[("quote", &(quote(new_description)).to_string())],
                    )
                }
            }
            ChatEventAction::PhotoChanged => tr(keys::Q_OPS_CHANGED_THE_CHAT_PHOTO).to_owned(),
            ChatEventAction::TitleChanged { new_title, .. } => tr_with(
                keys::Q_OPS_CHANGED_THE_TITLE_TO,
                &[("quote", &(quote(new_title)).to_string())],
            ),
            ChatEventAction::InviteLinkEdited {
                old_name, new_name, ..
            } => {
                let name = if new_name.is_empty() {
                    old_name
                } else {
                    new_name
                };
                if name.is_empty() {
                    tr(keys::Q_OPS_EDITED_AN_INVITE_LINK).to_owned()
                } else {
                    tr_with(
                        keys::ADMIN_LOG_EDITED_INVITE_LINK,
                        &[("link", &(quote(name)).to_string())],
                    )
                }
            }
            ChatEventAction::InviteLinkRevoked { name, .. } => {
                if name.is_empty() {
                    tr(keys::Q_OPS_REVOKED_AN_INVITE_LINK).to_owned()
                } else {
                    tr_with(
                        keys::Q_OPS_REVOKED_INVITE_LINK,
                        &[("quote", &(quote(name)).to_string())],
                    )
                }
            }
            ChatEventAction::InviteLinkDeleted { name, .. } => {
                if name.is_empty() {
                    tr(keys::Q_OPS_DELETED_AN_INVITE_LINK).to_owned()
                } else {
                    tr_with(
                        keys::Q_OPS_DELETED_INVITE_LINK,
                        &[("quote", &(quote(name)).to_string())],
                    )
                }
            }
            ChatEventAction::Unsupported { .. } => tr(keys::Q_OPS_PERFORMED_AN_ACTION).to_owned(),
        }
    }
}
