//! The poll box's settings section (tdesktop `CreatePollBox`): the switch
//! rows and their order, "Limit Duration" with its "Poll ends" row and the
//! custom deadline box, and the keyboard button that opens the box.

use crate::ui::app::QuillApp;
use crate::ui::dialogs::poll::QuizLock;
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::local_time::now_unix;
use quill::poll::{POLL_DEFAULT_PERIOD_SECS, PollClose};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1000.), px(1400.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn dialog(app: &QuillApp) -> &crate::ui::dialogs::poll::PollDialog {
    app.composer_ui.poll_dialog.as_ref().unwrap()
}

fn open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, lock: QuizLock) {
    app.update_in(vcx, |app, window, cx| {
        app.open_poll_dialog_locked(lock, window, cx);
    });
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn the_box_opens_with_tdesktops_defaults(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    open(&app, &mut vcx, QuizLock::Free);
    app.update_in(&mut vcx, |app, _, _| {
        let dialog = dialog(app);
        assert!(!dialog.is_anonymous, "Show Who Voted is on");
        assert!(dialog.allows_multiple_answers);
        assert!(dialog.allow_adding_options);
        assert!(dialog.allows_revoting);
        assert!(dialog.shuffle_options);
        assert!(!dialog.is_quiz);
        assert!(!dialog.limit_duration);
    });
    vcx.update(|window, _| {
        for id in [
            "poll-toggle-who-voted",
            "poll-toggle-multiple",
            "poll-toggle-add-options",
            "poll-toggle-revoting",
            "poll-toggle-shuffle",
            "poll-toggle-quiz",
            "poll-toggle-duration",
        ] {
            assert!(window.try_find(id).is_some(), "{id} is shown");
        }
        // Channel-only rows and the duration's own rows stay hidden.
        for id in ["poll-toggle-subscribers", "poll-toggle-country", "poll-ends"] {
            assert!(window.try_find(id).is_none(), "{id} is hidden");
        }
    });
    app.update(&mut vcx, |app, cx| {
        assert!(!dialog(app).is_dirty(cx), "defaults are not input");
    });
}

#[gpui_kit::test]
fn set_correct_answer_turns_off_what_a_quiz_cannot_have(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    open(&app, &mut vcx, QuizLock::Free);
    vcx.update(|window, cx| window.click("poll-toggle-quiz", cx));
    app.update(&mut vcx, |app, cx| {
        let dialog = dialog(app);
        assert!(dialog.is_quiz);
        assert!(!dialog.allow_adding_options);
        assert!(!dialog.allows_revoting);
        assert!(dialog.add_options_locked());
        assert!(dialog.is_dirty(cx));
    });
    // The locked "Allow Adding Options" row ignores a click.
    vcx.update(|window, cx| window.click("poll-toggle-add-options", cx));
    app.update(&mut vcx, |app, _| assert!(!dialog(app).allow_adding_options));
}

#[gpui_kit::test]
fn a_poll_that_names_voters_only_can_take_new_options(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    open(&app, &mut vcx, QuizLock::Free);
    vcx.update(|window, cx| window.click("poll-toggle-who-voted", cx));
    app.update(&mut vcx, |app, _| {
        let dialog = dialog(app);
        assert!(dialog.is_anonymous);
        assert!(!dialog.allow_adding_options);
        assert!(dialog.add_options_locked());
    });
}

#[gpui_kit::test]
fn several_answers_can_be_right_only_with_multiple_answers(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    open(&app, &mut vcx, QuizLock::Free);
    app.update(&mut vcx, |app, _| {
        let dialog = app.composer_ui.poll_dialog.as_mut().unwrap();
        dialog.set_quiz(true);
        dialog.set_correct_row(0, true);
        dialog.set_correct_row(2, true);
        assert_eq!(dialog.quiz_correct_rows, [0, 2]);
        // Switching multiple off keeps one.
        dialog.set_multiple(false);
        assert_eq!(dialog.quiz_correct_rows, [0]);
        dialog.set_correct_row(1, true);
        assert_eq!(dialog.quiz_correct_rows, [1]);
        // A removed option moves the marks after it.
        dialog.set_multiple(true);
        dialog.set_correct_row(3, true);
        dialog.forget_correct_row(0);
        assert_eq!(dialog.quiz_correct_rows, [0, 2]);
    });
}

#[gpui_kit::test]
fn limit_duration_shows_the_ends_row_and_sends_the_period(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    open(&app, &mut vcx, QuizLock::Free);
    app.update(&mut vcx, |app, cx| {
        let draft = dialog(app).draft(cx);
        assert_eq!((draft.open_period, draft.close_date), (0, 0));
    });
    vcx.update(|window, cx| window.click("poll-toggle-duration", cx));
    vcx.run_until_parked();
    vcx.update(|window, _| {
        assert!(window.try_find("poll-ends").is_some());
        assert!(window.try_find("poll-toggle-hide-results").is_some());
    });
    app.update(&mut vcx, |app, cx| {
        let dialog = app.composer_ui.poll_dialog.as_mut().unwrap();
        assert_eq!(dialog.close, PollClose::Period(POLL_DEFAULT_PERIOD_SECS));
        dialog.hide_results_until_closes = true;
        let draft = dialog.draft(cx);
        assert_eq!(draft.open_period, POLL_DEFAULT_PERIOD_SECS);
        assert_eq!(draft.close_date, 0);
        assert!(draft.hide_results_until_closes);
        // Switching it off again drops the period and the hidden results.
        dialog.limit_duration = false;
        let draft = dialog.draft(cx);
        assert_eq!((draft.open_period, draft.close_date), (0, 0));
        assert!(!draft.hide_results_until_closes);
    });
}

#[gpui_kit::test]
fn a_custom_deadline_is_picked_in_its_own_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    open(&app, &mut vcx, QuizLock::Free);
    app.update_in(&mut vcx, |app, window, cx| {
        app.composer_ui.poll_dialog.as_mut().unwrap().limit_duration = true;
        app.open_poll_deadline_box(window, cx);
        assert!(app.poll_deadline_box_open());
    });
    vcx.run_until_parked();
    vcx.update(|window, _| {
        assert!(window.try_find("poll-deadline-set").is_some());
        assert!(window.try_find("poll-deadline-cancel").is_some());
    });
    // The picker opens a day out, so "Set Deadline" takes it.
    vcx.update(|window, cx| window.click("poll-deadline-set", cx));
    app.update(&mut vcx, |app, cx| {
        assert!(!app.poll_deadline_box_open());
        let PollClose::Date(date) = dialog(app).close else {
            panic!("a custom deadline replaces the span");
        };
        assert!((date - now_unix() - POLL_DEFAULT_PERIOD_SECS).abs() < 120);
        let draft = dialog(app).draft(cx);
        assert_eq!((draft.open_period, draft.close_date), (0, date));
    });
}

