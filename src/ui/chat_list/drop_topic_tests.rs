#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! A file or message drop on a forum row asks for a topic first (tdesktop
//! `ShowDropMediaBox`): the chooser lists the forum's topics, a pick opens
//! the chat in that topic with the files attached or the messages waiting
//! in the forward bar, and Cancel changes nothing.

use super::DropPayload;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, ExternalPaths, FileDropEvent, Modifiers, MouseButton, PlatformInput, Point,
    TestAppContext, VisualTestContext, point, px, size,
};
use quill::drop_modes::DragState;
use quill::ids::{ChatId, MessageId};
use quill::suggested_posts::DirectMessagesRole;
use std::path::PathBuf;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open(cx: &mut TestAppContext, demo: &str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    crate::ui::interface_zoom::set_initial_zoom(1.);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(demo);
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn row_centre(vcx: &mut VisualTestContext, chat: u64) -> Point<gpui_kit::Pixels> {
    vcx.update(|window, _| window.find(("chat-row", chat)).bounds().center())
}

fn dropped_file(name: &str) -> PathBuf {
    let dir = quill::scratch::process_dir("quill-drop-topic");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(name);
    let _ = std::fs::write(&path, b"hello");
    path
}

fn file_drop(vcx: &mut VisualTestContext, event: FileDropEvent) {
    vcx.update(|window, cx| {
        window.dispatch_event(PlatformInput::FileDrop(event), cx);
    });
    frames(vcx);
}

/// Drops one file on the row of `chat`.
fn drop_file_on(vcx: &mut VisualTestContext, chat: u64) {
    let at = row_centre(vcx, chat);
    file_drop(
        vcx,
        FileDropEvent::Entered {
            position: at,
            paths: ExternalPaths([dropped_file("note.txt")].into_iter().collect()),
        },
    );
    file_drop(vcx, FileDropEvent::Pending { position: at });
    file_drop(vcx, FileDropEvent::Submit { position: at });
}

fn open_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<ChatId> {
    app.update(vcx, |app, _| app.session().and_then(|s| s.open_chat))
}

fn open_topic(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<i32> {
    app.update(vcx, |app, _| app.session().and_then(|s| s.open_topic))
}

fn attached(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> usize {
    app.update(vcx, |app, _| app.pending_attachments().len())
}

fn chooser_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.chat_list.drop_topic.is_some())
}

fn listed(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<String> {
    app.read_with(vcx, |app, cx| {
        app.drop_topic_choices(cx)
            .into_iter()
            .map(|choice| choice.name)
            .collect()
    })
}

fn pick(vcx: &mut VisualTestContext, topic: i32) {
    vcx.update(|window, cx| window.click(format!("drop-topic-{topic}"), cx));
    frames(vcx);
}

#[gpui_kit::test]
fn a_file_drop_on_a_forum_row_lists_its_topics_and_opens_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-forum-row-topics");
    let vcx = &mut vcx;
    let chat = open_chat(&app, vcx);
    let before = attached(&app, vcx);
    drop_file_on(vcx, 71);
    assert!(chooser_open(&app, vcx));
    assert_eq!(
        listed(&app, vcx),
        vec![
            "Off topic",
            "Release planning",
            "Backend",
            "Design",
            "Announcements"
        ]
    );
    assert_eq!(open_chat(&app, vcx), chat);
    assert_eq!(attached(&app, vcx), before);
    assert!(vcx.update(|window, _| window.try_find("drop-topic-4").is_some()));
}

#[gpui_kit::test]
fn choosing_a_topic_opens_the_forum_in_it_with_the_file_attached(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-forum-row-topics");
    let vcx = &mut vcx;
    let before = attached(&app, vcx);
    drop_file_on(vcx, 71);
    pick(vcx, 4);
    assert!(!chooser_open(&app, vcx));
    assert_eq!(open_chat(&app, vcx), Some(ChatId(71)));
    assert_eq!(open_topic(&app, vcx), Some(4));
    assert_eq!(attached(&app, vcx), before + 1);
}

#[gpui_kit::test]
fn cancelling_the_chooser_changes_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-forum-row-topics");
    let vcx = &mut vcx;
    let chat = open_chat(&app, vcx);
    let before = attached(&app, vcx);
    drop_file_on(vcx, 71);
    vcx.update(|window, cx| window.click("drop-topic-cancel", cx));
    frames(vcx);
    assert!(!chooser_open(&app, vcx));
    assert_eq!(open_chat(&app, vcx), chat);
    assert_eq!(open_topic(&app, vcx), None);
    assert_eq!(attached(&app, vcx), before);
}

