//! The live story's frame in the story viewer: the stream picture (Cover)
//! with the comments over its lower half, the pinned paid comments, the
//! "Stars Sent!" toast and the Stars effects; and the header's LIVE badge,
//! viewer count and sound toggle (tdesktop `Header::setVideoStreamViewers`,
//! `Calls::Group::MessagesUi` in video stream mode).
#![deny(clippy::indexing_slicing)]

use super::live_state::{
    ADMIN_BADGE_OPACITY, ADMIN_BG_OPACITY, DARK_OVER_OPACITY, PAID_BG_OPACITY, Pinned,
    effect_progress, level_color, pinned_comments,
};
use super::*;
use crate::ui::calls::stream_picture::frame_image;
use crate::ui::calls::stream_tile::StreamTile;
use crate::ui::group_call_panel::BG as GROUP_CALL_BG;
use crate::ui::message_text::kit_avatar_element;
use crate::ui::screen_chooser::split_bold;
use gpui_kit::assets::IconName;
use quill::calls::engine::{StreamState, VideoFrame};
use quill::lang::{trn, trn_with};
use quill::live_story::{MAX_TOP_SHOWN, level_for_stars};
use quill::premium_hub::group_digits;
use quill::state::{Session, unix_ms_now};
use quill::telegram::envelope::{GroupCallMessageLevel, ParsedGroupCallMessage};
use std::sync::Arc;

