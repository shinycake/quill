//! UI integration tests for the Telegram Business dialog against the demo
//! `QuillApp` (gpui-kit's test support needs the `demo-capture` feature).
//! Nothing here reaches a live account: demo mode refuses every save.
use super::{MessageKind, ScheduleChoice, View};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::business_hours_edit::extract_day;
use quill::quick_replies::{QuickRepliesUpdate, QuickReplyShortcut};
use quill::telegram::envelope::ChatKind;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
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

/// A Premium demo account with a `hello` shortcut that has messages.
fn seed(app: &Entity<QuillApp>, cx: &mut VisualTestContext, premium: bool) {
    app.update(cx, |app, _| {
        let session = app.demo_session.as_mut().expect("demo session");
        session.payments.premium_option = Some(premium);
        session.apply_quick_replies(QuickRepliesUpdate::Shortcut(QuickReplyShortcut {
            id: 2,
            name: "hello".into(),
            message_count: 1,
        }));
    });
}

fn open(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| app.update(cx, |app, cx| app.open_business(window, cx)));
    frames(cx);
}

fn go(app: &Entity<QuillApp>, cx: &mut VisualTestContext, view: View) {
    cx.update(|window, cx| app.update(cx, |app, cx| app.business_go(view, window, cx)));
    frames(cx);
}

fn act(
    app: &Entity<QuillApp>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&mut QuillApp, &mut gpui_kit::Window, &mut gpui_kit::Context<QuillApp>),
) {
    cx.update(|window, cx| app.update(cx, |app, cx| f(app, window, cx)));
    frames(cx);
}

fn view(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> View {
    app.read_with(cx, |app, _| {
        app.dialogs.business_dialog.as_ref().unwrap().view
    })
}

fn error(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Option<String> {
    app.read_with(cx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .local_error
            .clone()
    })
}

#[gpui_kit::test]
fn every_screen_renders(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    open(&app, vcx);
    assert_eq!(view(&app, vcx), View::Hub);
    for screen in [
        View::Greeting,
        View::Away,
        View::Hours,
        View::Day(2),
        View::TimeZone,
        View::Location,
        View::Hub,
    ] {
        go(&app, vcx, screen);
        assert_eq!(view(&app, vcx), screen);
    }
}

#[gpui_kit::test]
fn without_premium_a_feature_does_not_open(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, false);
    open(&app, vcx);
    act(&app, vcx, |app, window, cx| {
        app.business_open_feature(Some(View::Greeting), window, cx)
    });
    // The Premium screen opens instead (a live session loads it); the
    // greeting editor does not.
    assert_eq!(view(&app, vcx), View::Hub);
    let draft = app.read_with(vcx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .greeting
            .is_some()
    });
    assert!(!draft);
}

#[gpui_kit::test]
fn greeting_messages_open_quick_replies_above_the_hub(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    open(&app, vcx);
    go(&app, vcx, View::Greeting);
    act(&app, vcx, |app, _, cx| {
        app.set_business_enabled(MessageKind::Greeting, true, cx)
    });
    act(&app, vcx, |app, window, cx| {
        app.edit_business_messages(MessageKind::Greeting, window, cx)
    });
    let (qr, business) = app.read_with(vcx, |app, _| {
        (
            app.dialogs.quick_replies_dialog.is_some(),
            app.dialogs.business_dialog.is_some(),
        )
    });
    assert!(qr && business);
    // Closing quick replies comes back to the greeting draft.
    act(&app, vcx, |app, _, cx| app.close_quick_replies(cx));
    assert_eq!(view(&app, vcx), View::Greeting);
    let enabled = app.read_with(vcx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .greeting
            .as_ref()
            .unwrap()
            .enabled
    });
    assert!(enabled);
}

#[gpui_kit::test]
fn the_limit_keeps_away_messages_off(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    app.update(vcx, |app, _| {
        let qr = &mut app.demo_session.as_mut().unwrap().settings.quick_replies;
        qr.shortcut_count_max = Some(1);
    });
    open(&app, vcx);
    go(&app, vcx, View::Away);
    act(&app, vcx, |app, _, cx| {
        app.set_business_enabled(MessageKind::Away, true, cx)
    });
    assert!(error(&app, vcx).unwrap().contains("too many quick replies"));
    let enabled = app.read_with(vcx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .away
            .as_ref()
            .unwrap()
            .enabled
    });
    assert!(!enabled);
}

