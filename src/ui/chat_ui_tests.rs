//! What a view has open for its chat goes when it shows another chat, as
//! one value (`ChatUi`), however the chat changed.

use crate::ui::app::QuillApp;
use crate::ui::entity_links::LinkPopup;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, point, px, size};
use quill::composer::ComposerReplyTo;
use quill::ids::{ChatId, MessageId};
use quill::text::LinkTarget;
use std::rc::Rc;

const CHAT: ChatId = ChatId(11);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// Another chat of the demo list than `CHAT`.
fn other_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> ChatId {
    app.read_with(vcx, |app, _| {
        let session = app.session().expect("a demo session");
        session
            .chats
            .keys()
            .copied()
            .find(|id| *id != CHAT.0)
            .map(ChatId)
            .expect("a second chat")
    })
}

fn open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, chat: ChatId) {
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(chat, window, cx)
    });
    frames(vcx);
}

/// Open the three surfaces the old hand list missed, and a reply.
fn open_chat_surfaces(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| {
        app.chat_ui.rich_editor_open = true;
        app.chat_ui.send_as_open = true;
        app.chat_ui.link_popup = Some(LinkPopup {
            position: point(px(10.), px(10.)),
            link: LinkTarget::Mention("@x".into()),
        });
        app.chat_ui
            .set_reply(Some(ComposerReplyTo::new(CHAT, MessageId(1))));
        cx.notify();
    });
}

struct Shown {
    rich_editor: bool,
    send_as: bool,
    link_popup: bool,
    reply: bool,
}

fn shown(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Shown {
    app.read_with(vcx, |app, _| Shown {
        rich_editor: app.chat_ui.rich_editor_open,
        send_as: app.chat_ui.send_as_open,
        link_popup: app.chat_ui.link_popup.is_some(),
        reply: app.chat_ui.has_reply(),
    })
}

#[gpui_kit::test]
fn another_chat_drops_what_was_open_for_the_last(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let other = other_chat(&app, &mut vcx);
    open(&app, &mut vcx, CHAT);
    open_chat_surfaces(&app, &mut vcx);
    let before = shown(&app, &mut vcx);
    assert!(before.rich_editor && before.send_as && before.link_popup && before.reply);

    open(&app, &mut vcx, other);
    let after = shown(&app, &mut vcx);
    assert!(!after.rich_editor, "the rich editor is the chat's");
    assert!(!after.send_as, "the send-as list is the chat's");
    assert!(!after.link_popup, "the link's copy menu is the chat's");
    assert!(!after.reply, "the reply is the chat's");
    let made_for = app.read_with(&vcx, |app, _| app.chat_ui.chat());
    assert_eq!(made_for, Some(other));
}

/// A round video clicked before its file was local is parked to play;
/// leaving the chat drops it, so it does not start with sound on return.
#[gpui_kit::test]
fn another_chat_drops_a_round_video_waiting_to_play(cx: &mut TestAppContext) {
    use crate::ui::play_intent::{PlayIntent, PlayTarget};
    let (app, mut vcx) = new_app(cx);
    let other = other_chat(&app, &mut vcx);
    let park = |app: &gpui_kit::Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.update(vcx, |app, _| {
            app.playback.intent = Some(PlayIntent {
                file: quill::ids::FileId(9001),
                target: PlayTarget::Inline {
                    chat: CHAT,
                    message: MessageId(5),
                    demand: crate::ui::inline_players::Demand::Sound,
                    window: None,
                    visit: app.chat_ui.visit(),
                },
            });
        });
    };
    open(&app, &mut vcx, CHAT);
    park(&app, &mut vcx);
    open(&app, &mut vcx, other);
    assert!(
        app.read_with(&vcx, |app, _| app.playback.intent.is_none()),
        "the click was for the chat left"
    );
    // Esc and the same chat again: the click was for the visit that ended.
    open(&app, &mut vcx, CHAT);
    park(&app, &mut vcx);
    app.update_in(&mut vcx, |app, window, cx| app.leave_open_chat(window, cx));
    open(&app, &mut vcx, CHAT);
    assert!(
        app.read_with(&vcx, |app, _| app.playback.intent.is_none()),
        "closing the chat ended the visit"
    );
}

#[gpui_kit::test]
fn choosing_the_open_chat_again_keeps_its_bars(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    open(&app, &mut vcx, CHAT);
    open_chat_surfaces(&app, &mut vcx);
    open(&app, &mut vcx, CHAT);
    let after = shown(&app, &mut vcx);
    assert!(after.rich_editor && after.link_popup && after.reply);
}

