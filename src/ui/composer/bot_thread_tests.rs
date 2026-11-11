//! UI tests for a Send in the "All" view of a bot whose users create
//! threads, over the `ready-bot-threads` demo (threads 2 and 3 exist).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::telegram::envelope::MessageContent;

const BOT: ChatId = ChatId(21);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-bot-threads")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn frames(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

fn send(app: &Entity<QuillApp>, cx: &mut VisualTestContext, text: &str) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| {
                input.set_value(text.to_string(), window, cx)
            });
            app.submit_composer(text.to_string(), window, cx);
        });
    });
    frames(cx);
}

fn texts(messages: impl Iterator<Item = quill::state::HistoryMessage>) -> Vec<String> {
    messages
        .filter_map(|message| match message.content {
            MessageContent::Text(text) => Some(text.text),
            _ => None,
        })
        .collect()
}

#[gpui_kit::test]
fn all_ends_with_the_new_thread_card(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.read_with(&vcx, |app, _| {
        let session = app.session().unwrap();
        assert!(session.new_bot_thread_row(BOT).is_some());
        assert!(session.send_starts_bot_thread(BOT, None));
    });
    // Inside a thread there is no card, and a send stays there.
    vcx.update(|_, cx| app.update(cx, |app, cx| app.select_topic_ui(2, cx)));
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        let session = app.session().unwrap();
        assert!(session.new_bot_thread_row(BOT).is_none());
        assert!(!session.send_starts_bot_thread(BOT, None));
    });
}

#[gpui_kit::test]
fn a_send_from_all_opens_a_new_thread_with_the_message(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let in_all = app.read_with(&vcx, |app, _| {
        let session = app.session().unwrap();
        texts(
            session
                .histories
                .get(&BOT.0)
                .unwrap()
                .ordered()
                .into_iter()
                .cloned(),
        )
    });
    send(&app, &mut vcx, "Plan a weekend in Lisbon");
    app.read_with(&vcx, |app, cx| {
        let session = app.session().unwrap();
        let topic = session.open_topic.expect("the new thread is open");
        assert!(topic > 3, "a new thread, not one of the old ones");
        let made = session.threads.forum_topics.get(&BOT.0).unwrap();
        let made = made.iter().find(|t| t.forum_topic_id == topic).unwrap();
        assert_eq!(made.name, "New Chat");
        let history = session
            .threads
            .topic_histories
            .get(&(BOT.0, topic))
            .unwrap();
        assert_eq!(
            texts(history.messages.values().cloned()),
            ["Plan a weekend in Lisbon"]
        );
        assert_eq!(app.composer.read(cx).value().to_string(), "");
    });
    // The "All" history did not get it as an off-thread message.
    app.read_with(&vcx, |app, _| {
        let session = app.session().unwrap();
        let all = session.histories.get(&BOT.0).unwrap().ordered();
        let all = texts(
            all.into_iter()
                .filter(|m| m.extras.forum_topic_id.is_none())
                .cloned(),
        );
        assert_eq!(all, in_all);
    });
}
