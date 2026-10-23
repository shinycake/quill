//! Recent actions of a group or channel (tdesktop `history/admin_log/`):
//! a fixed header (what is shown, Filter, search, the explainer) over the
//! events, oldest first with the newest at the bottom like the chat history.
//! Earlier events load when the list is scrolled to its top.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::dialogs::group_admin_settings::GroupSettingsView;
use super::event_log_rows::{LogEntry, render_entry};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::event_log_list::{ListSync, plan_sync};
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::state::{AdminListFetch, ChatEventLogFetch, ChatEventLogPage};
use std::rc::Rc;
use std::time::Duration;

/// How often an open recent-actions page asks for new events (tdesktop's
/// `kNewEventsCheckInterval`).
const EVENT_LOG_POLL_INTERVAL: Duration = Duration::from_secs(5);
/// Typing in the search box searches once it pauses.
const SEARCH_DELAY: Duration = Duration::from_millis(400);
/// The list counts as at its top within this many pixels.
const TOP_SLACK: f32 = 2.;
/// Rows kept laid out beyond the visible part of the list.
const LIST_OVERDRAW: f32 = 400.;

impl QuillApp {
    /// The search box (created on first use; its value follows the chat's
    /// stored query).
    pub(super) fn ensure_event_log_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.admin.event_log_search.is_some() {
            return;
        }
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_PEOPLE_SEARCH_RECENT_ACTIONS))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::Change) {
                this.in_window_of(window, cx, |this, _, cx| this.event_log_search_typed(cx));
            }
        })
        .detach();
        self.admin.event_log_search = Some(input);
    }

    /// The chat whose recent actions are open.
    fn event_log_open_chat(&self) -> Option<ChatId> {
        self.admin
            .group_settings_dialog
            .as_ref()
            .filter(|dialog| {
                matches!(
                    dialog.view,
                    GroupSettingsView::RecentActions | GroupSettingsView::RecentActionsFilter
                )
            })
            .map(|dialog| dialog.chat_id)
    }

    /// The search text changed: search once typing pauses.
    fn event_log_search_typed(&mut self, cx: &mut Context<Self>) {
        self.admin.event_log_keystroke = self.admin.event_log_keystroke.wrapping_add(1);
        let keystroke = self.admin.event_log_keystroke;
        self.spawn_scoped(cx, async move |this, cx| {
            cx.background_executor().timer(SEARCH_DELAY).await;
            this.update(cx, |this, cx| {
                if this.admin.event_log_keystroke == keystroke
                    && let Some(chat_id) = this.event_log_open_chat()
                {
                    this.apply_event_log_search(chat_id, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Apply the search box value (the driver stores it per chat; an empty
    /// box clears it) when it differs from what the log is filtered by.
    pub(super) fn apply_event_log_search(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let query = self
            .admin
            .event_log_search
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_string())
            .unwrap_or_default();
        let current = self
            .session()
            .and_then(|session| session.groups.event_log_queries.get(&chat_id.0).cloned())
            .unwrap_or_default();
        if query == current {
            return;
        }
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

    /// Ask the list to tell the app when it is scrolled to its top.
    pub(super) fn hook_event_log_scroll(&mut self, cx: &mut Context<Self>) {
        if self.admin.event_log_hooked {
            return;
        }
        self.admin.event_log_hooked = true;
        let app = cx.weak_entity();
        // Runs while the list holds its own state: it must not touch the
        // list, so the load starts after the scroll event is done.
        self.admin
            .event_log_list
            .set_scroll_handler(move |event, _, cx| {
                if event.visible_range.start == 0 {
                    let app = app.clone();
                    cx.defer(move |cx| {
                        app.update(cx, |this, cx| {
                            if let Some(chat_id) = this.event_log_open_chat() {
                                this.load_more_event_log(chat_id, cx);
                            }
                        })
                        .ok();
                    });
                }
            });
    }

    /// Keep an open recent-actions page live: every few seconds the newest
    /// events are fetched and join the bottom of the list (the list keeps a
    /// reader who has scrolled up where they are).
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
        if self.event_log_open_chat() != Some(chat_id) {
            return false;
        }
        if self.frame.window_active.get()
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

    /// Administrators to offer in the filter: the loaded administrator
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

    /// "Recent actions" of the group or channel, for its administrators and
    /// the creator (`getChatEventLog` "requires administrator rights").
    pub(super) fn event_log_section(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        if !session.chat_can_view_event_log(chat_id) {
            return None;
        }
        let groups = &session.groups;
        let fetch = groups.event_logs.get(&chat_id.0).cloned();
        let query = groups
            .event_log_queries
            .get(&chat_id.0)
            .cloned()
            .unwrap_or_default();
        let filtered = groups.event_log_filters.contains_key(&chat_id.0)
            || groups
                .event_log_users
                .get(&chat_id.0)
                .is_some_and(|users| !users.is_empty());
        let is_channel = session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.is_channel());
        let theme = cx.theme();
        let muted = theme.muted_foreground;

        let title = if filtered {
            tr(keys::ADMIN_LOG_TITLE_SELECTED)
        } else {
            tr(keys::ADMIN_LOG_TITLE_ALL)
        };
        let mut filter_button = Button::new("event-log-filter-open")
            .label(tr(keys::ADMIN_LOG_FILTER_TITLE))
            .small()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_event_log_filter(chat_id, cx);
            }));
        filter_button = if filtered {
            filter_button.primary()
        } else {
            filter_button.outline()
        };
        let mut header = div().flex().flex_col().flex_none().w_full().gap_2().child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(div().text_sm().font_semibold().child(title))
                .child(filter_button),
        );
        if let Some(input) = self.admin.event_log_search.clone() {
            header = header.child(
                Textarea::new(&input)
                    .aria_label(tr(keys::Q_PEOPLE_SEARCH_RECENT_ACTIONS))
                    .h(px(32.)),
            );
        }
        header = header.child(
            div()
                .text_xs()
                .text_color(muted)
                .child(quill::admin_extras::event_log_about(is_channel)),
        );

        let content = match fetch {
            None | Some(ChatEventLogFetch::Loading) => {
                self.event_log_reset_list();
                self.event_log_centered(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_sm()
                        .text_color(muted)
                        .child(Spinner::new().small())
                        .child(tr(keys::Q_OPS_LOADING_RECENT_ACTIONS)),
                )
            }
            Some(ChatEventLogFetch::Failed(message)) => {
                self.event_log_reset_list();
                self.event_log_centered(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .child(div().text_sm().text_color(muted).child(message))
                        .child(
                            Button::new("event-log-retry")
                                .label(tr(keys::Q_OPS_RETRY))
                                .small()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.refresh_event_log(chat_id, cx);
                                })),
                        ),
                )
            }
            Some(ChatEventLogFetch::Loaded(page)) if page.events.is_empty() => {
                self.event_log_reset_list();
                self.event_log_centered(self.event_log_empty(&query, filtered, is_channel, cx))
            }
            Some(ChatEventLogFetch::Loaded(page)) => self.event_log_list_view(chat_id, &page, cx),
        };
        Some(
            div()
                .id("event-log-section")
                .flex()
                .flex_col()
                .size_full()
                .gap_2()
                .child(header)
                .child(content)
                .into_any_element(),
        )
    }

    /// tdesktop's empty text: a semibold title over a description that
    /// names the search, the filter or the plain lack of events.
    fn event_log_empty(
        &self,
        query: &str,
        filtered: bool,
        is_channel: bool,
        cx: &Context<Self>,
    ) -> Div {
        let searched = !query.is_empty();
        let title = if searched || filtered {
            tr(keys::ADMIN_LOG_NO_RESULTS_TITLE)
        } else {
            tr(keys::ADMIN_LOG_NO_EVENTS_TITLE)
        };
        let text = if searched {
            tr_with(keys::ADMIN_LOG_NO_RESULTS_SEARCH_TEXT, &[("query", query)])
        } else if filtered {
            tr(keys::ADMIN_LOG_NO_RESULTS_TEXT).to_owned()
        } else if is_channel {
            tr(keys::ADMIN_LOG_NO_EVENTS_TEXT_CHANNEL).to_owned()
        } else {
            tr(keys::ADMIN_LOG_NO_EVENTS_TEXT).to_owned()
        };
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .text_center()
            .child(div().text_sm().font_semibold().child(title))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(text),
            )
    }

    fn event_log_centered(&self, child: impl IntoElement) -> AnyElement {
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .px_4()
            .child(child)
            .into_any_element()
    }

    /// Forget the rows the list was built for.
    fn event_log_reset_list(&self) {
        let mut shown = self.admin.event_log_ids.borrow_mut();
        if !shown.is_empty() {
            shown.clear();
            self.admin.event_log_list.reset(0);
        }
    }

    /// The events, oldest first. The list is told what joined it (earlier
    /// events at the top, polled ones at the bottom) so the reader's place
    /// holds; anything else starts it over at the newest event.
    fn event_log_list_view(
        &self,
        chat_id: ChatId,
        page: &ChatEventLogPage,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chronological: Vec<_> = page.events.iter().rev().cloned().collect();
        let entries: Rc<Vec<LogEntry>> = Rc::new(self.event_log_entries(&chronological));
        let ids: Vec<i64> = entries.iter().map(|entry| entry.id).collect();
        let state = self.admin.event_log_list.clone();
        {
            let mut shown = self.admin.event_log_ids.borrow_mut();
            match plan_sync(&shown, &ids) {
                ListSync::Keep => {}
                ListSync::Reset => state.reset(ids.len()),
                ListSync::Grow { older, newer } => {
                    let before = shown.len();
                    if older > 0 {
                        state.splice(0..0, older);
                        // The old first row may have lost its day label.
                        state.remeasure_items(older..older.saturating_add(1));
                    }
                    if newer > 0 {
                        let end = older.saturating_add(before);
                        state.splice(end..end, newer);
                    }
                }
            }
            *shown = ids;
        }
        // Only once the list has been laid out: before that every offset is 0.
        let laid_out = state.viewport_bounds().size.height > px(0.);
        let at_top = laid_out && state.scroll_px_offset_for_scrollbar().y > px(-TOP_SLACK);
        if page.has_more && at_top && self.admin.event_log_autoload.get() != page.events.len() {
            // Short lists never scroll, so the next page is asked for here,
            // once for each length the list reaches.
            self.admin.event_log_autoload.set(page.events.len());
            let app = cx.weak_entity();
            cx.defer(move |cx| {
                app.update(cx, |this, cx| this.load_more_event_log(chat_id, cx))
                    .ok();
            });
        }
        let rows = Rc::clone(&entries);
        let rows_list = list(state, move |ix, _, cx| {
            rows.get(ix)
                .map(|entry| render_entry(entry, cx))
                .unwrap_or_else(|| div().into_any_element())
        })
        .size_full();
        let theme = cx.theme();
        let mut area = div()
            .id("event-log-list")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full();
        if page.has_more {
            // Above the first row while earlier events remain to load.
            area = area.child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .py_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(Spinner::new().xsmall())
                    .child(tr(keys::Q_ADMIN_LOG_LOADING_OLDER)),
            );
        }
        area = area.child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(rows_list),
        );
        area.into_any_element()
    }
}

/// A fresh list for the open page: newest at the bottom.
pub(super) fn new_event_log_list() -> ListState {
    ListState::new(0, ListAlignment::Bottom, px(LIST_OVERDRAW))
}
