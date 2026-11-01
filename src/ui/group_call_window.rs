//! The group call's own window (tdesktop `Calls::Group::Panel`): opens and
//! closes with the tracked call and draws either the voice / video chat
//! panel (`group_call_panel.rs`) or, for a channel's RTMP live stream, the
//! stream layout (`group_call_stream_layout.rs`).
//!
//! A live stream window is 720 × 580 (at least 240 × 160) and titled with
//! the chat's name, as tdesktop sizes `_call->rtmp()` panels; the chat
//! panel stays 420 × 640 (at least 380 × 520). When a call switches
//! between the two the window reopens at the other size. A stream that
//! ends closes the window (tdesktop closes the panel with the call).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::calls::stream_view::StreamPictureView;
use super::frame_clock::{follow_app, trace_slice_render};
use super::group_call_members_view::GroupCallMembers;
use super::group_call_panel::BG;
use super::group_call_stream_layout::{
    leave_stream_full, stream_full, stream_viewport, uses_stream_layout,
};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{Root, WindowExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::engine::StreamState;
use quill::lang::{keys, tr};

impl QuillApp {
    /// Close the window of a call you haven't joined.
    pub(super) fn close_group_call_window(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .map(|c| c.id)
        {
            self.group_call.window_closed_by_user = Some(id);
        }
        self.group_call.stream.pending_join = None;
        cx.notify();
    }

    /// Open or close the group call window with the tracked group call.
    pub(super) fn sync_group_call_window(&mut self, cx: &mut Context<Self>) {
        self.sync_global_ptt(cx);
        let call = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            // A live story plays in the story viewer, not in this window.
            .filter(|call| !call.is_live_story)
            .map(|call| {
                (
                    call.id,
                    uses_stream_layout(call),
                    call.stream == StreamState::Ended,
                    call.is_joined,
                )
            });
        // The stream ended: stop listening and close the window, as
        // tdesktop closes the panel with the call. Full screen is left
        // first so the window does not close into an empty space.
        if let Some((id, true, true, joined)) = call {
            if self.group_call.window_closed_by_user != Some(id) {
                self.group_call.window_closed_by_user = Some(id);
                if let Some(handle) = self.group_call.window {
                    let _ = handle.update(cx, |_, window, _| leave_stream_full(window));
                }
            }
            if joined {
                self.leave_active_group_call(cx);
            }
            self.group_call.stream.pending_join = None;
        }
        // The stream you asked for, or the call you created, is tracked:
        // join it. Nothing else joins by itself; an unjoined window offers
        // its Join button.
        self.consume_stream_join_intent(cx);
        let call = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .filter(|call| !call.is_live_story)
            .map(|call| (call.id, uses_stream_layout(call)));
        let wanted = call.filter(|(id, _)| self.group_call.window_closed_by_user != Some(*id));
        // Tile players of a call that is no longer shown go.
        let shown = wanted.map(|(id, _)| id);
        self.group_call.players.retain(|key, player| {
            let keep = key.call_id().is_some() && key.call_id() == shown;
            if !keep {
                player.release_live(cx);
            }
            keep
        });
        // A live PiP plays the stream on: its player stays.
        if wanted.is_none_or(|(_, stream)| !stream) && !self.live_pip_keeps_stream() {
            self.group_call.stream.clear(cx);
        }
        match (wanted, self.group_call.window) {
            (Some((_, stream)), None) => self.open_group_call_window(stream, cx),
            (None, Some(handle)) => {
                self.group_call.window = None;
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
            // Sized for the other layout: reopen.
            (Some((_, stream)), Some(handle))
                if stream != self.group_call.stream.window_is_stream =>
            {
                self.group_call.window = None;
                let _ = handle.update(cx, |_, window, _| window.remove_window());
                self.open_group_call_window(stream, cx);
            }
            _ => {}
        }
    }

    fn open_group_call_window(&mut self, stream: bool, cx: &mut Context<Self>) {
        if self.group_call.window_opening {
            return;
        }
        self.group_call.window_opening = true;
        let title: SharedString = if stream {
            self.session()
                .and_then(|s| s.calls.active_group_call.as_ref())
                .and_then(|call| self.group_call_chat(call))
                .map(|chat| chat.title.clone())
                .unwrap_or_default()
                .into()
        } else {
            tr(keys::MENU_START_GROUP_CALL_OPTIONS).into()
        };
        let (window_size, min_size) = if stream {
            (size(px(720.), px(580.)), size(px(240.), px(160.)))
        } else {
            (size(px(420.), px(640.)), size(px(380.), px(520.)))
        };
        let owner = cx.entity();
        cx.defer(move |cx| {
            let weak = owner.downgrade();
            let render_owner = owner.clone();
            let bounds = Bounds::centered(None, window_size, cx);
            let result = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(min_size),
                    titlebar: Some(TitlebarOptions {
                        title: Some(title),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(12.), px(12.))),
                    }),
                    focus: true,
                    show: true,
                    ..Default::default()
                },
                move |window, cx| {
                    window.activate_window();
                    window.on_window_should_close(cx, move |_, cx| {
                        let _ = weak.update(cx, |app, cx| {
                            app.group_call.window = None;
                            app.group_call.window_closed_by_user = app
                                .session()
                                .and_then(|s| s.calls.active_group_call.as_ref())
                                .map(|c| c.id);
                            app.group_call.stream.pending_join = None;
                            app.group_call.stream.overlay = None;
                            app.sync_group_call_large_tile();
                            cx.notify();
                        });
                        true
                    });
                    let view = cx.new(|cx| {
                        let activation_owner = render_owner.downgrade();
                        let visibility_owner = render_owner.downgrade();
                        GroupCallPanel {
                            owner: render_owner.downgrade(),
                            _observe: follow_app(&render_owner, cx),
                            focus: cx.focus_handle(),
                            // A key-up can't arrive once the window is in
                            // the background: close the push-to-talk mic.
                            _activation: cx.observe_window_activation(
                                window,
                                move |_, window, cx| {
                                    if !window.is_window_active() {
                                        let _ = activation_owner.update(cx, |app, cx| {
                                            app.group_call_ptt_release_all(cx)
                                        });
                                    }
                                },
                            ),
                            _visibility: cx.observe_window_visibility(
                                window,
                                move |_, visibility, window, cx| {
                                    let from = window.window_handle();
                                    let _ = visibility_owner.update(cx, |app, _| {
                                        // A replaced window still reports while it goes.
                                        if app.group_call.window != Some(from) {
                                            return;
                                        }
                                        app.group_call.window_visible = visibility.is_visible();
                                        app.sync_group_call_large_tile();
                                    });
                                },
                            ),
                            pinned_on_top: false,
                            title: None,
                            members: cx.new(|cx| GroupCallMembers::new(&render_owner, cx)),
                        }
                    });
                    let picture = cx.new(|cx| StreamPictureView::new(&render_owner, cx));
                    let root = cx.new(|cx| GroupCallRoot {
                        owner: render_owner.downgrade(),
                        _observe: follow_app(&render_owner, cx),
                        panel: view,
                        picture,
                    });
                    cx.new(|cx| Root::new(root, window, cx))
                },
            );
            owner.update(cx, |app, cx| {
                app.group_call.window_opening = false;
                match result {
                    Ok(handle) => {
                        app.group_call.window = Some(handle.into());
                        app.group_call.window_visible = true;
                        app.group_call.stream.window_is_stream = stream;
                    }
                    Err(_) => app.connection.set_note(StatusNote::error(tr(
                        keys::Q_AV_COULDNT_OPEN_THE_VIDEO_CHAT,
                    ))),
                }
                cx.notify();
            });
        });
    }

    pub(super) fn group_call_large_tile(&self) -> Option<quill::calls::tile_pin::TileKey> {
        let seen = self.group_call.window.is_some() && self.group_call.window_visible;
        self.group_call.pin.pinned().filter(|_| seen)
    }

    fn sync_group_call_large_tile(&mut self) {
        let tile = self.group_call_large_tile();
        if let Some(live) = self.live.as_mut() {
            live.driver.set_group_call_large_tile(tile);
        }
    }

    /// Bring the group call window back (the call bar).
    pub(super) fn show_group_call_window(&mut self, cx: &mut Context<Self>) {
        self.group_call.window_closed_by_user = None;
        if let Some(handle) = self.group_call.window {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        }
        cx.notify();
    }

    /// Whether the tracked call shows as a live stream.
    pub(super) fn group_call_is_stream(&self) -> bool {
        self.session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .is_some_and(uses_stream_layout)
    }
}

