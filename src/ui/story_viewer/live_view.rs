//! The live story's frame in the story viewer: the stream picture fitted
//! as tdesktop's `DecideFrameResize` fits it, with the comments over its
//! lower half (`live_comments`), the pinned paid comments and the "Stars
//! Sent!" toast; and the header's LIVE badge and viewer count (tdesktop
//! `Header::setVideoStreamViewers`) and its sound toggle, the live
//! controller's header layout (`VideoController::live`). The live
//! states (opening spinner, stall, paused, failed with Retry) go over the
//! picture from the same overlay code as the stream window.
#![deny(clippy::indexing_slicing)]

use super::live_comments::live_pinned_strip;
use super::live_state::pinned_comments;
use super::*;
use crate::ui::calls::stream_view::frame_expands;
use crate::ui::calls::stream_volume::VolumeHost;
use crate::ui::message_text::kit_avatar_element;
use crate::ui::screen_chooser::split_bold;
use crate::ui::video_player::controls::{
    LiveActions, LiveFacts, LiveInput, LiveOverlayParts, PictureOverlay, PictureState,
    VideoController, VolumeHoverHandler,
};
use crate::ui::video_player::{VideoPlayer, follow_live_slot, video_frame};
use gpui_kit::assets::IconName;
use gpui_kit::component::slider::SliderState;
use quill::calls::live_video::LiveSink;
use quill::lang::{trn, trn_with};
use quill::premium_hub::group_digits;
use quill::state::ActiveGroupCall;
use quill::state::{Session, unix_ms_now};
use quill::telegram::envelope::ParsedGroupCallMessage;
use std::rc::Rc;

/// The story frame (the viewer's media box).
pub(super) const FRAME_W: f32 = 360.;
pub(super) const FRAME_H: f32 = 640.;
/// tdesktop `FillBlurredBackground` darkens the backdrop by 48 / 255.
const BACKDROP_SHADE: f32 = 48. / 255.;

/// A sender's avatar: photo when downloaded, else initials.
pub(super) fn sender_avatar(session: &Session, sender: MessageSender, size: f32) -> AnyElement {
    let name = session.sender_label(sender);
    let photo = match sender {
        MessageSender::User { user_id } => session.user_photo_path(user_id),
        MessageSender::Chat { chat_id } => session.chat_photo_path(ChatId(chat_id)),
    };
    kit_avatar_element(&name, photo.map(std::path::Path::new), px(size))
}

impl QuillApp {
    /// The stream being watched when it is the story the viewer shows, not
    /// one playing on in Picture-in-Picture behind another story.
    pub(in crate::ui) fn live_story_shown(&self) -> Option<(ChatId, i32)> {
        let shown = self.stories.viewer.current().map(StoryViewerItem::key);
        self.stories
            .live
            .watching
            .filter(|watched| shown == Some(*watched))
    }

