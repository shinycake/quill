//! The 1:1 call window's pictures (tdesktop `Calls::Panel`): the peer's
//! screen share or camera filling the window, and your camera in the
//! corner above the buttons. Each is a live player over the call's feed
//! (`LiveKey::Call`), the same player streams and video chats use, in a
//! view of its own: a new frame redraws only its picture, while the panel
//! over it (name, status, buttons) replays its cached frame.
//!
//! Your camera is mirrored, as tdesktop's outgoing bubble
//! (`calls_panel.cpp`, `setMirrored(!screen sharing)`; in a 1:1 call your
//! camera and your screen share never run together, so the preview is
//! always the camera). It is drawn as an image, which rounds its own
//! corners over the peer's picture.
//!
//! The window's root draws, bottom to top: the dark ground and the peer's
//! photo colors (`call_panel_backdrop`), the peer's picture, your preview,
//! then the panel with its shades and controls.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::app_theme::call_chrome;
use crate::ui::call_panel::CallPanel;
use crate::ui::call_panel_backdrop::backdrop;
use crate::ui::frame_clock::trace_slice_render;
use crate::ui::video_player::{Purpose, VideoPlayer, video_frame};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::engine::RemoteVideoState;
use quill::calls::live_video::{LiveKey, LiveSink};
use quill::state::ActiveCall;
use quill::telegram::envelope::CallState;
use std::cell::Cell;
use std::sync::Arc;

/// Which of the call's pictures a view shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallPicture {
    /// The peer's screen share, else their camera.
    Remote,
    /// Your camera.
    Local,
}

/// Your camera's corner preview: 160 × 110 at 12 from the right and 99
/// from the bottom, above the buttons, radius 10.
const PREVIEW_SIZE: (f32, f32) = (160., 110.);
const PREVIEW_RIGHT: f32 = 12.;
const PREVIEW_BOTTOM: f32 = 99.;
const PREVIEW_RADIUS: f32 = 10.;

/// The feed `picture` shows in `call` now, `None` for nothing: the
/// peer's screen share before their camera, once the call is up (a
/// paused or stopped picture shows their photo instead); your camera
/// while `camera` (it runs).
pub(crate) fn call_picture_key(
    call: &ActiveCall,
    picture: CallPicture,
    camera: bool,
) -> Option<LiveKey> {
    let (local, screen) = match picture {
        CallPicture::Local => (true, false),
        CallPicture::Remote => {
            if !matches!(call.state, CallState::Ready) {
                return None;
            }
            if call.remote_screen == RemoteVideoState::Active {
                (false, true)
            } else if call.remote_video == RemoteVideoState::Active {
                (false, false)
            } else {
                return None;
            }
        }
    };
    if local && !camera {
        return None;
    }
    Some(LiveKey::Call {
        call_id: call.id,
        local,
        screen,
    })
}

impl QuillApp {
    /// `key`'s feed: the driver's, or a demo fixture's.
    fn call_sink(&self, key: LiveKey) -> Option<LiveSink> {
        match self.live.as_ref() {
            Some(live) => live.driver.live_sink(key),
            None => self
                .demo_ui
                .group_feeds
                .get(&key)
                .map(|feed| feed.sink.clone()),
        }
    }

    /// The player of `picture`, following its feed: a new feed (the peer
    /// switched from camera to screen, or turned the camera on again)
    /// gets a new player; no feed, no player.
    pub(crate) fn call_picture_player(
        &mut self,
        picture: CallPicture,
        cx: &mut App,
    ) -> Option<VideoPlayer> {
        let key = self
            .session()
            .and_then(|s| s.calls.active_call.as_ref())
            .and_then(|call| call_picture_key(call, picture, self.call_camera_effective(call)));
        let sink = key.and_then(|key| self.call_sink(key));
        let slot = match picture {
            CallPicture::Remote => &mut self.calls.remote_player,
            CallPicture::Local => &mut self.calls.local_player,
        };
        let same = match (slot.as_ref(), sink.as_ref()) {
            (Some(player), Some(sink)) => player.live_sink().is_some_and(|own| own.same_as(sink)),
            (None, None) => true,
            _ => false,
        };
        if !same {
            if let Some(old) = slot.take() {
                old.release_live(cx);
            }
            *slot = sink.map(|sink| {
                let player = VideoPlayer::live(sink, Purpose::Live { tile: false }, cx);
                if picture == CallPicture::Local {
                    player.set_live_mirrored(true);
                    player.set_live_images_only(true);
                }
                player
            });
        }
        slot.clone()
    }

