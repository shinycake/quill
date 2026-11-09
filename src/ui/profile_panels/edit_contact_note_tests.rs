//! Edit contact opened before the full user info arrives: Save never sends
//! the empty field as the note, and the note goes in once it lands (needs
//! the `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::dialogs::{ContactNote, ProfileDialog};
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::state::UserFullInfoData;

const USER: i64 = 81;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
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
                Some(ScreenshotDemo::named("ready-profile-user")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let Some(app) = slot.borrow().clone() else {
        panic!("the window opened");
    };
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn frame(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
}

fn note_state(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
) -> Option<(ContactNote, String)> {
    app.read_with(vcx, |app, cx| match &app.dialogs.profile_dialog {
        Some(ProfileDialog::EditContact(dialog)) => {
            Some((dialog.note, dialog.note_input.read(cx).value().to_string()))
        }
        _ => None,
    })
}

#[gpui_kit::test]
fn edit_contact_waits_for_the_note_before_saving(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        if let Some(session) = app.session_mut() {
            assert!(session.user(USER).is_some(), "the demo has user {USER}");
            session.users_state.user_full_infos.remove(&USER);
        }
        app.open_edit_contact_dialog(USER, window, cx);
    });
    frame(vcx);
    assert_eq!(
        note_state(&app, vcx),
        Some((ContactNote::Waiting, String::new()))
    );
    // Round 17 C finding 24: Save before the note is known goes through
    // (the note is sent as "keep", null), so a full-info fetch that never
    // answers does not hold the rename up for good.
    let saved = app.update_in(vcx, |app, _, cx| app.submit_edit_contact(cx));
    assert!(saved);

    app.update_in(vcx, |app, _, _| {
        let mut info = UserFullInfoData::default();
        info.extras.note = "Met at the conference".into();
        if let Some(session) = app.session_mut() {
            session.users_state.user_full_infos.insert(USER, info);
        }
    });
    frame(vcx);
    assert_eq!(
        note_state(&app, vcx),
        Some((ContactNote::Seeded, "Met at the conference".to_string()))
    );
}

#[gpui_kit::test]
fn a_note_typed_while_waiting_is_kept(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        if let Some(session) = app.session_mut() {
            session.users_state.user_full_infos.remove(&USER);
        }
        app.open_edit_contact_dialog(USER, window, cx);
        if let Some(ProfileDialog::EditContact(dialog)) = &app.dialogs.profile_dialog {
            dialog
                .note_input
                .update(cx, |input, cx| input.set_value("typed", window, cx));
        }
        let mut info = UserFullInfoData::default();
        info.extras.note = "old".into();
        if let Some(session) = app.session_mut() {
            session.users_state.user_full_infos.insert(USER, info);
        }
    });
    frame(vcx);
    assert_eq!(
        note_state(&app, vcx),
        Some((ContactNote::Seeded, "typed".to_string()))
    );
}