#[gpui_kit::test]
fn a_reply_aimed_at_the_next_chat_comes_along(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let other = other_chat(&app, &mut vcx);
    open(&app, &mut vcx, CHAT);
    app.update(&mut vcx, |app, _| {
        let reply = ComposerReplyTo::new(CHAT, MessageId(1)).into_chat(other);
        app.chat_ui.set_reply(Some(reply));
        app.chat_ui.rich_editor_open = true;
    });
    open(&app, &mut vcx, other);
    let after = shown(&app, &mut vcx);
    assert!(after.reply, "Reply in Another Chat carries the reply");
    assert!(!after.rich_editor);
}

/// A chat change that goes around the selection paths (another device, an
/// upgrade) still replaces the value at the next frame.
#[gpui_kit::test]
fn a_chat_changed_elsewhere_is_caught_at_the_next_frame(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let other = other_chat(&app, &mut vcx);
    open(&app, &mut vcx, CHAT);
    open_chat_surfaces(&app, &mut vcx);
    app.update(&mut vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.open_chat(other);
        }
        cx.notify();
    });
    frames(&mut vcx);
    let after = shown(&app, &mut vcx);
    assert!(!after.rich_editor && !after.send_as && !after.link_popup && !after.reply);
}

#[gpui_kit::test]
fn leaving_the_chat_drops_its_state(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    open(&app, &mut vcx, CHAT);
    open_chat_surfaces(&app, &mut vcx);
    app.update_in(&mut vcx, |app, window, cx| app.leave_open_chat(window, cx));
    let after = shown(&app, &mut vcx);
    assert!(!after.rich_editor && !after.send_as && !after.link_popup && !after.reply);
    let made_for = app.read_with(&vcx, |app, _| app.chat_ui.chat());
    assert_eq!(made_for, None);
}

/// A forum's topics share the chat id: an edit opened in one topic does
/// not stay in the next topic's composer, and the value is made for the
/// topic shown.
#[gpui_kit::test]
fn another_forum_topic_drops_the_edit(cx: &mut TestAppContext) {
    use crate::ui::demos::composer::{FORUM_CHAT, RELEASES_TOPIC};
    use quill::composer::{ComposerEdit, ComposerEditKind};
    cx.update(gpui_kit::init);
    let slot = Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-composer-bars")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    let select = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext, topic: i32| {
        app.update(vcx, |app, cx| {
            if let Some(session) = app.demo_session.as_mut() {
                session.select_topic(FORUM_CHAT, topic);
            }
            cx.notify();
        });
        frames(vcx);
    };
    open(&app, &mut vcx, FORUM_CHAT);
    select(&app, &mut vcx, 1);
    app.update(&mut vcx, |app, cx| {
        let edit = ComposerEdit {
            chat_id: FORUM_CHAT,
            message_id: MessageId(20),
            kind: ComposerEditKind::Text,
            scheduled: false,
            caption_above: false,
            media_edit: Default::default(),
            link_preview: Default::default(),
        };
        app.chat_ui.enter_edit(edit, String::new(), None);
        cx.notify();
    });
    app.update_in(&mut vcx, |app, window, cx| {
        app.set_composer_markup("the edited message", window, cx);
    });
    frames(&mut vcx);
    assert!(app.read_with(&vcx, |app, _| app.chat_ui.is_editing()));

    // A switch no menu made (no draft restore asked for), as a Saved
    // sublist or a thread inside the topic switches.
    select(&app, &mut vcx, RELEASES_TOPIC);
    app.read_with(&vcx, |app, cx| {
        assert!(!app.chat_ui.is_editing(), "the edit was the other topic's");
        assert_eq!(app.chat_ui.chat(), Some(FORUM_CHAT));
        // Round 17 (area B, finding 1): the edited text did not stay in
        // the field as a plain draft for Enter to send as a new message.
        assert_ne!(
            app.composer.read(cx).value().to_string(),
            "the edited message"
        );
    });
}

