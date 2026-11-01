//! Color emoji glyphs a window no longer draws leave the GPU once Quill goes
//! idle.
//!
//! GPUI rasterizes each emoji once per size and keeps the tile in the
//! window's sprite atlas until the window closes. Color emoji are 4 bytes a
//! pixel and often 64 px or more at 2x, so the emoji panel alone kept 8 MB
//! after one look and more with every row scrolled past, as did chats full
//! of emoji.
//!
//! A sweep runs in two steps, because a cached slice (`app_slice`) replays
//! glyphs from an earlier frame without asking the atlas for them:
//! [`begin`] (from the idle trim, between frames) starts a new use mark and
//! redraws every view, cached ones too, so each emoji on screen is asked
//! for under it; once that frame is drawn, [`start_frame`] (which saw it
//! start, from the main and chat windows' `render`) drops the emoji glyphs
//! nothing asked for since. The redraw is one ordinary frame, and the emoji on
//! screen keep their tiles. Each window has its own atlas, and sweeps alone.
//! New emoji glyphs count as activity, so the idle trim comes after
//! scrolling through emoji too.

use gpui_kit::{AnyWindowHandle, App, Window, WindowId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::mem;

/// Emoji glyphs kept without a sweep: about a screen of them.
const KEEP: usize = 128;

#[derive(Clone, Copy, Default)]
struct WindowSweep {
    /// The sweep under way: its use mark and the frame it began after.
    pending: Option<(u64, u64)>,
    /// Frames this window has started.
    frames: u64,
    /// Emoji glyphs the last sweep kept (what was on screen then).
    kept: usize,
    /// Emoji glyphs the atlas held at the start of the last frame.
    seen: usize,
    /// The window went to the background since the last sweep began.
    left: bool,
}

thread_local! {
    static WINDOWS: RefCell<HashMap<WindowId, WindowSweep>> = RefCell::default();
}

fn with_sweep<R>(id: WindowId, read: impl FnOnce(&mut WindowSweep) -> R) -> R {
    WINDOWS.with_borrow_mut(|windows| read(windows.entry(id).or_default()))
}

/// Sweep when the atlas holds more emoji than a screen's worth and twice
/// what the last sweep kept, so a screen full of emoji is not redrawn
/// after every scroll; or when the window went to the background.
fn worth_a_sweep(count: usize, kept: usize, left: bool) -> bool {
    count > KEEP.max(kept * 2) || left && count > 0
}

/// A window became active or not. Going to the background sweeps once the
/// idle trim comes, however few emoji there are: nothing tells how many left
/// the screen (a closed emoji panel), and one redraw behind another app costs
/// nothing the user sees.
pub(super) fn window_activation(id: WindowId, active: bool) {
    if !active {
        with_sweep(id, |sweep| sweep.left = true);
        super::image_budget::note_activity();
    }
}

/// The idle trim's pass: forget closed windows, and start a sweep of each open
/// one's emoji glyphs if it holds more than it can be showing, or went to the
/// background. Call between frames.
pub(super) fn begin_for(open: &[AnyWindowHandle], cx: &mut App) {
    WINDOWS.with_borrow_mut(|windows| {
        windows.retain(|id, _| open.iter().any(|handle| handle.window_id() == *id));
    });
    for handle in open {
        let _ = handle.update(cx, |_, window, _| begin(window));
    }
}

fn begin(window: &mut Window) {
    let id = window.window_handle().window_id();
    let count = window.emoji_glyph_count();
    let due = with_sweep(id, |sweep| {
        sweep.pending.is_none() && worth_a_sweep(count, sweep.kept, mem::take(&mut sweep.left))
    });
    if due {
        let mark = window.begin_emoji_glyph_sweep();
        with_sweep(id, |sweep| sweep.pending = Some((mark, sweep.frames)));
    }
}

/// At the start of each frame of a window: note new emoji glyphs as activity,
/// and finish a sweep right after the frame that redraws everything (the first
/// after `begin`), so an idle window needs no frame more.
pub(super) fn start_frame(window: &mut Window, cx: &mut App) {
    let id = window.window_handle().window_id();
    let finished = with_sweep(id, |sweep| {
        sweep.frames += 1;
        let frames = sweep.frames;
        sweep
            .pending
            .take_if(|(_, began)| frames > *began)
            .map(|(mark, began)| (mark, frames == began + 1))
    });
    match finished {
        Some((mark, true)) => {
            // Deferred work runs once this frame is drawn.
            let handle = window.window_handle();
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| sweep(window, mark));
            });
        }
        Some((mark, false)) => sweep(window, mark),
        None => {}
    }
    let count = window.emoji_glyph_count();
    if count > with_sweep(id, |sweep| mem::replace(&mut sweep.seen, count)) {
        super::image_budget::note_activity();
    }
}

