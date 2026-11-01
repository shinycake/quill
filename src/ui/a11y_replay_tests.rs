//! While something reads the accessibility tree, a slice that replays its
//! last frame keeps its part of the tree: its nodes, the focus it reports,
//! the tags readers see, and the actions assistive technology can ask for.
//! Over the `ready-chats` demo; nothing reaches Telegram.
#![cfg(test)]
#![cfg(feature = "demo-capture")]

use crate::ui::app::QuillApp;
use crate::ui::frame_clock::take_slice_renders;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::accesskit::{ActionData, NodeId};
use gpui_kit::component::Root;
use gpui_kit::{
    AccessibleAction, AppContext, Entity, Focusable, Modifiers, TestAppContext, VisualTestContext,
    point, px, size,
};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    (app, vcx)
}

/// The composer takes the focus and the tree is built from the next frame.
fn read_the_tree_with_the_composer_focused(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| input.focus(window, cx));
        });
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        window.set_a11y_forced(true);
        window.draw(cx).clear(cx);
    });
}

/// The last tree's nodes (as text) and focus.
fn tree(vcx: &mut VisualTestContext) -> (String, Option<NodeId>) {
    vcx.update(|window, _| {
        let tree = window.a11y_tree_snapshot().expect("a tree");
        (format!("{:?}", tree.nodes), tree.focus)
    })
}

/// Draws a frame in which only the conversation slice renders: the chat
/// list and the composer replay their last frame.
fn redraw_only_the_conversation(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let (slice, _) = app
        .read_with(vcx, |app, _| app.slices.conversation_ids())
        .expect("the conversation slice");
    let _ = take_slice_renders();
    vcx.update(|_, cx| cx.notify(slice));
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let renders = take_slice_renders();
    assert!(renders.contains_key("conversation"), "{renders:?}");
    assert!(!renders.contains_key("sidebar"), "{renders:?}");
    assert!(!renders.contains_key("composer"), "{renders:?}");
}

#[gpui_kit::test]
fn replayed_slices_keep_their_nodes_focus_and_tags(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    // Over the chat list.
    vcx.simulate_mouse_move(point(px(150.), px(300.)), None, Modifiers::none());
    read_the_tree_with_the_composer_focused(&app, vcx);
    let before = tree(vcx);
    let focused = vcx.update(|window, _| window.focused_a11y_tags());
    let hovered = vcx.update(|window, _| window.hovered_a11y_tags());
    assert!(before.1.is_some(), "the composer reports the focus");
    assert!(
        !focused.is_empty(),
        "the composer is painted inside tagged regions"
    );
    assert!(!hovered.is_empty(), "the chat list is tagged");

    redraw_only_the_conversation(&app, vcx);
    assert_eq!(tree(vcx), before);
    assert_eq!(vcx.update(|window, _| window.focused_a11y_tags()), focused);
    assert_eq!(vcx.update(|window, _| window.hovered_a11y_tags()), hovered);
}

/// Draws the frame of a tick for the conversation's animation layer.
/// Returns how many frames built a tree over it.
fn tick_the_layer(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> u64 {
    let (_, painter) = app
        .read_with(vcx, |app, _| app.slices.conversation_ids())
        .expect("the conversation slice");
    let progress = |vcx: &mut VisualTestContext| {
        vcx.update(|window, _| (window.frame_number(), window.a11y_tree_frame()))
    };
    let (frames, trees) = progress(vcx);
    vcx.update(|_, cx| cx.notify(painter));
    vcx.run_until_parked();
    let (frames_after, trees_after) = progress(vcx);
    assert_eq!(frames_after, frames + 1, "the tick draws a frame");
    trees_after - trees
}

/// A tick builds no tree and sends none: layers add no nodes, so the tree
/// the last whole frame sent stands, focus included.
#[gpui_kit::test]
fn a_layer_tick_leaves_the_tree_as_it_is(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    read_the_tree_with_the_composer_focused(&app, vcx);
    let before = tree(vcx);
    let focused = vcx.update(|window, _| window.focused_a11y_tags());
    assert!(before.1.is_some(), "the composer reports the focus");

    assert_eq!(tick_the_layer(&app, vcx), 0);
    assert_eq!(tree(vcx), before);
    assert_eq!(vcx.update(|window, _| window.focused_a11y_tags()), focused);

    redraw_only_the_conversation(&app, vcx);
    assert_eq!(
        tree(vcx).1,
        before.1,
        "a whole frame after it keeps the focus"
    );
}

#[gpui_kit::test]
fn a_replayed_composer_takes_a_value_from_assistive_technology(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    read_the_tree_with_the_composer_focused(&app, vcx);
    redraw_only_the_conversation(&app, vcx);

    let (_, focus) = tree(vcx);
    let composer = focus.expect("the composer reports the focus");
    vcx.update(|window, cx| {
        window.simulate_a11y_action(
            composer,
            AccessibleAction::SetValue,
            Some(ActionData::Value("hello".into())),
            cx,
        );
    });
    let text = app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string());
    assert_eq!(text, "hello");
}
