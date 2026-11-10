//! Driving the message widgets on screen: which ones run (visible ones, at
//! most [`MAX_LIVE`] beyond that, the oldest hidden released first), what
//! their runtimes said (frames, heights, links, storage, queries, copies,
//! public data polls) and TDLib's answers. Every change lands in
//! `WidgetsUi` and the bubble draws from it (`ui/widget_bubble.rs`).
//! Telegram Desktop's behaviour, described: docs/decisions/message-widgets.md.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::owned_image::OwnedImage;
use super::status_note::StatusNote;
use super::widget_runtime::{Launch, Runtime, RuntimeEvent, WIDGETS};
use super::widgets_state::{
    Identity, Poll, PresenceChange, Showing, WidgetEntry, WidgetStorage, write_pending,
};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr_with};
use quill::message_widget::storage;
use quill::message_widget::{
    Lifecycle, MAX_CRASHES, PoolEntry, QueryOutcome, WidgetAnswer, WidgetKey, capacity, card_width,
    initial_height, victim,
};

use quill::telegram::envelope::{ChatKind, MessageContent, WidgetContent};
use std::time::{Duration, Instant};

/// Storage writes wait this long for more (Telegram Desktop's debounce).
const SAVE_AFTER: Duration = Duration::from_secs(1);

impl super::on_screen::Farewell for super::widgets_state::WidgetsUi {
    fn farewell(app: &mut QuillApp, _window: &mut Window, _cx: &mut Context<QuillApp>) {
        app.save_widget_storage();
    }
}

impl QuillApp {
    /// The widget message `key` and its content, from the session.
    fn widget_message(&self, key: WidgetKey) -> Option<(&WidgetContent, bool)> {
        let message = self
            .session()?
            .histories
            .get(&key.chat_id.0)?
            .messages
            .get(&key.message_id.0)?;
        match &message.content {
            MessageContent::Widget(widget) => Some((widget, message.is_outgoing)),
            _ => None,
        }
    }

    fn widget_dir(&self) -> Option<std::path::PathBuf> {
        // No account, no folder: never `widgets/default`. A logged-out
        // account's folder is gone and stays gone.
        self.session()?;
        if self.widgets.storage == WidgetStorage::Forgotten {
            return None;
        }
        let root = quill::settings::safe_app_root()?;
        Some(quill::account_data::widget_dir(
            &root,
            &self.web_app_account(),
        ))
    }

    /// A history pass of the window being built begins: its widget rows
    /// note themselves while they render, and once the pass is drawn the
    /// widgets it did not draw go off screen ([`Self::reconcile_widgets`]).
    pub(super) fn begin_widget_pass(&self, view: Option<EntityId>, cx: &mut Context<Self>) {
        let window = self.windows.scope;
        let mut passes = self.widgets.passes.borrow_mut();
        passes.seen.insert(window, Default::default());
        passes.views.insert(window, view);
        if passes.queued.insert(window) {
            // The pass's window's state in place: its chat's theme colors
            // the widgets it shows.
            self.defer_screen(cx, move |this, cx| {
                let ran =
                    this.in_window_scope(window, cx, |this, cx| this.reconcile_widgets(window, cx));
                if ran.is_none() {
                    this.widgets.passes.borrow_mut().queued.remove(&window);
                }
            });
        }
    }

    /// A widget row rendered in this pass at `width` CSS px.
    pub(super) fn note_widget(&self, key: WidgetKey, width: f32) {
        let window = self.windows.scope;
        self.widgets
            .passes
            .borrow_mut()
            .seen
            .entry(window)
            .or_default()
            .insert(key, width);
    }