#[gpui_kit::test]
fn cancelling_the_deadline_box_keeps_the_choice(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    open(&app, &mut vcx, QuizLock::Free);
    app.update_in(&mut vcx, |app, window, cx| {
        let dialog = app.composer_ui.poll_dialog.as_mut().unwrap();
        dialog.limit_duration = true;
        dialog.close = PollClose::Period(3 * 3600);
        app.open_poll_deadline_box(window, cx);
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| window.click("poll-deadline-cancel", cx));
    app.update(&mut vcx, |app, _| {
        assert!(!app.poll_deadline_box_open());
        assert_eq!(dialog(app).close, PollClose::Period(3 * 3600));
    });
}

#[gpui_kit::test]
fn the_duration_cap_comes_from_tdlib(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update(&mut vcx, |app, _| {
        assert_eq!(app.poll_close_max_secs(), quill::poll::POLL_DEADLINE_MAX_SECS);
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.poll_open_period_max = Some(7 * 86_400);
        }
        assert_eq!(app.poll_close_max_secs(), 7 * 86_400);
    });
}

#[gpui_kit::test]
fn a_bot_button_fixes_the_poll_type(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    open(&app, &mut vcx, QuizLock::Quiz);
    app.update(&mut vcx, |app, _| {
        let dialog = dialog(app);
        assert!(dialog.is_quiz && dialog.quiz_locked());
        assert!(!dialog.allows_revoting);
    });
    // The locked switch cannot turn the quiz off.
    vcx.update(|window, cx| window.click("poll-toggle-quiz", cx));
    app.update(&mut vcx, |app, _| assert!(dialog(app).is_quiz));
    app.update(&mut vcx, |app, cx| app.close_poll_dialog(cx));

    open(&app, &mut vcx, QuizLock::Regular);
    vcx.update(|window, cx| window.click("poll-toggle-quiz", cx));
    app.update(&mut vcx, |app, _| assert!(!dialog(app).is_quiz));
}

#[gpui_kit::test]
fn the_request_poll_button_opens_the_box_for_the_type_it_asks_for(cx: &mut TestAppContext) {
    use quill::ids::MessageId;
    use quill::telegram::envelope::{KeyboardButton, KeyboardButtonType, ReplyKeyboard};
    let (app, mut vcx) = new_app(cx);
    for (force_regular, force_quiz, lock) in [
        (false, false, QuizLock::Free),
        (true, false, QuizLock::Regular),
        (false, true, QuizLock::Quiz),
    ] {
        app.update(&mut vcx, |app, cx| {
            let session = app.demo_session.as_mut().unwrap();
            let chat = session.open_chat.map_or(1, |id| id.0);
            session.bots.reply_keyboards.by_chat.insert(
                chat,
                Some((
                    MessageId(500),
                    ReplyKeyboard {
                    rows: vec![vec![KeyboardButton {
                        text: "Make a poll".into(),
                        kind: KeyboardButtonType::RequestPoll {
                            force_regular,
                            force_quiz,
                        },
                    }]],
                    is_persistent: false,
                    resize_keyboard: false,
                    one_time: false,
                    is_personal: false,
                    force_reply: false,
                    placeholder: String::new(),
                    },
                )),
            );
            cx.notify();
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| window.click("kbd-btn-500-0-0", cx));
        vcx.run_until_parked();
        app.update(&mut vcx, |app, cx| {
            assert_eq!(dialog(app).lock, lock);
            assert_eq!(dialog(app).is_quiz, lock == QuizLock::Quiz);
            app.close_poll_dialog(cx);
        });
    }
}
