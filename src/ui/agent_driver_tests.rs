//! The UI driver in a real window: the tree is cut to the granted chats, a
//! blocked surface answers with a note and refuses input, a command stops
//! when the chat under it changes, points land only on allowed regions,
//! the composer's send goes to the agent path, and a tool that gave up
//! never gets its input injected.
use crate::ui::agent_state::{TEST_LOCK, reset_for_test};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::agent::grants::{self, ChatGrant, Scope};
use quill::agent::prompts::{self, Ask};
use quill::agent::ui_driver::bridge::{self, BridgeError, Button, Target, UiCommand, UiReply};
use quill::agent::ui_driver::keys::parse_sequence;
use quill::agent::ui_driver::tree::UiTree;
use quill::agent::{access, activity};
use quill::ids::AccountKey;
use std::time::Duration;

pub(super) fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
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

pub(super) fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        vcx.run_until_parked();
        vcx.executor().advance_clock(Duration::from_millis(300));
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// What the agent wake task does in the app: start waiting commands, then
/// let frames and timers run.
pub(super) fn pump(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
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
pub(super) fn drive(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    command: UiCommand,
) -> Result<UiReply, BridgeError> {
    let ticket = bridge::post("Test agent", command)?;
    for _ in 0..40 {
        pump(app, vcx);
        if let Some(reply) = ticket.poll() {
            return Ok(reply);
        }
    }
    ticket.wait(Duration::from_millis(10))
}

/// Access and the driver on, no grants.
pub(super) fn turn_on() {
    reset_for_test(&AccountKey::primary());
    access::set_enabled(true);
    access::set_ui_driver(true);
    bridge::set_paused(false);
}

pub(super) fn grant(chat_id: i64, scope: Scope) {
    grants::update(&AccountKey::primary(), |g| {
        g.chats.insert(
            chat_id,
            ChatGrant {
                scope: Some(scope),
                title: format!("chat {chat_id}"),
            },
        );
    });
}

pub(super) fn read_tree(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> UiTree {
    match drive(app, vcx, UiCommand::ReadTree) {
        Ok(UiReply::Tree(tree)) => tree,
        other => panic!("expected a tree, got {other:?}"),
    }
}

pub(super) fn click(target: Target) -> UiCommand {
    UiCommand::Click {
        target,
        button: Button::Left,
        count: 1,
    }
}

pub(super) fn refused(reply: &Result<UiReply, BridgeError>, part: &str) -> bool {
    matches!(reply, Ok(UiReply::Refused(text)) if text.contains(part))
}

/// The ids of the chats whose rows are in `tree` (`chat=<id>` tags).
fn chat_rows(tree: &UiTree) -> Vec<i64> {
    let mut ids: Vec<i64> = tree
        .nodes
        .iter()
        .filter(|node| node.role == "Button")
        .filter_map(|node| node.tag.as_deref())
        .filter_map(quill::agent::ui_driver::redact::chat_of_tag)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub(super) fn node_named<'a>(
    tree: &'a UiTree,
    name: &str,
) -> Option<&'a quill::agent::ui_driver::tree::UiNode> {
    tree.nodes
        .iter()
        .find(|node| node.name.as_deref().is_some_and(|n| n.contains(name)))
}

/// The composer's text box (the chat list's search box is a multiline
/// input too).
pub(super) fn message_box(tree: &UiTree) -> String {
    tree.nodes
        .iter()
        .find(|n| n.role == "MultilineTextInput" && n.name.as_deref() == Some("Message"))
        .map(|n| n.id.clone())
        .unwrap_or_else(|| panic!("the message box"))
}

#[gpui_kit::test]
fn the_tree_has_granted_rows_only_and_a_click_by_id_lands(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    let _presence = activity::connect("Test agent");
    settle(vcx);

    // The demo opens chat 11, which has no grant: nothing to see.
    let reply = drive(&app, vcx, UiCommand::ReadTree);
    assert!(refused(&reply, "ungranted chat"), "{reply:?}");

    grant(11, Scope::ReadWrite);
    grant(13, Scope::Read);
    let tree = read_tree(&app, vcx);
    assert!(vcx.update(|window, _| window.is_a11y_forced()));
    assert_eq!(tree.width, 1100.0);
    let summary: Vec<String> = tree
        .nodes
        .iter()
        .map(|n| format!("{} {:?} {:?}", n.role, n.name, n.tag))
        .collect();
    if chat_rows(&tree) != [11, 13] {
        let raw = vcx.update(|window, _| crate::ui::agent_driver_tree::raw_tree(window));
        let raw: Vec<String> = raw
            .iter()
            .flat_map(|tree| tree.nodes.iter())
            .map(|n| {
                format!(
                    "{} {} {:?} {:?} {:?}",
                    n.id, n.role, n.name, n.tag, n.children
                )
            })
            .collect();
        panic!("cut tree {summary:?}\nraw tree {raw:#?}");
    }
    assert!(
        node_named(&tree, "Demo chat B").is_none(),
        "the ungranted chat's title is nowhere: {summary:?}"
    );
    assert!(
        tree.node(&tree.root)
            .is_some_and(|root| root.role == "Window" && root.name.is_none()),
        "the window root stays as a nameless ancestor"
    );

    // Click the chat list's search box; focus follows the click, which only
    // a real dispatch does.
    let target = tree
        .nodes
        .iter()
        .find(|n| n.role.ends_with("Input") && n.bounds.is_some())
        .map(|n| n.id.clone())
        .unwrap_or_else(|| panic!("a text input in {summary:?}"));
    let reply = drive(&app, vcx, click(Target::Node(target.clone())));
    let Ok(UiReply::Done {
        tree: after,
        refusals,
        ..
    }) = reply
    else {
        panic!("expected Done, got {reply:?}");
    };
    assert!(after.frame > tree.frame, "the answer is from a later frame");
    assert!(refusals.is_empty(), "{refusals:?}");
    assert!(vcx.update(|window, cx| window.focused(cx).is_some()));

    // Typing lands in the focused box; the activity log has both commands.
    let reply = drive(&app, vcx, UiCommand::TypeText { text: "ann".into() });
    let Ok(UiReply::Done { tree: typed, .. }) = reply else {
        panic!("expected Done, got {reply:?}");
    };
    assert_eq!(
        typed.node(&target).and_then(|n| n.value.clone()).as_deref(),
        Some("ann"),
        "{:?}",
        typed.node(&target)
    );
    let log = activity::recent(&AccountKey::primary());
    let tools: Vec<&str> = log.iter().map(|a| a.tool.as_str()).collect();
    assert_eq!(tools, ["ui_type", "ui_click"], "{log:?}");
    assert_eq!(
        log.first().map(|a| a.summary.as_str()),
        Some("type \"ann\"")
    );
    assert!(log.iter().all(|a| a.chat_id == 11 && a.ok));
}

#[gpui_kit::test]
fn a_blocked_surface_answers_with_a_note_and_takes_no_input(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);
    let tree = read_tree(&app, vcx);
    let search = tree
        .nodes
        .iter()
        .find(|n| n.role.ends_with("Input") && n.bounds.is_some())
        .and_then(|n| n.center())
        .unwrap_or_else(|| panic!("a text input"));

    // Settings open: a note instead of a tree, and no click, even by point
    // on what was the search box.
    vcx.update(|_, cx| {
        app.update(cx, |app, _| app.open_settings(None));
    });
    settle(vcx);
    let reply = drive(&app, vcx, UiCommand::ReadTree);
    assert!(refused(&reply, "Quill is showing Settings"), "{reply:?}");
    let reply = drive(
        &app,
        vcx,
        click(Target::Point {
            x: search.0,
            y: search.1,
        }),
    );
    assert!(refused(&reply, "Settings"), "{reply:?}");
    assert!(vcx.update(|_, cx| app.read(cx).settings.open), "still open");
    vcx.update(|_, cx| {
        app.update(cx, |app, _| app.close_settings());
    });
    settle(vcx);

    // An agent's own request on screen: blocked, and the Allow button
    // cannot be clicked by point. The request is still waiting after.
    let account = AccountKey::primary();
    let (id, _rx) = prompts::file(
        account.clone(),
        "Test agent".into(),
        Ask::Chat {
            chat_id: 12,
            title: "Demo chat B".into(),
            secret: false,
            scope: Scope::Read,
        },
        String::new(),
    )
    .unwrap_or_else(|| panic!("filed"));
    settle(vcx);
    assert!(
        vcx.update(|_, cx| app.read(cx).kit_dialog_open()),
        "dialog shows"
    );
    let reply = drive(&app, vcx, UiCommand::ReadTree);
    assert!(refused(&reply, "a request to the user"), "{reply:?}");
    let allow = vcx.update(|window, _| window.find("agent-request-allow").bounds().center());
    let reply = drive(
        &app,
        vcx,
        click(Target::Point {
            x: allow.x.into(),
            y: allow.y.into(),
        }),
    );
    assert!(refused(&reply, "a request to the user"), "{reply:?}");
    assert_eq!(
        prompts::current_for(&account).map(|p| p.id),
        Some(id),
        "the request is still waiting"
    );
    assert_eq!(
        grants::for_account(&account).scope_for(12, grants::ChatClass::Other),
        None
    );
    prompts::deny_all();
}