    /// A frame of the window `window` (`None`: the main one) starts: the
    /// animating widgets it owns take one display frame each, and ask for
    /// the next. Each widget ticks in its owner's frames, once a frame; its
    /// frames redraw every window that shows it ([`Self::land_widgets`]).
    pub(super) fn widgets_frame_start(&self, window: Option<ChatId>, cx: &mut Context<Self>) {
        let mut views = Vec::new();
        for entry in self.widgets.entries.values() {
            if entry.presence.owner() != Some(window) || !entry.wants_frames {
                continue;
            }
            if let Some(runtime) = entry.lifecycle.runtime() {
                runtime.tick(super::widget_runtime::clock_ms());
                views.extend(entry.presence.views());
            }
        }
        views.sort_unstable();
        views.dedup();
        for view in views {
            self.request_animation_tick_for(view, 60, cx);
        }
    }

    /// The widget message `key` was deleted: its entry, runtime, session
    /// and stored data go.
    fn forget_widget(&mut self, key: WidgetKey) {
        self.widgets.entries.remove(&key);
        self.widgets.session.remove(&key);
        if let Some(dir) = self.widget_dir() {
            storage::remove(&dir, key);
        }
    }

    /// Off-screen widgets kept with their last frame (a few MB each), so
    /// scrolling back shows them at once; older ones are dropped and start
    /// over from their stored data when they come back.
    const KEEP_HIDDEN: usize = 16;

    /// Drops the hidden widgets past [`Self::KEEP_HIDDEN`], oldest first,
    /// writing what they still had to keep.
    fn prune_hidden_widgets(&mut self) {
        let mut hidden: Vec<(u64, WidgetKey)> = self
            .widgets
            .entries
            .iter()
            .filter(|(_, entry)| !entry.presence.is_shown())
            .map(|(key, entry)| (entry.presence.hidden_since(), *key))
            .collect();
        let Some(excess) = hidden.len().checked_sub(Self::KEEP_HIDDEN) else {
            return;
        };
        if excess == 0 {
            return;
        }
        hidden.sort_unstable();
        let dir = self.widget_dir();
        for (_, key) in hidden.into_iter().take(excess) {
            let Some(entry) = self.widgets.entries.remove(&key) else {
                continue;
            };
            let mut entry = entry;
            if let Some(dir) = &dir {
                write_pending(&mut entry.unsaved, || {
                    storage::save(dir, key, &entry.stored)
                });
            }
            if entry.unsaved && dir.is_some() {
                // Pruned with its storage unwritten, it would be lost: it
                // stays, and the next prune or save writes it again.
                self.widgets.entries.insert(key, entry);
            }
        }
    }

    /// `window` (`None`: the main one) stopped drawing the widget `key`:
    /// another window that draws it takes it over at its width, or it goes
    /// off screen, stops drawing and drops what it can rebuild.
    fn widget_left(&mut self, key: WidgetKey, window: Option<ChatId>) {
        let clock = self.widgets.clock.saturating_add(1);
        let Some(entry) = self.widgets.entries.get_mut(&key) else {
            return;
        };
        let change = entry.presence.leave(window, clock);
        match change {
            PresenceChange::Hid => {
                if let Some(runtime) = entry.lifecycle.runtime() {
                    runtime.set_visible(false);
                    runtime.trim();
                }
            }
            PresenceChange::Width(width) => {
                if let Some(runtime) = entry.lifecycle.runtime() {
                    runtime.set_width(f64::from(width));
                }
            }
            PresenceChange::Same | PresenceChange::CameBack => {}
        }
        if change == PresenceChange::Hid {
            self.widgets.clock = clock;
        }
    }

    /// The window `window` (`None`: the main one) closed or shows no
    /// history: it draws none of its widgets now, not at the next pass of
    /// some other window.
    pub(super) fn widgets_left_window(&mut self, window: Option<ChatId>) {
        {
            let mut passes = self.widgets.passes.borrow_mut();
            passes.seen.remove(&window);
            passes.views.remove(&window);
            passes.queued.remove(&window);
        }
        let left: Vec<WidgetKey> = self
            .widgets
            .entries
            .iter()
            .filter(|(_, entry)| entry.presence.drawn_in(window))
            .map(|(key, _)| *key)
            .collect();
        if left.is_empty() {
            return;
        }
        for key in left {
            self.widget_left(key, window);
        }
        self.prune_hidden_widgets();
    }

