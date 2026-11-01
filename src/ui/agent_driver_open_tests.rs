//! What an agent's click may reach in a granted chat: a link in a message
//! is refused at the door to the OS (nothing is launched), while the
//! emoji, sticker and GIF panels over the composer are the chat's own
//! controls and take clicks.
use super::tests::{click, drive, grant, new_app, node_named, read_tree, settle, turn_on};
use crate::ui::agent_state::TEST_LOCK;
use crate::ui::app::QuillApp;
use crate::ui::media_panel::PanelTab;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    ElementId, Entity, Modifiers, Pixels, Point, TestAppContext, VisualTestContext, point, px,
};
use quill::agent::activity;
use quill::agent::grants::Scope;
use quill::agent::ui_driver::bridge::{Target, UiReply};
use quill::agent::ui_driver::tree::UiTree;
use quill::platform::os_launch_attempts;
use quill::telegram::envelope::{MessageContent, TextContent};
use quill::text::{TextEntity, TextEntityKind};

const LINK_TEXT: &str = "https://example.com/agent-click-through-the-window";

/// Turn the newest text message of chat 11 into a link (a URL entity over
/// the whole text) and return its id.
fn put_link_in_chat_11(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> u64 {
    let id = app.update_in(vcx, |app, _, cx| {
        let Some(history) = app
            .demo_session
            .as_mut()
            .and_then(|session| session.histories.get_mut(&11))
        else {
            panic!("chat 11's history");
        };
        let id = history
            .messages
            .values()
            .filter(|message| matches!(message.content, MessageContent::Text(_)))
            .map(|message| message.id.0)
            .max()
            .unwrap_or_else(|| panic!("a text message in chat 11"));
        let Some(message) = history.messages.get_mut(&id) else {
            panic!("message {id}");
        };
        message.content = MessageContent::Text(TextContent {
            text: LINK_TEXT.to_string(),
            entities: vec![TextEntity {
                utf8_start: 0,
                utf8_end: LINK_TEXT.len(),
                kind: TextEntityKind::Url,
            }],
            link_preview: None,
            animated: None,
        });
        cx.notify();
        id
    });
    settle(vcx);
    u64::try_from(id).unwrap_or_else(|_| panic!("message id {id}"))
}

/// Where the link in message `id` is: the user clicks along the lines
/// above its hover actions (the bubble's foot) and left of them, nearest
/// line first, until one click queues the link (taken back before it runs)
/// or opens it (in test builds only counted).
fn find_link(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: u64) -> Point<Pixels> {
    let row = app.read_with(vcx, |app, _| {
        app.history
            .painted_row_center(quill::ids::MessageId(id as i64))
    });
    vcx.simulate_mouse_move(row, None, Modifiers::none());
    settle(vcx);
    let (actions, chats) = vcx.update(|window, _| {
        (
            window.find(format!("message-reply-{id}")).bounds(),
            window
                .find(ElementId::NamedInteger("chat-row".into(), 11))
                .bounds(),
        )
    });
    let launched = os_launch_attempts();
    let left = f32::from(chats.origin.x + chats.size.width) + 8.;
    let right = f32::from(actions.origin.x) - 4.;
    let foot = f32::from(actions.origin.y + actions.size.height);
    for up in (0..160).step_by(6) {
        let mut x = left;
        while x < right {
            let at = point(px(x), px(foot - up as f32));
            vcx.simulate_click(at, Modifiers::none());
            let queued = app.update(vcx, |app, _| app.message_ui.pending_link.take().is_some());
            if queued || os_launch_attempts() != launched {
                return at;
            }
            x += 12.;
        }
    }
    panic!("no link in message {id} left of {actions:?}");
}

#[gpui_kit::test]
fn a_link_an_agent_clicks_in_a_granted_chat_launches_nothing(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);
    let id = put_link_in_chat_11(&app, vcx);
    let link = find_link(&app, vcx, id);
    settle(vcx);
    let _ = read_tree(&app, vcx);

    let before = os_launch_attempts();
    let reply = drive(
        &app,
        vcx,
        click(Target::Point {
            x: link.x.into(),
            y: link.y.into(),
        }),
    );
    let Ok(UiReply::Done { refusals, .. }) = reply else {
        panic!("expected Done, got {reply:?}");
    };
    assert!(
        refusals
            .iter()
            .any(|r| r.contains("open a link in the browser")),
        "{refusals:?}"
    );
    assert_eq!(os_launch_attempts(), before, "nothing reached the OS");

    // The same link clicked by the user opens (counted, not launched, in
    // test builds): the door is shut only for the agent.
    vcx.simulate_click(link, Modifiers::none());
    settle(vcx);
    assert_eq!(os_launch_attempts(), before + 1);
}

fn has_named(tree: &UiTree, name: &str) -> bool {
    node_named(tree, name).is_some()
}

#[gpui_kit::test]
fn the_emoji_sticker_and_gif_panels_take_clicks_in_a_granted_chat(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);

    for (tab, next) in [
        (PanelTab::Emoji, "Stickers"),
        (PanelTab::Stickers, "GIFs"),
        (PanelTab::Gifs, "Emoji"),
    ] {
        app.update_in(vcx, |app, _, cx| app.open_media_panel(tab, cx));
        settle(vcx);
        let tree = read_tree(&app, vcx);
        let summary: Vec<String> = tree
            .nodes
            .iter()
            .map(|n| format!("{} {:?} {:?}", n.role, n.name, n.tag))
            .collect();
        assert!(
            has_named(&tree, next),
            "{tab:?}: the {next} tab in {summary:#?}"
        );
        let target = node_named(&tree, next)
            .map(|node| node.id.clone())
            .unwrap_or_else(|| panic!("{next}"));
        let reply = drive(&app, vcx, click(Target::Node(target)));
        let Ok(UiReply::Done { refusals, .. }) = reply else {
            panic!("{tab:?}: expected Done, got {reply:?}");
        };
        assert!(refusals.is_empty(), "{tab:?}: {refusals:?}");
        let switched = vcx.update(|_, cx| app.read(cx).pickers.media_panel.tab);
        assert_ne!(switched, tab, "{tab:?}: the click switched the tab");
        app.update_in(vcx, |app, _, cx| {
            app.close_media_panel(cx);
        });
        settle(vcx);
    }
}