#[gpui_kit::test]
fn a_key_that_opens_an_ungranted_chat_stops_the_sequence(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);
    let tree = read_tree(&app, vcx);
    // Keys reach the app's handlers through the focused element.
    let reply = drive(&app, vcx, click(Target::Node(message_box(&tree))));
    assert!(matches!(reply, Ok(UiReply::Done { .. })), "{reply:?}");
    // GPUI recorded the tags above the focused message box: keys are
    // checked against them the way a click is checked against the hover.
    let focus_tags = vcx.update(|window, _| window.focused_a11y_tags());
    assert!(
        focus_tags
            .iter()
            .any(|tag| tag.as_ref() == quill::agent::ui_driver::redact::REGION_CHAT),
        "{focus_tags:?}"
    );
    assert!(vcx.update(|window, cx| app.read(cx).agent_focus_allowed(window)));

    // "Next chat" from chat 11 lands on chat 12, which is not granted: the
    // handler refuses to open it, the second chord is not pressed, and the
    // reply carries the refusal.
    let chords = parse_sequence("ctrl-tab ctrl-tab").unwrap_or_default();
    let reply = drive(&app, vcx, UiCommand::PressKeys { chords });
    let Ok(UiReply::Done { tree, refusals, .. }) = reply else {
        panic!("expected Done, got {reply:?}");
    };
    assert!(
        refusals.iter().any(|r| r.contains("not granted")),
        "{refusals:?}"
    );
    assert_eq!(
        vcx.update(|_, cx| app.read(cx).open_chat_id().map(|c| c.0)),
        Some(11),
        "the ungranted chat did not open"
    );
    assert!(node_named(&tree, "Demo chat B").is_none());
    let log = activity::recent(&AccountKey::primary());
    assert_eq!(log.first().map(|a| a.tool.as_str()), Some("ui_keys"));

    // Space and Enter go nowhere while no text box has the focus.
    vcx.update(|window, cx| window.blur(cx));
    settle(vcx);
    assert!(
        vcx.update(|window, _| window.focused_a11y_tags())
            .is_empty()
    );
    assert!(!vcx.update(|window, cx| app.read(cx).agent_focus_allowed(window)));
    let chords = parse_sequence("space").unwrap_or_default();
    let reply = drive(&app, vcx, UiCommand::PressKeys { chords });
    assert!(refused(&reply, "click_element"), "{reply:?}");
    let reply = drive(&app, vcx, UiCommand::TypeText { text: "x".into() });
    assert!(refused(&reply, "No text box"), "{reply:?}");

    // Keys that would leave Quill, a node that is not on screen, the
    // switch off and the pause: refused before anything is sent.
    let chords = parse_sequence("cmd-q").unwrap_or_default();
    let reply = drive(&app, vcx, UiCommand::PressKeys { chords });
    assert!(refused(&reply, "quit"), "{reply:?}");
    // The clipboard is the user's: the paste chord is refused (a paste
    // dispatched any other way reads an empty clipboard through the
    // `ZoomPlatform`, which the test platform does not go through).
    let chords = parse_sequence("cmd-v").unwrap_or_default();
    let reply = drive(&app, vcx, UiCommand::PressKeys { chords });
    assert!(refused(&reply, "clipboard"), "{reply:?}");
    let reply = drive(&app, vcx, click(Target::Node("deadbeef".into())));
    assert!(refused(&reply, "not on screen"), "{reply:?}");
    access::set_ui_driver(false);
    let reply = drive(&app, vcx, UiCommand::ReadTree);
    assert!(refused(&reply, "Let agents drive"), "{reply:?}");
    access::set_ui_driver(true);
    bridge::set_paused(true);
    let reply = drive(&app, vcx, UiCommand::ReadTree);
    assert!(refused(&reply, "paused"), "{reply:?}");
    bridge::set_paused(false);
}