    /// The pass of `window` was drawn: what it showed is on screen, what it
    /// did not show of that window's widgets is not.
    fn reconcile_widgets(&mut self, window: Option<ChatId>, cx: &mut Context<Self>) {
        let (seen, view) = {
            let mut passes = self.widgets.passes.borrow_mut();
            passes.queued.remove(&window);
            let view = passes.views.get(&window).copied().flatten();
            (passes.seen.get(&window).cloned().unwrap_or_default(), view)
        };
        // Only a stopped widget starting again changes what the rows show.
        let mut redraw = false;
        let dir = self.widget_dir();
        let left: Vec<WidgetKey> = self
            .widgets
            .entries
            .iter()
            .filter(|(key, entry)| entry.presence.drawn_in(window) && !seen.contains_key(*key))
            .map(|(key, _)| *key)
            .collect();
        for key in left {
            self.widget_left(key, window);
        }
        self.prune_hidden_widgets();
        for (key, width) in seen {
            let Some((content, outgoing)) = self.widget_message(key) else {
                continue;
            };
            let identity = Identity {
                html: content.html.clone(),
                is_copy: content.is_copy,
                can_send_queries: content.can_send_queries,
                can_send_background_queries: content.can_send_background_queries,
            };
            let fixed_height = content.height;
            let has_id = content.widget_id != 0;
            let theme = self.widget_theme(outgoing, cx);
            let entry = self.widgets.entries.entry(key).or_insert_with(|| {
                let stored = dir
                    .as_deref()
                    .map(|dir| storage::load(dir, key))
                    .unwrap_or_default();
                let height = initial_height(fixed_height, stored.height);
                WidgetEntry::new(stored, height)
            });
            if entry.identity.as_ref() != Some(&identity) {
                // New content: start over.
                entry.lifecycle = Lifecycle::Idle;
                entry.crashes = 0;
                entry.polled = None;
                entry.identity = Some(identity);
            }
            let change = entry.presence.show(window, Showing { width, view });
            let came_back = change == PresenceChange::CameBack;
            if came_back {
                // Its links ask from the window that shows it now.
                entry.input_window = window;
            }
            if let PresenceChange::Width(width) = change
                && let Some(runtime) = entry.lifecycle.runtime()
            {
                runtime.set_width(f64::from(width));
            }
            if let Some(runtime) = entry.lifecycle.runtime() {
                if came_back {
                    runtime.set_visible(true);
                }
                // Each set re-sanitises the widget's CSS: only changes go,
                // and only from the owner, so two windows never trade it.
                if entry.presence.owner() == Some(window) && entry.theme.as_ref() != Some(&theme) {
                    runtime.set_theme(theme.0.clone(), theme.1);
                    entry.theme = Some(theme.clone());
                }
            }
            if came_back
                && has_id
                && let Poll::Due(at) = entry.poll
            {
                entry.poll = Poll::Due(at.min(Instant::now()));
            }
            let restart = match &entry.lifecycle {
                Lifecycle::Idle | Lifecycle::Waiting | Lifecycle::Released => true,
                Lifecycle::Stopped { .. } => came_back && entry.crashes < MAX_CRASHES,
                Lifecycle::Loading(_) | Lifecycle::Running(_) | Lifecycle::Failed { .. } => false,
            };
            if restart {
                redraw |= entry.lifecycle.is_stopped();
                entry.lifecycle = Lifecycle::Waiting;
            }
        }
        self.sync_widget_data();
        self.start_waiting_widgets(cx);
        self.schedule_widget_polls(cx);
        if redraw {
            cx.notify();
        }
    }