/// The story frame (the viewer's media box).
pub(super) const FRAME_W: f32 = 360.;
pub(super) const FRAME_H: f32 = 640.;
/// tdesktop `groupCallMessagePadding` and `groupCallMessageSkip`.
const MESSAGE_SKIP: f32 = 8.;
/// tdesktop `groupCallUserpic` / `groupCallPinnedUserpic`.
const USERPIC: f32 = 20.;
const PINNED_USERPIC: f32 = 22.;
/// tdesktop `groupCallPinnedMaxWidth`.
const PINNED_MAX_TEXT: f32 = 96.;
/// The comment text size (`messageTextStyle`).
const TEXT_SIZE: f32 = 13.;

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
        .text_color(rgb(0xffffff))
        .child(tr(keys::VIDEO_STREAM_LIVE).to_uppercase())
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
                .children(viewers.map(|text| div().text_xs().text_color(text_bright()).child(text)))
                .child(
                    Button::new("live-story-sound")
                        .icon(if muted {
                            IconName::VolumeX
                        } else {
                            IconName::Volume2
                        })
                        .ghost()
                        .small()
                        .text_color(text_bright())
                        .accessibility_label(tr(keys::Q_MEDIA_LIVE_STORY))
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_live_story_sound(cx))),
                )
                .into_any_element(),
        )
    }

    /// The newest picture of the live story, converted off the UI thread
    /// (one conversion at a time; the image from two frames back leaves
    /// the atlas, [`StreamPicture`](crate::ui::calls::stream_picture)).
    fn live_story_picture(&mut self, key: i64, cx: &mut Context<Self>) -> Option<Arc<RenderImage>> {
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
                let image = frame_image(frame);
                let picture = &mut self.stories.live.picture;
                picture.start(key, frame.seq, Task::ready(()));
                if let Some(old) = picture.finish(key, frame.seq, image) {
                    cx.drop_image(old, None);
                }
            } else if let Some(frame) = frame {
                let seq = frame.seq;
                let task = cx.spawn(async move |this, cx| {
                    let image = cx
                        .background_spawn(async move { frame_image(&frame) })
                        .await;
                    let _ = this.update(cx, |app, cx| {
                        if let Some(old) = app.stories.live.picture.finish(key, seq, image) {
                            cx.drop_image(old, None);
                        }
                        cx.notify();
                    });
                });
                self.stories.live.picture.start(key, seq, task);
            }
        }
        self.stories.live.picture.current(key)
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
        let picture = self.live_story_picture(key, cx);
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
        let tile = StreamTile::new(("live-story-tile", key as u64), &self.stories.live.tile)
            .picture(picture)
            .fit(ObjectFit::Cover)
            .frame(px(6.), hsla(0., 0., 0., 1.))
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
                .text_color(text_bright())
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

    /// The comments over the lower half of the frame, newest at the
    /// bottom (tdesktop: left 8, width frame − 16, at most half the frame).
    fn live_comments(
        &self,
        session: &Session,
        messages: &[ParsedGroupCallMessage],
        levels: &[GroupCallMessageLevel],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let live = session.stories.live.as_ref();
        let top: Vec<MessageSender> = live
            .and_then(|live| live.donors.as_ref())
            .map(|donors| {
                donors
                    .top_donors
                    .iter()
                    .filter(|donor| !donor.is_anonymous)
                    .filter_map(|donor| donor.sender)
                    .take(MAX_TOP_SHOWN)
                    .collect()
            })
            .unwrap_or_default();
        let failed = live.map(|live| live.failed.clone()).unwrap_or_default();
        let rows: Vec<AnyElement> = messages
            .iter()
            .filter(|m| !m.text.is_empty())
            .map(|message| {
                let place = top.iter().position(|s| *s == message.sender_id);
                comment_row(
                    session,
                    message,
                    levels,
                    place,
                    failed.contains(&message.message_id),
                    self.stories.live.menu == Some(message.message_id),
                    cx,
                )
            })
            .collect();
        div()
            .id("live-story-comments")
            .debug_selector(|| "live-story-comments".into())
            .absolute()
            .left(px(8.))
            .bottom(px(MESSAGE_SKIP))
            .w(px(FRAME_W - 16.))
            .max_h(px(FRAME_H / 2.))
            .overflow_y_scroll()
            .track_scroll(&self.stories.live.scroll)
            .flex()
            .flex_col()
            .items_start()
            .gap(px(MESSAGE_SKIP))
            .children(rows)
            .into_any_element()
    }

    /// "Stars Sent!" with Undo and the seconds left, while your Stars wait
    /// (tdesktop `PaidReactionToast`).
    fn live_stars_toast(&self, session: &Session, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pending = session.stories.live.as_ref()?.pending_reaction?;
        let now = unix_ms_now();
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
                .bg(rgba(0x000000b3))
                .text_color(rgb(0xffffff))
                .child(
                    Icon::new(IconName::StarFill)
                        .with_size(px(24.))
                        .text_color(rgb(0xffb800)),
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
                        .aria_label(tr(keys::PAID_REACT_UNDO))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(0x5eb5f7))
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
            let right = 8. + (index % 2) as f32 * 24.;
            let burst = (0..6).map(move |ray| {
                let angle = ray as f32 * std::f32::consts::TAU / 6.0 + progress;
                let reach = 10. + progress * 36.;
                div()
                    .absolute()
                    .left(px(30. + angle.cos() * reach))
                    .top(px(8. + angle.sin() * reach))
                    .text_size(px(10.))
                    .text_color(rgb(0xffd54f))
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
                            .text_color(rgb(0xffffff))
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
                        window.paint_path(path, rgb(0x5eb5f7));
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
        .child(seconds.to_string())
        .into_any_element()
}

/// One comment: userpic, "**Name** text", the price, on the background of
/// its kind (tdesktop `MessagesUi::setupMessagesWidget` paint).
fn comment_row(
    session: &Session,
    message: &ParsedGroupCallMessage,
    levels: &[GroupCallMessageLevel],
    place: Option<usize>,
    failed: bool,
    menu_open: bool,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let stars = message.paid_message_star_count;
    let admin = message.is_from_owner;
    let line = px(TEXT_SIZE * 1.4);
    let radius = (3. + TEXT_SIZE * 1.4 + 2.) / 2.;
    let level = level_for_stars(levels, stars);
    let background = if stars > 0 {
        Some(level_color(level.first_color, PAID_BG_OPACITY))
    } else if admin {
        Some(level_color(GROUP_CALL_BG, ADMIN_BG_OPACITY))
    } else {
        None
    };
    let name = session.sender_label(message.sender_id);
    let body: AnyElement = if failed {
        div()
            .italic()
            .child(tr(keys::Q_LIVE_STORY_FAILED))
            .into_any_element()
    } else {
        let full = format!("{name} {}", message.text);
        let bold = HighlightStyle {
            font_weight: Some(FontWeight::SEMIBOLD),
            ..Default::default()
        };
        StyledText::new(full)
            .with_highlights(vec![(0..name.len(), bold)])
            .into_any_element()
    };
    let crown = place.map(|place| {
        div()
            .flex()
            .items_center()
            .text_size(px(9.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(0xffd54f))
            .child(Icon::new(IconName::Crown).with_size(px(11.)))
            .child((place + 1).to_string())
    });
    let price = (stars > 0).then(|| {
        div()
            .flex_none()
            .px(px(5.))
            .rounded_full()
            .bg(hsla(0., 0., 0., DARK_OVER_OPACITY))
            .text_size(px(12.))
            .child(format!("\u{2605}{}", group_digits(stars)))
    });
    let admin_badge = admin.then(|| {
        div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(live_badge(cx))
            .child(
                div()
                    .text_size(px(11.))
                    .opacity(ADMIN_BADGE_OPACITY)
                    .child(tr(keys::ADMIN_BADGE)),
            )
    });
    let message_id = message.message_id;
    let sender = message.sender_id;
    let text = message.text.clone();
    let date = message.date;
    let can_delete = message.can_be_deleted;
    let menu = menu_open.then(|| comment_menu(message_id, sender, text, date, can_delete, cx));
    div()
        .id(("live-story-comment", message_id as u64))
        .debug_selector(move || format!("live-story-comment-{message_id}"))
        .relative()
        .max_w(px(FRAME_W - 16.))
        .flex()
        .items_start()
        .gap(px(4.))
        .pl(px(2.))
        .pr(px(8.))
        .pt(px(2.))
        .pb(px(2.))
        .rounded(px(radius))
        .when_some(background, |this, bg| this.bg(bg))
        .text_size(px(TEXT_SIZE))
        .line_height(line)
        .text_color(rgb(0xffffff))
        .when(!failed, |this| {
            this.child(
                div()
                    .flex_none()
                    .child(sender_avatar(session, sender, USERPIC)),
            )
        })
        .when(failed, |this| this.pl(px(8.)))
        .child(
            div()
                .min_w_0()
                .pt(px(1.))
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .children(crown)
                        .child(div().min_w_0().child(body)),
                )
                .children(admin_badge),
        )
        .children(price)
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, _, _, cx| {
                this.stories.live.menu = Some(message_id);
                cx.notify();
            }),
        )
        .children(menu)
        .into_any_element()
}