#[gpui_kit::test]
fn the_clicked_message_box_is_the_focused_node(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);
    let tree = read_tree(&app, vcx);
    let target = message_box(&tree);

    // The text input's role sits on its frame and the editor inside takes
    // the focus; the frame is what an agent (and a screen reader) sees
    // focused, in the click's own reply, after typing and in a fresh read.
    let focused = |tree: &UiTree| {
        assert_eq!(tree.focus.as_deref(), Some(target.as_str()), "focus");
        assert!(
            tree.node(&target)
                .is_some_and(|node| node.states.iter().any(|s| s == "focused")),
            "{:?}",
            tree.node(&target)
        );
    };
    let reply = drive(&app, vcx, click(Target::Node(target.clone())));
    let Ok(UiReply::Done { tree: clicked, .. }) = reply else {
        panic!("expected Done, got {reply:?}");
    };
    focused(&clicked);
    let reply = drive(&app, vcx, UiCommand::TypeText { text: "hi".into() });
    let Ok(UiReply::Done { tree: typed, .. }) = reply else {
        panic!("expected Done, got {reply:?}");
    };
    focused(&typed);
    focused(&read_tree(&app, vcx));

    // Focus elsewhere: the message box is no longer reported.
    vcx.update(|window, cx| window.blur(cx));
    settle(vcx);
    let blurred = read_tree(&app, vcx);
    assert_ne!(blurred.focus.as_deref(), Some(target.as_str()));
}