    /// New `chat_data` / `public_data` reach the runtimes that run.
    fn sync_widget_data(&mut self) {
        let keys: Vec<WidgetKey> = self.widgets.entries.keys().copied().collect();
        for key in keys {
            let Some(data) = self.widget_message(key).map(|(content, _)| content.clone()) else {
                continue;
            };
            let Some(entry) = self.widgets.entries.get_mut(&key) else {
                continue;
            };
            let public = entry.polled.clone().unwrap_or(data.public_data);
            let json =
                serde_json::json!({"chatData": data.chat_data, "publicData": public}).to_string();
            if entry.data != json {
                if let Some(runtime) = entry.lifecycle.runtime() {
                    runtime.set_data(json.clone());
                }
                entry.data = json;
            }
        }
    }

    /// Starts the widgets waiting for a sandbox, releasing hidden ones to stay in the limit.
    fn start_waiting_widgets(&mut self, cx: &mut Context<Self>) {
        let mut waiting: Vec<WidgetKey> = self
            .widgets
            .entries
            .iter()
            .filter(|(_, entry)| {
                matches!(entry.lifecycle, Lifecycle::Waiting) && entry.presence.is_shown()
            })
            .map(|(key, _)| *key)
            .collect();
        waiting.sort();
        let visible = self
            .widgets
            .entries
            .values()
            .filter(|entry| entry.presence.is_shown())
            .count();
        let limit = capacity(visible);
        for key in waiting {
            loop {
                let live = self
                    .widgets
                    .entries
                    .values()
                    .filter(|e| e.lifecycle.is_live())
                    .count();
                if live < limit {
                    break;
                }
                let pool: Vec<PoolEntry> = self
                    .widgets
                    .entries
                    .iter()
                    .map(|(key, e)| PoolEntry {
                        key: *key,
                        live: e.lifecycle.is_live(),
                        visible: e.presence.is_shown(),
                        hidden_at: e.presence.hidden_since(),
                    })
                    .collect();
                let Some(out) = victim(&pool) else { break };
                if let Some(entry) = self.widgets.entries.get_mut(&out) {
                    entry.lifecycle = Lifecycle::Released;
                }
            }
            let live = self
                .widgets
                .entries
                .values()
                .filter(|e| e.lifecycle.is_live())
                .count();
            if live >= limit {
                continue;
            }
            self.start_widget(key, cx);
        }
    }

    fn start_widget(&mut self, key: WidgetKey, cx: &mut Context<Self>) {
        // A logged-out account's widgets wait for the screen to go.
        if self.widgets.storage == WidgetStorage::Forgotten {
            return;
        }
        let Some((content, outgoing)) = self.widget_message(key) else {
            return;
        };
        let content = content.clone();
        let (theme, background) = self.widget_theme(outgoing, cx);
        let chat_type = self
            .session()
            .and_then(|s| s.chats.get(&key.chat_id.0))
            .map(|chat| match chat.kind {
                ChatKind::Private { .. } | ChatKind::Secret { .. } => "private",
                ChatKind::BasicGroup { .. } => "group",
                ChatKind::Supergroup {
                    is_channel: true, ..
                } => "channel",
                ChatKind::Supergroup { .. } => "supergroup",
                ChatKind::Unknown => "",
            });
        let session_area = self.widgets.session.get(&key).cloned().unwrap_or_default();
        // A frame with device pixels the window shows; 2× until one drew.
        let scale = self.widgets.scale;
        let dpr = if scale > 0.0 {
            scale.clamp(1.0, 4.0)
        } else {
            2.0
        };
        let Some(entry) = self.widgets.entries.get_mut(&key) else {
            return;
        };
        let options = serde_json::json!({
            "width": entry.presence.width().max(1.0),
            "dpr": dpr,
            "maxHeight": quill::message_widget::MAX_HEIGHT,
            "visible": entry.presence.is_shown(),
            "locale": quill::lang::current_pack_id(),
            "theme": serde_json::from_str::<serde_json::Value>(&theme).unwrap_or_default(),
            "storage": {"local": entry.stored.local, "session": session_area},
            "data": {"chatData": content.chat_data, "publicData": entry.polled.clone().unwrap_or(content.public_data)},
            "context": {
                "isCopy": content.is_copy,
                "canQuery": content.can_send_queries,
                "backgroundQuery": content.can_send_background_queries,
                // A copy does not say where it is (Telegram Desktop).
                "chatType": if content.is_copy { "" } else { chat_type.unwrap_or("") },
            },
        });
        entry.queries.clear();
        entry.theme = Some((theme, background));
        entry.lifecycle = Lifecycle::Loading(Runtime::start(Launch {
            html: content.html,
            options: options.to_string(),
            background,
        }));
        self.watch_widget_runtimes(cx);
    }

