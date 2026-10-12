//! The boost box opens from session state, renders each phase and takes
//! row clicks without boosting anything (needs the `demo-capture` feature
//! for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::boost_box::{BoostBoxState, BoostDecision, BoostNotice, BoostPhase};
use quill::telegram::envelope::{ParsedBoostSlot, ParsedBoostStatus};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

fn slot(id: i32, chat_id: i64, cooldown: i32) -> ParsedBoostSlot {
    ParsedBoostSlot {
        slot_id: id,
        chat_id,
        start_date: 1,
        expiration_date: 2_000_000_000,
        cooldown_until_date: cooldown,
    }
}

/// Open a box on the demo's first chat, with a status loaded.
fn open_box(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> i64 {
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        let chat_id = *session.chats.keys().min().expect("a chat");
        session.groups.chat_boost_details.insert(
            chat_id,
            ParsedBoostStatus {
                level: 1,
                boost_count: 3,
                current_level_boost_count: 2,
                next_level_boost_count: 5,
                ..ParsedBoostStatus::default()
            },
        );
        session.groups.boost_box = Some(BoostBoxState::new(chat_id, false));
        cx.notify();
        chat_id
    })
}

fn set_phase(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, decision: BoostDecision) {
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        session
            .groups
            .boost_box
            .as_mut()
            .expect("box open")
            .apply_decision(decision);
        cx.notify();
    });
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn the_box_opens_from_session_state_and_every_phase_renders(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_box(&app, vcx);
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::BoostBox));
    });
    set_phase(&app, vcx, BoostDecision::Boost(1));
    for notice in [
        BoostNotice::Already,
        BoostNotice::PremiumNeeded,
        BoostNotice::GiftedNoBoosts,
        BoostNotice::GiftMore {
            receive: 2,
            again: false,
        },
        BoostNotice::Flood {
            seconds_left: 7_300,
            group: false,
        },
    ] {
        set_phase(&app, vcx, BoostDecision::Notice(notice));
    }
    // A lone slot asks to replace; a list shows every row.
    set_phase(&app, vcx, BoostDecision::Reassign(vec![slot(1, 5, 0)]));
    set_phase(
        &app,
        vcx,
        BoostDecision::Reassign(vec![slot(1, 5, 0), slot(2, 6, 0), slot(3, 7, i32::MAX)]),
    );
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::BoostBox));
    });
}

#[gpui_kit::test]
fn a_row_click_picks_a_boost_and_a_waiting_row_does_not(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_box(&app, vcx);
    set_phase(
        &app,
        vcx,
        BoostDecision::Reassign(vec![slot(1, 5, 0), slot(2, 6, i32::MAX)]),
    );
    app.update_in(vcx, |app, _, cx| {
        app.boost_box_toggle(1, cx);
        app.boost_box_toggle(2, cx);
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        let state = app
            .session()
            .and_then(|session| session.groups.boost_box.as_ref())
            .expect("box open");
        let BoostPhase::Reassign { selected, .. } = &state.phase else {
            panic!("still the reassign list");
        };
        assert_eq!(selected, &vec![1]);
        assert_eq!(state.note, Some(quill::boost_box::waiting_note(0)));
    });
}

#[gpui_kit::test]
fn closing_clears_the_box_without_a_connection(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_box(&app, vcx);
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| app.close_boost_box_state(cx));
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(
            app.session()
                .is_some_and(|session| session.groups.boost_box.is_none())
        );
    });
    shell.read_with(vcx, |shell, _| {
        assert_ne!(shell.open_dialog, Some(DialogKind::BoostBox));
    });
}
