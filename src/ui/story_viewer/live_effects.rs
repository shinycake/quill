//! The Stars effects over the live story's Stars button (tdesktop
//! `ComposeControls::setupStarsEffectsCanvas`): each rises through a
//! 128 by 256 canvas centred on the button, a badge with the sender and
//! the count fading in and out, and a burst playing once around it.
#![deny(clippy::indexing_slicing)]

use super::live_state::{FlyingStars, effect_progress, level_color};
use super::live_view::sender_avatar;
use super::*;
use crate::ui::anim_layer::LayeredClip;
use crate::ui::sticker_playback::{ClipKey, PlaybackSize};
use gpui_kit::assets::IconName;
use quill::live_story::{
    STAR_EFFECT_CANVAS_H, STAR_EFFECT_CANVAS_W, STAR_EFFECT_SIZE, StarEffectFrame,
    burst_frame_index, burst_order, level_for_stars, ready_burst, star_effect_frame,
};
use quill::premium_hub::group_digits;
use quill::state::Session;
use quill::sticker_playback::Tint;
use quill::telegram::envelope::StickerItem;
use std::sync::Arc;

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
    pub(super) fn key(&self, sticker: &StickerItem) -> ClipKey {
        ClipKey {
            file: sticker.file_id,
            tint: self.tint,
        }
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
pub(super) fn burst_tint() -> Tint {
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
        for effect in &live.flying {
            let Some(progress) = effect_progress(effect.started, now) else {
                continue;
            };
            let age = now.saturating_duration_since(effect.started);
            let frame = self
                .burst_clip(&burst, effect, cx)
                .and_then(|clip| burst_frame(&clip, age));
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
            self.request_animation_tick(60, cx);
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

    /// The burst `effect` plays (tdesktop `randomLoadedFrom`, resolved when
    /// the effect starts): the first in its order whose tinted clip was
    /// ready by then, else none. The seed's own is asked for while it is
    /// missing, so later effects have it.
    fn burst_clip(
        &self,
        burst: &StarBurst<'_>,
        effect: &FlyingStars,
        cx: &mut Context<Self>,
    ) -> Option<LayeredClip> {
        let mut clips: Vec<Option<LayeredClip>> = burst
            .candidates
            .iter()
            .map(|sticker| self.decoded_clip(burst.key(sticker), PlaybackSize::Sticker, false))
            .collect();
        if let Some(own) = burst_order(effect.seed, clips.len()).next()
            && clips.get(own).is_some_and(Option::is_none)
            && let Some(sticker) = burst.candidates.get(own)
        {
            self.request_clip(
                burst.key(sticker),
                sticker.format,
                PlaybackSize::Sticker,
                cx,
            );
        }
        let ready: Vec<Option<Instant>> = clips
            .iter()
            .map(|clip| clip.as_ref().map(|clip| clip.started))
            .collect();
        let pick = ready_burst(effect.seed, &ready, effect.started)?;
        clips.get_mut(pick).and_then(Option::take)
    }
}

/// The burst's frame `age` into the effect: played once at the clip's own
/// rate, gone after its last frame.
fn burst_frame(clip: &LayeredClip, age: std::time::Duration) -> Option<Arc<RenderImage>> {
    let index = burst_frame_index(age, clip.fps, clip.frames.len())?;
    let frame = clip.frames.get(index).cloned()?;
    if let Some(shown) = &clip.shown {
        shown.set(Instant::now());
    }
    Some(frame)
}

/// The burst is drawn at full opacity: tdesktop draws it after restoring
/// the badge's transform.
fn effect_element(
    frame: StarEffectFrame,
    badge: Div,
    burst: Option<Arc<RenderImage>>,
) -> AnyElement {
    div()
        .absolute()
        .left(px(frame.left))
        .top(px(frame.top))
        .size(px(STAR_EFFECT_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .child(badge.opacity(frame.opacity.clamp(0.0, 1.0)))
        .children(burst.map(|image| {
            div()
                .debug_selector(|| "live-story-effect-burst".into())
                .absolute()
                .inset_0()
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, (), window, _| {
                            let _ = window.paint_image(
                                bounds,
                                bounds,
                                Corners::default(),
                                image.clone(),
                                0,
                                false,
                            );
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