    /// The loop that takes what the runtimes left whenever one pokes [`WIDGETS`].
    /// It is held, not detached: it ends with the screen that owns it.
    fn watch_widget_runtimes(&mut self, cx: &mut Context<Self>) {
        if self.widgets.lander.is_some() {
            return;
        }
        let mut seen = WIDGETS.epoch();
        // window-scope: frames land on entries, each naming the windows that draw it.
        let lander = self.spawn_screen(cx, async move |this, cx| {
            loop {
                WIDGETS.wait_since(seen).await;
                seen = WIDGETS.epoch();
                if this.update(cx, |this, cx| this.land_widgets(cx)).is_err() {
                    break;
                }
            }
        });
        self.widgets.lander = Some(lander);
    }

    fn land_widgets(&mut self, cx: &mut Context<Self>) {
        let keys: Vec<WidgetKey> = self.widgets.entries.keys().copied().collect();
        let mut views = Vec::new();
        let mut all = false;
        for key in keys {
            let Some(entry) = self.widgets.entries.get_mut(&key) else {
                continue;
            };
            let Some(runtime) = entry.lifecycle.runtime() else {
                continue;
            };
            let out = runtime.take();
            let mut changed = false;
            if let Some(frame) = out.frame {
                entry.frame = Some((OwnedImage::new(frame.image), frame.width, frame.height));
                changed = true;
            }
            if entry.wants_frames != out.wants_frames || entry.cursor != out.cursor {
                entry.wants_frames = out.wants_frames;
                entry.cursor = out.cursor;
                changed = true;
            }
            // Off screen nothing redraws for it (round 18 C #2, #3): its
            // views list is empty, and its rows are not drawn.
            let shown = entry.presence.is_shown();
            for event in out.events {
                // Height and lifecycle move the rows: the whole history redraws.
                all |= shown
                    && matches!(
                        event,
                        RuntimeEvent::Height(_)
                            | RuntimeEvent::Stopped(_)
                            | RuntimeEvent::Failed(_)
                    );
                // The window it was pressed in answers it: its link box,
                // its toast (the owner until a press).
                let window = self
                    .widgets
                    .entries
                    .get(&key)
                    .and_then(WidgetEntry::asking_window);
                let mut event = Some(event);
                self.in_window_scope(window, cx, |this, cx| {
                    if let Some(event) = event.take() {
                        this.widget_event(key, event, cx);
                    }
                });
                if let Some(event) = event {
                    self.widget_event(key, event, cx);
                }
            }
            if changed && let Some(entry) = self.widgets.entries.get(&key) {
                // Every window that draws it shows the new frame.
                views.extend(entry.presence.views());
            }
        }
        if all {
            cx.notify();
        } else {
            let app: &mut App = cx;
            for view in views.into_iter().flatten() {
                app.notify(view);
            }
        }
    }