#[gpui_kit::test]
fn typing_narrows_the_list_by_word_prefixes(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-forum-row-topics");
    let vcx = &mut vcx;
    drop_file_on(vcx, 71);
    let search = app.read_with(vcx, |app, _| {
        app.chat_list
            .drop_topic
            .as_ref()
            .map(|waiting| waiting.search.clone())
    });
    let search = search.expect("the chooser is open");
    vcx.update(|window, cx| {
        search.update(cx, |input, cx| input.set_value("plan rel", window, cx));
    });
    frames(vcx);
    assert_eq!(listed(&app, vcx), vec!["Release planning"]);
    vcx.update(|window, cx| {
        search.update(cx, |input, cx| input.set_value("zzz", window, cx));
    });
    frames(vcx);
    assert!(listed(&app, vcx).is_empty());
}

#[gpui_kit::test]
fn a_forum_whose_topics_have_not_arrived_shows_none_to_pick(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-forum-row-topics");
    let vcx = &mut vcx;
    drop_file_on(vcx, 73);
    assert!(chooser_open(&app, vcx));
    assert!(listed(&app, vcx).is_empty());
}

#[gpui_kit::test]
fn a_chat_that_is_not_a_forum_still_takes_the_drop_straight_away(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-forum-row-topics");
    let vcx = &mut vcx;
    let before = attached(&app, vcx);
    let plain = 12_i64;
    drop_file_on(vcx, plain as u64);
    assert!(!chooser_open(&app, vcx));
    assert_eq!(open_chat(&app, vcx), Some(ChatId(plain)));
    assert_eq!(attached(&app, vcx), before + 1);
}

/// Chat 13 of the selection demo turned into a forum with two topics.
fn forum_of_13(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, _| {
        let Some(session) = app.demo_session.as_mut() else {
            return;
        };
        if let Some(summary) = session.chats.get_mut(&13) {
            summary.kind = quill::telegram::envelope::ChatKind::Supergroup {
                supergroup_id: 4242,
                is_channel: false,
            };
            summary.is_forum = Some(true);
        }
        let topic = |id: i32, name: &str, order: i64| quill::telegram::envelope::ForumTopic {
            forum_topic_id: id,
            name: name.to_string(),
            is_general: id == 1,
            is_closed: false,
            is_pinned: false,
            is_hidden: false,
            unread_count: 0,
            order,
            last_message_preview: String::new(),
            icon_color: 0,
            icon_custom_emoji_id: 0,
            last_message_id: 0,
            last_read_inbox_message_id: 0,
            notification_settings: Default::default(),
            unread_mention_count: 0,
            unread_reaction_count: 0,
            creation_date: 0,
            creator: None,
            is_outgoing: false,
            draft: None,
        };
        session
            .threads
            .forum_topics
            .insert(13, vec![topic(1, "General", 1), topic(2, "Plans", 2)]);
    });
    frames(vcx);
}

fn message_centre(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    id: MessageId,
) -> Point<gpui_kit::Pixels> {
    // A taller layout (other fonts) can leave the row below the fold.
    app.update(vcx, |app, cx| app.jump_to_latest_messages(cx));
    frames(vcx);
    let at = app.read_with(vcx, |app, _| {
        let hits = app.history.hit_rows.borrow();
        hits.iter()
            .find_map(|(ix, bounds)| match app.history.rows.get(*ix) {
                Some(crate::ui::HistoryRow::Single(inputs)) if inputs.message.id == id => {
                    // Off the bubble, low in the row, clear of the hover toolbar
                    // and where the selection overlay takes the press.
                    let y = bounds.bottom() - px(35.);
                    Some(if inputs.message.is_outgoing {
                        point(bounds.left() + px(6.), y)
                    } else {
                        point(bounds.right() - px(6.), y)
                    })
                }
                _ => None,
            })
    });
    assert!(at.is_some(), "message {id:?} is painted");
    let at = at.unwrap_or_default();
    // The composer takes the bottom of the window.
    let window = vcx.update(|window, _| window.viewport_size());
    assert!(
        at.y > px(60.) && at.y < window.height - px(120.),
        "the press point {at:?} is off the history of a {window:?} window"
    );
    at
}

