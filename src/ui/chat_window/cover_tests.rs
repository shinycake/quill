//! A main window showing two looping stickers, with a chat opened in its
//! own window: the main window draws nothing while the active chat window
//! covers it whole, and animates again, exactly as alone in front, from
//! the moment any of it shows. Every other window keeps animating, whoever
//! asks the app for ticks. Over the demo; nothing reaches Telegram.

use crate::ui::actions::CloseWindow;
use crate::ui::app::QuillApp;
use crate::ui::frame_clock::{follow_app, frames_over};
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Context, DisplayId, Entity, IntoElement, ParentElement,
    Pixels, Render, StyleRefinement, Styled, Subscription, TestAppContext, VisualTestContext,
    Window, WindowBounds, WindowOptions, div, point, px, size,
};
use quill::ids::ChatId;
use quill::sticker_playback::player::DecodedFrames;
use std::cell::Cell;
use std::rc::Rc;
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
    let before = vcx.windows();
    app.update_in(vcx, |app, window, cx| {
        app.open_chat_window_at(chat, Some(bounds), window, cx);
    });
    vcx.run_until_parked();
    let opened: Vec<_> = vcx
        .windows()
        .into_iter()
        .filter(|handle| !before.contains(handle))
        .collect();
    assert_eq!(opened.len(), 1, "one chat window opened");
    let window = opened[0];
    let active = vcx
        .update_window(window, |_, window, _| window.is_window_active())
        .expect("the chat window");
    assert!(active, "the chat window is the active one");
    window
}

/// The main window's animation-layer draws and frames over the next two
/// seconds of the test clock: from the change before, with no time to
/// settle, so animation that starts late counts short.
fn main_draws(vcx: &mut VisualTestContext) -> (u32, u32) {
    let drawn = frames_over(vcx, 2);
    let count = |kind| drawn.get(kind).copied().unwrap_or(0);
    (count("animation layer"), count("(window frame)"))
}

/// [`main_draws`] of the main window alone, from the moment it comes back
/// from behind another app.
fn alone_draws(vcx: &mut VisualTestContext) -> (u32, u32) {
    vcx.deactivate_window();
    let _ = frames_over(vcx, 1);
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    main_draws(vcx)
}

/// The frames `window` draws over the next two seconds, from the change
/// before.
fn frames_of(window: AnyWindowHandle, vcx: &mut VisualTestContext) -> u64 {
    let frame = |vcx: &mut VisualTestContext| {
        vcx.update_window(window, |_, window, _| window.frame_number())
            .expect("the window")
    };
    let before = frame(vcx);
    let _ = frames_over(vcx, 2);
    frame(vcx) - before
}

/// A window over all of the main window, standing out past each edge.
fn covering() -> Bounds<Pixels> {
    Bounds::new(point(px(-20.), px(-30.)), size(px(1160.), px(760.)))
}

/// A window right of the main window, overlapping its right edge.
fn beside() -> Bounds<Pixels> {
    Bounds::new(point(px(900.), px(100.)), size(px(600.), px(500.)))
}

/// Put `chat`'s window state in place (`None`: the main window's). Live
/// mode puts the main window's back at every poll (`poll_live`); input in
/// a chat window leaves that window's until then.
fn scope(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, chat: Option<ChatId>) {
    app.update(vcx, |app, _| app.set_window_scope(chat));
}

/// A window like the group call's: its view follows the app, and its
/// content asks the app for ticks while it draws (the microphone halo,
/// the speaking blobs), in whichever window's state is in place. It holds
/// a cached view that does not follow the app, like a call's picture
/// between its own frames.
struct CallWindow {
    app: Entity<QuillApp>,
    picture: Entity<Built>,
    _follow: Subscription,
}

impl Render for CallWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.app
            .update(cx, |app, cx| app.request_animation_tick(30, cx));
        div().size_full().child(
            self.picture
                .clone()
                .cached(StyleRefinement::default().size_full()),
        )
    }
}

/// A view that counts how often it is built, and follows nothing.
struct Built(Rc<Cell<u32>>);

impl Render for Built {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.set(self.0.get() + 1);
        div().size_full()
    }
}

/// Open a window at `origin` off to the side, not active (it never takes
/// focus here).
fn open_side_window<V: Render>(
    vcx: &mut VisualTestContext,
    origin: gpui_kit::Point<Pixels>,
    build: impl FnOnce(&mut Context<V>) -> V + 'static,
) -> AnyWindowHandle {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            origin,
            size(px(360.), px(480.)),
        ))),
        ..Default::default()
    };
    let window = vcx.update(|_, cx| {
        cx.open_window(options, |_, cx| cx.new(build))
            .expect("the window")
    });
    vcx.run_until_parked();
    window.into()
}

