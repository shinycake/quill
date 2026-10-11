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
use gpui_kit::gpui::{AnyElement, Context, Window};
use quill::composer::inline_query_trigger;
use quill::inline_grid::layout_results;
use quill::state::{InlineBotResolve, InlineQueryFetch};
use quill::telegram::envelope::{InlineQueryResultSummary, InlineQueryResultsButton};
use std::time::Duration;

/// What the results panel shows for the current `@bot query` trigger.
#[derive(Clone)]
pub(super) enum InlineView {
    /// Resolving, loading, failed or empty: one line of text.
    Status(String),
    /// Loaded results in arrival order, with the bot's button.
    Results {
        results: Vec<InlineQueryResultSummary>,
        button: Option<InlineQueryResultsButton>,
    },
}

impl InlineView {
    /// Result indices in the order the grid shows them, which is the order
    /// Up/Down walk.
    pub(super) fn display_order(&self) -> Vec<usize> {
        match self {
            Self::Status(_) => Vec::new(),
            Self::Results { results, .. } => layout_results(results).order(),
        }
    }
}

/// The panel's content for the current trigger. `None` when there is no
/// `@bot` trigger (the dropdown stays closed).
pub(super) fn inline_view(app: &QuillApp, cx: &Context<QuillApp>) -> Option<InlineView> {
    let text = app.composer.read(cx).value().to_string();
    let (username, query) = inline_query_trigger(&text)?;
    let session = app.session()?;
    let view = match &session.bots.inline_bot_resolve {
        None | Some(InlineBotResolve::Resolving { .. }) => {
            InlineView::Status(format!("Resolving @{username}…"))
        }
        Some(InlineBotResolve::Failed { reason, .. }) => InlineView::Status(reason.clone()),
        Some(InlineBotResolve::Resolved {
            is_inline: Some(false),
            ..
        }) => InlineView::Status(format!("@{username} doesn't support inline mode")),
        // `is_inline` is `Some(true)` (cache-verified) or `None`
        // (`searchPublicChat` answer without a user object — the query
        // attempt itself is the capability check).
        Some(InlineBotResolve::Resolved { user_id, .. }) => {
            let chat_id = app.open_chat_id()?;
            let slot = session.bots.inline_query.as_ref().filter(|slot| {
                slot.chat_id == chat_id && slot.bot_user_id == *user_id && slot.query == query
            });
            match slot.map(|slot| &slot.fetch) {
                None | Some(InlineQueryFetch::Loading) => {
                    InlineView::Status("Loading results…".to_string())
                }
                Some(InlineQueryFetch::Failed(reason)) => InlineView::Status(reason.clone()),
                Some(InlineQueryFetch::Loaded {
                    results, button, ..
                }) => {
                    // tdesktop keeps the panel for a button alone.
                    let button = button
                        .clone()
                        .filter(|button| quill::inline_grid::button_action(button).is_some());
                    if results.is_empty() && button.is_none() {
                        InlineView::Status("No results".to_string())
                    } else {
                        InlineView::Results {
                            results: results.clone(),
                            button,
                        }
                    }
                }
            }
        }
    };
    Some(view)
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
        let username_changed = match self
            .session()
            .and_then(|s| s.bots.inline_bot_resolve.as_ref())
        {
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
            self.session()
                .and_then(|s| s.bots.inline_bot_resolve.as_ref()),
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
            .and_then(|s| s.bots.inline_bot_resolve.as_ref())
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
        // A page that landed may still not fill the panel.
        self.load_more_inline_results_if_needed(cx);
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
                live.driver.session.bots.inline_bot_resolve = Some(InlineBotResolve::Failed {
                    username: username.to_string(),
                    reason: format!("@{username} is not a bot"),
                });
            }
            Some((user_id, true, is_inline)) => {
                live.driver.session.bots.inline_bot_resolve = Some(InlineBotResolve::Resolved {
                    username: username.to_string(),
                    user_id,
                    is_inline: Some(is_inline),
                });
            }
            None => {
                if live.driver.resolve_inline_bot(username).is_err() {
                    live.driver.session.bots.inline_bot_resolve = Some(InlineBotResolve::Failed {
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
                .and_then(|s| s.bots.inline_bot_resolve.as_ref())
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
            .and_then(|s| s.bots.inline_query.as_ref())
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
                // A new query starts at the top of its results.
                this.composer_ui.inline_results_selected = 0;
                this.composer_ui
                    .inline_scroll
                    .set_offset(gpui_kit::gpui::point(
                        gpui_kit::gpui::px(0.),
                        gpui_kit::gpui::px(0.),
                    ));
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

    /// Bots slice: Up/Down highlight across the shown results.
    /// Returns true when the dropdown consumed the key.
    pub(super) fn step_inline_results(&mut self, delta: i32, cx: &mut Context<Self>) -> bool {
        let count = inline_view(self, cx)
            .map(|view| view.display_order().len())
            .unwrap_or(0);
        if !self.composer_ui.inline_results_open || count == 0 {
            return false;
        }
        self.composer_ui.inline_results_selected = quill::inline_grid::step_selection(
            self.composer_ui.inline_results_selected,
            delta,
            count,
        );
        self.scroll_inline_selection_into_view(cx);
        cx.notify();
        true
    }

    /// Bots slice: Enter with the dropdown open picks the highlighted
    /// result. Returns true when Enter was consumed.
    pub(super) fn pick_inline_result_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(InlineView::Results { results, .. }) = inline_view(self, cx) else {
            return false;
        };
        let order = layout_results(&results).order();
        if !self.composer_ui.inline_results_open || order.is_empty() {
            return false;
        }
        let index = self
            .composer_ui
            .inline_results_selected
            .min(order.len() - 1);
        let result = &results[order[index]];
        self.send_inline_pick(result.query_id, result.id.clone(), window, cx);
        true
    }

    /// Bots slice: tap / Enter pick. Sends via
    /// `sendInlineQueryResultMessage` and clears the composer (the schema
    /// always clears the chat draft on that call).
    pub(super) fn send_inline_pick(
        &mut self,
        query_id: i64,
        result_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(chat_id) = self.open_chat_id() else {
            return;
        };
        // Clear first: the schema clears the chat draft on send, so the
        // composer must never show a stale `@bot query` after the round
        // trip.
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

    /// Bots slice: the inline-results panel above the composer (see
    /// `inline_grid.rs` for the grid and list rows).
    pub(super) fn inline_results_dropdown(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.composer_ui.inline_results_open {
            return None;
        }
        let view = inline_view(self, cx)?;
        self.inline_results_panel(&view, cx)
    }
}
