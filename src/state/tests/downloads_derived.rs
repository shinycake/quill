//! Download status is derived from TDLib's file state plus the recorded
//! intent: file updates drive what rows show, with no extra bookkeeping.
use super::common::*;
use super::*;

fn update_file(id: i32, active: bool, completed: bool, downloaded: i64) -> String {
    let path = if completed { "/tmp/derived.bin" } else { "" };
    format!(
        r#"{{"@type":"updateFile","file":{{"@type":"file","id":{id},"size":100,"expected_size":100,"local":{{"@type":"localFile","path":"{path}","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":{active},"is_downloading_completed":{completed},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":{downloaded}}},"remote":{{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":100}}}}}}"#
    )
}

fn status(session: &Session, id: i32) -> DownloadStatus {
    session.media.downloads.status(id, &session.media.files)
}

#[test]
fn file_updates_alone_drive_the_shown_status() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    assert_eq!(status(&session, 60), DownloadStatus::Idle);
    // A transfer nobody here asked for (another device, TDLib resuming).
    apply_json(&mut session, &seq, &sink, &update_file(60, true, false, 10));
    assert_eq!(status(&session, 60), DownloadStatus::Downloading);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &update_file(60, false, true, 100),
    );
    assert_eq!(status(&session, 60), DownloadStatus::Done);
    // TDLib drops the file from the cache.
    apply_json(&mut session, &seq, &sink, &update_file(60, false, false, 0));
    assert_eq!(status(&session, 60), DownloadStatus::Idle);
}

#[test]
fn a_stale_request_never_shows_over_a_finished_file() {
    let (mut session, _sink) = session();
    session.begin_user_download(FileId(61));
    assert_eq!(status(&session, 61), DownloadStatus::Downloading);
    // The completed snapshot lands without passing the update path.
    session.media.files.insert(ParsedFile {
        id: FileId(61),
        size: 100,
        expected_size: 100,
        uploading: false,
        local: LocalFileState {
            path: "/tmp/derived.bin".into(),
            can_be_downloaded: true,
            is_downloading_active: false,
            is_downloading_completed: true,
            downloaded_size: 100,
        },
    });
    assert_eq!(status(&session, 61), DownloadStatus::Done);
}

#[test]
fn cancel_hides_progress_at_once_and_is_never_a_failure() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_download(FileId(62));
    session.begin_user_download(FileId(62));
    apply_json(&mut session, &seq, &sink, &update_file(62, true, false, 20));
    assert_eq!(status(&session, 62), DownloadStatus::Downloading);
    session.abort_download(FileId(62));
    // TDLib has not confirmed yet: the file still reports active.
    assert_eq!(status(&session, 62), DownloadStatus::Idle);
    assert!(!session.media.downloads.is_user(62));
    // A late progress update does not bring the row back.
    apply_json(&mut session, &seq, &sink, &update_file(62, true, false, 30));
    assert_eq!(status(&session, 62), DownloadStatus::Idle);
    // The cancelled request answers with an error, then the idle update.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"canceled"}}"#,
            extra.0
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &update_file(62, false, false, 30),
    );
    assert_eq!(status(&session, 62), DownloadStatus::Idle);
    assert!(!session.media.downloads.is_failed(62));
    assert!(!session.media.downloads.is_stalled_auto(62));
    // Asking again works.
    session.begin_user_download(FileId(62));
    assert_eq!(status(&session, 62), DownloadStatus::Downloading);
}

#[test]
fn a_failed_row_follows_tdlib_when_the_transfer_resumes() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.begin_user_download(FileId(63));
    apply_json(&mut session, &seq, &sink, &update_file(63, true, false, 10));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &update_file(63, false, false, 10),
    );
    assert_eq!(status(&session, 63), DownloadStatus::Failed);
    assert_eq!(
        session
            .media
            .downloads
            .failed_ids(&session.media.files)
            .collect::<Vec<_>>(),
        vec![63]
    );
    // TDLib resumes the transfer (another device, restart).
    apply_json(&mut session, &seq, &sink, &update_file(63, true, false, 40));
    assert_eq!(status(&session, 63), DownloadStatus::Downloading);
    assert_eq!(
        session
            .media
            .downloads
            .failed_ids(&session.media.files)
            .count(),
        0
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &update_file(63, false, true, 100),
    );
    assert_eq!(status(&session, 63), DownloadStatus::Done);
    assert!(!session.media.downloads.is_failed(63));
}