/// Open a [`CallWindow`]; `builds` counts its picture's builds.
fn open_call_window(
    app: &Entity<QuillApp>,
    builds: &Rc<Cell<u32>>,
    vcx: &mut VisualTestContext,
) -> AnyWindowHandle {
    let (app, builds) = (app.clone(), builds.clone());
    open_side_window(vcx, point(px(1200.), px(0.)), move |cx| CallWindow {
        _follow: follow_app(&app, cx),
        app,
        picture: cx.new(|_| Built(builds)),
    })
}

/// Open a window that does not follow the app and draws by its own clock,
/// like Picture-in-Picture or a notification popup.
fn open_own_clock_window(vcx: &mut VisualTestContext) -> AnyWindowHandle {
    open_side_window(vcx, point(px(1200.), px(500.)), |_| Built(Rc::default()))
}

/// Open a kit dialog in `window` that asks the app for ticks while it is
/// built, as the AI style preview does while its examples load. The kit
/// builds it after the chat window's view has put back the state it found.
fn open_ticking_dialog(
    app: &Entity<QuillApp>,
    window: AnyWindowHandle,
    vcx: &mut VisualTestContext,
) {
    let app = app.clone();
    vcx.update_window(window, |_, window, cx| {
        window.open_dialog(cx, move |dialog, _, cx| {
            app.update(cx, |app, cx| app.request_animation_tick(30, cx));
            dialog.child("Loading examples")
        });
    })
    .expect("the chat window");
    vcx.run_until_parked();
}

