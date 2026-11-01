//! Rows after an update that patches or splices them are the rows a full
//! build makes from the same state, around sender runs, day separators,
//! the unread divider and albums.
use super::*;

const BASE_DATE: i64 = 1790632000;

/// Message `n` of the history: ids leave a gap for a message to land
/// between two.
fn id(n: i64) -> i64 {
    1000 + 2 * n
}

/// Runs of three: Ann, Bob, Ann, then the user's own, again and again; a
/// new day every ten messages; an album of three (60..=62, Ann, opening
/// a day) and one of two (75..=76, Bob).
fn new_message(n: i64) -> String {
    let id = id(n);
    let date = BASE_DATE + (n / 10) * 86400 + n * 60;
    let album = match n {
        60..=62 => 500,
        75..=76 => 501,
        _ => 0,
    };
    let (sender, outgoing) = match (n / 3) % 4 {
        _ if album == 500 => (21, false),
        _ if album == 501 => (22, false),
        0 | 2 => (21, false),
        1 => (22, false),
        _ => (0, true),
    };
    let sender = if outgoing {
        String::new()
    } else {
        format!(r#""sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"#)
    };
    let content = if album != 0 {
        photo(9000 + n)
    } else {
        format!(
            r#"{{"@type":"messageText","text":{{"@type":"formattedText","text":"Message {n}","entities":[]}}}}"#
        )
    };
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":31,{sender}"is_outgoing":{outgoing},"date":{date},"media_album_id":"{album}","content":{content}}}}}"#
    )
}

fn photo(file: i64) -> String {
    format!(
        r#"{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"x","photo":{{"@type":"file","id":{file},"size":10,"expected_size":10,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"r{file}","unique_id":"u{file}","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}}},"width":1200,"height":800,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}"#
    )
}

const MESSAGES: i64 = 120;
/// The first unread message is the one after this.
const READ_UP_TO: i64 = 89;

fn mixed_group(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    open_group(app, cx);
    app.update(cx, |app, _| app.history.follow_revision_in_tests = true);
    let updates: Vec<String> = (0..MESSAGES).map(new_message).collect();
    let refs: Vec<&str> = updates.iter().map(String::as_str).collect();
    apply(app, cx, &refs);
    app.update(cx, |app, cx| {
        if let Some(history) = app
            .demo_session
            .as_mut()
            .and_then(|session| session.histories.get_mut(&GROUP))
        {
            history.unread_anchor = Some(MessageId(id(READ_UP_TO)));
        }
        cx.notify();
    });
    frames(cx);
}

