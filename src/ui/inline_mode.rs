//! Bots slice: `@botname query` inline mode in the composer.
//!
//! Typing `@botname query` at the composer start resolves the bot (local
//! user cache first, `searchPublicChat` otherwise), debounces
//! `getInlineQueryResults` (100ms, TGX `InlineSearchContext`), and shows a
//! selectable results dropdown above the composer. Picking a result sends
//! it via `sendInlineQueryResultMessage` and clears the composer (the
//! schema always clears the chat draft on that call).
//!
//! The trigger parser lives in `crate::composer` (pure, unit-tested);
//! this module owns the App-side sync, dropdown, keyboard/mouse picking,
//! and the secret-chat alert hookup. `src/ui/mod.rs` only declares the
//! module, holds the three fields, and calls in at the existing
//! composer/keystroke/render sites.

use super::QuillApp;
use super::pressable::PressableDiv;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::gpui::{AnyElement, Context, Window, div, prelude::*};
use quill::composer::inline_query_trigger;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{InlineBotResolve, InlineQueryFetch, InlineQuerySlot, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::InlineQueryResultSummary;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
/// One dropdown row: a status line, a result, or the "load more" row.
#[derive(Clone)]
enum InlineRow {
    Status(String),
    Result {
        query_id: i64,
        result_id: String,
        title: String,
        subtitle: String,
    },
    More {
        next_offset: String,
    },
}

/// Rows for the current trigger. `None` when there is no `@bot` trigger
/// (the dropdown stays closed).
fn inline_rows(app: &QuillApp, cx: &Context<QuillApp>) -> Option<Vec<InlineRow>> {
    let text = app.composer.read(cx).value().to_string();
    let (username, query) = inline_query_trigger(&text)?;
    let session = app.session()?;
    let rows = match &session.inline_bot_resolve {
        None | Some(InlineBotResolve::Resolving { .. }) => {
            vec![InlineRow::Status(format!("Resolving @{username}…"))]
        }
        Some(InlineBotResolve::Failed { reason, .. }) => vec![InlineRow::Status(reason.clone())],
        Some(InlineBotResolve::Resolved {
            is_inline: Some(false),
            ..
        }) => vec![InlineRow::Status(format!(
            "@{username} doesn't support inline mode"
        ))],
        // `is_inline` is `Some(true)` (cache-verified) or `None`
        // (`searchPublicChat` answer without a user object — the query
        // attempt itself is the capability check).
        Some(InlineBotResolve::Resolved { user_id, .. }) => {
            let chat_id = app.open_chat_id()?;
            let slot = session.inline_query.as_ref().filter(|slot| {
                slot.chat_id == chat_id && slot.bot_user_id == *user_id && slot.query == query
            });
            match slot.map(|slot| &slot.fetch) {
                None | Some(InlineQueryFetch::Loading) => {
                    vec![InlineRow::Status("Loading results…".to_string())]
                }
                Some(InlineQueryFetch::Failed(reason)) => vec![InlineRow::Status(reason.clone())],
                Some(InlineQueryFetch::Loaded {
                    inline_query_id,
                    results,
                    next_offset,
                    ..
                }) => {
                    if results.is_empty() {
                        vec![InlineRow::Status("No results".to_string())]
                    } else {
                        let mut rows: Vec<InlineRow> = results
                            .iter()
                            .map(|r| InlineRow::Result {
                                query_id: *inline_query_id,
                                result_id: r.id.clone(),
                                title: result_title(r),
                                subtitle: r.description.clone(),
                            })
                            .collect();
                        if !next_offset.is_empty() {
                            rows.push(InlineRow::More {
                                next_offset: next_offset.clone(),
                            });
                        }
                        rows
                    }
                }
            }
        }
    };
    Some(rows)
}

/// Row title: the result title, falling back to a capitalized kind label
/// (`sticker` results carry no title).
fn result_title(result: &InlineQueryResultSummary) -> String {
    if !result.title.is_empty() {
        return result.title.clone();
    }
    let mut kind = result.kind.clone();
    if let Some(first) = kind.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    if kind.is_empty() {
        "Result".to_string()
    } else {
        kind
    }
}

impl QuillApp {
    /// Bots slice: keep the inline-results dropdown in sync with the
    /// composer text. Called on every composer change (next to
    /// `sync_command_menu`).
    pub(super) fn sync_inline_mode(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        let Some((username, query)) = inline_query_trigger(&text) else {
            // Trigger gone: close, and clear a typed-mode alert marker
            // (an empty stash is ours; a non-empty one belongs to a
            // `SwitchInline` button press).
            if self.composer_ui.pending_inline_bot_alert.as_deref() == Some("") {
                self.composer_ui.pending_inline_bot_alert = None;
            }
            self.composer_ui.inline_results_open = false;
            self.composer_ui.inline_query_armed = None;
            return;
        };
        // TGX: the first inline use in a secret chat shows the privacy
        // alert before any query is sent. The banner renders while the
        // stash is set; confirming re-syncs (the text is already there).
        if self.open_chat_is_secret() && !self.composer_ui.inline_bot_alert_shown {
            if self.composer_ui.pending_inline_bot_alert.is_none() {
                self.composer_ui.pending_inline_bot_alert = Some(String::new());
            }
            self.composer_ui.inline_results_open = false;
            cx.notify();
            return;
        }
        let username_changed = match self.session().and_then(|s| s.inline_bot_resolve.as_ref()) {
            Some(InlineBotResolve::Resolving { username: u, .. })
            | Some(InlineBotResolve::Resolved { username: u, .. })
            | Some(InlineBotResolve::Failed { username: u, .. }) => u != username,
            None => true,
        };
        if username_changed {
            self.resolve_inline_bot(username);
            self.composer_ui.inline_query_armed = None;
            self.composer_ui.inline_results_open = true;
            self.composer_ui.inline_results_selected = 0;
            cx.notify();
            return;
        }
        // Same username: only bots with inline mode (or unknown
        // capability) get a debounced query.
        let proceed = matches!(
            self.session().and_then(|s| s.inline_bot_resolve.as_ref()),
            Some(InlineBotResolve::Resolved {
                is_inline: Some(true) | None,
                ..
            })
        );
        if !proceed {
            self.composer_ui.inline_results_open = true;
            cx.notify();
            return;
        }
        self.maybe_dispatch_inline_query(username, query, cx);
        self.composer_ui.inline_results_open = true;
        cx.notify();
    }

    /// Bots slice: event-driven progress for the inline flow. Called from
    /// `poll_live` after session updates land — covers the two cases no
    /// composer keystroke can reach: (1) `searchPublicChat` resolved the
    /// bot while the user sat on the trigger text, so the debounced
    /// `getInlineQueryResults` was never armed; (2) an older in-flight
    /// query's response landed and the newest query still needs sending.
    /// Idempotent: `maybe_dispatch_inline_query` no-ops when the slot is
    /// fresh or a debounce timer is already armed for this trigger.
    pub(super) fn progress_inline_mode(&mut self, cx: &mut Context<Self>) {
        if self.live.is_none() {
            return;
        }
        let text = self.composer.read(cx).value().to_string();
        let Some((username, query)) = inline_query_trigger(&text) else {
            return;
        };
        let resolved_same = self
            .session()
            .and_then(|s| s.inline_bot_resolve.as_ref())
            .is_some_and(|r| match r {
                InlineBotResolve::Resolved {
                    username: u,
                    is_inline: Some(true) | None,
                    ..
                } => u == username,
                _ => false,
            });
        if !resolved_same {
            return;
        }
        self.maybe_dispatch_inline_query(username, query, cx);
    }

    /// Resolve `@username`: local user-cache hit short-circuits (no
    /// request); otherwise `searchPublicChat` via the driver.
    fn resolve_inline_bot(&mut self, username: &str) {
        let hit = self.session().and_then(|session| {
            session
                .users
                .values()
                .find(|user| {
                    user.username.eq_ignore_ascii_case(username)
                        || user
                            .active_usernames
                            .iter()
                            .any(|active| active.eq_ignore_ascii_case(username))
                })
                .map(|user| (user.id, user.is_bot, user.is_inline))
        });
        let Some(live) = self.live.as_mut() else {
            return;
        };
        match hit {
            Some((_, false, _)) => {
                live.driver.session.inline_bot_resolve = Some(InlineBotResolve::Failed {
                    username: username.to_string(),
                    reason: format!("@{username} is not a bot"),
                });
            }
            Some((user_id, true, is_inline)) => {
                live.driver.session.inline_bot_resolve = Some(InlineBotResolve::Resolved {
                    username: username.to_string(),
                    user_id,
                    is_inline: Some(is_inline),
                });
            }
            None => {
                if live.driver.resolve_inline_bot(username).is_err() {
                    live.driver.session.inline_bot_resolve = Some(InlineBotResolve::Failed {
                        username: username.to_string(),
                        reason: "could not look up the bot".to_string(),
                    });
                }
            }
        }
    }

    /// Debounced `getInlineQueryResults` (100ms quiet window, TGX). The
    /// token pattern mirrors the link-preview prefetch: only the latest
    /// quiet window fires, and it re-checks the trigger survived.
    fn maybe_dispatch_inline_query(&mut self, username: &str, query: &str, cx: &mut Context<Self>) {
        let (user_id, chat_id) = match (
            self.session()
                .and_then(|s| s.inline_bot_resolve.as_ref())
                .and_then(|r| match r {
                    InlineBotResolve::Resolved { user_id, .. } => Some(*user_id),
                    _ => None,
                }),
            self.open_chat_id(),
        ) {
            (Some(user_id), Some(chat_id)) => (user_id, chat_id),
            _ => return,
        };
        let fresh = self
            .session()
            .and_then(|s| s.inline_query.as_ref())
            .is_some_and(|slot| {
                slot.chat_id == chat_id && slot.bot_user_id == user_id && slot.query == query
            });
        if fresh {
            return;
        }
        // Idempotency: `poll_live` re-runs the progress check after every
        // batch of session updates — never arm two timers for the same
        // trigger. A newer keystroke bumps the token and re-arms for its
        // own (username, query).
        if self
            .composer_ui
            .inline_query_armed
            .as_ref()
            .is_some_and(|(u, q)| u == username && q == query)
        {
            return;
        }
        self.composer_ui.inline_query_token = self.composer_ui.inline_query_token.wrapping_add(1);
        let token = self.composer_ui.inline_query_token;
        self.composer_ui.inline_query_armed = Some((username.to_string(), query.to_string()));
        let username = username.to_string();
        let query = query.to_string();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            this.update(cx, |this, cx| {
                if this.composer_ui.inline_query_token != token {
                    return;
                }
                if this
                    .composer_ui
                    .inline_query_armed
                    .as_ref()
                    .is_some_and(|(u, q)| u == &username && q == &query)
                {
                    this.composer_ui.inline_query_armed = None;
                }
                // The trigger must have survived the quiet window; a
                // newer keystroke scheduled its own timer.
                let text = this.composer.read(cx).value().to_string();
                if inline_query_trigger(&text) != Some((username.as_str(), query.as_str())) {
                    return;
                }
                let Some(chat_id) = this.open_chat_id() else {
                    return;
                };
                let Some(live) = this.live.as_mut() else {
                    return;
                };
                if live
                    .driver
                    .inline_query(user_id, chat_id, &query, "")
                    .is_err()
                {
                    this.composer_ui.inline_results_open = false;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Bots slice: close the inline-results dropdown. Returns true when it
    /// consumed the key (open); mirrors `close_command_menu`.
    pub(super) fn close_inline_results(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.composer_ui.inline_results_open {
            return false;
        }
        self.composer_ui.inline_results_open = false;
        self.composer_ui.inline_results_selected = 0;
        self.composer_ui.inline_query_armed = None;
        cx.notify();
        true
    }

    /// Bots slice: Up/Down highlight across result + "more" rows.
    /// Returns true when the dropdown consumed the key.
    pub(super) fn step_inline_results(&mut self, delta: i32, cx: &mut Context<Self>) -> bool {
        let rows = inline_rows(self, cx).unwrap_or_default();
        if !self.composer_ui.inline_results_open || rows.is_empty() {
            return false;
        }
        self.composer_ui.inline_results_selected =
            (self.composer_ui.inline_results_selected as i32 + delta).rem_euclid(rows.len() as i32)
                as usize;
        cx.notify();
        true
    }

    /// Bots slice: Enter with the dropdown open picks the highlighted
    /// row. Returns true when Enter was consumed.
    pub(super) fn pick_inline_result_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let rows = inline_rows(self, cx).unwrap_or_default();
        if !self.composer_ui.inline_results_open || rows.is_empty() {
            return false;
        }
        let index = self.composer_ui.inline_results_selected.min(rows.len() - 1);
        self.pick_inline_row(&rows[index], window, cx);
        true
    }

    /// Bots slice: tap / Enter pick. A result row sends via
    /// `sendInlineQueryResultMessage` and clears the composer (the schema
    /// always clears the chat draft on that call); the "more" row loads
    /// the next page into the existing slot.
    fn pick_inline_row(&mut self, row: &InlineRow, window: &mut Window, cx: &mut Context<Self>) {
        match row {
            InlineRow::Status(_) => {}
            InlineRow::More { next_offset } => {
                let text = self.composer.read(cx).value().to_string();
                let Some((_, query)) = inline_query_trigger(&text) else {
                    return;
                };
                let (user_id, chat_id, offset) = match (
                    self.session()
                        .and_then(|s| s.inline_bot_resolve.as_ref())
                        .and_then(|r| match r {
                            InlineBotResolve::Resolved { user_id, .. } => Some(*user_id),
                            _ => None,
                        }),
                    self.open_chat_id(),
                ) {
                    (Some(user_id), Some(chat_id)) => (user_id, chat_id, next_offset.clone()),
                    _ => return,
                };
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.inline_query(user_id, chat_id, query, &offset);
                }
                cx.notify();
            }
            InlineRow::Result {
                query_id,
                result_id,
                ..
            } => {
                let query_id = *query_id;
                let result_id = result_id.clone();
                let Some(chat_id) = self.open_chat_id() else {
                    return;
                };
                // Clear first: the schema clears the chat draft on send,
                // so the composer must never show a stale `@bot query`
                // after the round trip.
                self.composer.update(cx, |input, cx| {
                    input.set_value(String::new(), window, cx);
                });
                self.composer_ui.inline_results_open = false;
                self.composer_ui.inline_results_selected = 0;
                self.composer_ui.inline_query_armed = None;
                if let Some(live) = self.live.as_mut()
                    && live
                        .driver
                        .send_inline_query_result(chat_id, query_id, &result_id)
                        .is_err()
                {
                    self.connection.status_note = "could not send the inline result".into();
                }
                cx.notify();
            }
        }
    }

    /// Bots slice: the inline-results dropdown above the composer.
    /// Mirrors the `/` command menu's styling and click behavior.
    pub(super) fn inline_results_dropdown(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.composer_ui.inline_results_open {
            return None;
        }
        let rows = inline_rows(self, cx)?;
        if rows.is_empty() {
            return None;
        }
        let selected = self.composer_ui.inline_results_selected.min(rows.len() - 1);
        let mut list = div()
            .id("inline-results")
            .flex()
            .flex_col()
            .mx_4()
            .mb_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar);
        for (index, row) in rows.iter().enumerate() {
            let highlighted = index == selected;
            match row {
                InlineRow::Status(text) => {
                    list = list.child(
                        div()
                            .px_3()
                            .py_2()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(text.clone()),
                    );
                }
                InlineRow::Result {
                    title, subtitle, ..
                } => {
                    let row = row.clone();
                    let label = if subtitle.is_empty() {
                        title.clone()
                    } else {
                        format!("{title} — {subtitle}")
                    };
                    list = list.child(
                        div()
                            .id(("inline-result-item", index as u64))
                            .w_full()
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .role(gpui_kit::Role::Button)
                            .aria_label(label.clone())
                            .tab_index(0)
                            .cursor_pointer()
                            .pressable(cx.theme())
                            .when(highlighted, |this| this.bg(cx.theme().selection))
                            // On press: the composer's blur closes the
                            // results before a click could land.
                            .on_mouse_down(
                                gpui_kit::MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    window.prevent_default();
                                    this.pick_inline_row(&row, window, cx);
                                }),
                            )
                            .child(div().text_sm().child(label)),
                    );
                }
                InlineRow::More { .. } => {
                    let row = row.clone();
                    list = list.child(
                        div()
                            .id(("inline-result-more", index as u64))
                            .w_full()
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .role(gpui_kit::Role::Button)
                            .aria_label("Show more inline results")
                            .tab_index(0)
                            .cursor_pointer()
                            .pressable(cx.theme())
                            .when(highlighted, |this| this.bg(cx.theme().selection))
                            // On press: the composer's blur closes the
                            // results before a click could land.
                            .on_mouse_down(
                                gpui_kit::MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    window.prevent_default();
                                    this.pick_inline_row(&row, window, cx);
                                }),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("More results…"),
                            ),
                    );
                }
            }
        }
        Some(list.into_any_element())
    }
}

