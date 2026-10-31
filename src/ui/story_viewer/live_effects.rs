//! The Stars effects over the live story's Stars button (tdesktop
//! `ComposeControls::setupStarsEffectsCanvas`): each rises through a
//! 128 by 256 canvas centred on the button, a badge with the sender and
//! the count fading in and out, and a burst playing once around it.
#![deny(clippy::indexing_slicing)]

use super::live_state::{FlyingStars, effect_progress, level_color};
use super::live_view::sender_avatar;
use super::*;
use crate::ui::sticker_player::{HeadId, Part, StickerView, ViewAsk};
use gpui_kit::assets::IconName;
use quill::live_story::{
    STAR_EFFECT_CANVAS_H, STAR_EFFECT_CANVAS_W, STAR_EFFECT_SIZE, StarEffectFrame, burst_order,
    level_for_stars, ready_burst, star_effect_frame,
};
use quill::premium_hub::group_digits;
use quill::state::Session;
use quill::sticker_playback::Tint;
use quill::sticker_playback::player::PlayMode;
use quill::telegram::envelope::StickerItem;

/// tdesktop `whoReadDateStyle`: the badge's 12 px font, on a 16 px line.
const BADGE_FONT: f32 = 12.;
const BADGE_LINE: f32 = 16.;
/// tdesktop `starIconEmojiSmall` (`chat/mini_stars`).
const BADGE_STAR: f32 = 12.;
/// tdesktop `groupCallEffectPadding`: the pill's right, top and bottom.
const BADGE_PAD_RIGHT: f32 = 3.;
const BADGE_PAD_Y: f32 = 1.;
/// tdesktop `groupCallEffectUserpicPadding`: left of and after the userpic.
const USERPIC_PAD_LEFT: f32 = 1.;
const USERPIC_PAD_RIGHT: f32 = 3.;

pub(super) struct StarBurst<'a> {
    pub(super) candidates: &'a [StickerItem],
    tint: Option<Tint>,
}

impl StarBurst<'_> {
    /// The view of `sticker`'s burst `age` into effect number `effect`:
    /// played once on the effect's clock, tinted, drawn at the effect's
    /// size.
    pub(super) fn ask(
        &self,
        sticker: &StickerItem,
        age: std::time::Duration,
        effect: usize,
    ) -> ViewAsk {
        ViewAsk::sticker(
            sticker.file_id,
            sticker.format,
            HeadId::of(Part::Chrome, ("star-burst", sticker.file_id.0, effect)),
            PlayMode::At(age),
            STAR_EFFECT_SIZE,
        )
        .tinted(self.tint)
    }
}

/// The swap point for the burst's source: the server's generic reaction
/// bursts tinted gold, in place of tdesktop's bundled star bursts.
pub(super) fn star_burst(session: &Session) -> StarBurst<'_> {
    StarBurst {
        candidates: session.stories.live_bursts.stickers(),
        tint: Some(burst_tint()),
    }
}

/// `chrome::stars_gold()` as a tint for the burst's frames.
fn burst_tint() -> Tint {
    let gold = chrome::stars_gold().to_rgb();
    Tint([gold.b, gold.g, gold.r].map(|channel| (channel * 255.).round() as u8))
}

