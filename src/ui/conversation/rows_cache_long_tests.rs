//! One update against a long open history (a live session's revision
//! rules): what it costs in rows built, rows patched, chromes derived and
//! messages copied. `--nocapture` prints the `MEASURE` lines.
use super::*;

/// Messages in the open history, on top of the three `open_group` adds.
const LONG: i64 = 1000;

fn long_group(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    open_group(app, cx);
    app.update(cx, |app, _| app.history.follow_revision_in_tests = true);
    let updates: Vec<String> = (0..LONG)
        .map(|n| {
            let id = 1000 + n;
            let sender = 21 + n % 2;
            let date = 1790632000 + n * 60;
            format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":31,"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"is_outgoing":false,"date":{date},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Message {n}","entities":[]}}}}}}}}"#
            )
        })
        .collect();
    let refs: Vec<&str> = updates.iter().map(String::as_str).collect();
    apply(app, cx, &refs);
    let last = 1000 + LONG - 1;
    apply(
        app,
        cx,
        &[&format!(
            r#"{{"@type":"updateChatReadInbox","chat_id":31,"last_read_inbox_message_id":{last},"unread_count":0}}"#
        )],
    );
    frames(cx);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Work {
    pub(super) chrome_built: u64,
    pub(super) rows_patched: u64,
    pub(super) rebuilds: u64,
    pub(super) messages_cloned: u64,
}

fn work(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Work {
    app.read_with(cx, |app, _| Work {
        chrome_built: crate::ui::conversation::row_chrome::chrome_built(),
        rows_patched: app.history.row_stats.rows_patched,
        rebuilds: app.history.row_stats.rebuilds,
        messages_cloned: app.history.row_stats.messages_cloned,
    })
}

fn work_for(app: &Entity<QuillApp>, cx: &mut VisualTestContext, update: &str) -> Work {
    let before = work(app, cx);
    apply(app, cx, &[update]);
    let after = work(app, cx);
    Work {
        chrome_built: after.chrome_built - before.chrome_built,
        rows_patched: after.rows_patched - before.rows_patched,
        rebuilds: after.rebuilds - before.rebuilds,
        messages_cloned: after.messages_cloned - before.messages_cloned,
    }
}

const OTHER_CHAT_MESSAGE: &str = r#"{"@type":"updateNewMessage","message":{"id":77,"chat_id":12,"sender_id":{"@type":"messageSenderUser","user_id":12},"is_outgoing":false,"date":1790700000,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Elsewhere.","entities":[]}}}}"#;
const FILE_PROGRESS: &str = r#"{"@type":"updateFile","file":{"@type":"file","id":4242,"size":1000000,"expected_size":1000000,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":true,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":1000,"downloaded_size":1000},"remote":{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":1000000}}}"#;
const EDIT: &str = r#"{"@type":"updateMessageContent","chat_id":31,"message_id":1500,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"Edited.","entities":[]}}}"#;
const REACTION: &str = r#"{"@type":"updateMessageInteractionInfo","chat_id":31,"message_id":1990,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":1,"is_chosen":false,"recent_sender_ids":[]}],"are_tags":false}}}"#;

#[gpui_kit::test]
fn measure_work_per_update_on_a_long_history(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    long_group(&app, vcx);
    let rows = app.read_with(vcx, |app, _| app.history.rows.len());
    println!("MEASURE rows={rows}");
    for (name, update) in [
        ("other-chat message", OTHER_CHAT_MESSAGE),
        ("off-screen file progress", FILE_PROGRESS),
        ("edit one message", EDIT),
        ("reaction on one message", REACTION),
    ] {
        let work = work_for(&app, vcx, update);
        println!("MEASURE {name}: {work:?}");
    }
}

/// Rows whose chrome an update may derive again: those on screen and the
/// margin around them, far fewer than the history holds.
const NEAR_VIEWPORT: u64 = 64;

#[gpui_kit::test]
fn an_unrelated_update_rebuilds_no_rows(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    long_group(&app, vcx);
    let elsewhere = work_for(&app, vcx, OTHER_CHAT_MESSAGE);
    assert_eq!(
        (
            elsewhere.rebuilds,
            elsewhere.rows_patched,
            elsewhere.messages_cloned
        ),
        (0, 0, 0)
    );
    assert!(
        elsewhere.chrome_built <= NEAR_VIEWPORT,
        "{elsewhere:?} for a history of {LONG}"
    );
    let progress = work_for(&app, vcx, FILE_PROGRESS);
    assert_eq!(
        progress,
        Work {
            chrome_built: 0,
            rows_patched: 0,
            rebuilds: 0,
            messages_cloned: 0
        },
        "download progress of a file no row shows"
    );
}

#[gpui_kit::test]
fn editing_one_message_rebuilds_one_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    long_group(&app, vcx);
    for update in [EDIT, REACTION] {
        let work = work_for(&app, vcx, update);
        assert_eq!(
            (work.rebuilds, work.rows_patched, work.messages_cloned),
            (0, 1, 1),
            "{update}"
        );
        assert!(work.chrome_built <= NEAR_VIEWPORT + 1, "{work:?}");
    }
    let edited = app.read_with(vcx, |app, _| {
        single(app, 1500).map(|row| quill::state::effective_preview(&row.message))
    });
    assert_eq!(edited.as_deref(), Some("Edited."));
}

#[gpui_kit::test]
fn a_row_scrolled_into_view_shows_a_rename_made_while_away(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    long_group(&app, vcx);
    assert_eq!(sender_name(&app, vcx, 201).as_deref(), Some("Ann"));
    let rename = work_for(
        &app,
        vcx,
        r#"{"@type":"updateUser","user":{"id":21,"first_name":"Annie","last_name":"","type":{"@type":"userTypeRegular"}}}"#,
    );
    assert!(rename.chrome_built <= NEAR_VIEWPORT, "{rename:?}");
    assert_eq!(rename.rebuilds, 0);
    assert_eq!(
        sender_name(&app, vcx, 201).as_deref(),
        Some("Ann"),
        "far from the viewport, the first row waits until it is drawn"
    );
    // Scrolling alone redraws the list, not the app.
    app.update(vcx, |app, cx| {
        app.history
            .scroller
            .update(cx, |state, cx| state.scroll_to_item(0, cx));
    });
    for _ in 0..3 {
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        vcx.run_until_parked();
    }
    assert_eq!(sender_name(&app, vcx, 201).as_deref(), Some("Annie"));
}

/// One `updateFile` for a finished download, `id` its file id.
fn finished_file(id: i32) -> String {
    format!(
        r#"{{"@type":"updateFile","file":{{"@type":"file","id":{id},"size":10,"expected_size":10,"local":{{"@type":"localFile","path":"/tmp/q/{id}.jpg","can_be_downloaded":true,"can_be_deleted":true,"is_downloading_active":false,"is_downloading_completed":true,"download_offset":0,"downloaded_prefix_size":10,"downloaded_size":10}},"remote":{{"@type":"remoteFile","id":"r{id}","unique_id":"u{id}","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}}}}}"#
    )
}

#[gpui_kit::test]
fn measure_files_kept_after_many_distinct_files(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    let updates: Vec<String> = (100_000..120_000).map(finished_file).collect();
    let refs: Vec<&str> = updates.iter().map(String::as_str).collect();
    apply(&app, vcx, &refs);
    let kept = app.read_with(vcx, |app, _| {
        app.demo_session.as_ref().map_or(0, |s| s.media.files.len())
    });
    println!("MEASURE files kept after 20000 distinct updateFile: {kept}");
    assert!(kept <= quill::state::FILES_MAX, "{kept}");
}