/// Screenshot demo fixture (`ready-inline-results`): like
/// `apply_ready_bot_chat`, but the Demo Bot is an inline bot (`@gif`,
/// `is_inline: true`) with an injected resolved slot and a loaded
/// results page, so the inline-results dropdown renders open above the
/// composer (injected, no live Telegram).
pub(super) fn apply_ready_inline_results(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    super::bots::apply_ready_bot_chat(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    // Re-inject the bot user with a username and inline capability (the
    // `updateUser` reducer replaces the cached object).
    let user_json = r#"{"@type":"updateUser","user":{"id":21,"first_name":"Demo","usernames":{"@type":"usernames","active_usernames":["gif"],"disabled_usernames":[],"editable_username":"gif"},"type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"is_inline":true,"inline_query_placeholder":"Search GIFs…","supports_guest_queries":false,"need_location":false,"active_user_count":0}}}"#;
    if let Some(owned) = copy_and_parse(user_json, seq, &dyn_sink) {
        session.apply(owned);
    }
    session.inline_bot_resolve = Some(InlineBotResolve::Resolved {
        username: "gif".into(),
        user_id: 21,
        is_inline: Some(true),
    });
    session.inline_query = Some(InlineQuerySlot {
        chat_id: ChatId(21),
        bot_user_id: 21,
        query: "cats".into(),
        fetch: InlineQueryFetch::Loaded {
            inline_query_id: 99,
            button: None,
            results: vec![
                InlineQueryResultSummary {
                    id: "a1".into(),
                    kind: "article".into(),
                    title: "Cute cats".into(),
                    description: "The cutest cats on Telegram".into(),
                },
                InlineQueryResultSummary {
                    id: "p2".into(),
                    kind: "photo".into(),
                    title: String::new(),
                    description: "A sleepy kitten".into(),
                },
                InlineQueryResultSummary {
                    id: "g3".into(),
                    kind: "gif".into(),
                    title: "Cat GIF".into(),
                    description: String::new(),
                },
            ],
            next_offset: "10".into(),
        },
    });
}
