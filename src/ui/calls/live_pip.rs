//! Live Picture-in-Picture: a stream's or a live story's live player in
//! the file PiP's window (`video_player::controls::pip`), in the live
//! controller's PiP form. tdesktop has no live PiP; its window, placement
//! and moves are the file PiP's (`media_view_pip.cpp`).
//!
//! - The stream window's PiP button and the live story header's open it:
//!   the stream window closes (as you closing it would) and the story
//!   viewer closes, while the call, the player and its feed go on. The
//!   call's sound keeps playing through the call.
//! - Enlarge reopens the stream window or the story viewer on the same
//!   story; once it shows the picture, the window lets go of the player
//!   (the same one: nothing restarts).
//! - Close keeps a stream's call (the main window's call bar brings the
//!   stream window back). A live story has no surface but the viewer, so
//!   closing its PiP leaves it, as closing the viewer does.
//! - One sound at a time: it closes a file PiP and a file PiP closes it,
//!   and so does any other sound starting (`QuillApp::sound_starts`).
//! - It goes on lock, on logout of its account, and when its call or
//!   stream ends.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::video_player::VideoPlayer;
use crate::ui::video_player::controls::pip::{
    LivePip, LivePipFacts, PipHooks, PipOpen, PipWindow, open_pip_window,
};
use gpui_kit::*;
use quill::calls::engine::StreamState;
use quill::ids::{AccountKey, ChatId};
use quill::lang::{keys, tr};
use std::rc::Rc;

/// What a live PiP plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LivePipSource {
    /// A live stream's picture, by its group call.
    Stream { call_id: i32 },
    /// A live story.
    Story { chat_id: ChatId, story_id: i32 },
}

/// An open (or opening) live PiP.
pub(crate) struct LivePipLink {
    /// `None` until the window is made (it opens deferred).
    pub(crate) window: Option<PipWindow>,
    pub(crate) source: LivePipSource,
    /// The account it was opened in.
    account: Option<AccountKey>,
    /// The window the picture was in, brought to the front on enlarge.
    return_to: Option<AnyWindowHandle>,
}

impl QuillApp {
    /// The live story the PiP plays: the viewer keeps it joined.
    pub(crate) fn live_pip_story(&self) -> Option<(ChatId, i32)> {
        match self.group_call.live_pip.as_ref()?.source {
            LivePipSource::Story { chat_id, story_id } => Some((chat_id, story_id)),
            LivePipSource::Stream { .. } => None,
        }
    }

    /// The PiP plays the stream: its player and state outlive the
    /// stream window.
    pub(crate) fn live_pip_keeps_stream(&self) -> bool {
        self.group_call
            .live_pip
            .as_ref()
            .is_some_and(|link| matches!(link.source, LivePipSource::Stream { .. }))
    }

