//! A main window showing two looping stickers, with a chat opened in its
//! own window: the main window draws nothing while the active chat window
//! covers it whole, and animates again, exactly as alone in front, as soon
//! as any of it shows. Over the demo; nothing reaches Telegram.

use crate::ui::actions::CloseWindow;
use crate::ui::app::QuillApp;
use crate::ui::frame_clock::frames_over;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, DisplayId, Entity, Pixels, TestAppContext,
    VisualTestContext, point, px, size,
};
use quill::ids::ChatId;
use quill::sticker_playback::player::DecodedFrames;
use std::sync::Arc;

/// The demo's open chat, with the two stickers, and another chat.
const OPEN: ChatId = ChatId(11);
const OTHER: ChatId = ChatId(12);
/// The two animated stickers in chat 11 (`apply_ready_sticker_playback`).
const STICKER_FILES: [i32; 2] = [44, 45];

/// The demo with two looping 30 fps stickers in the open chat, in an
/// active 1100 × 700 main window at the origin, their first frames drawn.
fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-sticker-playback");
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        for file in STICKER_FILES {
            view.read(cx).playback.stickers.set_fixture(
                file,
                Arc::new(|edge| DecodedFrames {
                    frames: (0..30u8)
                        .map(|i| vec![i; (edge as usize).pow(2) * 4])
                        .collect(),
                    fps: 30.0,
                }),
            );
        }
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    for _ in 0..4 {
        vcx.update(|window, cx| {
            window.activate_window();
            window.render_frame(cx);
        });
        vcx.run_until_parked();
        app.read_with(&vcx, |app, _| app.playback.stickers.settle());
        app.update(&mut vcx, |_, cx| cx.notify());
    }
    vcx.run_until_parked();
    (app, vcx)
}

/// Open `chat` in a window at `bounds`; it opens active, as on a desktop.
fn open_chat_window(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    chat: ChatId,
    bounds: Bounds<Pixels>,
) -> AnyWindowHandle {
    app.update_in(vcx, |app, window, cx| {
        app.open_chat_window_at(chat, Some(bounds), window, cx);
    });
    vcx.run_until_parked();
    let main = vcx.update(|window, _| window.window_handle().window_id());
    let others: Vec<_> = vcx
        .windows()
        .into_iter()
        .filter(|handle| handle.window_id() != main)
        .collect();
    assert_eq!(others.len(), 1, "one chat window opened");
    let window = others[0];
    let active = vcx
        .update_window(window, |_, window, _| window.is_window_active())
        .expect("the chat window");
    assert!(active, "the chat window is the active one");
    window
}

/// The main window's animation-layer draws and frames over two seconds of
/// the test clock, after one to settle.
fn main_draws(vcx: &mut VisualTestContext) -> (u32, u32) {
    let _ = frames_over(vcx, 1);
    let drawn = frames_over(vcx, 2);
    let count = |kind| drawn.get(kind).copied().unwrap_or(0);
    (count("animation layer"), count("(window frame)"))
}

/// The frames `window` draws while the main window's draws are counted.
fn frames_of(window: AnyWindowHandle, vcx: &mut VisualTestContext) -> u64 {
    let frame = |vcx: &mut VisualTestContext| {
        vcx.update_window(window, |_, window, _| window.frame_number())
            .expect("the window")
    };
    let _ = frames_over(vcx, 1);
    let before = frame(vcx);
    let _ = frames_over(vcx, 2);
    frame(vcx) - before
}

/// A window over all of the main window, standing out past each edge.
fn covering() -> Bounds<Pixels> {
    Bounds::new(point(px(-20.), px(-30.)), size(px(1160.), px(760.)))
}

