//! Screenshot-demo fixtures for the account-level sync updates
//! (`--screenshot-demo ready-updates-sync`, mode from `QUILL_DEMO_SYNC`:
//! `frozen` (default), `live`, `speech`, `age`, `downloads`). Everything is
//! injected through the real reducers; no live Telegram. English only.

use super::app::QuillApp;
use super::audio_playback::apply_ready_voice;
use super::screenshot_demo::ScreenshotDemo;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The mode the demo shows.
pub(super) fn demo_sync_mode() -> String {
    std::env::var("QUILL_DEMO_SYNC").unwrap_or_else(|_| "frozen".into())
}

fn apply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, json: &str) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

fn live_message(id: i64, chat_id: i64, expires_in: i32) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":{chat_id},"is_outgoing":true,"date":1790000000,"content":{{"@type":"messageLiveLocation","location":{{"@type":"liveLocation","location":{{"@type":"location","latitude":48.8566,"longitude":2.3522,"horizontal_accuracy":0}},"live_period":3600,"heading":0,"proximity_alert_radius":0}},"expires_in":{expires_in}}}}}"#
    )
}

fn apply_mode(mode: &str, session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let now = quill::local_time::now_unix();
    match mode {
        "frozen" => apply(
            session,
            sink,
            seq,
            &format!(
                r#"{{"@type":"updateFreezeState","is_frozen":true,"freezing_date":{},"deletion_date":{},"appeal_link":"https://t.me/spambot"}}"#,
                now - 3 * 86_400,
                now + 11 * 86_400
            ),
        ),
        "live" => {
            let update = format!(
                r#"{{"@type":"updateActiveLiveLocationMessages","messages":[{},{}]}}"#,
                live_message(301, 11, 2_700),
                live_message(302, 12, 600)
            );
            apply(session, sink, seq, &update);
            apply(
                session,
                sink,
                seq,
                r#"{"@type":"updateMessageLiveLocationViewed","chat_id":11,"message_id":301}"#,
            );
        }
        "speech" => {
            apply_ready_voice(session, sink, seq);
            session.open_chat(ChatId(11));
            apply(
                session,
                sink,
                seq,
                &format!(
                    r#"{{"@type":"updateSpeechRecognitionTrial","max_media_duration":300,"weekly_count":2,"left_count":1,"next_reset_date":{}}}"#,
                    now + 5 * 86_400
                ),
            );
        }
        "age" => apply(
            session,
            sink,
            seq,
            r#"{"@type":"updateAgeVerificationParameters","parameters":{"@type":"ageVerificationParameters","min_age":18,"verification_bot_username":"AgeVerifyBot","country":"GB"}}"#,
        ),
        "downloads" => {
            // A download another device started, then the list totals.
            apply(
                session,
                sink,
                seq,
                r#"{"@type":"updateFileAddedToDownloads","file_download":{"@type":"fileDownload","file_id":28,"message":{"@type":"message","id":207,"chat_id":11,"is_outgoing":false,"date":1790000000,"content":{"@type":"messageDocument","document":{"@type":"document","file_name":"trip-photos.zip","mime_type":"application/zip","document":{"@type":"file","id":28,"size":52428800,"expected_size":52428800,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":true,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":52428800}}},"caption":{"@type":"formattedText","text":"","entities":[]}}},"add_date":1790000000,"complete_date":0,"is_paused":false},"counts":{"@type":"downloadedFileCounts","active_count":3,"paused_count":1,"completed_count":1}}"#,
            );
            apply(
                session,
                sink,
                seq,
                r#"{"@type":"updateFileDownloads","total_size":154534176,"total_count":5,"downloaded_size":31457280}"#,
            );
            session.downloads_panel_open = true;
        }
        _ => {}
    }
}

impl QuillApp {
    pub(super) fn demo_setup_updates_sync(
        &mut self,
        demo: Option<ScreenshotDemo>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if demo != Some(ScreenshotDemo::ReadyUpdatesSync) {
            return;
        }
        let mode = demo_sync_mode();
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_mode(&mode, session, &self.demo_sink, &self.demo_seq);
        }
        match mode.as_str() {
            "frozen" => self.freeze_info_open = true,
            "age" => self.age_verify_open = true,
            _ => {}
        }
    }
}
