//! "Swipe to the next unread channel", the UI half (`quill::pull_next`,
//! tdesktop `HistoryView::PullToNextChannel`).
//!
//! A capture-phase wheel listener over the history feeds trackpad scrolls
//! to the machine; while it pulls, the history rises and the indicator
//! (capsule and arrow, then the next chat's photo and name) fills the room
//! under it, and a note covers the composer. Frames come from the frame
//! clock. Letting go on a ready pull preloads and opens the next unread
//! channel (or forum topic), which slides in from the bottom.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::pull_next::{
    self as metrics, HintState, IndicatorLayout, IndicatorState, Mode, Phase, PullMachine, Step,
};
use std::cell::Cell;
use std::time::{Duration, Instant};

/// tdesktop `SlideAnimation` with `FromBottom`: `st::slideDuration * 2`,
/// `anim::easeOutQuint`.
const SLIDE_MS: f32 = 480.;

thread_local! {
    /// The chat the main window slides in from the bottom, and since when.
    static SLIDE: Cell<Option<(ChatId, Option<i32>, Instant)>> = const { Cell::new(None) };
}

/// What the pull in the shown history leads to (`setHistory` /
/// `setTopic`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Target {
    chat: ChatId,
    topic: Option<i32>,
    mode: Mode,
}

/// The pull's state, per history view.
#[derive(Default)]
pub(crate) struct PullUi {
    machine: PullMachine,
    epoch: Option<Instant>,
    target: Option<Target>,
    next_chat: Option<ChatId>,
    next_topic: Option<i32>,
    /// The window the gesture happened in and its chat-window scope: the
    /// jump runs there.
    window: Option<(AnyWindowHandle, Option<ChatId>)>,
    jump: Option<Task<()>>,
}

impl PullUi {
    fn now(&mut self) -> u64 {
        self.epoch
            .get_or_insert_with(Instant::now)
            .elapsed()
            .as_millis() as u64
    }

    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn machine_for_tests(&self) -> &PullMachine {
        &self.machine
    }

    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn next_chat_for_tests(&self) -> Option<ChatId> {
        self.next_chat
    }
}

/// tdesktop `base::Platform::Haptic`: a generic tap on a Force Touch
/// trackpad. Other platforms have no trackpad haptics to ask for.
fn haptic() {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{
            NSHapticFeedbackManager, NSHapticFeedbackPattern, NSHapticFeedbackPerformanceTime,
            NSHapticFeedbackPerformer,
        };
        NSHapticFeedbackManager::defaultPerformer().performFeedbackPattern_performanceTime(
            NSHapticFeedbackPattern::Generic,
            NSHapticFeedbackPerformanceTime::Default,
        );
    }
}

impl QuillApp {
    /// What the shown history's pull leads to: the next channel from a
    /// chat's own history, the next topic from a forum topic. Reply
    /// threads and Saved Messages views have none.
    fn pull_target(&self) -> Option<Target> {
        let chat = self.viewed_chat()?;
        let session = self.session()?;
        if self.thread_active()
            || session.threads.saved.sublist.is_some()
            || session.threads.saved.tag_search.is_some()
        {
            return None;
        }
        Some(match self.viewed_topic() {
            Some(topic) => Target {
                chat,
                topic: Some(topic),
                mode: Mode::Topic,
            },
            None => Target {
                chat,
                topic: None,
                mode: Mode::History,
            },
        })
    }

    /// The list shows its newest message and nothing newer is left to
    /// load (`PullToNextChannel::atBottom`).
    fn pull_at_bottom(&self, target: Target, cx: &App) -> bool {
        if self.history.scroller.read(cx).is_scrolled_up() {
            return false;
        }
        match target.mode {
            Mode::History => self
                .session()
                .is_none_or(|s| !s.history_has_newer(target.chat)),
            // A topic's history opens at its newest messages and pages
            // older only.
            Mode::Topic => true,
        }
    }