    /// Whether `picture` has a frame to show (the panel lays out around
    /// the peer's picture, and shrinks the photo for your preview).
    pub(crate) fn call_picture_drawn(&self, picture: CallPicture) -> bool {
        let slot = match picture {
            CallPicture::Remote => &self.calls.remote_player,
            CallPicture::Local => &self.calls.local_player,
        };
        slot.as_ref().is_some_and(VideoPlayer::has_live_frame)
    }

    /// The window closed or the call is over: its players go, and their
    /// images leave the atlas now.
    pub(crate) fn release_call_players(&mut self, cx: &mut App) {
        for player in [
            self.calls.remote_player.take(),
            self.calls.local_player.take(),
        ]
        .into_iter()
        .flatten()
        {
            player.release_live(cx);
        }
    }

    /// The radial backdrop from the peer's profile colors (tdesktop
    /// `PanelBackground`), for the call or the ended call's summary.
    pub(crate) fn call_peer_backdrop(&self) -> Option<Arc<RenderImage>> {
        let session = self.session()?;
        let user_id = match (&session.calls.active_call, &session.calls.summary) {
            (Some(call), _) => call.user_id,
            (None, Some(summary)) => summary.user_id,
            (None, None) => return None,
        };
        let user = session
            .user(user_id)
            .filter(|u| u.profile_accent_color_id >= 0)?;
        let colors = session
            .chats_state
            .profile_accent_colors
            .iter()
            .find(|color| color.id == user.profile_accent_color_id)?;
        backdrop(&colors.dark_colors)
    }
}

/// One of the call's pictures.
pub(crate) struct CallPictureView {
    owner: WeakEntity<QuillApp>,
    picture: CallPicture,
    _observe: Subscription,
}

impl CallPictureView {
    pub(crate) fn new(
        owner: &Entity<QuillApp>,
        picture: CallPicture,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            owner: owner.downgrade(),
            picture,
            // A change to the call (a feed starting or ending) redraws the
            // picture; frames come with the player's waker.
            _observe: cx.observe(owner, |_, _, cx| cx.notify()),
        }
    }
}

impl Render for CallPictureView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        trace_slice_render(match self.picture {
            CallPicture::Remote => "call-remote-picture",
            CallPicture::Local => "call-local-picture",
        });
        let picture = self.picture;
        let player = self
            .owner
            .upgrade()
            .and_then(|owner| owner.update(cx, |app, cx| app.call_picture_player(picture, cx)));
        let frame = div().size_full();
        let Some(player) = player else {
            return frame;
        };
        let video = video_frame(&player).object_fit(ObjectFit::Cover);
        match picture {
            CallPicture::Remote => frame.debug_selector(|| "call-remote-picture".into()),
            CallPicture::Local => frame.debug_selector(|| "call-local-picture".into()),
        }
        .child(match picture {
            CallPicture::Remote => video,
            // An image rounds itself (the preview is never a surface).
            CallPicture::Local => {
                video.corner_radii(Corners::all(px(PREVIEW_RADIUS)), call_chrome::surface())
            }
        })
    }
}

/// The call window's root: the ground, the two pictures and the panel.
pub(crate) struct CallWindowRoot {
    owner: WeakEntity<QuillApp>,
    _observe: Subscription,
    panel: Entity<CallPanel>,
    remote: Entity<CallPictureView>,
    local: Entity<CallPictureView>,
    /// Which pictures had a frame at the last draw: the panel lays out
    /// around them, so it is redrawn when that changes.
    drawn: (bool, bool),
}

impl CallWindowRoot {
    /// `panel_view` learns the panel's id, for the clock's own redraw.
    pub(crate) fn new(
        owner: &Entity<QuillApp>,
        panel_view: &Cell<Option<EntityId>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let panel = cx.new(|cx| CallPanel::new(owner, cx));
        panel_view.set(Some(panel.entity_id()));
        Self {
            owner: owner.downgrade(),
            _observe: cx.observe(owner, |_, _, cx| cx.notify()),
            panel,
            remote: cx.new(|cx| CallPictureView::new(owner, CallPicture::Remote, cx)),
            local: cx.new(|cx| CallPictureView::new(owner, CallPicture::Local, cx)),
            drawn: (false, false),
        }
    }
}