/// Lifts the selected messages of the selection demo and drops them on the
/// row of chat 13.
fn drop_selection_on_13(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let from = message_centre(app, vcx, MessageId(101));
    let none = Modifiers::default();
    vcx.simulate_mouse_move(from, None, none);
    vcx.simulate_mouse_down(from, MouseButton::Left, none);
    let aim = row_centre(vcx, 13);
    for step in 1..=4 {
        let t = step as f32 / 4.;
        let at = point(from.x + (aim.x - from.x) * t, from.y + (aim.y - from.y) * t);
        vcx.simulate_mouse_move(at, Some(MouseButton::Left), none);
        frames(vcx);
    }
    // The list may have reflowed while the drag started.
    let to = row_centre(vcx, 13);
    vcx.simulate_mouse_move(to, Some(MouseButton::Left), none);
    frames(vcx);
    vcx.simulate_mouse_up(to, MouseButton::Left, none);
    frames(vcx);
}

#[gpui_kit::test]
fn a_forward_drop_on_a_forum_row_asks_for_a_topic_then_waits_in_its_bar(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-selection");
    let vcx = &mut vcx;
    forum_of_13(&app, vcx);
    drop_selection_on_13(&app, vcx);
    assert!(chooser_open(&app, vcx));
    assert_eq!(open_chat(&app, vcx), Some(ChatId(11)));
    assert_eq!(
        app.read_with(vcx, |app, _| app.share.forward_bar_dest),
        None
    );
    pick(vcx, 2);
    assert_eq!(open_chat(&app, vcx), Some(ChatId(13)));
    assert_eq!(open_topic(&app, vcx), Some(2));
    app.read_with(vcx, |app, _| {
        assert_eq!(app.share.forward_bar_dest, Some(ChatId(13)));
        let ids = app
            .share
            .pending_forward
            .as_ref()
            .map(|d| d.message_ids.clone());
        assert_eq!(ids, Some(vec![MessageId(101), MessageId(102)]));
        // The topic is bound when it is picked, not when Send runs.
        assert_eq!(
            app.share.pending_forward.as_ref().and_then(|d| d.topic_id),
            Some(2)
        );
    });
}

#[gpui_kit::test]
fn cancelling_a_forward_drop_leaves_the_forward_bar_alone(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-selection");
    let vcx = &mut vcx;
    forum_of_13(&app, vcx);
    drop_selection_on_13(&app, vcx);
    vcx.update(|window, cx| window.click("drop-topic-cancel", cx));
    frames(vcx);
    assert!(!chooser_open(&app, vcx));
    assert_eq!(open_chat(&app, vcx), Some(ChatId(11)));
    assert_eq!(
        app.read_with(vcx, |app, _| app.share.forward_bar_dest),
        None
    );
}

fn direct_messages_of_13(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    role: DirectMessagesRole,
) {
    app.update(vcx, |app, _| {
        let Some(session) = app.demo_session.as_mut() else {
            return;
        };
        if let Some(summary) = session.chats.get_mut(&13) {
            summary.kind = quill::telegram::envelope::ChatKind::Supergroup {
                supergroup_id: 4242,
                is_channel: false,
            };
        }
        session.groups.supergroup_direct_messages.insert(4242, role);
    });
    frames(vcx);
}

#[gpui_kit::test]
fn an_administrators_direct_messages_keep_the_note_instead_of_a_chooser(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-selection");
    let vcx = &mut vcx;
    direct_messages_of_13(&app, vcx, DirectMessagesRole::Admin);
    drop_selection_on_13(&app, vcx);
    assert!(!chooser_open(&app, vcx));
    assert_eq!(
        app.read_with(vcx, |app, _| app.share.forward_bar_dest),
        None
    );
}