/// Covered whole by the active chat window, the main window draws nothing;
/// moved so a strip of the main window shows, it animates as alone.
#[gpui_kit::test]
fn a_covered_main_window_draws_nothing_until_the_chat_window_moves(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let alone = alone_draws(vcx);
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
    let alone = alone_draws(vcx);
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

/// Moved or resized so part of it leaves the chat window's frame, the
/// main window animates as alone; moved or resized within it, it stays
/// still.
#[gpui_kit::test]
fn a_covered_main_window_animates_once_moved_or_resized_out(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let alone = alone_draws(vcx);
    let main = vcx.update(|window, _| window.window_handle());
    let _chat = open_chat_window(&app, vcx, OTHER, covering());
    assert_eq!(main_draws(vcx), (0, 0), "covered");
    vcx.simulate_window_move(main, point(px(10.), px(20.)));
    vcx.run_until_parked();
    assert_eq!(main_draws(vcx), (0, 0), "moved within it");
    // The chat window ends at 1140: 50 px of the main window show.
    vcx.simulate_window_move(main, point(px(90.), px(20.)));
    vcx.run_until_parked();
    assert_eq!(main_draws(vcx), alone, "moved partly out");
    vcx.simulate_window_move(main, point(px(0.), px(0.)));
    vcx.run_until_parked();
    assert_eq!(main_draws(vcx), (0, 0), "moved back");
    vcx.simulate_window_resize(main, size(px(1000.), px(700.)));
    vcx.run_until_parked();
    assert_eq!(main_draws(vcx), (0, 0), "narrowed within it");
    vcx.simulate_window_resize(main, size(px(1100.), px(760.)));
    vcx.run_until_parked();
    assert_eq!(main_draws(vcx), alone, "its bottom edge shows");
}

/// An active chat window beside the main window, at covering bounds on
/// another display, or fullscreen (a Space of its own on macOS), leaves
/// the main window animating as alone.
#[gpui_kit::test]
fn a_chat_window_that_leaves_the_main_window_in_view_keeps_it_animating(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let alone = alone_draws(vcx);
    let chat = open_chat_window(&app, vcx, OTHER, beside());
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
    let chat = open_chat_window(&app, vcx, OPEN, beside());
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

/// Beside the main window, a chat window's stickers redraw that window
/// alone: no main window slice renders for them, and `QuillApp` neither.
#[gpui_kit::test]
fn a_chat_window_beside_the_main_window_ticks_alone(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let chat = open_chat_window(&app, vcx, OPEN, beside());
    for _ in 0..4 {
        app.read_with(vcx, |app, _| app.playback.stickers.settle());
        let _ = frames_over(vcx, 1);
    }
    let frame = |vcx: &mut VisualTestContext| {
        vcx.update_window(chat, |_, window, _| window.frame_number())
            .expect("the chat window")
    };
    let before = frame(vcx);
    let drawn = frames_over(vcx, 2);
    assert!(frame(vcx) > before + 5, "its stickers animate: {drawn:?}");
    for main in ["app", "sidebar", "conversation", "composer"] {
        assert_eq!(drawn.get(main), None, "the main window: {drawn:?}");
    }
}

/// A call window's own animation (the group call's microphone halo and
/// speaking blobs) asks the app for ticks: it plays on while the active
/// chat window covers the main window, in the main window's state as live
/// mode leaves it, or in the chat window's, and the main window under
/// that chat window stays still.
#[gpui_kit::test]
fn a_call_window_keeps_animating_under_a_covering_chat_window(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let call = open_call_window(&app, &Rc::default(), vcx);
    let alone = frames_of(call, vcx);
    assert!(alone > 0, "the call window animates");
    let chat = open_chat_window(&app, vcx, OTHER, beside());
    scope(&app, vcx, None);
    assert_eq!(frames_of(call, vcx), alone, "a chat window beside");
    vcx.simulate_window_move(chat, covering().origin);
    vcx.simulate_window_resize(chat, covering().size);
    vcx.run_until_parked();
    scope(&app, vcx, None);
    assert_eq!(frames_of(call, vcx), alone, "the main window's state");
    assert_eq!(main_draws(vcx), (0, 0), "the main window under it");
    scope(&app, vcx, Some(OTHER));
    assert_eq!(frames_of(call, vcx), alone, "the chat window's state");
    assert_eq!(main_draws(vcx), (0, 0), "the main window under it");
}

/// A dialog in the covering chat window, built in the state the window
/// found (the main window's, in live mode), animates there as beside the
/// main window; the main window under it stays still.
#[gpui_kit::test]
fn a_dialog_in_a_covering_chat_window_keeps_animating(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let chat = open_chat_window(&app, vcx, OTHER, beside());
    scope(&app, vcx, None);
    let quiet = frames_of(chat, vcx);
    open_ticking_dialog(&app, chat, vcx);
    let shown = frames_of(chat, vcx);
    assert!(
        shown > quiet + 10,
        "the dialog animates: {shown} vs {quiet}"
    );
    vcx.simulate_window_move(chat, covering().origin);
    vcx.simulate_window_resize(chat, covering().size);
    vcx.run_until_parked();
    scope(&app, vcx, None);
    assert_eq!(frames_of(chat, vcx), shown, "covering the main window");
    assert_eq!(main_draws(vcx), (0, 0), "the main window under it");
}

/// While the main window is covered, a tick for the whole app reaches the
/// windows that follow the app and nothing else, as with the main window
/// in view: a window that draws by its own clock draws no frame, and a
/// cached view in the call window that does not follow the app is not
/// built again, while the call window animates.
#[gpui_kit::test]
fn a_covered_main_window_leaves_windows_and_views_that_do_not_follow_the_app_alone(
    cx: &mut TestAppContext,
) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let builds = Rc::default();
    let call = open_call_window(&app, &builds, vcx);
    let own_clock = open_own_clock_window(vcx);
    let alone = frames_of(call, vcx);
    assert!(alone > 0, "the call window animates");
    let chat = open_chat_window(&app, vcx, OTHER, beside());
    for covered in [false, true] {
        if covered {
            vcx.simulate_window_move(chat, covering().origin);
            vcx.simulate_window_resize(chat, covering().size);
            vcx.run_until_parked();
        }
        scope(&app, vcx, None);
        let built = builds.get();
        let call_frames = frames_of(call, vcx);
        let rebuilt = builds.get() - built;
        let own_clock_frames = frames_of(own_clock, vcx);
        assert_eq!(
            (call_frames, rebuilt, own_clock_frames),
            (alone, 0, 0),
            "call window frames, picture builds, own-clock window frames; covered: {covered}"
        );
    }
    assert_eq!(main_draws(vcx), (0, 0), "the main window under it");
}

/// A chat window inside the main window's bounds, away from its edges.
fn inside() -> Bounds<Pixels> {
    Bounds::new(point(px(100.), px(100.)), size(px(400.), px(300.)))
}

/// A chat window with stickers hidden whole behind the active chat window
/// draws nothing, and draws as before at once when it shows again.
#[gpui_kit::test]
fn a_chat_window_hidden_behind_the_active_one_draws_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let under = open_chat_window(&app, vcx, OPEN, inside());
    for _ in 0..4 {
        app.read_with(vcx, |app, _| app.playback.stickers.settle());
        let _ = frames_over(vcx, 1);
    }
    let front = open_chat_window(&app, vcx, OTHER, beside());
    let shown = frames_of(under, vcx);
    assert!(shown > 0, "its stickers animate beside the active window");

    vcx.simulate_window_move(front, covering().origin);
    vcx.simulate_window_resize(front, covering().size);
    vcx.run_until_parked();
    assert_eq!(frames_of(under, vcx), 0, "hidden whole: nothing drawn");

    vcx.simulate_window_resize(front, beside().size);
    vcx.simulate_window_move(front, beside().origin);
    vcx.run_until_parked();
    assert_eq!(frames_of(under, vcx), shown, "shown again");
}