#[test]
fn recent_list_drops_a_file_removed_from_the_cache() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.begin_user_download(FileId(64));
    apply_json(&mut session, &seq, &sink, &update_file(64, true, false, 10));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &update_file(64, false, true, 100),
    );
    assert_eq!(
        session
            .media
            .downloads
            .recent(&session.media.files)
            .collect::<Vec<_>>(),
        vec![64]
    );
    apply_json(&mut session, &seq, &sink, &update_file(64, false, false, 0));
    assert_eq!(
        session.media.downloads.recent(&session.media.files).count(),
        0
    );
    assert_eq!(status(&session, 64), DownloadStatus::Idle);
}

#[test]
fn closing_the_session_forgets_every_download() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.begin_user_download(FileId(65));
    session.media.downloads.set_paused(65, true);
    session.begin_download(FileId(66));
    session.media.downloads.end_as(67, DownloadOrigin::User);
    session.media.downloads.record_finished(68);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosed"}}"#,
    );
    assert_eq!(session.media.downloads.user_count(), 0);
    assert_eq!(session.media.downloads.requested_count(), 0);
    assert!(!session.media.downloads.is_failed(67));
    assert_eq!(
        session.media.downloads.recent(&session.media.files).count(),
        0
    );
}

#[test]
fn an_auto_download_failure_never_shows_failed() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_download(FileId(77));
    session.begin_download(FileId(77));
    apply_json(&mut session, &seq, &sink, &update_file(77, true, false, 5));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"network"}}"#,
            extra.0
        ),
    );
    apply_json(&mut session, &seq, &sink, &update_file(77, false, false, 5));
    assert_ne!(status(&session, 77), DownloadStatus::Failed);
    assert!(!session.media.downloads.is_failed(77));
    assert!(session.media.downloads.is_stalled_auto(77));
    assert_eq!(
        session
            .media
            .downloads
            .failed_ids(&session.media.files)
            .count(),
        0
    );
    assert_eq!(session.media.downloads.user_count(), 0);
}

#[test]
fn a_transfer_tdlib_already_runs_is_not_requested_again() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    assert!(session.should_download(FileId(78)));
    // Started on another device: active, with no record here.
    apply_json(&mut session, &seq, &sink, &update_file(78, true, false, 10));
    assert!(!session.media.downloads.is_requested(78));
    assert!(!session.should_download(FileId(78)));
    // It stalled: the automatic path may ask again.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &update_file(78, false, false, 10),
    );
    assert!(session.should_download(FileId(78)));
}

#[test]
fn counts_follow_every_mutation() {
    let mut d = DownloadIntents::default();
    let files = FileMap::default();
    let counts = |d: &DownloadIntents| {
        (
            d.requested_count(),
            d.user_count(),
            d.failed_ids(&files).count(),
        )
    };
    d.begin(1, DownloadOrigin::User);
    d.begin(2, DownloadOrigin::Auto);
    d.begin(3, DownloadOrigin::User);
    assert_eq!(counts(&d), (3, 2, 0));
    // Begin again over the same id does not double count.
    d.begin(1, DownloadOrigin::User);
    assert_eq!(counts(&d), (3, 2, 0));
    d.set_paused(1, true);
    assert_eq!(counts(&d), (3, 2, 0));
    d.end(1);
    assert_eq!(counts(&d), (2, 1, 1));
    d.end(2);
    assert_eq!(counts(&d), (1, 1, 1));
    d.cancel(3);
    assert_eq!(counts(&d), (0, 0, 1));
    d.confirm_cancel(3);
    d.begin(1, DownloadOrigin::Auto);
    assert_eq!(counts(&d), (1, 0, 0));
    d.begin(1, DownloadOrigin::User);
    d.settle(1);
    assert_eq!(counts(&d), (0, 0, 0));
    d.adopt(4, false);
    d.end_as(5, DownloadOrigin::User);
    assert_eq!(counts(&d), (1, 1, 1));
    d.forget(5);
    assert_eq!(counts(&d), (1, 1, 0));
    d.clear_stalled(2);
    d.clear();
    assert_eq!(counts(&d), (0, 0, 0));
    assert_eq!(d.user_ids().count(), 0);
}