/// Everything a row shows, apart from its chrome generation.
fn facts(app: &QuillApp) -> Vec<String> {
    app.history
        .rows
        .iter()
        .map(|row| match row {
            HistoryRow::Single(row) => format!(
                "{:?}",
                (
                    &row.message,
                    &row.chrome,
                    row.spot,
                    row.highlighted,
                    row.run_start,
                    row.joined_above,
                    &row.day_label,
                    row.unread_divider,
                    row.fact_check_expanded,
                    row.is_secret,
                    row.vanishing,
                )
            ),
            HistoryRow::Album {
                album_id,
                messages,
                chrome,
                spot,
                day_label,
                unread_divider,
                ..
            } => format!(
                "{:?}",
                (album_id, messages, chrome, spot, day_label, unread_divider)
            ),
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    Patch,
    Splice,
    /// Every row built again from the messages, at least once.
    Build,
}

type Stats = crate::ui::history_row::RowCacheStats;

fn stats(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Stats {
    app.read_with(cx, |app, _| app.history.row_stats)
}

/// Apply `updates`, check the rows were patched or spliced (not built
/// again) and equal the rows a full build makes, and that the scroller
/// lists every row.
fn check(
    app: &Entity<QuillApp>,
    cx: &mut VisualTestContext,
    name: &str,
    updates: &[&str],
    expect: Expect,
) {
    let before = stats(app, cx);
    apply(app, cx, updates);
    compare_with_full_build(app, cx, name, before, expect);
}

/// Like `check`, for a change made straight on the session.
fn check_after(
    app: &Entity<QuillApp>,
    cx: &mut VisualTestContext,
    name: &str,
    change: impl FnOnce(&mut quill::state::Session),
    expect: Expect,
) {
    let before = stats(app, cx);
    app.update(cx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            change(session);
        }
        cx.notify();
    });
    frames(cx);
    compare_with_full_build(app, cx, name, before, expect);
}

fn compare_with_full_build(
    app: &Entity<QuillApp>,
    cx: &mut VisualTestContext,
    name: &str,
    before: Stats,
    expect: Expect,
) {
    let after = stats(app, cx);
    match expect {
        Expect::Patch => {
            assert_eq!(after.rebuilds, before.rebuilds, "{name}: rows built whole");
            assert!(after.rows_patched > before.rows_patched, "{name}");
        }
        Expect::Splice => {
            assert_eq!(after.rebuilds, before.rebuilds, "{name}: rows built whole");
            assert_eq!(after.splices, before.splices + 1, "{name}");
        }
        Expect::Build => {
            assert!(after.rebuilds > before.rebuilds, "{name}: built whole");
            assert_eq!(after.splices, before.splices, "{name}: not spliced");
        }
    }
    let (kept, listed, rows) = app.read_with(cx, |app, cx| {
        (
            facts(app),
            app.history.scroller.read(cx).item_count(),
            app.history.rows.len(),
        )
    });
    assert_eq!(listed, rows, "{name}: scroller in step");
    app.update(cx, |app, cx| {
        app.history.rows_key = None;
        cx.notify();
    });
    frames(cx);
    let built = app.read_with(cx, |app, _| facts(app));
    assert_eq!(built.len(), kept.len(), "{name}: row count");
    for (ix, (kept, built)) in kept.iter().zip(&built).enumerate() {
        assert_eq!(kept, built, "{name}: row {ix}");
    }
}

fn delete(ns: &[i64]) -> String {
    let ids: Vec<String> = ns.iter().map(|n| id(*n).to_string()).collect();
    format!(
        r#"{{"@type":"updateDeleteMessages","chat_id":31,"message_ids":[{}],"is_permanent":true,"from_cache":false}}"#,
        ids.join(",")
    )
}

fn text_message(id: i64, sender: Option<i64>, date: i64, extra: &str) -> String {
    let sender = sender.map_or(String::new(), |user| {
        format!(r#""sender_id":{{"@type":"messageSenderUser","user_id":{user}}},"#)
    });
    let outgoing = sender.is_empty();
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":31,{sender}"is_outgoing":{outgoing},"date":{date},{extra}"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"New {id}","entities":[]}}}}}}}}"#
    )
}

