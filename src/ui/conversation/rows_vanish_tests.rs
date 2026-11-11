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

/// Round 7, 2g: a dissolve is matched by chat and message. Message ids
/// repeat across channels and supergroups: a row of another chat with the
/// same id is not cut away.
#[gpui_kit::test]
fn a_dissolve_is_its_own_chats(cx: &mut TestAppContext) {
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
    app.read_with(vcx, |app, _| {
        assert!(app.vanish_progress(ChatId(GROUP), MessageId(202)).is_some());
        assert!(
            app.vanish_progress(ChatId(GROUP + 1), MessageId(202))
                .is_none(),
            "another chat's message 202 is not the one deleted"
        );
    });
}

fn dissolves_enabled() {
    quill::power_saving::set(0);
    quill::power_saving::set_force_all(false);
    quill::power_saving::set_os_reduce_motion(false);
}

/// A delete the server never answers (refused, offline) shows nothing, so
/// it asks for no frames: the 60 fps redraw starts with the deletion.
#[gpui_kit::test]
fn a_delete_still_waiting_asks_for_no_frames(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    dissolves_enabled();
    app.update(vcx, |app, _| {
        app.begin_vanish(ChatId(GROUP), &[MessageId(202)]);
        app.paint_vanishing_for_test();
    });
    frames(vcx);
    let demand = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.update(vcx, |app, cx| {
            app.frame.window_active.set(true);
            app.frame.animation_demand.set(0);
            let _ = app.vanish_overlay(cx);
            app.frame.animation_demand.get()
        })
    };
    assert_eq!(demand(&app, vcx), 0, "waiting for the answer: no ticks");
    apply(
        &app,
        vcx,
        &[
            r#"{"@type":"updateDeleteMessages","chat_id":31,"message_ids":[202],"is_permanent":true,"from_cache":false}"#,
        ],
    );
    frames(vcx);
    assert!(demand(&app, vcx) > 0, "the dissolve runs once it landed");
}

/// Deleting the only messages left: the empty list still starts the
/// dissolves, once the session knows the messages are gone.
#[gpui_kit::test]
fn the_last_messages_of_a_chat_dissolve_too(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    dissolves_enabled();
    app.update(vcx, |app, _| {
        app.begin_vanish(
            ChatId(GROUP),
            &[MessageId(201), MessageId(202), MessageId(203)],
        );
        app.paint_vanishing_for_test();
    });
    apply(
        &app,
        vcx,
        &[
            r#"{"@type":"updateDeleteMessages","chat_id":31,"message_ids":[201,202,203],"is_permanent":true,"from_cache":false}"#,
        ],
    );
    frames(vcx);
    app.read_with(vcx, |app, _| {
        for id in [201, 202, 203] {
            assert!(
                app.vanish_progress(ChatId(GROUP), MessageId(id)).is_some(),
                "message {id} dissolves in the emptied chat"
            );
        }
    });
}

/// The dust starts where the row is painted now. A row painted in an
/// older frame (scrolled away, or another chat is showing) is not there
/// any more: that message crumbles without dust, not over other rows.
#[gpui_kit::test]
fn dust_starts_only_from_a_row_painted_in_the_latest_frame(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    dissolves_enabled();
    app.update(vcx, |app, _| {
        app.begin_vanish(ChatId(GROUP), &[MessageId(202)]);
    });
    let shown = app.read_with(vcx, |app, _| app.vanishing_painted_for_test());
    assert_eq!(shown, [true], "painted in the latest frame");
    // Another chat shows: the group's rows are no longer painted.
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(ChatId(11), window, cx);
    });
    frames(vcx);
    app.update(vcx, |app, _| {
        app.begin_vanish(ChatId(GROUP), &[MessageId(203)]);
    });
    let shown = app.read_with(vcx, |app, _| app.vanishing_painted_for_test());
    assert_eq!(shown, [true, false], "the group's row is out of date");
}
