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
use super::widgets_state::{Identity, Poll, WidgetEntry};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr_with};
use quill::message_widget::storage;
use quill::message_widget::{
    Lifecycle, MAX_CRASHES, PoolEntry, QueryOutcome, WidgetAnswer, WidgetKey, capacity, card_width, initial_height,
    victim,
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
        let root = quill::settings::safe_app_root()?;
        Some(quill::account_data::widget_dir(&root, &self.web_app_account()))
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
            // window-scope: the widgets are the screen's; each entry names its window.
            self.defer_screen(cx, move |this, cx| this.reconcile_widgets(window, cx));
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
        let mut hidden = Vec::new();
        for (key, entry) in &mut self.widgets.entries {
            if entry.window == window && entry.visible && !seen.contains_key(key) {
                hidden.push(*key);
            }
        }
        for key in hidden {
            self.widgets.clock = self.widgets.clock.saturating_add(1);
            let clock = self.widgets.clock;
            if let Some(entry) = self.widgets.entries.get_mut(&key) {
                entry.visible = false;
                entry.hidden_at = clock;
                entry.wants_frames = false;
                if let Some(runtime) = &entry.runtime {
                    runtime.set_visible(false);
                    runtime.trim();
                }
            }
        }
        for (key, width) in seen {
            let Some((content, outgoing)) = self.widget_message(key) else { continue };
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
                let stored = dir.as_deref().map(|dir| storage::load(dir, key)).unwrap_or_default();
                let height = initial_height(fixed_height, stored.height);
                WidgetEntry::new(stored, height)
            });
            entry.window = window;
            entry.view = view.or(entry.view);
            if entry.identity.as_ref() != Some(&identity) {
                // New content: start over.
                entry.runtime = None;
                entry.lifecycle = Lifecycle::Idle;
                entry.crashes = 0;
                entry.polled = None;
                entry.identity = Some(identity);
            }
            let came_back = !entry.visible;
            entry.visible = true;
            entry.hidden_at = 0;
            if (entry.width - width).abs() > 0.5 {
                entry.width = width;
                if let Some(runtime) = &entry.runtime {
                    runtime.set_width(f64::from(width));
                }
            }
            if let Some(runtime) = &entry.runtime {
                if came_back {
                    runtime.set_visible(true);
                }
                runtime.set_theme(theme.0.clone(), theme.1);
            }
            if came_back && has_id && let Poll::Due(at) = entry.poll {
                entry.poll = Poll::Due(at.min(Instant::now()));
            }
            let restart = match &entry.lifecycle {
                Lifecycle::Idle | Lifecycle::Waiting | Lifecycle::Released => true,
                Lifecycle::Stopped { .. } => came_back && entry.crashes < MAX_CRASHES,
                Lifecycle::Loading | Lifecycle::Running | Lifecycle::Failed { .. } => false,
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
            let Some(data) = self.widget_message(key).map(|(content, _)| content.clone()) else { continue };
            let Some(entry) = self.widgets.entries.get_mut(&key) else { continue };
            let public = entry.polled.clone().unwrap_or(data.public_data);
            let json = serde_json::json!({"chatData": data.chat_data, "publicData": public}).to_string();
            if entry.data != json {
                if let Some(runtime) = &entry.runtime {
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
            .filter(|(_, entry)| entry.lifecycle == Lifecycle::Waiting && entry.visible)
            .map(|(key, _)| *key)
            .collect();
        waiting.sort();
        let visible = self.widgets.entries.values().filter(|entry| entry.visible).count();
        let limit = capacity(visible);
        for key in waiting {
            loop {
                let live = self.widgets.entries.values().filter(|e| e.lifecycle.is_live()).count();
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
                        visible: e.visible,
                        hidden_at: e.hidden_at,
                    })
                    .collect();
                let Some(out) = victim(&pool) else { break };
                if let Some(entry) = self.widgets.entries.get_mut(&out) {
                    entry.runtime = None;
                    entry.lifecycle = Lifecycle::Released;
                }
            }
            let live = self.widgets.entries.values().filter(|e| e.lifecycle.is_live()).count();
            if live >= limit {
                continue;
            }
            self.start_widget(key, cx);
        }
    }

    fn start_widget(&mut self, key: WidgetKey, cx: &mut Context<Self>) {
        let Some((content, outgoing)) = self.widget_message(key) else { return };
        let content = content.clone();
        let (theme, background) = self.widget_theme(outgoing, cx);
        let chat_type = self.session().and_then(|s| s.chats.get(&key.chat_id.0)).map(|chat| match chat.kind {
            ChatKind::Private { .. } | ChatKind::Secret { .. } => "private",
            ChatKind::BasicGroup { .. } => "group",
            ChatKind::Supergroup { is_channel: true, .. } => "channel",
            ChatKind::Supergroup { .. } => "supergroup",
            ChatKind::Unknown => "",
        });
        let session_area = self.widgets.session.get(&key).cloned().unwrap_or_default();
        let Some(entry) = self.widgets.entries.get_mut(&key) else { return };
        let options = serde_json::json!({
            "width": entry.width.max(1.0),
            "dpr": 2.0,
            "maxHeight": quill::message_widget::MAX_HEIGHT,
            "visible": entry.visible,
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
        entry.runtime = Some(Runtime::start(Launch {
            html: content.html,
            options: options.to_string(),
            background,
        }));
        entry.lifecycle = Lifecycle::Loading;
        self.watch_widget_runtimes(cx);
    }

    /// The loop that takes what the runtimes left whenever one pokes [`WIDGETS`].
    fn watch_widget_runtimes(&mut self, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.widgets.landing, true) {
            return;
        }
        let mut seen = WIDGETS.epoch();
        // window-scope: frames land on entries, each naming its window and view.
        self.spawn_screen(cx, async move |this, cx| {
            loop {
                WIDGETS.wait_since(seen).await;
                seen = WIDGETS.epoch();
                if this.update(cx, |this, cx| this.land_widgets(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn land_widgets(&mut self, cx: &mut Context<Self>) {
        let keys: Vec<WidgetKey> = self.widgets.entries.keys().copied().collect();
        let mut views = Vec::new();
        let mut all = false;
        for key in keys {
            let Some(entry) = self.widgets.entries.get_mut(&key) else { continue };
            let Some(runtime) = &entry.runtime else { continue };
            let out = runtime.take();
            let mut changed = false;
            if let Some(frame) = out.frame {
                entry.frame = Some((OwnedImage::new(frame.image), frame.width, frame.height));
                changed = true;
            }
            if entry.wants_frames != out.wants_frames || entry.cursor != out.cursor {
                entry.wants_frames = out.wants_frames && entry.visible;
                entry.cursor = out.cursor;
                changed = true;
            }
            for event in out.events {
                // Height and lifecycle move the rows: the whole history redraws.
                all |= matches!(event, RuntimeEvent::Height(_) | RuntimeEvent::Stopped(_) | RuntimeEvent::Failed(_));
                self.widget_event(key, event, cx);
            }
            if changed && let Some(entry) = self.widgets.entries.get(&key) {
                views.push(entry.view);
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
                    entry.lifecycle = Lifecycle::Running;
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
                    entry.runtime = None;
                    entry.wants_frames = false;
                    entry.crashes = entry.crashes.saturating_add(1);
                    entry.lifecycle = if entry.frame.is_some() {
                        Lifecycle::Stopped { reason }
                    } else {
                        Lifecycle::Failed { reason }
                    };
                }
            }
            RuntimeEvent::Link { telegram: true, url } => self.open_link(&url, cx),
            RuntimeEvent::Link { telegram: false, url } => {
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
                let sent = self
                    .live
                    .as_mut()
                    .map(|live| live.driver.send_widget_query(key, token, data.as_bytes()).is_ok())
                    .unwrap_or(false);
                if !sent && let Some(runtime) = self.widgets.entries.get(&key).and_then(|e| e.runtime.as_ref()) {
                    runtime.respond(token, Err(quill::message_widget::QueryFailure::Failed.code()));
                }
            }
            RuntimeEvent::Copy(text) => self.copy_widget_text(text, cx),
            RuntimeEvent::Selection(text) => {
                if let Some(entry) = self.widgets.entries.get_mut(&key) {
                    entry.selection = text;
                }
            }
            RuntimeEvent::Watch(on) => {
                let has_id = self.widget_message(key).is_some_and(|(content, _)| content.widget_id != 0);
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
        self.connection
            .set_note(StatusNote::info(tr_with(keys::TGW_COPIED, &[("text", &text)])));
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
    pub(super) fn land_widget_answers(&mut self, answers: Vec<WidgetAnswer>, cx: &mut Context<Self>) {
        for answer in answers {
            match answer {
                WidgetAnswer::Query { key, token, outcome } => {
                    if let Some(runtime) = self.widgets.entries.get(&key).and_then(|e| e.runtime.as_ref()) {
                        runtime.respond(
                            token,
                            match outcome {
                                QueryOutcome::Answered(data) => Ok(data),
                                QueryOutcome::Failed(failure) => Err(failure.code()),
                            },
                        );
                    }
                }
                WidgetAnswer::PublicData { key, data } => {
                    let Some(entry) = self.widgets.entries.get_mut(&key) else { continue };
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

    /// Polls what is due on screen and sets the timer for the next one.
    fn schedule_widget_polls(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let due: Vec<WidgetKey> = self
            .widgets
            .entries
            .iter()
            .filter(|(_, e)| e.visible && matches!(e.poll, Poll::Due(at) if at <= now))
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
            .filter(|e| e.visible)
            .filter_map(|e| match e.poll {
                Poll::Due(at) => Some(at),
                _ => None,
            })
            .min();
        let Some(next) = next else {
            self.widgets.poll_timer = None;
            return;
        };
        if self.widgets.poll_timer.as_ref().is_some_and(|(at, _)| *at <= next) {
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
    pub(super) fn save_widget_storage(&mut self) {
        let Some(dir) = self.widget_dir() else { return };
        for (key, entry) in &mut self.widgets.entries {
            if std::mem::take(&mut entry.unsaved)
                && let Err(err) = storage::save(&dir, *key, &entry.stored)
            {
                eprintln!("quill: could not save a widget's storage: {err}");
            }
        }
    }

    /// The widget's theme (`WidgetTheme` JSON) on its bubble, and the
    /// bubble's color, from Quill's theme (the mapping Telegram Desktop
    /// uses, every color flattened onto the bubble).
    pub(super) fn widget_theme(&self, outgoing: bool, cx: &App) -> (String, [u8; 3]) {
        use super::chat_theme as t;
        let look = self.bubble_look(cx);
        let bg = if outgoing { look.outgoing_fill() } else { t::bg_bubble_incoming() };
        let text: Rgba = if outgoing { look.outgoing_text().into() } else { t::text_primary() };
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
            (t::text_muted(), t::accent(), t::accent_strong(), t::text_on_fill())
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
