//! The UI driver in a real window: the forced tree has Quill's elements,
//! a command filed on the bridge comes back with the tree, a click by node
//! id reaches the element, and the gates refuse before anything is sent.
use crate::ui::agent_state::{TEST_LOCK, reset_for_test};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::agent::ui_driver::bridge::{self, BridgeError, Button, Target, UiCommand, UiReply};
use quill::agent::ui_driver::tree::UiTree;
use quill::agent::{access, activity};
use quill::ids::AccountKey;
use std::time::Duration;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-agent-driver-ui");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot
        .borrow()
        .clone()
        .unwrap_or_else(|| panic!("window opened"));
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        vcx.run_until_parked();
        vcx.executor().advance_clock(Duration::from_millis(300));
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// What the agent wake task does in the app: start waiting commands, then
/// let frames and timers run.
fn pump(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.agent_driver_tick(window, cx);
        })
    });
    settle(vcx);
}

/// File `command` as a tool thread would and pump the window until it is
/// answered. (Filed from the test thread: the test scheduler allows no
/// wake-ups from other threads.)
fn drive(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    command: UiCommand,
    expect: Option<Option<i64>>,
) -> Result<UiReply, BridgeError> {
    let ticket = bridge::post("Test agent", command, expect)?;
    for _ in 0..40 {
        pump(app, vcx);
        if let Some(reply) = ticket.poll() {
            return Ok(reply);
        }
    }
    ticket.wait(Duration::from_millis(10))
}

fn turn_on() {
    reset_for_test(&AccountKey::primary());
    access::set_enabled(true);
    access::set_ui_driver(true);
    bridge::set_paused(false);
}

/// The chat open in the window, as the tools learn it before a command.
fn open_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<i64> {
    match drive(app, vcx, UiCommand::State, None) {
        Ok(UiReply::State(state)) => state.open_chat,
        other => panic!("expected the state, got {other:?}"),
    }
}