    fn widget_event(&mut self, key: WidgetKey, event: RuntimeEvent, cx: &mut Context<Self>) {
        match event {
            RuntimeEvent::Started => {
                if let Some(entry) = self.widgets.entries.get_mut(&key) {
                    entry.lifecycle =
                        std::mem::replace(&mut entry.lifecycle, Lifecycle::Idle).started();
                    entry.crashes = 0;
                }
            }
            RuntimeEvent::Height(height) => {
                if let Some(entry) = self.widgets.entries.get_mut(&key) {
                    entry.height = height;
                    entry.stored.height = Some(height);
                    entry.unsaved = true;
                }
                self.schedule_widget_save(cx);
            }
            RuntimeEvent::Stopped(reason) | RuntimeEvent::Failed(reason) => {
                if let Some(entry) = self.widgets.entries.get_mut(&key) {
                    entry.wants_frames = false;
                    entry.crashes = entry.crashes.saturating_add(1);
                    entry.lifecycle = if entry.frame.is_some() {
                        Lifecycle::Stopped { reason }
                    } else {
                        Lifecycle::Failed { reason }
                    };
                }
            }
            RuntimeEvent::Link {
                telegram: true,
                url,
            } => {
                // The window it was pressed in asked: what the link opens shows there.
                let window = self
                    .widgets
                    .entries
                    .get(&key)
                    .and_then(WidgetEntry::asking_window);
                self.as_asked_by(window, |this| this.open_link(&url, cx));
            }
            RuntimeEvent::Link {
                telegram: false,
                url,
            } => {
                // A widget's link always asks first, as in Telegram Desktop.
                self.message_ui.open_link_confirm = Some(super::entity_links::OpenLinkConfirm {
                    shown: url.clone(),
                    url,
                    suspicious: Vec::new(),
                    from_poll_option: false,
                });
                cx.notify();
            }
            RuntimeEvent::Storage { area, values } => {
                if area == "session" {
                    self.widgets.session.insert(key, values);
                } else if let Some(entry) = self.widgets.entries.get_mut(&key) {
                    entry.stored.local = values;
                    entry.unsaved = true;
                    self.schedule_widget_save(cx);
                }
            }
            RuntimeEvent::Gesture { hold } => {
                if let Some(entry) = self.widgets.entries.get_mut(&key) {
                    entry.hold = hold;
                }
            }
            RuntimeEvent::Request { token, data } => {
                let ours = super::widgets_state::next_query_token();
                let Some(entry) = self.widgets.entries.get_mut(&key) else {
                    return;
                };
                entry.queries.insert(ours, token);
                let sent = self
                    .live
                    .as_mut()
                    .map(|live| {
                        live.driver
                            .send_widget_query(key, ours, data.as_bytes())
                            .is_ok()
                    })
                    .unwrap_or(false);
                if !sent {
                    self.answer_widget_query(
                        key,
                        ours,
                        Err(quill::message_widget::QueryFailure::Failed.code()),
                    );
                }
            }
            RuntimeEvent::Copy(text) => self.copy_widget_text(text, cx),
            RuntimeEvent::Selection(text) => {
                if let Some(entry) = self.widgets.entries.get_mut(&key) {
                    entry.selection = text;
                }
            }
            RuntimeEvent::Watch(on) => {
                let has_id = self
                    .widget_message(key)
                    .is_some_and(|(content, _)| content.widget_id != 0);
                if let Some(entry) = self.widgets.entries.get_mut(&key) {
                    entry.poll = match (on && has_id, entry.poll) {
                        (false, _) => Poll::Off,
                        (true, Poll::Off) => Poll::Due(Instant::now()),
                        (true, poll) => poll,
                    };
                }
                self.schedule_widget_polls(cx);
            }
        }
    }

    /// `copyText`: the clipboard, and a toast that says what was copied.
    fn copy_widget_text(&mut self, text: String, cx: &mut Context<Self>) {
        if text.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        self.set_note(StatusNote::info(tr_with(
            keys::TGW_COPIED,
            &[("text", &text)],
        )));
        cx.notify();
    }

    /// The selected text of a widget, for the message menu's copy.
    pub(super) fn widget_selection(&self, key: WidgetKey) -> Option<&str> {
        self.widgets
            .entries
            .get(&key)
            .map(|entry| entry.selection.as_str())
            .filter(|text| !text.is_empty())
    }

