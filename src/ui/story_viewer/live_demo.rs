//! Screenshot demos of a live story in the story viewer (injected, no live
//! Telegram, no call, no Stars): the demo channel's live story playing a
//! still picture with the LIVE badge, three top donors, four comments (one
//! paid and pinned) and a paid comment price; the same with your Stars
//! waiting under the Undo toast and their effect rising; and the Stars box
//! for a comment.
#![deny(clippy::indexing_slicing)]

use super::live_state::StarsBoxMode;
use crate::ui::app::QuillApp;
use crate::ui::demo::demo_media_allowlist;
use crate::ui::groups::apply_ready_channels;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::calls::engine::{GroupCallMode, StreamState, VideoFrame};
use quill::diagnostics::DiagnosticSink;
use quill::ids::ChatId;
use quill::live_story::PendingLiveReaction;
use quill::paid_reaction::PaidReactor;
use quill::state::{ActiveGroupCall, Session, unix_ms_now};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{LiveStoryDonors, MessageSender, ParsedGroupCallMessage};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

const CHANNEL: i64 = 13;
const STORY: i32 = 61;
const CALL: i32 = 4411;
const VIEWERS: [(i64, &str, &str); 4] = [
    (801, "Maya", "Cohen"),
    (802, "Daniel", "Weiss"),
    (803, "Noa", "Levi"),
    (804, "Ari", "Ben"),
];

register_demos![
    DemoSpec::chats(
        "ready-live-story",
        "screenshot demo — live story with comments, donors and a paid price"
    )
    .setup(QuillApp::demo_live_story),
    DemoSpec::chats(
        "ready-live-story-stars",
        "screenshot demo — live story with Stars waiting to be sent"
    )
    .setup(QuillApp::demo_live_story_stars),
    DemoSpec::chats(
        "ready-live-story-box",
        "screenshot demo — live story Stars box for a comment"
    )
    .setup(QuillApp::demo_live_story_box),
];

impl QuillApp {
    fn demo_live_story(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.seed_demo_live_story(cx);
    }

    fn demo_live_story_stars(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.seed_demo_live_story(cx);
        if let Some(live) = self
            .demo_session
            .as_mut()
            .and_then(|s| s.stories.live.as_mut())
        {
            live.pending_reaction = Some(PendingLiveReaction::add(None, 5, unix_ms_now()));
        }
        let me = MessageSender::User { user_id: 801 };
        self.stories.live.fly(me, 5, Instant::now());
        cx.notify();
    }

    fn demo_live_story_box(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.seed_demo_live_story(cx);
        self.stories.live.chosen_stars = 50;
        self.open_live_stars_box(StarsBoxMode::Comment, 50, false, cx);
    }

    fn seed_demo_live_story(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        apply_ready_channels(session, &self.demo_ui.sink, &self.demo_ui.seq);
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        seed_live_story(session, &sink, &self.demo_ui.seq);
        self.stories.live.demo_frame = demo_stream_frame().map(Arc::new);
        let items = quill::story_viewer::collect_found_story_items(
            &[(CHANNEL, STORY)],
            &session.stories.stories,
        );
        self.stories.live.chosen_stars = 0;
        self.begin_story_viewer(items, 0, cx);
        // The viewer joins by itself; the demo has no driver, so the
        // watched story is marked here.
        self.stories.live.watching = Some((ChatId(CHANNEL), STORY));
        cx.notify();
    }
}

/// The demo channel's live story, its call and what is said in it.
fn seed_live_story(session: &mut Session, sink: &Arc<dyn DiagnosticSink>, seq: &AtomicU64) {
    let mut ingest = |json: String| {
        if let Some(owned) = copy_and_parse(&json, seq, sink) {
            session.apply(owned);
        }
    };
    for (id, first, last) in VIEWERS {
        ingest(format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ));
    }
    ingest(format!(
        r#"{{"@type":"story","id":{STORY},"poster_chat_id":{CHANNEL},"date":1700000000,"content":{{"@type":"storyContentLive","group_call_id":{CALL},"is_rtmp_stream":false}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#
    ));
    let now = i32::try_from(unix_ms_now() / 1000).unwrap_or(i32::MAX);
    let comment =
        |id: i32, user_id: i64, text: &str, stars: i64, owner: bool| ParsedGroupCallMessage {
            message_id: id,
            sender_id: if owner {
                MessageSender::Chat { chat_id: CHANNEL }
            } else {
                MessageSender::User { user_id }
            },
            date: now - 40 + id,
            text: text.to_string(),
            paid_message_star_count: stars,
            is_from_owner: owner,
            can_be_deleted: true,
        };
    session.calls.active_group_call = Some(ActiveGroupCall {
        is_live_story: true,
        is_joined: true,
        is_video_chat: true,
        can_send_messages: true,
        are_messages_allowed: true,
        paid_message_star_count: 5,
        stream_mode: GroupCallMode::Stream,
        stream: StreamState::Live,
        viewer_count: 2_318,
        messages: vec![
            comment(1, 802, "The light over the harbour is unreal", 0, false),
            comment(2, 803, "Hello from Haifa!", 0, false),
            comment(3, 0, "Thanks for watching, questions in a minute", 0, true),
            comment(4, 801, "Can you show the boats again?", 250, false),
        ],
        ..ActiveGroupCall::fresh(CALL)
    });
    session.begin_live_story(CHANNEL, STORY, CALL);
    let reactor = |user_id: i64, star_count: i64| PaidReactor {
        sender: Some(MessageSender::User { user_id }),
        star_count,
        is_top: true,
        is_me: false,
        is_anonymous: false,
    };
    let messages = session
        .calls
        .active_group_call
        .as_ref()
        .map(|call| call.messages.clone())
        .unwrap_or_default();
    let me = session.live_story_sender();
    if let Some(live) = session.stories.live.as_mut() {
        live.donors = Some(LiveStoryDonors {
            total_star_count: 1_240,
            top_donors: vec![reactor(801, 500), reactor(804, 420), reactor(802, 180)],
        });
        live.comments.on_messages(&messages, me);
        live.extras_requested = true;
    }
}

/// A still picture for the stream: a showcase photo as a frame.
fn demo_stream_frame() -> Option<VideoFrame> {
    let path = demo_media_allowlist().join("showcase/p2.jpg");
    let rgba = image::open(path).ok()?.to_rgba8();
    Some(VideoFrame {
        seq: 1,
        width: u16::try_from(rgba.width()).ok()?,
        height: u16::try_from(rgba.height()).ok()?,
        rgba: rgba.into_raw(),
        is_local: false,
        participant_user_id: None,
        is_screen: false,
    })
}