#[gpui_kit::test]
fn patched_and_spliced_rows_equal_a_full_build(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    let unread = app.read_with(vcx, |app, _| {
        app.history
            .rows
            .iter()
            .position(HistoryRow::unread_divider)
            .and_then(|ix| app.history.rows.get(ix))
            .and_then(HistoryRow::first_id)
    });
    assert_eq!(
        unread,
        Some(MessageId(id(READ_UP_TO + 1))),
        "divider set up"
    );
    let edit = format!(
        r#"{{"@type":"updateMessageContent","chat_id":31,"message_id":{},"new_content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Edited.","entities":[]}}}}}}"#,
        id(100)
    );
    check(&app, vcx, "edit", &[&edit], Expect::Patch);
    let reaction = format!(
        r#"{{"@type":"updateMessageInteractionInfo","chat_id":31,"message_id":{},"interaction_info":{{"@type":"messageInteractionInfo","view_count":3,"forward_count":0,"reactions":{{"@type":"messageReactions","reactions":[{{"@type":"messageReaction","type":{{"@type":"reactionTypeEmoji","emoji":"👍"}},"total_count":1,"is_chosen":false,"recent_sender_ids":[]}}],"are_tags":false}}}}}}"#,
        id(101)
    );
    check(&app, vcx, "reaction", &[&reaction], Expect::Patch);
    let last_date = BASE_DATE + 11 * 86400 + 200 * 60;
    let new_at_end = text_message(id(MESSAGES), Some(22), last_date, "");
    check(
        &app,
        vcx,
        "new message at the end",
        &[&new_at_end],
        Expect::Splice,
    );
    let same_sender = text_message(id(MESSAGES + 1), Some(22), last_date + 60, "");
    check(
        &app,
        vcx,
        "same sender again",
        &[&same_sender],
        Expect::Splice,
    );
    let next_day = text_message(id(MESSAGES + 2), Some(21), last_date + 86400, "");
    check(
        &app,
        vcx,
        "a new day at the end",
        &[&next_day],
        Expect::Splice,
    );
    let pending_id = id(MESSAGES + 3);
    let pending = text_message(
        pending_id,
        None,
        last_date + 86460,
        r#""sending_state":{"@type":"messageSendingStatePending","sending_id":0},"#,
    );
    check(
        &app,
        vcx,
        "own message pending",
        &[&pending],
        Expect::Splice,
    );
    let sent = format!(
        r#"{{"@type":"updateMessageSendSucceeded","old_message_id":{pending_id},"message":{{"id":{},"chat_id":31,"is_outgoing":true,"date":{},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"New {pending_id}","entities":[]}}}}}}}}"#,
        pending_id + 1,
        last_date + 86460
    );
    check(&app, vcx, "send succeeded", &[&sent], Expect::Splice);
    let failed_id = id(MESSAGES + 4);
    let pending = text_message(
        failed_id,
        None,
        last_date + 86520,
        r#""sending_state":{"@type":"messageSendingStatePending","sending_id":0},"#,
    );
    check(
        &app,
        vcx,
        "second message pending",
        &[&pending],
        Expect::Splice,
    );
    let failed = format!(
        r#"{{"@type":"updateMessageSendFailed","old_message_id":{failed_id},"message":{{"id":{},"chat_id":31,"is_outgoing":true,"date":{},"sending_state":{{"@type":"messageSendingStateFailed","can_retry":true}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"New {failed_id}","entities":[]}}}}}},"error":{{"code":500}}}}"#,
        failed_id + 1,
        last_date + 86520
    );
    check(&app, vcx, "send failed", &[&failed], Expect::Splice);
    let between = text_message(
        id(33) + 1,
        Some(21),
        BASE_DATE + 3 * 86400 + 33 * 60 + 30,
        "",
    );
    check(
        &app,
        vcx,
        "new message in the middle",
        &[&between],
        Expect::Splice,
    );
    check(
        &app,
        vcx,
        "first of a day deleted",
        &[&delete(&[50])],
        Expect::Splice,
    );
    check(
        &app,
        vcx,
        "first unread deleted",
        &[&delete(&[READ_UP_TO + 1])],
        // The bar leaves with its message, so the rows key changes.
        Expect::Build,
    );
    check(
        &app,
        vcx,
        "last of a run deleted",
        &[&delete(&[8])],
        Expect::Splice,
    );
    check(
        &app,
        vcx,
        "album item deleted",
        &[&delete(&[61])],
        Expect::Splice,
    );
    check(
        &app,
        vcx,
        "album down to one",
        &[&delete(&[76])],
        Expect::Splice,
    );
    check(
        &app,
        vcx,
        "a run deleted, its neighbours join",
        &[&delete(&[27, 28, 29])],
        Expect::Splice,
    );
    check(
        &app,
        vcx,
        "first message deleted",
        &[&delete(&[0])],
        Expect::Splice,
    );
}

#[gpui_kit::test]
fn a_new_first_unread_moves_the_divider_from_rows_further_down(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    // Read up to message 8: the user's own 9..=11 follow, so the divider
    // sits on 12, four rows below the message that lands next.
    app.update(vcx, |app, cx| {
        if let Some(history) = app
            .demo_session
            .as_mut()
            .and_then(|session| session.histories.get_mut(&GROUP))
        {
            history.unread_anchor = Some(MessageId(id(8)));
        }
        cx.notify();
    });
    frames(vcx);
    let spliced = app.read_with(vcx, |app, _| app.history.row_stats.rows_spliced);
    let landed = text_message(id(8) + 1, Some(22), BASE_DATE + 8 * 60 + 30, "");
    check(&app, vcx, "a new first unread", &[&landed], Expect::Splice);
    let divider = app.read_with(vcx, |app, _| {
        app.history
            .rows
            .iter()
            .filter(|row| row.unread_divider())
            .filter_map(HistoryRow::first_id)
            .collect::<Vec<_>>()
    });
    assert_eq!(divider, vec![MessageId(id(8) + 1)]);
    let built = app.read_with(vcx, |app, _| app.history.row_stats.rows_spliced) - spliced;
    assert!(built >= 6, "the rows down to the old divider: {built}");
}