/// The window's root: the panel alone, or for a live stream the picture
/// under the panel as two cached views, so a new frame redraws only the
/// picture and the panel's chrome replays.
struct GroupCallRoot {
    owner: WeakEntity<QuillApp>,
    _observe: Subscription,
    panel: Entity<GroupCallPanel>,
    picture: Entity<StreamPictureView>,
}

impl Render for GroupCallRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = self.owner.upgrade();
        let stream = owner
            .as_ref()
            .is_some_and(|owner| owner.read(cx).group_call_is_stream());
        if !stream {
            return div()
                .size_full()
                .child(self.panel.clone())
                .into_any_element();
        }
        let full = owner
            .as_ref()
            .is_some_and(|owner| stream_full(window, owner.read(cx)));
        let (inset, top) = stream_viewport(full);
        div()
            .size_full()
            .relative()
            .bg(if full { black() } else { rgb(BG).into() })
            .child(
                div()
                    .absolute()
                    .left(px(inset))
                    .right(px(inset))
                    .bottom(px(inset))
                    .top(px(top))
                    .child(
                        self.picture
                            .clone()
                            .cached(StyleRefinement::default().size_full()),
                    ),
            )
            .child(
                div().absolute().inset_0().child(
                    self.panel
                        .clone()
                        .cached(StyleRefinement::default().size_full()),
                ),
            )
            .into_any_element()
    }
}

