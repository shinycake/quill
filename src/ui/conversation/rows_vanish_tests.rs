//! A dissolve keeps its clock and ends in panes that build no rows.
use super::*;
use crate::ui::vanish::Deletion;

#[gpui_kit::test]
fn a_dissolve_runs_and_ends_in_a_pane_without_rows(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    quill::power_saving::set(0);
    quill::power_saving::set_force_all(false);
    quill::power_saving::set_os_reduce_motion(false);
    app.update(vcx, |app, _| {
        app.begin_vanish(ChatId(GROUP), &[MessageId(202)], Deletion::Local);
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
            session.open = quill::state::OpenView::of(None);
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
        app.begin_vanish(ChatId(GROUP), &[MessageId(202)], Deletion::Local);
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
        app.begin_vanish(ChatId(GROUP), &[MessageId(202)], Deletion::Local);
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
            Deletion::Local,
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
        app.begin_vanish(ChatId(GROUP), &[MessageId(202)], Deletion::Local);
    });
    let shown = app.read_with(vcx, |app, _| app.vanishing_painted_for_test());
    assert_eq!(shown, [true], "painted in the latest frame");
    // Another chat shows: the group's rows are no longer painted.
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(ChatId(11), window, cx);
    });
    frames(vcx);
    app.update(vcx, |app, _| {
        app.begin_vanish(ChatId(GROUP), &[MessageId(203)], Deletion::Local);
    });
    let shown = app.read_with(vcx, |app, _| app.vanishing_painted_for_test());
    assert_eq!(shown, [true, false], "the group's row is out of date");
}

/// A message that is only missing from the rows built (a topic switch, a
/// search, a jump) is not a deleted one: its ghost keeps waiting, and no dust
/// plays for it until the session knows the deletion.
#[gpui_kit::test]
fn a_message_missing_from_the_rows_is_not_a_deleted_one(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    dissolves_enabled();
    app.update(vcx, |app, _| {
        app.begin_vanish(ChatId(GROUP), &[MessageId(202)], Deletion::Local);
        let mut rows: Vec<quill::state::HistoryMessage> = app
            .session()
            .and_then(|s| s.histories.get(&GROUP))
            .map(|h| {
                h.messages
                    .values()
                    .filter(|m| m.id != MessageId(202))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        assert!(!rows.is_empty(), "the chat has other messages");
        app.merge_vanishing(&mut rows);
        assert!(
            app.vanish_progress(ChatId(GROUP), MessageId(202)).is_none(),
            "not crumbling: the session does not know a deletion"
        );
    });
}

/// A refused delete (its answer is an error) ends its ghost at once: the
/// message is still there, and no dust plays for it.
#[gpui_kit::test]
fn a_refused_delete_drops_its_ghost_and_an_answered_one_waits(cx: &mut TestAppContext) {
    use quill::ids::RequestId;
    use quill::state::{Answer, RequestAcks};
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    dissolves_enabled();
    app.update(vcx, |app, _| {
        let mut acks = RequestAcks::default();
        let (refused, answered) = (RequestId(901), RequestId(902));
        acks.expect(refused);
        acks.expect(answered);
        app.begin_vanish(ChatId(GROUP), &[MessageId(202)], Deletion::Sent(refused));
        app.begin_vanish(ChatId(GROUP), &[MessageId(203)], Deletion::Sent(answered));
        acks.settle(refused, Answer::Failed("no".into()));
        acks.settle(answered, Answer::Done);
        app.settle_ghosts_for_test(&mut acks);
        let (held, _) = app.vanishing_state_for_test();
        assert_eq!(held, 1, "only the answered delete still waits");
    });
}

/// Sweep r4-12: `begin_vanish` says whether a ghost now holds the delete's
/// request; the callers register the request with the acks only then, so a
/// request no ghost holds (power saving, a message not in the history, a
/// ghost already there) leaves nothing in them.
#[gpui_kit::test]
fn a_delete_no_ghost_holds_registers_no_ack(cx: &mut TestAppContext) {
    use quill::ids::RequestId;
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    dissolves_enabled();
    app.update(vcx, |app, _| {
        let sent = Deletion::Sent(RequestId(910));
        assert!(app.begin_vanish(ChatId(GROUP), &[MessageId(202)], sent));
        assert!(
            !app.begin_vanish(ChatId(GROUP), &[MessageId(202)], sent),
            "a ghost already holds that message"
        );
        assert!(
            !app.begin_vanish(ChatId(GROUP), &[MessageId(999)], sent),
            "not in the history"
        );
        quill::power_saving::set_force_all(true);
        assert!(
            !app.begin_vanish(ChatId(GROUP), &[MessageId(203)], sent),
            "power saving: no effect, no ghost"
        );
        quill::power_saving::set_force_all(false);
    });
}

/// A wait that ran out forgets its request, and a ghost already crumbling
/// (the update that removed the message came before the answer) lets go of
/// its request when the answer comes: neither leaves an ack behind.
#[gpui_kit::test]
fn a_ghost_that_ends_or_crumbles_leaves_no_ack(cx: &mut TestAppContext) {
    use quill::ids::RequestId;
    use quill::state::{Answer, RequestAcks};
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    dissolves_enabled();
    let (waits, lands) = (RequestId(920), RequestId(921));
    app.update(vcx, |app, _| {
        app.freeze_vanish_clock();
        app.begin_vanish(ChatId(GROUP), &[MessageId(202)], Deletion::Sent(waits));
        app.begin_vanish(ChatId(GROUP), &[MessageId(203)], Deletion::Sent(lands));
        app.paint_vanishing_for_test();
    });
    apply(
        &app,
        vcx,
        &[
            r#"{"@type":"updateDeleteMessages","chat_id":31,"message_ids":[203],"is_permanent":true,"from_cache":false}"#,
        ],
    );
    frames(vcx);
    app.update(vcx, |app, _| {
        let mut acks = RequestAcks::default();
        acks.expect(waits);
        acks.expect(lands);
        // The crumbling one's answer comes late; the waiting one's never.
        acks.settle(lands, Answer::Done);
        app.advance_vanish_clock(11.0);
        app.settle_ghosts_for_test(&mut acks);
        assert!(acks.take(lands).is_none(), "its answer was taken");
        // Forgotten: an answer that comes now is not kept.
        acks.settle(waits, Answer::Failed("late".into()));
        assert!(acks.take(waits).is_none(), "its wait was forgotten");
    });
}

/// A waiting ghost whose message is already gone, in a chat that is not the
/// one shown, is dropped: back in that chat the message must not reappear
/// and crumble on arrival.
#[gpui_kit::test]
fn a_waiting_ghost_of_a_chat_not_shown_is_dropped_once_its_message_is_gone(
    cx: &mut TestAppContext,
) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    dissolves_enabled();
    app.update(vcx, |app, _| {
        app.begin_vanish(ChatId(GROUP), &[MessageId(202)], Deletion::Local);
        app.paint_vanishing_for_test();
        // Another chat is shown (none here) when the deletion lands.
        if let Some(session) = app.demo_session.as_mut() {
            session.open = quill::state::OpenView::of(None);
        }
    });
    apply(
        &app,
        vcx,
        &[
            r#"{"@type":"updateDeleteMessages","chat_id":31,"message_ids":[202],"is_permanent":true,"from_cache":false}"#,
        ],
    );
    app.update(vcx, |app, cx| {
        assert_eq!(app.vanishing_state_for_test().0, 1, "still waiting");
        app.settle_vanishing(cx);
        assert_eq!(app.vanishing_state_for_test().0, 0, "dropped");
    });
}
