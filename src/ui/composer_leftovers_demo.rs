//! Screenshot fixtures for the composer leftovers: a group that restricts
//! the viewer, and a paused voice recording. Injected through the normal
//! reducer (no live Telegram).

use super::app::QuillApp;
use super::demo::demo_media_allowlist;
use super::screenshot_demo::{DemoSpec, register_demos};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{ChannelMemberStatus, ChatPermissions, MemberRestriction};
use quill::voice::VoiceCapture;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const GROUP: i64 = -1001700000001;

/// A supergroup whose admins restricted the viewer. `QUILL_DEMO_RESTRICTION`
/// picks the sentence: `until` (default, a restriction that ends in two
/// days), `forever`, `everyone` (the group itself forbids sending) or
/// `media` (text allowed, photos and voice refused).
pub(super) fn apply_ready_restricted_composer(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    variant: &str,
    now: i64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let jsons = [
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":1700000001,"usernames":null,"status":{"@type":"chatMemberStatusMember"},"member_count":128,"is_channel":false,"is_broadcast_group":false}}"#.to_string(),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{GROUP},"title":"Design Crew","type":{{"@type":"chatTypeSupergroup","supergroup_id":1700000001,"is_channel":false}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":1048576,"chat_id":{GROUP},"sender_id":{{"@type":"messageSenderUser","user_id":11}},"is_outgoing":false,"date":1790632300,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Moodboard is in the shared folder. Feedback by Friday please.","entities":[]}}}}}}}}"#
        ),
    ];
    session.open_chat(ChatId(GROUP));
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let Some(chat) = session.chats.get_mut(&GROUP) else {
        return;
    };
    let everything = ChatPermissions::all();
    let text_only = ChatPermissions {
        can_send_basic_messages: true,
        ..ChatPermissions::default()
    };
    match variant {
        "everyone" => {
            chat.permissions = Some(ChatPermissions::default());
            chat.can_send_basic_messages = false;
            chat.set_my_restriction(None);
            chat.my_member_status = Some(ChannelMemberStatus::Member);
        }
        "media" => {
            chat.permissions = Some(everything);
            chat.set_my_restriction(Some(MemberRestriction {
                until_date: 0,
                permissions: text_only,
            }));
            chat.my_member_status = Some(ChannelMemberStatus::Restricted);
        }
        other => {
            chat.permissions = Some(everything);
            let until_date = if other == "forever" {
                0
            } else {
                (now + 2 * 24 * 3600) as i32
            };
            chat.set_my_restriction(Some(MemberRestriction {
                until_date,
                permissions: ChatPermissions::default(),
            }));
            chat.my_member_status = Some(ChannelMemberStatus::Restricted);
        }
    }
}

register_demos![
    // Composer leftovers: a group that restricts the viewer replaces the
    // composer with the reason (`QUILL_DEMO_RESTRICTION`).
    DemoSpec::chats(
        "ready-restricted-composer",
        "screenshot demo — restricted composer"
    )
    .setup(|app, _, _| app.demo_restricted_composer()),
    // Composer leftovers: a paused voice recording with its preview and
    // the Play once switch.
    DemoSpec::chats(
        "ready-voice-pause",
        "screenshot demo — voice record bar + history playback"
    )
    .setup(|app, _, _| app.demo_voice_pause()),
];

impl QuillApp {
    fn demo_restricted_composer(&mut self) {
        let variant = std::env::var("QUILL_DEMO_RESTRICTION").unwrap_or_default();
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_restricted_composer(
                session,
                &self.demo_sink,
                &self.demo_seq,
                &variant,
                quill::local_time::now_unix(),
            );
        }
        self.status_note = if variant == "media" {
            self.send_denial(quill::send_rights::SendKind::VoiceMessages)
                .unwrap_or_default()
        } else {
            "screenshot demo — restricted composer".into()
        };
    }

    fn demo_voice_pause(&mut self) {
        // A paused recording: the play button leads the bar, the
        // preview has played to 3 s of 7, and Play once is on.
        let bars = vec![
            4, 16, 28, 12, 8, 20, 6, 18, 10, 24, 8, 14, 22, 9, 17, 5, 26, 11,
        ];
        let mut capture =
            VoiceCapture::preview(demo_media_allowlist().join("demo-voice.ogg"), 7, bars);
        capture.pause();
        self.recording.voice_capture = Some(capture);
        let mut clock = quill::playback::PlaybackClock::new(7.0);
        clock.seek(3.0);
        self.recording.preview = Some(clock);
        self.recording.once = true;
        self.status_note = "screenshot demo — recording paused · previewing · play once".into();
    }
}