fn read_tree(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> UiTree {
    let open = open_chat(app, vcx);
    match drive(app, vcx, UiCommand::ReadTree, Some(open)) {
        Ok(UiReply::Tree(tree)) => tree,
        other => panic!("expected a tree, got {other:?}"),
    }
}

#[gpui_kit::test]
fn the_forced_tree_lists_the_window_and_a_click_by_id_lands(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    let _presence = activity::connect("Test agent");
    settle(vcx);
    assert!(
        vcx.update(|window, _| !window.is_a11y_forced()),
        "nothing drove yet, so the tree is not forced"
    );

    let open = open_chat(&app, vcx);
    assert_eq!(open, Some(11), "the demo opens chat 11");
    let tree = read_tree(&app, vcx);
    assert!(
        vcx.update(|window, _| window.is_a11y_forced()),
        "a read forces the tree"
    );
    assert_eq!(tree.width, 1100.0);
    assert_eq!(tree.height, 900.0);
    let summary: Vec<String> = tree
        .nodes
        .iter()
        .map(|n| format!("{} {:?}", n.role, n.name))
        .collect();
    assert!(tree.nodes.len() > 10, "{summary:?}");
    assert!(
        tree.nodes.iter().any(|n| n.role == "Button" && n.bounds.is_some()),
        "{summary:?}"
    );
    assert!(tree.node(&tree.root).is_some_and(|n| n.role == "Window"));

    // Click the first focusable text box (the chat list's search); focus
    // follows the click, which only a real dispatch does.
    let target = tree
        .nodes
        .iter()
        .find(|n| n.role.ends_with("Input") && n.bounds.is_some())
        .map(|n| n.id.clone())
        .unwrap_or_else(|| panic!("a text input in {summary:?}"));
    let reply = drive(
        &app,
        vcx,
        UiCommand::Click {
            target: Target::Node(target.clone()),
            button: Button::Left,
            count: 1,
        },
        Some(open),
    );
    let Ok(UiReply::Done { tree: after, .. }) = reply else {
        panic!("expected Done, got {reply:?}");
    };
    assert!(after.frame > tree.frame, "the answer is from a later frame");
    // GPUI reports a focused node only when the focused element itself has
    // a role; the kit Input's focus handle sits on a wrapper, so `focus`
    // stays None here. The typed value below shows the click took effect.
    assert!(
        vcx.update(|window, cx| window.focused(cx).is_some()),
        "the click focused something"
    );

    // Typing lands in the focused box.
    let reply = drive(
        &app,
        vcx,
        UiCommand::TypeText {
            text: "ann".into(),
        },
        Some(open),
    );
    let Ok(UiReply::Done { tree: typed, .. }) = reply else {
        panic!("expected Done, got {reply:?}");
    };
    assert_eq!(
        typed.node(&target).and_then(|n| n.value.clone()).as_deref(),
        Some("ann"),
        "{:?}",
        typed.node(&target)
    );
}

#[gpui_kit::test]
fn the_gates_refuse_before_anything_is_sent(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    let _presence = activity::connect("Test agent");
    settle(vcx);
    let open = open_chat(&app, vcx);

    // The open chat moved between the grant check and the command.
    let reply = drive(
        &app,
        vcx,
        UiCommand::Click {
            target: Target::Point { x: 10.0, y: 10.0 },
            button: Button::Left,
            count: 1,
        },
        Some(Some(4242)),
    );
    assert!(
        matches!(&reply, Ok(UiReply::Refused(text)) if text.contains("open chat changed")),
        "{reply:?}"
    );

    // A node that is not on screen.
    let reply = drive(
        &app,
        vcx,
        UiCommand::Click {
            target: Target::Node("deadbeef".into()),
            button: Button::Left,
            count: 1,
        },
        Some(open),
    );
    assert!(
        matches!(&reply, Ok(UiReply::Refused(text)) if text.contains("not on screen")),
        "{reply:?}"
    );

    // Keys that would leave Quill.
    let chords = quill::agent::ui_driver::keys::parse_sequence("cmd-q").unwrap();
    let reply = drive(&app, vcx, UiCommand::PressKeys { chords }, Some(open));
    assert!(
        matches!(&reply, Ok(UiReply::Refused(text)) if text.contains("quit")),
        "{reply:?}"
    );

    // The driver switch off: refused at once, no frame needed.
    access::set_ui_driver(false);
    let reply = drive(&app, vcx, UiCommand::ReadTree, Some(open));
    assert!(
        matches!(&reply, Ok(UiReply::Refused(text)) if text.contains("Let agents drive")),
        "{reply:?}"
    );
    access::set_ui_driver(true);

    // Paused by the user's own input.
    bridge::set_paused(true);
    let reply = drive(&app, vcx, UiCommand::ReadTree, Some(open));
    assert!(
        matches!(&reply, Ok(UiReply::Refused(text)) if text.contains("paused")),
        "{reply:?}"
    );
    bridge::set_paused(false);
}

/// The cost of building the tree every frame, printed for the decision
/// doc: `cargo test --features demo-capture --bin quill -- --ignored
/// --nocapture forced_tree_frame_cost`.
#[gpui_kit::test]
#[ignore]
fn forced_tree_frame_cost(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (_app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    settle(vcx);
    let frames = 60u32;
    let measure = |vcx: &mut VisualTestContext, forced: bool| {
        vcx.update(|window, _| window.set_a11y_forced(forced));
        vcx.update(|window, cx| window.render_frame(cx));
        let start = std::time::Instant::now();
        for _ in 0..frames {
            vcx.update(|window, cx| window.render_frame(cx));
        }
        start.elapsed() / frames
    };
    let off = measure(vcx, false);
    let on = measure(vcx, true);
    let off2 = measure(vcx, false);
    let (nodes, read) = vcx.update(|window, cx| {
        window.set_a11y_forced(true);
        window.render_frame(cx);
        let start = std::time::Instant::now();
        let tree = crate::ui::agent_driver_tree::read(window);
        (tree.map(|t| t.nodes.len()).unwrap_or(0), start.elapsed())
    });
    println!(
        "forced tree: frame {off:?} -> {on:?} (off again {off2:?}); {nodes} nodes; snapshot read {read:?}"
    );
}
