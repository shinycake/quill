//! The live stream's picture in the call window: its own view, so a new
//! frame redraws only the picture while the panel above it (title,
//! controls, labels) replays its cached frame.
//!
//! The picture is a live player (`VideoPlayer::live`) over the stream's
//! feed. Before the first picture the view shows the chat's photo
//! blurred, as tdesktop's viewport draws a track with no frame yet
//! (`Viewport::RendererSW::validateUserpicFrame`, radius 15), or the
//! chat's initials when it has no photo. The element reports its width
//! whenever it changes, so the driver can pick the quality bucket
//! (tdesktop `updateRequestedVideoChannels`).
//!
//! Over the picture go the live controller's states
//! (`video_player::controls`, `LiveOverlayClock`): the spinner after
//! 500 ms while opening, the dimmed picture with the spinner and
//! "Connecting…" while stalled, the paused shadow, the ended plate with
//! Leave and the failed card with Retry. The view keeps the clock and
//! redraws itself when a delay is up.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::chat_row::initials_avatar;
use crate::ui::frame_clock::{follow_app, trace_slice_render};
use crate::ui::group_call_panel::MEMBERS_BG;
use crate::ui::group_call_stream_layout::stream_full;
use crate::ui::video_player::controls::{
    LiveInput, LiveOverlayClock, LiveOverlayParts, PictureOverlay, PictureState,
};
use crate::ui::video_player::{VideoPlayer, WidthReport, follow_live_slot, video_frame};
use gpui_kit::*;
use quill::calls::engine::StreamState;
use quill::calls::live_video::{LiveKey, LiveSink};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

/// tdesktop `kBlurRadius` (calls_group_viewport_raster.cpp).
pub(crate) const PHOTO_BLUR_RADIUS: u32 = 15;
/// The initials stand-in for a chat without a photo.
const INITIALS_SIZE: f32 = 96.;

/// What the picture view draws, read from the app once per draw.
pub(crate) struct StreamPictureParts {
    pub(crate) chat_id: i64,
    pub(crate) player: Option<VideoPlayer>,
    /// The blurred photo, once made.
    pub(crate) photo: Option<Arc<RenderImage>>,
    /// The chat has a photo (blurring, or blurred).
    pub(crate) has_photo: bool,
    pub(crate) name: String,
    /// Nothing is expected (not joined, nobody broadcasting, not
    /// playable here): the placeholder shows without a spinner.
    pub(crate) idle: bool,
    /// The joined call's connect reported this error.
    pub(crate) failure: Option<String>,
}

impl QuillApp {
    /// The stream's feed: the driver's, or a demo fixture's.
    pub(crate) fn stream_sink(&self, chat_id: i64) -> Option<LiveSink> {
        match self.live.as_ref() {
            Some(live) => live.driver.live_sink(LiveKey::Stream { chat_id }),
            None => self
                .group_call
                .stream
                .demo_feed
                .as_ref()
                .map(|feed| feed.sink.clone()),
        }
    }

    /// The stream picture's parts, with the player following the feed: a
    /// new feed (a rejoin) gets a new player; no feed, no player.
    pub(crate) fn stream_picture_parts(&mut self, cx: &mut App) -> Option<StreamPictureParts> {
        let facts = self.group_call_stream_facts()?;
        let (chat_id, name) = facts.chat?;
        let sink = self.stream_sink(chat_id);
        let has_photo = self
            .session()
            .and_then(|s| s.chat_photo_path(quill::ids::ChatId(chat_id)))
            .is_some();
        let idle = !facts.joined || facts.stream == StreamState::NoStream;
        let failure = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            // The stream's own connect failed: the error is shared with
            // later failures (a screen share, a video subscribe) once the
            // transport is up.
            .filter(|call| call.is_joined && !call.transport_ready)
            .and_then(|call| call.transport_error.clone());
        let stream = &mut self.group_call.stream;
        follow_live_slot(&mut stream.player, sink.as_ref(), cx);
        Some(StreamPictureParts {
            chat_id,
            player: stream.player.clone(),
            photo: stream.photo.as_ref().and_then(|(_, image)| image.clone()),
            has_photo,
            name,
            idle,
            failure,
        })
    }

