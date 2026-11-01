//! The "Send with Effect" card (tdesktop `EffectPreview` in
//! `menu/menu_send.cpp`): a 224 px square of the account's chat background
//! with a sample incoming message ("Good morning", replying to Bob Harris's
//! "Do you know what time it is?") whose effect badge sits a third of the
//! way left of the centre, the effect looping over it mirrored as for a
//! message on the left, and under it the send button or the Premium offer.
//! It fades in and out with `easeOutCirc` over 400 ms; until the icon and
//! the animation are there, a spinner stands where the badge goes.

use super::super::app::QuillApp;
use super::super::chat_theme::{accent, bg_bubble_incoming, text_muted, text_primary};
use super::super::message_effect_badge::{self as badge, BadgeIcon};
use super::super::motion::ease_out_circ;
use super::super::nested_click::SwallowPress;
use super::super::sticker_player::{HeadId, Part, ViewAsk};
use super::super::wallpaper::paint_wallpaper;
use super::super::wallpaper_cache::Slot;
use super::{Preview, SendMenu, tracker};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::{TestSupportExt, *};
use quill::lang::{keys, tr, tr_with};
use quill::message_effects_picker::{
    PREVIEW_BUTTON, PREVIEW_SIDE, PREVIEW_TOGGLE_MS, can_send, preview_origin,
};
use quill::sticker_playback::player::PlayMode;
use std::time::{Duration, Instant};

/// The sample bubble: wide enough that its left edge is off the card.
const BUBBLE_WIDTH: f32 = 240.;
const BUBBLE_PAD_X: f32 = 8.;
const BUBBLE_PAD_BOTTOM: f32 = 6.;
/// Between the badge and the time.
const FOOTER_GAP: f32 = 3.;
/// The time's box in the footer's 12 px: "12:34", or "12:34 PM" in the
/// 12-hour clock.
fn time_width() -> f32 {
    match quill::clock_style::ClockStyle::current() {
        quill::clock_style::ClockStyle::Hour24 => 30.,
        quill::clock_style::ClockStyle::Hour12 => 50.,
    }
}
const FPS: u32 = 60;
/// The card keeps this far from the window's edges, like the menu.
const WINDOW_MARGIN: f32 = 8.;

/// How far shown the card is, 0 to 1 (`_shownAnimation`).
fn progress(preview: &Preview, now: Instant) -> f32 {
    let span = Duration::from_millis(PREVIEW_TOGGLE_MS).as_secs_f32();
    let since = |at: Instant| now.saturating_duration_since(at).as_secs_f32() / span;
    match preview.hiding {
        Some(at) => 1. - ease_out_circ(since(at)),
        None => ease_out_circ(since(preview.shown)),
    }
}