    /// TDLib's answers to the widgets' queries and polls.
    pub(super) fn land_widget_answers(
        &mut self,
        answers: Vec<WidgetAnswer>,
        cx: &mut Context<Self>,
    ) {
        for answer in answers {
            match answer {
                WidgetAnswer::Query {
                    key,
                    token,
                    outcome,
                } => self.answer_widget_query(
                    key,
                    token,
                    match outcome {
                        QueryOutcome::Answered(data) => Ok(data),
                        QueryOutcome::Failed(failure) => Err(failure.code()),
                    },
                ),
                WidgetAnswer::Deleted { key } => self.forget_widget(key),
                WidgetAnswer::PublicData { key, data } => {
                    let Some(entry) = self.widgets.entries.get_mut(&key) else {
                        continue;
                    };
                    let period = match data {
                        Some((data, period)) => {
                            entry.polled = Some(data);
                            period
                        }
                        None => quill::message_widget::DEFAULT_POLL_PERIOD,
                    };
                    if entry.poll == Poll::Asking {
                        entry.poll = Poll::Due(Instant::now() + period);
                    }
                }
            }
        }
        self.sync_widget_data();
        self.schedule_widget_polls(cx);
    }

    /// The answer to Quill's query `token`, for the runtime that asked:
    /// dropped when that runtime is gone (a restart starts a new list).
    fn answer_widget_query(&mut self, key: WidgetKey, token: u64, outcome: Result<String, i32>) {
        let Some(entry) = self.widgets.entries.get_mut(&key) else {
            return;
        };
        let Some(theirs) = entry.queries.remove(&token) else {
            return;
        };
        if let Some(runtime) = entry.lifecycle.runtime() {
            runtime.respond(theirs, outcome);
        }
    }