/// Round 7, 2a and 2b: a Reschedule picker names a message id of its
/// chat, and the scheduled list's buttons act on the chat shown. Both go
/// when another chat opens, as from a chat-list click or a notification.
#[gpui_kit::test]
fn another_chat_drops_the_reschedule_picker_and_the_scheduled_list(cx: &mut TestAppContext) {
    use crate::ui::scheduled::ScheduleTarget;
    let (app, mut vcx) = new_app(cx);
    let other = other_chat(&app, &mut vcx);
    open(&app, &mut vcx, CHAT);
    app.update_in(&mut vcx, |app, window, cx| {
        app.chat_ui.scheduled_dialog_open = true;
        app.open_schedule_picker(ScheduleTarget::Reschedule(MessageId(5)), window, cx);
    });
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(app.chat_ui.schedule_picker.is_some() && app.chat_ui.schedule_popup_open);
    });
    open(&app, &mut vcx, other);
    app.read_with(&vcx, |app, _| {
        assert!(
            app.chat_ui.schedule_picker.is_none(),
            "the picker was the last chat's"
        );
        assert!(!app.chat_ui.schedule_popup_open);
        assert!(
            !app.chat_ui.scheduled_dialog_open,
            "the list was the last chat's"
        );
    });
}

/// Round 7, 2g: the topic and thread info cards are the chat's.
#[gpui_kit::test]
fn another_chat_closes_the_info_cards(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let other = other_chat(&app, &mut vcx);
    open(&app, &mut vcx, CHAT);
    app.update(&mut vcx, |app, cx| {
        app.chat_ui.topic_info_open = true;
        app.chat_ui.thread_info_open = true;
        cx.notify();
    });
    open(&app, &mut vcx, other);
    app.read_with(&vcx, |app, _| {
        assert!(!app.chat_ui.topic_info_open && !app.chat_ui.thread_info_open);
    });
}

/// Round 8, 2a: a new poll or checklist goes to the chat shown at Create,
/// so its box goes when another chat opens (a notification click opens
/// one under the box).
#[gpui_kit::test]
fn another_chat_closes_the_poll_and_checklist_boxes(cx: &mut TestAppContext) {
    use crate::ui::ChecklistDialog;
    use crate::ui::dialogs::poll::{PollDialog, QuizLock};
    let (app, mut vcx) = new_app(cx);
    let other = other_chat(&app, &mut vcx);
    open(&app, &mut vcx, CHAT);
    app.update_in(&mut vcx, |app, window, cx| {
        app.chat_ui.poll_dialog = Some(PollDialog::new(window, cx, false, QuizLock::Free));
        app.chat_ui.checklist_dialog = Some(ChecklistDialog::new(None, window, cx));
    });
    frames(&mut vcx);
    open(&app, &mut vcx, other);
    app.read_with(&vcx, |app, _| {
        assert!(
            app.chat_ui.poll_dialog.is_none(),
            "the poll was the last chat's"
        );
        assert!(
            app.chat_ui.checklist_dialog.is_none(),
            "the checklist was the last chat's"
        );
    });
}

/// Round 16 (area B, finding 1): a selection is its conversation's. Made
/// in one forum topic, it is not shown, counted, deleted or forwarded in
/// the next topic of the same forum (the frame before the upkeep drops it
/// included), and it goes whole on the switch.
#[gpui_kit::test]
fn a_selection_made_in_one_topic_goes_with_it(cx: &mut TestAppContext) {
    use crate::ui::demos::composer::{FORUM_CHAT, RELEASES_TOPIC};
    use quill::ids::TopicId;
    cx.update(gpui_kit::init);
    let slot = Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-composer-bars")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    open(&app, &mut vcx, FORUM_CHAT);
    app.update(&mut vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.select_topic(FORUM_CHAT, 1);
        }
        cx.notify();
    });
    frames(&mut vcx);
    app.update(&mut vcx, |app, cx| {
        app.toggle_forward_select(FORUM_CHAT, MessageId(20), false, cx);
        app.toggle_forward_select(FORUM_CHAT, MessageId(21), false, cx);
    });
    app.read_with(&vcx, |app, _| {
        assert!(app.selecting_in(FORUM_CHAT));
        let draft = app.forward_draft().expect("a selection");
        assert!(draft.is_from(FORUM_CHAT, TopicId::forum(1)));
        assert_eq!(draft.count(), 2);
    });
    // The other topic, before any frame ran its upkeep.
    app.update(&mut vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.select_topic(FORUM_CHAT, RELEASES_TOPIC);
        }
        assert!(!app.selecting_in(FORUM_CHAT), "not in the other topic");
        assert!(
            app.forward_draft().is_none(),
            "nothing to delete or forward"
        );
    });
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(app.share.forward.draft().is_none(), "it went on the switch");
        assert!(!app.selecting_in(FORUM_CHAT));
    });
}