#[gpui_kit::test]
fn changes_far_apart_build_the_rows_whole(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    let before = app.read_with(vcx, |app, _| app.history.row_stats);
    apply(&app, vcx, &[&delete(&[5, 110])]);
    let after = app.read_with(vcx, |app, _| app.history.row_stats);
    assert_eq!(
        (after.rebuilds, after.splices),
        (before.rebuilds + 1, before.splices)
    );
}

/// The main history's messages, oldest first.
fn main_messages(session: &quill::state::Session) -> Vec<quill::state::HistoryMessage> {
    session
        .histories
        .get(&GROUP)
        .map(|history| history.messages.values().cloned().collect())
        .unwrap_or_default()
}

fn top_message(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Option<(MessageId, f32)> {
    app.read_with(cx, |app, cx| {
        let (ix, offset) = app.history.scroller.read(cx).top_anchor()?;
        Some((app.history.rows.get(ix)?.first_id()?, offset))
    })
}

fn following_tail(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> bool {
    app.read_with(cx, |app, cx| {
        app.history.scroller.read(cx).is_following_tail()
    })
}

#[gpui_kit::test]
fn a_replaced_history_builds_the_rows_whole(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    // The window is replaced by a small one without the window epoch
    // moving (as `history_entry` default or a reopened sublist do): few
    // messages are newer than the cached revision, and none are gone.
    check_after(
        &app,
        vcx,
        "a replaced history",
        |session| {
            let kept: Vec<_> = main_messages(session)
                .into_iter()
                .skip(50)
                .take(5)
                .collect();
            if let Some(history) = session.histories.get_mut(&GROUP) {
                history.messages = quill::state::HistoryMessages::default();
                for message in kept {
                    history.messages.insert(message.id.0, message);
                }
            }
        },
        Expect::Build,
    );
    let rows = app.read_with(vcx, |app, _| app.history.rows.len());
    assert!(rows <= 8, "only the new window's rows are left: {rows}");
}

#[gpui_kit::test]
fn a_crumbling_message_makes_the_rows_build_whole(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    // The dissolve must really start: chat effects on, motion not reduced.
    quill::power_saving::set(0);
    quill::power_saving::set_force_all(false);
    quill::power_saving::set_os_reduce_motion(false);
    app.update(vcx, |app, _| {
        app.begin_vanish(ChatId(GROUP), &[MessageId(id(5))]);
    });
    let crumbling = app.read_with(vcx, |app, _| !app.history.vanishing.borrow().is_empty());
    assert!(crumbling, "begin_vanish skipped the dissolve");
    check(
        &app,
        vcx,
        "delete while dissolving",
        &[&delete(&[5])],
        Expect::Build,
    );
    // A single new message while the dust is in the air: never a splice.
    let revision = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.histories.get(&GROUP))
            .map(|h| h.messages.revision())
    });
    let revision = revision.expect("the open chat's history");
    let landed = text_message(id(MESSAGES + 1), Some(21), BASE_DATE + 20 * 86400, "");
    check(
        &app,
        vcx,
        "new message while dissolving",
        &[&landed],
        Expect::Build,
    );
    // Revision taken just before that one message: only the vanish guard
    // can decline. With the dissolve gone the same call splices.
    let splice_since = |app: &QuillApp| {
        let history = app.session()?.histories.get(&GROUP)?;
        Some(
            app.row_splice(
                &history.messages,
                history.messages.instance(),
                revision,
                None,
                0,
            )
            .is_some(),
        )
    };
    let (while_crumbling, still_crumbling) = app.read_with(vcx, |app, _| {
        (
            splice_since(app),
            !app.history.vanishing.borrow().is_empty(),
        )
    });
    assert!(still_crumbling, "the dissolve ended before the check");
    assert_eq!(while_crumbling, Some(false), "splice while dissolving");
    let after = app.read_with(vcx, |app, _| {
        app.history.vanishing.borrow_mut().clear();
        splice_since(app)
    });
    assert_eq!(after, Some(true), "one new message splices without dust");
}