#[gpui_kit::test]
fn the_composer_send_goes_to_the_agent_path_and_read_only_refuses(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);
    let tree = read_tree(&app, vcx);
    let composer = message_box(&tree);
    let before = vcx.update(|_, cx| {
        app.read(cx)
            .demo_session
            .as_ref()
            .map(|s| s.histories.get(&11).map(|h| h.messages.len()).unwrap_or(0))
            .unwrap_or(0)
    });
    let reply = drive(&app, vcx, click(Target::Node(composer)));
    assert!(matches!(reply, Ok(UiReply::Done { .. })), "{reply:?}");
    let reply = drive(
        &app,
        vcx,
        UiCommand::TypeText {
            text: "hi there".into(),
        },
    );
    assert!(matches!(reply, Ok(UiReply::Done { .. })), "{reply:?}");

    // Enter in the message box: the window keeps the draft and hands the
    // request over; nothing was appended to the chat.
    let chords = parse_sequence("enter").unwrap_or_default();
    let reply = drive(&app, vcx, UiCommand::PressKeys { chords });
    let Ok(UiReply::Done {
        ui_send, refusals, ..
    }) = reply
    else {
        panic!("expected Done, got {reply:?}");
    };
    let send = ui_send.unwrap_or_else(|| panic!("a send handed over: {refusals:?}"));
    assert_eq!(send.chat_id, 11);
    assert_eq!(send.text, "hi there");
    assert_eq!(send.request["@type"], "sendMessage");
    assert_eq!(send.request["chat_id"], 11);
    assert_eq!(
        send.request["input_message_content"]["text"]["text"],
        "hi there"
    );
    assert!(send.request.get("@extra").is_none());
    let (after, draft) = vcx.update(|_, cx| {
        let app = app.read(cx);
        (
            app.demo_session
                .as_ref()
                .map(|s| s.histories.get(&11).map(|h| h.messages.len()).unwrap_or(0))
                .unwrap_or(0),
            app.composer_markup(cx),
        )
    });
    assert_eq!(after, before, "the window sent nothing itself");
    assert_eq!(
        draft, "hi there",
        "the draft stays until the agent path sends"
    );

    // The agent path reports the send: the box clears.
    let reply = drive(
        &app,
        vcx,
        UiCommand::ComposerSent {
            chat_id: 11,
            text: "hi there".into(),
        },
    );
    assert!(matches!(reply, Ok(UiReply::Ok)), "{reply:?}");
    assert_eq!(vcx.update(|_, cx| app.read(cx).composer_markup(cx)), "");

    // A chat granted for reading only takes no input at all.
    grant(11, Scope::Read);
    let reply = drive(&app, vcx, UiCommand::TypeText { text: "x".into() });
    assert!(refused(&reply, "reading only"), "{reply:?}");
    assert!(matches!(
        drive(&app, vcx, UiCommand::ReadTree),
        Ok(UiReply::Tree(_))
    ));
}