    /// The failed card's Retry: the card shows only for a joined call, so
    /// Retry always rejoins (never a second join on a joined call).
    pub(crate) fn retry_stream(&mut self, cx: &mut Context<Self>) {
        let joined = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .is_some_and(|call| call.is_joined);
        if joined {
            self.rejoin_active_group_call(cx);
        }
    }
}

/// The stream window's picture.
pub(crate) struct StreamPictureView {
    owner: WeakEntity<QuillApp>,
    _observe: Subscription,
    /// When the overlay's phase began, and its armed redraw.
    pub(crate) overlay: LiveOverlayClock,
}

impl StreamPictureView {
    pub(crate) fn new(owner: &Entity<QuillApp>, cx: &mut Context<Self>) -> Self {
        Self {
            owner: owner.downgrade(),
            // Every change to QuillApp redraws the picture's view (not the
            // panel), as the stream window's root does: the parts it reads
            // (the feed, the photo, the name) have no narrower signal.
            // Frames themselves come with the player's waker.
            _observe: follow_app(owner, cx),
            overlay: LiveOverlayClock::default(),
        }
    }
}

impl Render for StreamPictureView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        trace_slice_render("stream-picture");
        let full = self
            .owner
            .upgrade()
            .is_some_and(|owner| stream_full(window, owner.read(cx)));
        let (radius, background): (Pixels, Hsla) = if full {
            (px(0.), black())
        } else {
            (px(6.), rgb(MEMBERS_BG).into())
        };
        let frame = div()
            .id("group-call-stream-tile")
            .debug_selector(|| "group-call-stream-tile".into())
            .relative()
            .size_full()
            .overflow_hidden()
            .rounded(radius)
            .bg(background);
        let Some(owner) = self.owner.upgrade() else {
            return frame;
        };
        let Some(parts) = owner.update(cx, |app, cx| app.stream_picture_parts(cx)) else {
            return frame;
        };
        let drawn = parts
            .player
            .as_ref()
            .is_some_and(VideoPlayer::has_live_frame);
        let input = LiveInput {
            state: parts.player.as_ref().map(VideoPlayer::state),
            idle: parts.idle,
            failed: parts.failure.is_some(),
        };
        let now = cx.background_executor().now();
        let (overlay, due) = self.overlay.observe(input, now);
        if let Some(due) = due {
            let wait = due.saturating_duration_since(now);
            let timer = cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            });
            self.overlay.arm(due, timer);
        }
        let leave_owner = self.owner.clone();
        let retry_owner = self.owner.clone();
        let overlay_parts = LiveOverlayParts {
            placeholder: (!drawn).then(|| placeholder(&parts)),
            wide: true,
            on_leave: Some(Rc::new(move |window, cx| {
                let _ = leave_owner.update(cx, |app, cx| {
                    if let Some(facts) = app.group_call_stream_facts() {
                        app.stream_leave_clicked(&facts, window, cx);
                    }
                });
            })),
            on_retry: Some(Rc::new(move |_, cx| {
                let _ = retry_owner.update(cx, |app, cx| app.retry_stream(cx));
            })),
            detail: parts.failure.clone().map(SharedString::from),
        };
        let state = PictureState::live(overlay, overlay_parts);
        // The placeholder goes under the picture when the overlay does not
        // draw it (a sender with no video yet, say).
        let placeholder = (!drawn && !matches!(state, Some(PictureState::LiveOpening { .. })))
            .then(|| placeholder(&parts));
        let overlay = state.map(|state| PictureOverlay::new(parts.chat_id as u64, state));
        let report: WidthReport = {
            let owner = self.owner.clone();
            let chat_id = parts.chat_id;
            Rc::new(move |width, cx: &mut App| {
                let _ = owner.update(cx, |app, _| {
                    if let Some(live) = app.live.as_mut() {
                        live.driver.report_stream_tile_width(chat_id, width);
                    }
                });
            })
        };
        let picture = parts.player.as_ref().map(|player| {
            video_frame(player)
                .object_fit(ObjectFit::Contain)
                .corner_radii(Corners::all(radius), background)
                .on_width(report)
        });
        frame
            .children(placeholder)
            .children(picture)
            .children(overlay)
    }
}