impl QuillApp {
    /// The card for `preview`, `None` once it has faded out.
    pub(super) fn effect_card(
        &self,
        open: &SendMenu,
        preview: &Preview,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let current = open
            .preview
            .as_ref()
            .is_some_and(|shown| std::ptr::eq(shown, preview));
        let toggle = Duration::from_millis(PREVIEW_TOGGLE_MS);
        // The executor's clock: simulated in tests, the real one otherwise.
        let now = cx.background_executor().now();
        let gone = preview
            .hiding
            .is_some_and(|since| now.saturating_duration_since(since) >= toggle);
        if gone {
            if current {
                open.areas.card.set(None);
            }
            return None;
        }
        let session = self.session()?;
        let effect = session.stickers.effects.get(preview.effect_id)?;
        let parent = if open.expanded {
            open.areas.panel.get()
        } else {
            open.areas.column.get()
        };
        let Some(parent) = parent else {
            // The menu has not been painted yet: place the card next frame.
            self.request_animation_tick(FPS, cx);
            return None;
        };
        let card = (PREVIEW_SIDE, PREVIEW_SIDE + PREVIEW_BUTTON);
        let (x, y) = preview_origin(
            (f32::from(preview.center.x), f32::from(preview.center.y)),
            card,
            (
                f32::from(parent.origin.x),
                f32::from(parent.origin.y),
                f32::from(parent.size.width),
                f32::from(parent.size.height),
            ),
        );
        // Kept inside the window too (the menu is, the card may be taller).
        let viewport = (
            f32::from(open.viewport.width),
            f32::from(open.viewport.height),
        );
        let x = x
            .min(viewport.0 - card.0 - WINDOW_MARGIN)
            .max(WINDOW_MARGIN);
        let y = y
            .min(viewport.1 - card.1 - WINDOW_MARGIN)
            .max(WINDOW_MARGIN);
        let progress = progress(preview, now);
        let hiding = preview.hiding.is_some();

        let icon = badge::effect_icon(effect, &session.media.files, &self.media_display_roots());
        // The effect loops over the card; the sample is incoming, on the
        // left: mirrored (`request.mirrorHorizontal`).
        let head = HeadId::of(Part::Chrome, ("effect-preview", preview.effect_id));
        let view = effect.animation().and_then(|animation| {
            let ask = ViewAsk::sticker(
                animation.file,
                animation.format,
                head,
                PlayMode::Loop,
                PREVIEW_SIDE,
            )
            .mirrored(true);
            self.sticker_view(ask, cx)
                .map(|view| (view, animation.file))
        });
        // Nothing to play (an unknown kind): the icon alone is the card.
        let animated = effect.animation().is_some();
        let ready = !matches!(icon, BadgeIcon::Waiting) && (view.is_some() || !animated);
        let lottie = view.filter(|_| ready).map(|(view, file)| {
            if preview.playing.get().is_none() {
                // Shown again: from its first frame.
                preview.playing.set(Some(Instant::now()));
                self.playback.stickers.restart(head, file.0);
            }
            view
        });
        let fps = lottie.as_ref().map_or(0, |view| view.fps()).max(FPS);
        self.request_animation_tick(fps, cx);

        let effect_id = preview.effect_id;
        let premium = session.my_is_premium();
        let bottom = if can_send(effect, premium) {
            send_button(effect_id, cx)
        } else {
            premium_offer(effect_id, cx)
        };
        let badge_cell = div()
            .size(px(badge::CELL))
            .flex()
            .items_center()
            .justify_center()
            .child(if ready {
                badge::picture(&icon).unwrap_or_else(|| div().into_any_element())
            } else {
                Spinner::new()
                    .with_size(px(badge::CELL))
                    .color(text_muted().into())
                    .into_any_element()
            });
        let body = div()
            .relative()
            .size_full()
            .rounded_lg()
            .overflow_hidden()
            .shadow_lg()
            .bg(cx.theme().popover)
            .flex()
            .flex_col()
            .child(
                paint_wallpaper(
                    div()
                        .relative()
                        .flex_none()
                        .w(px(PREVIEW_SIDE))
                        .h(px(PREVIEW_SIDE))
                        .overflow_hidden(),
                    self.current_wallpaper(cx).as_ref(),
                    0,
                    Slot::Thumb,
                )
                .child(sample_bubble(badge_cell)),
            )
            .child(bottom);

        let mut layer = div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(card.0))
            .h(px(card.1))
            .child(div().size_full().opacity(progress).child(body));
        if let Some(view) = lottie {
            // The effect stays opaque while the card fades in.
            let opacity = if hiding { progress } else { 1. };
            layer = layer.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(PREVIEW_SIDE))
                    .opacity(opacity)
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                view.paint(bounds, window);
                            },
                        )
                        .size_full(),
                    ),
            );
        }
        if hiding {
            // Fading out: drawn, not pressed.
            return Some(layer.into_any_element());
        }
        let areas = open.areas.clone();
        Some(
            layer
                .id(("send-effect-card", effect_id as u64))
                .test_support()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.hide_effect_preview(cx);
                    }),
                )
                .child(tracker(move |bounds| areas.card.set(Some(bounds))))
                .into_any_element(),
        )
    }
}