    /// The stream bar's PiP button: the stream's picture goes into a
    /// floating window and the stream window closes; the call stays.
    pub(crate) fn open_stream_pip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(call_id) = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .filter(|call| !call.is_live_story)
            .map(|call| call.id)
        else {
            return;
        };
        let Some(player) = self.group_call.stream.player.clone() else {
            return;
        };
        let return_to = Some(window.window_handle());
        self.open_live_pip(player, LivePipSource::Stream { call_id }, return_to, cx);
        self.group_call.window_closed_by_user = Some(call_id);
        self.group_call.stream.pending_join = None;
        self.group_call.stream.overlay = None;
        // The stream window goes now, out of full screen first, rather
        // than on the main window's next frame.
        if self.group_call.window == Some(window.window_handle()) {
            self.group_call.window = None;
            crate::ui::group_call_stream_layout::leave_stream_full(window);
            window.remove_window();
        }
        cx.notify();
    }

    /// The live story header's PiP button: the story's picture goes into
    /// a floating window and the viewer closes; the story stays joined.
    pub(crate) fn open_story_pip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((chat_id, story_id)) = self.stories.live.watching else {
            return;
        };
        let Some(player) = self.stories.live.player.clone() else {
            return;
        };
        let return_to = Some(window.window_handle());
        let source = LivePipSource::Story { chat_id, story_id };
        self.open_live_pip(player, source, return_to, cx);
        self.close_story_viewer(cx);
    }

    fn open_live_pip(
        &mut self,
        player: VideoPlayer,
        source: LivePipSource,
        return_to: Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) {
        // One sound at a time; one live PiP at a time.
        self.close_video_pip(cx);
        self.close_live_pip(cx);
        let account = self.session().map(|session| session.account.clone());
        self.group_call.live_pip = Some(LivePipLink {
            window: None,
            source,
            account,
            return_to,
        });
        let weak = cx.entity().downgrade();
        let display = return_to.and_then(|handle| {
            handle
                .update(cx, |_, window, cx| window.display(cx).map(|d| d.id()))
                .ok()
                .flatten()
        });
        let open = PipOpen {
            dimensions: player.picture_size(),
            player,
            hooks: live_pip_hooks(weak.clone()),
            last_volume: 1.0,
            display,
            live: Some(live_pip_controls(weak.clone(), source)),
        };
        // Window creation draws at once: let this update finish first.
        cx.defer(move |cx| {
            let opened = open_pip_window(open, cx);
            let _ = weak.update(cx, |app, cx| {
                let pending = app
                    .group_call
                    .live_pip
                    .as_mut()
                    .filter(|link| link.source == source && link.window.is_none());
                match (pending, opened) {
                    (Some(link), Some(window)) => link.window = Some(window),
                    // Closed before it opened: the window goes too.
                    (None, Some(window)) => close_window(window, false, cx),
                    (_, None) => {
                        app.group_call.live_pip = None;
                        app.connection.status_note = tr(keys::Q_MEDIA_PIP_FAILED).into();
                        app.check_live_pip(cx);
                    }
                }
                cx.notify();
            });
        });
    }

    /// Close the live PiP. A stream's call stays (the next window sync
    /// drops its player while its window stays closed); a live story the
    /// viewer does not show is left at the next check. Runs without the
    /// main window drawing.
    pub(crate) fn close_live_pip(&mut self, cx: &mut App) {
        let Some(link) = self.group_call.live_pip.take() else {
            return;
        };
        if let Some(window) = link.window {
            close_window(window, false, cx);
        }
        cx.refresh_windows();
    }

    /// The window shows the picture again (the stream window, or the
    /// viewer on the story): the PiP lets go of the player, which plays on
    /// there, and closes.
    fn detach_live_pip(&mut self, cx: &mut Context<Self>) {
        if let Some(window) = self.group_call.live_pip.take().and_then(|link| link.window) {
            close_window(window, true, cx);
        }
    }

    /// Enlarge: back to the stream window or the story.
    fn live_pip_enlarge(&mut self, cx: &mut Context<Self>) {
        let Some(link) = self.group_call.live_pip.as_ref() else {
            return;
        };
        let account = self.session().map(|session| session.account.clone());
        if account != link.account {
            return;
        }
        let (source, return_to) = (link.source, link.return_to);
        match source {
            LivePipSource::Stream { .. } => self.show_group_call_window(cx),
            LivePipSource::Story { chat_id, story_id } => {
                let items = self
                    .session()
                    .map(|session| {
                        quill::story_viewer::collect_found_story_items(
                            &[(chat_id.0, story_id)],
                            &session.stories.stories,
                        )
                    })
                    .unwrap_or_default();
                if items.is_empty() {
                    return;
                }
                self.begin_story_viewer(items, 0, cx);
                if let Some(handle) = return_to {
                    cx.defer(move |cx| {
                        cx.activate(true);
                        let _ = handle.update(cx, |_, window, _| window.activate_window());
                    });
                }
            }
        }
        self.check_live_pip(cx);
        cx.notify();
    }

    /// After updates land and at each root render: the window closed
    /// itself, the picture shows elsewhere again, the app locked, or the
    /// call or stream ended.
    pub(crate) fn check_live_pip(&mut self, cx: &mut Context<Self>) {
        let Some(link) = self.group_call.live_pip.as_ref() else {
            // A live story kept by a PiP that is gone, with no viewer on
            // it: leave it, as closing the viewer would have.
            if self.stories.live.watching.is_some() && !self.stories.viewer.is_open() {
                self.sync_live_story(cx);
            }
            return;
        };
        let source = link.source;
        if let Some(window) = link.window.as_ref() {
            let closed = window
                .view
                .upgrade()
                .is_none_or(|view| view.read(cx).player().is_none());
            if closed {
                self.group_call.live_pip = None;
                self.check_live_pip(cx);
                return;
            }
        }
        if self.account.passcode.locked || !self.live_pip_source_alive(source) {
            self.close_live_pip(cx);
            return;
        }
        let shown_again =
            match source {
                LivePipSource::Stream { .. } => self.group_call.window.is_some(),
                LivePipSource::Story { chat_id, story_id } => {
                    self.stories.viewer.is_open()
                        && self.stories.viewer.current().is_some_and(|item| {
                            item.chat_id == chat_id && item.story_id == story_id
                        })
                }
            };
        if shown_again && link.window.is_some() {
            self.detach_live_pip(cx);
        }
    }

    /// The PiP's call is still there and its picture not over: the stream
    /// or live story's call is tracked, not left, and not ended; a live
    /// story is still the one watched.
    fn live_pip_source_alive(&self, source: LivePipSource) -> bool {
        let Some(calls) = self.session().map(|s| &s.calls) else {
            return false;
        };
        let call = calls.active_group_call.as_ref();
        match source {
            LivePipSource::Stream { call_id } => call.is_some_and(|call| {
                call.id == call_id
                    && call.is_joined
                    && calls.left_group_call != Some(call_id)
                    && call.stream != StreamState::Ended
            }),
            LivePipSource::Story { chat_id, story_id } => {
                self.stories.live.watching == Some((chat_id, story_id))
                    && !call.is_some_and(|call| call.stream == StreamState::Ended)
            }
        }
    }

    /// The account `account` logged out or was removed: its live PiP
    /// goes with it.
    pub(crate) fn live_pip_account_ended(&mut self, account: &AccountKey, cx: &mut Context<Self>) {
        if self
            .group_call
            .live_pip
            .as_ref()
            .is_some_and(|link| link.account.as_ref() == Some(account))
        {
            self.close_live_pip(cx);
        }
    }

    /// The badge and the mute toggle's state.
    fn live_pip_facts(&self) -> LivePipFacts {
        LivePipFacts {
            viewers: self
                .session()
                .and_then(|s| s.calls.active_group_call.as_ref())
                .map_or(0, |call| call.viewer_count),
            muted: self.stream_muted(),
        }
    }
}

