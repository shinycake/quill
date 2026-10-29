//! UI tests for "Add People" over the 1:1 call demo (injected, nothing
//! reaches Telegram): the box lists every contact, typing filters them,
//! the server's people follow, and picks survive the search.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, RequestId};

/// Maya, the call's other side (`call-add-person-4502`).
const PEER: i64 = 4502;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// The demo's app and the call window with the box open.
fn open_box(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
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
                Some(ScreenshotDemo::named("ready-call-add-people")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let mut main = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut main);
    app.update(&mut main, |app, cx| app.sync_call_window(cx));
    frames(&mut main);
    let window = app
        .read_with(&main, |app, _| app.calls.window)
        .expect("the call window opens");
    let mut call = VisualTestContext::from_window(window, cx);
    frames(&mut call);
    (app, call)
}

fn shown(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (Vec<i64>, Vec<i64>) {
    app.read_with(vcx, |app, cx| {
        let view = app.call_add_people_view(cx).expect("the box is open");
        let ids = |rows: &[quill::state::AddPeopleRow]| rows.iter().map(|r| r.user_id).collect();
        (ids(&view.rows), ids(&view.global))
    })
}

#[gpui_kit::test]
fn every_contact_is_listed_with_the_other_side_already_in(cx: &mut TestAppContext) {
    let (app, mut call) = open_box(cx);
    let call = &mut call;
    assert!(call.debug_bounds("call-add-people").is_some());
    assert!(call.debug_bounds("call-add-people-link").is_some());
    let (rows, global) = shown(&app, call);
    assert_eq!(rows.len(), 8, "Maya and the seven contacts");
    assert!(global.is_empty());
    assert!(call.debug_bounds("call-add-person-4502").is_some());
    app.read_with(call, |app, cx| {
        let view = app.call_add_people_view(cx).expect("the box");
        let peer = view.rows.iter().find(|row| row.user_id == PEER);
        assert!(peer.is_some_and(|row| row.already_in && row.checked));
    });
}

#[gpui_kit::test]
fn typing_filters_the_contacts_and_the_server_people_follow(cx: &mut TestAppContext) {
    let (app, mut call) = open_box(cx);
    let call = &mut call;
    // The field has the focus: type into it.
    call.simulate_input("da");
    frames(call);
    let (rows, global) = shown(&app, call);
    // By last seen, then by name: Daniel Katz (online), Omer Ben David
    // and Yael Dagan (recently), Dana Levi (last week).
    assert_eq!(rows, [4510, 4512, 4511, 4513]);
    assert!(global.is_empty());
    assert!(
        call.debug_bounds("call-add-people-link").is_none(),
        "Create Call Link hides while searching"
    );
    // The server answers for "da".
    app.update(call, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            let search = &mut session.calls.add_people_search;
            search.begin("da", RequestId(1), RequestId(2));
            search.accept(RequestId(1), &[]);
            search.accept(RequestId(2), &[ChatId(4520), ChatId(4521)]);
        }
        cx.notify();
    });
    frames(call);
    assert_eq!(shown(&app, call).1, [4520, 4521]);
    assert!(call.debug_bounds("call-add-person-4520").is_some());
    // Enter picks the first match; a server person picked stays on top
    // once the field is cleared.
    app.update(call, |app, cx| {
        app.toggle_first_call_add_person(cx);
        app.toggle_call_add_person(4520, true, cx);
    });
    call.simulate_input("zzz");
    frames(call);
    assert!(call.debug_bounds("call-add-people-empty").is_some());
    call.simulate_keystrokes("backspace backspace backspace backspace backspace");
    frames(call);
    let (rows, _) = shown(&app, call);
    assert_eq!(rows.first(), Some(&4520));
    let picked = app.read_with(call, |app, _| {
        app.calls
            .add_people
            .as_ref()
            .map(|open| open.picked.clone())
    });
    assert_eq!(picked, Some(vec![4510, 4520]));
}

#[gpui_kit::test]
fn the_box_goes_with_its_call(cx: &mut TestAppContext) {
    let (app, mut call) = open_box(cx);
    let call = &mut call;
    call.simulate_input("da");
    app.update(call, |app, cx| app.toggle_first_call_add_person(cx));
    frames(call);
    // The call ends: the box, its field and its picks are gone.
    app.update(call, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.calls.active_call = None;
        }
        app.sync_call_window(cx);
    });
    let open = app.read_with(call, |app, _| app.calls.add_people.is_some());
    assert!(!open);
    // A new call's box starts empty.
    app.update(call, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                app.demo_ui.sink.clone();
            crate::ui::calls::call_video_demo::seed_video_call(
                session,
                false,
                &sink,
                &app.demo_ui.seq,
            );
        }
        app.open_call_add_people(cx);
    });
    let fresh = app.read_with(call, |app, cx| {
        let open = app.calls.add_people.as_ref().expect("the box");
        (open.picked.is_empty(), app.call_add_people_query(cx))
    });
    assert_eq!(fresh, (true, String::new()));
}