impl QuillApp {
    /// The effects in flight over a Stars button `button_width` wide.
    pub(super) fn live_effects_layer(
        &self,
        session: &Session,
        button_width: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let live = &self.stories.live;
        if live.flying.is_empty() {
            return None;
        }
        let now = live.now();
        let levels = session.live_levels();
        let burst = star_burst(session);
        let mut effects = Vec::new();
        let mut fps = 60;
        for (index, effect) in live.flying.iter().enumerate() {
            let Some(progress) = effect_progress(effect.started, now) else {
                continue;
            };
            let age = now.saturating_duration_since(effect.started);
            let frame = self.burst_view(&burst, effect, index, age, cx);
            if let Some(view) = &frame {
                fps = fps.max(view.fps());
            }
            let color = level_for_stars(&levels, effect.stars).first_color;
            let motion = star_effect_frame(progress, effect.shift);
            effects.push(effect_element(
                motion,
                badge(session, effect, color, motion.scale),
                frame,
            ));
        }
        if effects.is_empty() {
            return None;
        }
        if live.demo_clock.is_none() {
            self.request_animation_tick(fps, cx);
        }
        Some(
            div()
                .debug_selector(|| "live-story-effects".into())
                .absolute()
                .bottom_0()
                .left(px((button_width - STAR_EFFECT_CANVAS_W) / 2.))
                .w(px(STAR_EFFECT_CANVAS_W))
                .h(px(STAR_EFFECT_CANVAS_H))
                .children(effects)
                .into_any_element(),
        )
    }

    /// The burst `effect` plays `age` in (tdesktop `randomLoadedFrom`,
    /// resolved when the effect starts): the first in its order whose
    /// tinted clip had a frame by then, else none. The seed's own is
    /// opened while it is missing, so later effects have it.
    fn burst_view(
        &self,
        burst: &StarBurst<'_>,
        effect: &FlyingStars,
        number: usize,
        age: std::time::Duration,
        cx: &mut Context<Self>,
    ) -> Option<StickerView> {
        let own = burst_order(effect.seed, burst.candidates.len()).next();
        let mut views: Vec<Option<StickerView>> = burst
            .candidates
            .iter()
            .enumerate()
            .map(|(index, sticker)| {
                let ask = burst.ask(sticker, age, number);
                if Some(index) == own {
                    self.sticker_view(ask, cx)
                } else {
                    self.opened_sticker_view(ask, cx)
                }
            })
            .collect();
        let ready: Vec<Option<Instant>> = views
            .iter()
            .map(|view| view.as_ref().and_then(StickerView::ready_at))
            .collect();
        let pick = ready_burst(effect.seed, &ready, effect.started)?;
        views.get_mut(pick).and_then(Option::take)
    }
}

/// The burst is drawn at full opacity: tdesktop draws it after restoring
/// the badge's transform.
fn effect_element(frame: StarEffectFrame, badge: Div, burst: Option<StickerView>) -> AnyElement {
    div()
        .absolute()
        .left(px(frame.left))
        .top(px(frame.top))
        .size(px(STAR_EFFECT_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .child(badge.opacity(frame.opacity.clamp(0.0, 1.0)))
        .children(burst.map(|view| {
            div()
                .debug_selector(|| "live-story-effect-burst".into())
                .absolute()
                .inset_0()
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, (), window, cx| {
                            crate::ui::sticker_player::note_display(window, cx);
                            view.paint(bounds, window);
                        },
                    )
                    .size_full(),
                )
        }))
        .into_any_element()
}

/// tdesktop `StarEffect`'s badge at `scale`: a pill in the level's color
/// with the sender's userpic, the gold star and the count in white.
/// GPUI cannot scale an element, so its metrics are scaled.
fn badge(session: &Session, effect: &FlyingStars, color: u32, scale: f32) -> Div {
    let at = |value: f32| px(value * scale);
    div()
        .debug_selector(|| "live-story-effect-badge".into())
        .flex_none()
        .flex()
        .items_center()
        .pl(at(USERPIC_PAD_LEFT))
        .pr(at(BADGE_PAD_RIGHT))
        .py(at(BADGE_PAD_Y))
        .rounded_full()
        .bg(level_color(color, 1.0))
        .text_color(chrome::fg())
        .text_size(at(BADGE_FONT))
        .line_height(at(BADGE_LINE))
        .child(
            div()
                .flex_none()
                .mr(at(USERPIC_PAD_RIGHT))
                .child(sender_avatar(session, effect.sender, BADGE_LINE * scale)),
        )
        .child(
            Icon::new(IconName::StarFill)
                .with_size(at(BADGE_STAR))
                .text_color(chrome::stars_gold()),
        )
        .child(group_digits(effect.stars))
}