fn sweep(window: &mut Window, mark: u64) {
    let gone = window.sweep_emoji_glyphs(mark);
    let kept = window.emoji_glyph_count();
    with_sweep(window.window_handle().window_id(), |sweep| {
        sweep.kept = kept;
        sweep.seen = kept;
    });
    if super::frame_clock::trace_ticks() {
        eprintln!("emoji glyph sweep: {gone} dropped, {kept} kept");
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "glyph_sweep_tests.rs"]
mod window_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{
        AtlasKey, DevicePixels, FontId, GlyphId, HeadlessAtlas, PlatformAtlas, RenderGlyphParams,
        size,
    };
    use std::borrow::Cow;

    fn glyph(id: u32, is_emoji: bool) -> AtlasKey {
        AtlasKey::Glyph(RenderGlyphParams {
            font_id: FontId(0),
            glyph_id: GlyphId(id),
            font_size: gpui_kit::px(32.),
            subpixel_variant: Default::default(),
            scale_factor: 2.,
            is_emoji,
            subpixel_rendering: false,
            dilation: 0,
        })
    }

    /// Ask for a glyph the way a paint does; whether it was rasterized
    /// (it was not in the atlas).
    fn ask(atlas: &HeadlessAtlas, key: AtlasKey) -> bool {
        let pixels = [0u8; 4];
        let mut built = false;
        let mut build = || {
            built = true;
            Ok(Some((
                size(DevicePixels(1), DevicePixels(1)),
                Cow::Borrowed(pixels.as_slice()),
            )))
        };
        atlas
            .get_or_insert_with(key, &mut build)
            .expect("headless atlas");
        built
    }

    #[test]
    fn a_sweep_drops_only_the_emoji_nothing_drew_since_it_began() {
        let atlas = HeadlessAtlas::default();
        for id in 0..4 {
            ask(&atlas, glyph(id, true));
        }
        ask(&atlas, glyph(10, false));
        assert_eq!(atlas.emoji_glyph_count(), 4);

        // The redraw after `begin` asks for what is on screen: emoji 1, a
        // new emoji 5, and a text glyph.
        let mark = atlas.next_use_mark();
        assert!(!ask(&atlas, glyph(1, true)));
        assert!(ask(&atlas, glyph(5, true)));
        assert!(!ask(&atlas, glyph(10, false)));

        assert_eq!(atlas.remove_emoji_glyphs_unused_since(mark), 3);
        assert_eq!(atlas.emoji_glyph_count(), 2);
        // What was on screen is still there; a swept emoji is drawn anew.
        assert!(!ask(&atlas, glyph(1, true)));
        assert!(!ask(&atlas, glyph(5, true)));
        assert!(ask(&atlas, glyph(0, true)));
    }

    #[test]
    fn text_glyphs_stay_whatever_their_mark() {
        let atlas = HeadlessAtlas::default();
        ask(&atlas, glyph(10, false));
        let mark = atlas.next_use_mark();
        assert_eq!(atlas.remove_emoji_glyphs_unused_since(mark), 0);
        assert!(!ask(&atlas, glyph(10, false)));
    }

    #[test]
    fn a_screen_of_emoji_is_not_swept_again_until_it_doubles() {
        assert!(!worth_a_sweep(KEEP, 0, false));
        assert!(worth_a_sweep(KEEP + 1, 0, false));
        assert!(!worth_a_sweep(300, 200, false));
        assert!(worth_a_sweep(401, 200, false));
    }

    #[test]
    fn going_to_the_background_sweeps_any_emoji() {
        // The emoji panel closed after a sweep kept what it showed.
        assert!(worth_a_sweep(100, 100, true));
        assert!(!worth_a_sweep(0, 0, true));
    }
}
