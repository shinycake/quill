//! The Stars effects over the live story's Stars button (tdesktop
//! `ComposeControls::setupStarsEffectsCanvas`): each rises through a
//! 128 by 256 canvas centred on the button, a badge with the sender and
//! the count fading in and out, and a burst playing once around it.
#![deny(clippy::indexing_slicing)]

use super::live_state::{FlyingStars, effect_progress, level_color};
use super::live_view::sender_avatar;
use super::*;
use crate::ui::anim_layer::LayeredClip;
use crate::ui::sticker_playback::PlaybackSize;
use gpui_kit::assets::IconName;
use quill::live_story::{
    STAR_EFFECT_CANVAS_H, STAR_EFFECT_CANVAS_W, STAR_EFFECT_SIZE, StarEffectFrame, burst_order,
    level_for_stars, star_effect_frame,
};
use quill::premium_hub::group_digits;
use quill::state::Session;
use quill::telegram::envelope::StickerItem;
use std::cell::RefCell;
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
/// One tinted clip per effect in flight.
const TINTED_KEPT: usize = 4;
/// Rec. 709 luma weights, in the frames' BGR order.
const LUMA: [f32; 3] = [0.0722, 0.7152, 0.2126];

pub(super) struct StarBurst<'a> {
    candidates: &'a [StickerItem],
    tint: Option<Hsla>,
}

/// The swap point for the burst's source: the server's generic reaction
/// bursts tinted gold, in place of tdesktop's bundled star bursts.
fn star_burst(session: &Session) -> StarBurst<'_> {
    StarBurst {
        candidates: session.stories.live_bursts.stickers(),
        tint: Some(chrome::stars_gold()),
    }
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

    /// The burst `effect` plays (tdesktop `randomLoadedFrom`): the first in
    /// its order decoded before it started, else the seed's own once it is
    /// decoded. The seed's own is always asked for, so it downloads and
    /// decodes for the next effects.
    fn burst_clip(
        &self,
        burst: &StarBurst<'_>,
        effect: &FlyingStars,
        cx: &mut Context<Self>,
    ) -> Option<LayeredClip> {
        let mut order = burst_order(effect.seed, burst.candidates.len())
            .filter_map(|i| burst.candidates.get(i));
        let own = order.next()?;
        let ready = std::iter::once(own).chain(order).find_map(|sticker| {
            self.decoded_clip(sticker.file_id, PlaybackSize::Sticker, false)
                .filter(|clip| clip.started <= effect.started)
                .map(|clip| (sticker.file_id, clip))
        });
        let asked = self.layered_clip(own.file_id, own.format, PlaybackSize::Sticker, false, cx);
        let (file, mut clip) = ready.or_else(|| asked.map(|clip| (own.file_id, clip)))?;
        if let Some(tint) = burst.tint {
            clip.frames = tinted_frames(file.0, tint, &clip.frames);
        }
        Some(clip)
    }
}

/// The burst's frame `age` into the effect: played once at the clip's own
/// rate, gone after its last frame.
fn burst_frame(clip: &LayeredClip, age: std::time::Duration) -> Option<Arc<RenderImage>> {
    let index = (age.as_secs_f64() * clip.fps) as usize;
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
            .absolute()
            .inset_0()
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

thread_local! {
    static TINTED: RefCell<Vec<((i32, u32), Arc<[Arc<RenderImage>]>)>> =
        const { RefCell::new(Vec::new()) };
}

pub(super) fn release_tinted_bursts() {
    let old = TINTED.with(|cache| std::mem::take(&mut *cache.borrow_mut()));
    crate::ui::image_budget::retire_all(old.into_iter().flat_map(|(_, frames)| frames.to_vec()));
}

fn tinted_frames(
    file: i32,
    tint: Hsla,
    frames: &Arc<[Arc<RenderImage>]>,
) -> Arc<[Arc<RenderImage>]> {
    let rgba = tint.to_rgb();
    let key = (file, u32::from(rgba));
    if let Some(found) = TINTED.with(|cache| {
        cache
            .borrow()
            .iter()
            .find(|(cached, _)| *cached == key)
            .map(|(_, frames)| frames.clone())
    }) {
        return found;
    }
    let tinted = tint_frames(frames, [rgba.b, rgba.g, rgba.r]);
    TINTED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= TINTED_KEPT {
            let (_, old) = cache.remove(0);
            crate::ui::image_budget::retire_all(old.iter().cloned());
        }
        cache.push((key, tinted.clone()));
    });
    tinted
}

/// Each pixel becomes `bgr` times its luminance, keeping its alpha.
fn tint_frames(frames: &[Arc<RenderImage>], bgr: [f32; 3]) -> Arc<[Arc<RenderImage>]> {
    frames
        .iter()
        .map(|frame| {
            let width = frame.size(0).width.0.max(0) as u32;
            let height = frame.size(0).height.0.max(0) as u32;
            let tinted = frame.as_bytes(0).and_then(|bytes| {
                image::RgbaImage::from_raw(width, height, tint_pixels(bytes, bgr))
            });
            match tinted {
                Some(pixels) => Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
                    image::Frame::new(pixels),
                ]))),
                None => frame.clone(),
            }
        })
        .collect::<Vec<_>>()
        .into()
}

fn tint_pixels(bytes: &[u8], bgr: [f32; 3]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for &[b, g, r, a] in bytes.as_chunks::<4>().0 {
        let [luma_b, luma_g, luma_r] = LUMA;
        let luminance =
            (luma_r * f32::from(r) + luma_g * f32::from(g) + luma_b * f32::from(b)) / 255.;
        for channel in bgr {
            out.push((channel * luminance * 255.).round().clamp(0., 255.) as u8);
        }
        out.push(a);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::tint_pixels;

    const GOLD: [f32; 3] = [0.0, 0.72, 1.0];

    #[test]
    fn white_takes_the_full_tint_and_keeps_its_alpha() {
        assert_eq!(
            tint_pixels(&[255, 255, 255, 128], GOLD),
            vec![0, 184, 255, 128]
        );
    }

    #[test]
    fn black_stays_black() {
        assert_eq!(tint_pixels(&[0, 0, 0, 200], GOLD), vec![0, 0, 0, 200]);
    }

    #[test]
    fn green_tints_brighter_than_blue() {
        let green = tint_pixels(&[0, 255, 0, 255], GOLD);
        let blue = tint_pixels(&[255, 0, 0, 255], GOLD);
        assert!(green.get(2) > blue.get(2));
    }
}
