//! The next send's attachments and options belong to the chat they were
//! chosen in: every way of opening another chat shows that chat's own
//! (docs/decisions/sweep-per-chat-composer.md). On a demo `QuillApp`
//! (gpui-kit's test support needs the `demo-capture` feature).
#![cfg(feature = "demo-capture")]

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Context, Entity, Focusable, TestAppContext, VisualContext,
    VisualTestContext, Window, px, size,
};
use quill::composer::{AttachmentKind, ComposerAttachment, ComposerScheduling};
use quill::ids::{ChatId, MessageId};
use quill::telegram::requests::SelfDestructSend;

/// The demo's open chat (a private chat) and two more from its list.
const A: ChatId = ChatId(11);
const B: ChatId = ChatId(12);
const C: ChatId = ChatId(13);
/// The one message in chat B.
const B_LAST: MessageId = MessageId(40);

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
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    (app, vcx)
}

fn photo(name: &str) -> ComposerAttachment {
    ComposerAttachment {
        path: std::env::temp_dir().join(name),
        kind: AttachmentKind::Photo,
        file_name: name.into(),
        spoiler: false,
        extras: Default::default(),
    }
}

/// In the open chat: a photo in the tray, a timer, caption above, a
/// schedule, previews off and "send silently".
fn choose_everything(app: &mut QuillApp) {
    let send = app.pending_send_mut().expect("an open chat");
    send.attachments.push(photo("a.jpg"));
    send.self_destruct = Some(SelfDestructSend::Timer(5));
    send.caption_above = true;
    send.scheduling = ComposerScheduling::SendWhenOnline;
    send.preview.disabled = true;
    app.toggle_composer_silent();
}

/// Nothing chosen in the open chat leaks into what it shows or sends.
fn assert_nothing_chosen(app: &QuillApp, path: &str) {
    assert!(
        app.pending_attachments().is_empty(),
        "{path}: attachment leaked"
    );
    let send = app.pending_send();
    assert!(send.self_destruct.is_none(), "{path}: self-destruct leaked");
    assert!(!send.caption_above, "{path}: caption-above leaked");
    let options = app.composer_send_options();
    assert!(
        matches!(options.scheduling, ComposerScheduling::None),
        "{path}: schedule leaked"
    );
    assert!(
        !options.link_preview_disabled,
        "{path}: preview choice leaked"
    );
    assert!(!options.disable_notification, "{path}: silent leaked");
}

fn open_chat(app: &QuillApp) -> Option<ChatId> {
    app.session().and_then(|session| session.open_chat)
}

#[gpui_kit::test]
fn a_chat_switch_shows_the_other_chats_choices_and_back_restores_them(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(A, window, cx);
        choose_everything(app);
        app.select_listed_chat(B, window, cx);
        assert_eq!(open_chat(app), Some(B));
        assert_nothing_chosen(app, "chat list");
        // Back in A, its tray and choices wait where they were left, like
        // its text draft (the tray sits in the compose area, which
        // tdesktop keeps per chat).
        app.select_listed_chat(A, window, cx);
        assert_eq!(app.pending_attachments().len(), 1);
        let options = app.composer_send_options();
        assert!(matches!(
            options.scheduling,
            ComposerScheduling::SendWhenOnline
        ));
        assert!(options.link_preview_disabled);
        assert!(options.disable_notification);
        assert_eq!(
            app.pending_send().self_destruct,
            Some(SelfDestructSend::Timer(5))
        );
    });
}

type OpenPath = fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>);