#[gpui_kit::test]
fn a_dissolve_keeps_the_rows_key_until_it_moves_on(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    quill::power_saving::set(0);
    quill::power_saving::set_force_all(false);
    quill::power_saving::set_os_reduce_motion(false);
    let key = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.pin_vanish_now();
            app.vanish_rows_hash()
        })
    };
    assert_eq!(key(&app, vcx), None, "no dissolve, no key part");
    app.update(vcx, |app, _| {
        app.begin_vanish(ChatId(GROUP), &[MessageId(id(5))]);
        app.paint_vanishing_for_test();
    });
    check(
        &app,
        vcx,
        "delete while dissolving",
        &[&delete(&[5])],
        Expect::Build,
    );
    // Put the animation at the moment under test before each stretch of
    // frames.
    let frames_at = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext, secs: f32| {
        app.update(vcx, |app, _| app.set_vanishing_clock(secs));
        let builds = stats(app, vcx).rebuilds;
        let first = key(app, vcx);
        for _ in 0..3 {
            app.update(vcx, |_, cx| cx.notify());
            frames(vcx);
            assert_eq!(key(app, vcx), first, "same key on every frame at {secs}s");
        }
        (first, builds)
    };
    // The message is among the rows, being cut away.
    let (rows, builds) = frames_at(&app, vcx, 0.05);
    assert!(rows.is_some(), "the dissolve runs");
    assert_eq!(
        stats(&app, vcx).rebuilds,
        builds,
        "frames of a dissolve build no rows"
    );
    // The row is gone and only dust is left: the key moves, once.
    let (dust, builds) = frames_at(&app, vcx, 1.2);
    assert_ne!(dust, rows, "the row's end changes the key");
    let rebuilt = stats(&app, vcx).rebuilds;
    assert!(rebuilt > builds, "the row's end builds the rows again");
    assert_eq!(
        rebuilt,
        builds + 1,
        "and only once, not on every frame of the dust"
    );
    // The dust settles: the dissolve ends, and so does its key part.
    app.update(vcx, |app, _| app.set_vanishing_clock(2.5));
    assert_ne!(key(&app, vcx), dust, "the dissolve's end changes the key");
    app.update(vcx, |_, cx| cx.notify());
    frames(vcx);
    assert_eq!(key(&app, vcx), None, "finished dissolves are dropped");
}

#[gpui_kit::test]
fn only_the_closing_gap_is_remeasured_by_list_index(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    quill::power_saving::set(0);
    quill::power_saving::set_force_all(false);
    quill::power_saving::set_os_reduce_motion(false);
    app.update(vcx, |app, _| {
        app.begin_vanish(ChatId(GROUP), &[MessageId(id(5))]);
        app.paint_vanishing_for_test();
    });
    check(
        &app,
        vcx,
        "delete while dissolving",
        &[&delete(&[5])],
        Expect::Build,
    );
    let closing = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext, secs: f32| {
        app.update(vcx, |app, _| app.set_vanishing_clock(secs));
        app.update(vcx, |_, cx| cx.notify());
        frames(vcx);
        app.read_with(vcx, |app, _| {
            let list_ix = app
                .history
                .rows
                .iter()
                .position(|row| row.first_id() == Some(MessageId(id(5))));
            (app.closing_rows(), list_ix)
        })
    };
    let (before, _) = closing(&app, vcx, 0.05);
    assert!(before.is_empty(), "not while the row is cut away");
    for _ in 0..3 {
        let (rows, list_ix) = closing(&app, vcx, 0.6);
        let list_ix = list_ix.expect("the ghost's row is among the rows");
        assert_eq!(rows, vec![list_ix], "its list index, every frame");
        assert_ne!(list_ix, 5, "a list index, not the history index");
    }
    let (after, _) = closing(&app, vcx, 1.2);
    assert!(after.is_empty(), "stops after the collapse window");
}

