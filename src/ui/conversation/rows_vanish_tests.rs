//! A dissolve keeps its clock and ends in panes that build no rows.
use super::*;

#[gpui_kit::test]
fn a_dissolve_runs_and_ends_in_a_pane_without_rows(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    quill::power_saving::set(0);
    quill::power_saving::set_force_all(false);
    quill::power_saving::set_os_reduce_motion(false);
    app.update(vcx, |app, _| {
        app.begin_vanish(ChatId(GROUP), &[MessageId(202)]);
        app.paint_vanishing_for_test();
    });
    apply(
        &app,
        vcx,
        &[
            r#"{"@type":"updateDeleteMessages","chat_id":31,"message_ids":[202],"is_permanent":true,"from_cache":false}"#,
        ],
    );
    frames(vcx);
    let (running, _) = app.read_with(vcx, |app, _| app.vanishing_state_for_test());
    assert_eq!(running, 1, "the dissolve started with the rows");
    // The chat is closed: this pane builds no rows.
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.open_chat = None;
        }
        cx.notify();
    });
    frames(vcx);
    let (_, first) = app.read_with(vcx, |app, _| app.vanishing_state_for_test());
    // 1.2 s on: the row phase (1.05 s) is over, the dust (2 s) still flies.
    app.update(vcx, |app, cx| {
        app.advance_vanish_clock(1.2);
        cx.notify();
    });
    frames(vcx);
    let (running, second) = app.read_with(vcx, |app, _| app.vanishing_state_for_test());
    assert_eq!(running, 1, "the dust is still showing");
    assert!(second > first, "the clock moves on every frame of the pane");
    // The dust settles: the next frame drops it, so no tick is asked for.
    app.update(vcx, |app, cx| {
        app.advance_vanish_clock(5.);
        cx.notify();
    });
    frames(vcx);
    let (running, _) = app.read_with(vcx, |app, _| app.vanishing_state_for_test());
    assert_eq!(running, 0, "a finished dissolve is dropped without rows");
}