    pub(super) fn live_story_volume(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<Entity<SliderState>> {
        self.live_story_shown()?;
        let call_id = self
            .session()?
            .calls
            .story_stream
            .as_ref()
            .map(|call| call.id);
        self.stream_volume_slider(VolumeHost::LiveStory, call_id, cx)
    }

    /// The viewer count of the stream being watched, which puts the LIVE
    /// badge and " {count} viewers" in the header's second line in place
    /// of the story's age (tdesktop `Header::setVideoStreamViewers`).
    pub(super) fn live_story_viewers(&self) -> Option<u32> {
        self.live_story_shown()?;
        Some(self.session()?.calls.story_stream.as_ref()?.viewer_count)
    }

    /// The header's sound toggle (tdesktop's volume button) and PiP: the
    /// live controller's header.
    pub(super) fn live_story_header(
        &self,
        volume: Option<Entity<SliderState>>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.live_story_shown()?;
        let call = self.session()?.calls.story_stream.as_ref()?;
        let muted = self.live_story_muted();
        let view = cx.entity().downgrade();
        let pip_view = view.clone();
        let pill_view = view.clone();
        let volume_hover = volume.is_some().then(|| -> VolumeHoverHandler {
            Rc::new(move |part, inside, _, cx| {
                let _ = pill_view.update(cx, |this, cx| {
                    let now = cx.background_executor().now();
                    let pill = &mut this.stories.live.volume_pill;
                    let was = pill.open(now);
                    if let Some(delay) = pill.hover(part, inside, now) {
                        cx.spawn(async move |this, cx| {
                            cx.background_executor().timer(delay).await;
                            let _ = this.update(cx, |_, cx| cx.notify());
                        })
                        .detach();
                    }
                    if pill.open(now) != was {
                        cx.notify();
                    }
                });
            })
        });
        let now = cx.background_executor().now();
        let facts = LiveFacts {
            muted,
            volume_focus: volume
                .is_some()
                .then(|| self.stories.live.volume_focus.clone()),
            volume,
            volume_open: self.stories.live.volume_pill.open(now),
            fullscreen: false,
        };
        let actions = LiveActions {
            toggle_mute: Rc::new(move |_, cx| {
                let _ = view.update(cx, |this, cx| this.toggle_live_story_sound(cx));
            }),
            fullscreen: None,
            pip: Some(Rc::new(move |window, cx| {
                let _ = pip_view.update(cx, |this, cx| this.open_story_pip(window, cx));
            })),
            hover: None,
            volume_hover,
        };
        Some(
            div()
                .text_xs()
                .text_color(chrome::secondary())
                .child(
                    VideoController::live(u64::from(call.id.unsigned_abs()), facts, actions)
                        .header()
                        .button_variant(chrome::ghost(cx)),
                )
                .into_any_element(),
        )
    }

    /// The live states over the story's picture (no placeholder: the
    /// story frame's background stands in), redrawn when a delay is up.
    fn live_story_overlay(
        &mut self,
        player: Option<&VideoPlayer>,
        row_id: u64,
        cx: &mut Context<Self>,
    ) -> Option<PictureOverlay> {
        let failed = self
            .session()
            .and_then(|s| s.calls.story_stream.as_ref())
            .is_some_and(ActiveGroupCall::join_failed);
        let input = LiveInput {
            state: player.map(VideoPlayer::state),
            idle: false,
            failed,
        };
        let now = cx.background_executor().now();
        let (overlay, due) = self.stories.live.overlay.observe(input, now);
        if let Some(due) = due {
            let wait = due.saturating_duration_since(now);
            let timer = cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            });
            self.stories.live.overlay.arm(due, timer);
        }
        let leave = cx.entity().downgrade();
        let retry = cx.entity().downgrade();
        let parts = LiveOverlayParts {
            placeholder: None,
            wide: true,
            // tdesktop closes the viewer when the stream is over.
            on_leave: Some(Rc::new(move |_, cx| {
                let _ = leave.update(cx, |this, cx| this.close_story_viewer(cx));
            })),
            on_retry: Some(Rc::new(move |_, cx| {
                let _ = retry.update(cx, |this, cx| this.retry_live_story(cx));
            })),
            detail: None,
        };
        PictureState::live(overlay, parts).map(|state| PictureOverlay::new(row_id, state))
    }

    /// The watched live story's feed: the driver's (the stream, or the
    /// presenter's camera or screen), or the demo fixture's.
    pub(crate) fn live_story_sink(&self) -> Option<LiveSink> {
        match self.live.as_ref() {
            Some(live) => live
                .driver
                .live_story_live_key()
                .and_then(|key| live.driver.live_sink(key)),
            None => self
                .stories
                .live
                .demo_feed
                .as_ref()
                .map(|feed| feed.sink.clone()),
        }
    }

    /// The live story's player, following its feed: a new feed (a
    /// rejoin, a presenter switch) gets a new player; no feed, no player.
    fn live_story_player(&mut self, cx: &mut App) -> Option<VideoPlayer> {
        let sink = self.live_story_sink();
        let live = &mut self.stories.live;
        follow_live_slot(&mut live.player, sink.as_ref(), cx);
        live.player.clone()
    }

    /// The live story's frame, or `None` while the viewer has not joined
    /// it (the Join button stays).
    pub(super) fn live_story_media(
        &mut self,
        item: &StoryViewerItem,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.stories.live.watching != Some((item.chat_id, item.story_id)) {
            return None;
        }
        let key = item.chat_id.0;
        let player = self.live_story_player(cx);
        let session = self.session()?;
        let live = session.stories.live.as_ref()?;
        let call = session
            .calls
            .story_stream
            .as_ref()
            .filter(|call| call.id == live.group_call_id);
        let levels = session.live_levels();
        let now = unix_ms_now() / 1000;
        let now = i64::try_from(now).unwrap_or(i64::MAX);
        let messages: &[ParsedGroupCallMessage] = call.map_or(&[], |call| &call.messages);
        let pinned = pinned_comments(messages, &levels, now);
        let comments = live
            .comments
            .shown()
            .then(|| self.live_comments(session, messages, &levels, cx));
        let toast = self.live_stars_toast(session, cx);
        let pinned_strip = (!pinned.is_empty()).then(|| live_pinned_strip(session, &pinned, cx));
        // tdesktop `DecideFrameResize`: cover when little is cut off, else
        // the whole picture over its blurred copy.
        let size = player
            .as_ref()
            .filter(|player| player.has_live_frame())
            .and_then(VideoPlayer::picture_size);
        let expands = size.is_none_or(|(width, height)| {
            frame_expands((FRAME_W, FRAME_H), (width as f32, height as f32))
        });
        let fit = if expands {
            ObjectFit::Cover
        } else {
            ObjectFit::Contain
        };
        // The last frame small and blurred behind a picture shown whole,
        // cropped to the frame's shape from its center as tdesktop's
        // `PrepareBlurredBackground` does (stretched, the band above the
        // picture matched nothing in it).
        let backdrop = (!expands)
            .then(|| player.as_ref().and_then(VideoPlayer::paused_picture))
            .flatten()
            .map(|image| {
                div()
                    .debug_selector(|| "live-story-backdrop".into())
                    .absolute()
                    .inset_0()
                    .child(
                        img(ImageSource::Render(image))
                            .absolute()
                            .inset_0()
                            .size_full()
                            .object_fit(ObjectFit::Cover),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .bg(hsla(0., 0., 0., BACKDROP_SHADE)),
                    )
            });
        let tile = player.as_ref().map(|player| {
            div()
                .id(("live-story-tile", key as u64))
                .absolute()
                .inset_0()
                .child(
                    video_frame(player)
                        .object_fit(fit)
                        .corner_radii(Corners::all(px(6.)), hsla(0., 0., 0., 0.)),
                )
        });
        let overlay = self.live_story_overlay(player.as_ref(), key as u64, cx);
        Some(
            div()
                .id(("live-story-frame", key as u64))
                .debug_selector(|| "live-story-frame".into())
                // Clicks on the stream stay here: the backdrop behind the
                // panel would close the viewer.
                .occlude()
                .relative()
                .w(px(FRAME_W))
                .h(px(FRAME_H))
                .rounded(px(6.))
                .overflow_hidden()
                .bg(chrome::compose_bg())
                .children(backdrop)
                .children(tile)
                .children(overlay)
                .child(
                    div()
                        .absolute()
                        .top(px(8.))
                        .left(px(8.))
                        .right(px(8.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .children(toast)
                        .children(pinned_strip),
                )
                .children(comments)
                .into_any_element(),
        )
    }

    /// "Stars Sent!" with Undo and the seconds left, while your Stars wait
    /// (tdesktop `PaidReactionToast`). It goes once they are due: the
    /// Stars are being sent and Undo could not take them back.
    fn live_stars_toast(&self, session: &Session, cx: &mut Context<Self>) -> Option<AnyElement> {
        let now = unix_ms_now();
        let pending = session
            .stories
            .live
            .as_ref()?
            .pending_reaction
            .filter(|pending| pending.commit_at_ms > now)?;
        let seconds = pending.seconds_left(now);
        let (text, bold) = split_bold(&trn_with(
            keys::PAID_REACT_TOAST_TEXT,
            pending.stars,
            &[("count", &group_digits(pending.stars))],
        ));
        let style = HighlightStyle {
            font_weight: Some(FontWeight::SEMIBOLD),
            ..Default::default()
        };
        let highlights: Vec<_> = bold.into_iter().map(|range| (range, style)).collect();
        Some(
            div()
                .id("live-story-toast")
                .debug_selector(|| "live-story-toast".into())
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(12.))
                .py(px(6.))
                .rounded(px(8.))
                .bg(hsla(0., 0., 0., 0.7))
                .text_color(chrome::fg())
                .child(
                    Icon::new(IconName::StarFill)
                        .with_size(px(24.))
                        .text_color(chrome::stars_gold()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(trn(keys::PAID_REACT_TOAST, pending.stars)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .child(StyledText::new(text).with_highlights(highlights)),
                        ),
                )
                .child(
                    div()
                        .id("live-story-undo")
                        .debug_selector(|| "live-story-undo".into())
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .cursor_pointer()
                        .role(gpui_kit::Role::Button)
                        .tab_index(0)
                        .aria_label(tr(keys::PAID_REACT_UNDO))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(chrome::blue())
                                .child(tr(keys::PAID_REACT_UNDO)),
                        )
                        .child(countdown_ring(seconds, pending.elapsed_fraction(now)))
                        .on_click(cx.listener(|this, _, _, cx| this.undo_live_stars(cx))),
                )
                .into_any_element(),
        )
    }
}

/// The seconds left in a ring that empties (tdesktop's Undo countdown).
fn countdown_ring(seconds: u64, gone: f32) -> AnyElement {
    let left = (1.0 - gone).clamp(0.0, 1.0);
    div()
        .relative()
        .size(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, (), window, _| {
                    let center = bounds.center();
                    let radius = f32::from(bounds.size.width) / 2. - 1.5;
                    let mut path = PathBuilder::stroke(px(2.));
                    let steps = 32;
                    let sweep = left * std::f32::consts::TAU;
                    for step in 0..=steps {
                        let angle =
                            -std::f32::consts::FRAC_PI_2 + sweep * step as f32 / steps as f32;
                        let point = point(
                            center.x + px(angle.cos() * radius),
                            center.y + px(angle.sin() * radius),
                        );
                        if step == 0 {
                            path.move_to(point);
                        } else {
                            path.line_to(point);
                        }
                    }
                    if left > 0.0
                        && let Ok(path) = path.build()
                    {
                        window.paint_path(path, chrome::blue());
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
        .child(seconds.to_string())
        .into_any_element()
}