#[gpui_kit::test]
fn two_reads_in_one_round_see_the_same_phase(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    quill::power_saving::set(0);
    quill::power_saving::set_force_all(false);
    quill::power_saving::set_os_reduce_motion(false);
    app.update(vcx, |app, _| {
        app.begin_vanish(ChatId(GROUP), &[MessageId(id(5))]);
        app.paint_vanishing_for_test();
    });
    check(
        &app,
        vcx,
        "delete while dissolving",
        &[&delete(&[5])],
        Expect::Build,
    );
    // Shortly before the end of the row phase (1.05s: wipe plus a 0.6s
    // collapse). The clock set also pins its instant.
    let (first, second, later) = app.update(vcx, |app, _| {
        app.set_vanishing_clock(1.00);
        let first = app.vanish_rows_hash();
        // Time passes inside the round and crosses the boundary: the
        // pinned instant holds.
        app.advance_vanish_clock(0.06);
        let second = app.vanish_rows_hash();
        // The next round pins again and sees the later phase.
        app.pin_vanish_now();
        (first, second, app.vanish_rows_hash())
    });
    assert_eq!(first, second, "both reads see the phase of the pin");
    assert_ne!(second, later, "the next round sees the new phase");
}

#[gpui_kit::test]
fn an_unreadable_dissolve_fails_closed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    let revision = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.histories.get(&GROUP))
            .map(|h| h.messages.revision())
    });
    let revision = revision.expect("the open chat's history");
    let landed = text_message(id(MESSAGES + 1), Some(21), BASE_DATE + 20 * 86400, "");
    check(&app, vcx, "one new message", &[&landed], Expect::Splice);
    let (idle, busy, splice) = app.read_with(vcx, |app, _| {
        let idle = app.vanish_rows_hash();
        let held = app.history.vanishing.borrow_mut();
        let busy = app.vanish_rows_hash();
        let splice = app
            .session()
            .and_then(|s| s.histories.get(&GROUP))
            .and_then(|h| app.row_splice(&h.messages, h.messages.instance(), revision, None, 0));
        drop(held);
        (idle, busy, splice.is_some())
    });
    assert_eq!(idle, None, "no dissolve, no key part");
    assert!(busy.is_some(), "an unreadable dissolve moves the rows key");
    assert!(!splice, "and the splice stands down");
}

#[gpui_kit::test]
fn a_splice_keeps_the_reader_where_they_were(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    app.update(vcx, |app, cx| {
        app.history
            .scroller
            .update(cx, |state, cx| state.scroll_to_item_offset(40, 12., cx));
    });
    frames(vcx);
    let left = top_message(&app, vcx).expect("scrolled away from the tail");
    assert_eq!(left.1, 12.);
    assert!(!following_tail(&app, vcx));
    // Below the viewport: a message lands among the later rows, then one
    // goes.
    let below = text_message(
        id(100) + 1,
        Some(21),
        BASE_DATE + 10 * 86400 + 100 * 60 + 30,
        "",
    );
    check(&app, vcx, "landing below", &[&below], Expect::Splice);
    assert_eq!(top_message(&app, vcx), Some(left), "landing below");
    check(
        &app,
        vcx,
        "deleting below",
        &[&delete(&[110])],
        Expect::Splice,
    );
    assert_eq!(top_message(&app, vcx), Some(left), "deleting below");
    // Above the viewport: rows before the top one come and go.
    let above = text_message(id(3) + 1, Some(21), BASE_DATE + 3 * 60 + 30, "");
    check(&app, vcx, "landing above", &[&above], Expect::Splice);
    assert_eq!(top_message(&app, vcx), Some(left), "landing above");
    check(
        &app,
        vcx,
        "deleting above",
        &[&delete(&[5])],
        Expect::Splice,
    );
    assert_eq!(top_message(&app, vcx), Some(left), "deleting above");
    assert!(!following_tail(&app, vcx), "still away from the tail");
}

