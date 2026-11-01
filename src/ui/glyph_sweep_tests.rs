use super::{KEEP, begin_for, start_frame, window_activation};
use gpui_kit::{
    AnyWindowHandle, AppContext, AtlasKey, Context, DevicePixels, FontId, GlyphId, IntoElement,
    Render, RenderGlyphParams, TestAppContext, Window, div, px, size,
};
use std::borrow::Cow;

struct SweepingWindow;

impl Render for SweepingWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        start_frame(window, cx);
        div()
    }
}

fn seed(window: AnyWindowHandle, emoji: usize, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, _| {
        for id in 0..emoji as u32 {
            let key = AtlasKey::Glyph(RenderGlyphParams {
                font_id: FontId(0),
                glyph_id: GlyphId(id),
                font_size: px(32.),
                subpixel_variant: Default::default(),
                scale_factor: 2.,
                is_emoji: true,
                subpixel_rendering: false,
                dilation: 0,
            });
            let pixels = [0u8; 4];
            let mut build = || {
                Ok(Some((
                    size(DevicePixels(1), DevicePixels(1)),
                    Cow::Borrowed(pixels.as_slice()),
                )))
            };
            window
                .sprite_atlas()
                .get_or_insert_with(key, &mut build)
                .expect("headless atlas");
        }
    })
    .unwrap();
}

fn count(window: AnyWindowHandle, cx: &mut TestAppContext) -> usize {
    cx.update_window(window, |_, window, _| window.emoji_glyph_count())
        .unwrap()
}

fn open(cx: &mut TestAppContext) -> AnyWindowHandle {
    cx.add_window(|_, _| SweepingWindow).into()
}

#[gpui_kit::test]
fn a_secondary_window_sweeps_its_own_emoji(cx: &mut TestAppContext) {
    let (main, chat) = (open(cx), open(cx));
    seed(chat, KEEP + 1, cx);

    cx.update(|cx| begin_for(&[main, chat], cx));
    cx.run_until_parked();

    assert_eq!(count(chat, cx), 0);
}

#[gpui_kit::test]
fn each_window_finishes_on_its_own_frames(cx: &mut TestAppContext) {
    let (main, chat) = (open(cx), open(cx));
    seed(main, KEEP + 1, cx);
    seed(chat, KEEP + 1, cx);

    cx.update(|cx| begin_for(&[main, chat], cx));
    cx.run_until_parked();

    assert_eq!((count(main, cx), count(chat, cx)), (0, 0));
}

#[gpui_kit::test]
fn a_window_in_the_background_sweeps_alone(cx: &mut TestAppContext) {
    let (main, chat) = (open(cx), open(cx));
    seed(main, 3, cx);
    seed(chat, 3, cx);
    let id = cx
        .update_window(chat, |_, window, _| window.window_handle().window_id())
        .unwrap();

    window_activation(id, false);
    cx.update(|cx| begin_for(&[main, chat], cx));
    cx.run_until_parked();

    assert_eq!((count(main, cx), count(chat, cx)), (3, 0));
}
