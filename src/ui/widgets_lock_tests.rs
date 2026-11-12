//! Message widgets leave their windows when Quill locks: a locked window
//! draws no history pass, so nothing else would say they went.
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::widgets_state::{Showing, WidgetEntry};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};
use quill::message_widget::WidgetKey;
use quill::message_widget::storage::Stored;

#[gpui_kit::test]
fn locking_takes_every_widget_off_screen(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("window built");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    let key = WidgetKey {
        chat_id: ChatId(11),
        message_id: MessageId(5),
    };
    app.update(&mut vcx, |app, cx| {
        let mut entry = WidgetEntry::new(Stored::default(), 120.);
        entry.presence.show(
            None,
            Showing {
                width: 300.,
                view: None,
            },
        );
        app.widgets.entries.insert(key, entry);
        assert!(app.widgets.entries[&key].presence.is_shown());
        app.account.passcode.enabled = true;
        app.lock_by_passcode(cx);
        assert!(
            !app.widgets.entries[&key].presence.is_shown(),
            "the locked window shows no widget"
        );
    });
}

/// A poll whose request left flight with no answer (dropped by the stale
/// sweep or a reconnect) is due again after a period instead of staying
/// `Asking` for as long as the widget lives.
#[gpui_kit::test]
fn a_poll_whose_request_was_dropped_is_due_again(cx: &mut TestAppContext) {
    use crate::ui::widgets_state::Poll;
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("window built");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    let key = WidgetKey {
        chat_id: ChatId(11),
        message_id: MessageId(6),
    };
    app.update(&mut vcx, |app, cx| {
        let mut entry = WidgetEntry::new(Stored::default(), 120.);
        entry.presence.show(
            None,
            Showing {
                width: 300.,
                view: None,
            },
        );
        entry.poll = Poll::Asking {
            request: quill::ids::RequestId(4242),
            asked: std::time::Instant::now(),
        };
        app.widgets.entries.insert(key, entry);
        app.schedule_widget_polls(cx);
        assert!(
            matches!(app.widgets.entries[&key].poll, Poll::Due(_)),
            "no such request is out any more"
        );
    });
}

/// An answer that arrived while Quill was locked left flight too, but it is
/// not a dropped request: it waits in the session for the drain, and lands
/// on its poll afterwards.
#[gpui_kit::test]
fn an_answer_waiting_for_the_drain_is_not_a_dropped_poll(cx: &mut TestAppContext) {
    use crate::ui::widgets_state::Poll;
    use quill::ids::RequestId;
    use quill::message_widget::WidgetAnswer;
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("window built");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    let key = WidgetKey {
        chat_id: ChatId(11),
        message_id: MessageId(7),
    };
    app.update(&mut vcx, |app, cx| {
        let mut entry = WidgetEntry::new(Stored::default(), 120.);
        entry.presence.show(
            None,
            Showing {
                width: 300.,
                view: None,
            },
        );
        entry.poll = Poll::Asking {
            request: RequestId(4243),
            asked: std::time::Instant::now(),
        };
        app.widgets.entries.insert(key, entry);
        let answer = WidgetAnswer::PublicData {
            key,
            request: RequestId(4243),
            data: Some(("{}".into(), std::time::Duration::from_secs(30))),
        };
        if let Some(session) = app.session_mut() {
            session.bots.widget_answers.push(answer);
        }
        app.schedule_widget_polls(cx);
        assert!(
            matches!(app.widgets.entries[&key].poll, Poll::Asking { .. }),
            "its answer is still to land"
        );
        let answers = app
            .session_mut()
            .map(|session| std::mem::take(&mut session.bots.widget_answers))
            .unwrap_or_default();
        app.land_widget_answers(answers, cx);
        let entry = &app.widgets.entries[&key];
        assert_eq!(entry.polled.as_deref(), Some("{}"));
        assert!(matches!(entry.poll, Poll::Due(_)));
    });
}