#[gpui_kit::test]
fn a_splice_keeps_following_the_tail(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    app.update(vcx, |app, cx| {
        app.history
            .scroller
            .update(cx, |state, cx| state.scroll_to_end(cx));
    });
    frames(vcx);
    assert!(following_tail(&app, vcx), "at the tail");
    // A row comes inside the list, then one goes, then one at the end.
    let middle = text_message(
        id(40) + 1,
        Some(21),
        BASE_DATE + 4 * 86400 + 40 * 60 + 30,
        "",
    );
    check(
        &app,
        vcx,
        "landing in the middle",
        &[&middle],
        Expect::Splice,
    );
    assert!(following_tail(&app, vcx), "after landing in the middle");
    check(
        &app,
        vcx,
        "deleting in the middle",
        &[&delete(&[30])],
        Expect::Splice,
    );
    assert!(following_tail(&app, vcx), "after deleting in the middle");
    let last_date = BASE_DATE + (MESSAGES / 10) * 86400 + MESSAGES * 60;
    let end = text_message(id(MESSAGES + 1), Some(22), last_date, "");
    check(&app, vcx, "landing at the end", &[&end], Expect::Splice);
    assert!(following_tail(&app, vcx), "after landing at the end");
    assert_eq!(top_message(&app, vcx), None, "no scroll anchor at the tail");
}

fn live_past_the_end(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    from: i64,
    count: i64,
) -> i64 {
    let updates: Vec<String> = (from..from + count)
        .map(|n| {
            let date = BASE_DATE + (MESSAGES / 10) * 86400 + n * 60;
            text_message(id(n), Some(21), date, "")
        })
        .collect();
    let refs: Vec<&str> = updates.iter().map(String::as_str).collect();
    apply(app, vcx, &refs);
    from + count
}