pub(super) struct GroupCallPanel {
    owner: WeakEntity<QuillApp>,
    _observe: Subscription,
    /// Takes keyboard focus so push-to-talk key events reach the window.
    focus: FocusHandle,
    _activation: Subscription,
    _visibility: Subscription,
    /// The window stays above the others (tdesktop "pin on top").
    pinned_on_top: bool,
    /// The title the window was given (a stream's follows its chat).
    title: Option<String>,
    members: Entity<GroupCallMembers>,
}

impl QuillApp {
    /// The call window's content: a stream's, the video stage or the panel.
    /// Returns it, whether it animates, and the stream's title.
    fn group_call_window_body(
        &mut self,
        panel: EntityId,
        members: &Entity<GroupCallMembers>,
        fullscreen: bool,
        stream_full: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (AnyElement, bool, Option<String>) {
        if let Some(facts) = self.group_call_stream_facts() {
            let body = self.group_call_stream_body(&facts, stream_full, window, cx);
            let title = facts.chat.map(|(_, name)| name).unwrap_or_default();
            return (body.element, body.animating, Some(title));
        }
        let wants_stage = fullscreen || self.demo_ui.group_stage;
        let call = wants_stage
            .then(|| self.session()?.calls.active_group_call.clone())
            .flatten();
        let stage = call.and_then(|call| self.group_call_stage(&call, cx));
        // The stage closed: its volume slider goes with it.
        if stage.is_none() {
            self.group_call.stage_volume = None;
        }
        let body = match stage {
            Some(stage) => stage,
            None => self.group_call_panel_body(panel, members, window, cx),
        };
        (body, false, None)
    }
}

impl Render for GroupCallPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        trace_slice_render("group-call-panel");
        let Some(owner) = self.owner.upgrade() else {
            window.remove_window();
            return div().into_any_element();
        };
        if self.focus.is_focused(window) || window.focused(cx).is_none() {
            self.focus.focus(window, cx);
        }
        let fullscreen = window.is_fullscreen();
        // tdesktop treats a maximized stream window as full screen too.
        let stream_full = stream_full(window, owner.read(cx));
        let panel = cx.entity_id();
        let members = self.members.clone();
        let (body, animating, stream_title) = owner.update(cx, |app, cx| {
            app.frame.tick_view.set(Some(panel));
            let body =
                app.group_call_window_body(panel, &members, fullscreen, stream_full, window, cx);
            app.frame.tick_view.set(None);
            body
        });
        owner.update(cx, |app, _| app.sync_group_call_large_tile());
        if animating {
            window.request_animation_frame();
        }
        let stream = stream_title.is_some();
        // The stream window is named after its chat, which can be renamed.
        if let Some(title) = stream_title
            && self.title.as_ref() != Some(&title)
        {
            window.set_window_title(&title);
            self.title = Some(title);
        }
        let full = if stream { stream_full } else { fullscreen };
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let Some(owner) = this.owner.upgrade() else {
                    return;
                };
                let key = event.keystroke.key.clone();
                let exits = quill::calls::tile_pin::exits_fullscreen(&key);
                if owner.read(cx).group_call_is_stream() {
                    // Esc closes an open box first; then the live keys
                    // (Esc leaves full screen or the maximized window,
                    // F toggles it, M mutes, Space does nothing).
                    if exits && owner.read(cx).group_call.stream.overlay.is_some() {
                        owner.update(cx, |app, cx| app.close_stream_overlay(cx));
                        cx.stop_propagation();
                        return;
                    }
                    let keystroke = event.keystroke.clone();
                    if owner.update(cx, |app, cx| app.stream_key_down(&keystroke, window, cx)) {
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    if exits {
                        return;
                    }
                }
                if exits && window.is_fullscreen() {
                    owner.update(cx, |app, cx| app.leave_group_call_stage(window, cx));
                    cx.stop_propagation();
                    return;
                }
                if owner.update(cx, |app, cx| app.group_call_key_down(&key, cx)) {
                    cx.stop_propagation();
                }
            }))
            .on_key_up(cx.listener(|this, event: &KeyUpEvent, _, cx| {
                let Some(owner) = this.owner.upgrade() else {
                    return;
                };
                let key = event.keystroke.key.clone();
                if owner.update(cx, |app, cx| app.group_call_key_up(&key, cx)) {
                    cx.stop_propagation();
                }
            }))
            .child(body)
            // tdesktop's pin-on-top control; full screen there is no
            // window chrome to pin.
            .when(!full, |this| {
                this.relative().child(
                    super::window_control::pin_on_top_button(
                        "group-call-pin-on-top",
                        self.pinned_on_top,
                        cx,
                        move |this, pinned, window, cx| {
                            this.pinned_on_top = pinned;
                            // A live stream says so (tdesktop
                            // `toggleWindowPinnedOnTop` for RTMP).
                            if stream {
                                let text = tr(if pinned {
                                    keys::GROUP_CALL_PINNED_ON_TOP
                                } else {
                                    keys::GROUP_CALL_UNPINNED_ON_TOP
                                });
                                window.push_notification(
                                    Notification::new().message(text.to_string()),
                                    cx,
                                );
                            }
                            cx.notify();
                        },
                    )
                    .text_color(white()),
                )
            })
            .into_any_element()
    }
}
