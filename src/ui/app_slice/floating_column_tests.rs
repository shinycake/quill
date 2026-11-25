//! The floating header and composer through GPUI: where the pills sit at
//! three widths, that Classic draws none, and that a typing peer's dots
//! tick in the header's own layer without the history.

use crate::ui::app::QuillApp;
use crate::ui::appearance::own_appearance_prefs;
use crate::ui::frame_clock::{frames_over, take_slice_renders};
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::layout_style::LayoutStyle;

fn open(
    cx: &mut TestAppContext,
    demo: &'static str,
    width: f32,
    layout: LayoutStyle,
) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    crate::ui::interface_zoom::set_initial_zoom(1.);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(demo);
    let handle = cx.open_window(size(px(width), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update(&mut vcx, |app, cx| {
        app.set_appearance(cx, |a| a.layout = layout);
    });
    vcx.run_until_parked();
    for _ in 0..4 {
        vcx.update(|window, cx| {
            window.activate_window();
            window.render_frame(cx);
        });
        vcx.run_until_parked();
    }
    (app, vcx)
}

/// The pills sit a gutter inside the chat, centred, at most a column wide.
fn assert_pills(vcx: &mut VisualTestContext, column: f32) {
    let slot = vcx.debug_bounds("conversation-slot").expect("chat");
    let top = vcx.debug_bounds("conversation-top").expect("header lane");
    let composer = vcx.debug_bounds("conversation-composer").expect("composer");
    assert_eq!(top.top(), slot.top() + px(16.), "header under the gutter");
    assert_eq!(
        composer.bottom(),
        slot.bottom() - px(16.),
        "composer over it"
    );
    assert_eq!(composer.size.width, px(column), "the column's width");
    let left = composer.left() - slot.left();
    let right = slot.right() - composer.right();
    assert!((left - right).abs() < px(1.), "centred: {left:?} {right:?}");
}

#[gpui_kit::test]
fn the_pills_float_in_a_wide_chat(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-pills-wide");
    let (_app, mut vcx) = open(cx, "ready-chats", 1440., LayoutStyle::Floating);
    assert_pills(&mut vcx, 696.);
}

#[gpui_kit::test]
fn the_pills_keep_their_gutters_in_the_drawer_band(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-pills-narrow");
    let (_app, mut vcx) = open(cx, "ready-chats", 700., LayoutStyle::Floating);
    let slot = vcx.debug_bounds("conversation-slot").expect("chat");
    assert_eq!(slot.size.width, px(700.), "the chat takes the window");
    assert_pills(&mut vcx, 700. - 32.);
}

#[gpui_kit::test]
fn classic_stacks_its_bars(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-pills-classic");
    let (_app, mut vcx) = open(cx, "ready-chats", 1440., LayoutStyle::Classic);
    assert!(vcx.debug_bounds("conversation-top").is_none());
    assert!(vcx.debug_bounds("conversation-composer").is_none());
}

#[gpui_kit::test]
fn typing_dots_tick_in_the_header_alone(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-pills-typing");
    let (_app, mut vcx) = open(cx, "ready-typing", 1440., LayoutStyle::Floating);
    let vcx = &mut vcx;
    let _ = frames_over(vcx, 1);
    let drawn = frames_over(vcx, 2);
    let count = |kind: &str| drawn.get(kind).copied().unwrap_or(0);
    assert!(count("animation layer") > 5, "the dots tick: {drawn:?}");
    for kind in ["app", "sidebar", "conversation", "header", "composer"] {
        assert_eq!(count(kind), 0, "{kind} stayed: {drawn:?}");
    }
    let _ = take_slice_renders();
}

/// The rows lay out in the column the pills sit in: their bubble room is
/// the column's, and every row stays between the pills' edges.
fn assert_rows_in_column(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, column: f32) {
    let composer = vcx.debug_bounds("conversation-composer").expect("composer");
    let (width, rows) = app.read_with(vcx, |app, _| {
        (
            app.history.measure.get().width(),
            app.history.hit_rows.borrow().clone(),
        )
    });
    let laid = quill::bubble_layout::HistoryWidth::new(
        crate::ui::chat_wide_ui::history_pane_width(column),
    );
    assert_eq!(width, laid, "bubbles are laid out for the column");
    assert!(!rows.is_empty(), "rows painted");
    for (ix, row) in rows {
        assert!(
            row.left() >= composer.left() && row.right() <= composer.right(),
            "row {ix} at {:?}..{:?}, column {:?}..{:?}",
            row.left(),
            row.right(),
            composer.left(),
            composer.right()
        );
    }
}

#[gpui_kit::test]
fn wide_rows_sit_in_the_centred_column(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-pills-rows-wide");
    let (app, mut vcx) = open(cx, "ready-chats", 1440., LayoutStyle::Floating);
    assert_rows_in_column(&app, &mut vcx, 696.);
}

#[gpui_kit::test]
fn narrow_rows_take_the_pane_less_its_gutters(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-pills-rows-narrow");
    let (app, mut vcx) = open(cx, "ready-chats", 700., LayoutStyle::Floating);
    assert_rows_in_column(&app, &mut vcx, 700. - 32.);
}