    /// `PullToNextChannel::active`.
    fn pull_active(&self, target: Target, cx: &App) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        if !session.settings.media_prefs.pull_to_next_channel || !self.pull_at_bottom(target, cx) {
            return false;
        }
        match target.mode {
            Mode::History => {
                session
                    .chats
                    .get(&target.chat.0)
                    .is_some_and(|chat| chat.is_channel())
                    && !quill::pull_next::sponsored_unshown(session, target.chat, Instant::now())
            }
            Mode::Topic => true,
        }
    }

    /// The machine's `active` input: in a topic, nothing starts before the
    /// forum's topics are known (tdesktop asks for them and waits).
    fn pull_can_start(&mut self, target: Target, cx: &App) -> bool {
        let active = self.pull_active(target, cx);
        if !active || target.mode != Mode::Topic || self.history.pull.machine.pulling() {
            return active;
        }
        let loaded = self
            .session()
            .is_some_and(|s| s.threads.forum_topics.contains_key(&target.chat.0));
        if !loaded {
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.request_forum_topics(target.chat);
            }
            return false;
        }
        true
    }

    /// Keep the pull on the history shown now: another chat or topic
    /// starts over (`setHistory` / `setTopic` → `reset(instant)`).
    fn pull_sync(&mut self) {
        let target = self.pull_target();
        if self.history.pull.target != target {
            let now = self.history.pull.now();
            let pull = &mut self.history.pull;
            pull.target = target;
            pull.machine.reset_all(now);
            pull.next_chat = None;
            pull.next_topic = None;
            pull.jump = None;
        }
    }

    /// One precise wheel event over the history. Returns whether the list
    /// must not see it.
    fn pull_wheel(
        &mut self,
        event: &ScrollEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // Trackpads only: a mouse wheel has no gesture to follow, and a
        // platform without phases never starts one.
        let ScrollDelta::Pixels(delta) = event.delta else {
            return false;
        };
        self.pull_sync();
        let Some(target) = self.history.pull.target else {
            return false;
        };
        let phase = match event.touch_phase {
            TouchPhase::Started => Phase::Started,
            TouchPhase::Moved => Phase::Moved,
            TouchPhase::Ended => Phase::Ended,
            TouchPhase::Cancelled => Phase::Cancelled,
        };
        // Toward newer messages; a mostly sideways gesture is not a pull.
        let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
        let delta_down = if dx.abs() >= dy.abs() { 0. } else { -dy };
        let at_bottom = self.pull_at_bottom(target, cx);
        let active = self.pull_can_start(target, cx);
        self.history.pull.window = Some((window.window_handle(), self.windows.scope));
        let now = self.history.pull.now();
        let step = self
            .history
            .pull
            .machine
            .wheel(phase, delta_down, at_bottom, active, now);
        self.pull_step(step, target, cx);
        if step.redraw || self.history.pull.machine.animating(now) {
            cx.notify();
        }
        step.consumed
    }

    /// Act on what the machine asks for.
    fn pull_step(&mut self, step: Step, target: Target, cx: &mut Context<Self>) {
        let now = self.history.pull.now();
        if step.started {
            self.pull_find_next(target, now);
        }
        if step.haptic {
            haptic();
        }
        if step.release_ready {
            self.pull_release(target, cx);
        }
    }

    /// A pull began: find where it leads and preload it.
    fn pull_find_next(&mut self, target: Target, now: u64) {
        let Some(session) = self.session() else {
            return;
        };
        match target.mode {
            Mode::History => {
                let next = quill::pull_next::find_next_unread_channel(
                    session,
                    target.chat,
                    self.folders.tab,
                );
                self.history.pull.next_chat = next;
                self.history.pull.next_topic = None;
                if let Some(next) = next
                    && let Some(live) = self.live.as_mut()
                {
                    let _ = live.driver.preload_next_channel(next);
                }
                self.history.pull.machine.set_has_next(next.is_some(), now);
            }
            Mode::Topic => {
                let next = target.topic.and_then(|current| {
                    quill::pull_next::find_next_unread_topic(session, target.chat, current)
                });
                self.history.pull.next_chat = None;
                self.history.pull.next_topic = next;
                self.history.pull.machine.set_has_next(next.is_some(), now);
            }
        }
    }

    /// The finger lifted on a ready pull: open the next chat if it is
    /// still unread and the pull still allowed.
    fn pull_release(&mut self, target: Target, cx: &mut Context<Self>) {
        let active = self.pull_active(target, cx);
        match target.mode {
            Mode::History => {
                let Some(next) = self.history.pull.next_chat else {
                    return;
                };
                let unread = self
                    .session()
                    .and_then(|s| s.chats.get(&next.0))
                    .is_some_and(|chat| chat.unread_count > 0);
                if !unread || !active {
                    return;
                }
                self.history.pull.machine.begin_jump();
                self.pull_jump_when_ready(next, cx);
            }
            Mode::Topic => {
                let Some(next) = self.history.pull.next_topic else {
                    return;
                };
                let unread = self.session().is_some_and(|s| {
                    s.ordered_forum_topics(target.chat)
                        .iter()
                        .any(|topic| topic.forum_topic_id == next && topic.unread_count > 0)
                });
                if !unread || !active || target.topic == Some(next) {
                    return;
                }
                self.history.pull.machine.begin_jump();
                let Some((handle, scope)) = self.history.pull.window else {
                    return;
                };
                let chat = target.chat;
                // `crl::on_main`: on the next turn, checked again.
                self.history.pull.jump = Some(cx.spawn(async move |this, cx| {
                    let _ = cx.update_window(handle, |_, _, cx| {
                        let _ = this.update(cx, |this, cx| {
                            this.in_window_scope(scope, cx, |this, cx| {
                                if this.history.pull.target.map(|t| t.chat) == Some(chat)
                                    && this.history.pull.machine.jumping()
                                {
                                    SLIDE.set(Some((chat, Some(next), Instant::now())));
                                    this.select_topic_ui(next, cx);
                                }
                            })
                        });
                    });
                }));
            }
        }
    }

    /// `jumpWhenReady`: wait for the next channel's history (up to 1.5 s)
    /// and its pinned bar (up to 0.6 s), checking every 100 ms, then open
    /// it.
    fn pull_jump_when_ready(&mut self, next: ChatId, cx: &mut Context<Self>) {
        let Some((handle, scope)) = self.history.pull.window else {
            return;
        };
        let started = Instant::now();
        self.history.pull.jump = Some(cx.spawn(async move |this, cx| {
            loop {
                let waited = started.elapsed().as_millis() as u64;
                let (history, pinned) = this
                    .update(cx, |this, _| match this.live.as_ref() {
                        Some(live) => (
                            live.driver.next_channel_history_ready(next),
                            live.driver.next_channel_pinned_ready(next),
                        ),
                        None => (true, true),
                    })
                    .unwrap_or((true, true));
                let pinned = pinned || waited >= metrics::JUMP_PINNED_MAX_WAIT_MS;
                if (history && pinned) || waited >= metrics::JUMP_MAX_WAIT_MS {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(metrics::JUMP_POLL_MS))
                    .await;
            }
            let _ = cx.update_window(handle, |_, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.in_window_scope(scope, cx, |this, cx| {
                        if this.history.pull.machine.jumping() {
                            SLIDE.set(Some((next, None, Instant::now())));
                            this.select_listed_chat(next, window, cx);
                        }
                    })
                });
            });
        }));
    }

    /// Per render of the history: follow the shown chat, advance the
    /// machine and return how far the history rises, the indicator, and the
    /// wheel listener.
    pub(super) fn pull_layer(
        &mut self,
        cx: &mut Context<Self>,
    ) -> (f32, Option<AnyElement>, AnyElement) {
        self.pull_sync();
        let listener = self.pull_listener(cx);
        let Some(target) = self.history.pull.target else {
            return (0., None, listener);
        };
        let now = self.history.pull.now();
        if self.history.pull.machine.animating(now) {
            let active = self.pull_active(target, cx);
            let step = self.history.pull.machine.frame(active, now);
            self.pull_step(step, target, cx);
            // The note over the composer is drawn by the app, not this
            // slice: the whole window redraws while the pull moves.
            self.request_animation_tick_for(None, 60, cx);
        }
        let shift = self.history.pull.machine.content_shift();
        let indicator = self
            .history
            .pull
            .machine
            .indicator(now)
            .and_then(|state| self.pull_indicator(state, target, cx));
        (shift, indicator, listener)
    }

    /// The capture-phase wheel listener over the history's bounds.
    fn pull_listener(&self, cx: &mut Context<Self>) -> AnyElement {
        let app = cx.weak_entity();
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let app = app.clone();
                window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture || !bounds.contains(&event.position) {
                        return;
                    }
                    let consumed = app
                        .update(cx, |this, cx| {
                            // GPUI clears a visible tooltip on a wheel event with
                            // `window.refresh()`, which replays a cached view's last
                            // frame — tooltip included. Dirtying the conversation
                            // slice makes the next frame drop it (the "React" hint
                            // otherwise stays put after the message scrolls away).
                            this.notify_conversation(cx);
                            this.pull_wheel(event, window, cx)
                        })
                        .unwrap_or(false);
                    if consumed {
                        cx.stop_propagation();
                    }
                });
            },
        )
        .absolute()
        .inset_0()
        .size_full()
        .into_any_element()
    }

    /// The indicator under the history (`Indicator::paintEvent`).
    fn pull_indicator(
        &self,
        state: IndicatorState,
        target: Target,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let width = f32::from(self.history.viewport.get()?.size.width);
        let layout = IndicatorLayout::compute(state, width);
        let palette = self.history.palette.get();
        let (bg, fg) = super::chat_palette::with_palette(palette, false, || {
            (
                super::chat_palette::service_bg(cx.theme().secondary),
                cx.theme().secondary_foreground,
            )
        });
        let session = self.session();
        // The next chat: its name, photo and unread count.
        let (name, avatar, count): (String, Option<AnyElement>, i32) = match target.mode {
            Mode::History => match self
                .history
                .pull
                .next_chat
                .and_then(|id| session?.chats.get(&id.0))
            {
                Some(chat) => {
                    let photo = session
                        .and_then(|s| s.chat_photo_path(chat.id))
                        .and_then(|path| sandboxed_display_path(path, &self.media_display_roots()));
                    let title = chat.title.clone();
                    let size = layout.avatar.map_or(0., |(rect, _)| rect.w);
                    (
                        title.clone(),
                        Some(
                            super::chat_row::chat_avatar(&title, photo.as_deref(), size)
                                .into_any_element(),
                        ),
                        chat.unread_count,
                    )
                }
                None => (metrics::nothing_left_text(Mode::History, ""), None, 0),
            },
            Mode::Topic => {
                let topic = self.history.pull.next_topic.and_then(|id| {
                    session?
                        .ordered_forum_topics(target.chat)
                        .into_iter()
                        .find(|topic| topic.forum_topic_id == id)
                });
                match topic {
                    Some(topic) => {
                        let size = layout.avatar.map_or(0., |(rect, _)| rect.w);
                        (
                            topic.name.clone(),
                            Some(super::subsection_tabs::topic_icon(&topic, size, cx)),
                            topic.unread_count,
                        )
                    }
                    None => {
                        let group = session
                            .and_then(|s| s.chats.get(&target.chat.0))
                            .map_or(String::new(), |chat| chat.title.clone());
                        (metrics::nothing_left_text(Mode::Topic, &group), None, 0)
                    }
                }
            }
        };
        let has_next = avatar.is_some();
        let mut root = div()
            .id("pull-next-indicator")
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .h(px(metrics::MAX_HEIGHT));
        if let Some(capsule) = layout.capsule {
            let rect = capsule.rect;
            root = root.child(
                div()
                    .absolute()
                    .left(px(rect.x))
                    .top(px(rect.y))
                    .w(px(rect.w))
                    .h(px(rect.h))
                    .rounded(px(capsule.radius))
                    .bg(bg.opacity(capsule.alpha)),
            );
        }
        if let Some((name_top, alpha)) = layout.name
            && !name.is_empty()
        {
            root = root.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(name_top))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .max_w(px((width - 2. * metrics::SKIP).max(0.)))
                            .px(px(metrics::SKIP))
                            .py(px(metrics::PADDING))
                            .rounded(px(metrics::NAME_RADIUS))
                            .bg(bg.opacity(alpha))
                            .text_color(fg.opacity(alpha))
                            .text_size(px(metrics::NAME_FONT))
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(name),
                    ),
            );
        }
        if let Some((rect, alpha)) = layout.avatar {
            let content = match avatar {
                Some(avatar) => avatar,
                None => div().size_full().rounded_full().bg(bg).into_any_element(),
            };
            root = root.child(
                div()
                    .absolute()
                    .left(px(rect.x))
                    .top(px(rect.y))
                    .size(px(rect.w))
                    .opacity(alpha)
                    .child(content),
            );
            let scale = layout.badge_scale;
            if has_next && scale > 0. && count > 0 {
                let height = metrics::BADGE * scale;
                root = root.child(
                    div()
                        .absolute()
                        .right(px(width - (rect.x + rect.w + metrics::BADGE / 3.)))
                        .top(px(
                            rect.y - metrics::BADGE / 4. + (metrics::BADGE - height) / 2.
                        ))
                        .opacity(alpha)
                        .child(
                            div()
                                .h(px(height))
                                .min_w(px(height))
                                .px(px(2. * scale))
                                .rounded_full()
                                // The ring tdesktop cuts out of the photo.
                                .border(px(metrics::BADGE_RING * scale))
                                .border_color(cx.theme().background)
                                .bg(super::chat_theme::accent_strong())
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(gpui_kit::white())
                                .text_size(px(metrics::BADGE_FONT * scale))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(metrics::badge_text(count)),
                        ),
                );
            }
        }
        // Strokes: the arrow (clipped to the capsule) and the check.
        let arrow = layout.capsule.map(|c| (c.rect, c.arrow));
        let check = layout.avatar.filter(|_| !has_next);
        root = root.child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let origin = bounds.origin;
                    let at = |(x, y): (f32, f32)| point(origin.x + px(x), origin.y + px(y));
                    if let Some((rect, (cx_, cy, size, alpha))) = arrow
                        && size > 0.
                        && alpha > 0.
                    {
                        let clip = Bounds::new(at((rect.x, rect.y)), size_px(rect.w, rect.h));
                        let progress = size / metrics::ARROW_SIZE;
                        window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                            for line in metrics::arrow_strokes(cx_, cy, size) {
                                let points: Vec<_> = line.into_iter().map(at).collect();
                                round_stroke(
                                    window,
                                    &points,
                                    metrics::ARROW_STROKE * progress,
                                    fg.opacity(alpha),
                                );
                            }
                        });
                    }
                    if let Some((rect, alpha)) = check {
                        let centre = (rect.x + rect.w / 2., rect.y + rect.h / 2.);
                        let points: Vec<_> = metrics::check_stroke(centre.0, centre.1, rect.w / 2.)
                            .into_iter()
                            .map(at)
                            .collect();
                        round_stroke(window, &points, metrics::ARROW_STROKE, fg.opacity(alpha));
                    }
                },
            )
            .absolute()
            .inset_0()
            .size_full(),
        );
        Some(root.into_any_element())
    }

    /// The note over the composer while pulling (`HintOverlay`), `height`
    /// tall at the bottom of the conversation.
    pub(super) fn pull_hint_overlay(&self, cx: &App) -> Option<AnyElement> {
        let target = self.history.pull.target?;
        let epoch = self.history.pull.epoch?;
        let now = epoch.elapsed().as_millis() as u64;
        let HintState { panel, release } = self.history.pull.machine.hint(now)?;
        let text = |release_text: bool, opacity: f32, shift: f32| {
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .px(px(2. * metrics::SKIP))
                .child(
                    div()
                        .relative()
                        .top(px(shift))
                        .opacity(opacity)
                        .text_size(px(metrics::HINT_FONT))
                        .text_color(cx.theme().muted_foreground)
                        .truncate()
                        .child(metrics::hint_text(target.mode, release_text)),
                )
        };
        Some(
            div()
                .id("pull-next-hint")
                .absolute()
                .inset_0()
                .occlude()
                .bg(cx.theme().background.opacity(panel))
                .when(release < 1., |this| {
                    this.child(text(
                        false,
                        panel * (1. - release),
                        -metrics::HINT_SLIDE * release,
                    ))
                })
                .when(release > 0., |this| {
                    this.child(text(
                        true,
                        panel * release,
                        metrics::HINT_SLIDE * (1. - release),
                    ))
                })
                .into_any_element(),
        )
    }

    /// The main window's conversation sliding in from the bottom after a
    /// pull opened it: how far down it still is, as a fraction of its
    /// height (`SlideAnimation`, `FromBottom`). The chat it left cannot be
    /// drawn sliding out: GPUI keeps no picture of it.
    pub(super) fn pull_slide_offset(&self, cx: &mut Context<Self>) -> f32 {
        let Some((chat, topic, start)) = SLIDE.get() else {
            return 0.;
        };
        let shown =
            self.open_chat_id() == Some(chat) && self.session().and_then(|s| s.open_topic) == topic;
        let t = start.elapsed().as_secs_f32() * 1000. / SLIDE_MS;
        if !shown || t >= 1. {
            SLIDE.set(None);
            return 0.;
        }
        self.request_animation_tick_for(None, 60, cx);
        1. - metrics::ease_out_quint(t)
    }

    /// Settings: tdesktop's "Swipe to the next unread channel" (on by
    /// default), after the corner buttons.
    pub(super) fn pull_next_switch(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self
            .session()
            .is_none_or(|s| s.settings.media_prefs.pull_to_next_channel);
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .child(
                Switch::new("media-pref-pull-next")
                    .checked(on)
                    .accessibility_label(tr(keys::SETTINGS_PULL_TO_NEXT_CHANNEL))
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_media_pref(|prefs| prefs.pull_to_next_channel = on, cx);
                    })),
            )
            .child(
                div()
                    .text_sm()
                    .child(tr(keys::SETTINGS_PULL_TO_NEXT_CHANNEL)),
            )
            .into_any_element()
    }
}

fn size_px(w: f32, h: f32) -> gpui_kit::Size<Pixels> {
    size(px(w), px(h))
}

/// A polyline with round caps and joins (tdesktop's `Qt::RoundCap` /
/// `Qt::RoundJoin`): the stroke, and a dot of its width at every vertex.
fn round_stroke(window: &mut Window, points: &[Point<Pixels>], width: f32, color: Hsla) {
    if points.len() < 2 || width <= 0. {
        return;
    }
    let mut path = PathBuilder::stroke(px(width));
    path.add_polygon(points, false);
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
    let radius = px(width / 2.);
    for point in points {
        window.paint_quad(
            fill(
                Bounds::new(
                    *point - gpui_kit::point(radius, radius),
                    size(radius * 2., radius * 2.),
                ),
                color,
            )
            .corner_radii(radius),
        );
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "pull_next_ui_tests.rs"]
mod tests;
