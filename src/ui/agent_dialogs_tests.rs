//! The request dialog and the all-chats consent in a real window: Allow
//! applies the grant and answers, Don't allow, Escape and a new request
//! behind the old one.
use crate::ui::agent_state::{TEST_LOCK, reset_for_test};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::agent::grants::{self, Scope};
use quill::agent::prompts::{self, Ask, Decision};
use quill::ids::AccountKey;
use std::time::Duration;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-agent-dialogs-ui");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot
        .borrow()
        .clone()
        .unwrap_or_else(|| panic!("window opened"));
    (
        app,
        shell,
        VisualTestContext::from_window(handle.into(), cx),
    )
}

fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        vcx.run_until_parked();
        vcx.executor().advance_clock(Duration::from_millis(300));
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_kind(shell: &Entity<QuillShell>, vcx: &mut VisualTestContext) -> Option<DialogKind> {
    shell.read_with(vcx, |shell, _| shell.open_dialog)
}

fn ask_chat(scope: Scope) -> Ask {
    Ask::Chat {
        chat_id: 7,
        title: "Ann Lee".into(),
        secret: false,
        scope,
    }
}

fn file(ask: Ask) -> (u64, std::sync::mpsc::Receiver<Decision>) {
    prompts::file(
        AccountKey::primary(),
        "Claude Code".into(),
        ask,
        "To summarize the trip.".into(),
    )
    .unwrap_or_else(|| panic!("filed"))
}

fn answer_of(id: u64, rx: &std::sync::mpsc::Receiver<Decision>) -> Option<Decision> {
    prompts::wait(id, rx, Duration::from_millis(10))
}

#[gpui_kit::test]
fn allow_applies_the_grant_then_answers(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (_app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    reset_for_test(&AccountKey::primary());
    settle(vcx);
    assert_eq!(open_kind(&shell, vcx), None);

    let (id, rx) = file(ask_chat(Scope::Read));
    settle(vcx);
    assert_eq!(open_kind(&shell, vcx), Some(DialogKind::AgentRequest));
    assert!(
        grants::for_account(&AccountKey::primary()).chats.is_empty(),
        "nothing is granted before the user answers"
    );

    vcx.update(|window, cx| window.click("agent-request-allow", cx));
    settle(vcx);
    assert_eq!(answer_of(id, &rx), Some(Decision::Allowed));
    let granted = grants::for_account(&AccountKey::primary());
    assert_eq!(
        granted.chats.get(&7).map(|g| (g.scope, g.title.as_str())),
        Some((Some(Scope::Read), "Ann Lee"))
    );
    assert_eq!(open_kind(&shell, vcx), None, "the dialog closed");
}

#[gpui_kit::test]
fn dont_allow_and_escape_answer_denied(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (_app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    reset_for_test(&AccountKey::primary());
    settle(vcx);

    let (id, rx) = file(Ask::Contacts);
    settle(vcx);
    assert_eq!(open_kind(&shell, vcx), Some(DialogKind::AgentRequest));
    vcx.update(|window, cx| window.click("agent-request-deny", cx));
    settle(vcx);
    assert_eq!(answer_of(id, &rx), Some(Decision::Denied));
    assert!(!grants::for_account(&AccountKey::primary()).contacts);
    assert_eq!(open_kind(&shell, vcx), None);

    let (id, rx) = file(Ask::AllChats { scope: Scope::Read });
    settle(vcx);
    assert_eq!(open_kind(&shell, vcx), Some(DialogKind::AgentRequest));
    vcx.update(|window, cx| window.press("escape", cx));
    settle(vcx);
    assert_eq!(answer_of(id, &rx), Some(Decision::Denied));
    assert_eq!(grants::for_account(&AccountKey::primary()).all_chats, None);
    assert_eq!(open_kind(&shell, vcx), None);
}

#[gpui_kit::test]
fn a_second_request_gets_its_own_dialog(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (_app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    reset_for_test(&AccountKey::primary());
    settle(vcx);

    let (first, first_rx) = file(Ask::Contacts);
    let (second, second_rx) = file(ask_chat(Scope::ReadWrite));
    settle(vcx);
    vcx.update(|window, cx| window.click("agent-request-allow", cx));
    settle(vcx);
    assert_eq!(answer_of(first, &first_rx), Some(Decision::Allowed));
    assert_eq!(
        open_kind(&shell, vcx),
        Some(DialogKind::AgentRequest),
        "the next request opens as soon as the first is answered"
    );
    vcx.update(|window, cx| window.click("agent-request-allow", cx));
    settle(vcx);
    assert_eq!(answer_of(second, &second_rx), Some(Decision::Allowed));
    let granted = grants::for_account(&AccountKey::primary());
    assert!(granted.contacts);
    assert_eq!(
        granted.chats.get(&7).and_then(|g| g.scope),
        Some(Scope::ReadWrite)
    );
    assert_eq!(open_kind(&shell, vcx), None);
}

#[gpui_kit::test]
fn a_request_for_another_account_waits(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (_app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    reset_for_test(&AccountKey::primary());
    settle(vcx);
    let other = AccountKey::new("account-9").unwrap_or_else(|| panic!("a valid key"));
    let filed = prompts::file(other, "Claude Code".into(), Ask::Contacts, String::new());
    let (id, rx) = filed.unwrap_or_else(|| panic!("filed"));
    settle(vcx);
    assert_eq!(open_kind(&shell, vcx), None);
    prompts::answer(id, Decision::Denied);
    assert_eq!(answer_of(id, &rx), Some(Decision::Denied));
}