#[gpui_kit::test]
fn picking_chats_selects_only_those(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    // One private chat to pick.
    let chat = app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().unwrap();
        let (id, chat) = session.chats.iter_mut().next().unwrap();
        chat.kind = ChatKind::Private {
            user_id: quill::ids::UserId(*id),
        };
        *id
    });
    open(&app, vcx);
    go(&app, vcx, View::Greeting);
    act(&app, vcx, |app, window, cx| {
        app.open_business_picker(MessageKind::Greeting, true, window, cx)
    });
    assert_eq!(view(&app, vcx), View::Picker);
    let offered = app.read_with(vcx, |app, _| app.business_picker_chats());
    assert!(offered.iter().any(|(id, _)| *id == chat));
    act(&app, vcx, |app, _, cx| {
        if let Some(p) = app.business_mut().and_then(|d| d.picker.as_mut()) {
            assert!(p.chats.toggle_chat(chat));
        }
        cx.notify();
    });
    act(&app, vcx, |app, window, cx| {
        app.finish_business_picker(window, cx)
    });
    assert_eq!(view(&app, vcx), View::Greeting);
    let recipients = app.read_with(vcx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .greeting
            .as_ref()
            .unwrap()
            .recipients
            .clone()
    });
    assert!(!recipients.all_but_excluded);
    assert_eq!(recipients.included.chat_ids, vec![chat]);
    // Removing the last chat goes back to "all except".
    act(&app, vcx, |app, _, cx| {
        app.remove_business_entry(MessageKind::Greeting, Some(chat), None, cx)
    });
    let all = app.read_with(vcx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .greeting
            .as_ref()
            .unwrap()
            .recipients
            .all_but_excluded
    });
    assert!(all);
}

#[gpui_kit::test]
fn a_day_gets_a_set_of_hours(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    open(&app, vcx);
    go(&app, vcx, View::Hours);
    go(&app, vcx, View::Day(0));
    act(&app, vcx, |app, window, cx| {
        app.add_business_day_row(window, cx)
    });
    act(&app, vcx, |app, window, cx| {
        app.save_business_day(0, window, cx)
    });
    assert_eq!(view(&app, vcx), View::Hours);
    let week = app.read_with(vcx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .hours
            .as_ref()
            .unwrap()
            .week
            .clone()
    });
    assert_eq!(extract_day(&week, 0), vec![(8 * 60, 20 * 60)]);
    assert!(extract_day(&week, 1).is_empty());
}

#[gpui_kit::test]
fn saving_in_demo_mode_says_a_live_session_is_needed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    open(&app, vcx);
    go(&app, vcx, View::Greeting);
    act(&app, vcx, |app, _, cx| {
        app.set_business_enabled(MessageKind::Greeting, true, cx)
    });
    act(&app, vcx, |app, window, cx| {
        app.save_business_greeting(window, cx)
    });
    assert_eq!(
        error(&app, vcx).as_deref(),
        Some("Business settings need a live session.")
    );
    assert_eq!(view(&app, vcx), View::Greeting);
}

#[gpui_kit::test]
fn an_away_message_needs_its_shortcut_and_a_valid_range(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    open(&app, vcx);
    go(&app, vcx, View::Away);
    act(&app, vcx, |app, _, cx| {
        app.set_business_enabled(MessageKind::Away, true, cx)
    });
    act(&app, vcx, |app, window, cx| {
        app.save_business_away(window, cx)
    });
    assert_eq!(
        error(&app, vcx).as_deref(),
        Some("Create an away message first.")
    );
    app.update(vcx, |app, _| {
        app.demo_session
            .as_mut()
            .unwrap()
            .apply_quick_replies(QuickRepliesUpdate::Shortcut(QuickReplyShortcut {
                id: 3,
                name: "away".into(),
                message_count: 2,
            }));
    });
    // A custom range that ends before it starts.
    act(&app, vcx, |app, window, cx| {
        let away = app.business_mut().unwrap().away.as_mut().unwrap();
        away.schedule = ScheduleChoice::Custom;
        let (start, end) = (away.start.clone(), away.end.clone());
        let value = |unix| crate::ui::scheduled::picker_value(unix).unwrap();
        start.update(cx, |s, cx| {
            s.set_date_time(value(1_900_000_000), window, cx)
        });
        end.update(cx, |s, cx| {
            s.set_date_time(value(1_800_000_000), window, cx)
        });
    });
    act(&app, vcx, |app, window, cx| {
        app.save_business_away(window, cx)
    });
    assert_eq!(
        error(&app, vcx).as_deref(),
        Some("The end time must be after the start time.")
    );
}