#[gpui_kit::test]
fn a_members_direct_messages_take_the_drop_like_any_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-selection");
    let vcx = &mut vcx;
    direct_messages_of_13(&app, vcx, DirectMessagesRole::Member);
    drop_selection_on_13(&app, vcx);
    assert!(!chooser_open(&app, vcx));
    assert_eq!(
        app.read_with(vcx, |app, _| app.share.forward_bar_dest),
        Some(ChatId(13))
    );
}

#[gpui_kit::test]
fn the_payload_remembers_what_a_file_drop_brought(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-forum-row-topics");
    let vcx = &mut vcx;
    drop_file_on(vcx, 71);
    app.read_with(vcx, |app, _| {
        let waiting = app.chat_list.drop_topic.as_ref().expect("open");
        match &waiting.payload {
            DropPayload::Files { paths, kind } => {
                assert_eq!(paths.len(), 1);
                assert_eq!(*kind, DragState::Files);
            }
            DropPayload::Forward(_) => panic!("a file drop"),
        }
    });
}

fn choose(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, topic: Option<i32>) {
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.choose_drop_topic(topic, window, cx));
    });
    frames(vcx);
}

#[gpui_kit::test]
fn a_topic_that_vanished_keeps_the_chooser_open_and_the_chat_closed(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-forum-row-topics");
    let vcx = &mut vcx;
    let chat = open_chat(&app, vcx);
    drop_file_on(vcx, 71);
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session
                .threads
                .forum_topics
                .update(71, |topics| topics.retain(|t| t.forum_topic_id != 4));
        }
    });
    choose(&app, vcx, Some(4));
    assert!(chooser_open(&app, vcx));
    assert_eq!(open_chat(&app, vcx), chat);
    assert_eq!(attached(&app, vcx), 0);
}

#[gpui_kit::test]
fn all_messages_leaves_the_topic_the_chat_was_open_in(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-forum-row-topics");
    let vcx = &mut vcx;
    drop_file_on(vcx, 71);
    pick(vcx, 4);
    assert_eq!(open_topic(&app, vcx), Some(4));
    drop_file_on(vcx, 71);
    choose(&app, vcx, None);
    assert_eq!(open_chat(&app, vcx), Some(ChatId(71)));
    assert_eq!(open_topic(&app, vcx), None);
}

#[gpui_kit::test]
fn a_chooser_closes_when_its_forum_is_no_longer_one(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-forum-row-topics");
    let vcx = &mut vcx;
    drop_file_on(vcx, 71);
    assert!(chooser_open(&app, vcx));
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.chats.remove(&71);
        }
    });
    // The poll that applies TDLib updates drops the stale chooser.
    app.update(vcx, |app, _| app.prune_drop_topic());
    frames(vcx);
    assert!(!chooser_open(&app, vcx));
}

#[gpui_kit::test]
fn a_topics_request_that_fails_leaves_loading_for_the_error_line(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-forum-row-topics");
    let vcx = &mut vcx;
    drop_file_on(vcx, 73);
    assert!(!app.read_with(vcx, |app, _| app.drop_topic_failed()));
    // TDLib answers the topics request of the forum with an error.
    app.update(vcx, |app, _| {
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> = app.demo_ui.sink.clone();
        if let Some(session) = app.demo_session.as_mut() {
            let extra = session.request(
                quill::state::RequestPurpose::GetForumTopics,
                Some(ChatId(73)),
            );
            let json = format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHANNEL_INVALID"}}"#,
                extra.0
            );
            if let Some(parsed) =
                quill::telegram::client::copy_and_parse(&json, &app.demo_ui.seq, &sink)
            {
                session.apply(parsed);
            }
        }
    });
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.drop_topic_failed()));
    assert!(listed(&app, vcx).is_empty());
}

#[gpui_kit::test]
fn choosing_on_a_forum_that_went_away_drops_the_chooser_and_does_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-forum-row-topics");
    let vcx = &mut vcx;
    let chat = open_chat(&app, vcx);
    drop_file_on(vcx, 71);
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.chats.remove(&71);
        }
    });
    choose(&app, vcx, Some(4));
    assert!(!chooser_open(&app, vcx));
    assert_eq!(open_chat(&app, vcx), chat);
    assert_eq!(attached(&app, vcx), 0);
}