/// Covered whole by the active chat window, the main window draws nothing;
/// moved so a strip of the main window shows, it animates as alone.
#[gpui_kit::test]
fn a_covered_main_window_draws_nothing_until_the_chat_window_moves(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let alone = main_draws(vcx);
    assert!(
        alone.0 > 0 && alone.1 > 0,
        "the stickers animate: {alone:?}"
    );
    let chat = open_chat_window(&app, vcx, OTHER, covering());
    for _ in 0..5 {
        assert_eq!(main_draws(vcx), (0, 0), "covered: nothing drawn");
    }
    // 40 px right: the main window's left edge shows.
    vcx.simulate_window_move(chat, point(px(20.), px(-30.)));
    vcx.run_until_parked();
    assert_eq!(main_draws(vcx), alone, "partly shown");
}

/// Resized to leave the main window's right edge out, the chat window
/// lets it animate; covering it again, not. Closed, with the main window
/// active again as the platform makes it, the main window animates.
#[gpui_kit::test]
fn a_covered_main_window_animates_once_resized_past_or_closed(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let alone = main_draws(vcx);
    let chat = open_chat_window(&app, vcx, OTHER, covering());
    assert_eq!(main_draws(vcx), (0, 0), "covered");
    vcx.simulate_window_resize(chat, size(px(1100.), px(760.)));
    vcx.run_until_parked();
    assert_eq!(main_draws(vcx), alone, "the right edge shows");
    vcx.simulate_window_resize(chat, covering().size);
    vcx.run_until_parked();
    assert_eq!(main_draws(vcx), (0, 0), "covered again");
    vcx.update_window(chat, |_, window, cx| {
        window.dispatch_action(Box::new(CloseWindow), cx);
    })
    .expect("the chat window");
    vcx.run_until_parked();
    assert!(
        !app.read_with(vcx, |app, _| app.windows.has(OTHER)),
        "closed"
    );
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    assert_eq!(main_draws(vcx), alone, "the chat window closed");
}

/// An active chat window beside the main window, at covering bounds on
/// another display, or fullscreen (a Space of its own on macOS), leaves
/// the main window animating as alone.
#[gpui_kit::test]
fn a_chat_window_that_leaves_the_main_window_in_view_keeps_it_animating(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let alone = main_draws(vcx);
    let beside = Bounds::new(point(px(900.), px(100.)), size(px(600.), px(500.)));
    let chat = open_chat_window(&app, vcx, OTHER, beside);
    assert_eq!(main_draws(vcx), alone, "a chat window beside it");

    vcx.simulate_display_added(DisplayId::new(2), None);
    vcx.simulate_window_move(chat, covering().origin);
    vcx.simulate_window_resize(chat, covering().size);
    vcx.simulate_window_move_to_display(chat, DisplayId::new(2));
    vcx.run_until_parked();
    assert_eq!(main_draws(vcx), alone, "the same bounds on another display");

    vcx.simulate_window_move_to_display(chat, DisplayId::new(1));
    vcx.run_until_parked();
    assert_eq!(main_draws(vcx), (0, 0), "back over it");
    vcx.update_window(chat, |_, window, _| window.toggle_fullscreen())
        .expect("the chat window");
    // Fullscreen resizes the window to its screen.
    vcx.simulate_window_resize(chat, size(px(1920.), px(1080.)));
    vcx.run_until_parked();
    assert_eq!(main_draws(vcx), alone, "a fullscreen chat window");
}

/// The chat window's own stickers draw as many frames covering the main
/// window as beside it, and their ticks no longer redraw the main window
/// under it.
#[gpui_kit::test]
fn a_covering_chat_window_keeps_its_own_stickers_animating(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let beside = Bounds::new(point(px(900.), px(100.)), size(px(600.), px(500.)));
    let chat = open_chat_window(&app, vcx, OPEN, beside);
    for _ in 0..4 {
        app.read_with(vcx, |app, _| app.playback.stickers.settle());
        let _ = frames_over(vcx, 1);
    }
    let shown = frames_of(chat, vcx);
    assert!(shown > 0, "the chat window's stickers animate");
    vcx.simulate_window_move(chat, covering().origin);
    vcx.simulate_window_resize(chat, covering().size);
    vcx.run_until_parked();
    assert_eq!(frames_of(chat, vcx), shown, "covering the main window");
    assert_eq!(main_draws(vcx), (0, 0), "the main window under it");
}