#[gpui_kit::test]
fn a_tool_that_gave_up_gets_nothing_injected(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);
    let tree = read_tree(&app, vcx);
    let search = tree
        .nodes
        .iter()
        .find(|n| n.role.ends_with("Input") && n.bounds.is_some())
        .map(|n| n.id.clone())
        .unwrap_or_else(|| panic!("a text input"));
    let _ = drive(&app, vcx, click(Target::Node(search.clone())));

    // The ticket times out before the UI runs the command: the UI sees the
    // cancellation and types nothing.
    let ticket = bridge::post(
        "Test agent",
        UiCommand::TypeText {
            text: "late".into(),
        },
    )
    .unwrap_or_else(|_| panic!("posted"));
    assert_eq!(
        ticket.wait(Duration::from_millis(1)),
        Err(BridgeError::Timeout)
    );
    // Withdrawn from the queue: nothing to start.
    pump(&app, vcx);
    // Taken, then cancelled: posted, taken by the tick, then given up on
    // before the frame it waits for.
    let ticket = bridge::post(
        "Test agent",
        UiCommand::TypeText {
            text: "late".into(),
        },
    )
    .unwrap_or_else(|_| panic!("posted"));
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.agent_driver_tick(window, cx);
        })
    });
    assert_eq!(
        ticket.wait(Duration::from_millis(1)),
        Err(BridgeError::Timeout)
    );
    settle(vcx);
    settle(vcx);
    let value = read_tree(&app, vcx)
        .node(&search)
        .and_then(|n| n.value.clone());
    assert_eq!(value, None, "nothing was typed for a tool that gave up");
}

/// The cost of building the tree every frame, printed for the decision
/// doc: `cargo test --features demo-capture --bin quill -- --ignored
/// --nocapture forced_tree_frame_cost`.
/// The tree stays forced for a while after a command, so the user may
/// open any screen under it: GPUI's debug-only tree checks must hold on
/// each (duplicate ids, orphaned nodes), whether or not the driver may
/// read it.
#[gpui_kit::test]
fn the_forced_tree_builds_on_every_screen(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    vcx.update(|window, _| window.set_a11y_forced(true));
    fn tree_on(vcx: &mut VisualTestContext, what: &str) {
        settle(vcx);
        let tree = vcx.update(|window, _| crate::ui::agent_driver_tree::raw_tree(window));
        let tree = tree.unwrap_or_else(|| panic!("a tree on {what}"));
        assert!(tree.nodes.len() > 1, "{what}: {} nodes", tree.nodes.len());
    }
    tree_on(vcx, "the chat list");

    vcx.update(|_, cx| app.update(cx, |app, _| app.open_settings(None)));
    tree_on(vcx, "Settings");
    vcx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.settings.page = Some(quill::settings_catalog::Page::AgentAccess);
        })
    });
    tree_on(vcx, "Settings > Agent access");
    vcx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.agent_ui.consent = Some(crate::ui::agent_state::AllChatsConsent {
                account: crate::ui::agent_state::active_account(),
                scope: Scope::Read,
            });
        })
    });
    tree_on(vcx, "the All chats consent");
    vcx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.agent_ui.consent = None;
            app.close_settings();
        })
    });

    let (_id, _rx) = prompts::file(
        AccountKey::primary(),
        "Test agent".into(),
        Ask::Chat {
            chat_id: 12,
            title: "Demo chat B".into(),
            secret: false,
            scope: Scope::Read,
        },
        String::new(),
    )
    .unwrap_or_else(|| panic!("filed"));
    tree_on(vcx, "an agent request");
    prompts::deny_all();
    tree_on(vcx, "the chat list again");
    vcx.update(|window, _| window.set_a11y_forced(false));
}

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
        let nodes = crate::ui::agent_driver_tree::raw_tree(window)
            .map(|tree| tree.nodes.len())
            .unwrap_or(0);
        (nodes, start.elapsed())
    });
    println!(
        "forced tree: frame {off:?} -> {on:?} (off again {off2:?}); {nodes} nodes; snapshot read {read:?}"
    );
}