fn room_below_the_cap(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> i64 {
    let loaded = app.read_with(vcx, |app, _| {
        app.demo_session
            .as_ref()
            .map_or(0, |session| session.histories[&GROUP].messages.len())
    });
    (quill::state::HISTORY_WINDOW_CAP - loaded) as i64
}

fn row_count(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (usize, usize) {
    app.read_with(vcx, |app, cx| {
        (
            app.history.rows.len(),
            app.history.scroller.read(cx).item_count(),
        )
    })
}

#[gpui_kit::test]
fn a_live_trim_keeps_following_the_tail(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    app.update(vcx, |app, cx| {
        app.history
            .scroller
            .update(cx, |state, cx| state.scroll_to_end(cx));
    });
    frames(vcx);
    let room = room_below_the_cap(&app, vcx);
    let next = live_past_the_end(&app, vcx, MESSAGES, room);
    assert!(following_tail(&app, vcx), "at the tail before the trim");
    let (before, _) = row_count(&app, vcx);
    live_past_the_end(&app, vcx, next, 2);
    assert!(following_tail(&app, vcx), "at the tail after the trim");
    let (rows, listed) = row_count(&app, vcx);
    assert!(rows < before, "{rows} rows, {before} before");
    assert_eq!(listed, rows, "scroller in step");
    assert_eq!(top_message(&app, vcx), None, "no scroll anchor at the tail");
}

#[gpui_kit::test]
fn a_live_trim_leaves_the_reader_where_they_were(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    let room = room_below_the_cap(&app, vcx);
    let next = live_past_the_end(&app, vcx, MESSAGES, room);
    app.update(vcx, |app, cx| {
        app.history
            .scroller
            .update(cx, |state, cx| state.scroll_to_item_offset(1_000, 12., cx));
    });
    frames(vcx);
    let left = top_message(&app, vcx).expect("away from the tail");
    let (before, _) = row_count(&app, vcx);
    live_past_the_end(&app, vcx, next, 2);
    let (rows, listed) = row_count(&app, vcx);
    assert!(rows < before, "{rows} rows, {before} before");
    assert_eq!(listed, rows, "scroller in step");
    assert_eq!(top_message(&app, vcx), Some(left), "the reader stays put");
    assert!(!following_tail(&app, vcx), "still away from the tail");
}

#[gpui_kit::test]
fn more_changes_than_a_patch_holds_build_the_rows_whole(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    // Over `PATCH_MAX` (32) but inside the removal log (64), so only the
    // cap declines the splice; the log running out is covered by
    // `removals_too_far_back_are_not_listed`.
    let deletes: Vec<String> = (1..=40).map(|n| delete(&[n])).collect();
    let updates: Vec<&str> = deletes.iter().map(String::as_str).collect();
    check(
        &app,
        vcx,
        "40 removals in one render",
        &updates,
        Expect::Build,
    );
}

/// The open chat's first messages as a `TopicHistory`, for the topic and
/// saved paths, which take the same splice route with their own source.
fn topic_copy(session: &quill::state::Session) -> quill::state::TopicHistory {
    let mut history = quill::state::TopicHistory::default();
    for message in main_messages(session).into_iter().take(60) {
        history.messages.insert(message.id.0, message);
    }
    history
}

/// A message like `like`, at `id`, half a minute after it.
fn sibling(like: &quill::state::HistoryMessage, id: i64) -> quill::state::HistoryMessage {
    let mut message = like.clone();
    message.id = MessageId(id);
    message.date += 30;
    message
}

fn land_next_to_20(messages: Option<&mut quill::state::HistoryMessages>) {
    if let Some(messages) = messages
        && let Some(like) = messages.get(&id(20)).cloned()
    {
        messages.insert(id(20) + 1, sibling(&like, id(20) + 1));
    }
}

fn sublist_messages(
    session: &mut quill::state::Session,
) -> Option<&mut quill::state::HistoryMessages> {
    let sublist = session.threads.saved.sublist.as_mut()?;
    Some(&mut sublist.history.messages)
}

const TOPIC: i32 = 7;

fn topic_messages(
    session: &mut quill::state::Session,
) -> Option<&mut quill::state::HistoryMessages> {
    let history = session.threads.topic_histories.get_mut(&(GROUP, TOPIC))?;
    Some(&mut history.messages)
}

#[gpui_kit::test]
fn the_saved_sublist_splices_like_the_open_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    let open_sublist = |session: &mut quill::state::Session, topic_id: i64| {
        let history = topic_copy(session);
        session.threads.saved.sublist = Some(quill::state::SavedSublistView { topic_id, history });
    };
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.my_user_id = Some(GROUP);
            open_sublist(session, 5);
        }
        cx.notify();
    });
    frames(vcx);
    let rows = app.read_with(vcx, |app, _| app.history.rows.len());
    assert!(rows > 20, "the sublist's rows are shown: {rows}");
    check_after(
        &app,
        vcx,
        "a message lands in the sublist",
        |session| land_next_to_20(sublist_messages(session)),
        Expect::Splice,
    );
    check_after(
        &app,
        vcx,
        "a message leaves the sublist",
        |session| {
            if let Some(messages) = sublist_messages(session) {
                messages.remove(&id(30));
            }
        },
        Expect::Splice,
    );
    // The sublist replaced by another one: built whole.
    check_after(
        &app,
        vcx,
        "another sublist",
        |session| open_sublist(session, 6),
        Expect::Build,
    );
}

#[gpui_kit::test]
fn a_topic_history_splices_like_the_open_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    mixed_group(&app, vcx);
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&GROUP) {
                chat.is_forum = Some(true);
            }
            session.open_topic = Some(TOPIC);
            let history = topic_copy(session);
            session
                .threads
                .topic_histories
                .insert((GROUP, TOPIC), history);
        }
        cx.notify();
    });
    frames(vcx);
    let rows = app.read_with(vcx, |app, _| app.history.rows.len());
    assert!(rows > 20, "the topic's rows are shown: {rows}");
    check_after(
        &app,
        vcx,
        "a message lands in the topic",
        |session| land_next_to_20(topic_messages(session)),
        Expect::Splice,
    );
    check_after(
        &app,
        vcx,
        "a message leaves the topic",
        |session| {
            if let Some(messages) = topic_messages(session) {
                messages.remove(&id(30));
            }
        },
        Expect::Splice,
    );
    check_after(
        &app,
        vcx,
        "the topic's history replaced",
        |session| {
            let history = topic_copy(session);
            session
                .threads
                .topic_histories
                .insert((GROUP, TOPIC), history);
        },
        Expect::Build,
    );
}