/// The blurred photo fitted like the picture, or the chat's initials.
fn placeholder(parts: &StreamPictureParts) -> AnyElement {
    match parts.photo.clone() {
        Some(photo) => img(ImageSource::Render(photo))
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        // Blurring: nothing yet rather than a flash of initials.
        None if parts.has_photo => div().into_any_element(),
        None => div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .child(initials_avatar(&parts.name, INITIALS_SIZE))
            .into_any_element(),
    }
}

/// tdesktop `DecideFrameResize`: whether a picture fills the box
/// (`expanding`, drawn `Cover`): when that crops nothing across and at
/// most a quarter of its height. Otherwise it is shown whole (`Contain`)
/// over its blurred copy.
pub(crate) fn frame_expands(outer: (f32, f32), original: (f32, f32)) -> bool {
    let (outer_w, outer_h) = outer;
    let (width, height) = original;
    if outer_w <= 0. || outer_h <= 0. || width <= 0. || height <= 0. {
        return true;
    }
    // `scaled(outer, KeepAspectRatioByExpanding)`.
    let scale = (outer_w / width).max(outer_h / height);
    let (big_w, big_h) = (width * scale, height * scale);
    // A rounding slack of half a pixel, as Qt rounds the sizes.
    big_w <= outer_w + 0.5 && big_h * 3. <= outer_h * 4. + 0.5
}

/// The photo blurred as tdesktop's tile placeholder: decoded, blurred with
/// radius 15 (lib_ui `BlurLargeImage` weights, `wallpaper_raster::blur`),
/// as a GPUI image (BGRA). Runs off the UI thread.
pub(crate) fn blurred_photo(path: &Path) -> Option<Arc<RenderImage>> {
    let rgba = image::open(path).ok()?.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    let mut bgra = rgba.into_raw();
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let mut raster = quill::wallpaper_raster::Raster {
        width,
        height,
        bgra,
    };
    quill::wallpaper_raster::blur(&mut raster, PHOTO_BLUR_RADIUS);
    let image = image::RgbaImage::from_raw(raster.width, raster.height, raster.bgra)?;
    Some(Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(image),
    ]))))
}

#[cfg(test)]
mod tests {
    use super::{blurred_photo, frame_expands};

    #[test]
    fn a_picture_fills_the_story_only_when_little_is_cut_off() {
        let story = (360., 640.);
        // Portrait 9:16 fills it.
        assert!(frame_expands(story, (720., 1280.)));
        // 3:4 would be cut at the sides: shown whole.
        assert!(!frame_expands(story, (960., 1280.)));
        // Landscape is shown whole over the backdrop.
        assert!(!frame_expands(story, (1280., 720.)));
        // A tall picture loses at most a quarter of its height.
        assert!(frame_expands(story, (360., 800.)));
        assert!(!frame_expands(story, (360., 900.)));
    }

    #[test]
    fn a_photo_blurs_and_a_missing_one_does_not() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("docs/screenshots/fixtures/showcase/av07.png");
        let image = blurred_photo(&fixture).expect("decodes");
        let size = image.size(0);
        assert!(size.width.0 > 0 && size.height.0 > 0);
        assert!(blurred_photo(std::path::Path::new("/nonexistent.png")).is_none());
    }
}