/// Close `window` (deferred: its view may be the one that asked).
/// `detach`: the picture shows elsewhere, so the player is let go of
/// without telling the owner it closed.
fn close_window(window: PipWindow, detach: bool, cx: &mut App) {
    cx.defer(move |cx| {
        let view = window.view.clone();
        let _ = window.window.update(cx, |_, window, cx| {
            let _ = view.update(cx, |pip, cx| {
                if detach {
                    pip.detach(window, cx);
                } else {
                    pip.close(window, cx);
                }
            });
        });
    });
}

/// What the window tells the app (each deferred by the window).
fn live_pip_hooks(app: WeakEntity<QuillApp>) -> PipHooks {
    let closed = app.clone();
    PipHooks {
        closed: Rc::new(move |cx| {
            let _ = closed.update(cx, |app, cx| app.check_live_pip(cx));
        }),
        enlarge: Rc::new(move |cx| {
            let _ = app.update(cx, |app, cx| app.live_pip_enlarge(cx));
        }),
        // No volume line: the sound is the call's.
        volume: Rc::new(|_, _| {}),
    }
}

/// The live controls' facts and mute, read from the app.
fn live_pip_controls(app: WeakEntity<QuillApp>, source: LivePipSource) -> LivePip {
    let facts = app.clone();
    LivePip {
        facts: Rc::new(move |cx| {
            facts
                .upgrade()
                .map(|app| app.read(cx).live_pip_facts())
                .unwrap_or_default()
        }),
        toggle_mute: Rc::new(move |cx| {
            let _ = app.update(cx, |app, cx| app.toggle_stream_mute(cx));
        }),
        back_label: match source {
            LivePipSource::Stream { .. } => tr(keys::Q_AV_LIVE_PIP_BACK_STREAM),
            LivePipSource::Story { .. } => tr(keys::Q_AV_LIVE_PIP_BACK_STORY),
        },
    }
}
