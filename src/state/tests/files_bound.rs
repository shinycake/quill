//! `media.files` stays bounded over a long session and never drops a file
//! in use.
use super::common::*;
use super::*;

fn update_file(id: i32, path: &str, downloading: bool, uploading: bool) -> String {
    let completed = !path.is_empty();
    format!(
        r#"{{"@type":"updateFile","file":{{"@type":"file","id":{id},"size":100,"expected_size":100,"local":{{"@type":"localFile","path":"{path}","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":{downloading},"is_downloading_completed":{completed},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":{uploading},"is_uploading_completed":{completed},"uploaded_size":0}}}}}}"#
    )
}

#[test]
fn many_files_stay_bounded_and_files_in_use_stay() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let downloading = 1;
    let asked_for = 2;
    let sending = 3;
    let shown = 4;
    let forgotten = 5;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &update_file(asked_for, "", false, false),
    );
    session.begin_user_download(FileId(asked_for));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &update_file(downloading, "", true, false),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &update_file(sending, "/tmp/s", false, true),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &update_file(shown, "/tmp/4", false, false),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &update_file(forgotten, "/tmp/5", false, false),
    );
    let total = 3 * FILES_MAX as i32;
    let mut largest = 0;
    for n in 0..total {
        apply_json(
            &mut session,
            &seq,
            &sink,
            &update_file(1000 + n, "/tmp/f", false, false),
        );
        // A row showing file 4 reads it on every render.
        if n % 1000 == 0 {
            assert!(session.media.files.get(&shown).is_some(), "file 4 at {n}");
        }
        largest = largest.max(session.media.files.len());
    }
    assert!(largest <= FILES_MAX, "{largest} files kept");
    for (id, what) in [
        (downloading, "downloading"),
        (asked_for, "asked for"),
        (sending, "being sent"),
        (shown, "shown"),
    ] {
        assert!(
            session.media.files.contains_key(&id),
            "the {what} file stays"
        );
    }
    assert!(
        !session.media.files.contains_key(&forgotten),
        "an unused file goes"
    );
}

#[test]
fn files_the_downloads_panel_lists_stay() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let finished = 1;
    let failed = 2;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &update_file(finished, "", false, false),
    );
    session.begin_user_download(FileId(finished));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &update_file(finished, "/tmp/1", false, false),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &update_file(failed, "", false, false),
    );
    session.begin_user_download(FileId(failed));
    session.media.downloads.end(failed);
    assert!(session.media.downloads.is_listed(finished));
    assert!(session.media.downloads.is_listed(failed));
    for n in 0..3 * FILES_MAX as i32 {
        apply_json(
            &mut session,
            &seq,
            &sink,
            &update_file(1000 + n, "/tmp/f", false, false),
        );
    }
    assert!(
        session.media.files.contains_key(&finished),
        "finished stays"
    );
    assert!(session.media.files.contains_key(&failed), "failed stays");
}

#[test]
fn download_progress_leaves_the_header_revision_alone() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, &update_file(7, "", true, false));
    let before = session.revision_without_file_progress();
    apply_json(&mut session, &seq, &sink, &update_file(7, "", true, false));
    assert_eq!(session.revision_without_file_progress(), before);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &update_file(7, "/tmp/7", false, false),
    );
    assert_ne!(
        session.revision_without_file_progress(),
        before,
        "a finished download can change an avatar"
    );
}