impl Render for CallWindowRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            window.remove_window();
            return div().into_any_element();
        };
        let (drawn, ground) = owner.update(cx, |app, cx| {
            // The players follow the call before anything reads them.
            let remote = app.call_picture_player(CallPicture::Remote, cx);
            let local = app.call_picture_player(CallPicture::Local, cx);
            let drawn = (
                remote.is_some() && app.call_picture_drawn(CallPicture::Remote),
                local.is_some() && app.call_picture_drawn(CallPicture::Local),
            );
            (
                drawn,
                (!drawn.0).then(|| app.call_peer_backdrop()).flatten(),
            )
        });
        if drawn != self.drawn {
            self.drawn = drawn;
            let panel = self.panel.entity_id();
            cx.defer(move |cx| cx.notify(panel));
        }
        let cached = || StyleRefinement::default().size_full();
        let (w, h) = PREVIEW_SIZE;
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(call_chrome::surface())
            .children(ground.map(|image| {
                img(ImageSource::Render(image))
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover)
            }))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .child(self.remote.clone().cached(cached())),
            )
            .when(drawn.1, |this| {
                this.child(
                    div()
                        .absolute()
                        .right(px(PREVIEW_RIGHT))
                        .bottom(px(PREVIEW_BOTTOM))
                        .w(px(w))
                        .h(px(h))
                        .rounded(px(PREVIEW_RADIUS))
                        .shadow_lg()
                        .child(self.local.clone().cached(cached())),
                )
            })
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .child(self.panel.clone().cached(cached())),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` above brings its own `test` attribute; these are plain.
    use core::prelude::v1::test;
    use quill::calls::engine::RemoteVideoState::{Active, Inactive, Paused};
    use std::time::Instant;

    fn call(
        state: CallState,
        remote_video: RemoteVideoState,
        remote_screen: RemoteVideoState,
    ) -> ActiveCall {
        ActiveCall {
            id: 7,
            user_id: 11,
            is_outgoing: true,
            is_video: true,
            state,
            started_at: Instant::now(),
            ready_at: None,
            ready: None,
            transport: None,
            transport_error: None,
            signaling_queue: Vec::new(),
            signaling_dropped: 0,
            muted: false,
            camera_on: true,
            screen_sharing: false,
            remote_video,
            remote_screen,
            remote_audio_muted: false,
            remote_battery_low: false,
            signal_bars: None,
        }
    }

    fn key(local: bool, screen: bool) -> Option<LiveKey> {
        Some(LiveKey::Call {
            call_id: 7,
            local,
            screen,
        })
    }

    #[test]
    fn the_peer_shows_their_screen_before_their_camera() {
        let remote = |video, screen| {
            call_picture_key(
                &call(CallState::Ready, video, screen),
                CallPicture::Remote,
                true,
            )
        };
        assert_eq!(remote(Active, Inactive), key(false, false));
        assert_eq!(remote(Active, Active), key(false, true));
        assert_eq!(remote(Inactive, Active), key(false, true));
        // Paused or off: their photo.
        assert_eq!(remote(Paused, Inactive), None);
        assert_eq!(remote(Inactive, Paused), None);
        assert_eq!(remote(Inactive, Inactive), None);
    }

    #[test]
    fn nothing_from_the_peer_before_the_call_is_up() {
        let ringing = call(CallState::ExchangingKeys, Active, Active);
        assert_eq!(call_picture_key(&ringing, CallPicture::Remote, true), None);
        // Your camera shows while it rings.
        assert_eq!(
            call_picture_key(&ringing, CallPicture::Local, true),
            key(true, false)
        );
    }

    #[test]
    fn your_preview_follows_the_camera() {
        let ready = call(CallState::Ready, Inactive, Inactive);
        assert_eq!(
            call_picture_key(&ready, CallPicture::Local, true),
            key(true, false)
        );
        assert_eq!(call_picture_key(&ready, CallPicture::Local, false), None);
    }
}