/// The comment's context menu: the date, Copy Text and Delete (tdesktop
/// `MessagesUi::showContextMenu`).
fn comment_menu(
    message_id: i32,
    sender: MessageSender,
    text: String,
    date: i32,
    can_delete: bool,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let when = quill::message_menu::sent_label(
        &quill::local_time::civil_local(i64::from(date)),
        &quill::local_time::civil_local(quill::local_time::now_unix()),
    );
    let item = |id: &'static str, label: &'static str| {
        div()
            .id((id, message_id as u64))
            .px(px(12.))
            .py(px(6.))
            .cursor_pointer()
            .hover(|style| style.bg(rgba(0xffffff1a)))
            .role(gpui_kit::Role::MenuItem)
            .child(label)
    };
    div()
        .id(("live-story-comment-menu", message_id as u64))
        .absolute()
        .top(px(0.))
        .left(px(40.))
        .occlude()
        .min_w(px(160.))
        .py(px(4.))
        .rounded(px(6.))
        .bg(rgb(0x2c333d))
        .shadow_md()
        .text_sm()
        .text_color(rgb(0xffffff))
        .child(
            div()
                .px(px(12.))
                .py(px(4.))
                .text_xs()
                .text_color(rgba(0xffffff99))
                .child(when),
        )
        .child(
            item("live-comment-copy", tr(keys::CONTEXT_COPY_TEXT)).on_click(
                cx.listener(move |this, _, _, cx| this.copy_live_comment(text.clone(), cx)),
            ),
        )
        .when(can_delete, |this| {
            this.child(
                item("live-comment-delete", tr(keys::BOX_DELETE))
                    .debug_selector(|| "live-comment-delete".into())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.ask_delete_live_comment(message_id, sender, cx)
                    })),
            )
        })
        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
            this.stories.live.menu = None;
            cx.notify();
        }))
        .into_any_element()
}

/// The pinned paid comments in a row at the top: userpic, a short text and
/// the pin time running out (tdesktop `MessagesUi::setupPinnedWidget`).
fn live_pinned_strip(
    session: &Session,
    pinned: &[Pinned],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let pills = pinned.iter().map(|pin| {
        let name = session.sender_label(pin.sender);
        let text = if pin.text.is_empty() {
            format!("\u{2605}{}", group_digits(pin.stars))
        } else {
            pin.text.clone()
        };
        div()
            .id(("live-story-pinned", pin.message_id as u64))
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(4.))
            .pl(px(2.))
            .pr(px(10.))
            .py(px(2.))
            .rounded_full()
            .overflow_hidden()
            .bg(level_color(pin.color, PAID_BG_OPACITY))
            .cursor_pointer()
            .role(gpui_kit::Role::Button)
            .aria_label(name)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(relative(pin.gone))
                    .bg(hsla(0., 0., 0., DARK_OVER_OPACITY)),
            )
            .child(sender_avatar(session, pin.sender, PINNED_USERPIC))
            .child(
                div()
                    .max_w(px(PINNED_MAX_TEXT))
                    .truncate()
                    .text_size(px(TEXT_SIZE))
                    .text_color(rgb(0xffffff))
                    .child(text),
            )
            .on_click(cx.listener(|this, _, _, cx| this.show_live_comments(cx)))
    });
    div()
        .id("live-story-pinned-strip")
        .debug_selector(|| "live-story-pinned-strip".into())
        .flex()
        .gap(px(MESSAGE_SKIP))
        .overflow_x_scroll()
        .children(pills)
        .into_any_element()
}