/// A link an agent clicked resolves after its step ended (TDLib answers
/// later): the open runs as part of that step, so an ungranted chat stays
/// closed with a refusal, a granted one opens, and the user's own links
/// are not gated.
#[gpui_kit::test]
fn a_link_resolved_after_the_step_opens_a_granted_chat_only(cx: &mut TestAppContext) {
    use quill::agent::ui_driver::guard;
    use quill::ids::ChatId;
    use quill::state::DeepLinkAction;

    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);
    let _ = guard::take_refusals();
    let start = |param: &str| DeepLinkAction::OpenUsername {
        domain: "demo".into(),
        start_param: Some(param.into()),
        post: None,
        story_id: None,
    };
    let resolved = |app: &Entity<QuillApp>,
                    vcx: &mut VisualTestContext,
                    chat: i64,
                    param: &str,
                    from_agent: bool| {
        app.update_in(vcx, |app, _, cx| {
            app.links.pending_deep_link_open_from_agent = from_agent;
            app.links.pending_deep_link_open = Some((ChatId(chat), start(param)));
            cx.notify();
        });
        settle(vcx);
        vcx.update(|_, cx| app.read(cx).composer.read(cx).value().to_string())
    };

    // The agent's link led to chat 12, which is not granted: nothing opens,
    // the start parameter is not put in the box. The step answered long
    // ago, so the refusal goes to the activity log, not to the next reply.
    assert_eq!(resolved(&app, vcx, 12, "secret", true), "");
    assert!(
        guard::take_refusals().is_empty(),
        "nothing for the next reply"
    );
    let logged = activity::recent(&AccountKey::primary())
        .into_iter()
        .find(|action| action.summary.contains("not granted"))
        .unwrap_or_else(|| panic!("the refusal is in the activity log"));
    assert!(logged.declined && !logged.ok);
    assert!(logged.tool.starts_with("ui_"), "{}", logged.tool);
    // Chat 11 is granted: the open runs (here, the `/start` prefill).
    assert_eq!(resolved(&app, vcx, 11, "hello", true), "/start hello");
    assert!(guard::take_refusals().is_empty());
    // The user's own link to chat 12 is not an agent's.
    assert_eq!(resolved(&app, vcx, 12, "theirs", false), "/start theirs");
    assert!(guard::take_refusals().is_empty());
    app.update_in(vcx, |app, window, cx| {
        app.composer
            .update(cx, |input, cx| input.set_value("", window, cx));
    });
}

#[gpui_kit::test]
fn a_hidden_window_says_so_in_its_state_and_its_missing_frame(cx: &mut TestAppContext) {
    use gpui_kit::WindowVisibility;
    use quill::agent::ui_driver::bridge::WindowState;
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);
    let state = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| match drive(
        app,
        vcx,
        UiCommand::State,
    ) {
        Ok(UiReply::State(state)) => state.window,
        other => panic!("expected a state, got {other:?}"),
    };
    assert_eq!(state(&app, vcx), WindowState::Visible);

    // Behind other windows: the platform stops asking for frames.
    vcx.simulate_visibility_change(WindowVisibility::Hidden);
    assert_eq!(state(&app, vcx), WindowState::Hidden);

    // A read or a click would wait for a frame that never comes: each
    // says at once that the window is hidden, and nothing is injected.
    let reply = drive(&app, vcx, UiCommand::ReadTree);
    assert!(refused(&reply, "is hidden"), "{reply:?}");
    let reply = drive(&app, vcx, click(Target::Point { x: 600.0, y: 400.0 }));
    assert!(refused(&reply, "is hidden"), "{reply:?}");
    assert!(activity::recent(&AccountKey::primary()).is_empty());

    // Shown again, the driver reads the window.
    vcx.simulate_visibility_change(WindowVisibility::Visible);
    assert_eq!(state(&app, vcx), WindowState::Visible);
    read_tree(&app, vcx);
}
