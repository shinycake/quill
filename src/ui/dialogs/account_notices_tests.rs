//! The account notices: a force-logout notice no close can pop, and the
//! terms prompt, whose step and age tick belong to one version of the
//! terms.
use super::{TermsPrompt, TermsStage};
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::state::ServiceNotice;
use quill::telegram::envelope::TermsOfService;

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
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

fn terms(id: &str) -> TermsOfService {
    TermsOfService {
        id: id.into(),
        text: "Be nice.".into(),
        min_user_age: 18,
        show_popup: true,
    }
}

/// Esc, the backdrop and any programmatic close run `dismiss_account_notice`:
/// the force-logout notice must stay until Log out.
#[gpui_kit::test]
fn a_force_logout_notice_cannot_be_dismissed(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update(&mut vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.settings.notices.service.extend([
                ServiceNotice {
                    kind: "AUTH_KEY_DROP_DUPLICATE".into(),
                    text: "Logged out".into(),
                },
                ServiceNotice {
                    kind: "OTHER".into(),
                    text: "Hello".into(),
                },
            ]);
        }
        app.dismiss_account_notice(cx);
    });
    let left = app.read_with(&vcx, |app, _| {
        app.session().map(|s| s.settings.notices.service.len())
    });
    assert_eq!(left, Some(2), "the force-logout notice stays");
    // An ordinary notice is popped.
    app.update(&mut vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.settings.notices.service.pop_front();
        }
        app.dismiss_account_notice(cx);
    });
    let left = app.read_with(&vcx, |app, _| {
        app.session().map(|s| s.settings.notices.service.len())
    });
    assert_eq!(left, Some(0));
}

/// A step reached in one version of the terms (the decline steps, the age
/// tick) is not the next version's.
#[gpui_kit::test]
fn the_terms_prompt_starts_over_for_another_version(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update(&mut vcx, |app, _| {
        app.set_terms_stage(
            "tos-1",
            TermsStage::Terms {
                age_ok: true,
                age_error: false,
            },
        );
        assert_eq!(
            app.terms_stage("tos-1"),
            TermsStage::Terms {
                age_ok: true,
                age_error: false
            }
        );
        app.set_terms_stage("tos-1", TermsStage::DeleteWarning);
        assert_eq!(app.terms_stage("tos-2"), TermsStage::default());
        assert_eq!(app.terms_stage("tos-1"), TermsStage::DeleteWarning);
    });
    // The terms cleared (accepted elsewhere): the prompt is gone with them.
    app.update(&mut vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.settings.notices.terms = Some(terms("tos-1"));
        }
        app.drain_account_notices();
        assert!(
            app.auth_ui.terms_prompt.is_some(),
            "kept while the terms stand"
        );
        if let Some(session) = app.demo_session.as_mut() {
            session.settings.notices.terms = None;
        }
        app.drain_account_notices();
        assert_eq!(app.auth_ui.terms_prompt, None::<TermsPrompt>);
    });
}

/// The frozen-account box the user opened ends with the freeze: when the
/// account is frozen again later, nothing opens by itself.
#[gpui_kit::test]
fn a_freeze_notice_closed_by_the_lifted_freeze_does_not_reopen(cx: &mut TestAppContext) {
    use crate::ui::account_state::OpenNotice;
    use quill::telegram::envelope::FreezeStateUpdate;
    let (app, mut vcx) = open(cx);
    let freeze = |frozen: bool| FreezeStateUpdate {
        is_frozen: frozen,
        freezing_date: 1,
        deletion_date: 2,
        appeal_link: String::new(),
    };
    app.update(&mut vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.sync.set_freeze(freeze(true));
        }
        app.account_boxes.notice = OpenNotice::Frozen;
        assert_eq!(app.account_notice(), Some(super::AccountNotice::Frozen));
        // The server lifts the freeze while the box is open.
        if let Some(session) = app.demo_session.as_mut() {
            session.sync.set_freeze(freeze(false));
        }
        assert_eq!(app.account_notice(), None);
        app.drain_account_notices();
        // Frozen again, later: the user has not asked for the box.
        if let Some(session) = app.demo_session.as_mut() {
            session.sync.set_freeze(freeze(true));
        }
        assert_eq!(app.account_boxes.notice, OpenNotice::None);
        assert_eq!(app.account_notice(), None);
    });
}
