//! Screenshot demos of a live story in the story viewer (injected, no live
//! Telegram, no call, no Stars): the demo channel's live story playing a
//! picture sent at 30 fps with the LIVE badge, three top donors, four comments (one
//! paid and pinned) and a paid comment price; the same with your Stars
//! waiting under the Undo toast and three Stars effects rising on a stopped
//! clock over a burst fixture; the Stars box for
//! a comment; a portrait stream that fills the frame (the landscape one is
//! shown whole over its blurred copy); the live story playing beside a
//! 1:1 call; and one that gave way to the call, showing Failed with Retry.
#![deny(clippy::indexing_slicing)]

use super::live_effects::star_burst;
use super::live_state::{EFFECT_DURATION, FlyingStars, StarsBoxMode};
use crate::ui::app::QuillApp;
use crate::ui::calls::demo_feed::{DemoFeed, demo_feed};
use crate::ui::demo::{demo_file_json, demo_media_allowlist};
use crate::ui::groups::apply_ready_channels;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::calls::engine::{GroupCallMode, StreamState};
use quill::diagnostics::DiagnosticSink;
use quill::ids::{ChatId, FileId};
use quill::live_story::PendingLiveReaction;
use quill::paid_reaction::PaidReactor;
use quill::state::{ActiveGroupCall, LiveBursts, Session, unix_ms_now};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{
    LiveStoryDonors, MessageSender, ParsedGroupCallMessage, StickerFormat, StickerItem,
};
use quill::text::{TextEntity, TextEntityKind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const CHANNEL: i64 = 13;
const STORY: i32 = 61;
const CALL: i32 = 4411;
const BURST_FILE: i32 = 4412;
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
    DemoSpec::chats(
        "ready-live-story-portrait",
        "screenshot demo — live story with a portrait stream filling the frame"
    )
    .setup(QuillApp::demo_live_story_portrait),
    DemoSpec::chats(
        "ready-live-story-in-call",
        "screenshot demo — live story playing beside a 1:1 call"
    )
    .setup(QuillApp::demo_live_story_in_call),
    DemoSpec::chats(
        "ready-live-story-key-taken",
        "screenshot demo — live story that gave way to a 1:1 call"
    )
    .setup(QuillApp::demo_live_story_key_taken),
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
        if let Some(session) = self.demo_session.as_mut() {
            let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
            seed_star_burst(session, &sink, &self.demo_ui.seq);
        }
        // The bursts open and tint on the player's workers; the effects
        // start once that settled, so they play them.
        let _ = self.star_bursts_settled(cx);
        let placed = std::rc::Rc::new(std::cell::Cell::new(false));
        cx.observe_self(move |this, cx| {
            if placed.get() || !this.star_bursts_settled(cx) {
                return;
            }
            placed.set(true);
            this.demo_star_effects(cx);
            cx.notify();
        })
        .detach();
        cx.notify();
    }

    /// Whether every burst's tinted clip has its first frame or failed
    /// (opening the ones not open yet).
    fn star_bursts_settled(&self, cx: &mut Context<Self>) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        let burst = star_burst(session);
        let asks: Vec<_> = burst
            .candidates
            .iter()
            .map(|sticker| burst.ask(sticker, Duration::ZERO, 0))
            .collect();
        // Every one is asked for (opened), not just up to the first unsettled.
        let settled: Vec<bool> = asks
            .into_iter()
            .map(|ask| self.sticker_view(ask, cx).is_some() || self.sticker_failed(ask))
            .collect();
        settled.iter().all(|settled| *settled)
    }

    /// Three effects caught mid-flight on a stopped clock, all started
    /// after the burst was ready.
    fn demo_star_effects(&mut self, cx: &mut Context<Self>) {
        let ready = self
            .session()
            .and_then(|session| {
                let burst = star_burst(session);
                let sticker = burst
                    .candidates
                    .iter()
                    .find(|sticker| sticker.file_id == FileId(BURST_FILE))?;
                Some(burst.ask(sticker, Duration::ZERO, 0))
            })
            .and_then(|ask| self.sticker_view(ask, cx))
            .and_then(|view| view.ready_at())
            .unwrap_or_else(Instant::now);
        let now = ready + EFFECT_DURATION;
        self.stories.live.demo_clock = Some(now);
        for (user_id, stars, progress, shift) in [
            (801, 5, 0.1, 0.5),
            (802, 50, 0.3, 0.1),
            (804, 250, 0.7, 0.9),
        ] {
            let age = EFFECT_DURATION.mul_f32(progress);
            self.stories.live.push_flying(FlyingStars {
                sender: MessageSender::User { user_id },
                stars,
                started: now.checked_sub(age).unwrap_or(now),
                shift,
                seed: 0,
            });
        }
    }

    fn demo_live_story_box(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.seed_demo_live_story(cx);
        self.stories.live.chosen_stars = 50;
        self.open_live_stars_box(StarsBoxMode::Comment, 50, false, cx);
    }

    fn demo_live_story_portrait(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.seed_demo_live_story(cx);
        self.stories.live.demo_feed = demo_stream_feed(true, cx);
        cx.notify();
    }

    fn demo_live_story_in_call(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
            crate::ui::calls::call_video_demo::seed_video_call(
                session,
                false,
                &sink,
                &self.demo_ui.seq,
            );
        }
        self.seed_demo_live_story(cx);
    }

    fn demo_live_story_key_taken(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_live_story_in_call(window, cx);
        if let Some(call) = self
            .demo_session
            .as_mut()
            .and_then(|s| s.calls.story_stream.as_mut())
        {
            call.transport_error = Some("another call needs this stream's connection".into());
        }
        cx.notify();
    }

    fn seed_demo_live_story(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        apply_ready_channels(session, &self.demo_ui.sink, &self.demo_ui.seq);
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        seed_live_story(session, &sink, &self.demo_ui.seq);
        self.stories.live.demo_feed = demo_stream_feed(false, cx);
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
    // "Hello" in italics, "Haifa" in bold.
    let formatted = vec![
        TextEntity {
            utf8_start: 0,
            utf8_end: 5,
            kind: TextEntityKind::Italic,
        },
        TextEntity {
            utf8_start: 11,
            utf8_end: 16,
            kind: TextEntityKind::Bold,
        },
    ];
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
            entities: Vec::new(),
            paid_message_star_count: stars,
            is_from_owner: owner,
            can_be_deleted: true,
        };
    session.calls.story_stream = Some(ActiveGroupCall {
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
            ParsedGroupCallMessage {
                entities: formatted,
                ..comment(2, 803, "Hello from Haifa!", 0, false)
            },
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
        .story_stream
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

/// A burst in place of the server's, made by
/// `scripts/make-star-burst-fixture.py`.
fn seed_star_burst(session: &mut Session, sink: &Arc<dyn DiagnosticSink>, seq: &AtomicU64) {
    let path = demo_media_allowlist().join("demo-star-burst.tgs");
    if let Some(owned) = copy_and_parse(
        &demo_file_json(BURST_FILE, &path.to_string_lossy(), true),
        seq,
        sink,
    ) {
        session.apply(owned);
    }
    session.stories.live_bursts = LiveBursts::Loaded(vec![StickerItem {
        custom_emoji_id: None,
        id: i64::from(BURST_FILE),
        set_id: 0,
        emoji: "\u{2728}".into(),
        width: 512,
        height: 512,
        format: StickerFormat::Tgs,
        file_id: FileId(BURST_FILE),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
        premium_animation: None,
    }]);
}

/// A still picture for the stream: a showcase photo as a frame, or its
/// middle at 9:16 for a portrait stream.
fn demo_stream_feed(portrait: bool, cx: &mut App) -> Option<DemoFeed> {
    let path = demo_media_allowlist().join("showcase/p2.jpg");
    let mut rgba = image::open(path).ok()?.to_rgba8();
    if portrait {
        let height = rgba.height();
        let width = (height * 9 / 16).min(rgba.width());
        let left = (rgba.width() - width) / 2;
        rgba = image::imageops::crop_imm(&rgba, left, 0, width, height).to_image();
    }
    let width = u16::try_from(rgba.width()).ok()?;
    let height = u16::try_from(rgba.height()).ok()?;
    demo_feed(rgba.as_raw(), width, height, true, cx)
}
