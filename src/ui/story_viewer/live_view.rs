//! The live story's frame in the story viewer: the stream picture fitted
//! as tdesktop's `DecideFrameResize` fits it, with the comments over its
//! lower half (`live_comments`), the pinned paid comments, the "Stars
//! Sent!" toast and the Stars effects; and the header's LIVE badge, viewer
//! count and sound toggle (tdesktop `Header::setVideoStreamViewers`), the
//! live controller's header layout (`VideoController::live`). The live
//! states (opening spinner, stall, paused, failed with Retry) go over the
//! picture from the same overlay code as the stream window.
#![deny(clippy::indexing_slicing)]

use super::live_comments::{MESSAGE_SKIP, live_pinned_strip};
use super::live_state::{effect_progress, level_color, pinned_comments};
use super::*;
use crate::ui::calls::stream_view::frame_expands;
use crate::ui::message_text::kit_avatar_element;
use crate::ui::screen_chooser::split_bold;
use crate::ui::video_player::controls::{
    LiveActions, LiveFacts, LiveInput, LiveOverlayParts, PictureOverlay, PictureState,
    VideoController,
};
use crate::ui::video_player::{Purpose, VideoPlayer, video_frame};
use gpui_kit::assets::IconName;
use quill::calls::engine::StreamState;
use quill::calls::live_video::LiveSink;
use quill::lang::{trn, trn_with};
use quill::live_story::level_for_stars;
use quill::premium_hub::group_digits;
use quill::state::{Session, unix_ms_now};
use quill::telegram::envelope::{GroupCallMessageLevel, ParsedGroupCallMessage};
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
    /// The header's LIVE badge with " {count} viewers", and the sound
    /// toggle (tdesktop's volume button; Quill mutes only: ntgcalls has
    /// no gain for the stream's sound): the live controller's header.
    pub(super) fn live_story_header(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.stories.live.watching?;
        let call = self.session()?.calls.active_group_call.as_ref()?;
        let muted = self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.group_call_playback_muted());
        let view = cx.entity().downgrade();
        let pip_view = view.clone();
        let facts = LiveFacts {
            viewers: call.viewer_count,
            muted,
            volume: None,
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
        unsupported: bool,
        row_id: u64,
        cx: &mut Context<Self>,
    ) -> Option<PictureOverlay> {
        let input = LiveInput {
            state: player.map(VideoPlayer::state),
            idle: unsupported,
            failed: false,
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

    /// The live story's player, following its feed: a new feed (a
    /// rejoin) gets a new player; no feed, no player.
    fn live_story_player(&mut self, cx: &mut App) -> Option<VideoPlayer> {
        let sink: Option<LiveSink> = match self.live.as_ref() {
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
        };
        let live = &mut self.stories.live;
        let same = match (live.player.as_ref(), sink.as_ref()) {
            (Some(player), Some(sink)) => player.live_sink().is_some_and(|own| own.same_as(sink)),
            (None, None) => true,
            _ => false,
        };
        if !same {
            if let Some(old) = live.player.take() {
                old.release_live(cx);
            }
            live.player =
                sink.map(|sink| VideoPlayer::live(sink, Purpose::Live { tile: false }, cx));
        }
        live.player.clone()
    }

    /// The live story's frame, or `None` while the viewer has not joined
    /// it (the Join button stays).
    pub(super) fn live_story_media(
        &mut self,
        item: &StoryViewerItem,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Not joined while another call holds the slot: the frame asks.
        if self.stories.live.watching != Some((item.chat_id, item.story_id))
            || self.stories.live.leave_other.is_some()
        {
            return None;
        }
        let key = item.chat_id.0;
        let player = self.live_story_player(cx);
        let session = self.session()?;
        let live = session.stories.live.as_ref()?;
        let call = session
            .calls
            .active_group_call
            .as_ref()
            .filter(|call| call.id == live.group_call_id);
        let unsupported = call.is_some_and(|call| call.stream == StreamState::Unsupported);
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
        let effects = self.live_effects_layer(session, &levels, cx);
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
        let overlay = self.live_story_overlay(player.as_ref(), unsupported, key as u64, cx);
        let unsupported_label = unsupported.then(|| {
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .px(px(24.))
                .text_sm()
                .text_center()
                .text_color(chrome::fg())
                .child(tr(keys::Q_LIVE_STORY_UNSUPPORTED))
        });
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
                .children(unsupported_label)
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
                .children(effects)
                .into_any_element(),
        )
    }

    /// Another call holds the call slot (Quill has one; see
    /// `live_story`): the frame asks, in place of the picture, whether to
    /// leave it, with tdesktop's `confirmLeaveCurrent` question.
    /// "Leave call and watch" leaves the other call and joins the story
    /// once the slot is free; Cancel (or Esc) brings the Join button back,
    /// which asks again.
    pub(super) fn live_leave_other_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(text) = self.stories.live.leave_other else {
            return div().into_any_element();
        };
        let question = super::live_stars_box::leave_other_question(text);
        div()
            .id("live-story-leave-other-card")
            .debug_selector(|| "live-story-leave-other-card".into())
            // Its buttons' presses stop here: not the backdrop's close nor
            // the media's hold-to-pause.
            .occlude()
            .role(Role::Group)
            .w(px(296.))
            .p(px(16.))
            .rounded(px(10.))
            .bg(chrome::compose_bg())
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .debug_selector(|| "live-story-leave-other".into())
                    .text_sm()
                    .text_color(chrome::fg())
                    .whitespace_normal()
                    .child(tr(question)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        Button::new("live-story-leave-other-cancel")
                            .debug_selector(|| "live-story-leave-other-cancel".into())
                            .label(tr(keys::CANCEL))
                            .custom(chrome::ghost(cx))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_live_leave_other(cx);
                            })),
                    )
                    .child(
                        Button::new("live-story-leave-other-confirm")
                            .debug_selector(|| "live-story-leave-other-confirm".into())
                            .label(tr(keys::Q_MEDIA_LIVE_LEAVE_AND_WATCH))
                            .custom(chrome::attention(cx))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.leave_other_call_for_live_story(cx);
                            })),
                    ),
            )
            .into_any_element()
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

    /// Stars someone added rise from the Stars button and fade (tdesktop
    /// `StarEffect`, 2 s): a badge with the sender and the count, and a
    /// burst of small stars around it in place of tdesktop's Lottie.
    fn live_effects_layer(
        &self,
        session: &Session,
        levels: &[GroupCallMessageLevel],
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.stories.live.flying.is_empty() {
            return None;
        }
        let now = Instant::now();
        let mut layer = div()
            .absolute()
            .right(px(8.))
            .bottom(px(0.))
            .w(px(140.))
            .h(px(FRAME_H / 2.));
        let mut running = false;
        for (index, effect) in self.stories.live.flying.iter().enumerate() {
            let Some(progress) = effect_progress(effect.started, now) else {
                continue;
            };
            running = true;
            let color = level_for_stars(levels, effect.stars).first_color;
            let fade = if progress > 0.7 {
                (1.0 - progress) / 0.3
            } else {
                1.0
            };
            let rise = progress * (FRAME_H / 2. - 40.);
            let right = MESSAGE_SKIP + (index % 2) as f32 * 24.;
            let burst = (0..6).map(move |ray| {
                let angle = ray as f32 * std::f32::consts::TAU / 6.0 + progress;
                let reach = 10. + progress * 36.;
                div()
                    .absolute()
                    .left(px(30. + angle.cos() * reach))
                    .top(px(8. + angle.sin() * reach))
                    .text_size(px(10.))
                    .text_color(chrome::stars_gold())
                    .opacity((1.0 - progress).clamp(0.0, 1.0))
                    .child("\u{2605}")
            });
            layer = layer.child(
                div()
                    .absolute()
                    .right(px(right))
                    .bottom(px(rise))
                    .opacity(fade.clamp(0.0, 1.0))
                    .child(
                        div()
                            .relative()
                            .flex()
                            .items_center()
                            .gap(px(3.))
                            .pl(px(1.))
                            .pr(px(6.))
                            .py(px(1.))
                            .rounded_full()
                            .bg(level_color(color, 1.0))
                            .text_color(chrome::fg())
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(sender_avatar(session, effect.sender, 18.))
                            .child(format!("\u{2605}{}", group_digits(effect.stars)))
                            .children(burst),
                    ),
            );
        }
        if running {
            self.request_animation_tick(60, cx);
        }
        Some(layer.into_any_element())
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