/// The forum demo, with the forum open in topic 1 and messages 20 and 21
/// selected there.
fn forum_with_selection(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    use crate::ui::demos::composer::FORUM_CHAT;
    cx.update(gpui_kit::init);
    let slot = Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-composer-bars")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    open(&app, &mut vcx, FORUM_CHAT);
    select_forum_topic(&app, &mut vcx, 1);
    app.update(&mut vcx, |app, cx| {
        app.toggle_forward_select(FORUM_CHAT, MessageId(20), false, cx);
        app.toggle_forward_select(FORUM_CHAT, MessageId(21), false, cx);
    });
    (app, vcx)
}

fn select_forum_topic(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, topic: i32) {
    use crate::ui::demos::composer::FORUM_CHAT;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.select_topic(FORUM_CHAT, topic);
        }
        cx.notify();
    });
    frames(vcx);
}

/// Round 17 (area B, finding 6): a selection forwarded into its own forum
/// becomes the forward bar there, which is no selection mode: no topic of
/// the forum shows "2 selected" or a Delete over ids it does not show.
#[gpui_kit::test]
fn a_forward_bar_into_the_selections_forum_is_no_selection_mode(cx: &mut TestAppContext) {
    use crate::ui::demos::composer::{FORUM_CHAT, RELEASES_TOPIC};
    use crate::ui::forward_stage::ForwardStage;
    let (app, mut vcx) = forum_with_selection(cx);
    app.update(&mut vcx, |app, cx| {
        let draft = app.share.forward.take_draft().expect("a selection");
        // What choosing the forum in the share box hands the window.
        app.share.forward = ForwardStage::Bar {
            draft,
            dest: FORUM_CHAT,
        };
        cx.notify();
    });
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(app.forward_bar_here(), "the bar waits in the forum");
        assert!(
            !app.selecting_in(FORUM_CHAT),
            "not in the topic picked from"
        );
    });
    select_forum_topic(&app, &mut vcx, RELEASES_TOPIC);
    app.read_with(&vcx, |app, _| {
        assert_eq!(app.share.forward.bar_shown(), Some(FORUM_CHAT));
        assert!(!app.selecting_in(FORUM_CHAT), "nor in another topic");
    });
}

/// Round 17 (area B, finding 7) and round 18 (area B, finding 5): a
/// forward's answer arrives after its send ended the stage, so a selection
/// it finds is a newer forward, from another topic or from the same one.
#[gpui_kit::test]
fn a_forward_answer_keeps_a_newer_selection(cx: &mut TestAppContext) {
    use crate::ui::demos::composer::{FORUM_CHAT, RELEASES_TOPIC};
    use quill::ids::TopicId;
    let (app, mut vcx) = forum_with_selection(cx);
    let answer = |topic: TopicId| quill::state::ForwardResult {
        dest_chat_id: ChatId(12),
        dest_title: "B".into(),
        from_chat_id: FORUM_CHAT,
        from_topic: topic,
        requested: 1,
        forwarded_ids: vec![MessageId(1)],
    };
    app.update(&mut vcx, |app, cx| {
        app.present_forward_result(answer(TopicId::forum(RELEASES_TOPIC)), cx);
    });
    app.read_with(&vcx, |app, _| {
        assert!(
            app.selecting_in(FORUM_CHAT),
            "the selection of topic 1 stays"
        );
        assert_eq!(app.forward_draft().map(|draft| draft.count()), Some(2));
    });
    app.update(&mut vcx, |app, cx| {
        app.present_forward_result(answer(TopicId::forum(1)), cx);
    });
    app.read_with(&vcx, |app, _| {
        assert!(
            app.selecting_in(FORUM_CHAT),
            "a late answer from the same topic does not end the newer selection"
        );
        assert!(app.share.forward_result.is_some(), "the answer still shows");
    });
}

/// TDLib answered with no identity to send as: the menu has nothing to
/// pick and does not open (a tap, or an answer that lands while it waits).
#[gpui_kit::test]
fn an_answered_empty_send_as_list_opens_no_menu(cx: &mut TestAppContext) {
    use quill::state::fetch::Fetch;
    let (app, mut vcx) = new_app(cx);
    open(&app, &mut vcx, CHAT);
    app.update(&mut vcx, |app, cx| {
        app.toggle_send_as(cx);
        assert!(
            app.chat_ui.send_as_open,
            "not asked yet: it opens and waits"
        );
        app.toggle_send_as(cx);
        if let Some(session) = app.demo_session.as_mut() {
            session
                .chats_state
                .send_as_options
                .insert(CHAT.0, Fetch::Ready(Vec::new()));
        }
        assert!(app.send_as_list_empty());
        app.toggle_send_as(cx);
        assert!(!app.chat_ui.send_as_open, "an empty list is not a menu");
    });
}
