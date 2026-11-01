//! Nothing here opens TDLib: the app has no credentials, so a retry stops at
//! the gate, and `prepare_connect` is covered in `connect::tests`.
use crate::ui::app::{PaneMode, QuillApp};
use crate::ui::connect_ui::ConnectUiStatus;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::auth::StartupFailure;
use quill::connect::ConnectBlocker;
use quill::telegram::envelope::AuthorizationState;

const RETRY: &str = "retry-connection";
const PHONE_FORM: &str = "country-select";

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-failure-card");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root.clone());
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let demo = ScreenshotDemo::named("need-tdjson");
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update(&mut vcx, |app, _| {
        app.background.test_root = Some(root);
    });
    vcx.run_until_parked();
    (app, vcx)
}

fn shown(vcx: &mut VisualTestContext, id: &'static str) -> bool {
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.update(|window, _| window.try_find(id).is_some())
}

/// Whether the card shows `text`, as a screen reader would read it.
fn reads(vcx: &mut VisualTestContext, text: &str) -> bool {
    let quoted = format!("{text:?}");
    vcx.update(|window, cx| {
        window.set_a11y_forced(true);
        window.render_frame(cx);
        let tree = window.a11y_tree_snapshot().expect("a tree");
        format!("{:?}", tree.nodes).contains(&quoted)
    })
}

#[gpui_kit::test]
fn each_blocker_shows_its_reason_and_retry_and_never_the_phone_form(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    for (blocker, failure) in [
        (ConnectBlocker::LockedStore, StartupFailure::KeyLocked),
        (ConnectBlocker::StoreError, StartupFailure::KeyUnreadable),
        (
            ConnectBlocker::MissingKeyAgainstExistingDb,
            StartupFailure::KeyMissing,
        ),
        (ConnectBlocker::DataFolder, StartupFailure::DataFolder),
        (ConnectBlocker::MissingTdjson, StartupFailure::Library),
        (ConnectBlocker::TdjsonLoad, StartupFailure::Library),
        (ConnectBlocker::TdjsonVersion, StartupFailure::Library),
        (ConnectBlocker::ReceiveThread, StartupFailure::NoThread),
    ] {
        app.update(&mut vcx, |app, cx| {
            app.connection.status = ConnectUiStatus::Live;
            app.auth_ui.demo_state = AuthorizationState::WaitTdlibParameters;
            app.open_client(Err(blocker), cx);
        });
        assert!(reads(&mut vcx, failure.reason()), "{failure:?}");
        assert!(shown(&mut vcx, RETRY));
        assert!(!shown(&mut vcx, PHONE_FORM));
        assert!(app.read_with(&vcx, |app, _| app.connection.status_note().is_empty()));
    }
}

#[gpui_kit::test]
fn a_database_tdlib_refused_shows_its_reason_and_retry(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    for failure in [StartupFailure::WrongKey, StartupFailure::DataFolder] {
        app.update(&mut vcx, |app, cx| {
            app.connection.status = ConnectUiStatus::Live;
            app.auth_ui.demo_state = AuthorizationState::StartupFailed(failure);
            cx.notify();
        });
        assert!(reads(&mut vcx, failure.reason()), "{failure:?}");
        assert!(shown(&mut vcx, RETRY));
        assert!(!shown(&mut vcx, PHONE_FORM));
    }
}

#[gpui_kit::test]
fn a_build_without_credentials_keeps_its_note_and_offers_no_retry(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update(&mut vcx, |app, cx| {
        app.open_client(Err(ConnectBlocker::MissingCredentials), cx);
    });
    assert!(app.read_with(&vcx, |app, _| {
        matches!(app.pane_mode(), PaneMode::Synthetic) && !app.connection.status_note().is_empty()
    }));
    assert!(!shown(&mut vcx, RETRY));
}

#[gpui_kit::test]
fn trying_again_leaves_the_card_for_a_new_connection(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update(&mut vcx, |app, cx| {
        app.open_client(Err(ConnectBlocker::LockedStore), cx);
    });
    assert!(shown(&mut vcx, RETRY));
    let generation = app.read_with(&vcx, |app, _| app.connection.generation);
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.click(RETRY, cx);
    });
    assert_eq!(
        app.read_with(&vcx, |app, _| app.current_auth()),
        AuthorizationState::WaitTdlibParameters,
        "the reason goes as soon as a new attempt starts"
    );
    assert!(!shown(&mut vcx, RETRY));
    assert_eq!(
        app.read_with(&vcx, |app, _| app.connection.generation),
        generation + 1
    );
    vcx.run_until_parked();
}
