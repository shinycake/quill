//! Color emoji glyphs the main window no longer draws leave the GPU once
//! Quill goes idle.
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
//! start, from `QuillApp::render`) drops the emoji glyphs nothing asked for
//! since. The redraw is one ordinary frame, and the emoji on screen keep
//! their tiles.
//! New emoji glyphs count as activity, so the idle trim comes after
//! scrolling through emoji too.

use gpui_kit::{App, Window};
use std::cell::Cell;

/// Emoji glyphs kept without a sweep: about a screen of them.
const KEEP: usize = 128;

thread_local! {
    /// The sweep under way: its use mark and the frame it began after.
    static PENDING: Cell<Option<(u64, u64)>> = const { Cell::new(None) };
    /// Emoji glyphs the last sweep kept (what was on screen then).
    static KEPT: Cell<usize> = const { Cell::new(0) };
    /// Emoji glyphs the atlas held at the start of the last frame.
    static SEEN: Cell<usize> = const { Cell::new(0) };
    /// The window went to the background since the last sweep began.
    static LEFT: Cell<bool> = const { Cell::new(false) };
}

/// Sweep when the atlas holds more emoji than a screen's worth and twice
/// what the last sweep kept, so a screen full of emoji is not redrawn
/// after every scroll; or when the window went to the background.
fn worth_a_sweep(count: usize, kept: usize, left: bool) -> bool {
    count > KEEP.max(kept * 2) || left && count > 0
}

/// The main window became active or not. Going to the background sweeps
/// once the idle trim comes, however few emoji there are: nothing tells
/// how many left the screen (a closed emoji panel), and one redraw behind
/// another app costs nothing the user sees.
pub(super) fn window_activation(active: bool) {
    if !active {
        LEFT.set(true);
        super::image_budget::note_activity();
    }
}

/// Quill went idle: start a sweep of the main window's emoji glyphs if it
/// holds more than it can be showing, or went to the background. Call
/// between frames.
pub(super) fn begin(window: &mut Window) {
    if PENDING.get().is_some() {
        return;
    }
    if !worth_a_sweep(window.emoji_glyph_count(), KEPT.get(), LEFT.replace(false)) {
        return;
    }
    let mark = window.begin_emoji_glyph_sweep();
    PENDING.set(Some((mark, super::image_budget::frame())));
}

/// At the start of each main-window frame, after
/// `image_budget::begin_frame`: note new emoji glyphs as activity, and
/// finish a sweep right after the frame that redraws everything (the first
/// after `begin`), so an idle window needs no frame more.
pub(super) fn start_frame(window: &mut Window, cx: &mut App) {
    if let Some((mark, began)) = PENDING.get() {
        let frame = super::image_budget::frame();
        if frame == began + 1 {
            // Deferred work runs once this frame is drawn.
            PENDING.set(None);
            let handle = window.window_handle();
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| sweep(window, mark));
            });
        } else if frame > began + 1 {
            PENDING.set(None);
            sweep(window, mark);
        }
    }
    let count = window.emoji_glyph_count();
    if count > SEEN.replace(count) {
        super::image_budget::note_activity();
    }
}

fn sweep(window: &mut Window, mark: u64) {
    let gone = window.sweep_emoji_glyphs(mark);
    let kept = window.emoji_glyph_count();
    KEPT.set(kept);
    SEEN.set(kept);
    if super::frame_clock::trace_ticks() {
        eprintln!("emoji glyph sweep: {gone} dropped, {kept} kept");
    }
}

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
