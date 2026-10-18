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
    let stats = |app: &Entity<QuillApp>, cx: &mut VisualTestContext| {
        app.read_with(cx, |app, _| app.history.row_stats)
    };
    let before = stats(app, cx);
    apply(app, cx, updates);
    let after = stats(app, cx);
    assert_eq!(after.rebuilds, before.rebuilds, "{name}: rows built whole");
    match expect {
        Expect::Patch => assert!(after.rows_patched > before.rows_patched, "{name}"),
        Expect::Splice => assert_eq!(after.splices, before.splices + 1, "{name}"),
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
        Expect::Splice,
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