/// The sample message, placed so its badge centre is at a sixth of the
/// card's width and half its height (`setupItem`: the badge shifted a third
/// of the width left of the centre).
fn sample_bubble(badge_cell: Div) -> AnyElement {
    let badge_center = (PREVIEW_SIDE / 2. - PREVIEW_SIDE / 3., PREVIEW_SIDE / 2.);
    let right = badge_center.0 + badge::CELL / 2. + FOOTER_GAP + time_width() + BUBBLE_PAD_X;
    let bottom = badge_center.1 + badge::CELL / 2. + BUBBLE_PAD_BOTTOM;
    let time = quill::local_time::hhmm(&quill::local_time::civil_local(
        quill::local_time::now_unix(),
    ));
    let reply_bg = Hsla::from(accent()).opacity(0.1);
    div()
        .absolute()
        .left(px(right - BUBBLE_WIDTH))
        .bottom(px(PREVIEW_SIDE - bottom))
        .w(px(BUBBLE_WIDTH))
        .rounded_lg()
        .bg(bg_bubble_incoming())
        .pt(px(6.))
        .px(px(BUBBLE_PAD_X))
        .pb(px(BUBBLE_PAD_BOTTOM))
        .flex()
        .flex_col()
        .gap_1()
        .text_size(px(14.))
        .text_color(text_primary())
        .child(
            div()
                .flex()
                .rounded_sm()
                .overflow_hidden()
                .bg(reply_bg)
                .child(div().w(px(3.)).flex_none().bg(accent()))
                .child(
                    div()
                        .px_1p5()
                        .py_0p5()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .text_size(px(13.))
                        .line_height(px(16.))
                        .child(
                            div()
                                .font_semibold()
                                .text_color(accent())
                                .truncate()
                                .child(tr(keys::Q_MISC_BOB_HARRIS)),
                        )
                        .child(div().truncate().child(tr(keys::SETTINGS_CHAT_MESSAGE))),
                ),
        )
        .child(
            div()
                .flex()
                .items_end()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(tr(keys::Q_MISC_GOOD_MORNING)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(FOOTER_GAP))
                        .h(px(badge::CELL))
                        .child(badge_cell)
                        .child(
                            div()
                                .w(px(time_width()))
                                .text_size(px(12.))
                                .text_color(text_muted())
                                .text_right()
                                .child(time),
                        ),
                ),
        )
        .into_any_element()
}

/// "Send with Effect"; a right-click offers sending silently or on a
/// schedule with it.
fn send_button(effect_id: i64, cx: &mut Context<QuillApp>) -> AnyElement {
    let theme = cx.theme();
    let hover_bg = theme.accent;
    div()
        .id("send-with-effect")
        .test_support()
        .flex_none()
        .h(px(PREVIEW_BUTTON))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_sm()
        .font_semibold()
        .text_color(accent())
        .hover(move |style| style.bg(hover_bg))
        .role(Role::Button)
        .child(tr(keys::EFFECT_SEND))
        .swallow_press()
        .on_click(cx.listener(move |this, _, window, cx| {
            this.send_with_effect(effect_id, false, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.open_card_menu(effect_id, event.position, window, cx);
            }),
        )
        .into_any_element()
}

/// "Subscribe to Telegram Premium to add this animated effect.", the link
/// opening the Premium preview of effects.
fn premium_offer(effect_id: i64, cx: &mut Context<QuillApp>) -> AnyElement {
    let link_text = tr(keys::SEND_AS_PREMIUM_REQUIRED_LINK);
    let text = tr_with(keys::EFFECT_PREMIUM, &[("link", link_text)]);
    let start = text.find(link_text).unwrap_or(0);
    let link = start..start + link_text.len();
    let highlight = HighlightStyle {
        color: Some(accent().into()),
        ..Default::default()
    };
    let owner = cx.entity().downgrade();
    div()
        .flex_none()
        .w(px(PREVIEW_SIDE))
        .min_h(px(PREVIEW_BUTTON))
        .px(px(4.))
        .py(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .text_center()
        .text_size(px(11.))
        .line_height(px(14.))
        .text_color(text_muted())
        .swallow_press()
        .child(
            div().w_full().child(
                InteractiveText::new(
                    ("send-effect-premium", effect_id as u64),
                    StyledText::new(text).with_highlights(vec![(link.clone(), highlight)]),
                )
                .on_click(vec![link], move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.close_send_menu(cx);
                        this.open_premium_at(
                            quill::premium_hub::PremiumFeature::MessageEffects,
                            cx,
                        );
                    });
                }),
            ),
        )
        .into_any_element()
}