    /// Polls what is due on screen and sets the timer for the next one.
    fn schedule_widget_polls(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let due: Vec<WidgetKey> = self
            .widgets
            .entries
            .iter()
            .filter(|(_, e)| e.presence.is_shown() && matches!(e.poll, Poll::Due(at) if at <= now))
            .map(|(key, _)| *key)
            .collect();
        for key in due {
            let asked = self
                .live
                .as_mut()
                .is_some_and(|live| live.driver.poll_widget_public_data(key).is_ok());
            if let Some(entry) = self.widgets.entries.get_mut(&key) {
                entry.poll = if asked {
                    Poll::Asking
                } else {
                    Poll::Due(now + quill::message_widget::DEFAULT_POLL_PERIOD)
                };
            }
        }
        let next = self
            .widgets
            .entries
            .values()
            .filter(|e| e.presence.is_shown())
            .filter_map(|e| match e.poll {
                Poll::Due(at) => Some(at),
                _ => None,
            })
            .min();
        let Some(next) = next else {
            self.widgets.poll_timer = None;
            return;
        };
        if self
            .widgets
            .poll_timer
            .as_ref()
            .is_some_and(|(at, _)| *at <= next)
        {
            return;
        }
        let wait = next.saturating_duration_since(now);
        let task = self.spawn_screen(cx, async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |this, cx| {
                this.widgets.poll_timer = None;
                this.schedule_widget_polls(cx);
            });
        });
        self.widgets.poll_timer = Some((next, task));
    }

    fn schedule_widget_save(&mut self, cx: &mut Context<Self>) {
        if self.widgets.save_timer.is_some() {
            return;
        }
        let task = self.spawn_screen(cx, async move |this, cx| {
            cx.background_executor().timer(SAVE_AFTER).await;
            let _ = this.update(cx, |this, _| {
                this.widgets.save_timer = None;
                this.save_widget_storage();
            });
        });
        self.widgets.save_timer = Some(task);
    }

    /// Writes the storage of every widget that changed.
    /// `account` logged out and its widget folder went: what its widgets
    /// still had to write goes nowhere (the account's goodbye would write it
    /// back), and `sessionStorage` goes with them. The runtimes go too: a
    /// live one would report storage or a height and write the folder back.
    pub(super) fn forget_widget_storage(&mut self, account: &quill::ids::AccountKey) {
        if self.web_app_account() != account.to_string() {
            return;
        }
        self.widgets.storage = WidgetStorage::Forgotten;
        self.widgets.entries.clear();
        self.widgets.session.clear();
        self.widgets.save_timer = None;
        self.widgets.poll_timer = None;
    }

    pub(super) fn save_widget_storage(&mut self) {
        let Some(dir) = self.widget_dir() else { return };
        for (key, entry) in &mut self.widgets.entries {
            write_pending(&mut entry.unsaved, || {
                storage::save(&dir, *key, &entry.stored)
            });
        }
    }

    /// The widget's theme (`WidgetTheme` JSON) on its bubble, and the
    /// bubble's color, from Quill's theme (the mapping Telegram Desktop
    /// uses, every color flattened onto the bubble).
    pub(super) fn widget_theme(&self, outgoing: bool, cx: &App) -> (String, [u8; 3]) {
        use super::chat_theme as t;
        let look = self.bubble_look(cx);
        let bg = if outgoing {
            look.outgoing_fill()
        } else {
            t::bg_bubble_incoming()
        };
        let text: Rgba = if outgoing {
            look.outgoing_text().into()
        } else {
            t::text_primary()
        };
        let flat = |c: Rgba, alpha: f32| -> Rgba {
            let a = c.a * alpha;
            Rgba {
                r: c.r * a + bg.r * (1.0 - a),
                g: c.g * a + bg.g * (1.0 - a),
                b: c.b * a + bg.b * (1.0 - a),
                a: 1.0,
            }
        };
        let hex = |c: Rgba| {
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            format!("#{:02x}{:02x}{:02x}", byte(c.r), byte(c.g), byte(c.b))
        };
        let (hint, accent, button, button_text) = if outgoing {
            (flat(text, 0.7), text, text, bg)
        } else {
            (
                t::text_muted(),
                t::accent(),
                t::accent_strong(),
                t::text_on_fill(),
            )
        };
        let luminance = |c: Rgba| 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
        let dark = luminance(bg) < 0.45;
        let font = f32::from(look.font).clamp(8.0, 64.0);
        let theme = serde_json::json!({
            "colorScheme": if dark { "dark" } else { "light" },
            "params": {
                "bg_color": hex(bg),
                "text_color": hex(flat(text, 1.0)),
                "hint_color": hex(flat(hint, 1.0)),
                "link_color": hex(flat(accent, 1.0)),
                "button_color": hex(flat(button, 1.0)),
                "button_text_color": hex(flat(button_text, 1.0)),
                "secondary_bg_color": hex(flat(accent, 0.12)),
                "accent_text_color": hex(flat(accent, 1.0)),
                "subtitle_text_color": hex(flat(hint, 1.0)),
                "destructive_text_color": hex(flat(t::danger(), 1.0)),
                "section_bg_color": hex(flat(t::bg_subtle(), 1.0)),
                "section_header_text_color": hex(flat(accent, 1.0)),
                "section_separator_color": hex(flat(hint, 0.3)),
                "header_bg_color": hex(flat(t::bg_subtle(), 1.0)),
                "bottom_bar_bg_color": hex(flat(t::bg_subtle(), 1.0)),
            },
            "text": {"fontSize": font, "lineHeight": 1.35},
        });
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        (theme.to_string(), [byte(bg.r), byte(bg.g), byte(bg.b)])
    }

    /// The card width of a widget in a bubble at most `max` CSS px wide.
    pub(super) fn widget_card_width(content: &WidgetContent, max: f32) -> f32 {
        card_width(content.width, max)
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "widgets_lock_tests.rs"]
mod lock_tests;
