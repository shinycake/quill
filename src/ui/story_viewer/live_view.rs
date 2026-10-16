//! The live story's frame in the story viewer: the stream picture fitted
//! as tdesktop's `DecideFrameResize` fits it, with the comments over its
//! lower half (`live_comments`), the pinned paid comments, the "Stars
//! Sent!" toast and the Stars effects; and the header's LIVE badge, viewer
//! count and sound toggle (tdesktop `Header::setVideoStreamViewers`).
#![deny(clippy::indexing_slicing)]

use super::live_comments::{MESSAGE_SKIP, live_pinned_strip};
use super::live_state::{effect_progress, level_color, pinned_comments};
use super::*;
use crate::ui::calls::stream_picture::{frame_backdrop, frame_image};
use crate::ui::calls::stream_tile::{StreamTile, frame_expands};
use crate::ui::message_text::kit_avatar_element;
use crate::ui::screen_chooser::split_bold;
use gpui_kit::assets::IconName;
use quill::calls::engine::{StreamState, VideoFrame};
use quill::lang::{trn, trn_with};
use quill::live_story::level_for_stars;
use quill::premium_hub::group_digits;
use quill::state::{Session, unix_ms_now};
use quill::telegram::envelope::{GroupCallMessageLevel, ParsedGroupCallMessage};
use std::sync::Arc;

/// The story frame (the viewer's media box).
pub(super) const FRAME_W: f32 = 360.;
pub(super) const FRAME_H: f32 = 640.;
/// The box the backdrop takes its proportions from.
const BACKDROP_BOX: (u32, u32) = (360, 640);
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

/// The red LIVE badge (`groupCallMessageBadge`: 11 px tall, radius 5,
/// 8 px semibold, `attentionButtonFg`).
pub(super) fn live_badge(cx: &App) -> Div {
    div()
        .flex_none()
        .h(px(11.))
        .px(px(3.))
        .rounded(px(5.))
        .bg(cx.theme().danger)
        .flex()
        .items_center()
        .text_size(px(8.))
        .line_height(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(chrome::fg())
        .child(tr(keys::VIDEO_STREAM_LIVE).to_uppercase())
}

/// The pictures to draw: the frame, and its blurred backdrop when the
/// frame is shown whole.
struct LivePictures {
    picture: Option<Arc<RenderImage>>,
    backdrop: Option<Arc<RenderImage>>,
    size: Option<(u16, u16)>,
}

impl QuillApp {
    /// The header's LIVE badge with " {count} viewers", and the sound
    /// toggle (tdesktop's volume button; Quill mutes only).
    pub(super) fn live_story_header(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.stories.live.watching?;
        let call = self.session()?.calls.active_group_call.as_ref()?;
        let count = call.viewer_count;
        let muted = self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.group_call_playback_muted());
        let viewers = (count > 0).then(|| {
            trn_with(
                keys::GROUP_CALL_RTMP_VIEWERS,
                i64::from(count),
                &[("count", &group_digits(i64::from(count)))],
            )
        });
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(live_badge(cx))
                .children(
                    viewers.map(|text| div().text_xs().text_color(chrome::secondary()).child(text)),
                )
                .child(
                    Button::new("live-story-sound")
                        .icon(if muted {
                            IconName::VolumeX
                        } else {
                            IconName::Volume2
                        })
                        .custom(chrome::ghost(cx))
                        .small()
                        .tab_index(0)
                        .accessibility_label(if muted {
                            tr(keys::Q_MEDIA_SOUND_OFF)
                        } else {
                            tr(keys::Q_MEDIA_SOUND_ON)
                        })
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_live_story_sound(cx))),
                )
                .into_any_element(),
        )
    }

    /// The newest picture of the live story and its backdrop, converted
    /// off the UI thread (one conversion at a time; the images from two
    /// frames back leave the atlas,
    /// [`StreamPicture`](crate::ui::calls::stream_picture)).
    fn live_story_pictures(&mut self, key: i64, cx: &mut Context<Self>) -> LivePictures {
        let seq = match self.live.as_ref() {
            Some(live) => live.driver.live_story_frame_seq(),
            None => self.stories.live.demo_frame.as_ref().map(|f| f.seq),
        };
        if let Some(seq) = seq
            && self.stories.live.picture.wants(key, seq)
        {
            let frame: Option<Arc<VideoFrame>> = match self.live.as_ref() {
                Some(live) => live.driver.live_story_frame(),
                None => self.stories.live.demo_frame.clone(),
            };
            // A screenshot fixture's still picture is converted at once so
            // the capture's first frame shows it.
            let inline = self.live.is_none() && !cfg!(test);
            if let Some(frame) = frame.as_ref().filter(|_| inline) {
                let (image, backdrop) = (frame_image(frame), frame_backdrop(frame, BACKDROP_BOX));
                self.finish_live_pictures(key, frame.seq, image, backdrop, cx);
            } else if let Some(frame) = frame {
                let seq = frame.seq;
                let task = cx.spawn(async move |this, cx| {
                    let (image, backdrop) = cx
                        .background_spawn(async move {
                            (frame_image(&frame), frame_backdrop(&frame, BACKDROP_BOX))
                        })
                        .await;
                    let _ = this.update(cx, |app, cx| {
                        app.finish_live_pictures(key, seq, image, backdrop, cx);
                        cx.notify();
                    });
                });
                self.stories.live.picture.start(key, seq, task);
            }
        }
        LivePictures {
            picture: self.stories.live.picture.current(key),
            backdrop: self.stories.live.backdrop.current(key),
            size: self
                .stories
                .live
                .picture
                .current(key)
                .map(|image| image.size(0))
                .and_then(|size| {
                    Some((
                        u16::try_from(size.width.0).ok()?,
                        u16::try_from(size.height.0).ok()?,
                    ))
                }),
        }
    }

    fn finish_live_pictures(
        &mut self,
        key: i64,
        seq: u64,
        image: Option<Arc<RenderImage>>,
        backdrop: Option<Arc<RenderImage>>,
        cx: &mut Context<Self>,
    ) {
        let live = &mut self.stories.live;
        // The inline path and the backdrop start no task of their own.
        live.picture.start_if_idle(key, seq);
        if let Some(old) = live.picture.finish(key, seq, image) {
            cx.drop_image(old, None);
        }
        live.backdrop.start_if_idle(key, seq);
        if let Some(old) = live.backdrop.finish(key, seq, backdrop) {
            cx.drop_image(old, None);
        }
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
        let pictures = self.live_story_pictures(key, cx);
        let session = self.session()?;
        let live = session.stories.live.as_ref()?;
        let call = session
            .calls
            .active_group_call
            .as_ref()
            .filter(|call| call.id == live.group_call_id);
        let unsupported = call.is_some_and(|call| call.stream == StreamState::Unsupported);
        let playing = call.is_some_and(|call| call.is_joined) && !unsupported;
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
        let expands = pictures.size.is_none_or(|(width, height)| {
            frame_expands((FRAME_W, FRAME_H), (f32::from(width), f32::from(height)))
        });
        let fit = if expands {
            ObjectFit::Cover
        } else {
            ObjectFit::Contain
        };
        let backdrop = (!expands)
            .then_some(pictures.backdrop)
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
                            .object_fit(ObjectFit::Fill),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .bg(hsla(0., 0., 0., BACKDROP_SHADE)),
                    )
            });
        let tile = StreamTile::new(("live-story-tile", key as u64), &self.stories.live.tile)
            .picture(pictures.picture)
            .fit(fit)
            .frame(px(6.), hsla(0., 0., 0., 0.))
            .playing(playing);
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
                .child(tile)
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