#[gpui_kit::test]
fn every_way_of_opening_a_chat_shows_its_own_choices(cx: &mut TestAppContext) {
    let paths: [(&str, OpenPath); 5] = [
        ("chat list", |app, window, cx| {
            app.select_listed_chat(B, window, cx)
        }),
        ("search result", |app, window, cx| {
            app.select_search_chat(B, window, cx)
        }),
        ("search message", |app, window, cx| {
            app.select_search_message(B, B_LAST, window, cx)
        }),
        ("story area", |app, _, cx| {
            app.story_area_open_message(B, B_LAST, cx)
        }),
        // Deep links, Settings > Ask a Question and new private chats are
        // opened by the driver once TDLib answers: nothing in the UI runs.
        ("driver", |app, _, _| {
            app.demo_session.as_mut().expect("demo").open_chat(B);
        }),
    ];
    let (app, mut vcx) = new_app(cx);
    for (path, open) in paths {
        app.update_in(&mut vcx, |app, window, cx| {
            app.select_listed_chat(A, window, cx);
            if app.pending_attachments().is_empty() {
                choose_everything(app);
            }
            open(app, window, cx);
            assert_eq!(open_chat(app), Some(B), "{path}");
            assert_nothing_chosen(app, path);
        });
    }
}

#[gpui_kit::test]
fn a_send_starts_its_chat_over_and_keeps_the_silent_choice(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(A, window, cx);
        let send = app.pending_send_mut().expect("an open chat");
        send.scheduling = ComposerScheduling::SendWhenOnline;
        send.preview.disabled = true;
        app.toggle_composer_silent();
        app.composer.update(cx, |input, cx| {
            input.set_value("see https://example.com", window, cx)
        });
        assert!(app.just_send_by_key(window, cx));
        // tdesktop clears the draft and its preview choice after a send.
        let options = app.composer_send_options();
        assert!(matches!(options.scheduling, ComposerScheduling::None));
        assert!(!options.link_preview_disabled);
        // The silent choice is the chat's, not the message's.
        assert!(options.disable_notification);
    });
}

#[gpui_kit::test]
fn silent_follows_each_chats_default_and_its_own_override(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let session = app.demo_session.as_mut().expect("demo");
        session.sync.set_default_silent(C.0, true);
        app.select_listed_chat(C, window, cx);
        assert!(
            app.composer_effective_silent(),
            "C sends silently by default"
        );
        app.toggle_composer_silent();
        assert!(!app.composer_effective_silent(), "turned on sound for C");
        app.select_listed_chat(B, window, cx);
        assert!(!app.composer_effective_silent(), "B has its own default");
        app.toggle_composer_silent();
        assert!(app.composer_effective_silent(), "B silenced");
        app.select_listed_chat(C, window, cx);
        assert!(!app.composer_effective_silent(), "C kept its choice");
        app.select_listed_chat(B, window, cx);
        assert!(app.composer_effective_silent(), "B kept its choice");
        // Toggled back to the default, nothing is left over.
        app.toggle_composer_silent();
        assert_eq!(app.composer_sends.silent_override(B), None);
    });
}

#[gpui_kit::test]
fn a_chat_window_keys_its_choices_by_chat_not_by_window(cx: &mut TestAppContext) {
    let (app, vcx) = new_app(cx);
    let main: AnyWindowHandle = vcx.window_handle();
    drop(vcx);
    cx.update_window(main, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.select_listed_chat(A, window, cx);
            app.pending_send_mut()
                .expect("an open chat")
                .attachments
                .push(photo("a.jpg"));
            app.open_chat_in_new_window(B, window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    let other = cx
        .windows()
        .into_iter()
        .find(|handle| handle.window_id() != main.window_id())
        .expect("B's window");
    cx.update_window(other, |_, window, cx| window.render_frame(cx))
        .unwrap();
    app.update(cx, |app, _| {
        // B's window: B's (empty) tray, then a file attached there.
        app.set_window_scope(Some(B));
        assert_eq!(open_chat(app), Some(B));
        assert!(app.pending_attachments().is_empty());
        app.pending_send_mut()
            .expect("B is open in its window")
            .attachments
            .push(photo("b.jpg"));
        // The main window still shows A's tray only.
        app.set_window_scope(None);
        assert_eq!(open_chat(app), Some(A));
        let names: Vec<_> = app
            .pending_attachments()
            .iter()
            .map(|attachment| attachment.file_name.clone())
            .collect();
        assert_eq!(names, ["a.jpg"]);
        // B's choice is stored under B, whichever window is drawing.
        let b = quill::composer::DraftTopic::Chat.key(B.0);
        assert_eq!(app.composer_sends.get(b).attachments.len(), 1);
    });
}
